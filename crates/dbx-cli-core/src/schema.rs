use crate::error::DbxError;
use crate::operations::{find_operation_by_dotted_name, operation_tree, operations};
use serde_json::{json, Value};

pub fn operation_schema(path: &str) -> Result<Value, DbxError> {
    let op = find_operation_by_dotted_name(path)?;
    Ok(json!({
        "name": op.dotted_name(),
        "description": op.description,
        "httpMethod": "POST",
        "url": op.url(),
        "authRequired": op.auth_required,
        "requestBody": op.request_schema,
        "response": op.response_schema,
        "pagination": op.pagination,
        "agentGuidance": {
            "rawPayload": "Pass request body with --json or --json-file. Payload maps directly to Dropbox API JSON.",
            "dryRun": "Use --dry-run before mutating operations.",
            "fields": "Use --fields to client-side trim large responses for context control."
        }
    }))
}

pub fn registry_schema() -> Value {
    let resources = operation_tree()
        .into_iter()
        .map(|(resource, ops)| {
            let methods: Vec<Value> = ops
                .into_iter()
                .map(|op| {
                    json!({
                        "name": op.method,
                        "dottedName": op.dotted_name(),
                        "description": op.description,
                        "schemaCommand": format!("dbx schema {}", op.dotted_name())
                    })
                })
                .collect();
            json!({"resource": resource, "methods": methods})
        })
        .collect::<Vec<_>>();

    json!({
        "name": "dbx-cli",
        "description": "Agent-first Dropbox CLI operation registry",
        "operationCount": operations().len(),
        "resources": resources
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_operation_schema() {
        let schema = operation_schema("files.list_folder").unwrap();
        assert_eq!(schema["name"], "files.list_folder");
        assert_eq!(schema["httpMethod"], "POST");
        assert_eq!(schema["authRequired"], true);
        assert!(schema["url"]
            .as_str()
            .unwrap()
            .ends_with("/files/list_folder"));
        assert!(schema["requestBody"]["properties"].get("path").is_some());
        assert_eq!(
            schema["pagination"]["continue_operation"],
            "files.list_folder_continue"
        );
        assert!(schema["agentGuidance"]["rawPayload"].is_string());
    }

    #[test]
    fn rejects_unknown_operation_schema() {
        let err = operation_schema("files.unknown").unwrap_err();
        assert!(err.to_string().contains("unknown operation"));
    }

    #[test]
    fn rejects_malformed_operation_schema_path() {
        let err = operation_schema("files").unwrap_err();
        assert!(err.to_string().contains("resource.method"));
    }

    #[test]
    fn emits_registry_schema() {
        let schema = registry_schema();
        assert_eq!(schema["name"], "dbx-cli");
        assert_eq!(schema["operationCount"], operations().len());
        let resources = schema["resources"].as_array().unwrap();
        assert!(resources
            .iter()
            .any(|resource| resource["resource"] == "files"));
        assert!(resources
            .iter()
            .any(|resource| resource["resource"] == "users"));
    }
}
