//! External tools the pipeline needs: the binding generator and a BTCPay Server checkout.
//!
//! Both are fetched into the shared cache on first use. Both are slow the first time, so
//! every path here says what it is doing before it blocks.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::workspace::{cache_root, run, run_streaming, UNIFFI_BINDGEN_CS_TAG};

/// Ensures the pinned `uniffi-bindgen-cs` exists, returning its path.
///
/// The version is pinned rather than taking whatever is installed: the generator targets one
/// specific uniffi release, and a mismatch yields bindings that compile but do not match the
/// library's actual ABI.
pub fn ensure_bindgen() -> Result<PathBuf, String> {
    let bin_dir = cache_root().join("bin");
    let installed = bin_dir.join("uniffi-bindgen-cs");

    if installed.exists() && reports_expected_version(&installed) {
        return Ok(installed);
    }

    // A matching version already on PATH is fine, and saves a multi-minute build.
    if let Some(found) = on_path_with_expected_version() {
        return Ok(found);
    }

    println!("Installing uniffi-bindgen-cs {UNIFFI_BINDGEN_CS_TAG} (a few minutes, once).");
    std::fs::create_dir_all(&bin_dir)
        .map_err(|e| format!("could not create {}: {e}", bin_dir.display()))?;

    run_streaming(
        "cargo install uniffi-bindgen-cs",
        Command::new("cargo")
            .args([
                "install",
                "uniffi-bindgen-cs",
                "--git",
                "https://github.com/NordSecurity/uniffi-bindgen-cs",
                "--tag",
                UNIFFI_BINDGEN_CS_TAG,
                "--root",
                &cache_root().to_string_lossy(),
            ])
            .env("CARGO_TERM_COLOR", "always"),
    )?;

    if !installed.exists() {
        return Err(format!(
            "uniffi-bindgen-cs was installed but is not at {}",
            installed.display()
        ));
    }
    Ok(installed)
}

fn expected_version_fragment() -> &'static str {
    // The tag is "v0.11.0+v0.31.0"; the binary reports "0.11.0+v0.31.0".
    UNIFFI_BINDGEN_CS_TAG.trim_start_matches('v')
}

fn reports_expected_version(binary: &Path) -> bool {
    Command::new(binary)
        .arg("--version")
        .output()
        .ok()
        .is_some_and(|out| {
            String::from_utf8_lossy(&out.stdout).contains(expected_version_fragment())
        })
}

fn on_path_with_expected_version() -> Option<PathBuf> {
    let candidate = PathBuf::from("uniffi-bindgen-cs");
    reports_expected_version(&candidate).then_some(candidate)
}

/// Ensures a BTCPay Server checkout at `tag` exists, returning the path to its main project.
///
/// `BTCPayServer.Abstractions` is not published on NuGet, so the generated C# has to compile
/// against BTCPay's source. One shallow checkout per tag is shared by every plugin.
pub fn ensure_btcpayserver(tag: &str) -> Result<PathBuf, String> {
    let checkout = cache_root().join("btcpayserver").join(tag);
    let project = checkout.join("BTCPayServer/BTCPayServer.csproj");

    if project.exists() {
        return Ok(project);
    }

    println!("Fetching BTCPay Server {tag} (about 40 MB, once per version).");
    println!("  BTCPayServer.Abstractions is not on NuGet, so the C# builds against source.");

    if checkout.exists() {
        // A previous run was interrupted, leaving a partial clone that git will not clone into.
        std::fs::remove_dir_all(&checkout)
            .map_err(|e| format!("could not clear {}: {e}", checkout.display()))?;
    }
    if let Some(parent) = checkout.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }

    run_streaming(
        "git clone btcpayserver",
        Command::new("git").args([
            "clone",
            "--depth",
            "1",
            "--branch",
            tag,
            "https://github.com/btcpayserver/btcpayserver.git",
            &checkout.to_string_lossy(),
        ]),
    )
    .map_err(|e| format!("{e}\nIs `{tag}` a real BTCPay Server tag? Check btcpay.toml."))?;

    if !project.exists() {
        return Err(format!(
            "cloned BTCPay Server {tag}, but {} is missing. The tag may predate the current \
             repository layout.",
            project.display()
        ));
    }
    Ok(project)
}

/// Builds BTCPay's plugin packer once per version and returns the assembly to invoke.
///
/// The packer is invoked as a built assembly rather than through `dotnet run`, which mangles
/// path arguments: a directory argument arrives split, with its last segment as the next
/// argument. Building it once is also faster on repeat packages.
pub fn ensure_plugin_packer(btcpayserver_project: &Path, tag: &str) -> Result<PathBuf, String> {
    let output_dir = cache_root().join("packer").join(tag);
    let assembly = output_dir.join("BTCPayServer.PluginPacker.dll");

    if assembly.exists() {
        return Ok(assembly);
    }

    println!("Building the plugin packer (once per BTCPay version)");
    let project = plugin_packer_project(btcpayserver_project)?;
    run_streaming(
        "dotnet build BTCPayServer.PluginPacker",
        Command::new("dotnet")
            .arg("build")
            .arg(&project)
            .args(["-c", "Release", "--nologo", "-v", "quiet"])
            .arg("-o")
            .arg(&output_dir),
    )?;

    if !assembly.exists() {
        return Err(format!("the packer did not produce {}", assembly.display()));
    }
    Ok(assembly)
}

/// Path to the plugin packer project within a checkout.
fn plugin_packer_project(btcpayserver_project: &Path) -> Result<PathBuf, String> {
    let checkout = btcpayserver_project
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| "unexpected btcpayserver checkout layout".to_string())?;

    let packer = checkout.join("BTCPayServer.PluginPacker/BTCPayServer.PluginPacker.csproj");
    if !packer.exists() {
        return Err(format!("{} is missing from the checkout", packer.display()));
    }
    Ok(packer)
}

/// Confirms a `dotnet` SDK is available, with a message that says what to install.
pub fn ensure_dotnet() -> Result<(), String> {
    run("dotnet --version", Command::new("dotnet").arg("--version")).map_err(|_| {
        "the .NET SDK 10.0 is required to build the plugin's C# wrapper, and `dotnet` was not \
         found. Install it from https://dotnet.microsoft.com/download"
            .to_string()
    })?;
    Ok(())
}
