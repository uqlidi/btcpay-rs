# Changelog

Notable changes to btcpay-rs. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Two version numbers matter here and they are not the same thing:

- The **crate version**, which this file tracks.
- The **ABI version**, which the host checks when it loads a plugin. A plugin built against a
  different ABI is refused rather than crashing. Any change to it is called out below, because it
  means every plugin must be rebuilt.

## [Unreleased]

Nothing is published yet, so everything below is the content of the first release rather than a
list of changes since one.

### Added

- **The plugin contract** (`btcpay-plugin`). A plugin implements one trait and applies
  `#[btcpay_plugin::plugin]`; `metadata()` is generated from the attribute plus `Cargo.toml`.
  Every other method has a default, so a working plugin is a few lines. All uniffi machinery lives
  in this crate, so plugin crates contain no FFI boilerplate and no `unsafe`.
- **A generated C# host.** `cargo-btcpay` materialises the shim that satisfies BTCPay's plugin
  contract and delegates across the boundary. Nothing in a plugin project is C#.
- **Declarative UI** (`btcpay-ui`). A plugin describes a settings page or dashboard as data and the
  host renders it in BTCPay's own styling. Pages cross the boundary as JSON, so a new kind of
  element is something an older host degrades over rather than an ABI break.
- **`#[derive(BtcpaySettings)]`** generates the form, the storage keys, loading, saving and
  validation from one struct, so they cannot drift apart. `#[derive(BtcpayChoice)]` renders a
  unit-only enum as a dropdown whose options come from the type.
- **Several pages per plugin, and commands.** `Actions` sections carry buttons, and a press arrives
  as `HostEvent::CommandInvoked`. Confirmation is declared by the plugin and enforced by the host,
  which reads the command from the rebuilt page rather than from the request, so a crafted post
  cannot invent a command or skip a confirmation.
- **Forms on pages other than the settings page**, delivered as `HostEvent::FormSubmitted` with the
  form's id. A page can ask the operator for input without that input being mistaken for
  configuration.
- **A plugin runtime**: a private data directory, a per-plugin key/value store, a bounded shutdown
  that cannot be blocked by plugin code, and a native dependency check at build time.
- **`cargo btcpay`**: `new`, `build`, `package`, `shim` and `inspect`. `--docker` runs the whole
  pipeline in a pinned image, so only Docker is needed rather than Rust and the .NET SDK together.
- **A regtest dev stack** under `dev/`, and `examples/hello-plugin`, which doubles as the host's
  integration test.
- **ABI version 4.**

### Guarantees worth stating

- A panic in plugin code never unwinds into .NET. It becomes an error naming the method that
  panicked.
- An exception escaping a host callback is contained and logged. Without that it would become a
  Rust panic that silently killed the thread it was called on, and the plugin would go quiet.
- A stored secret is never sent to the browser, and a save that leaves a secret field blank keeps
  the stored value rather than clearing it.

### Known limitations

`linux-x64` only. No hot reload, because BTCPay never unloads a plugin's load context. Commands run
inside the operator's request, so there is no place to run work that takes minutes and no deadline
on a page render. Pages do not update themselves. Storage is key/value plus a data directory, with
no schema or migrations. Settings are server-wide, with no per-store scope. Payment method plugins
are out of scope.

### Not yet published

The crates are not on crates.io, so `cargo btcpay new` emits a `btcpay-plugin = "0.1"` dependency
that does not resolve. Building against a checkout of this repository is the only route until the
first release.

[Unreleased]: https://github.com/uqlidi/btcpay-rs/commits/master
