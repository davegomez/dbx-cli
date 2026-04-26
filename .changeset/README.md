# Changesets

Use changesets to describe user-visible changes that should appear in the next release PR.

## Add a changeset

From a feature branch, run:

```bash
pnpm changeset
```

Choose the npm package affected by the change, then choose the semver bump:

- `patch` for fixes and small compatible changes
- `minor` for new compatible features
- `major` for breaking changes

Commit the generated `.changeset/*.md` file with your code change.

## Release flow

When changesets land on `develop`, the Release Changesets workflow opens or updates a release PR. That PR runs `pnpm run version-sync` to update package versions, `Cargo.toml`, `Cargo.lock`, npm metadata, skill metadata, and changelogs.

Merging the release PR runs `pnpm run tag-release`, which creates an idempotent `vX.Y.Z` tag. That tag triggers the binary release workflow.
