# Contributing

Thanks for looking. This document covers what is unusual about the repository, because the
ordinary parts (fork, branch, open a PR) work the way you expect.

## What you need

| | |
|---|---|
| Rust | 1.88 or newer, the MSRV declared in `Cargo.toml` |
| .NET SDK | 10.0, for the C# host and its tests |
| Docker | enough on its own for packaging, and how CI runs the C# tests |

You can work on the Rust side with Rust alone. Touching anything under `dotnet/` needs the .NET
SDK, or the pinned build image.

## Running the tests

```sh
cargo test --workspace                # the Rust side
cargo fmt --all --check               # formatting is enforced
cargo clippy --workspace --all-targets -- -D warnings
./dev/check-pins.sh                   # see "Versions that must agree" below
```

The C# tests need the .NET SDK. With only Docker:

```sh
docker build -f docker/build.Dockerfile -t btcpay-rs-build:local .
docker run --rm -v "$PWD:$PWD" -w "$PWD" -u "$(id -u):$(id -g)" btcpay-rs-build:local \
  bash -c 'cargo build --release -p hello-plugin \
           && ./dotnet/regen-bindings.sh \
           && dotnet test dotnet/BtcpayRs.Host.Tests/BtcpayRs.Host.Tests.csproj -c Release'
```

`regen-bindings.sh` has to run first: the bindings are generated from the compiled library and are
not committed.

## Trying a plugin for real

```sh
cargo btcpay package --manifest-dir examples/hello-plugin --docker
./dev/run-btcpay.sh artifacts/BTCPayServer.Plugins.Hello
```

That brings up a regtest BTCPay on <http://localhost:14142> with the example installed. Pass the
artifact **directory** rather than a file: the packer writes into a version-named subdirectory, and
the script picks the newest so you cannot silently install a stale build.

`dev/docker-compose.yml` also carries Tor and a custom signet node. Neither is needed for
btcpay-rs itself; they exist for a coinswap plugin built on top of it.

## Versions that must agree

Run `./dev/check-pins.sh` before opening a PR. Two pairs are duplicated across languages and both
fail in ways that point nowhere near the cause:

- **`uniffi` and `uniffi-bindgen-cs`.** The generator targets one specific uniffi release. A
  mismatch produces bindings that compile and then disagree with the library's actual memory
  layout at runtime. Bump them together, never separately.
- **`ABI_VERSION` in Rust and `SupportedAbi` in C#.** The host refuses to load a plugin whose ABI
  it does not recognise. If these drift, every plugin stops loading.

Raise `ABI_VERSION` whenever you change the shape of anything crossing the FFI boundary: the
`Plugin` trait, `HostServices`, `HostEvent`, `PluginAction`. You do **not** need to raise it to add
a new kind of page element, because pages cross as JSON and an older host degrades over an
unfamiliar one.

## Two things that will surprise you

**The C# host is embedded in `cargo-btcpay`.** `crates/cargo-btcpay/src/host.rs` pulls every host
source in with `include_str!` and materialises them into generated projects at build time. So:

- **Editing a file under `dotnet/` means rebuilding the CLI.** Invoking a previously built binary
  materialises the old copy. Use `cargo run --release -p cargo-btcpay -- btcpay ...` while
  iterating. This has cost real time more than once.
- **A new file under `dotnet/` must be added to `host.rs`.** A guard test walks the directory and
  fails if anything is missing, so you will be told, but the message is easier to understand if
  you already know why.

**`crates/cargo-btcpay/dotnet/` is two symlinks**, pointing at the real projects at the repository
root. `include_str!` cannot read above its own crate directory, so without them the published
crate does not compile at all. There is one copy of the sources; the symlinks only make them
reachable from inside the package.

On Windows, git needs symlink support for a usable checkout: developer mode enabled, or
`git config core.symlinks true`. Consumers installing from crates.io are unaffected, because cargo
materialises real file contents into the published tarball.

## Style

- `cargo fmt` and `clippy -D warnings` are both enforced in CI.
- Comments earn their place by saying **why**, especially when the code looks odd. Most of the odd
  code here is odd because something failed, and the comment is the record of what.
- Plain ASCII in code and documentation. No em-dashes, arrows or smart quotes.
- Commit subjects follow conventional commits: `fix:`, `feat:`, `docs:`, `ci:`, optionally scoped
  as `feat(dev):`.

## Pull requests

Small and stacked beats large. The history here is a chain of narrow PRs, each building on the one
below, which the [`gh stack`](https://github.com/github/gh-stack) extension manages:

```sh
gh stack view                 # see the chain and which PRs are merged
gh stack rebase               # cascade-rebase after something below you lands
gh stack push
```

Rebasing by hand also works, but a squash-merged parent needs
`git rebase --onto origin/master <old-parent> <branch>` so the merged commit is dropped rather than
replayed onto a trunk that already contains it. `gh stack rebase` does that for you.

Please say in the PR body what you verified and how. "Tests pass" is less useful than naming the
case you were worried about and what you did to check it.

## License

By contributing you agree that your work is dual licensed under MIT and Apache-2.0, matching the
rest of the project.
