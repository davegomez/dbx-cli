---
name: recipe-delete-path
description: "Delete a Dropbox file or folder safely with the schema → dry-run → confirm → execute flow."
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

# Safe Delete Path

> **PREREQUISITE:** Load the following skills to execute this recipe: `dbx-files`

Delete a Dropbox path through the four-step gate. Dropbox keeps a trash, but `dbx` does not undo on its own.

## Steps

1. Confirm the schema: `dbx schema files.delete_v2` — verify it is still single-field `path`.
2. Verify the target exists: `dbx files get_metadata --json '{"path":"/tmp/old.txt"}'` — `path/not_found` here means the user's path is wrong.
3. Dry-run the delete: `dbx files delete_v2 --json '{"path":"/tmp/old.txt"}' --dry-run` — surface the JSON plan to the user.
4. After explicit user confirmation, execute: `dbx files delete_v2 --json '{"path":"/tmp/old.txt"}'`.

## Tips

- Re-dry-run before each execution even if you ran one earlier in the conversation.
- Validation rejection (exit 40) → fix the path per the path-safety rules in `dbx-shared`.
- `path/conflict` (exit 20) → concurrent change; refresh metadata before retrying.
- No `delete_batch` operation is registered today. To delete multiple paths, loop with confirmation per path.

> [!CAUTION]
> `delete_v2` is a **write** command — confirm each path with the user before executing.

## See Also

- [dbx-shared](../dbx-shared/SKILL.md) — `--dry-run` semantics, error codes, path safety
- [dbx-files](../dbx-files/SKILL.md) — `delete_v2`, `get_metadata`, schema
