#!/usr/bin/env bash
set -euo pipefail

# Block generated-by attribution boilerplate from landing in committed files.
# Normal prose mentions are fine; this targets narrow trailer/marker patterns.

if [ "$#" -eq 0 ]; then
  exit 0
fi

pattern='Co-Authored-By: *Claude|Generated with \[?Claude Code\]?|Claude <noreply@anthropic\.com>'

if matches="$(grep -InE "$pattern" -- "$@" 2>/dev/null)"; then
  printf 'AI attribution boilerplate found in staged files:\n%s\n' "$matches" >&2
  printf '\nRemove the line(s) above before committing.\n' >&2
  exit 1
fi

exit 0
