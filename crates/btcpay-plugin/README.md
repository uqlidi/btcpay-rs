# btcpay-plugin

The contract a [BTCPay Server](https://btcpayserver.org) plugin written in Rust implements.

BTCPay plugins are .NET assemblies. This crate lets you write one in Rust instead: your crate
holds the logic, and the C# that BTCPay actually loads is generated at build time by
[`cargo-btcpay`](https://crates.io/crates/cargo-btcpay). There is no C# in your project to
maintain, and no `unsafe` or FFI boilerplate to write.

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

Settings come from a struct, so the form, the storage keys, the parsing and the validation are
one declaration rather than four that can disagree:

```rust
use btcpay_plugin::prelude::*;

#[derive(Default, BtcpaySettings)]
struct Settings {
    #[setting(label = "Greeting", help = "Logged when the plugin starts.", required)]
    greeting: String,

    #[setting(label = "API key", secret)]
    api_key: String,
}
```

A `secret` field is never sent back to the browser, and a save that leaves it blank keeps the
stored value.

## Getting started

```sh
cargo install cargo-btcpay
cargo btcpay new my-plugin
```

Requires Rust 1.88 or newer, and the .NET SDK 10.0 to build the generated C#. `cargo btcpay
package --docker` runs the whole pipeline in a pinned image instead, so only Docker is needed
locally.

See the [workspace README](https://github.com/uqlidi/btcpay-rs) for how it works, what an
operator ends up running, and the current limitations.

## License

MIT or Apache-2.0, at your option.
