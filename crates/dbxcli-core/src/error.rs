use serde_json::{json, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbxError {
    #[error("api error {status}: {message}")]
    Api {
        status: u16,
        message: String,
        body: Option<Value>,
    },
    #[error("auth error: {0}")]
    Auth(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("schema error: {0}")]
    Schema(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl DbxError {
    pub const EXIT_CODE_API: i32 = 20;
    pub const EXIT_CODE_AUTH: i32 = 30;
    pub const EXIT_CODE_VALIDATION: i32 = 40;
    pub const EXIT_CODE_SCHEMA: i32 = 50;
    pub const EXIT_CODE_OTHER: i32 = 1;

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Api { .. } => Self::EXIT_CODE_API,
            Self::Auth(_) => Self::EXIT_CODE_AUTH,
            Self::Validation(_) => Self::EXIT_CODE_VALIDATION,
            Self::Schema(_) => Self::EXIT_CODE_SCHEMA,
            Self::Other(_) => Self::EXIT_CODE_OTHER,
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Self::Api {
                status,
                message,
                body,
            } => json!({
                "error": {"type": "api", "status": status, "message": message, "body": body}
            }),
            Self::Auth(message) => json!({"error": {"type": "auth", "message": message}}),
            Self::Validation(message) => {
                json!({"error": {"type": "validation", "message": message}})
            }
            Self::Schema(message) => json!({"error": {"type": "schema", "message": message}}),
            Self::Other(err) => json!({"error": {"type": "internal", "message": err.to_string()}}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_errors_to_exit_codes() {
        let cases = [
            (
                DbxError::Api {
                    status: 400,
                    message: "bad request".to_string(),
                    body: None,
                },
                DbxError::EXIT_CODE_API,
            ),
            (
                DbxError::Auth("missing token".to_string()),
                DbxError::EXIT_CODE_AUTH,
            ),
            (
                DbxError::Validation("invalid payload".to_string()),
                DbxError::EXIT_CODE_VALIDATION,
            ),
            (
                DbxError::Schema("unknown operation".to_string()),
                DbxError::EXIT_CODE_SCHEMA,
            ),
            (
                DbxError::Other(anyhow::anyhow!("unexpected")),
                DbxError::EXIT_CODE_OTHER,
            ),
        ];

        for (error, expected_code) in cases {
            assert_eq!(error.exit_code(), expected_code);
        }
    }

    #[test]
    fn serializes_errors_as_structured_json() {
        let api_error = DbxError::Api {
            status: 409,
            message: "conflict".to_string(),
            body: Some(json!({"reason": "path_lookup"})),
        };
        assert_eq!(api_error.to_json()["error"]["type"], "api");
        assert_eq!(api_error.to_json()["error"]["status"], 409);
        assert_eq!(
            api_error.to_json()["error"]["body"]["reason"],
            "path_lookup"
        );

        assert_eq!(
            DbxError::Auth("missing".to_string()).to_json()["error"]["type"],
            "auth"
        );
        assert_eq!(
            DbxError::Validation("bad".to_string()).to_json()["error"]["type"],
            "validation"
        );
        assert_eq!(
            DbxError::Schema("unknown".to_string()).to_json()["error"]["type"],
            "schema"
        );
        assert_eq!(
            DbxError::Other(anyhow::anyhow!("boom")).to_json()["error"]["type"],
            "internal"
        );
    }
}
