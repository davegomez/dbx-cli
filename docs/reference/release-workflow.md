# Release workflow reference

`dbx-cli` uses two release workflows:

- `.github/workflows/release-changesets.yml`: prepares version bumps and creates release tags.
- `.github/workflows/release.yml`: builds artefacts and publishes packages from a semver tag.

## Package names

| Registry | Package | Purpose |
| --- | --- | --- |
| crates.io | `dbx-cli-core` | Core Dropbox operation registry, validation, and execution primitives. |
| crates.io | `dbx-cli` | CLI binary crate. Installs the `dbx` executable. |
| npm | `@silky/dbx-cli` | Prebuilt binary installer package. Installs the `dbx` executable. |

## Release secrets

| Secret | Required by | Purpose |
| --- | --- | --- |
| `CARGO_REGISTRY_TOKEN` | `release.yml` / `publish-crates` | Publishes `dbx-cli-core` and `dbx-cli` to crates.io. |
| `NPM_TOKEN` | `release.yml` / `publish-npm` | Publishes `@silky/dbx-cli` to npm and sets public access. |
| `DBX_RELEASE_TOKEN` | `release-changesets.yml` | Optional token for creating release PRs and tags. Falls back to `GITHUB_TOKEN`. |
| `GITHUB_TOKEN` | both workflows | Creates GitHub Releases, uploads assets, and authenticates fallback release automation. |

## `.github/workflows/release-changesets.yml`

### Triggers

- Manual dispatch.
- Push to `develop`.

### Permissions

- `contents: write`
- `pull-requests: write`

### Job: `release`

The job installs Rust, pnpm, and Node.js, then runs `changesets/action@v1`.

Configuration:

| Setting | Value |
| --- | --- |
| Version command | `pnpm run version-sync` |
| Publish command | `pnpm run tag-release` |
| Commit | `chore: release versions` |
| Pull request title | `chore: release versions` |

### Version command

`scripts/version-sync.sh` performs these updates after `pnpm exec changeset version`:

- synchronises `package.json` and `npm/package.json` versions;
- updates `[workspace.package] version` in `Cargo.toml`;
- updates `dbx-cli-core` dependency versions in crate manifests;
- updates skill metadata versions under `skills/**/SKILL.md`;
- regenerates `Cargo.lock`;
- stages release files for the Changesets commit.

### Publish command

`scripts/tag-release.sh` reads `package.json` and creates `vX.Y.Z`.

The script is idempotent. It exits successfully if the tag already exists locally or remotely.

## `.github/workflows/release.yml`

### Trigger

A pushed tag matching:

```text
v[0-9]+.[0-9]+.[0-9]+*
```

Examples:

- `v0.1.0`
- `v0.2.0-beta.1`

### Permissions

- `contents: write`
- `attestations: write`
- `id-token: write`

### Concurrency

The concurrency group is:

```text
release-${{ github.ref }}
```

A release run is not cancelled when a newer run starts for the same tag.

### Job: `plan`

Extracts release metadata from the tag:

| Output | Description |
| --- | --- |
| `version` | Tag without leading `v`. |
| `prerelease` | `true` when the version contains `-`; otherwise `false`. |

### Job: `build`

Builds and uploads release archives for every target.

| Target | Runner | Archive | Cross |
| --- | --- | --- | --- |
| `aarch64-apple-darwin` | `macos-latest` | `tar.gz` | no |
| `x86_64-apple-darwin` | `macos-latest` | `tar.gz` | no |
| `aarch64-unknown-linux-gnu` | `ubuntu-latest` | `tar.gz` | yes |
| `aarch64-unknown-linux-musl` | `ubuntu-latest` | `tar.gz` | yes |
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | `tar.gz` | no |
| `x86_64-unknown-linux-musl` | `ubuntu-latest` | `tar.gz` | yes |
| `x86_64-pc-windows-msvc` | `windows-latest` | `zip` | no |

Unix archive contents:

- `dbx`
- `LICENSE`
- `README.md`
- `CHANGELOG.md`

Windows archive contents:

- `dbx.exe`
- `LICENSE`
- `README.md`
- `CHANGELOG.md`

Each archive has a matching `.sha256` file.

Build provenance attestations run only for public repositories. Private repositories skip attestations because GitHub does not support the feature for user-owned private repositories.

### Job: `release`

Creates or updates the GitHub Release for the tag.

If the release does not exist, the job runs `gh release create` with generated notes and uploaded assets.

If the release already exists, the job:

- updates target, title, notes, and draft state;
- sets `--draft=false`;
- uploads assets with `--clobber`.

### Job: `publish-crates`

Runs only for stable releases where `prerelease == false`.

Publishing order:

1. `dbx-cli-core`
2. wait for `dbx-cli-core@X.Y.Z` to appear in crates.io index
3. `dbx-cli`

Existing crate versions are skipped. Existence checks use:

```bash
cargo info "$crate@$VERSION" --registry crates-io
```

Each new crate publish runs a dry-run first:

```bash
cargo publish --dry-run --locked --package <crate>
cargo publish --locked --package <crate>
```

### Job: `publish-npm`

Runs only for stable releases where `prerelease == false`.

The package is packed from `npm/`:

```bash
npm pack ./npm --dry-run
```

The publish step runs in `npm/` and derives the package name from `npm/package.json`.

Existing npm versions are skipped. New versions are published with:

```bash
npm publish --access public --provenance
```

After publish or skip, the job sets package access and verifies anonymous public visibility:

```bash
npm access set status=public "${PACKAGE_NAME}"
NPM_CONFIG_USERCONFIG="${PUBLIC_NPMRC}" npm view "${PACKAGE_NAME}@${VERSION}" version
```

## Published artefacts

GitHub Release assets use these names:

```text
dbx-cli-aarch64-apple-darwin.tar.gz
dbx-cli-aarch64-apple-darwin.tar.gz.sha256
dbx-cli-x86_64-apple-darwin.tar.gz
dbx-cli-x86_64-apple-darwin.tar.gz.sha256
dbx-cli-aarch64-unknown-linux-gnu.tar.gz
dbx-cli-aarch64-unknown-linux-gnu.tar.gz.sha256
dbx-cli-aarch64-unknown-linux-musl.tar.gz
dbx-cli-aarch64-unknown-linux-musl.tar.gz.sha256
dbx-cli-x86_64-unknown-linux-gnu.tar.gz
dbx-cli-x86_64-unknown-linux-gnu.tar.gz.sha256
dbx-cli-x86_64-unknown-linux-musl.tar.gz
dbx-cli-x86_64-unknown-linux-musl.tar.gz.sha256
dbx-cli-x86_64-pc-windows-msvc.zip
dbx-cli-x86_64-pc-windows-msvc.zip.sha256
```

## Stable versus prerelease behaviour

| Release type | GitHub Release | crates.io | npm |
| --- | --- | --- | --- |
| Stable tag, for example `v0.1.0` | published | published | published |
| Prerelease tag, for example `v0.2.0-beta.1` | published as prerelease | skipped | skipped |
