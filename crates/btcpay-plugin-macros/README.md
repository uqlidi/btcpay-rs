# btcpay-plugin-macros

Procedural macros for [`btcpay-plugin`](https://crates.io/crates/btcpay-plugin).

You do not normally depend on this crate directly. `btcpay-plugin` re-exports everything here,
and its prelude is what a plugin imports:

```rust
use btcpay_plugin::prelude::*;
```

## What it generates

- `#[btcpay_plugin::plugin]` on an `impl Plugin` block: the UniFFI exports BTCPay's host calls
  into, plus `metadata()` derived from the attribute and `Cargo.toml`.
- `#[derive(BtcpaySettings)]`: the settings form, the storage keys, parsing and validation, from
  one struct.
- `#[derive(BtcpayChoice)]`: a fixed set of options for a settings field, rendered as a select.

See the [workspace README](https://github.com/uqlidi/btcpay-rs) for the whole picture.

## License

MIT or Apache-2.0, at your option.
