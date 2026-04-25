---
name: recipe-list-folder-recursive
description: "List every entry under a Dropbox folder recursively with paging and field projection."
metadata:
  version: 0.1.0
  openclaw:
    category: "recipe"
    domain: "productivity"
    requires:
      bins:
        - dbx
      skills:
        - dbx-files
---

# Recursive Folder List

> **PREREQUISITE:** Load the following skills to execute this recipe: `dbx-files`

List every entry under a Dropbox path with cursor pagination and field projection.

## Steps

1. List recursively with paging: `dbx files list_folder --json '{"path":"/Projects","recursive":true,"limit":2000}' --page-all --page-limit 50 --fields entries.name,entries.path_display,entries.id,cursor,has_more`
2. Filter file-vs-folder via `jq` (the `--fields` mask cannot address `.tag`): `dbx files list_folder --json '{"path":"","recursive":true,"limit":2000}' --page-all --page-limit 100 | jq -c '.entries[] | select(.[".tag"]=="file") | .path_display'`
3. Resume after a page-limit hit: capture the last `cursor`, then `dbx files list_folder_continue --json '{"cursor":"<cursor>"}' --page-all --page-limit 50`. Cursors are short-lived; resume promptly or restart.

## Tips

- Use `path: ""` to walk the entire Dropbox root.
- Set `--page-limit` deliberately. Unbounded walks waste tokens and Dropbox quota.
- Cursor expired mid-walk → restart from `files.list_folder`; do not retry the same cursor.
- Rate limited (`too_many_requests`, exit 20) → back off and retry the whole walk.

## See Also

- [dbx-shared](../dbx-shared/SKILL.md) — `--page-all`, `--fields`, error handling
- [dbx-files](../dbx-files/SKILL.md) — `list_folder` and `list_folder_continue` reference
