#!/usr/bin/env bash
# Regenerates the C# bindings from the Rust contract. Run after changing anything exported
# from btcpay-plugin, then commit the result.
#
# The output is identical for every plugin: the namespace is fixed by setup_scaffolding!,
# and the DllImport name is fixed because every btcpay-rs plugin builds its cdylib as
# `btcpay_plugin_native`. That is what lets these bindings ship inside BtcpayRs.Host
# instead of being regenerated per plugin.
set -euo pipefail
cd "$(dirname "$0")/.."

# NOTE: the library must NOT be stripped. uniffi-bindgen-cs reads metadata from its symbols
# and, when stripped, exits 0 having written nothing at all.
cargo build --release -p hello-plugin
uniffi-bindgen-cs \
  --library target/release/libbtcpay_plugin_native.so \
  --config dotnet/uniffi.toml \
  --out-dir dotnet/BtcpayRs.Host/Generated

echo "regenerated dotnet/BtcpayRs.Host/Generated/btcpay.cs"
