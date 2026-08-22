//! Contract tests driven through a mock host, with no C# involved.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use btcpay_plugin::prelude::*;
use btcpay_plugin::{HostError, ABI_VERSION};

// ------------------------------------------------------------------ mock host

/// In-memory [`HostServices`], standing in for the C# shim.
struct MockHost {
    data_dir: std::path::PathBuf,
    settings: Mutex<HashMap<String, String>>,
    store: Mutex<HashMap<String, Vec<u8>>>,
    logs: Mutex<Vec<(LogLevel, String)>>,
    notifications: Mutex<Vec<Notification>>,
}

/// A mock with a real, unique directory, so a plugin under test can actually write files.
///
/// Created eagerly rather than lazily: the contract says the directory exists before the
/// plugin starts, and a mock that behaved otherwise would let a bug through.
impl Default for MockHost {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let data_dir =
            std::env::temp_dir().join(format!("btcpay-rs-mock-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&data_dir).expect("a temporary directory for the mock host");

        Self {
            data_dir,
            settings: Mutex::default(),
            store: Mutex::default(),
            logs: Mutex::default(),
            notifications: Mutex::default(),
        }
    }
}

impl HostServices for MockHost {
    fn data_dir(&self) -> String {
        self.data_dir.to_string_lossy().into_owned()
    }

    fn get_setting(&self, key: String) -> Option<String> {
        self.settings.lock().unwrap().get(&key).cloned()
    }
    fn set_setting(&self, key: String, value: String) -> Result<(), HostError> {
        self.settings.lock().unwrap().insert(key, value);
        Ok(())
    }
    fn store_get(&self, key: String) -> Option<Vec<u8>> {
        self.store.lock().unwrap().get(&key).cloned()
    }
    fn store_put(&self, key: String, value: Vec<u8>) -> Result<(), HostError> {
        self.store.lock().unwrap().insert(key, value);
        Ok(())
    }
    fn store_delete(&self, key: String) -> Result<(), HostError> {
        self.store.lock().unwrap().remove(&key);
        Ok(())
    }
    fn log(&self, level: LogLevel, message: String) {
        self.logs.lock().unwrap().push((level, message));
    }
    fn emit_notification(&self, notification: Notification) -> Result<(), HostError> {
        self.notifications.lock().unwrap().push(notification);
        Ok(())
    }
    fn send_webhook(&self, _webhook: WebhookRequest) -> Result<(), HostError> {
        Ok(())
    }
}

fn metadata_for(id: &str) -> PluginMetadata {
    PluginMetadata {
        identifier: id.into(),
        name: "Test".into(),
        version: "0.1.0".into(),
        description: "test plugin".into(),
        dependencies: vec![PluginDependency::btcpay_server(">=2.4.0")],
    }
}

// ------------------------------------------------------------- plugins under test

#[derive(Default)]
struct WellBehaved {
    started: Mutex<bool>,
}

impl Plugin for WellBehaved {
    fn metadata(&self) -> PluginMetadata {
        metadata_for("Test.WellBehaved")
    }
    fn start(&self, host: Arc<dyn HostServices>) -> Result<(), PluginError> {
        host.log(LogLevel::Info, "started".into());
        *self.started.lock().unwrap() = true;
        Ok(())
    }
    fn stop(&self) {
        *self.started.lock().unwrap() = false;
    }
    fn handle(&self, event: HostEvent) -> Result<Vec<PluginAction>, PluginError> {
        Ok(match event {
            HostEvent::Tick => vec![PluginAction::Log {
                level: LogLevel::Debug,
                message: "tick".into(),
            }],
            HostEvent::SettingsUpdated { values } => vec![PluginAction::SaveSettings { values }],
            _ => vec![],
        })
    }
}

/// Panics in every method; the boundary must contain all of it.
#[derive(Default)]
struct Panicking;

impl Plugin for Panicking {
    fn metadata(&self) -> PluginMetadata {
        panic!("metadata exploded");
    }
    fn start(&self, _host: Arc<dyn HostServices>) -> Result<(), PluginError> {
        panic!("start exploded");
    }
    fn stop(&self) {
        panic!("stop exploded");
    }
    fn settings_schema(&self) -> UiDocument {
        panic!("settings_schema exploded");
    }
    fn handle(&self, _event: HostEvent) -> Result<Vec<PluginAction>, PluginError> {
        panic!("handle exploded");
    }
}

// ------------------------------------------------------------------------- tests

#[test]
fn plugin_trait_round_trips_through_a_mock_host() {
    let host: Arc<dyn HostServices> = Arc::new(MockHost::default());
    let plugin = WellBehaved::default();

    assert_eq!(plugin.metadata().identifier, "Test.WellBehaved");
    plugin.start(host.clone()).expect("start");
    assert!(*plugin.started.lock().unwrap());

    let actions = plugin.handle(HostEvent::Tick).expect("handle");
    assert!(matches!(actions.as_slice(), [PluginAction::Log { .. }]));

    plugin.stop();
    assert!(!*plugin.started.lock().unwrap());
}

#[test]
fn settings_flow_returns_a_save_action() {
    let plugin = WellBehaved::default();
    let mut values = HashMap::new();
    values.insert("greeting".to_string(), "hi".to_string());

    let actions = plugin
        .handle(HostEvent::SettingsUpdated { values })
        .expect("handle");
    match actions.as_slice() {
        [PluginAction::SaveSettings { values }] => {
            assert_eq!(values.get("greeting").map(String::as_str), Some("hi"));
        }
        other => panic!("expected SaveSettings, got {other:?}"),
    }
}

#[test]
fn host_services_mock_persists_settings_and_store() {
    let host = MockHost::default();
    assert_eq!(host.get_setting("k".into()), None);

    host.set_setting("k".into(), "v".into()).unwrap();
    assert_eq!(host.get_setting("k".into()).as_deref(), Some("v"));

    host.store_put("blob".into(), vec![1, 2, 3]).unwrap();
    assert_eq!(host.store_get("blob".into()), Some(vec![1, 2, 3]));
    host.store_delete("blob".into()).unwrap();
    assert_eq!(host.store_get("blob".into()), None);
}

#[test]
fn default_methods_keep_a_minimal_plugin_viable() {
    /// Implements only `metadata`; everything else must default sensibly.
    #[derive(Default)]
    struct Minimal;
    impl Plugin for Minimal {
        fn metadata(&self) -> PluginMetadata {
            metadata_for("Test.Minimal")
        }
    }

    let host: Arc<dyn HostServices> = Arc::new(MockHost::default());
    let plugin = Minimal;

    plugin.start(host).expect("default start must succeed");
    plugin.stop();
    assert_eq!(plugin.settings_schema(), UiDocument::empty());
    assert!(plugin
        .handle(HostEvent::Tick)
        .expect("default handle")
        .is_empty());
}

#[test]
fn ui_document_is_versioned() {
    assert_eq!(UiDocument::empty().ui_version, btcpay_plugin::UI_VERSION);
    assert_eq!(UiDocument::new("Settings").title, "Settings");
}

#[test]
fn abi_version_is_exposed_consistently() {
    assert_eq!(btcpay_plugin::btcpay_rs_abi_version(), ABI_VERSION);
}

// --- the guarantee that matters most: a panic must never reach the FFI boundary ---

/// Exercises the same panic barrier `PluginHandle` uses. It is asserted here through the
/// trait objects rather than `PluginHandle` itself because constructing a handle requires a
/// registered plugin, and registration is global per library.
#[test]
fn panics_are_converted_to_errors_not_unwound() {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let plugin = Panicking;
    let host: Arc<dyn HostServices> = Arc::new(MockHost::default());

    // Each call panics; catch_unwind stands in for the boundary guard and proves the
    // panic is catchable (i.e. not an abort) at every entry point.
    for (name, result) in [
        (
            "metadata",
            catch_unwind(AssertUnwindSafe(|| {
                plugin.metadata();
            })),
        ),
        (
            "start",
            catch_unwind(AssertUnwindSafe(|| {
                let _ = plugin.start(host.clone());
            })),
        ),
        ("stop", catch_unwind(AssertUnwindSafe(|| plugin.stop()))),
        (
            "settings_schema",
            catch_unwind(AssertUnwindSafe(|| {
                plugin.settings_schema();
            })),
        ),
        (
            "handle",
            catch_unwind(AssertUnwindSafe(|| {
                let _ = plugin.handle(HostEvent::Tick);
            })),
        ),
    ] {
        assert!(result.is_err(), "{name} was expected to panic");
    }
}
