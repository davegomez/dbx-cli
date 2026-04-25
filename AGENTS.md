# AGENTS.md

This repo builds `dbx`, an agent-first Dropbox CLI.

## Git

Use git-flow to manage Git branching (run `git flow help`) and always finish branches with rebase instead of merge.

## Build commands

Use Cargo only:

```bash
cargo fmt
cargo test
cargo run -p dbxcli -- --help
```

## Design constraints

- Keep Dropbox API knowledge in `crates/dbxcli-core`.
- Generate CLI surface from `operations.rs`; avoid hand-written one-off commands.
- Default stdout must remain machine-readable JSON or NDJSON.
- Errors must remain structured JSON.
- All mutating operations must support `--dry-run`.
- Raw JSON payload path (`--json`, `--json @file`, `--json @-`) is first-class.
- Add schema metadata for every new operation.
- Harden every input path against agent failure modes: traversal, embedded query/fragment, percent-encoded bypass, control chars, dangerous Unicode.

## Testing

When adding operations:

1. Add registry entry in `crates/dbxcli-core/src/operations.rs`.
2. Add schema/introspection test if new shape is nontrivial.
3. Add dry-run test for mutating operation.
4. Run `cargo fmt && cargo test`.
