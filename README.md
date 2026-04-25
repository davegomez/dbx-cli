# dbx-cli (`dbx`)

Agent-first command-line interface for Dropbox.

`dbx` is built for agents and automation. It accepts raw Dropbox JSON payloads, emits structured JSON, exposes machine-readable schemas, supports dry-runs, and authenticates with Dropbox OAuth 2 PKCE.

All examples assume `dbx` is installed and available on `PATH`.

## Quick start

Authenticate with Dropbox:

```bash
dbx auth login
```

Check stored authentication status:

```bash
dbx auth status
```

Verify the authenticated account:

```bash
dbx users get_current_account
```

List the root folder:

```bash
dbx files list_folder --json '{"path":"","limit":10}'
```

Reduce response size for agent context:

```bash
dbx files list_folder \
  --json '{"path":"","limit":10}' \
  --fields entries.name,entries.id,cursor,has_more
```

## How to authenticate

Use the built-in shared Dropbox app key:

```bash
dbx auth login
```

`dbx` opens Dropbox in your browser, waits for approval on a local callback URL, exchanges the authorisation code using PKCE, and stores credentials locally.

The callback URL is:

```text
http://127.0.0.1:53682/oauth/callback
```

Successful login prints JSON like:

```json
{
  "authenticated": true,
  "accountId": "dbid:...",
  "credentialsPath": "/Users/you/.config/dbx-cli/credentials.json",
  "hasRefreshToken": true,
  "scopes": ["account_info.read", "files.content.read", "files.content.write", "files.metadata.read"],
  "uid": "..."
}
```

### Use an existing token

For CI, scripts, or managed agent environments:

```bash
export DBX_CLI_TOKEN="<dropbox-access-token>"
```

`DROPBOX_ACCESS_TOKEN` is also supported.

Credential precedence:

1. `DBX_CLI_TOKEN`
2. `DBXCLI_TOKEN`
3. `DROPBOX_ACCESS_TOKEN`
4. credentials from `dbx auth login`

Stored credentials are refreshed automatically when expired and retried once after a Dropbox 401 response when a refresh token is available.

### Check auth status

Print structured authentication status without exposing access or refresh tokens:

```bash
dbx auth status
```

Example output:

```json
{
  "authenticated": true,
  "credentialsFileExists": true,
  "credentialsPath": "/Users/you/.config/dbx-cli/credentials.json",
  "accountId": "dbid:...",
  "uid": "...",
  "scopes": ["account_info.read", "files.metadata.read"],
  "hasRefreshToken": true,
  "expiresAtUnixSeconds": 1710000000,
  "expired": false
}
```

### Log out

Remove stored credentials safely:

```bash
dbx auth logout
```

Preview logout without deleting credentials:

```bash
dbx auth logout --dry-run
```

`auth logout` only removes the credentials file. It does not revoke app authorization in Dropbox.

### Inspect auth without logging in

Print the OAuth authorisation plan without opening a browser, listening for a callback, exchanging a token, or writing credentials:

```bash
dbx auth login --no-browser --json
```

Use this when an agent needs to inspect the exact authorisation URL.

## How to send JSON payloads

Pass raw Dropbox API request bodies with `--json`:

```bash
dbx files get_metadata --json '{"path":"/README.md"}'
```

Read from a file:

```bash
dbx files get_metadata --json @payload.json
```

or:

```bash
dbx files get_metadata --json-file payload.json
```

Read from stdin:

```bash
echo '{"path":"/README.md"}' | dbx files get_metadata --json @-
```

For safety, JSON files must resolve inside the current working directory.

## How to discover operations

List supported operations:

```bash
dbx operations
```

Inspect one operation schema:

```bash
dbx schema files.list_folder
```

Inspect the whole schema registry:

```bash
dbx schema
```

Use schemas before constructing unfamiliar payloads. Schema commands do not require authentication and do not contact Dropbox.

## How to list files

List one page from the root folder:

```bash
dbx files list_folder --json '{"path":"","limit":10}'
```

List a subfolder:

```bash
dbx files list_folder --json '{"path":"/Reports","limit":50}'
```

List recursively:

```bash
dbx files list_folder --json '{"path":"/Reports","recursive":true,"limit":100}'
```

Continue with a cursor manually:

```bash
dbx files list_folder_continue --json '{"cursor":"<cursor>"}'
```

Follow cursor pagination automatically and print one JSON page per line:

```bash
dbx files list_folder \
  --json '{"path":"","limit":100}' \
  --page-all --page-limit 5
```

## How to reduce output size

Use client-side field projection with `--fields`:

```bash
dbx files list_folder \
  --json '{"path":"","limit":10}' \
  --fields entries.name,entries.path_display,cursor,has_more
```

Nested fields use dot notation. Arrays are preserved. Missing fields are omitted.

Force newline-delimited JSON output:

```bash
dbx files list_folder \
  --json '{"path":"","limit":10}' \
  --format ndjson
```

`--page-all` always prints NDJSON.

## How to inspect account details

```bash
dbx users get_current_account
```

Limit fields:

```bash
dbx users get_current_account --fields account_id,email,name.display_name
```

## How to delete safely

Dry-run first:

```bash
dbx files delete_v2 --json '{"path":"/old.txt"}' --dry-run
```

Review the request plan. Then delete:

```bash
dbx files delete_v2 --json '{"path":"/old.txt"}'
```

`--dry-run` validates input and prints the request plan without requiring credentials or contacting Dropbox.

## Command reference

### Top-level commands

| Command | Purpose |
| --- | --- |
| `dbx auth` | Dropbox authentication commands. |
| `dbx files` | Dropbox file operations. |
| `dbx users` | Dropbox user/account operations. |
| `dbx operations` | Print the supported operation registry as JSON. |
| `dbx schema [resource.method]` | Print machine-readable schemas. |
| `dbx help [command]` | Print help. |
| `dbx --version` | Print version. |

### Auth commands

| Command | Purpose |
| --- | --- |
| `dbx auth login` | Start OAuth 2 PKCE browser login and store credentials. |
| `dbx auth login --no-browser --json` | Print an authorisation plan only. |
| `dbx auth login --client-id <ID>` | Use a specific Dropbox app key for this login. |
| `dbx auth status` | Print structured auth status without secrets. |
| `dbx auth logout` | Delete stored credentials. |
| `dbx auth logout --dry-run` | Preview logout without deleting credentials. |

### Supported Dropbox operations

| Operation | Command | Payload |
| --- | --- | --- |
| `users.get_current_account` | `dbx users get_current_account` | none |
| `files.list_folder` | `dbx files list_folder` | `{"path":"","limit":10}` |
| `files.list_folder_continue` | `dbx files list_folder_continue` | `{"cursor":"..."}` |
| `files.get_metadata` | `dbx files get_metadata` | `{"path":"/file.txt"}` |
| `files.delete_v2` | `dbx files delete_v2` | `{"path":"/file.txt"}` |

### Operation flags

These flags apply to Dropbox operation commands:

| Flag | Purpose |
| --- | --- |
| `--json <JSON>` | Raw Dropbox request body. |
| `--json @path` | Read request body from file. |
| `--json @-` | Read request body from stdin. |
| `--json-file <path>` | Read request body from file. |
| `--dry-run` | Validate and print request plan without calling Dropbox. |
| `--page-all` | Follow cursor pagination and print NDJSON. |
| `--page-limit <N>` | Maximum pages to fetch with `--page-all`. |
| `--fields <paths>` | Project response fields client-side. |
| `--format json` | Pretty JSON output. |
| `--format ndjson` | One compact JSON object per line. |

## Configuration reference

### Dropbox app key

Default shared app key:

```text
o70nz9ebged3rpq
```

Override with a flag:

```bash
dbx auth login --client-id <app-key>
```

Override with environment:

```bash
export DBX_CLI_CLIENT_ID="<app-key>"
dbx auth login
```

Client ID precedence:

1. `--client-id <ID>`
2. `DBX_CLI_CLIENT_ID`
3. built-in shared app key

If you use your own Dropbox app, register this redirect URI exactly:

```text
http://127.0.0.1:53682/oauth/callback
```

### Credentials file

Default credentials path:

```text
~/.config/dbx-cli/credentials.json
```

Override with:

```bash
export DBX_CLI_CREDENTIALS_FILE="/path/to/credentials.json"
```

The credentials file contains access and refresh tokens. Do not commit it or print it in logs. Override paths are rejected if they contain traversal (`..`), query or fragment markers, percent-encoding, control characters, or dangerous Unicode.

### Dropbox scopes

`dbx auth login` requests:

```text
account_info.read
files.metadata.read
files.content.read
files.content.write
```

## Output and errors

Successful commands print JSON to stdout.

API and validation errors also print JSON to stdout, plus a short human-readable message to stderr:

```json
{
  "error": {
    "type": "validation",
    "message": "invalid JSON payload: ..."
  }
}
```

Exit codes:

| Code | Meaning |
| --- | --- |
| `20` | Dropbox API error. |
| `30` | Authentication error. |
| `40` | Validation error. |
| `50` | Schema error. |
| `1` | Internal error. |

## Input safety

`dbx` validates JSON strings before sending requests to Dropbox.

Rejected input includes:

- control characters
- dangerous Unicode formatting characters
- Dropbox paths containing `..` traversal segments
- Dropbox paths containing query or fragment markers (`?` or `#`)
- pre-percent-encoded Dropbox paths containing `%`

Pass Dropbox paths as raw Dropbox paths, not URLs.

## Current limitations

- Supported Dropbox operations are listed by `dbx operations`.
