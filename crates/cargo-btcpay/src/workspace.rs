//! Where things live: caches shared between plugins, and build output within one plugin.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Pinned toolchain versions.
///
/// The uniffi crate and the C# generator must move together: the generator targets one
/// specific uniffi release, and a mismatch produces bindings that do not match the library.
pub const UNIFFI_BINDGEN_CS_TAG: &str = "v0.11.0+v0.31.0";

/// Cache shared across every plugin on this machine.
///
/// Holds a btcpayserver checkout per tag and the pinned binding generator. These are large
/// and identical for everyone, so they do not belong in a plugin's own `target/`.
pub fn cache_root() -> PathBuf {
    std::env::var_os("BTCPAY_RS_CACHE")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CACHE_HOME").map(|c| PathBuf::from(c).join("btcpay-rs")))
        .or_else(|| dirs_home().map(|h| h.join(".cache/btcpay-rs")))
        .unwrap_or_else(|| PathBuf::from(".btcpay-rs-cache"))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Build output for one plugin, all under `target/` so `cargo clean` removes it.
pub struct Layout {
    pub root: PathBuf,
}

impl Layout {
    pub fn new(plugin_dir: &Path) -> Self {
        Self { root: plugin_dir.join("target/btcpay") }
    }

    /// The generated C# project wrapping this plugin.
    pub fn shim_dir(&self) -> PathBuf {
        self.root.join("shim")
    }

    /// The host assemblies, materialised from the CLI so a plugin needs no checkout of
    /// btcpay-rs itself.
    pub fn host_dir(&self) -> PathBuf {
        self.root.join("host")
    }

    /// Where `dotnet publish` puts the assembled plugin, before packing.
    pub fn publish_dir(&self) -> PathBuf {
        self.root.join("publish")
    }
}

/// Runs a command, returning its stdout, or a message naming what failed.
pub fn run(what: &str, command: &mut Command) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|e| format!("could not run {what}: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Build failures put the useful detail in one stream or the other depending on the
        // tool, so show whichever has content rather than guessing.
        let detail = if stderr.trim().is_empty() { stdout } else { stderr };
        return Err(format!("{what} failed:\n{}", tail(&detail, 25)));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Runs a command with its output streamed to the terminal, for long steps where silence
/// would look like a hang.
pub fn run_streaming(what: &str, command: &mut Command) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|e| format!("could not run {what}: {e}"))?;

    if !status.success() {
        return Err(format!("{what} failed"));
    }
    Ok(())
}

/// Keeps the last `lines` lines, where the actual error usually is.
fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_output_stays_under_target_so_cargo_clean_removes_it() {
        let layout = Layout::new(Path::new("/plugins/demo"));

        assert!(layout.shim_dir().starts_with("/plugins/demo/target"));
        assert!(layout.host_dir().starts_with("/plugins/demo/target"));
        assert!(layout.publish_dir().starts_with("/plugins/demo/target"));
    }

    #[test]
    fn the_cache_location_can_be_overridden() {
        // Needed by CI, which caches this directory between runs.
        temp_env_var("BTCPAY_RS_CACHE", "/tmp/explicit", || {
            assert_eq!(cache_root(), PathBuf::from("/tmp/explicit"));
        });
    }

    #[test]
    fn only_the_end_of_a_long_failure_is_shown() {
        let long: String = (1..=60).map(|i| format!("line {i}\n")).collect();
        let shown = tail(&long, 25);

        assert!(shown.contains("line 60"), "the actual error is at the end");
        assert!(!shown.contains("line 1\n"), "noise from the start is dropped");
    }

    /// Sets an environment variable for the duration of `f`.
    ///
    /// Tests in a binary share a process, so this restores the previous value rather than
    /// leaving it set for whatever runs next.
    fn temp_env_var(key: &str, value: &str, f: impl FnOnce()) {
        let previous = std::env::var_os(key);
        unsafe { std::env::set_var(key, value) };
        f();
        match previous {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
    }
}
