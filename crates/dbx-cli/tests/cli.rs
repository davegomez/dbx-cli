use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::tempdir;

fn dbx() -> Command {
    let mut command = Command::cargo_bin("dbx").expect("dbx binary should build");
    command
        .env_remove("DBX_CLI_TOKEN")
        .env_remove("DBXCLI_TOKEN")
        .env_remove("DROPBOX_ACCESS_TOKEN")
        .env(
            "DBX_CLI_CREDENTIALS_FILE",
            "/tmp/dbx-cli-test-missing-credentials.json",
        );
    command
}

fn stdout_json(command: &mut Command) -> Value {
    let output = command.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).expect("stdout should be JSON")
}

fn write_credentials(path: &std::path::Path) {
    std::fs::write(
        path,
        r#"{
            "clientId":"client",
            "accessToken":"access-secret",
            "refreshToken":"refresh-secret",
            "accountId":"acct",
            "uid":"uid",
            "scopes":["account_info.read"],
            "expiresAtUnixSeconds":1
        }"#,
    )
    .unwrap();
}

#[test]
fn operations_prints_registry_json() {
    let json = stdout_json(dbx().arg("operations"));

    assert_eq!(json["name"], "dbx-cli");
    assert_eq!(json["operationCount"], 5);
    assert!(json["resources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|resource| { resource["resource"] == "files" }));
}

#[test]
fn schema_prints_registry_when_no_operation_is_given() {
    let json = stdout_json(dbx().arg("schema"));

    assert_eq!(json["name"], "dbx-cli");
    assert!(json["resources"].is_array());
}

#[test]
fn schema_prints_operation_schema() {
    let json = stdout_json(dbx().args(["schema", "files.list_folder"]));

    assert_eq!(json["name"], "files.list_folder");
    assert_eq!(json["authRequired"], true);
    assert_eq!(json["requestBody"]["required"][0], "path");
}

#[test]
fn auth_login_no_browser_prints_plan_without_credentials() {
    let json = stdout_json(dbx().args(["auth", "login", "--no-browser", "--json"]));

    assert_eq!(json["clientId"], "o70nz9ebged3rpq");
    assert_eq!(json["noBrowser"], true);
    assert_eq!(json["codeChallengeMethod"], "S256");
    assert!(json["authorizationUrl"]
        .as_str()
        .unwrap()
        .contains("https://www.dropbox.com/oauth2/authorize"));
    assert!(!json.to_string().contains("verifier"));
}

#[test]
fn auth_status_prints_metadata_without_secrets() {
    let dir = tempdir().unwrap();
    let credentials = dir.path().join("credentials.json");
    write_credentials(&credentials);

    let json = stdout_json(
        dbx()
            .env("DBX_CLI_CREDENTIALS_FILE", &credentials)
            .args(["auth", "status"]),
    );

    assert_eq!(json["authenticated"], true);
    assert_eq!(json["hasRefreshToken"], true);
    assert_eq!(json["accountId"], "acct");
    assert_eq!(json["expired"], true);
    assert!(!json.to_string().contains("access-secret"));
    assert!(!json.to_string().contains("refresh-secret"));
}

#[test]
fn auth_status_reports_missing_credentials_as_json() {
    let dir = tempdir().unwrap();
    let credentials = dir.path().join("missing.json");

    let json = stdout_json(
        dbx()
            .env("DBX_CLI_CREDENTIALS_FILE", &credentials)
            .args(["auth", "status"]),
    );

    assert_eq!(json["authenticated"], false);
    assert_eq!(json["credentialsFileExists"], false);
}

#[test]
fn auth_logout_supports_dry_run_and_delete() {
    let dir = tempdir().unwrap();
    let credentials = dir.path().join("credentials.json");
    write_credentials(&credentials);

    let dry_run = stdout_json(dbx().env("DBX_CLI_CREDENTIALS_FILE", &credentials).args([
        "auth",
        "logout",
        "--dry-run",
    ]));

    assert_eq!(dry_run["dryRun"], true);
    assert_eq!(dry_run["credentialsFileExisted"], true);
    assert!(credentials.exists());

    let deleted = stdout_json(
        dbx()
            .env("DBX_CLI_CREDENTIALS_FILE", &credentials)
            .args(["auth", "logout"]),
    );

    assert_eq!(deleted["loggedOut"], true);
    assert!(!credentials.exists());
}

#[test]
fn dry_run_prints_request_plan_without_credentials() {
    let json = stdout_json(dbx().args([
        "files",
        "get_metadata",
        "--json",
        r#"{"path":"/README.md"}"#,
        "--dry-run",
    ]));

    assert_eq!(json["dryRun"], true);
    assert_eq!(json["operation"], "files.get_metadata");
    assert_eq!(
        json["request"]["headers"]["Authorization"],
        "Bearer <redacted>"
    );
}

#[test]
fn dry_run_can_read_json_from_file() {
    let dir = tempdir().unwrap();
    let payload = dir.path().join("payload.json");
    std::fs::write(&payload, r#"{"path":"/README.md"}"#).unwrap();

    let json = stdout_json(dbx().current_dir(dir.path()).args([
        "files",
        "get_metadata",
        "--json-file",
        "payload.json",
        "--dry-run",
    ]));

    assert_eq!(json["request"]["json"]["path"], "/README.md");
}

#[test]
fn dry_run_can_read_json_from_stdin() {
    let mut command = dbx();
    command.args(["files", "get_metadata", "--json", "@-", "--dry-run"]);
    command.write_stdin(r#"{"path":"/README.md"}"#);

    let json = stdout_json(&mut command);

    assert_eq!(json["request"]["json"]["path"], "/README.md");
}

#[test]
fn dry_run_supports_field_projection() {
    let json = stdout_json(dbx().args([
        "users",
        "get_current_account",
        "--dry-run",
        "--fields",
        "request.json",
    ]));

    assert_eq!(json["request"]["json"], Value::Null);
}

#[test]
fn page_all_prints_ndjson() {
    dbx()
        .args([
            "files",
            "list_folder",
            "--json",
            r#"{"path":"","limit":10}"#,
            "--dry-run",
            "--page-all",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\n").and(predicate::str::contains("dryRun")));
}

#[test]
fn invalid_json_prints_structured_validation_error() {
    dbx()
        .args(["files", "get_metadata", "--json", "not-json"])
        .assert()
        .code(40)
        .stdout(predicate::str::contains(r#""type": "validation""#))
        .stderr(predicate::str::contains("invalid JSON payload"));
}

#[test]
fn missing_auth_prints_structured_auth_error() {
    dbx()
        .args(["users", "get_current_account"])
        .assert()
        .code(30)
        .stdout(predicate::str::contains(r#""type": "auth""#))
        .stderr(predicate::str::contains("dbx auth login"));
}
