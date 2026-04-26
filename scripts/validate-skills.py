#!/usr/bin/env python3
"""Validate hand-maintained dbx skill metadata without external dependencies."""

from __future__ import annotations

import pathlib
import re
import sys

SKILLS_DIR = pathlib.Path("skills")
SEMVER = re.compile(r"^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$")
SAFE_NAME = re.compile(r"^[a-z0-9][a-z0-9-]*$")
CONTROL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")


def error(path: pathlib.Path, message: str) -> None:
    print(f"::error file={path},title=Invalid skill::{message}", file=sys.stderr)


def unquote(value: str) -> str:
    value = value.strip()
    if (value.startswith('"') and value.endswith('"')) or (
        value.startswith("'") and value.endswith("'")
    ):
        return value[1:-1]
    return value


def frontmatter(path: pathlib.Path, text: str) -> list[str] | None:
    lines = text.splitlines()
    if not lines or lines[0] != "---":
        error(path, "SKILL.md must start with YAML frontmatter delimited by '---'.")
        return None
    try:
        end = lines[1:].index("---") + 1
    except ValueError:
        error(path, "SKILL.md frontmatter must end with '---'.")
        return None
    return lines[1:end]


def parse_skill(path: pathlib.Path) -> tuple[dict[str, str], list[str]] | None:
    text = path.read_text(encoding="utf-8")
    match = CONTROL.search(text)
    if match:
        error(path, f"Control character U+{ord(match.group()):04X} is not allowed.")
        return None

    lines = frontmatter(path, text)
    if lines is None:
        return None

    fields: dict[str, str] = {}
    required_skills: list[str] = []
    in_metadata = False
    in_openclaw = False
    in_requires = False
    in_required_skills = False

    for line in lines:
        if line.startswith("name:"):
            fields["name"] = unquote(line.split(":", 1)[1])
        elif line.startswith("description:"):
            fields["description"] = unquote(line.split(":", 1)[1])
        elif line == "metadata:":
            in_metadata = True
            in_openclaw = False
            in_requires = False
            in_required_skills = False
        elif in_metadata and line.startswith("  version:"):
            fields["version"] = unquote(line.split(":", 1)[1])
        elif in_metadata and line == "  openclaw:":
            in_openclaw = True
            in_requires = False
            in_required_skills = False
        elif in_openclaw and line.startswith("    category:"):
            fields["category"] = unquote(line.split(":", 1)[1])
        elif in_openclaw and line == "    requires:":
            in_requires = True
            in_required_skills = False
        elif in_requires and line == "      skills:":
            in_required_skills = True
        elif in_required_skills and line.startswith("        - "):
            required_skills.append(unquote(line.split("- ", 1)[1]))
        elif line and not line.startswith(" "):
            in_metadata = False
            in_openclaw = False
            in_requires = False
            in_required_skills = False

    return fields, required_skills


def main() -> int:
    skill_files = sorted(SKILLS_DIR.glob("*/SKILL.md"))
    if not skill_files:
        print("::error title=No skills found::Expected at least one skills/*/SKILL.md file.", file=sys.stderr)
        return 1

    failed = False
    names: dict[str, pathlib.Path] = {}
    required_by_path: dict[pathlib.Path, list[str]] = {}

    for path in skill_files:
        parsed = parse_skill(path)
        if parsed is None:
            failed = True
            continue
        fields, required_skills = parsed
        required_by_path[path] = required_skills

        name = fields.get("name", "")
        if not name:
            error(path, "Missing required frontmatter field: name.")
            failed = True
        elif not SAFE_NAME.match(name):
            error(path, "Skill name must be lowercase kebab-case.")
            failed = True
        elif path.parent.name != name:
            error(path, f"Skill name '{name}' must match directory '{path.parent.name}'.")
            failed = True
        elif name in names:
            error(path, f"Duplicate skill name also used by {names[name]}.")
            failed = True
        else:
            names[name] = path

        description = fields.get("description", "")
        if not description:
            error(path, "Missing required frontmatter field: description.")
            failed = True

        version = fields.get("version", "")
        if not version:
            error(path, "Missing required metadata.version.")
            failed = True
        elif not SEMVER.match(version):
            error(path, f"metadata.version '{version}' must be semver, for example 0.1.0.")
            failed = True

        if not fields.get("category"):
            error(path, "Missing required metadata.openclaw.category.")
            failed = True

    for path, required_skills in required_by_path.items():
        for required in required_skills:
            if required not in names:
                error(path, f"Required skill '{required}' does not exist under skills/.")
                failed = True

    if failed:
        return 1

    print(f"Validated {len(skill_files)} skills.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
