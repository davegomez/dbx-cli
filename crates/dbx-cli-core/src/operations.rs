use crate::error::DbxError;
use crate::validate::validate_api_name;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HttpMethod {
    Post,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Host {
    Api,
    Content,
}

impl Host {
    pub fn base_url(self) -> &'static str {
        match self {
            Self::Api => "https://api.dropboxapi.com/2",
            Self::Content => "https://content.dropboxapi.com/2",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Pagination {
    pub cursor_field: &'static str,
    pub has_more_field: &'static str,
    pub continue_operation: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Operation {
    pub resource: &'static str,
    pub method: &'static str,
    pub description: &'static str,
    pub http_method: HttpMethod,
    pub host: Host,
    pub path: &'static str,
    pub auth_required: bool,
    pub request_schema: Value,
    pub response_schema: Value,
    pub pagination: Option<Pagination>,
}

impl Operation {
    pub fn dotted_name(&self) -> String {
        format!("{}.{}", self.resource, self.method)
    }

    pub fn url(&self) -> String {
        format!("{}{}", self.host.base_url(), self.path)
    }
}

pub fn operations() -> Vec<Operation> {
    vec![
        Operation {
            resource: "users",
            method: "get_current_account",
            description: "Get account profile for current Dropbox access token.",
            http_method: HttpMethod::Post,
            host: Host::Api,
            path: "/users/get_current_account",
            auth_required: true,
            request_schema: json!({"type": "null"}),
            response_schema: account_schema(),
            pagination: None,
        },
        Operation {
            resource: "files",
            method: "list_folder",
            description: "List folder contents. Use --page-all to follow cursor pages as NDJSON.",
            http_method: HttpMethod::Post,
            host: Host::Api,
            path: "/files/list_folder",
            auth_required: true,
            request_schema: list_folder_request_schema(),
            response_schema: list_folder_response_schema(),
            pagination: Some(Pagination {
                cursor_field: "cursor",
                has_more_field: "has_more",
                continue_operation: "files.list_folder_continue",
            }),
        },
        Operation {
            resource: "files",
            method: "list_folder_continue",
            description: "Continue a files.list_folder cursor.",
            http_method: HttpMethod::Post,
            host: Host::Api,
            path: "/files/list_folder/continue",
            auth_required: true,
            request_schema: json!({
                "type": "object",
                "required": ["cursor"],
                "additionalProperties": false,
                "properties": {"cursor": {"type": "string"}}
            }),
            response_schema: list_folder_response_schema(),
            pagination: Some(Pagination {
                cursor_field: "cursor",
                has_more_field: "has_more",
                continue_operation: "files.list_folder_continue",
            }),
        },
        Operation {
            resource: "files",
            method: "get_metadata",
            description: "Get metadata for file or folder path/id.",
            http_method: HttpMethod::Post,
            host: Host::Api,
            path: "/files/get_metadata",
            auth_required: true,
            request_schema: json!({
                "type": "object",
                "required": ["path"],
                "additionalProperties": false,
                "properties": {
                    "path": {"type": "string", "description": "Dropbox path or id:."},
                    "include_media_info": {"type": "boolean"},
                    "include_deleted": {"type": "boolean"},
                    "include_has_explicit_shared_members": {"type": "boolean"}
                }
            }),
            response_schema: metadata_schema(),
            pagination: None,
        },
        Operation {
            resource: "files",
            method: "delete_v2",
            description: "Delete file or folder path/id. Always run with --dry-run first.",
            http_method: HttpMethod::Post,
            host: Host::Api,
            path: "/files/delete_v2",
            auth_required: true,
            request_schema: json!({
                "type": "object",
                "required": ["path"],
                "additionalProperties": false,
                "properties": {"path": {"type": "string", "description": "Dropbox path or id:."}}
            }),
            response_schema: json!({
                "type": "object",
                "properties": {"metadata": metadata_schema()}
            }),
            pagination: None,
        },
    ]
}

pub fn operation_tree() -> BTreeMap<&'static str, Vec<Operation>> {
    let mut tree: BTreeMap<&'static str, Vec<Operation>> = BTreeMap::new();
    for op in operations() {
        tree.entry(op.resource).or_default().push(op);
    }
    for ops in tree.values_mut() {
        ops.sort_by_key(|op| op.method);
    }
    tree
}

pub fn find_operation(resource: &str, method: &str) -> Result<Operation, DbxError> {
    validate_api_name(resource)?;
    validate_api_name(method)?;
    operations()
        .into_iter()
        .find(|op| op.resource == resource && op.method == method)
        .ok_or_else(|| DbxError::Schema(format!("unknown operation '{resource}.{method}'")))
}

pub fn find_operation_by_dotted_name(path: &str) -> Result<Operation, DbxError> {
    let Some((resource, method)) = path.split_once('.') else {
        return Err(DbxError::Schema(format!(
            "schema path must look like resource.method, got '{path}'"
        )));
    };
    find_operation(resource, method)
}

fn list_folder_request_schema() -> Value {
    json!({
        "type": "object",
        "required": ["path"],
        "additionalProperties": false,
        "properties": {
            "path": {"type": "string", "description": "Dropbox folder path. Empty string lists root."},
            "recursive": {"type": "boolean", "default": false},
            "include_media_info": {"type": "boolean", "default": false},
            "include_deleted": {"type": "boolean", "default": false},
            "include_has_explicit_shared_members": {"type": "boolean", "default": false},
            "include_mounted_folders": {"type": "boolean", "default": true},
            "limit": {"type": "integer", "minimum": 1, "maximum": 2000},
            "shared_link": {"type": "object"}
        }
    })
}

fn list_folder_response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "entries": {"type": "array", "items": metadata_schema()},
            "cursor": {"type": "string"},
            "has_more": {"type": "boolean"}
        }
    })
}

fn metadata_schema() -> Value {
    json!({
        "oneOf": [
            {"$ref": "FileMetadata"},
            {"$ref": "FolderMetadata"},
            {"$ref": "DeletedMetadata"}
        ],
        "commonProperties": {
            ".tag": {"type": "string", "enum": ["file", "folder", "deleted"]},
            "name": {"type": "string"},
            "path_lower": {"type": "string"},
            "path_display": {"type": "string"},
            "id": {"type": "string"}
        }
    })
}

fn account_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "account_id": {"type": "string"},
            "name": {"type": "object"},
            "email": {"type": "string"},
            "email_verified": {"type": "boolean"},
            "disabled": {"type": "boolean"}
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_known_operation() {
        let op = find_operation_by_dotted_name("files.list_folder").unwrap();
        assert_eq!(op.path, "/files/list_folder");
        assert_eq!(op.url(), "https://api.dropboxapi.com/2/files/list_folder");
        assert_eq!(op.dotted_name(), "files.list_folder");
    }

    #[test]
    fn rejects_unknown_and_malformed_operations() {
        assert!(find_operation("files", "missing").is_err());
        assert!(find_operation("bad.name", "method").is_err());
        assert!(find_operation_by_dotted_name("files").is_err());
    }

    #[test]
    fn groups_operations_by_resource() {
        let tree = operation_tree();
        assert!(tree.get("files").unwrap().len() >= 3);
        assert_eq!(tree.get("users").unwrap().len(), 1);
        let file_methods = tree
            .get("files")
            .unwrap()
            .iter()
            .map(|operation| operation.method)
            .collect::<Vec<_>>();
        assert_eq!(
            file_methods,
            vec![
                "delete_v2",
                "get_metadata",
                "list_folder",
                "list_folder_continue"
            ]
        );
    }

    #[test]
    fn host_base_urls_are_dropbox_api_urls() {
        assert_eq!(Host::Api.base_url(), "https://api.dropboxapi.com/2");
        assert_eq!(Host::Content.base_url(), "https://content.dropboxapi.com/2");
    }
}
