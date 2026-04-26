#!/usr/bin/env bash
# Sync Changesets version updates into Cargo, npm, skills, and lockfiles.
# Used by changesets/action as the custom version command.
set -euo pipefail

pnpm exec changeset version

VERSION=$(node <<'NODE'
const fs = require('fs');
const sources = ['./npm/package.json', './package.json'];
for (const source of sources) {
  if (fs.existsSync(source)) {
    const version = JSON.parse(fs.readFileSync(source, 'utf8')).version;
    if (version) {
      console.log(version);
      process.exit(0);
    }
  }
}
throw new Error('No package version found');
NODE
)

VERSION="$VERSION" node <<'NODE'
const fs = require('fs');
const version = process.env.VERSION;
for (const file of ['./package.json', './npm/package.json']) {
  if (!fs.existsSync(file)) continue;
  const pkg = JSON.parse(fs.readFileSync(file, 'utf8'));
  pkg.version = version;
  fs.writeFileSync(file, JSON.stringify(pkg, null, 2) + '\n');
}
NODE

python3 - "$VERSION" <<'PY'
import pathlib
import sys

version = sys.argv[1]

cargo = pathlib.Path('Cargo.toml')
if cargo.exists():
    lines = cargo.read_text().splitlines()
    in_workspace_package = False
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped == '[workspace.package]':
            in_workspace_package = True
            continue
        if stripped.startswith('[') and stripped != '[workspace.package]':
            in_workspace_package = False
        if in_workspace_package and stripped.startswith('version = '):
            lines[index] = f'version = "{version}"'
            break
    cargo.write_text('\n'.join(lines) + '\n')

for skill in pathlib.Path('skills').glob('**/SKILL.md') if pathlib.Path('skills').exists() else []:
    lines = skill.read_text().splitlines()
    in_metadata = False
    changed = False
    for index, line in enumerate(lines):
        if line == 'metadata:':
            in_metadata = True
            continue
        if in_metadata and line and not line.startswith(' '):
            in_metadata = False
        if in_metadata and line.startswith('  version:'):
            lines[index] = f'  version: {version}'
            changed = True
            break
    if changed:
        skill.write_text('\n'.join(lines) + '\n')
PY

cargo generate-lockfile

files=(package.json Cargo.toml Cargo.lock)
[ -f pnpm-lock.yaml ] && files+=(pnpm-lock.yaml)
[ -f npm/package.json ] && files+=(npm/package.json)
[ -f CHANGELOG.md ] && files+=(CHANGELOG.md)
[ -d .changeset ] && files+=(.changeset)
[ -d skills ] && files+=(skills)
[ -f npm/CHANGELOG.md ] && files+=(npm/CHANGELOG.md)

git add "${files[@]}"
