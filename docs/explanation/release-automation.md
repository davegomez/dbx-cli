# Release automation design

`dbx-cli` release automation has two responsibilities that are deliberately separated:

1. decide what version should be released;
2. publish artefacts from an immutable semver tag.

This split keeps everyday development on `develop`, keeps `main` as the release branch, and makes publishing repeatable when a registry or workflow fails part-way through a release.

## Why Changesets drives versioning

Changesets records release intent at the time a change is made. That matters because `dbx-cli` publishes more than one package:

- Rust library crate: `dbx-cli-core`
- Rust binary crate: `dbx-cli`
- npm installer package: `@silky/dbx-cli`
- generated agent skills with version metadata

A single Changeset lets the release PR update all versioned surfaces together. Without that release PR, maintainers would have to manually keep `Cargo.toml`, `Cargo.lock`, npm metadata, skill metadata, and the changelog in sync.

## Why tags trigger publishing

The release workflow publishes from `vX.Y.Z` tags rather than from branch pushes. Tags provide a stable release identity:

- GitHub Release assets are attached to the tag.
- npm installer URLs point at that tag.
- crates.io packages record source metadata for that tag's commit.
- failed runs can be retried against the same version.

The tag is the boundary between preparing a release and publishing it.

## Why GitHub assets publish before npm

The npm package is a thin installer. It does not contain platform binaries. During `postinstall`, it downloads the matching `dbx` archive from the GitHub Release and verifies the `.sha256` checksum.

Because npm install depends on GitHub Release assets, the release workflow creates or refreshes GitHub Release assets before publishing `@silky/dbx-cli`.

## Why crates publish in dependency order

`dbx-cli` depends on `dbx-cli-core`. crates.io must know about `dbx-cli-core@X.Y.Z` before it accepts `dbx-cli@X.Y.Z`.

The workflow publishes `dbx-cli-core` first, waits for that version to appear in the crates.io index, then publishes `dbx-cli`.

## Why publishing is idempotent

A release can fail after partial success. For example:

- GitHub assets may upload, then npm publishing may fail.
- `dbx-cli-core` may publish, then `dbx-cli` may fail while waiting for the index.
- npm may publish, then public package visibility may lag.

Registries do not allow overwriting an existing version, so a retry must skip what already exists. The release workflow therefore checks each destination before publishing:

- existing GitHub Releases are edited and assets are uploaded with `--clobber`;
- existing crates.io versions are skipped;
- existing npm versions are skipped;
- npm access is set to public after publish or skip.

Idempotency makes recovery safe without inventing a new version number for infrastructure failures.

## Why npm uses the `@silky` scope

npm rejected the unscoped `dbx-cli` package name as too similar to existing packages. Publishing under `@silky/dbx-cli` avoids the package-name policy issue and uses an organisation the maintainer controls.

The executable remains `dbx`, so users install the package with:

```bash
npm install -g @silky/dbx-cli
```

and run:

```bash
dbx --help
```

The package name changed; the command name did not.

## Why private repositories skip attestations

GitHub build provenance attestations are useful for public release assets, but GitHub does not support this feature for user-owned private repositories. The workflow skips attestations when `github.event.repository.private` is true and runs them otherwise.

This keeps public releases attestable without blocking private-repository release rehearsals.

## Why `develop` and `main` both matter

`develop` is the integration branch used by git-flow and the Changesets release workflow. `main` is the release branch and repository default branch.

The release tag identifies the exact commit that was published. For stable releases, `main` should be fast-forwarded to that commit so users browsing the default branch see the released source and documentation.
