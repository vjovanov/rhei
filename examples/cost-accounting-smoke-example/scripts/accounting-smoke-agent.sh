#!/usr/bin/env bash
set -eu

state="${RHEI_STATE:-unknown}"
capture="${RHEI_ACCOUNTING_USAGE_PATH:-}"
schema="${RHEI_ACCOUNTING_USAGE_SCHEMA:-rhei.accounting.usage.v1}"

case "$state" in
  collect)
    input=24000
    output=2200
    cached=8000
    delay=4
    ;;
  verify)
    input=18000
    output=1400
    cached=3000
    delay=3
    ;;
  *)
    input=0
    output=0
    cached=0
    delay=1
    ;;
esac

if [ -z "$capture" ]; then
  echo "RHEI_ACCOUNTING_USAGE_PATH is not set" >&2
  exit 1
fi

echo "accounting smoke state=$state starting; sleeping ${delay}s before usage emission"
sleep "$delay"

mkdir -p "$(dirname "$capture")"
printf '{"schema":"%s","usage":{"input_tokens":%s,"output_tokens":%s,"cached_input_tokens":%s}}\n' \
  "$schema" "$input" "$output" "$cached" >> "$capture"

echo "accounting smoke state=$state input=$input output=$output cached_input=$cached"
