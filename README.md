<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/btcpay-rs-logo-dark.svg">
  <img src="assets/btcpay-rs-logo.svg" alt="btcpay-rs" width="440">
</picture>

<h2>Write BTCPay Server plugins in Rust.</h2>

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

</div>

---

BTCPay Server plugins are .NET assemblies. btcpay-rs lets you write one in Rust instead: your
crate holds the logic, and the C# that BTCPay actually loads is generated at build time. There is
no C# in your project to maintain, and no `unsafe` or FFI boilerplate to write.

```rust
use btcpay_plugin::prelude::*;

#[derive(Default)]
struct HelloPlugin;

#[btcpay_plugin::plugin(identifier = "Acme.Plugins.Hello")]
impl Plugin for HelloPlugin {}
```

That is a complete, installable plugin. `metadata()` is generated from the attribute plus
`Cargo.toml`, so the name and version cannot drift from the crate that built it, and every other
trait method has a default.

Settings come from a struct, so the form, the storage keys, the parsing and the validation are one
declaration rather than four that can disagree:

```rust
#[derive(Default, BtcpaySettings)]
struct Settings {
    #[setting(label = "Greeting", help = "Logged when the plugin starts.", required)]
    greeting: String,

    #[setting(label = "Times to log it", min = 1, max = 10)]
    repeat: u32,

    #[setting(label = "API key", secret)]
    api_key: String,
}
```

The host renders that in BTCPay's own styling. A `secret` field is never sent back to the browser,
and a save that leaves it blank keeps the stored value.

## Status

**Early, and not yet published.** The crates are not on crates.io, so `cargo btcpay new` emits a
`btcpay-plugin = "0.1"` dependency that will not resolve until they are. Until then, build against
a checkout of this repository.

The framework is exercised by two plugins: `examples/hello-plugin` here, and a real
[coinswap](https://github.com/citadel-tech/coinswap) maker and taker that runs a long-lived
service, holds its own wallets, and links a C library. Everything in this README is in use by one
of them.

## Requirements

| | |
|---|---|
| Rust | 1.88 or newer |
| .NET SDK | 10.0, to build the generated C# |
| BTCPay Server | 2.4.x |
| Target | `linux-x64` |

Those two toolchains together are the awkward part, so `--docker` runs the whole pipeline in a
pinned image and needs only Docker.

The first build fetches a BTCPay Server checkout, because `BTCPayServer.Abstractions` is not
published on NuGet. It is cached, so only the first build pays for it.

## Usage

```sh
cargo btcpay new my-plugin        # a project with no C# in it
cd my-plugin
cargo test                        # your plugin is a normal Rust crate
cargo btcpay package              # produce an installable .btcpay
cargo btcpay package --docker     # the same, with only Docker installed
```

Then upload the file from `artifacts/` in BTCPay under Server Settings, Plugins, and restart.
Plugins are read once at startup, so a restart is required after installing or upgrading.

`cargo btcpay inspect <library>` prints what a compiled plugin reports about itself, which is the
quickest way to see why BTCPay refused one.

## How it works

- **The C# shim is generated**, per plugin, from your crate's metadata. It satisfies BTCPay's
  plugin contract and delegates across the FFI boundary via
  [UniFFI](https://mozilla.github.io/uniffi-rs/).
- **The native library sits flat beside the plugin assembly.** BTCPay resolves natives from
  `deps.json`, which a copied file never populates, so the conventional `runtimes/{rid}/native/`
  layout does not work.
- **Pages cross the boundary as JSON, not as generated types.** A plugin describes a page as data
  and the host renders it, so a new kind of page element is something an older host degrades over
  rather than an ABI break.
- **An ABI version is checked at load.** A plugin built against a different contract is refused
  with a message saying so, rather than crashing.
- **A panic in plugin code never unwinds into .NET.** It becomes an error naming the method that
  panicked.

## What you are running

A plugin loads **into BTCPay's own process**: same privileges, same fate on a crash, same blast
radius on an exploit. A Rust plugin is exactly as trusted as a C# one, and being written in Rust
does not change that. Install third-party plugins on that basis.

There is no hot reload. BTCPay loads each plugin into a load context it never unloads, so
upgrading means restarting the server.

## Limitations

- **`linux-x64` only.** Nothing here is portable to other runtime identifiers yet.
- **Commands run inside the operator's request.** Work that takes minutes needs a background
  thread of the plugin's own; there is no job abstraction, and no deadline on a page render.
- **Pages do not update themselves.** A page reflects the moment it was loaded.
- **Storage is key/value plus a data directory.** No schema, no migrations, no EF Core.
- **Settings are server-wide.** There is no per-store scope.
- **Payment method plugins are out of scope.**

## Examples

[`examples/hello-plugin`](examples/hello-plugin) is the smallest useful plugin: it stores
settings, reacts to invoice events, renders a dashboard with buttons and a form, and doubles as
the integration test for the host.

## Development

```sh
cargo test --workspace        # the Rust side
./dev/run-btcpay.sh           # a regtest BTCPay with the example installed
```

The C# tests need the .NET SDK, or Docker via the pinned build image. `dev/` holds a small
compose stack: postgres, bitcoind, NBXplorer and BTCPay, plus Tor and a custom signet node used by
the coinswap plugin.

## License

Dual licensed under either of

- MIT ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option, matching the Rust ecosystem norm.
