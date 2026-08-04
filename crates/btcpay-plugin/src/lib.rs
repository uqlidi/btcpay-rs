//! Write BTCPay Server plugins in Rust.
//!
//! A plugin implements [`Plugin`] and annotates the impl with
//! [`#[btcpay_plugin::plugin]`](macro@plugin). Everything else (the C# shim, the FFI
//! bindings, packaging) is generated.
//!
//! ```ignore
//! use btcpay_plugin::prelude::*;
//!
//! #[derive(Default)]
//! struct HelloPlugin;
//!
//! #[btcpay_plugin::plugin(identifier = "Acme.Plugins.Hello")]
//! impl Plugin for HelloPlugin {}
//! ```
//!
//! That is a complete, working plugin. `metadata()` is generated: name, version and
//! description come from `Cargo.toml`, so they cannot drift from the package that built the
//! library. Every other trait method has a default, so you implement only what you use.
//!
//! # How this crate is put together
//!
//! **All** uniffi exports live here, not in plugin crates. A plugin crate contains no uniffi
//! annotations at all: it registers its implementation, and the host talks to the
//! [`PluginHandle`] exported from this crate. That keeps the generated C# identical in shape
//! for every plugin (one namespace, `uniffi.btcpay`), differing only in the native library
//! name it binds to.
//!
//! # Guarantees at the boundary
//!
//! - A panic in plugin code never unwinds into .NET; it becomes
//!   [`PluginError::Internal`] naming the method that panicked.
//! - `stop()` cannot fail, so shutdown is never blocked by plugin code.
//! - Foreign callbacks ([`HostServices`], [`EventListener`]) are implemented by the host,
//!   which swallows and logs its own exceptions. See [`host`] for why that matters.

#![deny(missing_docs)]
#![warn(clippy::all)]

mod error;
mod handle;
pub mod host;
mod plugin;
mod tooling;
mod types;

pub use error::{HostError, PluginError};
pub use handle::{btcpay_rs_abi_version, PluginHandle, SharedEventListener, ABI_VERSION};
pub use host::{EventListener, HostServices};
pub use plugin::Plugin;
pub use types::{
    HostEvent, InvoiceSummary, InvoiceTrigger, LogLevel, Notification, PluginAction,
    PluginDependency, PluginEvent, PluginMetadata, UiDocument, WebhookRequest, UI_VERSION,
};

#[doc(hidden)]
pub use handle::register_plugin;

pub use btcpay_plugin_macros::plugin;

/// Everything a typical plugin needs, in one import.
pub mod prelude {
    pub use crate::UiDocument;
    pub use crate::{
        EventListener, HostError, HostEvent, HostServices, InvoiceSummary, InvoiceTrigger,
        LogLevel, Notification, Plugin, PluginAction, PluginDependency, PluginError, PluginEvent,
        PluginMetadata, WebhookRequest,
    };
    pub use std::sync::Arc;
}

uniffi::setup_scaffolding!("btcpay");

#[doc(hidden)]
pub mod __private {
    //! Implementation details used by the generated code. Not public API.
    pub use ctor::ctor;
}
