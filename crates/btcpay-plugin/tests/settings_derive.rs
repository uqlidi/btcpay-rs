//! What `#[derive(BtcpaySettings)]` generates.
//!
//! The point of the derive is that the form, the storage keys and the parsing cannot disagree,
//! so these check that they actually agree rather than that the macro compiles.

use std::collections::HashMap;

use btcpay_plugin::prelude::*;
use btcpay_plugin::ui::{FieldKind, Section};
use btcpay_plugin::BtcpaySettings;

#[derive(Debug, Clone, Default, PartialEq, BtcpaySettings)]
struct Settings {
    #[setting(label = "API key", secret, required)]
    api_key: String,

    #[setting(label = "Poll interval", help = "In seconds.", min = 5, max = 60)]
    poll_secs: u32,

    #[setting(label = "Enabled")]
    enabled: bool,

    #[setting(key = "custom_storage_key")]
    renamed: String,
}

fn fields_of(form: Form) -> Vec<btcpay_plugin::ui::Field> {
    match Section::from(form) {
        Section::Form { fields, .. } => fields,
        other => panic!("expected a form, got {other:?}"),
    }
}

#[test]
fn each_field_becomes_the_input_its_type_calls_for() {
    let fields = fields_of(Settings::default().form());

    assert_eq!(fields.len(), 4);
    assert!(
        matches!(fields[0].kind, FieldKind::Password),
        "String + secret"
    );
    assert!(matches!(fields[1].kind, FieldKind::Number { .. }), "u32");
    assert!(matches!(fields[2].kind, FieldKind::Toggle), "bool");
    assert!(matches!(fields[3].kind, FieldKind::Text { .. }), "String");
}

#[test]
fn attributes_reach_the_form() {
    let fields = fields_of(Settings::default().form());

    assert_eq!(fields[0].label, "API key");
    assert!(fields[0].required);
    assert_eq!(fields[1].help.as_deref(), Some("In seconds."));
    assert!(matches!(
        fields[1].kind,
        FieldKind::Number {
            min: Some(5),
            max: Some(60)
        }
    ));
}

#[test]
fn a_field_without_a_label_gets_a_readable_one() {
    let fields = fields_of(Settings::default().form());

    // Derived from the field name, so a form is never blank-labelled by omission.
    assert_eq!(fields[3].label, "Renamed");
}

#[test]
fn a_secret_never_carries_its_value_into_the_form() {
    // The single most important property here: a stored key must not reach the browser.
    let settings = Settings {
        api_key: "super-secret".into(),
        ..Default::default()
    };

    let fields = fields_of(settings.form());

    assert_eq!(fields[0].value, None);
}

#[test]
fn values_round_trip_through_storage_and_back() {
    let settings = Settings {
        api_key: "key".into(),
        poll_secs: 30,
        enabled: true,
        renamed: "value".into(),
    };

    let stored = settings.to_values();
    let parsed = Settings::from_values(&stored).expect("its own output should parse");

    assert_eq!(parsed, settings);
}

#[test]
fn the_storage_key_can_be_renamed_independently_of_the_field() {
    let settings = Settings {
        renamed: "value".into(),
        ..Default::default()
    };

    let stored = settings.to_values();

    assert_eq!(
        stored.get("custom_storage_key").map(String::as_str),
        Some("value")
    );
    assert!(!stored.contains_key("renamed"));
}

#[test]
fn a_submission_is_validated_with_the_rules_the_form_declares() {
    let out_of_range = HashMap::from([("poll_secs".to_string(), "1".to_string())]);
    let err = Settings::from_values(&out_of_range).unwrap_err();
    let message = format!("{err}");
    assert!(
        message.contains("Poll interval"),
        "should name the field: {message}"
    );
    assert!(message.contains('5'), "should state the bound: {message}");

    let not_a_number = HashMap::from([("poll_secs".to_string(), "soon".to_string())]);
    assert!(Settings::from_values(&not_a_number).is_err());

    let empty_required = HashMap::from([("api_key".to_string(), "  ".to_string())]);
    // A secret is exempt: empty means "keep the stored one", because the browser was never
    // given the current value.
    assert!(Settings::from_values(&empty_required).is_ok());
}

#[test]
fn absent_values_keep_their_defaults_rather_than_being_cleared() {
    // A partial submission must not wipe fields it did not mention.
    let parsed = Settings::from_values(&HashMap::new()).unwrap();

    assert_eq!(parsed, Settings::default());
}

#[test]
fn a_toggle_is_off_unless_it_says_true() {
    let on = HashMap::from([("enabled".to_string(), "true".to_string())]);
    let off = HashMap::from([("enabled".to_string(), "false".to_string())]);

    assert!(Settings::from_values(&on).unwrap().enabled);
    assert!(!Settings::from_values(&off).unwrap().enabled);
}

// ---------------------------------------------------------------- dropdowns

#[derive(Debug, Clone, Copy, PartialEq, Default, btcpay_plugin::BtcpayChoice)]
enum Network {
    #[default]
    #[choice(label = "Mainnet")]
    Main,
    #[choice(label = "Testnet")]
    Test,
    // No label: falls back to a readable form of the variant name.
    CoreRpc,
    // An explicit stored value, so it can be renamed without breaking storage.
    #[choice(value = "rt", label = "Regtest")]
    Regtest,
}

#[derive(Debug, Clone, Default, PartialEq, BtcpaySettings)]
struct WithChoice {
    #[setting(label = "Network")]
    network: Network,
}

#[test]
fn a_choice_type_renders_as_a_dropdown_with_options_from_the_type() {
    let fields = fields_of(WithChoice::default().form());

    match &fields[0].kind {
        FieldKind::Select { options } => {
            assert_eq!(options.len(), 4);
            assert_eq!(options[0].value, "main");
            assert_eq!(options[0].label, "Mainnet");
            // Defaults derived from the variant name.
            assert_eq!(options[2].value, "core_rpc");
            assert_eq!(options[2].label, "Core rpc");
            // And an explicitly renamed stored value.
            assert_eq!(options[3].value, "rt");
            assert_eq!(options[3].label, "Regtest");
        }
        other => panic!("expected a select, got {other:?}"),
    }
}

#[test]
fn the_current_choice_is_the_selected_value() {
    let settings = WithChoice {
        network: Network::Test,
    };

    let fields = fields_of(settings.form());

    assert_eq!(fields[0].value.as_deref(), Some("test"));
}

#[test]
fn a_choice_round_trips_through_storage() {
    let settings = WithChoice {
        network: Network::Regtest,
    };

    let parsed = WithChoice::from_values(&settings.to_values()).unwrap();

    assert_eq!(parsed, settings);
    // Stored under the renamed value, not the variant name.
    assert_eq!(
        settings.to_values().get("network").map(String::as_str),
        Some("rt")
    );
}

#[test]
fn a_submitted_value_that_is_not_an_option_is_refused() {
    // The form offered a fixed set, so anything else means a tampered post.
    let values = HashMap::from([("network".to_string(), "banana".to_string())]);

    let err = WithChoice::from_values(&values).unwrap_err();

    assert!(
        format!("{err}").contains("Network"),
        "should name the field"
    );
}

#[test]
fn a_stored_value_that_is_no_longer_an_option_falls_back_when_loading() {
    // An option removed in a newer version must not stop the plugin starting.
    struct Host(std::sync::Mutex<HashMap<String, String>>);

    impl HostServices for Host {
        fn data_dir(&self) -> String {
            std::env::temp_dir().to_string_lossy().into_owned()
        }
        fn get_setting(&self, key: String) -> Option<String> {
            self.0.lock().unwrap().get(&key).cloned()
        }
        fn set_setting(&self, _: String, _: String) -> Result<(), HostError> {
            Ok(())
        }
        fn store_get(&self, _: String) -> Option<Vec<u8>> {
            None
        }
        fn store_put(&self, _: String, _: Vec<u8>) -> Result<(), HostError> {
            Ok(())
        }
        fn store_delete(&self, _: String) -> Result<(), HostError> {
            Ok(())
        }
        fn log(&self, _: LogLevel, _: String) {}
        fn emit_notification(&self, _: Notification) -> Result<(), HostError> {
            Ok(())
        }
        fn send_webhook(&self, _: WebhookRequest) -> Result<(), HostError> {
            Ok(())
        }
    }

    let host = Host(std::sync::Mutex::new(HashMap::from([(
        "network".to_string(),
        "a_removed_option".to_string(),
    )])));

    let loaded = WithChoice::load(&host);

    assert_eq!(loaded.network, Network::default());
}

#[test]
fn a_save_that_does_not_retype_a_secret_keeps_the_stored_one() {
    // The whole point of `update`. The host omits an untouched secret from the submission so
    // that a stored password survives a save; `from_values` cannot honour that, because it
    // starts from `Default` and has nothing to keep. Parsing onto the current settings does.
    let mut settings = Settings {
        api_key: "stored-key".into(),
        poll_secs: 30,
        ..Default::default()
    };

    let submission = HashMap::from([("poll_secs".to_string(), "45".to_string())]);
    settings.update(&submission).unwrap();

    assert_eq!(settings.api_key, "stored-key", "the secret must survive");
    assert_eq!(settings.poll_secs, 45, "the edited field must change");
}

#[test]
fn from_values_resets_an_omitted_secret_which_is_why_update_exists() {
    // Documents the trap rather than the fix, so that anyone who reaches for `from_values` in a
    // struct with a secret sees why it is the wrong tool.
    let submission = HashMap::from([("poll_secs".to_string(), "45".to_string())]);

    let parsed = Settings::from_values(&submission).unwrap();

    assert_eq!(parsed.api_key, "", "no stored value was available to keep");
}

#[test]
fn update_applies_the_same_rules_from_values_does() {
    // The two share their generated parsing, so a rule cannot hold in one and not the other.
    let mut settings = Settings::default();
    let out_of_range = HashMap::from([("poll_secs".to_string(), "1".to_string())]);

    let err = settings.update(&out_of_range).unwrap_err();

    assert!(format!("{err}").contains("Poll interval"));
}

#[test]
fn update_leaves_untouched_fields_alone() {
    let mut settings = Settings {
        api_key: "key".into(),
        poll_secs: 30,
        enabled: true,
        renamed: "value".into(),
    };
    let before = settings.clone();

    settings.update(&HashMap::new()).unwrap();

    assert_eq!(settings, before);
}
