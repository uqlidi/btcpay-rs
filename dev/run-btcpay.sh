#!/usr/bin/env bash
#
# Installs the packed plugin into a local BTCPay regtest and starts it.
#
#   ./dev/run-btcpay.sh [path/to/plugin.btcpay | path/to/artifacts/<Identifier>]
#
# Installs by extracting into the plugin directory, which is what BTCPay's own installer does
# after an admin uploads a .btcpay. That skips creating an account just to click Upload.
set -euo pipefail
cd "$(dirname "$0")"

# Accepts a .btcpay file, or a directory to take the newest .btcpay from. A directory is the
# safer thing to pass: the packer puts each build under a version-named subdirectory, and that
# name comes from the plugin's version rather than from anything the caller controls, so a
# hardcoded path silently installs an older build the moment the shape of that name changes.
BTCPAY="${1:-../artifacts/BTCPayServer.Plugins.Hello}"

if [ -d "$BTCPAY" ]; then
  NEWEST="$(find "$BTCPAY" -name '*.btcpay' -type f -printf '%T@ %p\n' 2>/dev/null \
    | sort -rn | head -1 | cut -d' ' -f2-)"
  if [ -z "$NEWEST" ]; then
    echo "error: no .btcpay found under $BTCPAY. Pack it first with: cargo btcpay package" >&2
    exit 1
  fi
  BTCPAY="$NEWEST"
  echo "==> using the newest package: $BTCPAY"
fi

IDENTIFIER="$(basename "$BTCPAY" .btcpay)"

if [ ! -f "$BTCPAY" ]; then
  echo "error: $BTCPAY not found. Pack it first with: cargo btcpay package" >&2
  exit 1
fi

# BTCPay records a plugin that crashed at startup in a `disabled` file and skips it on every
# later boot. Reinstalling does not clear it, so without this a single bad build looks like a
# plugin that silently stopped existing.
if [ -f plugins/disabled ] && grep -q "$IDENTIFIER" plugins/disabled 2>/dev/null; then
  echo "==> $IDENTIFIER was disabled after a crash; re-enabling"
  grep -v "$IDENTIFIER" plugins/disabled > plugins/disabled.tmp 2>/dev/null || true
  mv plugins/disabled.tmp plugins/disabled
fi

echo "==> installing $IDENTIFIER"
rm -rf "plugins/$IDENTIFIER"
mkdir -p "plugins/$IDENTIFIER"
unzip -q "$BTCPAY" -d "plugins/$IDENTIFIER"

# PluginManager looks for <identifier>.dll inside a directory of the same name; without it the
# directory is skipped silently.
if [ ! -f "plugins/$IDENTIFIER/$IDENTIFIER.dll" ]; then
  echo "error: $IDENTIFIER.dll missing from the package; BTCPay would skip it." >&2
  exit 1
fi
ls "plugins/$IDENTIFIER"

echo
echo "==> starting BTCPay regtest (first run pulls images and builds the database)"
docker compose up -d

# `up -d` leaves an already-running container alone, and extracting a plugin does not change any
# compose config, so on a reinstall BTCPay would keep running with the previous copy loaded. That
# looks exactly like a plugin that installed fine but whose pages are missing. Plugins are only
# read at startup, so the restart is what makes the install take effect.
echo
echo "==> restarting BTCPay so it loads $IDENTIFIER"
docker compose restart btcpayserver

# Tor borrows btcpayserver's network namespace, so restarting BTCPay takes it down with it. Its
# restart policy would eventually bring it back, but doing it here means the maker does not spend
# its first minutes unable to reach a control port.
echo "==> restarting tor, whose network namespace went with it"
docker compose restart tor

echo
echo "BTCPay will be at http://localhost:14142 once it finishes starting."
echo "Follow the plugin loading with:"
echo "  docker compose -f dev/docker-compose.yml logs -f btcpayserver | grep -i ${IDENTIFIER##*.}"
