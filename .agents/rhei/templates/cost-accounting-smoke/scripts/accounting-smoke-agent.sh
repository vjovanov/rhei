#!/usr/bin/env bash
set -eu

state="${RHEI_STATE:-unknown}"
capture="${RHEI_ACCOUNTING_USAGE_PATH:-}"
schema="${RHEI_ACCOUNTING_USAGE_SCHEMA:-rhei.accounting.usage.v1}"

case "$state" in
  collect)
    input={{collect_input_tokens}}
    output={{collect_output_tokens}}
    cached={{collect_cached_input_tokens}}
    delay={{collect_delay_seconds}}
    ;;
  verify)
    input={{verify_input_tokens}}
    output={{verify_output_tokens}}
    cached={{verify_cached_input_tokens}}
    delay={{verify_delay_seconds}}
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
