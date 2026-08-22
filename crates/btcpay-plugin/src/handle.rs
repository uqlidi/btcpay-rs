//! The single uniffi object the host talks to, and the panic barrier around it.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, OnceLock};

use crate::error::PluginError;
use crate::host::{EventListener, HostServices};
use crate::plugin::Plugin;
use crate::types::UiDocument;
use crate::types::{HostEvent, PluginAction, PluginMetadata};

/// Contract version. The host refuses to load a plugin whose ABI it does not understand.
///
/// Bump on **any** breaking change to the exported surface: a new/removed method on
/// [`Plugin`], a changed record field, a renamed enum variant.
pub const ABI_VERSION: u32 = 3;

/// Reported to the host before anything else is called.
#[uniffi::export]
pub fn btcpay_rs_abi_version() -> u32 {
    ABI_VERSION
}

type PluginFactory = fn() -> Arc<dyn Plugin>;
static FACTORY: OnceLock<PluginFactory> = OnceLock::new();

/// Registers the plugin implementation. Called by the generated code from
/// [`macro@btcpay_plugin_macros::plugin`]. Not intended for direct use.
#[doc(hidden)]
pub fn register_plugin(factory: PluginFactory) {
    if FACTORY.set(factory).is_err() {
        // Two `#[plugin]` attributes in one cdylib: a build-time mistake, and the second
        // registration would be silently ignored. Fail loudly instead.
        panic!(
            "btcpay-rs: a plugin is already registered in this library; \
             exactly one type may be annotated with #[btcpay_plugin::plugin]"
        );
    }
}

/// The registered factory, if a plugin has been registered. Used by the build tooling in
/// [`crate::tooling`], which reads metadata without going through the uniffi contract.
pub(crate) fn plugin_factory() -> Option<PluginFactory> {
    FACTORY.get().copied()
}

/// Runs `f`, converting a panic into a [`PluginError`] rather than letting it unwind across
/// the FFI boundary.
///
/// Every method below goes through this. Unwinding into the .NET runtime is undefined
/// behaviour, and even where uniffi catches it, an uncaught panic degrades into an opaque
/// `PanicException` that tells the operator nothing useful.
fn guard<T>(what: &str, f: impl FnOnce() -> Result<T, PluginError>) -> Result<T, PluginError> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(payload) => Err(PluginError::internal(format!(
            "plugin panicked in {what}: {}",
            panic_message(&payload)
        ))),
    }
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

/// The object the C# shim holds. Every call is delegated to the registered [`Plugin`],
/// wrapped in a panic barrier.
#[derive(uniffi::Object)]
pub struct PluginHandle {
    inner: Arc<dyn Plugin>,
}

#[uniffi::export]
impl PluginHandle {
    /// Instantiates the registered plugin.
    ///
    /// Fails if the library contains no `#[btcpay_plugin::plugin]` type, which means the
    /// cdylib was built without linking the plugin, a build-configuration error.
    #[uniffi::constructor]
    pub fn new() -> Result<Arc<Self>, PluginError> {
        let factory = FACTORY.get().ok_or_else(|| {
            PluginError::internal(
                "no plugin registered in this library; is #[btcpay_plugin::plugin] applied \
                 to your Plugin impl, and is the crate built as a cdylib?",
            )
        })?;
        let inner = guard("plugin construction", || Ok(factory()))?;
        Ok(Arc::new(Self { inner }))
    }

    /// See [`Plugin::metadata`]. Safe to call before `start`.
    pub fn metadata(&self) -> Result<PluginMetadata, PluginError> {
        guard("metadata()", || Ok(self.inner.metadata()))
    }

    /// See [`Plugin::start`].
    pub fn start(&self, host: Arc<dyn HostServices>) -> Result<(), PluginError> {
        guard("start()", || self.inner.start(host))
    }

    /// See [`Plugin::stop`]. Never fails: shutdown must not be blockable by plugin code.
    pub fn stop(&self) {
        let _ = guard("stop()", || {
            self.inner.stop();
            Ok(())
        });
    }

    /// See [`Plugin::settings_schema`].
    pub fn settings_schema(&self) -> Result<UiDocument, PluginError> {
        guard("settings_schema()", || Ok(self.inner.settings_schema()))
    }

    /// See [`Plugin::handle`].
    pub fn handle(&self, event: HostEvent) -> Result<Vec<PluginAction>, PluginError> {
        guard("handle()", || self.inner.handle(event))
    }

    /// The ABI this library was built against, for the host's version handshake.
    pub fn abi_version(&self) -> u32 {
        ABI_VERSION
    }
}

/// Re-exported so the host can hold a listener without the plugin crate naming uniffi types.
pub type SharedEventListener = Arc<dyn EventListener>;
