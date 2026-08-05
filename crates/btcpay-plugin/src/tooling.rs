//! A minimal C ABI for build tooling, separate from the uniffi contract.
//!
//! `cargo btcpay` generates the C# shim from whatever the plugin actually reports, rather
//! than from a config file that could disagree with it. Reading that through uniffi would
//! mean reimplementing its marshalling inside the tool; these two functions are a stable,
//! trivially callable alternative that will not change shape as the contract evolves.
//!
//! Not part of the plugin API. Nothing at runtime calls these.

use std::ffi::{c_char, CString};

use crate::handle::plugin_factory;

/// Returns the registered plugin's metadata as a JSON string, or null if no plugin is
/// registered or serialisation fails.
///
/// # Safety
///
/// The caller owns the returned pointer and must release it with
/// [`btcpay_rs_string_free`]. Reading it after freeing is undefined behaviour.
#[no_mangle]
pub extern "C" fn btcpay_rs_metadata_json() -> *mut c_char {
    // A panic crossing this boundary would abort the build tool with no useful message.
    let json = std::panic::catch_unwind(|| {
        let factory = plugin_factory()?;
        let metadata = factory().metadata();

        let dependencies = metadata
            .dependencies
            .iter()
            .map(|d| {
                format!(
                    r#"{{"identifier":{},"condition":{}}}"#,
                    quote(&d.identifier),
                    quote(&d.condition)
                )
            })
            .collect::<Vec<_>>()
            .join(",");

        // Whether the plugin describes a settings page, so the build can skip generating a
        // controller and a menu entry for a plugin that has nothing to configure.
        //
        // Asked without a host, since none exists at build time. A plugin that describes
        // sections only once configured will look as though it has no page; such a plugin
        // should return its form unconditionally and fill in values when it can.
        let has_settings_page =
            btcpay_ui::Document::from_json(&factory().settings_schema().document_json)
                .map(|document| !document.is_empty())
                .unwrap_or(false);

        Some(format!(
            r#"{{"identifier":{},"name":{},"version":{},"description":{},"abiVersion":{},"hasSettingsPage":{},"dependencies":[{}]}}"#,
            quote(&metadata.identifier),
            quote(&metadata.name),
            quote(&metadata.version),
            quote(&metadata.description),
            crate::ABI_VERSION,
            has_settings_page,
            dependencies
        ))
    });

    match json {
        Ok(Some(s)) => match CString::new(s) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        _ => std::ptr::null_mut(),
    }
}

/// Releases a string returned by [`btcpay_rs_metadata_json`].
///
/// # Safety
///
/// `ptr` must be null, or a pointer returned by [`btcpay_rs_metadata_json`] and not yet
/// freed.
#[no_mangle]
pub unsafe extern "C" fn btcpay_rs_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

/// Serialises a string as a JSON literal.
///
/// Hand-written to keep a JSON dependency out of every plugin's dependency tree for the sake
/// of one build-time function. Covers what RFC 8259 requires: the two mandatory escapes and
/// all control characters.
fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::quote;

    #[test]
    fn strings_are_escaped_for_json() {
        assert_eq!(quote("plain"), r#""plain""#);
        assert_eq!(quote(r#"say "hi""#), r#""say \"hi\"""#);
        assert_eq!(quote(r"back\slash"), r#""back\\slash""#);
        assert_eq!(quote("line\nbreak"), r#""line\nbreak""#);
        // Control characters have no short escape and must use the \u form.
        assert_eq!(quote("bell\u{7}"), r#""bell\u0007""#);
    }
}
