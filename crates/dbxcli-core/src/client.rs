use crate::error::DbxError;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use std::sync::OnceLock;

const CONNECT_TIMEOUT_SECS: u64 = 10;

pub fn access_token_from_env() -> Result<String, DbxError> {
    if let Ok(token) = std::env::var("DBXCLI_TOKEN") {
        return Ok(token);
    }
    if let Ok(token) = std::env::var("DROPBOX_ACCESS_TOKEN") {
        return Ok(token);
    }
    if let Ok(path) = crate::auth::default_credentials_path() {
        if let Ok(credentials) = crate::auth::load_credentials(&path) {
            return Ok(credentials.access_token);
        }
    }
    Err(DbxError::Auth(
        "set DBXCLI_TOKEN or DROPBOX_ACCESS_TOKEN, or run `dbx auth login`".to_string(),
    ))
}

fn build_client_inner() -> Result<reqwest::Client, String> {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&format!("dbxcli/{}", env!("CARGO_PKG_VERSION")))
            .map_err(|e| e.to_string())?,
    );

    reqwest::Client::builder()
        .default_headers(headers)
        .connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("failed to build HTTP client: {e}"))
}

pub fn shared_client() -> Result<reqwest::Client, DbxError> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    match CLIENT.get_or_init(build_client_inner) {
        Ok(client) => Ok(client.clone()),
        Err(message) => Err(DbxError::Other(anyhow::anyhow!(message.clone()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{store_credentials, StoredCredentials};
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn set_env(key: &str, value: &str) {
        unsafe { std::env::set_var(key, value) }
    }

    fn remove_env(key: &str) {
        unsafe { std::env::remove_var(key) }
    }

    fn clear_auth_env() {
        remove_env("DBXCLI_TOKEN");
        remove_env("DROPBOX_ACCESS_TOKEN");
        remove_env("DBXCLI_CREDENTIALS_FILE");
    }

    #[test]
    fn access_token_prefers_dbxcli_token() {
        let _guard = env_lock().lock().unwrap();
        clear_auth_env();
        set_env("DBXCLI_TOKEN", "primary");
        set_env("DROPBOX_ACCESS_TOKEN", "fallback");

        assert_eq!(access_token_from_env().unwrap(), "primary");

        clear_auth_env();
    }

    #[test]
    fn access_token_uses_dropbox_token_when_primary_missing() {
        let _guard = env_lock().lock().unwrap();
        clear_auth_env();
        set_env("DROPBOX_ACCESS_TOKEN", "fallback");

        assert_eq!(access_token_from_env().unwrap(), "fallback");

        clear_auth_env();
    }

    #[test]
    fn access_token_reads_stored_credentials() {
        let _guard = env_lock().lock().unwrap();
        clear_auth_env();
        let dir = tempdir().unwrap();
        let path = dir.path().join("credentials.json");
        let credentials = StoredCredentials {
            client_id: "client".to_string(),
            access_token: "stored".to_string(),
            refresh_token: Some("refresh".to_string()),
            account_id: Some("account".to_string()),
            uid: Some("uid".to_string()),
            scopes: vec!["account_info.read".to_string()],
            expires_at_unix_seconds: Some(123),
        };
        store_credentials(&path, &credentials).unwrap();
        set_env("DBXCLI_CREDENTIALS_FILE", path.to_str().unwrap());

        assert_eq!(access_token_from_env().unwrap(), "stored");

        clear_auth_env();
    }

    #[test]
    fn access_token_errors_when_no_credentials_exist() {
        let _guard = env_lock().lock().unwrap();
        clear_auth_env();
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.json");
        set_env("DBXCLI_CREDENTIALS_FILE", path.to_str().unwrap());

        let err = access_token_from_env().unwrap_err();

        assert!(err.to_string().contains("dbx auth login"));
        clear_auth_env();
    }

    #[test]
    fn shared_client_can_be_constructed() {
        let client = shared_client().unwrap();
        let _request = client.get("https://api.dropboxapi.com/2/test");
    }
}
