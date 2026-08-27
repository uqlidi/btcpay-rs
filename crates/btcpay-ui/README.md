# btcpay-ui

Describe a [BTCPay Server](https://btcpayserver.org) plugin's settings page and dashboard as
data, and let the host render it.

A plugin returns a `Document`; generic Razor views in the btcpay-rs host turn it into a page
using BTCPay's own styles. No Razor, no C#, no HTML.

```rust
use btcpay_ui::{Document, Form};

let doc = Document::new("Swap settings").form(
    Form::new("settings")
        .text("api_key", "API key")
        .number("poll_secs", "Poll interval (seconds)"),
);
```

Alongside forms there are stats, tables, alerts and buttons, which is enough for a settings page
and a dashboard.

The tree crosses the FFI as JSON inside a single string field rather than as generated types.
That is deliberate: adding a node type is then a wire-format change the host can degrade over,
not an ABI break that forces every plugin to be rebuilt.

Used through [`btcpay-plugin`](https://crates.io/crates/btcpay-plugin), which re-exports the
parts a plugin needs. See the [workspace README](https://github.com/uqlidi/btcpay-rs).

## License

MIT or Apache-2.0, at your option.
