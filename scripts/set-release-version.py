#!/usr/bin/env python3
"""Update all checked-in Rhei package version files. §FS-rhei-distribution.2"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Sequence


VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")


def replace_one(path: Path, pattern: str, replacement: str) -> None:
    text = path.read_text(encoding="utf-8")
    new, count = re.subn(pattern, replacement, text, count=1, flags=re.MULTILINE)
    if count != 1:
        raise RuntimeError(f"{path}: expected one match for {pattern!r}, found {count}")
    path.write_text(new, encoding="utf-8")


def update_json(path: Path, version: str) -> None:
    data = json.loads(path.read_text(encoding="utf-8"))
    data["version"] = version
    if path.name == "package.json" and data.get("name") == "rhei-api":
        data.setdefault("dependencies", {})["rhei"] = version
    path.write_text(json.dumps(data, indent=2, ensure_ascii=True) + "\n", encoding="utf-8")


def update(version: str) -> None:
    root = Path.cwd()
    if VERSION_RE.match(version) is None:
        raise RuntimeError(f"version must look like 0.1.0, got {version!r}")

    replace_one(root / "Cargo.toml", r'^(version\s*=\s*)"[0-9]+\.[0-9]+\.[0-9]+"$', rf'\1"{version}"')

    internal_dep_version = re.compile(
        r'(\{ package = "rhei-[^"]+", path = "[^"]+", version = ")=[0-9]+\.[0-9]+\.[0-9]+(" \})'
    )
    for manifest in (root / "crates").glob("*/Cargo.toml"):
        text = manifest.read_text(encoding="utf-8")
        text = internal_dep_version.sub(rf"\1={version}\2", text)
        manifest.write_text(text, encoding="utf-8")

    update_json(root / "packages/npm/rhei-cli/package.json", version)
    update_json(root / "packages/npm/rhei-api/package.json", version)

    replace_one(root / "packages/python/rhei-cli/pyproject.toml", r'^(version\s*=\s*)"[0-9]+\.[0-9]+\.[0-9]+"$', rf'\1"{version}"')
    replace_one(root / "packages/python/rhei-api/pyproject.toml", r'^(version\s*=\s*)"[0-9]+\.[0-9]+\.[0-9]+"$', rf'\1"{version}"')
    replace_one(
        root / "packages/python/rhei-api/pyproject.toml",
        r'"rhei-cli==[0-9]+\.[0-9]+\.[0-9]+"',
        f'"rhei-cli=={version}"',
    )

    replace_one(root / "packages/python/rhei-cli/src/rhei_cli/__init__.py", r'^__version__ = "[0-9]+\.[0-9]+\.[0-9]+"$', f'__version__ = "{version}"')
    replace_one(root / "packages/python/rhei-cli/src/rhei_cli/__init__.py", r'^_CRATE_VERSION = "[0-9]+\.[0-9]+\.[0-9]+"$', f'_CRATE_VERSION = "{version}"')
    replace_one(root / "packages/python/rhei-api/src/rhei_api/__init__.py", r'^__version__ = "[0-9]+\.[0-9]+\.[0-9]+"$', f'__version__ = "{version}"')


def main(argv: Sequence[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    if len(args) != 1:
        print("usage: scripts/set-release-version.py <version>", file=sys.stderr)
        return 2
    try:
        update(args[0])
    except RuntimeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
