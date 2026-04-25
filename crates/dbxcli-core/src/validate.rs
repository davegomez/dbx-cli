use crate::error::DbxError;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn is_dangerous_unicode(c: char) -> bool {
    matches!(c,
        '\u{200B}'..='\u{200D}' | '\u{FEFF}' |
        '\u{202A}'..='\u{202E}' |
        '\u{2028}'..='\u{2029}' |
        '\u{2066}'..='\u{2069}'
    )
}

pub fn reject_dangerous_chars(value: &str, label: &str) -> Result<(), DbxError> {
    for c in value.chars() {
        if c.is_control() {
            return Err(DbxError::Validation(format!(
                "{label} contains control characters"
            )));
        }
        if is_dangerous_unicode(c) {
            return Err(DbxError::Validation(format!(
                "{label} contains dangerous Unicode characters"
            )));
        }
    }
    Ok(())
}

pub fn validate_dropbox_path(path: &str) -> Result<&str, DbxError> {
    reject_dangerous_chars(path, "Dropbox path")?;
    if path.contains('?') || path.contains('#') {
        return Err(DbxError::Validation(
            "Dropbox path must not contain query or fragment markers".to_string(),
        ));
    }
    if path.contains('%') {
        return Err(DbxError::Validation(
            "Dropbox path must not be pre-percent-encoded".to_string(),
        ));
    }
    if path.split('/').any(|segment| segment == "..") {
        return Err(DbxError::Validation(
            "Dropbox path must not contain '..' traversal segments".to_string(),
        ));
    }
    Ok(path)
}

pub fn validate_json_strings(value: &Value) -> Result<(), DbxError> {
    match value {
        Value::String(s) => {
            reject_dangerous_chars(s, "JSON string")?;
            if looks_like_resource_or_path(s) {
                validate_dropbox_path(s)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                validate_json_strings(item)?;
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                reject_dangerous_chars(key, "JSON key")?;
                validate_json_strings(value)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
    Ok(())
}

fn looks_like_resource_or_path(value: &str) -> bool {
    value.starts_with('/')
        || value.starts_with("id:")
        || value.starts_with("rev:")
        || value.starts_with("ns:")
}

pub fn validate_api_name(name: &str) -> Result<&str, DbxError> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(DbxError::Validation(format!(
            "invalid API identifier '{name}'"
        )));
    }
    Ok(name)
}

pub fn validate_safe_file_path(path_str: &str, flag_name: &str) -> Result<PathBuf, DbxError> {
    reject_dangerous_chars(path_str, flag_name)?;
    let path = Path::new(path_str);
    let cwd = std::env::current_dir()
        .map_err(|e| DbxError::Validation(format!("failed to determine current directory: {e}")))?;
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    let canonical = if resolved.exists() {
        resolved
            .canonicalize()
            .map_err(|e| DbxError::Validation(format!("failed to resolve {flag_name}: {e}")))?
    } else {
        normalize_dotdot(&resolved)
    };
    let canonical_cwd = cwd.canonicalize().map_err(|e| {
        DbxError::Validation(format!("failed to canonicalize current directory: {e}"))
    })?;
    if !canonical.starts_with(&canonical_cwd) {
        return Err(DbxError::Validation(format!(
            "{flag_name} resolves outside current directory"
        )));
    }
    Ok(canonical)
}

fn normalize_dotdot(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

pub fn sanitize_for_terminal(text: &str) -> String {
    text.chars()
        .filter(|&c| {
            if c == '\n' || c == '\t' {
                return true;
            }
            if c.is_control() {
                return false;
            }
            !is_dangerous_unicode(c)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Mutex, OnceLock};

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn rejects_path_traversal() {
        let err = validate_dropbox_path("/safe/../secret").unwrap_err();
        assert!(err.to_string().contains("traversal"));
    }

    #[test]
    fn rejects_embedded_query_or_fragment() {
        for path in ["/file.txt?fields=name", "/file.txt#fragment"] {
            let err = validate_dropbox_path(path).unwrap_err();
            assert!(err.to_string().contains("query or fragment"));
        }
    }

    #[test]
    fn rejects_percent_encoded_paths_and_dangerous_chars() {
        assert!(validate_dropbox_path("/file%20name.txt")
            .unwrap_err()
            .to_string()
            .contains("percent"));
        assert!(validate_dropbox_path("/bad\u{202E}name")
            .unwrap_err()
            .to_string()
            .contains("dangerous Unicode"));
    }

    #[test]
    fn validates_nested_json_strings() {
        let payload =
            json!({"path": "/reports/q1", "entries": ["id:abc"], "ok": true, "n": 1, "none": null});
        validate_json_strings(&payload).unwrap();
    }

    #[test]
    fn rejects_unsafe_json_keys_and_values() {
        assert!(validate_json_strings(&json!({"bad\u{202E}key": "value"})).is_err());
        assert!(validate_json_strings(&json!({"path": "/safe/../secret"})).is_err());
        assert!(validate_json_strings(&json!(["/file%20name.txt"])).is_err());
    }

    #[test]
    fn validates_api_names() {
        assert_eq!(validate_api_name("files_1-ok").unwrap(), "files_1-ok");
        for name in ["", "files.list", "bad name", "bad/name"] {
            assert!(validate_api_name(name).is_err());
        }
    }

    #[test]
    fn validates_safe_file_paths_inside_cwd() {
        let _guard = cwd_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        std::fs::write("payload.json", "{}").unwrap();

        let resolved = validate_safe_file_path("payload.json", "--json-file").unwrap();

        std::env::set_current_dir(original).unwrap();
        assert!(resolved.ends_with("payload.json"));
    }

    #[test]
    fn rejects_safe_file_paths_outside_cwd() {
        let _guard = cwd_lock().lock().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(cwd.path()).unwrap();
        let path = outside.path().join("payload.json");
        std::fs::write(&path, "{}").unwrap();

        let err = validate_safe_file_path(path.to_str().unwrap(), "--json-file").unwrap_err();

        std::env::set_current_dir(original).unwrap();
        assert!(err.to_string().contains("outside current directory"));
    }

    #[test]
    fn normalizes_nonexistent_dotdot_file_paths() {
        let _guard = cwd_lock().lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        std::fs::create_dir("payloads").unwrap();

        let resolved = validate_safe_file_path("payloads/../payload.json", "--json-file").unwrap();

        std::env::set_current_dir(original).unwrap();
        assert!(resolved.ends_with("payload.json"));
    }

    #[test]
    fn sanitizes_terminal_output() {
        assert_eq!(sanitize_for_terminal("a\u{202E}b\x1b\n\t"), "ab\n\t");
    }
}
