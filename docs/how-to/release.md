# How to release dbx-cli

This guide shows maintainers how to publish a stable `dbx-cli` release to GitHub Releases, crates.io, and npm.

**Prerequisites:** clean `develop` and `main` branches, release secrets configured, and a user-visible change with a Changeset.

## Check release secrets

Confirm these repository secrets exist before releasing:

- `CARGO_REGISTRY_TOKEN`: crates.io token allowed to publish `dbx-cli-core` and `dbx-cli`.
- `NPM_TOKEN`: npm automation token allowed to publish `@silky/dbx-cli` under the `@silky` organisation.
- `DBX_RELEASE_TOKEN` (optional): token used by the Changesets release workflow. If absent, `GITHUB_TOKEN` is used.

## Add a Changeset

From a feature branch, add a Changeset for user-visible changes:

```bash
pnpm changeset
```

Choose package `@silky/dbx-cli` and select the semver bump.

Commit the generated `.changeset/*.md` file with the code or documentation change it describes.

## Merge changes to `develop`

Finish the git-flow branch with rebase, then push `develop`:

```bash
git flow feature finish <name> --rebase --ff
git push origin develop
```

The **Release (Changesets)** workflow opens or updates a release pull request.

## Review the release pull request

Check that the release PR updates:

- `package.json`
- `npm/package.json`
- `Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- skill metadata under `skills/*/SKILL.md`, when present

The PR title is:

```text
chore: release versions
```

Merge the release PR into `develop` when the version and changelog are correct.

## Publish the release tag

After the release PR lands on `develop`, the **Release (Changesets)** workflow runs again. With no pending Changesets, it runs:

```bash
pnpm run tag-release
```

That script creates and pushes `vX.Y.Z` from the version in `package.json`. The tag triggers the **Release** workflow.

If the tag already exists locally or remotely, the script exits cleanly and does not recreate it.

## Keep `main` aligned

`main` is the release branch. After a stable release commit lands on `develop`, fast-forward `main` to the same commit:

```bash
git switch main
git merge --ff-only develop
git push origin main
```

Do this before moving a release tag manually. Never force-push `main` for a release.

## Watch the release workflow

Find the latest release run:

```bash
gh run list --workflow Release --limit 5
```

Watch it to completion:

```bash
gh run watch <run-id> --exit-status
```

The run should complete these jobs:

- `plan`
- seven `build` matrix jobs
- `release`
- `publish-crates`
- `publish-npm`

## Verify published packages

Check GitHub Release metadata:

```bash
gh release view vX.Y.Z \
  --json tagName,targetCommitish,isDraft,isPrerelease,publishedAt,assets
```

Check crates.io:

```bash
cargo info dbx-cli-core@X.Y.Z --registry crates-io
cargo info dbx-cli@X.Y.Z --registry crates-io
```

Check npm:

```bash
npm view @silky/dbx-cli@X.Y.Z version
npm view @silky/dbx-cli@X.Y.Z bin --json
```

Expected npm binary mapping:

```json
{
  "dbx": "run.js"
}
```

## Smoke-test installs

Install from npm:

```bash
npm install -g @silky/dbx-cli
dbx --help
```

Install from crates.io:

```bash
cargo install dbx-cli
dbx --help
```

## Recover from a partial release

Use recovery only when publishing failed after some artefacts or packages already published.

1. Fix the workflow on a git-flow branch.
2. Fast-forward `develop` and `main` to the fix.
3. Move the release tag only if the broken tag points at the wrong workflow commit.
4. Re-run the release workflow by pushing the corrected tag.

Signed tag recreation pattern:

```bash
git push origin :refs/tags/vX.Y.Z
git tag -d vX.Y.Z
export GPG_TTY=$(tty)
git tag -s vX.Y.Z -m "vX.Y.Z"
git push origin vX.Y.Z
```

The release workflow is idempotent for existing GitHub Release assets, crates.io versions, and npm versions.
