#!/usr/bin/env bash
#
# Asserts that a packaged plugin actually loaded and ran inside a live BTCPay.
#
#   ./dev/assert-plugin-loaded.sh BTCPayServer.Plugins.Hello "Hello"
#
# Used by CI and usable by hand. Checks the things that have actually broken before: the
# plugin being skipped, the native library not resolving, and an ABI or identity mismatch.
set -euo pipefail
cd "$(dirname "$0")/.."

IDENTIFIER="${1:?usage: assert-plugin-loaded.sh <identifier> [display-name]}"
DISPLAY_NAME="${2:-}"
COMPOSE="docker compose -f dev/docker-compose.yml"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-300}"

SNAPSHOT="$(mktemp)"
trap 'rm -f "$SNAPSHOT"' EXIT

# Logs are snapshotted to a file rather than piped into grep. Under `set -o pipefail`,
# `grep -q` exits at its first match and the producer takes SIGPIPE, so the pipeline reports
# failure even though the pattern was found. That failure mode is invisible: every check
# would time out and report that the plugin never loaded.
snapshot() {
  $COMPOSE logs btcpayserver 2>&1 | sed 's/\x1b\[[0-9;]*m//g' > "$SNAPSHOT"
}

fail() {
  echo "FAIL: $1" >&2
  shift
  for line in "$@"; do echo "$line" >&2; done
  echo "--- last 40 log lines ---" >&2
  tail -40 "$SNAPSHOT" >&2
  exit 1
}

# Waits for `pattern` to appear, failing fast if BTCPay reports an error that means it never
# will.
wait_for() {
  local pattern="$1" description="$2"
  local deadline=$(( $(date +%s) + TIMEOUT_SECONDS ))

  while true; do
    snapshot
    if grep -q "$pattern" "$SNAPSHOT"; then
      return 0
    fi
    if grep -qE "Error when loading plugin $IDENTIFIER|ABI mismatch|identity mismatch|DllNotFoundException" "$SNAPSHOT"; then
      fail "BTCPay reported an error loading $IDENTIFIER" \
           "$(grep -E 'Error when loading plugin|ABI mismatch|identity mismatch|DllNotFound' "$SNAPSHOT" | tail -3)"
    fi
    if [ "$(date +%s)" -ge "$deadline" ]; then
      fail "$description did not happen within ${TIMEOUT_SECONDS}s"
    fi
    sleep 5
  done
}

echo "==> waiting for BTCPay to load $IDENTIFIER (up to ${TIMEOUT_SECONDS}s)"
wait_for "Running plugin $IDENTIFIER" "BTCPay loading $IDENTIFIER"
echo "  ok: PluginManager loaded the plugin"

# The plugin's hosted service starts after the database migration, so it can lag the load.
echo "==> waiting for the Rust plugin to start"
wait_for "\[$IDENTIFIER\] started" "the plugin runtime starting"

started_line="$(grep -E "\[$IDENTIFIER\] started" "$SNAPSHOT" | tail -1)"
echo "  ok:${started_line#*info:}"

# The ABI is reported by the native library, so seeing it proves the .so resolved and was
# called, not merely that the C# assembly loaded.
if ! echo "$started_line" | grep -qE "ABI [0-9]+"; then
  fail "no ABI in the startup line, so the native library may not have been called"
fi
echo "  ok: the native library resolved and reported its ABI"

if [ -n "$DISPLAY_NAME" ] && ! echo "$started_line" | grep -q "$DISPLAY_NAME"; then
  fail "expected the display name '$DISPLAY_NAME' in: $started_line"
fi

echo
echo "PASS: $IDENTIFIER is running inside BTCPay"
