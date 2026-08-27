# cargo-btcpay

Scaffold, build and package [BTCPay Server](https://btcpayserver.org) plugins written in Rust.

```sh
cargo install cargo-btcpay
cargo btcpay new my-plugin        # a project with no C# in it
cd my-plugin
cargo test                        # your plugin is a normal Rust crate
cargo btcpay package              # produces an installable .btcpay file
```

Upload the result in BTCPay under Server Settings, Plugins, Upload.

## Commands

|           |                                                                 |
| --------- | --------------------------------------------------------------- |
| `new`     | Create a plugin project.                                        |
| `build`   | Compile the plugin and the C# that wraps it.                    |
| `package` | Build and assemble an installable `.btcpay` file.               |
| `inspect` | Print what a compiled plugin library reports about itself.      |
| `shim`    | Generate the C# project for a compiled library, for inspection. |

## Requirements

Rust 1.88 or newer and the .NET SDK 10.0, since a plugin is a Rust cdylib wrapped in generated
C#. Those two toolchains together are the awkward part, so `cargo btcpay package --docker` runs
the whole pipeline in a pinned image and needs only Docker locally.

The first build fetches a BTCPay Server checkout, because `BTCPayServer.Abstractions` is not
published on NuGet. It is cached, so only the first build pays for it.

The C# host project is embedded in this binary, so a `cargo install` needs no checkout of the
repository.

See the [workspace README](https://github.com/uqlidi/btcpay-rs) for what an operator ends up
running and the current limitations.

## License

MIT or Apache-2.0, at your option.
