#!/usr/bin/env bash
set -euo pipefail

VERSION="0.1.0"
PY_VERSION="0.1.0"
TAG="v${VERSION}"
MODE="dry-run"
SKIP_TESTS=0
PUSH_GIT_TAG=0
CREATE_GITHUB_RELEASE=0

usage() {
  cat <<EOF
Usage: scripts/release-all.sh [OPTIONS]

Release Rhei ${VERSION} to crates.io, npm, PyPI, and optionally GitHub.

Default mode is a non-publishing dry run.

Options:
  --publish                 Perform real publishes. Requires logged-in registries.
  --skip-tests              Skip cargo test preflight.
  --push-git-tag            Create and push ${TAG}.
  --github-release          Create GitHub release with gh. Implies --push-git-tag.
  -h, --help                Show this help.

Prerequisites for --publish:
  cargo login <crates-token>
  npm login
  python3 -m pip install --user build twine
  twine configured with a PyPI token

Optional for --github-release:
  gh auth login
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --publish)
      MODE="publish"
      ;;
    --skip-tests)
      SKIP_TESTS=1
      ;;
    --push-git-tag)
      PUSH_GIT_TAG=1
      ;;
    --github-release)
      PUSH_GIT_TAG=1
      CREATE_GITHUB_RELEASE=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

run() {
  printf '\n+'
  printf ' %q' "$@"
  printf '\n'
  "$@"
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

confirm_publish() {
  if [[ "$MODE" != "publish" ]]; then
    return
  fi

  cat <<EOF

This will publish immutable package versions:
  crates.io: rhei-api, rhei-cli-tui, rhei-cli-output, rhei-cli-validator, rhei-api-napi, rhei-cli
  npm:       rhei, rhei-api
  PyPI:      rhei-cli, rhei-api

EOF
  read -r -p "Type ${VERSION} to publish: " answer
  if [[ "$answer" != "$VERSION" ]]; then
    echo "publish aborted" >&2
    exit 1
  fi
}

check_npm_names() {
  local cli_name api_dep
  cli_name="$(node -p 'require("./packages/npm/rhei-cli/package.json").name')"
  api_dep="$(node -p 'require("./packages/npm/rhei-api/package.json").dependencies.rhei || ""')"
  if [[ "$cli_name" != "rhei" ]]; then
    echo "expected npm CLI package name to be 'rhei', got '$cli_name'" >&2
    exit 1
  fi
  if [[ "$api_dep" != "$VERSION" ]]; then
    echo "expected npm rhei-api dependency on rhei@$VERSION, got '$api_dep'" >&2
    exit 1
  fi
}

check_readme_install_docs() {
  local required=(
    "cargo install rhei-cli --locked"
    "npm install -g rhei"
    "npm install rhei-api"
    "python3 -m pip install rhei-cli"
    "python3 -m pip install rhei-api"
    "import rhei_api"
    "require(\"rhei-api\")"
  )
  local needle
  for needle in "${required[@]}"; do
    if ! grep -Fq "$needle" README.md; then
      echo "README.md is missing install/API snippet: $needle" >&2
      exit 1
    fi
  done
}

preflight() {
  require_cmd cargo
  require_cmd npm
  require_cmd node
  require_cmd python3
  require_cmd twine

  check_npm_names
  check_readme_install_docs

  if [[ "$SKIP_TESTS" -eq 0 ]]; then
    run cargo test --workspace --no-fail-fast
  fi

  run cargo publish --dry-run -p rhei-api
  run cargo publish --dry-run -p rhei-cli-tui

  run npm --prefix packages/npm/rhei-cli pack --dry-run
  run npm --prefix packages/npm/rhei-api pack --dry-run

  rm -rf packages/python/rhei-cli/dist packages/python/rhei-cli/build packages/python/rhei-cli/src/rhei_cli.egg-info
  rm -rf packages/python/rhei-api/dist packages/python/rhei-api/build packages/python/rhei-api/src/rhei_api.egg-info
  run python3 -m build packages/python/rhei-cli
  run python3 -m build packages/python/rhei-api
  run twine check packages/python/rhei-cli/dist/*
  run twine check packages/python/rhei-api/dist/*
}

publish_crates() {
  run cargo publish -p rhei-api
  run cargo publish -p rhei-cli-tui

  echo "Waiting for crates.io index propagation..."
  sleep 90

  run cargo publish --dry-run -p rhei-cli-output
  run cargo publish -p rhei-cli-output

  run cargo publish --dry-run -p rhei-cli-validator
  run cargo publish -p rhei-cli-validator

  run cargo publish --dry-run -p rhei-api-napi
  run cargo publish -p rhei-api-napi

  echo "Waiting for crates.io index propagation..."
  sleep 90

  run cargo publish --dry-run -p rhei-cli
  run cargo publish -p rhei-cli
}

publish_npm() {
  run npm --prefix packages/npm/rhei-cli publish --access public
  run npm --prefix packages/npm/rhei-api publish --access public
}

publish_pypi() {
  run twine upload packages/python/rhei-cli/dist/*
  run twine upload packages/python/rhei-api/dist/*
}

tag_and_release() {
  if [[ "$PUSH_GIT_TAG" -eq 0 ]]; then
    return
  fi

  if git rev-parse "$TAG" >/dev/null 2>&1; then
    echo "tag ${TAG} already exists locally"
  else
    run git tag -a "$TAG" -m "Rhei ${VERSION}"
  fi
  run git push origin "$TAG"

  if [[ "$CREATE_GITHUB_RELEASE" -eq 1 ]]; then
    require_cmd gh
    if gh release view "$TAG" >/dev/null 2>&1; then
      echo "GitHub release ${TAG} already exists"
    else
      run gh release create "$TAG" --title "Rhei ${VERSION}" --notes-file CHANGELOG.md
    fi
  fi
}

smoke_tests() {
  run cargo install rhei-cli --locked --force
  run rhei version

  run npm install -g rhei
  run rhei version

  run python3 -m pip install --upgrade rhei-cli
  run rhei version
}

preflight

if [[ "$MODE" != "publish" ]]; then
  cat <<EOF

Dry run complete. Nothing was published.
Run this to publish all configured platforms:

  scripts/release-all.sh --publish --push-git-tag

Or, if gh is authenticated and you want the GitHub release too:

  scripts/release-all.sh --publish --github-release
EOF
  exit 0
fi

confirm_publish
publish_crates
publish_npm
publish_pypi
tag_and_release
smoke_tests

echo "Rhei ${VERSION} release complete."
