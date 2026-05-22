#!/usr/bin/env bash
set -euo pipefail

# Pre-release package-name guard. Claimed package names must be available or
# already owned by this repository before a release is allowed to publish.
# §FS-rhei-distribution.1

ua="rhei-release-name-check/0.1"
repo_pattern='github.com[/:]vjovanov/rhei'
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

http_get() {
  local url="$1"
  local out="$2"
  curl -sS -L -A "$ua" -o "$out" -w '%{http_code}' "$url"
}

metadata_mentions_repo() {
  local file="$1"
  python3 - "$file" "$repo_pattern" <<'PY'
import json
import re
import sys

path, pattern = sys.argv[1], sys.argv[2]
with open(path, "r", encoding="utf-8") as fh:
    data = json.load(fh)

haystack = json.dumps(data, sort_keys=True).lower()
sys.exit(0 if re.search(pattern, haystack) else 1)
PY
}

check_claimed_json_name() {
  local registry="$1"
  local name="$2"
  local url="$3"
  local out="$tmpdir/${registry}-${name}.json"
  local code

  code="$(http_get "$url" "$out")"
  case "$code" in
    200)
      if metadata_mentions_repo "$out"; then
        echo "ok: $registry/$name is owned by this project"
      else
        echo "error: $registry/$name is already taken by another project" >&2
        echo "       $url" >&2
        return 1
      fi
      ;;
    404)
      echo "ok: $registry/$name is available"
      ;;
    *)
      echo "error: could not query $registry/$name (HTTP $code)" >&2
      echo "       $url" >&2
      return 1
      ;;
  esac
}

notice_external_json_name() {
  local registry="$1"
  local name="$2"
  local url="$3"
  local out="$tmpdir/${registry}-${name}-external.json"
  local code

  code="$(http_get "$url" "$out")"
  case "$code" in
    200)
      if metadata_mentions_repo "$out"; then
        echo "notice: $registry/$name is owned by this project"
      else
        echo "notice: $registry/$name is occupied by an external package as documented"
      fi
      ;;
    404)
      echo "notice: $registry/$name appears available; revisit the alternate-name rationale before publishing"
      ;;
    *)
      echo "warning: could not query documented external collision $registry/$name (HTTP $code)" >&2
      ;;
  esac
}

check_claimed_json_name "crates.io" "rhei-plan-core" "https://crates.io/api/v1/crates/rhei-plan-core"
check_claimed_json_name "crates.io" "rhei-cli-tui" "https://crates.io/api/v1/crates/rhei-cli-tui"
check_claimed_json_name "crates.io" "rhei-agent-core" "https://crates.io/api/v1/crates/rhei-agent-core"
check_claimed_json_name "crates.io" "rhei-cli-output" "https://crates.io/api/v1/crates/rhei-cli-output"
check_claimed_json_name "crates.io" "rhei-cli-validator" "https://crates.io/api/v1/crates/rhei-cli-validator"
check_claimed_json_name "crates.io" "rhei-api" "https://crates.io/api/v1/crates/rhei-api"
check_claimed_json_name "crates.io" "rhei-cli" "https://crates.io/api/v1/crates/rhei-cli"
check_claimed_json_name "npm" "rhei" "https://registry.npmjs.org/rhei"
check_claimed_json_name "npm" "rhei-api" "https://registry.npmjs.org/rhei-api"
check_claimed_json_name "pypi" "rhei-cli" "https://pypi.org/pypi/rhei-cli/json"
check_claimed_json_name "pypi" "rhei-api" "https://pypi.org/pypi/rhei-api/json"

notice_external_json_name "pypi" "rhei" "https://pypi.org/pypi/rhei/json"
