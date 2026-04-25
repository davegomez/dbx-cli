use crate::error::DbxError;
use crate::validate::reject_dangerous_chars;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use getrandom::fill as random_fill;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const DEFAULT_SHARED_CLIENT_ID: &str = "o70nz9ebged3rpq";
pub const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:53682/oauth/callback";
pub const TOKEN_ACCESS_TYPE_OFFLINE: &str = "offline";
pub const CODE_CHALLENGE_METHOD_S256: &str = "S256";

const AUTHORIZATION_ENDPOINT: &str = "https://www.dropbox.com/oauth2/authorize";
const TOKEN_ENDPOINT: &str = "https://api.dropboxapi.com/oauth2/token";

const DEFAULT_SCOPES: &[&str] = &[
    "account_info.read",
    "files.metadata.read",
    "files.content.read",
    "files.content.write",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkcePair {
    pub verifier: String,
    pub challenge: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginSession {
    pub plan: LoginPlan,
    pub pkce_verifier: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LoginPlan {
    pub authorization_url: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
    pub token_access_type: &'static str,
    pub code_challenge_method: &'static str,
    pub no_browser: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    pub uid: Option<String>,
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StoredCredentials {
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub account_id: Option<String>,
    pub uid: Option<String>,
    pub scopes: Vec<String>,
    pub expires_at_unix_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LoginSuccess {
    pub authenticated: bool,
    pub account_id: Option<String>,
    pub uid: Option<String>,
    pub scopes: Vec<String>,
    pub has_refresh_token: bool,
    pub credentials_path: String,
}

pub fn resolve_client_id(flag_client_id: Option<&str>) -> Result<String, DbxError> {
    let env_client_id = std::env::var("DBX_CLI_CLIENT_ID")
        .or_else(|_| std::env::var("DBXCLI_CLIENT_ID"))
        .ok();
    resolve_client_id_from_sources(flag_client_id, env_client_id.as_deref())
}

pub fn resolve_client_id_from_sources(
    flag_client_id: Option<&str>,
    env_client_id: Option<&str>,
) -> Result<String, DbxError> {
    let candidate = flag_client_id
        .or(env_client_id)
        .unwrap_or(DEFAULT_SHARED_CLIENT_ID);
    validate_client_id(candidate).map(|value| value.to_string())
}

pub fn default_scopes() -> &'static [&'static str] {
    DEFAULT_SCOPES
}

pub fn generate_pkce_pair() -> Result<PkcePair, DbxError> {
    let mut verifier_bytes = [0u8; 32];
    random_fill(&mut verifier_bytes)
        .map_err(|e| DbxError::Other(anyhow::anyhow!("failed to generate PKCE verifier: {e}")))?;
    let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);
    let challenge = pkce_challenge_from_verifier(&verifier)?;
    Ok(PkcePair {
        verifier,
        challenge,
    })
}

pub fn pkce_challenge_from_verifier(verifier: &str) -> Result<String, DbxError> {
    reject_dangerous_chars(verifier, "PKCE verifier")?;
    if verifier.len() < 43 || verifier.len() > 128 {
        return Err(DbxError::Validation(
            "PKCE verifier must be 43-128 characters long".to_string(),
        ));
    }
    if !verifier
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~'))
    {
        return Err(DbxError::Validation(
            "PKCE verifier must use only RFC 7636 unreserved characters".to_string(),
        ));
    }

    let digest = Sha256::digest(verifier.as_bytes());
    Ok(URL_SAFE_NO_PAD.encode(digest))
}

pub fn generate_state() -> Result<String, DbxError> {
    let mut state_bytes = [0u8; 16];
    random_fill(&mut state_bytes)
        .map_err(|e| DbxError::Other(anyhow::anyhow!("failed to generate OAuth state: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(state_bytes))
}

pub fn build_login_plan(
    flag_client_id: Option<&str>,
    no_browser: bool,
) -> Result<LoginPlan, DbxError> {
    Ok(build_login_session(flag_client_id, no_browser)?.plan)
}

pub fn build_login_session(
    flag_client_id: Option<&str>,
    no_browser: bool,
) -> Result<LoginSession, DbxError> {
    let client_id = resolve_client_id(flag_client_id)?;
    let pkce = generate_pkce_pair()?;
    let state = generate_state()?;
    let plan =
        build_login_plan_from_parts(client_id, no_browser, pkce.challenge, Some(state.as_str()));
    Ok(LoginSession {
        plan,
        pkce_verifier: pkce.verifier,
        state,
    })
}

pub(crate) fn build_login_plan_from_parts(
    client_id: String,
    no_browser: bool,
    pkce_challenge: String,
    state: Option<&str>,
) -> LoginPlan {
    let redirect_uri = DEFAULT_REDIRECT_URI.to_string();
    let scopes = default_scopes()
        .iter()
        .map(|scope| (*scope).to_string())
        .collect::<Vec<_>>();
    let authorization_url =
        build_authorization_url(&client_id, &redirect_uri, &scopes, &pkce_challenge, state);

    LoginPlan {
        authorization_url,
        client_id,
        redirect_uri,
        scopes,
        token_access_type: TOKEN_ACCESS_TYPE_OFFLINE,
        code_challenge_method: CODE_CHALLENGE_METHOD_S256,
        no_browser,
    }
}

pub fn parse_callback_request_line(request_line: &str) -> Result<CallbackQuery, DbxError> {
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    if method != "GET" || target.is_empty() {
        return Err(DbxError::Validation(
            "OAuth callback must be an HTTP GET request".to_string(),
        ));
    }
    let (_, query) = target
        .split_once('?')
        .ok_or_else(|| DbxError::Validation("OAuth callback missing query string".to_string()))?;
    let code = query_value(query, "code")?
        .ok_or_else(|| DbxError::Validation("OAuth callback missing code".to_string()))?;
    let state = query_value(query, "state")?
        .ok_or_else(|| DbxError::Validation("OAuth callback missing state".to_string()))?;
    Ok(CallbackQuery { code, state })
}

pub fn verify_callback_state(
    callback: &CallbackQuery,
    expected_state: &str,
) -> Result<(), DbxError> {
    if callback.state != expected_state {
        return Err(DbxError::Validation(
            "OAuth callback state did not match login session".to_string(),
        ));
    }
    Ok(())
}

pub fn build_token_request_body(
    client_id: &str,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<String, DbxError> {
    validate_client_id(client_id)?;
    reject_dangerous_chars(code, "authorization code")?;
    pkce_challenge_from_verifier(code_verifier)?;
    reject_dangerous_chars(redirect_uri, "redirect URI")?;

    Ok(format!(
        "grant_type=authorization_code&code={}&client_id={}&code_verifier={}&redirect_uri={}",
        percent_encode(code),
        percent_encode(client_id),
        percent_encode(code_verifier),
        percent_encode(redirect_uri)
    ))
}

#[cfg(not(coverage))]
pub async fn exchange_authorization_code(
    client_id: &str,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<TokenResponse, DbxError> {
    exchange_authorization_code_at(TOKEN_ENDPOINT, client_id, code, code_verifier, redirect_uri)
        .await
}

async fn exchange_authorization_code_at(
    token_endpoint: &str,
    client_id: &str,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<TokenResponse, DbxError> {
    let body = build_token_request_body(client_id, code, code_verifier, redirect_uri)?;
    let response = reqwest::Client::new()
        .post(token_endpoint)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| DbxError::Other(anyhow::anyhow!("token exchange failed: {e}")))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| DbxError::Other(anyhow::anyhow!("failed to read token response: {e}")))?;

    if status.is_success() {
        serde_json::from_str(&text)
            .map_err(|e| DbxError::Other(anyhow::anyhow!("failed to parse token response: {e}")))
    } else {
        let parsed =
            serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({"raw": text}));
        Err(DbxError::Api {
            status: status.as_u16(),
            message: "Dropbox token exchange failed".to_string(),
            body: Some(parsed),
        })
    }
}

pub fn credentials_from_token_response(
    client_id: String,
    response: TokenResponse,
    now_unix_seconds: u64,
) -> StoredCredentials {
    let scopes = response
        .scope
        .as_deref()
        .unwrap_or_default()
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let expires_at_unix_seconds = response
        .expires_in
        .map(|expires_in| now_unix_seconds.saturating_add(expires_in));

    StoredCredentials {
        client_id,
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        account_id: response.account_id,
        uid: response.uid,
        scopes,
        expires_at_unix_seconds,
    }
}

pub fn default_credentials_path() -> Result<PathBuf, DbxError> {
    if let Ok(path) = std::env::var("DBX_CLI_CREDENTIALS_FILE") {
        return Ok(PathBuf::from(path));
    }
    if let Ok(path) = std::env::var("DBXCLI_CREDENTIALS_FILE") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME").ok_or_else(|| {
        DbxError::Validation("HOME is not set; cannot locate credentials".to_string())
    })?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("dbx-cli")
        .join("credentials.json"))
}

pub fn store_credentials(path: &Path, credentials: &StoredCredentials) -> Result<(), DbxError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            DbxError::Validation(format!("failed to create credentials directory: {e}"))
        })?;
    }
    let json = serde_json::to_string_pretty(credentials)
        .map_err(|e| DbxError::Other(anyhow::anyhow!("failed to serialize credentials: {e}")))?;
    write_secret_file(path, json.as_bytes())
}

pub fn load_credentials(path: &Path) -> Result<StoredCredentials, DbxError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| DbxError::Auth(format!("failed to read stored credentials: {e}")))?;
    serde_json::from_str(&text)
        .map_err(|e| DbxError::Auth(format!("failed to parse stored credentials: {e}")))
}

pub fn current_unix_seconds() -> Result<u64, DbxError> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| DbxError::Other(anyhow::anyhow!("system clock before Unix epoch: {e}")))?
        .as_secs())
}

fn build_authorization_url(
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
    code_challenge: &str,
    state: Option<&str>,
) -> String {
    let scope = scopes.join(" ");
    let mut url = format!(
        "{AUTHORIZATION_ENDPOINT}?client_id={}&response_type=code&code_challenge={}&code_challenge_method={}&token_access_type={}&redirect_uri={}&scope={}",
        percent_encode(client_id),
        percent_encode(code_challenge),
        percent_encode(CODE_CHALLENGE_METHOD_S256),
        percent_encode(TOKEN_ACCESS_TYPE_OFFLINE),
        percent_encode(redirect_uri),
        percent_encode(&scope),
    );
    if let Some(state) = state {
        url.push_str("&state=");
        url.push_str(&percent_encode(state));
    }
    url
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn query_value(query: &str, key: &str) -> Result<Option<String>, DbxError> {
    for pair in query.split('&') {
        let (raw_key, raw_value) = pair.split_once('=').unwrap_or((pair, ""));
        if percent_decode(raw_key)? == key {
            return Ok(Some(percent_decode(raw_value)?));
        }
    }
    Ok(None)
}

fn percent_decode(input: &str) -> Result<String, DbxError> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).map_err(|_| {
                    DbxError::Validation("invalid percent encoding in OAuth callback".to_string())
                })?;
                let byte = u8::from_str_radix(hex, 16).map_err(|_| {
                    DbxError::Validation("invalid percent encoding in OAuth callback".to_string())
                })?;
                out.push(byte);
                i += 3;
            }
            b'%' => {
                return Err(DbxError::Validation(
                    "truncated percent encoding in OAuth callback".to_string(),
                ));
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8(out)
        .map_err(|_| DbxError::Validation("OAuth callback query was not UTF-8".to_string()))
}

#[cfg(unix)]
fn write_secret_file(path: &Path, bytes: &[u8]) -> Result<(), DbxError> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| DbxError::Auth(format!("failed to open credentials file: {e}")))?;
    file.write_all(bytes)
        .map_err(|e| DbxError::Auth(format!("failed to write credentials file: {e}")))
}

#[cfg(not(unix))]
fn write_secret_file(path: &Path, bytes: &[u8]) -> Result<(), DbxError> {
    std::fs::write(path, bytes)
        .map_err(|e| DbxError::Auth(format!("failed to write credentials file: {e}")))
}

fn validate_client_id(candidate: &str) -> Result<&str, DbxError> {
    reject_dangerous_chars(candidate, "client id")?;
    if candidate.is_empty() {
        return Err(DbxError::Validation(
            "client id must not be empty".to_string(),
        ));
    }
    if !candidate
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err(DbxError::Validation(
            "client id must use only ASCII letters, digits, '-' or '_'".to_string(),
        ));
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::sync::mpsc;

    fn spawn_token_server(status: &str, body: &str) -> (String, mpsc::Receiver<String>) {
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
            let request = format!(
                "{}\n{}",
                request_head,
                String::from_utf8(request_body).unwrap()
            );
            tx.send(request).unwrap();

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

    #[test]
    fn resolves_client_id_precedence() {
        assert_eq!(
            resolve_client_id_from_sources(Some("flag123"), Some("env456")).unwrap(),
            "flag123"
        );
        assert_eq!(
            resolve_client_id_from_sources(None, Some("env456")).unwrap(),
            "env456"
        );
        assert_eq!(
            resolve_client_id_from_sources(None, None).unwrap(),
            DEFAULT_SHARED_CLIENT_ID
        );
    }

    #[test]
    fn rejects_bad_client_id() {
        for candidate in ["", "bad?id", "bad id", "bad/id", "bad\u{202E}id"] {
            let err = resolve_client_id_from_sources(Some(candidate), None).unwrap_err();
            assert!(err.to_string().contains("client id"));
        }
    }

    #[test]
    fn pkce_challenge_matches_rfc_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = pkce_challenge_from_verifier(verifier).unwrap();
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn rejects_invalid_pkce_verifiers() {
        for verifier in [
            "short",
            "has spaces in verifier value that is long enough",
            "bad/verifier/value/that/is/long/enough",
        ] {
            let err = pkce_challenge_from_verifier(verifier).unwrap_err();
            assert!(err.to_string().contains("PKCE verifier"));
        }
    }

    #[test]
    fn generated_pkce_pair_uses_base64url_shape() {
        let pair = generate_pkce_pair().unwrap();
        assert!(pair.verifier.len() >= 43);
        assert!(pair.verifier.len() <= 128);
        assert!(pair
            .verifier
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')));
        assert!(pair
            .challenge
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')));
        assert_eq!(pair.challenge.len(), 43);
    }

    #[test]
    fn generated_state_uses_base64url_shape() {
        let state = generate_state().unwrap();
        assert!(!state.is_empty());
        assert!(state
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')));
    }

    #[test]
    fn builds_login_plan_without_secrets() {
        let plan = build_login_plan_from_parts(
            "abc123".to_string(),
            true,
            "challenge-value".to_string(),
            Some("state-value"),
        );
        assert_eq!(plan.client_id, "abc123");
        assert_eq!(plan.redirect_uri, DEFAULT_REDIRECT_URI);
        assert_eq!(plan.token_access_type, TOKEN_ACCESS_TYPE_OFFLINE);
        assert_eq!(plan.code_challenge_method, CODE_CHALLENGE_METHOD_S256);
        assert!(plan.authorization_url.contains("client_id=abc123"));
        assert!(plan.authorization_url.contains("token_access_type=offline"));
        assert!(plan
            .authorization_url
            .contains("code_challenge=challenge-value"));
        assert!(plan.authorization_url.contains("state=state-value"));
        assert!(plan.authorization_url.contains("scope=account_info.read%20files.metadata.read%20files.content.read%20files.content.write"));
        assert!(!plan.authorization_url.contains("verifier"));
        assert!(plan.no_browser);
    }

    #[test]
    fn login_plan_can_omit_state() {
        let plan = build_login_plan_from_parts(
            "abc123".to_string(),
            false,
            "challenge-value".to_string(),
            None,
        );
        assert!(!plan.authorization_url.contains("state="));
        assert!(!plan.no_browser);
    }

    #[test]
    fn login_session_keeps_verifier_internal() {
        let session = build_login_session(Some("abc123"), true).unwrap();
        let json = serde_json::to_string(&session.plan).unwrap();
        assert!(!json.contains(&session.pkce_verifier));
        assert!(session.plan.authorization_url.contains("state="));
    }

    #[test]
    fn parses_oauth_callback_request_line() {
        let callback = parse_callback_request_line(
            "GET /oauth/callback?code=abc%20123&state=state-1 HTTP/1.1",
        )
        .unwrap();
        assert_eq!(callback.code, "abc 123");
        assert_eq!(callback.state, "state-1");
    }

    #[test]
    fn rejects_invalid_oauth_callback_requests() {
        for request_line in [
            "POST /oauth/callback?code=abc&state=state HTTP/1.1",
            "GET /oauth/callback HTTP/1.1",
            "GET /oauth/callback?state=state HTTP/1.1",
            "GET /oauth/callback?code=abc HTTP/1.1",
            "GET /oauth/callback?code=%ZZ&state=state HTTP/1.1",
            "GET /oauth/callback?code=%E2%82&state=state HTTP/1.1",
        ] {
            assert!(
                parse_callback_request_line(request_line).is_err(),
                "request should fail: {request_line}"
            );
        }
    }

    #[test]
    fn rejects_state_mismatch() {
        let callback = CallbackQuery {
            code: "code".to_string(),
            state: "wrong".to_string(),
        };
        let err = verify_callback_state(&callback, "expected").unwrap_err();
        assert!(err.to_string().contains("state"));
    }

    #[test]
    fn token_request_body_uses_pkce_without_client_secret() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let body = build_token_request_body(
            "client123",
            "code 123",
            verifier,
            "http://127.0.0.1:53682/oauth/callback",
        )
        .unwrap();
        assert!(body.contains("grant_type=authorization_code"));
        assert!(body.contains("client_id=client123"));
        assert!(body.contains("code=code%20123"));
        assert!(body.contains("code_verifier="));
        assert!(body.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A53682%2Foauth%2Fcallback"));
        assert!(!body.contains("client_secret"));
    }

    #[tokio::test]
    async fn exchanges_authorization_code_with_pkce() {
        let (url, request_rx) = spawn_token_server(
            "200 OK",
            r#"{"access_token":"access","token_type":"bearer","expires_in":3600,"refresh_token":"refresh","scope":"account_info.read","uid":"uid","account_id":"acct"}"#,
        );
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

        let response = exchange_authorization_code_at(
            &url,
            "client123",
            "code 123",
            verifier,
            DEFAULT_REDIRECT_URI,
        )
        .await
        .unwrap();

        assert_eq!(response.access_token, "access");
        assert_eq!(response.refresh_token.as_deref(), Some("refresh"));
        let request = request_rx.recv().unwrap();
        assert!(request.starts_with("POST / HTTP/1.1"));
        assert!(request.contains("grant_type=authorization_code"));
        assert!(request.contains("code=code%20123"));
        assert!(request.contains("client_id=client123"));
        assert!(request.contains("code_verifier="));
        assert!(!request.contains("client_secret"));
    }

    #[tokio::test]
    async fn exchange_authorization_code_reports_dropbox_errors() {
        let (url, _request_rx) = spawn_token_server(
            "400 Bad Request",
            r#"{"error":"invalid_grant","error_description":"bad code"}"#,
        );
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

        let err = exchange_authorization_code_at(
            &url,
            "client123",
            "code",
            verifier,
            DEFAULT_REDIRECT_URI,
        )
        .await
        .unwrap_err();

        match err {
            DbxError::Api {
                status,
                message,
                body,
            } => {
                assert_eq!(status, 400);
                assert_eq!(message, "Dropbox token exchange failed");
                assert_eq!(body.unwrap()["error"], "invalid_grant");
            }
            other => panic!("expected API error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn exchange_authorization_code_reports_invalid_success_json() {
        let (url, _request_rx) = spawn_token_server("200 OK", "not-json");
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

        let err = exchange_authorization_code_at(
            &url,
            "client123",
            "code",
            verifier,
            DEFAULT_REDIRECT_URI,
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("failed to parse token response"));
    }

    #[test]
    fn credentials_from_token_response_calculates_expiry_and_scopes() {
        let response = TokenResponse {
            access_token: "access".to_string(),
            token_type: "bearer".to_string(),
            expires_in: Some(3600),
            refresh_token: Some("refresh".to_string()),
            scope: Some("files.metadata.read files.content.read".to_string()),
            uid: Some("uid".to_string()),
            account_id: Some("acct".to_string()),
        };
        let credentials = credentials_from_token_response("client".to_string(), response, 1000);
        assert_eq!(credentials.client_id, "client");
        assert_eq!(credentials.access_token, "access");
        assert_eq!(credentials.expires_at_unix_seconds, Some(4600));
        assert_eq!(
            credentials.scopes,
            vec!["files.metadata.read", "files.content.read"]
        );
        assert_eq!(credentials.refresh_token.as_deref(), Some("refresh"));
        assert_eq!(credentials.account_id.as_deref(), Some("acct"));
        assert_eq!(credentials.uid.as_deref(), Some("uid"));
    }

    #[test]
    fn credentials_allow_missing_optional_token_fields() {
        let response = TokenResponse {
            access_token: "access".to_string(),
            token_type: "bearer".to_string(),
            expires_in: None,
            refresh_token: None,
            scope: None,
            uid: None,
            account_id: None,
        };
        let credentials = credentials_from_token_response("client".to_string(), response, 1000);
        assert!(credentials.scopes.is_empty());
        assert_eq!(credentials.expires_at_unix_seconds, None);
        assert_eq!(credentials.refresh_token, None);
    }

    #[test]
    fn stores_and_loads_credentials() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("credentials.json");
        let credentials = StoredCredentials {
            client_id: "client".to_string(),
            access_token: "access".to_string(),
            refresh_token: Some("refresh".to_string()),
            account_id: Some("acct".to_string()),
            uid: Some("uid".to_string()),
            scopes: vec!["files.metadata.read".to_string()],
            expires_at_unix_seconds: Some(123),
        };

        store_credentials(&path, &credentials).unwrap();
        let loaded = load_credentials(&path).unwrap();
        assert_eq!(loaded, credentials);

        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn load_credentials_reports_read_and_parse_errors() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.json");
        assert!(load_credentials(&missing)
            .unwrap_err()
            .to_string()
            .contains("failed to read"));

        let invalid = dir.path().join("credentials.json");
        fs::write(&invalid, "not-json").unwrap();
        assert!(load_credentials(&invalid)
            .unwrap_err()
            .to_string()
            .contains("failed to parse"));
    }

    #[test]
    fn default_credentials_path_uses_override_env() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        unsafe { std::env::set_var("DBX_CLI_CREDENTIALS_FILE", &path) };
        let resolved = default_credentials_path().unwrap();
        unsafe { std::env::remove_var("DBX_CLI_CREDENTIALS_FILE") };
        assert_eq!(resolved, path);
    }

    #[test]
    fn current_unix_seconds_returns_current_time() {
        assert!(current_unix_seconds().unwrap() > 0);
    }
}
