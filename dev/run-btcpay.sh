#!/usr/bin/env bash
#
# Installs the packed plugin into a local BTCPay regtest and starts it.
#
#   ./dev/run-btcpay.sh [path/to/plugin.btcpay]
#
# Installs by extracting into the plugin directory, which is what BTCPay's own installer does
# after an admin uploads a .btcpay. That skips creating an account just to click Upload.
set -euo pipefail
cd "$(dirname "$0")"

BTCPAY="${1:-../artifacts/BTCPayServer.Plugins.Hello/0.1.0.0/BTCPayServer.Plugins.Hello.btcpay}"
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

echo
echo "BTCPay will be at http://localhost:14142 once it finishes starting."
echo "Follow the plugin loading with:"
echo "  docker compose -f dev/docker-compose.yml logs -f btcpayserver | grep -i hello"
