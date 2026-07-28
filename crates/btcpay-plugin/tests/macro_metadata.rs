//! Verifies `#[plugin(identifier = ...)]` generates correct metadata, and that the
//! registration it emits actually produces a working `PluginHandle`.
//!
//! Its own test binary because registration is global per library: only one type may carry
//! the attribute.

use btcpay_plugin::prelude::*;
use btcpay_plugin::PluginHandle;

#[derive(Default)]
struct GeneratedMetadata;

// `name`/`version`/`description` deliberately omitted: they must fall back to CARGO_PKG_*.
#[btcpay_plugin::plugin(identifier = "Test.Generated")]
impl Plugin for GeneratedMetadata {}

#[test]
fn metadata_is_generated_from_attribute_and_cargo_manifest() {
    let md = GeneratedMetadata.metadata();

    assert_eq!(md.identifier, "Test.Generated");
    assert_eq!(md.name, env!("CARGO_PKG_NAME"));
    assert_eq!(md.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(md.description, env!("CARGO_PKG_DESCRIPTION"));
}

#[test]
fn btcpay_dependency_defaults_to_the_supported_minimum() {
    let md = GeneratedMetadata.metadata();

    assert_eq!(md.dependencies.len(), 1);
    assert_eq!(md.dependencies[0].identifier, "BTCPayServer");
    assert_eq!(md.dependencies[0].condition, ">=2.4.0");
}

#[test]
fn the_attribute_registers_a_working_plugin_handle() {
    // Exercises the ctor-based registration: no explicit wiring, yet the handle resolves the
    // plugin the host will talk to.
    let handle = PluginHandle::new().expect("a plugin should be registered by the attribute");

    let md = handle
        .metadata()
        .expect("metadata through the FFI boundary");
    assert_eq!(md.identifier, "Test.Generated");
    assert_eq!(handle.abi_version(), btcpay_plugin::ABI_VERSION);

    // Default trait methods must survive the trip through the handle.
    assert_eq!(
        handle.settings_schema().expect("schema"),
        UiDocument::empty()
    );
    assert!(handle.handle(HostEvent::Tick).expect("handle").is_empty());
    handle.stop();
}
