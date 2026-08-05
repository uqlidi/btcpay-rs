//! The smallest useful btcpay-rs plugin: it greets, remembers a setting, and reacts to
//! invoices. Note what is absent: no uniffi annotations, no `unsafe`, no C#.

use std::sync::{Arc, Mutex};

use btcpay_plugin::prelude::*;

#[derive(Default)]
struct HelloPlugin {
    host: Mutex<Option<Arc<dyn HostServices>>>,
    /// The greeting the plugin is currently using.
    ///
    /// Held in memory as well as stored, so a change takes effect immediately instead of at
    /// the next restart. Settings that are only read at startup look saved but do nothing
    /// until BTCPay is restarted, which is a confusing thing to hand an operator.
    greeting: Mutex<String>,
}

// `metadata()` is generated: name, version and description come from Cargo.toml, so they
// cannot drift from the package that produced the library.
#[btcpay_plugin::plugin(identifier = "BTCPayServer.Plugins.Hello", name = "Hello")]
impl Plugin for HelloPlugin {
    fn start(&self, host: Arc<dyn HostServices>) -> Result<(), PluginError> {
        let greeting = host
            .get_setting("greeting".into())
            .unwrap_or_else(|| "Hello from Rust".to_string());

        host.log(LogLevel::Info, format!("hello-plugin started: {greeting}"));
        *self.greeting.lock().unwrap() = greeting;
        *self.host.lock().unwrap() = Some(host);
        Ok(())
    }

    fn stop(&self) {
        // Nothing to join here: this plugin owns no threads. A plugin that spawns one MUST
        // join it here, so no callback can fire after stop() returns.
        *self.host.lock().unwrap() = None;
    }

    /// The settings page, described as data. The host renders it with BTCPay's own styles,
    /// so there is no Razor or HTML here.
    fn settings_schema(&self) -> UiDocument {
        let host = self.host.lock().unwrap();
        let stored = |key: &str| {
            host.as_ref()
                .and_then(|h| h.get_setting(key.to_string()))
                .unwrap_or_default()
        };

        Document::new("Hello")
            .text("A minimal plugin, written in Rust.")
            .form(
                Form::new("settings")
                    .title("Greeting")
                    .text("greeting", "Greeting")
                    .required()
                    .help("Logged when the plugin starts.")
                    .placeholder("Hello from Rust")
                    .value(stored("greeting"))
                    .number("repeat", "Times to log it")
                    .range(1, 10)
                    .value(stored("repeat"))
                    .toggle("verbose", "Log every invoice event")
                    .value(stored("verbose")),
            )
            .into()
    }

    fn handle(&self, event: HostEvent) -> Result<Vec<PluginAction>, PluginError> {
        Ok(match event {
            // Settlement is reported by more than one trigger, so act on the specific one
            // rather than on the resulting status.
            HostEvent::InvoiceStatusChanged { invoice, trigger } => {
                let cause = match trigger {
                    InvoiceTrigger::PaidInFull => "paid in full".to_string(),
                    InvoiceTrigger::Confirmed => "payment confirmed".to_string(),
                    InvoiceTrigger::Completed => "completed".to_string(),
                    InvoiceTrigger::Expired => "expired".to_string(),
                    InvoiceTrigger::Other { name } => format!("{name} (unmodelled)"),
                    other => format!("{other:?}"),
                };
                let greeting = self.greeting.lock().unwrap().clone();
                vec![PluginAction::Log {
                    level: LogLevel::Info,
                    message: format!(
                        "{greeting}: invoice {} is now {} ({} {}), because it was {}",
                        invoice.invoice_id, invoice.status, invoice.amount, invoice.currency, cause
                    ),
                }]
            }
            HostEvent::SettingsUpdated { values } => {
                // Taken from the event, not re-read from storage: the host saves only after
                // this returns, so storage still holds the previous values here.
                let greeting = values.get("greeting").cloned().unwrap_or_default();
                if greeting.trim().is_empty() {
                    return Err(PluginError::invalid_input("greeting must not be empty"));
                }

                // Applied before asking for the save, so the running plugin and what is
                // stored cannot disagree.
                *self.greeting.lock().unwrap() = greeting.clone();

                vec![
                    PluginAction::Log {
                        level: LogLevel::Info,
                        message: format!("greeting is now: {greeting}"),
                    },
                    PluginAction::SaveSettings { values },
                    PluginAction::Refresh,
                ]
            }
            _ => Vec::new(),
        })
    }
}
