#![cfg_attr(coverage, allow(dead_code, unused_imports))]

use clap::{Arg, ArgAction, Command};
#[cfg(not(coverage))]
use dbx_cli_core::auth::exchange_authorization_code;
use dbx_cli_core::auth::{
    auth_status, build_login_plan, build_login_session, credentials_from_token_response,
    current_unix_seconds, default_credentials_path, logout_credentials,
    parse_callback_request_line, store_credentials, verify_callback_state, CallbackQuery,
};
use dbx_cli_core::executor::{execute, ExecuteOptions};
use dbx_cli_core::operations::{find_operation, operation_tree};
use dbx_cli_core::schema::{operation_schema, registry_schema};
use dbx_cli_core::validate::{sanitize_for_terminal, validate_safe_file_path};
use dbx_cli_core::DbxError;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        print_error(&err);
        std::process::exit(err.exit_code());
    }
}

async fn run() -> Result<(), DbxError> {
    let matches = build_cli().get_matches();

    match matches.subcommand() {
        Some(("schema", sub)) => {
            let output = if let Some(path) = sub.get_one::<String>("path") {
                operation_schema(path)?
            } else {
                registry_schema()
            };
            print_json_pretty(&output)?;
            Ok(())
        }
        Some(("operations", _)) => {
            print_json_pretty(&registry_schema())?;
            Ok(())
        }
        Some(("auth", auth_matches)) => match auth_matches.subcommand() {
            Some(("login", login_matches)) => run_auth_login(login_matches).await,
            Some(("status", _)) => run_auth_status(),
            Some(("logout", logout_matches)) => run_auth_logout(logout_matches),
            _ => Err(DbxError::Validation("missing auth command".to_string())),
        },
        Some((resource, resource_matches)) => {
            let Some((method, method_matches)) = resource_matches.subcommand() else {
                return Err(DbxError::Validation(format!(
                    "missing method for resource '{resource}'"
                )));
            };
            let operation = find_operation(resource, method)?;
            let body = read_body(method_matches)?;
            let options = ExecuteOptions {
                json_body: body,
                dry_run: method_matches.get_flag("dry-run"),
                page_all: method_matches.get_flag("page-all"),
                page_limit: *method_matches.get_one::<u32>("page-limit").unwrap_or(&10),
                fields: method_matches.get_one::<String>("fields").cloned(),
            };
            let force_ndjson = method_matches
                .get_one::<String>("format")
                .is_some_and(|format| format == "ndjson");
            let pages = execute(operation, options).await?;
            print_pages(&pages, force_ndjson || method_matches.get_flag("page-all"))?;
            Ok(())
        }
        _ => Err(DbxError::Validation("no command provided".to_string())),
    }
}

fn build_cli() -> Command {
    let mut root = Command::new("dbx")
        .about("Agent-first Dropbox CLI")
        .long_about("Agent-first Dropbox CLI. Raw JSON payloads, schema introspection, dry-run, structured errors, auth planning, and NDJSON pagination are first-class.")
        .version(env!("CARGO_PKG_VERSION"))
        .arg_required_else_help(true)
        .subcommand_required(true)
        .subcommand(
            Command::new("schema")
                .about("Print machine-readable operation schema")
                .arg(Arg::new("path").help("Operation path, e.g. files.list_folder")),
        )
        .subcommand(Command::new("operations").about("Print operation registry as JSON"))
        .subcommand(auth_command());

    for (resource, operations) in operation_tree() {
        let mut resource_cmd = Command::new(resource)
            .about(format!("Dropbox {resource} operations"))
            .subcommand_required(true)
            .arg_required_else_help(true);

        for operation in operations {
            resource_cmd = resource_cmd.subcommand(operation_command(&operation));
        }

        root = root.subcommand(resource_cmd);
    }

    root
}

fn auth_command() -> Command {
    Command::new("auth")
        .about("Dropbox auth commands")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("login")
                .about("Build Dropbox OAuth PKCE login plan")
                .arg(
                    Arg::new("client-id")
                        .long("client-id")
                        .value_name("ID")
                        .help("Override Dropbox app client id; falls back to DBX_CLI_CLIENT_ID and built-in shared key"),
                )
                .arg(
                    Arg::new("no-browser")
                        .long("no-browser")
                        .help("Skip browser launch and print authorization plan only")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("json")
                        .long("json")
                        .help("Emit JSON plan (default output)")
                        .action(ArgAction::SetTrue),
                ),
        )
        .subcommand(Command::new("status").about("Print structured authentication status"))
        .subcommand(
            Command::new("logout")
                .about("Remove stored Dropbox credentials")
                .arg(
                    Arg::new("dry-run")
                        .long("dry-run")
                        .help("Print logout plan without deleting credentials")
                        .action(ArgAction::SetTrue),
                ),
        )
}

fn operation_command(operation: &dbx_cli_core::operations::Operation) -> Command {
    Command::new(operation.method)
        .about(operation.description)
        .arg(
            Arg::new("json")
                .long("json")
                .value_name("JSON|@PATH|@-")
                .help("Raw Dropbox API request body as JSON, @file, or @- for stdin"),
        )
        .arg(
            Arg::new("json-file")
                .long("json-file")
                .value_name("PATH")
                .help("Read raw Dropbox API request body from file"),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .help("Validate and print request plan without calling Dropbox")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("page-all")
                .long("page-all")
                .help("Follow Dropbox cursor pagination and print one JSON object per line")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("page-limit")
                .long("page-limit")
                .value_name("N")
                .help("Maximum pages to fetch with --page-all")
                .value_parser(clap::value_parser!(u32)),
        )
        .arg(
            Arg::new("fields")
                .long("fields")
                .value_name("FIELD[,FIELD]")
                .help("Client-side field projection, e.g. entries.name,entries.id,cursor"),
        )
        .arg(
            Arg::new("format")
                .long("format")
                .value_name("FORMAT")
                .help("Output format: json or ndjson")
                .value_parser(["json", "ndjson"]),
        )
}

async fn run_auth_login(matches: &clap::ArgMatches) -> Result<(), DbxError> {
    let client_id_arg = matches.get_one::<String>("client-id").map(String::as_str);

    if matches.get_flag("no-browser") {
        let plan = build_login_plan(client_id_arg, true)?;
        let output = serde_json::to_value(plan)
            .map_err(|e| DbxError::Other(anyhow::anyhow!("failed to serialize auth plan: {e}")))?;
        return print_json_pretty(&output);
    }

    run_auth_login_in_browser(client_id_arg).await
}

fn run_auth_status() -> Result<(), DbxError> {
    let credentials_path = default_credentials_path()?;
    let status = auth_status(&credentials_path, current_unix_seconds()?)?;
    let output = serde_json::to_value(status)
        .map_err(|e| DbxError::Other(anyhow::anyhow!("failed to serialize auth status: {e}")))?;
    print_json_pretty(&output)
}

fn run_auth_logout(matches: &clap::ArgMatches) -> Result<(), DbxError> {
    let credentials_path = default_credentials_path()?;
    let result = logout_credentials(&credentials_path, matches.get_flag("dry-run"))?;
    let output = serde_json::to_value(result)
        .map_err(|e| DbxError::Other(anyhow::anyhow!("failed to serialize logout result: {e}")))?;
    print_json_pretty(&output)
}

#[cfg(coverage)]
async fn run_auth_login_in_browser(_client_id_arg: Option<&str>) -> Result<(), DbxError> {
    Err(DbxError::Auth(
        "browser OAuth login is not exercised during coverage runs".to_string(),
    ))
}

#[cfg(not(coverage))]
async fn run_auth_login_in_browser(client_id_arg: Option<&str>) -> Result<(), DbxError> {
    let session = build_login_session(client_id_arg, false)?;
    eprintln!("Opening Dropbox authorization URL in your browser...");
    eprintln!("If the browser does not open, visit this URL:");
    eprintln!("{}", session.plan.authorization_url);

    webbrowser::open(&session.plan.authorization_url)
        .map_err(|e| DbxError::Other(anyhow::anyhow!("failed to open browser: {e}")))?;

    let callback = wait_for_oauth_callback(&session.state)?;
    let token_response = exchange_authorization_code(
        &session.plan.client_id,
        &callback.code,
        &session.pkce_verifier,
        &session.plan.redirect_uri,
    )
    .await?;
    let credentials = credentials_from_token_response(
        session.plan.client_id.clone(),
        token_response,
        current_unix_seconds()?,
    );
    let credentials_path = default_credentials_path()?;
    store_credentials(&credentials_path, &credentials)?;

    let success = dbx_cli_core::auth::LoginSuccess {
        authenticated: true,
        account_id: credentials.account_id.clone(),
        uid: credentials.uid.clone(),
        scopes: credentials.scopes.clone(),
        has_refresh_token: credentials.refresh_token.is_some(),
        credentials_path: credentials_path.display().to_string(),
    };
    let output = serde_json::to_value(success)
        .map_err(|e| DbxError::Other(anyhow::anyhow!("failed to serialize login result: {e}")))?;
    print_json_pretty(&output)
}

fn wait_for_oauth_callback(expected_state: &str) -> Result<CallbackQuery, DbxError> {
    let listener = TcpListener::bind("127.0.0.1:53682").map_err(|e| {
        DbxError::Auth(format!(
            "failed to listen on 127.0.0.1:53682 for OAuth callback: {e}"
        ))
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|e| DbxError::Auth(format!("failed to configure OAuth callback listener: {e}")))?;

    let deadline = Instant::now() + Duration::from_secs(300);
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _addr)) => {
                let mut first_line = String::new();
                {
                    let mut reader = BufReader::new(&mut stream);
                    reader.read_line(&mut first_line).map_err(|e| {
                        DbxError::Auth(format!("failed to read OAuth callback: {e}"))
                    })?;
                }

                let callback = parse_callback_request_line(first_line.trim_end())?;
                let response = match verify_callback_state(&callback, expected_state) {
                    Ok(()) => {
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\ndbx-cli login complete. You can close this browser tab.\n"
                    }
                    Err(_) => {
                        "HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain; charset=utf-8\r\nConnection: close\r\n\r\ndbx-cli login failed: state mismatch. Return to your terminal.\n"
                    }
                };
                stream.write_all(response.as_bytes()).map_err(|e| {
                    DbxError::Auth(format!("failed to write OAuth callback response: {e}"))
                })?;
                verify_callback_state(&callback, expected_state)?;
                return Ok(callback);
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(err) => {
                return Err(DbxError::Auth(format!(
                    "failed while waiting for OAuth callback: {err}"
                )));
            }
        }
    }

    Err(DbxError::Auth(
        "timed out waiting for Dropbox OAuth callback".to_string(),
    ))
}

fn read_body(matches: &clap::ArgMatches) -> Result<Value, DbxError> {
    let json_arg = matches.get_one::<String>("json");
    let json_file = matches.get_one::<String>("json-file");

    match (json_arg, json_file) {
        (Some(_), Some(_)) => Err(DbxError::Validation(
            "use only one of --json or --json-file".to_string(),
        )),
        (Some(raw), None) => read_json_arg(raw),
        (None, Some(path)) => read_json_file(path),
        (None, None) => Ok(json!({})),
    }
}

fn read_json_arg(raw: &str) -> Result<Value, DbxError> {
    if raw == "@-" {
        let mut input = String::new();
        std::io::stdin()
            .read_to_string(&mut input)
            .map_err(|e| DbxError::Validation(format!("failed to read stdin: {e}")))?;
        parse_json(&input)
    } else if let Some(path) = raw.strip_prefix('@') {
        read_json_file(path)
    } else {
        parse_json(raw)
    }
}

fn read_json_file(path: &str) -> Result<Value, DbxError> {
    let safe_path = validate_safe_file_path(path, "--json-file")?;
    let text = std::fs::read_to_string(&safe_path).map_err(|e| {
        DbxError::Validation(format!(
            "failed to read JSON file '{}': {e}",
            safe_path.display()
        ))
    })?;
    parse_json(&text)
}

fn parse_json(text: &str) -> Result<Value, DbxError> {
    serde_json::from_str(text)
        .map_err(|e| DbxError::Validation(format!("invalid JSON payload: {e}")))
}

fn print_pages(pages: &[Value], ndjson: bool) -> Result<(), DbxError> {
    if ndjson {
        for page in pages {
            println!(
                "{}",
                serde_json::to_string(page).map_err(|e| DbxError::Other(e.into()))?
            );
        }
    } else if pages.len() == 1 {
        print_json_pretty(&pages[0])?;
    } else {
        print_json_pretty(&Value::Array(pages.to_vec()))?;
    }
    Ok(())
}

fn print_json_pretty(value: &Value) -> Result<(), DbxError> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|e| DbxError::Other(e.into()))?
    );
    Ok(())
}

fn print_error(err: &DbxError) {
    println!(
        "{}",
        serde_json::to_string_pretty(&err.to_json()).unwrap_or_default()
    );
    eprintln!("error: {}", sanitize_for_terminal(&err.to_string()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;
    use dbx_cli_core::operations::operations;
    use std::net::TcpStream;
    use std::sync::{Mutex, OnceLock};

    fn callback_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn assert_parses(args: &[&str]) {
        build_cli()
            .try_get_matches_from(args)
            .unwrap_or_else(|err| {
                panic!("expected CLI args to parse: {args:?}\n{err}");
            });
    }

    #[test]
    fn cli_exposes_top_level_commands() {
        let cmd = build_cli();
        for name in ["schema", "operations", "auth", "files", "users"] {
            assert!(
                cmd.find_subcommand(name).is_some(),
                "missing top-level command {name}"
            );
        }
    }

    #[test]
    fn cli_exposes_auth_commands() {
        let cmd = build_cli();
        let auth = cmd.find_subcommand("auth").expect("auth command exists");
        let login = auth
            .find_subcommand("login")
            .expect("auth login command exists");

        assert!(auth.find_subcommand("status").is_some());
        assert!(auth.find_subcommand("logout").is_some());
        for flag in ["client-id", "no-browser", "json"] {
            assert!(
                login.get_arguments().any(|arg| arg.get_id() == flag),
                "missing auth login flag --{flag}"
            );
        }
    }

    #[test]
    fn cli_exposes_every_registered_operation() {
        let cmd = build_cli();
        for operation in operations() {
            let resource = cmd
                .find_subcommand(operation.resource)
                .unwrap_or_else(|| panic!("missing resource command {}", operation.resource));
            assert!(
                resource.find_subcommand(operation.method).is_some(),
                "missing operation command {}.{}",
                operation.resource,
                operation.method
            );
        }
    }

    #[test]
    fn cli_parses_available_command_paths() {
        assert_parses(&["dbx", "schema"]);
        assert_parses(&["dbx", "schema", "files.list_folder"]);
        assert_parses(&["dbx", "operations"]);
        assert_parses(&["dbx", "auth", "login"]);
        assert_parses(&["dbx", "auth", "login", "--no-browser", "--json"]);
        assert_parses(&["dbx", "auth", "login", "--client-id", "app-key"]);
        assert_parses(&["dbx", "auth", "status"]);
        assert_parses(&["dbx", "auth", "logout", "--dry-run"]);
        assert_parses(&["dbx", "users", "get_current_account"]);
        assert_parses(&[
            "dbx",
            "files",
            "list_folder",
            "--json",
            r#"{"path":"","limit":10}"#,
        ]);
        assert_parses(&[
            "dbx",
            "files",
            "list_folder_continue",
            "--json",
            r#"{"cursor":"cursor"}"#,
        ]);
        assert_parses(&[
            "dbx",
            "files",
            "get_metadata",
            "--json",
            r#"{"path":"/README.md"}"#,
        ]);
        assert_parses(&[
            "dbx",
            "files",
            "delete_v2",
            "--json",
            r#"{"path":"/old.txt"}"#,
            "--dry-run",
        ]);
    }

    #[test]
    fn operation_commands_accept_shared_agent_flags() {
        for operation in operations() {
            assert_parses(&["dbx", operation.resource, operation.method, "--json", "{}"]);
            assert_parses(&[
                "dbx",
                operation.resource,
                operation.method,
                "--json-file",
                "payload.json",
            ]);
            assert_parses(&[
                "dbx",
                operation.resource,
                operation.method,
                "--dry-run",
                "--page-all",
                "--page-limit",
                "2",
                "--fields",
                "entries.name,cursor",
                "--format",
                "ndjson",
            ]);
        }
    }

    #[test]
    fn read_body_rejects_multiple_json_sources() {
        let matches = build_cli()
            .try_get_matches_from([
                "dbx",
                "files",
                "get_metadata",
                "--json",
                "{}",
                "--json-file",
                "payload.json",
            ])
            .unwrap();
        let (_, resource_matches) = matches.subcommand().unwrap();
        let (_, method_matches) = resource_matches.subcommand().unwrap();
        let err = read_body(method_matches).unwrap_err();
        assert!(err
            .to_string()
            .contains("use only one of --json or --json-file"));
    }

    #[test]
    fn version_flag_is_available() {
        let err = build_cli()
            .try_get_matches_from(["dbx", "--version"])
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::DisplayVersion);
    }

    #[test]
    fn parses_inline_json() {
        let value = read_json_arg(r#"{"path":""}"#).unwrap();
        assert_eq!(value["path"], "");
    }

    #[test]
    fn waits_for_oauth_callback_and_returns_code() {
        let _guard = callback_lock().lock().unwrap();
        let handle = std::thread::spawn(|| wait_for_oauth_callback("state-ok"));
        let mut stream = connect_to_callback_listener();
        stream
            .write_all(
                b"GET /oauth/callback?code=code-123&state=state-ok HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
            )
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        let callback = handle.join().unwrap().unwrap();

        assert_eq!(callback.code, "code-123");
        assert!(response.contains("200 OK"));
    }

    #[test]
    fn waits_for_oauth_callback_and_rejects_bad_state() {
        let _guard = callback_lock().lock().unwrap();
        let handle = std::thread::spawn(|| wait_for_oauth_callback("expected"));
        let mut stream = connect_to_callback_listener();
        stream
            .write_all(
                b"GET /oauth/callback?code=code-123&state=wrong HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
            )
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();

        let err = handle.join().unwrap().unwrap_err();

        assert!(err.to_string().contains("state"));
        assert!(response.contains("400 Bad Request"));
    }

    fn connect_to_callback_listener() -> TcpStream {
        for _ in 0..50 {
            match TcpStream::connect("127.0.0.1:53682") {
                Ok(stream) => return stream,
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        panic!("OAuth callback listener did not start");
    }
}
