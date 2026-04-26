#!/usr/bin/env bash
# Create and push a release tag based on package.json.
# Idempotent: exits cleanly if the tag already exists locally or remotely.
set -euo pipefail

VERSION=$(node -p "require('./package.json').version")
TAG="v${VERSION}"

if git rev-parse -q --verify "refs/tags/${TAG}" >/dev/null || \
   git ls-remote --exit-code --tags origin "refs/tags/${TAG}" >/dev/null 2>&1; then
  echo "Tag ${TAG} already exists, skipping"
  exit 0
fi

echo "Creating tag ${TAG}"
git tag "${TAG}"
git push origin "refs/tags/${TAG}"
