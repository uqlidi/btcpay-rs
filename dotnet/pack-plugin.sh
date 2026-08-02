#!/usr/bin/env bash
#
# Builds a btcpay-rs plugin end to end and packs it into an installable .btcpay.
#
#   ./dotnet/pack-plugin.sh <plugin-dotnet-dir> <assembly-name> [output-dir]
#
# e.g. ./dotnet/pack-plugin.sh examples/hello-plugin/dotnet BTCPayServer.Plugins.Hello
#
# This is the manual form of what `cargo btcpay package` will do in the build-pipeline
# milestone. It exists now so the chain is verifiable end to end.
#
# Requires BTCPAYSERVER to point at a checkout of btcpayserver (the C# side compiles against
# its source; BTCPayServer.Abstractions is not published on NuGet).
set -euo pipefail

PLUGIN_DIR="${1:?usage: pack-plugin.sh <plugin-dotnet-dir> <assembly-name> [output-dir]}"
ASSEMBLY="${2:?missing assembly name}"
OUT_DIR="${3:-artifacts}"
BTCPAYSERVER="${BTCPAYSERVER:?set BTCPAYSERVER to a btcpayserver checkout}"

cd "$(dirname "$0")/.."
REPO="$PWD"

echo "==> 1/4 building the Rust library"
# Must not be stripped: uniffi-bindgen-cs reads metadata from its symbols, and on a stripped
# library it exits 0 having written nothing.
cargo build --release -p hello-plugin

echo "==> 2/4 regenerating C# bindings"
./dotnet/regen-bindings.sh >/dev/null

echo "==> 3/4 publishing the plugin assembly"
PUBLISH_DIR="$REPO/target/plugin-publish/$ASSEMBLY"
rm -rf "$PUBLISH_DIR"
dotnet publish "$PLUGIN_DIR" \
  -c Release \
  -o "$PUBLISH_DIR" \
  -p:BtcpayServerProject="$BTCPAYSERVER/BTCPayServer/BTCPayServer.csproj"

# A plugin missing one of these still builds, packs, and installs; it only fails when BTCPay
# tries to start it. Checking here turns a runtime failure into a build failure.
for required in libbtcpay_plugin_native.so BtcpayRs.Host.dll BtcpayRs.Host.BTCPay.dll; do
  if [ ! -f "$PUBLISH_DIR/$required" ]; then
    echo "error: '$required' is missing from the published output." >&2
    echo "       The plugin would install but fail to start." >&2
    if [ "$required" != "libbtcpay_plugin_native.so" ]; then
      echo "       Add an explicit <ProjectReference> with <Private>true</Private> for it:" >&2
      echo "       the ItemDefinitionGroup that keeps BTCPayServer's assemblies out of the" >&2
      echo "       package also suppresses transitive references." >&2
    fi
    exit 1
  fi
done

echo "==> 4/4 packing .btcpay"
# BTCPay's own packer: it instantiates the plugin type to write the metadata sidecar, then
# zips the publish directory verbatim.
mkdir -p "$REPO/$OUT_DIR"
dotnet run --project "$BTCPAYSERVER/BTCPayServer.PluginPacker" -c Release -- \
  "$PUBLISH_DIR" "$ASSEMBLY" "$REPO/$OUT_DIR"

echo
echo "packed:"
find "$REPO/$OUT_DIR/$ASSEMBLY" -type f | sed "s|$REPO/||"
