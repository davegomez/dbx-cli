---
name: dbx-files
description: "Dropbox files: List, inspect, and delete files and folders."
metadata:
  version: 0.1.0
  openclaw:
    category: "productivity"
    requires:
      bins:
        - dbx
    cliHelp: "dbx files --help"
---

# files

> **PREREQUISITE:** Read `../dbx-shared/SKILL.md` for auth, global flags, dry-run, paging, fields, errors, and path safety.

```bash
dbx files <method> [flags]
```

## Helper Commands

None registered yet. (Helper commands would be prefixed `+` in the gws style; dbx has no `+upload`, `+download`, or similar wired up today.)

## API Methods

### list_folder

  - `list_folder` — List folder contents under a path. Required: `path` (`""` for root, or absolute like `/Projects`). Optional: `limit` (1–2000), `recursive`, `include_media_info`, `include_deleted`, `include_has_explicit_shared_members`, `include_mounted_folders` (default true), `shared_link`. Response: `{entries[], cursor, has_more}`. Each entry has a `.tag` discriminator (`file | folder | deleted`). Paginated — pair with `--page-all --page-limit N`.
  - `list_folder_continue` — Continue a `list_folder` cursor. Required: `cursor`. Same response shape as `list_folder`. Used automatically by `--page-all`; call directly only when persisting cursors across invocations.

### get_metadata

  - `get_metadata` — Fetch metadata for one path or id. Required: `path` (absolute path, or `id:<id>`). Optional: `include_media_info`, `include_deleted`, `include_has_explicit_shared_members`. Response: a `metadata` object with `.tag` discriminator. Useful as a cheap existence check before mutating.

### delete_v2 (mutating)

  - `delete_v2` — Delete a file or folder. Required: `path`. Response: `{metadata: {...}}`. **Always run with `--dry-run` first.** See `recipe-delete-path` for the safe four-step flow.

## Examples

```bash
# List root, names + ids only.
dbx files list_folder --json '{"path":""}' \
  --fields entries.name,entries.id,cursor,has_more

# Recursive walk with paging.
dbx files list_folder \
  --json '{"path":"/Projects","recursive":true,"limit":2000}' \
  --page-all --page-limit 50 \
  --fields entries.name,entries.path_display,cursor,has_more

# Existence check by id.
dbx files get_metadata --json '{"path":"id:abc123"}'

# Delete (dry-run first!).
dbx files delete_v2 --json '{"path":"/tmp/old.txt"}' --dry-run
dbx files delete_v2 --json '{"path":"/tmp/old.txt"}'
```

> [!CAUTION]
> `delete_v2` is a **write** command — confirm with the user before executing.

## Discovering Commands

```bash
dbx files --help
dbx schema files.list_folder
dbx schema files.delete_v2
```

## Not Yet Registered

`files.upload`, `files.download`, search, batch ops, and async jobs are not in the registry today. Do not invent them. If asked, surface the gap to the user.

## See Also

- [dbx-shared](../dbx-shared/SKILL.md) — Global flags, auth, errors, path safety
- [recipe-list-folder-recursive](../recipe-list-folder-recursive/SKILL.md) — Walk an entire folder tree
- [recipe-delete-path](../recipe-delete-path/SKILL.md) — Safe delete flow
