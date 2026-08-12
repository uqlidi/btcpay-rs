//! Services the host (the C# shim) provides to the plugin.
//!
//! Both traits here are uniffi *foreign* traits: Rust declares them, C# implements them.
//!
//! # The callback-safety rule
//!
//! uniffi turns an unexpected exception thrown by a foreign implementation into a Rust
//! **panic**. On a synchronous call that surfaces to the C# caller; on a Rust background
//! thread it unwinds and **kills that thread**, silently stopping the plugin. The generated
//! `BtcpayRs.Host` implementations therefore wrap every method body in a catch-all that logs
//! and never rethrows. Plugin authors do not need to do anything, but they should not
//! assume a host call that "returned" actually succeeded, which is why the fallible
//! operations return [`Result`].

use crate::error::HostError;
use crate::types::{LogLevel, Notification, PluginEvent, WebhookRequest};

/// Host capabilities available to a running plugin.
///
/// Implemented in C#, handed to the plugin in [`crate::Plugin::start`]. Deliberately small:
/// every method here is permanent API surface and a compatibility obligation.
#[uniffi::export(with_foreign)]
pub trait HostServices: Send + Sync {
    /// Read a value from this plugin's settings (the operator-editable key/value store).
    fn get_setting(&self, key: String) -> Option<String>;

    /// Write a value to this plugin's settings.
    fn set_setting(&self, key: String, value: String) -> Result<(), HostError>;

    /// Read from this plugin's private key/value store (namespaced per plugin).
    fn store_get(&self, key: String) -> Option<Vec<u8>>;

    /// Write to this plugin's private key/value store.
    fn store_put(&self, key: String, value: Vec<u8>) -> Result<(), HostError>;

    /// Remove a key from this plugin's private store.
    fn store_delete(&self, key: String) -> Result<(), HostError>;

    /// A directory this plugin may write files in.
    ///
    /// Created before the plugin starts, private to this plugin, and stable across restarts
    /// and upgrades. Use it for anything that does not fit a key/value pair: a wallet, a
    /// database, a log the plugin manages itself.
    ///
    /// It lives inside BTCPay's own data directory, so an operator's existing backups and
    /// volume mounts already cover it. Two consequences worth knowing:
    ///
    /// - **Nothing here is transactional.** Unlike settings, a half-written file stays
    ///   half-written. A plugin holding anything it cannot afford to lose is responsible for
    ///   writing it safely, for example by writing beside the target and renaming.
    /// - **Nothing here is migrated.** File formats are the plugin's own problem across
    ///   versions.
    fn data_dir(&self) -> String;

    /// Write to BTCPay's log. Prefer this over `println!`: it reaches the operator.
    fn log(&self, level: LogLevel, message: String);

    /// Raise a BTCPay notification for the operator.
    fn emit_notification(&self, notification: Notification) -> Result<(), HostError>;

    /// Deliver a webhook through BTCPay's webhook machinery (respecting its retry policy).
    fn send_webhook(&self, webhook: WebhookRequest) -> Result<(), HostError>;
}

/// Sink for events the plugin pushes to the host, typically from a background thread.
///
/// Verified to work off-thread inside BTCPay's plugin `AssemblyLoadContext`.
#[uniffi::export(with_foreign)]
pub trait EventListener: Send + Sync {
    /// Deliver one event to the host.
    fn on_event(&self, event: PluginEvent);
}
