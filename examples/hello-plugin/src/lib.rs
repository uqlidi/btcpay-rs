//! The smallest useful btcpay-rs plugin: it greets, remembers a setting, and reacts to
//! invoices. Note what is absent — no uniffi annotations, no `unsafe`, no C#.

use std::sync::{Arc, Mutex};

use btcpay_plugin::prelude::*;

#[derive(Default)]
struct HelloPlugin {
    host: Mutex<Option<Arc<dyn HostServices>>>,
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
        *self.host.lock().unwrap() = Some(host);
        Ok(())
    }

    fn stop(&self) {
        // Nothing to join here: this plugin owns no threads. A plugin that spawns one MUST
        // join it here, so no callback can fire after stop() returns.
        *self.host.lock().unwrap() = None;
    }

    fn handle(&self, event: HostEvent) -> Result<Vec<PluginAction>, PluginError> {
        Ok(match event {
            HostEvent::InvoiceStatusChanged {
                invoice,
                previous_status,
            } => {
                vec![PluginAction::Log {
                    level: LogLevel::Info,
                    message: format!(
                        "invoice {} moved {} -> {} ({} {})",
                        invoice.invoice_id,
                        previous_status,
                        invoice.status,
                        invoice.amount,
                        invoice.currency
                    ),
                }]
            }
            HostEvent::SettingsUpdated { values } => {
                let greeting = values.get("greeting").cloned().unwrap_or_default();
                if greeting.trim().is_empty() {
                    return Err(PluginError::invalid_input("greeting must not be empty"));
                }
                vec![PluginAction::SaveSettings { values }, PluginAction::Refresh]
            }
            _ => Vec::new(),
        })
    }
}
