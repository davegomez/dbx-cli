---
name: dbx-users
description: "Dropbox users: Account info for the authenticated token."
metadata:
  version: 0.1.0
  openclaw:
    category: "productivity"
    requires:
      bins:
        - dbx
    cliHelp: "dbx users --help"
---

# users

> **PREREQUISITE:** Read `../dbx-shared/SKILL.md` for auth, global flags, and security rules.

```bash
dbx users <method> [flags]
```

## API Methods

### get_current_account

  - `get_current_account` — Returns the profile for the resolved access token. Schema is `{"type":"null"}`, so the executor sends a `null` body — pass nothing, or `--json 'null'` explicitly. Response: `{account_id, name:{given_name, surname, display_name}, email, email_verified, disabled, ...}`.

## Examples

```bash
# Full profile.
dbx users get_current_account

# Identity only.
dbx users get_current_account --fields account_id,email,name.display_name

# Token validity probe.
dbx users get_current_account --fields account_id
```

## When to Call

- After `dbx auth login` to confirm the right account is connected.
- Before destructive ops, to assert the token belongs to the expected account.
- As a low-cost token-validity probe (faster than a `list_folder`).

A 401 means the token is missing, expired, or revoked — re-run `dbx auth login` or refresh `DBX_CLI_TOKEN`.

## Discovering Commands

```bash
dbx users --help
dbx schema users.get_current_account
```

## See Also

- [dbx-shared](../dbx-shared/SKILL.md) — Global flags, auth, errors
