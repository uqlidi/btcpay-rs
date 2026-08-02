//! `btcpay.toml`: per-plugin build settings.
//!
//! Deliberately small. Identity (identifier, name, version, dependencies) is *not* here:
//! it lives in the Rust source and is read back from the compiled library, so there is no
//! second copy to drift. This file carries only what the plugin code cannot know, such as
//! which BTCPay checkout to compile the C# against.

use std::path::Path;

use serde::Deserialize;

/// Contents of a plugin's `btcpay.toml`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub build: Build,
}

/// Build settings.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Build {
    /// BTCPay Server tag to compile the generated C# against.
    ///
    /// `BTCPayServer.Abstractions` is not published on NuGet, so the shim must be built
    /// against a source checkout. The tooling fetches and caches this tag.
    #[serde(default = "default_btcpay_tag")]
    pub btcpay_tag: String,

    /// Runtime identifiers to build the native library for.
    ///
    /// linux-x64 is the only tier-1 target: BTCPay deployments are overwhelmingly Linux
    /// Docker. Others are for local development.
    #[serde(default = "default_targets")]
    pub targets: Vec<String>,
}

impl Default for Build {
    fn default() -> Self {
        Self {
            btcpay_tag: default_btcpay_tag(),
            targets: default_targets(),
        }
    }
}

fn default_btcpay_tag() -> String {
    "v2.4.1".to_string()
}

fn default_targets() -> Vec<String> {
    vec!["linux-x64".to_string()]
}

impl Config {
    /// Reads `btcpay.toml` from a plugin directory, falling back to defaults when absent.
    pub fn load(dir: &Path) -> Result<Self, String> {
        let path = dir.join("btcpay.toml");
        if !path.exists() {
            return Ok(Self { build: Build::default() });
        }

        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("could not read {}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("{} is not valid: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_file_yields_usable_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load(dir.path()).expect("defaults should apply");

        assert_eq!(config.build.btcpay_tag, "v2.4.1");
        assert_eq!(config.build.targets, vec!["linux-x64"]);
    }

    #[test]
    fn settings_are_read_and_unset_ones_defaulted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("btcpay.toml"),
            "[build]\nbtcpay_tag = \"v2.5.0\"\n",
        )
        .unwrap();

        let config = Config::load(dir.path()).unwrap();

        assert_eq!(config.build.btcpay_tag, "v2.5.0");
        assert_eq!(config.build.targets, vec!["linux-x64"], "unset keys keep their default");
    }

    #[test]
    fn a_misspelled_key_is_rejected_rather_than_ignored() {
        // Silently ignoring `btcpay_version` would leave the plugin building against the
        // wrong BTCPay with no indication why.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("btcpay.toml"),
            "[build]\nbtcpay_version = \"v2.5.0\"\n",
        )
        .unwrap();

        let err = Config::load(dir.path()).expect_err("unknown key should be rejected");
        assert!(err.contains("btcpay_version"), "error should name the key: {err}");
    }
}
