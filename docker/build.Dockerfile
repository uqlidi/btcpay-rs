# Build environment for btcpay-rs plugins: Rust and the .NET SDK in one image.
#
# Building a plugin needs both, which is an awkward pair to install. `cargo btcpay package
# --docker` uses this so a developer needs only Docker, and so the artifact does not depend
# on whatever versions happen to be on the machine.
#
# Versions are pinned deliberately: a plugin built here should come out the same anywhere.
FROM mcr.microsoft.com/dotnet/sdk:10.0

ARG RUST_VERSION=1.92.0
ARG UNIFFI_BINDGEN_CS_TAG=v0.11.0+v0.31.0

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    DOTNET_CLI_TELEMETRY_OPTOUT=1 \
    DOTNET_NOLOGO=1

RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential ca-certificates curl git \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --profile minimal --default-toolchain ${RUST_VERSION} \
    && chmod -R a+w $RUSTUP_HOME $CARGO_HOME

# Baked in rather than fetched per build: it is a multi-minute compile, and pinning it here
# keeps the generator in step with the uniffi version the contract is built against.
RUN cargo install uniffi-bindgen-cs \
      --git https://github.com/NordSecurity/uniffi-bindgen-cs \
      --tag ${UNIFFI_BINDGEN_CS_TAG} \
    && chmod -R a+w $CARGO_HOME

WORKDIR /plugin
