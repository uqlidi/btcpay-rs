//! Reads a plugin's identity out of its compiled library.
//!
//! The library is the source of truth. Anything the tooling needs to know about a plugin
//! (its identifier, version, BTCPay dependency) is whatever the Rust code reports, so the
//! generated C# cannot drift from the plugin it wraps.

use std::ffi::{c_char, CStr};
use std::path::Path;

/// Plugin identity, as reported by a built plugin library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginMetadata {
    pub identifier: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub abi_version: u32,
    pub dependencies: Vec<Dependency>,
}

/// A plugin this one depends on, such as `BTCPayServer >= 2.4.0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub identifier: String,
    pub condition: String,
}

type MetadataFn = unsafe extern "C" fn() -> *mut c_char;
type FreeFn = unsafe extern "C" fn(*mut c_char);

/// Loads `library` and asks it to describe itself.
pub fn read(library: &Path) -> Result<PluginMetadata, String> {
    if !library.exists() {
        return Err(format!(
            "{} does not exist. Build the plugin first with `cargo build --release`.",
            library.display()
        ));
    }

    // SAFETY: we call only the two documented tooling functions, and free the string we are
    // handed with the allocator that produced it.
    unsafe {
        let lib = libloading::Library::new(library)
            .map_err(|e| format!("could not load {}: {e}", library.display()))?;

        let metadata_fn: libloading::Symbol<MetadataFn> =
            lib.get(b"btcpay_rs_metadata_json").map_err(|_| {
                format!(
                    "{} does not export btcpay_rs_metadata_json. It was probably not built \
                     against btcpay-plugin, or is not a btcpay-rs plugin.",
                    library.display()
                )
            })?;
        let free_fn: libloading::Symbol<FreeFn> =
            lib.get(b"btcpay_rs_string_free").map_err(|e| {
                format!(
                    "{} is missing btcpay_rs_string_free: {e}",
                    library.display()
                )
            })?;

        let ptr = metadata_fn();
        if ptr.is_null() {
            return Err(format!(
                "{} reported no plugin. Is #[btcpay_plugin::plugin] applied to your Plugin impl?",
                library.display()
            ));
        }

        let json = CStr::from_ptr(ptr).to_string_lossy().into_owned();
        free_fn(ptr);

        parse(&json)
    }
}

/// Parses the metadata document emitted by `btcpay_rs_metadata_json`.
///
/// Deliberately minimal rather than a JSON dependency: this parses one document whose exact
/// shape we also generate, a few fields deep.
fn parse(json: &str) -> Result<PluginMetadata, String> {
    let field = |name: &str| -> Result<String, String> {
        string_field(json, name)
            .ok_or_else(|| format!("plugin metadata is missing `{name}`: {json}"))
    };

    let abi_version = number_field(json, "abiVersion")
        .ok_or_else(|| format!("plugin metadata is missing `abiVersion`: {json}"))?;

    Ok(PluginMetadata {
        identifier: field("identifier")?,
        name: field("name")?,
        version: field("version")?,
        description: field("description")?,
        abi_version,
        dependencies: parse_dependencies(json),
    })
}

fn parse_dependencies(json: &str) -> Vec<Dependency> {
    let Some(start) = json.find(r#""dependencies":["#) else {
        return Vec::new();
    };
    let rest = &json[start + r#""dependencies":["#.len()..];
    let Some(end) = rest.find(']') else {
        return Vec::new();
    };

    rest[..end]
        .split("},")
        .filter_map(|entry| {
            Some(Dependency {
                identifier: string_field(entry, "identifier")?,
                condition: string_field(entry, "condition")?,
            })
        })
        .collect()
}

/// Extracts `"name":"value"`, honouring backslash escapes so an escaped quote does not end
/// the value early.
pub(crate) fn string_field(json: &str, name: &str) -> Option<String> {
    let key = format!("\"{name}\":\"");
    let start = json.find(&key)? + key.len();

    let mut value = String::new();
    let mut chars = json[start..].chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(value),
            '\\' => match chars.next()? {
                'n' => value.push('\n'),
                'r' => value.push('\r'),
                't' => value.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    let code = u32::from_str_radix(&hex, 16).ok()?;
                    value.push(char::from_u32(code)?);
                }
                other => value.push(other),
            },
            c => value.push(c),
        }
    }
    None
}

fn number_field(json: &str, name: &str) -> Option<u32> {
    let key = format!("\"{name}\":");
    let start = json.find(&key)? + key.len();
    let digits: String = json[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"identifier":"Acme.Plugins.Thing","name":"Thing","version":"1.2.3","description":"Does things.","abiVersion":2,"dependencies":[{"identifier":"BTCPayServer","condition":">=2.4.0"}]}"#;

    #[test]
    fn parses_a_full_metadata_document() {
        let md = parse(SAMPLE).expect("should parse");

        assert_eq!(md.identifier, "Acme.Plugins.Thing");
        assert_eq!(md.name, "Thing");
        assert_eq!(md.version, "1.2.3");
        assert_eq!(md.abi_version, 2);
        assert_eq!(md.dependencies.len(), 1);
        assert_eq!(md.dependencies[0].identifier, "BTCPayServer");
        assert_eq!(md.dependencies[0].condition, ">=2.4.0");
    }

    #[test]
    fn escaped_quotes_do_not_truncate_a_value() {
        let json = r#"{"identifier":"A","name":"say \"hi\"","version":"1","description":"d","abiVersion":1,"dependencies":[]}"#;
        assert_eq!(parse(json).unwrap().name, r#"say "hi""#);
    }

    #[test]
    fn several_dependencies_are_all_returned() {
        let json = r#"{"identifier":"A","name":"n","version":"1","description":"d","abiVersion":1,"dependencies":[{"identifier":"BTCPayServer","condition":">=2.4.0"},{"identifier":"Other.Plugin","condition":">=1.0"}]}"#;
        let deps = parse(json).unwrap().dependencies;

        assert_eq!(deps.len(), 2);
        assert_eq!(deps[1].identifier, "Other.Plugin");
    }

    #[test]
    fn a_missing_field_is_reported_rather_than_defaulted() {
        let json = r#"{"identifier":"A","version":"1","description":"d","abiVersion":1,"dependencies":[]}"#;
        let err = parse(json).expect_err("missing name should fail");

        assert!(
            err.contains("name"),
            "error should name the missing field: {err}"
        );
    }
}
