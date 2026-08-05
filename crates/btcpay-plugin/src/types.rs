//! Data types crossing the FFI boundary.
//!
//! Everything here is a uniffi `Record` or `Enum`, so it maps to an idiomatic C# record.
//! Note the naming transform: Rust `snake_case` fields become C# `PascalCase`, and
//! `Vec<T>` becomes `T[]` (not `List<T>`).

use std::collections::HashMap;

// ---------------------------------------------------------------------- metadata

/// Identity of a plugin, as BTCPay sees it.
///
/// This is the Rust-side source of truth, but the generated C# shim also carries these
/// values as compile-time constants and only *verifies* them against this at startup.
/// That is not redundancy: `BTCPayServer.PluginPacker` instantiates the plugin class at
/// *packaging* time, when no native library is loadable, so metadata must be obtainable
/// without calling into Rust.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PluginMetadata {
    /// Reverse-DNS identifier, e.g. `"Acme.Plugins.Swaps"`. Must match the C# assembly's
    /// `Identifier`; a mismatch is a build error, not a runtime surprise.
    pub identifier: String,
    /// Human-readable name shown in BTCPay's plugin list.
    pub name: String,
    /// Semver version string.
    pub version: String,
    /// One-line summary shown in BTCPay's plugin list.
    pub description: String,
    /// Other plugins (including `"BTCPayServer"` itself) this one requires.
    pub dependencies: Vec<PluginDependency>,
}

/// A dependency on another plugin, e.g. `BTCPayServer >= 2.4.0`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PluginDependency {
    /// Identifier of the required plugin, or `"BTCPayServer"` for the host itself.
    pub identifier: String,
    /// A BTCPay version condition such as `">=2.4.0"`.
    pub condition: String,
}

impl PluginDependency {
    /// The dependency every plugin needs: a minimum BTCPay Server version.
    pub fn btcpay_server(condition: impl Into<String>) -> Self {
        Self {
            identifier: "BTCPayServer".to_string(),
            condition: condition.into(),
        }
    }
}

// ------------------------------------------------------------------------ logging

/// Severity of a log line written through [`HostServices::log`](crate::HostServices::log).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LogLevel {
    /// Very fine-grained diagnostics.
    Trace,
    /// Developer-facing diagnostics.
    Debug,
    /// Normal operational messages.
    Info,
    /// Something unexpected that did not stop the plugin.
    Warn,
    /// A failure the operator should see.
    Error,
}

// -------------------------------------------------------------- host -> plugin events

/// A minimal view of a BTCPay invoice. Deliberately small: this crosses the FFI on every
/// invoice event, and widening it later is easier than narrowing it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct InvoiceSummary {
    /// BTCPay's invoice identifier.
    pub invoice_id: String,
    /// Store the invoice belongs to.
    pub store_id: String,
    /// BTCPay invoice status, e.g. `"Settled"`.
    pub status: String,
    /// Amount in the invoice's currency, as a decimal string, never a float. Money in
    /// binary floating point is a bug waiting to be filed.
    pub amount: String,
    /// ISO currency code of `amount`.
    pub currency: String,
}

/// What moved an invoice into its current status.
///
/// BTCPay reports the event that fired, not the status the invoice came from, so this names
/// the cause rather than pretending to describe a transition. If you need to know the prior
/// status, record it yourself when you see the earlier event.
///
/// [`InvoiceTrigger::Other`] carries anything this version of btcpay-rs does not model, so a
/// new BTCPay event shows up as data instead of breaking the contract.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum InvoiceTrigger {
    /// The invoice received enough to cover its total, but is not yet confirmed.
    PaidInFull,
    /// A payment reached the required number of confirmations.
    Confirmed,
    /// The invoice is fully settled and closed.
    Completed,
    /// The invoice expired before being paid in full.
    Expired,
    /// The invoice expired having received a partial payment.
    ExpiredPaidPartial,
    /// An operator marked the invoice settled by hand.
    MarkedCompleted,
    /// An operator marked the invoice invalid by hand.
    MarkedInvalid,
    /// A payment arrived (possibly one of several).
    ReceivedPayment,
    /// A payment was settled on-chain or on Lightning.
    PaymentSettled,
    /// A BTCPay event this version of btcpay-rs does not model.
    Other {
        /// BTCPay's raw event name, e.g. `"invoice_failedToConfirm"`.
        name: String,
    },
}

/// Something that happened in BTCPay, delivered to the plugin.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum HostEvent {
    /// A new invoice was created.
    InvoiceCreated {
        /// The newly created invoice.
        invoice: InvoiceSummary,
    },
    /// An invoice changed status.
    ///
    /// Several triggers can leave an invoice in the same status (a payment confirming and
    /// the invoice completing both end at `Settled`), so treat handling as idempotent rather
    /// than assuming one event per status.
    InvoiceStatusChanged {
        /// The invoice in its new state.
        invoice: InvoiceSummary,
        /// What caused the change.
        trigger: InvoiceTrigger,
    },
    /// The operator saved the settings form.
    ///
    /// The values are the submission, already checked against the form the plugin described.
    ///
    /// Two things to know:
    ///
    /// - **Read them from here, not from storage.** The host performs the actions this
    ///   handler returns only after it returns, so [`HostServices::get_setting`] still
    ///   reports the previous values while handling this event.
    /// - **Apply them to any in-memory state now.** A plugin that only reads its settings in
    ///   [`crate::Plugin::start`] will appear to save them while continuing to use the old
    ///   ones until BTCPay restarts.
    ///
    /// Returning [`PluginAction::SaveSettings`] is what actually persists them; returning
    /// nothing, or an error, rejects the submission and leaves the stored values alone.
    SettingsUpdated {
        /// The submitted settings, keyed by field id.
        values: HashMap<String, String>,
    },
    /// A form rendered from the plugin's `UiDocument` was submitted.
    FormSubmitted {
        /// Identifier of the submitted form.
        form_id: String,
        /// Submitted values, keyed by field id.
        values: HashMap<String, String>,
    },
    /// Periodic heartbeat from the host's hosted service; use it for polling work.
    Tick,
}

/// What the plugin wants the host to do in response to a `HostEvent`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum PluginAction {
    /// Persist these settings.
    SaveSettings {
        /// Values to store, keyed by field id.
        values: HashMap<String, String>,
    },
    /// Raise a notification for the operator.
    Notify {
        /// The notification to raise.
        notification: Notification,
    },
    /// Deliver a webhook.
    SendWebhook {
        /// The webhook to deliver.
        webhook: WebhookRequest,
    },
    /// Write a line to BTCPay's log.
    Log {
        /// Severity.
        level: LogLevel,
        /// Message text.
        message: String,
    },
    /// Re-render the plugin's UI (the host will call `settings_schema`/`dashboard` again).
    Refresh,
}

// -------------------------------------------------------------- plugin -> host events

/// Pushed from the plugin to the host, typically off a background thread.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum PluginEvent {
    /// Free-form status the host surfaces on the plugin's dashboard.
    StatusChanged {
        /// New status text.
        status: String,
    },
    /// Raise a notification for the operator.
    Notify {
        /// The notification to raise.
        notification: Notification,
    },
    /// Write a line to BTCPay's log.
    Log {
        /// Severity.
        level: LogLevel,
        /// Message text.
        message: String,
    },
    /// The plugin has failed in a way the operator should see.
    Failed {
        /// What went wrong.
        message: String,
    },
}

/// A notification surfaced to the BTCPay operator.
#[derive(Debug, Clone, uniffi::Record)]
pub struct Notification {
    /// Short headline.
    pub title: String,
    /// Body text.
    pub body: String,
    /// Optional BTCPay-relative link, e.g. `"/plugins/acme/status"`.
    pub link: Option<String>,
}

/// A webhook to deliver through BTCPay's webhook machinery.
#[derive(Debug, Clone, uniffi::Record)]
pub struct WebhookRequest {
    /// Event type name, e.g. `"SwapCompleted"`.
    pub event_type: String,
    /// JSON payload, already serialized.
    pub payload_json: String,
}

// ----------------------------------------------------------------------------- UI

/// Wire-format version of [`UiDocument`], so an older host can degrade gracefully rather
/// than mis-render a document produced by a newer plugin.
pub const UI_VERSION: u32 = 1;

/// A renderable description of a plugin's UI, rendered by generic Razor views in the host.
///
/// Build one with [`btcpay_ui::Document`] and convert; this type is only the envelope that
/// crosses the FFI. The page itself travels as JSON in [`UiDocument::document_json`] rather
/// than as generated types, so adding a node kind is something the host can degrade over
/// instead of an ABI break that forces every plugin to be rebuilt.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct UiDocument {
    /// Always [`UI_VERSION`] for documents built by this crate.
    pub ui_version: u32,
    /// Heading shown above the rendered content.
    pub title: String,
    /// The page, as JSON. See [`btcpay_ui::Document`] for its shape.
    pub document_json: String,
}

impl UiDocument {
    /// A document with no content: the default for plugins that expose no settings.
    pub fn empty() -> Self {
        Self::from(btcpay_ui::Document::empty())
    }

    /// A titled but otherwise empty document.
    pub fn new(title: impl Into<String>) -> Self {
        Self::from(btcpay_ui::Document::new(title))
    }
}

impl From<btcpay_ui::Document> for UiDocument {
    fn from(document: btcpay_ui::Document) -> Self {
        Self {
            ui_version: UI_VERSION,
            title: document.title.clone(),
            document_json: document.to_json(),
        }
    }
}

impl Default for UiDocument {
    fn default() -> Self {
        Self::empty()
    }
}
