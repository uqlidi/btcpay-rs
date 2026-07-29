//! The trait a plugin author implements.

use std::sync::Arc;

use crate::error::PluginError;
use crate::host::HostServices;
use crate::types::UiDocument;
use crate::types::{HostEvent, PluginAction, PluginMetadata};

/// What a BTCPay plugin written in Rust must provide.
///
/// This is a **plain Rust trait** with no uniffi annotations. Authors implement it normally and
/// apply [`macro@btcpay_plugin_macros::plugin`] to the impl block; all FFI machinery lives in
/// this crate, so plugin crates contain no unsafe code and no bindings boilerplate.
///
/// ```ignore
/// use btcpay_plugin::prelude::*;
///
/// #[derive(Default)]
/// struct MyPlugin;
///
/// #[btcpay_plugin::plugin(identifier = "Acme.Plugins.Mine")]
/// impl Plugin for MyPlugin {}
/// ```
///
/// [`Plugin::metadata`] is the only method without a default, and the attribute writes it
/// for you unless you need to compute it. So a minimal plugin is genuinely two lines.
pub trait Plugin: Send + Sync + 'static {
    /// Identity of this plugin. Must be a pure function of the plugin itself: it is called
    /// before `start`, and its values must agree with the generated C# constants.
    ///
    /// Usually generated: pass `identifier = "..."` to
    /// [`#[btcpay_plugin::plugin]`](macro@btcpay_plugin_macros::plugin) and omit this method.
    /// Implement it by hand when the plugin depends on *other* plugins, or derives its
    /// identity at runtime.
    fn metadata(&self) -> PluginMetadata;

    /// Called once when BTCPay starts the plugin. Spawn background work here.
    ///
    /// `host` stays valid until [`Plugin::stop`] returns; clone the `Arc` freely.
    /// Returning `Err` disables the plugin and surfaces the message to the operator.
    fn start(&self, host: Arc<dyn HostServices>) -> Result<(), PluginError> {
        let _ = host;
        Ok(())
    }

    /// Called on shutdown. Must **join** any threads that hold an `EventListener`, so no
    /// callback can fire after this returns.
    fn stop(&self) {}

    /// Declarative description of this plugin's settings form.
    ///
    /// Rendered by generic Razor views in the host, so no C# or Razor is needed.
    fn settings_schema(&self) -> UiDocument {
        UiDocument::empty()
    }

    /// Handle something that happened in BTCPay, returning actions for the host to perform.
    fn handle(&self, event: HostEvent) -> Result<Vec<PluginAction>, PluginError> {
        let _ = event;
        Ok(Vec::new())
    }
}
