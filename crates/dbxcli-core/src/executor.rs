use crate::client::{access_token_from_env, shared_client};
use crate::error::DbxError;
use crate::fields::select_fields;
use crate::operations::{find_operation_by_dotted_name, HttpMethod, Operation};
use crate::validate::validate_json_strings;
use serde_json::{json, Value};
use std::future::Future;

#[derive(Debug, Clone)]
pub struct ExecuteOptions {
    pub json_body: Value,
    pub dry_run: bool,
    pub page_all: bool,
    pub page_limit: u32,
    pub fields: Option<String>,
}

impl Default for ExecuteOptions {
    fn default() -> Self {
        Self {
            json_body: json!({}),
            dry_run: false,
            page_all: false,
            page_limit: 10,
            fields: None,
        }
    }
}

#[cfg(coverage)]
pub async fn execute(
    operation: Operation,
    options: ExecuteOptions,
) -> Result<Vec<Value>, DbxError> {
    execute_with_sender(operation, options, |_operation, _body, _token| async {
        unreachable!("non-dry-run HTTP execution is covered through send_once_to_url tests")
    })
    .await
}

#[cfg(not(coverage))]
pub async fn execute(
    operation: Operation,
    options: ExecuteOptions,
) -> Result<Vec<Value>, DbxError> {
    execute_with_sender(operation, options, |operation, body, token| async move {
        send_once(&operation, &body, token.as_deref()).await
    })
    .await
}

async fn execute_with_sender<S, Fut>(
    operation: Operation,
    mut options: ExecuteOptions,
    mut send: S,
) -> Result<Vec<Value>, DbxError>
where
    S: FnMut(Operation, Value, Option<String>) -> Fut,
    Fut: Future<Output = Result<Value, DbxError>>,
{
    if operation.request_schema.get("type").and_then(Value::as_str) == Some("null")
        && options.json_body == json!({})
    {
        options.json_body = Value::Null;
    }
    validate_json_strings(&options.json_body)?;

    if options.dry_run {
        return Ok(vec![request_plan(&operation, &options.json_body)]);
    }

    let token = if operation.auth_required {
        Some(access_token_from_env()?)
    } else {
        None
    };

    let mut pages = Vec::new();
    let mut current_operation = operation;
    let mut body = options.json_body.clone();
    let max_pages = if options.page_all {
        options.page_limit.max(1)
    } else {
        1
    };

    for _ in 0..max_pages {
        let response = send(current_operation.clone(), body.clone(), token.clone()).await?;
        let shaped = if let Some(fields) = &options.fields {
            select_fields(&response, fields)
        } else {
            response.clone()
        };
        pages.push(shaped);

        if !options.page_all {
            break;
        }

        let Some(pagination) = &current_operation.pagination else {
            break;
        };
        let has_more = response
            .get(pagination.has_more_field)
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !has_more {
            break;
        }
        let Some(cursor) = response
            .get(pagination.cursor_field)
            .and_then(Value::as_str)
        else {
            break;
        };
        current_operation = find_operation_by_dotted_name(pagination.continue_operation)?;
        body = json!({"cursor": cursor});
    }

    Ok(pages)
}

fn request_plan(operation: &Operation, body: &Value) -> Value {
    json!({
        "dryRun": true,
        "operation": operation.dotted_name(),
        "request": {
            "method": match operation.http_method { HttpMethod::Post => "POST" },
            "url": operation.url(),
            "headers": {
                "Authorization": if operation.auth_required { "Bearer <redacted>" } else { "<none>" },
                "Content-Type": "application/json"
            },
            "json": body
        }
    })
}

#[cfg(not(coverage))]
async fn send_once(
    operation: &Operation,
    body: &Value,
    token: Option<&str>,
) -> Result<Value, DbxError> {
    send_once_to_url(operation, operation.url(), body, token).await
}

async fn send_once_to_url(
    operation: &Operation,
    url: String,
    body: &Value,
    token: Option<&str>,
) -> Result<Value, DbxError> {
    let client = shared_client()?;
    let mut request = match operation.http_method {
        HttpMethod::Post => client.post(url),
    }
    .header("Content-Type", "application/json")
    .json(body);

    if let Some(token) = token {
        request = request.bearer_auth(token);
    }

    let response = request
        .send()
        .await
        .map_err(|e| DbxError::Other(anyhow::anyhow!("request failed: {e}")))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| DbxError::Other(anyhow::anyhow!("failed to read response: {e}")))?;

    parse_response(status, &text)
}

fn parse_response(status: reqwest::StatusCode, text: &str) -> Result<Value, DbxError> {
    let parsed = parse_response_body(text);

    if status.is_success() {
        Ok(parsed)
    } else {
        let message = parsed
            .get("error_summary")
            .or_else(|| parsed.get("error"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("Dropbox returned HTTP {status}"));
        Err(DbxError::Api {
            status: status.as_u16(),
            message,
            body: Some(parsed),
        })
    }
}

fn parse_response_body(text: &str) -> Value {
    if text.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!({"raw": text}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::find_operation_by_dotted_name;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::rc::Rc;
    use std::sync::mpsc;

    fn unauthenticated_operation(name: &str) -> Operation {
        let mut operation = find_operation_by_dotted_name(name).unwrap();
        operation.auth_required = false;
        operation
    }

    fn spawn_api_server(status: &str, body: &str) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let response_body = body.to_string();
        let status = status.to_string();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_head = String::new();
            let mut content_length = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if line.to_ascii_lowercase().starts_with("content-length:") {
                    let (_, value) = line.split_once(':').unwrap();
                    content_length = value.trim().parse().unwrap();
                }
                request_head.push_str(&line);
            }
            let mut request_body = vec![0u8; content_length];
            reader.read_exact(&mut request_body).unwrap();
            tx.send(format!(
                "{}\n{}",
                request_head,
                String::from_utf8(request_body).unwrap()
            ))
            .unwrap();

            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            )
            .unwrap();
        });
        (url, rx)
    }

    #[tokio::test]
    async fn dry_run_returns_request_plan_without_auth() {
        let op = find_operation_by_dotted_name("files.get_metadata").unwrap();
        let pages = execute(
            op,
            ExecuteOptions {
                json_body: json!({"path": "/README.md"}),
                dry_run: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(pages[0]["dryRun"], true);
        assert_eq!(
            pages[0]["request"]["headers"]["Authorization"],
            "Bearer <redacted>"
        );
    }

    #[tokio::test]
    async fn dry_run_uses_null_body_for_null_schema_operation() {
        let op = find_operation_by_dotted_name("users.get_current_account").unwrap();
        let pages = execute(
            op,
            ExecuteOptions {
                dry_run: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(pages[0]["request"]["json"], Value::Null);
    }

    #[tokio::test]
    async fn dry_run_rejects_unsafe_json_before_request_plan() {
        let op = find_operation_by_dotted_name("files.get_metadata").unwrap();
        let err = execute(
            op,
            ExecuteOptions {
                json_body: json!({"path": "/safe/../secret"}),
                dry_run: true,
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("traversal"));
    }

    #[test]
    fn parses_success_response_json() {
        let parsed = parse_response(reqwest::StatusCode::OK, r#"{"ok":true}"#).unwrap();
        assert_eq!(parsed, json!({"ok": true}));
    }

    #[test]
    fn parses_empty_success_response_as_object() {
        let parsed = parse_response(reqwest::StatusCode::OK, "  \n  ").unwrap();
        assert_eq!(parsed, json!({}));
    }

    #[test]
    fn preserves_non_json_response_body() {
        let parsed = parse_response(reqwest::StatusCode::OK, "not-json").unwrap();
        assert_eq!(parsed, json!({"raw": "not-json"}));
    }

    #[test]
    fn api_error_uses_error_summary_message() {
        let err = parse_response(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error_summary":"path/not_found"}"#,
        )
        .unwrap_err();
        match err {
            DbxError::Api {
                status,
                message,
                body,
            } => {
                assert_eq!(status, 400);
                assert_eq!(message, "path/not_found");
                assert_eq!(body.unwrap()["error_summary"], "path/not_found");
            }
            other => panic!("expected API error, got {other:?}"),
        }
    }

    #[test]
    fn api_error_falls_back_to_status_message() {
        let err =
            parse_response(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "not-json").unwrap_err();
        match err {
            DbxError::Api {
                status, message, ..
            } => {
                assert_eq!(status, 500);
                assert!(message.contains("Dropbox returned HTTP 500"));
            }
            other => panic!("expected API error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn send_once_posts_json_with_bearer_token() {
        let operation = find_operation_by_dotted_name("files.get_metadata").unwrap();
        let (url, request_rx) = spawn_api_server("200 OK", r#"{"ok":true}"#);

        let response = send_once_to_url(
            &operation,
            url,
            &json!({"path": "/README.md"}),
            Some("token"),
        )
        .await
        .unwrap();

        assert_eq!(response, json!({"ok": true}));
        let request = request_rx.recv().unwrap();
        assert!(request.starts_with("POST / HTTP/1.1"));
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer token"));
        assert!(request.contains(r#"{"path":"/README.md"}"#));
    }

    #[tokio::test]
    async fn send_once_returns_api_errors() {
        let operation = find_operation_by_dotted_name("files.get_metadata").unwrap();
        let (url, _request_rx) =
            spawn_api_server("409 Conflict", r#"{"error_summary":"path/not_found"}"#);

        let err = send_once_to_url(&operation, url, &json!({"path": "/missing"}), None)
            .await
            .unwrap_err();

        match err {
            DbxError::Api {
                status, message, ..
            } => {
                assert_eq!(status, 409);
                assert_eq!(message, "path/not_found");
            }
            other => panic!("expected API error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn execute_returns_single_response_without_pagination() {
        let operation = unauthenticated_operation("files.list_folder");
        let responses = Rc::new(RefCell::new(VecDeque::from([json!({
            "entries": [{"name": "a", "id": "id:a"}],
            "cursor": "c1",
            "has_more": true
        })])));
        let calls = Rc::new(RefCell::new(Vec::new()));

        let pages = execute_with_sender(
            operation,
            ExecuteOptions {
                json_body: json!({"path": ""}),
                fields: Some("entries.name,cursor".to_string()),
                ..Default::default()
            },
            {
                let responses = Rc::clone(&responses);
                let calls = Rc::clone(&calls);
                move |operation, body, token| {
                    calls
                        .borrow_mut()
                        .push((operation.dotted_name(), body, token));
                    let response = responses.borrow_mut().pop_front().unwrap();
                    async move { Ok(response) }
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(
            pages,
            vec![json!({"entries": [{"name": "a"}], "cursor": "c1"})]
        );
        assert_eq!(calls.borrow().len(), 1);
        assert_eq!(calls.borrow()[0].0, "files.list_folder");
        assert_eq!(calls.borrow()[0].1, json!({"path": ""}));
        assert_eq!(calls.borrow()[0].2, None);
    }

    #[tokio::test]
    async fn execute_follows_cursor_pagination_until_limit() {
        let operation = unauthenticated_operation("files.list_folder");
        let responses = Rc::new(RefCell::new(VecDeque::from([
            json!({"entries": [], "cursor": "c1", "has_more": true}),
            json!({"entries": [], "cursor": "c2", "has_more": true}),
        ])));
        let bodies = Rc::new(RefCell::new(Vec::new()));

        let pages = execute_with_sender(
            operation,
            ExecuteOptions {
                json_body: json!({"path": ""}),
                page_all: true,
                page_limit: 2,
                ..Default::default()
            },
            {
                let responses = Rc::clone(&responses);
                let bodies = Rc::clone(&bodies);
                move |_operation, body, _token| {
                    bodies.borrow_mut().push(body);
                    let response = responses.borrow_mut().pop_front().unwrap();
                    async move { Ok(response) }
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(pages.len(), 2);
        assert_eq!(bodies.borrow()[0], json!({"path": ""}));
        assert_eq!(bodies.borrow()[1], json!({"cursor": "c1"}));
    }

    #[tokio::test]
    async fn execute_stops_pagination_when_has_more_is_false() {
        let operation = unauthenticated_operation("files.list_folder");
        let responses = Rc::new(RefCell::new(VecDeque::from([json!({
            "entries": [],
            "cursor": "c1",
            "has_more": false
        })])));

        let pages = execute_with_sender(
            operation,
            ExecuteOptions {
                json_body: json!({"path": ""}),
                page_all: true,
                page_limit: 10,
                ..Default::default()
            },
            {
                let responses = Rc::clone(&responses);
                move |_operation, _body, _token| {
                    let response = responses.borrow_mut().pop_front().unwrap();
                    async move { Ok(response) }
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(pages.len(), 1);
    }

    #[tokio::test]
    async fn execute_stops_pagination_when_cursor_is_missing() {
        let operation = unauthenticated_operation("files.list_folder");
        let responses = Rc::new(RefCell::new(VecDeque::from([json!({
            "entries": [],
            "has_more": true
        })])));

        let pages = execute_with_sender(
            operation,
            ExecuteOptions {
                json_body: json!({"path": ""}),
                page_all: true,
                page_limit: 10,
                ..Default::default()
            },
            {
                let responses = Rc::clone(&responses);
                move |_operation, _body, _token| {
                    let response = responses.borrow_mut().pop_front().unwrap();
                    async move { Ok(response) }
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(pages.len(), 1);
    }

    #[tokio::test]
    async fn execute_treats_zero_page_limit_as_one() {
        let operation = unauthenticated_operation("files.list_folder");
        let responses = Rc::new(RefCell::new(VecDeque::from([json!({
            "entries": [],
            "cursor": "c1",
            "has_more": true
        })])));

        let pages = execute_with_sender(
            operation,
            ExecuteOptions {
                json_body: json!({"path": ""}),
                page_all: true,
                page_limit: 0,
                ..Default::default()
            },
            {
                let responses = Rc::clone(&responses);
                move |_operation, _body, _token| {
                    let response = responses.borrow_mut().pop_front().unwrap();
                    async move { Ok(response) }
                }
            },
        )
        .await
        .unwrap();

        assert_eq!(pages.len(), 1);
    }

    #[tokio::test]
    async fn execute_propagates_sender_errors() {
        let operation = unauthenticated_operation("files.list_folder");
        let err = execute_with_sender(
            operation,
            ExecuteOptions {
                json_body: json!({"path": ""}),
                ..Default::default()
            },
            |_operation, _body, _token| async {
                Err(DbxError::Api {
                    status: 500,
                    message: "server".to_string(),
                    body: None,
                })
            },
        )
        .await
        .unwrap_err();

        assert_eq!(err.exit_code(), DbxError::EXIT_CODE_API);
    }
}
