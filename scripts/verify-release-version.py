#!/usr/bin/env python3
"""Verify checked-in package versions before publishing. §FS-rhei-distribution.2"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Sequence


VERSION_RE = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")


def read_workspace_version(root: Path) -> str:
    text = (root / "Cargo.toml").read_text(encoding="utf-8")
    in_workspace_package = False
    for line in text.splitlines():
        if line.strip() == "[workspace.package]":
            in_workspace_package = True
            continue
        if in_workspace_package and line.startswith("["):
            break
        if in_workspace_package:
            match = re.match(r'\s*version\s*=\s*"([^"]+)"\s*$', line)
            if match:
                return match.group(1)
    raise RuntimeError("Cargo.toml is missing [workspace.package] version")


def require_equal(label: str, actual: str, expected: str) -> None:
    if actual != expected:
        raise RuntimeError(f"{label} is {actual}, expected {expected}")


def require_contains(path: Path, needle: str) -> None:
    if needle not in path.read_text(encoding="utf-8"):
        raise RuntimeError(f"{path} is missing {needle!r}")


def verify(root: Path, version: str) -> None:
    if VERSION_RE.match(version) is None:
        raise RuntimeError(f"version must look like 0.1.0, got {version!r}")

    require_equal("workspace version", read_workspace_version(root), version)

    internal_dep_version = re.compile(
        r'\{ package = "rhei-[^"]+", path = "[^"]+", version = "=[0-9]+\.[0-9]+\.[0-9]+" \}'
    )
    for manifest in (root / "crates").glob("*/Cargo.toml"):
        text = manifest.read_text(encoding="utf-8")
        stale_exact = internal_dep_version.findall(text)
        for value in stale_exact:
            if f'version = "={version}"' not in value:
                raise RuntimeError(f"{manifest} internal dependency requirement is stale: {value}")

    npm_cli = json.loads((root / "packages/npm/rhei-cli/package.json").read_text(encoding="utf-8"))
    npm_api = json.loads((root / "packages/npm/rhei-api/package.json").read_text(encoding="utf-8"))
    require_equal("packages/npm/rhei-cli/package.json version", npm_cli["version"], version)
    require_equal("packages/npm/rhei-api/package.json version", npm_api["version"], version)
    require_equal("packages/npm/rhei-api/package.json dependency rhei", npm_api["dependencies"]["rhei"], version)

    require_contains(root / "packages/python/rhei-cli/pyproject.toml", f'version = "{version}"')
    require_contains(root / "packages/python/rhei-api/pyproject.toml", f'version = "{version}"')
    require_contains(root / "packages/python/rhei-api/pyproject.toml", f'"rhei-cli=={version}"')
    require_contains(root / "packages/python/rhei-cli/src/rhei_cli/__init__.py", f'__version__ = "{version}"')
    require_contains(root / "packages/python/rhei-cli/src/rhei_cli/__init__.py", f'_CRATE_VERSION = "{version}"')
    require_contains(root / "packages/python/rhei-api/src/rhei_api/__init__.py", f'__version__ = "{version}"')


def main(argv: Sequence[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    if len(args) != 1:
        print("usage: scripts/verify-release-version.py <version>", file=sys.stderr)
        return 2
    try:
        verify(Path.cwd(), args[0])
    except RuntimeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
