//! The smallest useful btcpay-rs plugin: it greets, remembers a setting, and reacts to
//! invoices. Note what is absent: no uniffi annotations, no `unsafe`, no C#.

use std::sync::{Arc, Mutex};

use btcpay_plugin::prelude::*;

/// How loudly the plugin talks to the log.
///
/// An enum rather than a string: the dropdown's options come from the type, so the form and
/// the values the field can hold cannot disagree, and matching on it needs no arm for a case
/// that should be impossible.
#[derive(Debug, Clone, Copy, PartialEq, Default, BtcpayChoice)]
enum Verbosity {
    /// One line per invoice event.
    #[choice(label = "Quiet: settlements only")]
    Quiet,
    /// Every invoice event.
    #[default]
    #[choice(label = "Normal: every invoice event")]
    Normal,
    /// Everything, including ticks.
    #[choice(label = "Chatty: everything, including heartbeats")]
    Chatty,
}

/// The plugin's settings, as a struct.
///
/// The derive generates the form, the storage keys, loading, saving and validation from this
/// one declaration, so they cannot drift apart. Compare the alternative: a form described in
/// one place, `values.get("greeting")` in another, and parsing by hand in a third.
#[derive(Debug, Clone, Default, BtcpaySettings)]
struct Settings {
    #[setting(label = "Greeting", help = "Logged when the plugin starts.", required)]
    greeting: String,

    #[setting(label = "Times to log it", min = 1, max = 10)]
    repeat: u32,

    #[setting(label = "Log every invoice event")]
    verbose: bool,

    #[setting(
        label = "Verbosity",
        help = "How much the plugin writes to the server log."
    )]
    verbosity: Verbosity,
}

#[derive(Default)]
struct HelloPlugin {
    /// Counts invoice events seen, so the dashboard has something that changes.
    invoices_seen: Mutex<u64>,
    /// The last few events, newest first.
    recent: Mutex<Vec<String>>,
    host: Mutex<Option<Arc<dyn HostServices>>>,
    /// The settings the plugin is currently using.
    ///
    /// Held in memory as well as stored, so a change takes effect immediately instead of at
    /// the next restart. Settings only read at startup look saved but do nothing until BTCPay
    /// is restarted, which is a confusing thing to hand an operator.
    settings: Mutex<Settings>,
}

// `metadata()` is generated: name, version and description come from Cargo.toml, so they
// cannot drift from the package that produced the library.
impl HelloPlugin {
    /// Records something for the Activity page, keeping the most recent few.
    fn record(&self, what: String) {
        *self.invoices_seen.lock().unwrap() += 1;
        let mut recent = self.recent.lock().unwrap();
        recent.insert(0, what);
        recent.truncate(10);
    }
}

#[btcpay_plugin::plugin(identifier = "BTCPayServer.Plugins.Hello", name = "Hello")]
impl Plugin for HelloPlugin {
    fn start(&self, host: Arc<dyn HostServices>) -> Result<(), PluginError> {
        let mut settings = Settings::load(host.as_ref());
        if settings.greeting.is_empty() {
            settings.greeting = "Hello from Rust".to_string();
        }
        let greeting = settings.greeting.clone();

        // Writing a file proves the directory is real and writable. A plugin with actual
        // state would put a wallet or a database here.
        let data_dir = host.data_dir();
        match std::fs::write(
            std::path::Path::new(&data_dir).join("last-start.txt"),
            format!("{greeting}\n"),
        ) {
            Ok(()) => host.log(LogLevel::Debug, format!("wrote state to {data_dir}")),
            Err(e) => host.log(
                LogLevel::Warn,
                format!("could not write to {data_dir}: {e}"),
            ),
        }

        host.log(LogLevel::Info, format!("hello-plugin started: {greeting}"));
        *self.settings.lock().unwrap() = settings;
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
        // The form comes from the settings struct, so adding a field is a one-line change in
        // one place rather than four edits that have to agree.
        let settings = self.settings.lock().unwrap().clone();

        Document::new("Hello")
            .text("A minimal plugin, written in Rust.")
            .form(settings.form().title("Greeting"))
            .into()
    }

    /// A second page, to show that a plugin is not limited to settings.
    fn pages(&self) -> Vec<PageInfo> {
        vec![PageInfo::new("dashboard", "Activity")]
    }

    fn page(&self, id: String) -> Result<UiDocument, PluginError> {
        if id != "dashboard" {
            // An unknown id means a stale link or a hand-typed URL. Empty is the right answer
            // and the host renders a not-found.
            return Ok(UiDocument::empty());
        }

        let seen = *self.invoices_seen.lock().unwrap();
        let recent = self.recent.lock().unwrap().clone();

        let mut table = Table::new(["What happened"]).empty_message("No invoice events yet.");
        for line in recent.iter() {
            table = table.row([line.clone()]);
        }

        Ok(Document::new("Activity")
            .stats(
                Stats::new()
                    .card("Invoice events", seen.to_string())
                    .detail("since the plugin started")
                    .card(
                        "Verbosity",
                        format!("{:?}", self.settings.lock().unwrap().verbosity),
                    ),
            )
            .section(table)
            .actions(
                Actions::new()
                    .title("Actions")
                    .button(Button::new("say-hello", "Say hello in the log").primary())
                    .button(
                        Button::new("clear", "Clear activity")
                            .destructive("Clear the recorded activity?"),
                    ),
            )
            .into())
    }

    fn handle(&self, event: HostEvent) -> Result<Vec<PluginAction>, PluginError> {
        Ok(match event {
            // Settlement is reported by more than one trigger, so act on the specific one
            // rather than on the resulting status.
            HostEvent::InvoiceCreated { invoice } => {
                self.record(format!(
                    "{} created ({} {})",
                    invoice.invoice_id, invoice.amount, invoice.currency
                ));

                // Creation is noise at the quietest setting.
                if self.settings.lock().unwrap().verbosity == Verbosity::Quiet {
                    return Ok(Vec::new());
                }

                vec![PluginAction::Log {
                    level: LogLevel::Info,
                    message: format!("invoice {} created", invoice.invoice_id),
                }]
            }

            HostEvent::InvoiceStatusChanged { invoice, trigger } => {
                let trigger_kind = trigger.clone();
                let cause = match trigger {
                    InvoiceTrigger::PaidInFull => "paid in full".to_string(),
                    InvoiceTrigger::Confirmed => "payment confirmed".to_string(),
                    InvoiceTrigger::Completed => "completed".to_string(),
                    InvoiceTrigger::Expired => "expired".to_string(),
                    InvoiceTrigger::Other { name } => format!("{name} (unmodelled)"),
                    other => format!("{other:?}"),
                };
                let settings = self.settings.lock().unwrap().clone();
                let greeting = settings.greeting.clone();

                // The dropdown decides what gets logged, so choosing a different option has a
                // visible effect rather than only being stored.
                let worth_logging = match settings.verbosity {
                    Verbosity::Quiet => matches!(
                        trigger_kind,
                        InvoiceTrigger::Confirmed | InvoiceTrigger::Completed
                    ),
                    Verbosity::Normal | Verbosity::Chatty => true,
                };

                self.record(format!(
                    "{} is now {} ({cause})",
                    invoice.invoice_id, invoice.status
                ));

                if !worth_logging {
                    return Ok(Vec::new());
                }

                vec![PluginAction::Log {
                    level: LogLevel::Info,
                    message: format!(
                        "{greeting}: invoice {} is now {} ({} {}), because it was {}",
                        invoice.invoice_id, invoice.status, invoice.amount, invoice.currency, cause
                    ),
                }]
            }
            HostEvent::SettingsUpdated { values } => {
                // Parsed and validated in one step, and typed from here on. Taken from the
                // event, not re-read from storage: the host saves only after this returns.
                let settings = Settings::from_values(&values)?;
                let greeting = settings.greeting.clone();

                // Applied before asking for the save, so the running plugin and what is
                // stored cannot disagree.
                *self.settings.lock().unwrap() = settings.clone();

                vec![
                    PluginAction::Log {
                        level: LogLevel::Info,
                        message: format!("greeting is now: {greeting}"),
                    },
                    PluginAction::SaveSettings {
                        values: settings.to_values(),
                    },
                    PluginAction::Refresh,
                ]
            }
            // Commands are how a page does something, as opposed to saving values.
            HostEvent::CommandInvoked { command, page } => match command.as_str() {
                "say-hello" => {
                    let greeting = self.settings.lock().unwrap().greeting.clone();
                    vec![
                        PluginAction::Log {
                            level: LogLevel::Info,
                            message: format!("{greeting}, from a button press on {page}"),
                        },
                        PluginAction::ShowMessage {
                            level: MessageLevel::Success,
                            text: format!("Wrote \"{greeting}\" to the server log."),
                        },
                    ]
                }
                "clear" => {
                    self.recent.lock().unwrap().clear();
                    *self.invoices_seen.lock().unwrap() = 0;
                    vec![PluginAction::ShowMessage {
                        level: MessageLevel::Success,
                        text: "Activity cleared.".into(),
                    }]
                }
                other => {
                    // A command the plugin does not know means a stale page. Say so rather
                    // than silently doing nothing.
                    vec![PluginAction::ShowMessage {
                        level: MessageLevel::Warning,
                        text: format!("Unknown action: {other}"),
                    }]
                }
            },
            _ => Vec::new(),
        })
    }
}
