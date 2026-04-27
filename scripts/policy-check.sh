#!/usr/bin/env bash
# Repository policy checks for release metadata and dbx invariants.
set -eo pipefail

changed_files_file=${CHANGED_FILES_FILE:-}
changed_files=()

if [[ -n "$changed_files_file" && -f "$changed_files_file" ]]; then
  while IFS= read -r line; do
    changed_files+=("$line")
  done < "$changed_files_file"
else
  while IFS= read -r line; do
    changed_files+=("$line")
  done < <(git diff --name-only HEAD)
fi

is_pr=false
case "${GITHUB_EVENT_NAME:-}" in
  pull_request|pull_request_target) is_pr=true ;;
esac

head_ref=${GITHUB_HEAD_REF:-}

requires_changeset=false
has_changeset=false

for file in "${changed_files[@]}"; do
  case "$file" in
    Cargo.toml|Cargo.lock)
      requires_changeset=true
      ;;
    crates/*)
      if [[ "$file" == *.rs || "$file" == */Cargo.toml ]]; then
        requires_changeset=true
      fi
      ;;
  esac

  if [[ "$file" == .changeset/*.md && "$(basename "$file")" != "README.md" ]]; then
    has_changeset=true
  fi
done

if [[ "$is_pr" == true && "$requires_changeset" == true && "$head_ref" != changeset-release/* ]]; then
  if [[ "$has_changeset" != true ]]; then
    {
      echo "::error title=Missing changeset::Rust or Cargo changes require a changeset. Run 'pnpm changeset', choose package '@silky/dbx-cli', commit the generated .changeset/*.md file, then push again."
      echo "Changed Rust/Cargo files:"
      for file in "${changed_files[@]}"; do
        case "$file" in
          Cargo.toml|Cargo.lock|crates/*)
            if [[ "$file" == Cargo.toml || "$file" == Cargo.lock || "$file" == *.rs || "$file" == */Cargo.toml ]]; then
              echo "  - $file"
            fi
            ;;
        esac
      done
    } >&2
    exit 1
  fi
fi

changesets=()
if [[ -d .changeset ]]; then
  while IFS= read -r -d '' file; do
    [[ "$(basename "$file")" == "README.md" ]] && continue
    changesets+=("$file")
  done < <(find .changeset -maxdepth 1 -type f -name '*.md' -print0)
fi

if (( ${#changesets[@]} > 0 )); then
  python3 - "${changesets[@]}" <<'PY'
import pathlib
import re
import sys

allowed = {"@silky/dbx-cli"}
failed = False
for raw_path in sys.argv[1:]:
    path = pathlib.Path(raw_path)
    text = path.read_text()
    match = re.match(r"^---\n(.*?)\n---\n", text, re.DOTALL)
    if not match:
        print(f"::error file={path},title=Invalid changeset::Changeset is missing frontmatter. Run 'pnpm changeset' to regenerate it.", file=sys.stderr)
        failed = True
        continue

    packages = []
    for line in match.group(1).splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        package_match = re.match(r"^[\"']?([^\"':]+)[\"']?\s*:\s*(patch|minor|major)\s*$", stripped)
        if package_match:
            packages.append(package_match.group(1))

    if not packages:
        print(f"::error file={path},title=Invalid changeset::Changeset must include a semver bump for package '@silky/dbx-cli'.", file=sys.stderr)
        failed = True
        continue

    for package in packages:
        if package not in allowed:
            print(f"::error file={path},title=Invalid changeset package::Unsupported changeset package '{package}'. Use '@silky/dbx-cli'.", file=sys.stderr)
            failed = True

if failed:
    sys.exit(1)
PY
fi

violations=""
if [[ -d crates ]]; then
  violations=$(find crates -path 'crates/dbx-cli-core' -prune -o -type f -name '*.rs' -print0 \
    | xargs -0 grep -nE 'dropboxapi\.com' 2>/dev/null || true)
fi

if [[ -n "$violations" ]]; then
  {
    echo "::error title=Dropbox API knowledge outside core::Dropbox API endpoint literals must stay in crates/dbx-cli-core. Move host/path knowledge into core operations/client modules."
    echo "$violations"
  } >&2
  exit 1
fi

echo "Policy checks passed."
