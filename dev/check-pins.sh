#!/usr/bin/env bash
#
# Checks the version numbers that are duplicated across the repository and must agree.
#
#   ./dev/check-pins.sh
#
# Two pairs matter, and both fail in ways that point nowhere near the cause:
#
#   uniffi <-> uniffi-bindgen-cs
#     The generator targets one specific uniffi release. A mismatch produces bindings that
#     compile and then disagree with the library's actual layout at runtime.
#
#   ABI_VERSION (Rust) <-> SupportedAbi (C#)
#     The host refuses to load a plugin whose ABI it does not recognise. If these drift,
#     every plugin stops loading.
set -euo pipefail
cd "$(dirname "$0")/.."

failures=0

report() {
	echo "FAIL: $1" >&2
	failures=$((failures + 1))
}

# --- the uniffi pair ------------------------------------------------------------------
# The tag looks like "v0.11.0+v0.31.0": the generator version, then the uniffi release it
# targets. Every copy must be identical, and the suffix must match the uniffi dependency.

# Workflows are discovered rather than listed, so a new one that pins the tag is checked
# without anyone remembering to add it here.
declare -A tags=(
	["crates/cargo-btcpay/src/workspace.rs"]='UNIFFI_BINDGEN_CS_TAG: &str = "\K[^"]+'
	["docker/build.Dockerfile"]='ARG UNIFFI_BINDGEN_CS_TAG=\K\S+'
)
for workflow in .github/workflows/*.yml; do
	if grep -q 'UNIFFI_BINDGEN_CS_TAG:' "$workflow"; then
		tags["$workflow"]='UNIFFI_BINDGEN_CS_TAG: \K\S+'
	fi
done

canonical=""
for file in "${!tags[@]}"; do
	if [ ! -f "$file" ]; then
		report "$file is missing, so its uniffi-bindgen-cs pin cannot be checked"
		continue
	fi

	found="$(grep -oP "${tags[$file]}" "$file" | head -1 || true)"
	if [ -z "$found" ]; then
		report "$file does not pin uniffi-bindgen-cs where expected"
		continue
	fi

	if [ -z "$canonical" ]; then
		canonical="$found"
		echo "  uniffi-bindgen-cs: $canonical ($file)"
	elif [ "$found" != "$canonical" ]; then
		report "$file pins uniffi-bindgen-cs $found, but $canonical is used elsewhere"
	else
		echo "  uniffi-bindgen-cs: $found ($file)"
	fi
done

# The uniffi crate must be the release the generator targets.
uniffi_version="$(grep -oP '^uniffi = "\K[^"]+' Cargo.toml | head -1 || true)"
if [ -z "$uniffi_version" ]; then
	report "Cargo.toml does not pin the uniffi crate where expected"
elif [ -n "$canonical" ]; then
	targeted="${canonical#*+v}"
	if [ "$uniffi_version" != "$targeted" ]; then
		report "the uniffi crate is $uniffi_version but uniffi-bindgen-cs $canonical targets $targeted"
	else
		echo "  uniffi crate:      $uniffi_version, which $canonical targets"
	fi
fi

# --- the ABI version ------------------------------------------------------------------

rust_abi="$(grep -oP 'ABI_VERSION: u32 = \K[0-9]+' crates/btcpay-plugin/src/handle.rs | head -1 || true)"
csharp_abi="$(grep -oP 'SupportedAbi = \K[0-9]+' dotnet/BtcpayRs.Host/NativeLoader.cs | head -1 || true)"

if [ -z "$rust_abi" ]; then
	report "crates/btcpay-plugin/src/handle.rs does not define ABI_VERSION where expected"
elif [ -z "$csharp_abi" ]; then
	report "dotnet/BtcpayRs.Host/NativeLoader.cs does not define SupportedAbi where expected"
elif [ "$rust_abi" != "$csharp_abi" ]; then
	report "Rust reports ABI $rust_abi but the host supports ABI $csharp_abi; every plugin would fail to load"
else
	echo "  ABI version:       $rust_abi, agreed by Rust and the host"
fi

echo
if [ "$failures" -gt 0 ]; then
	echo "$failures pin(s) disagree" >&2
	exit 1
fi
echo "PASS: all duplicated version pins agree"
