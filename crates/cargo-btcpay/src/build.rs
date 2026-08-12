//! `cargo btcpay build` and `package`: from Rust source to an installable `.btcpay`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;
use crate::metadata::PluginMetadata;
use crate::workspace::{run, run_streaming, Layout};
use crate::{host, metadata, native, shim, toolchain};

/// Everything produced by a successful build.
pub struct Built {
    pub metadata: PluginMetadata,
    pub native_library: PathBuf,
    pub shim_project: PathBuf,
    pub btcpayserver_project: PathBuf,
    pub layout: Layout,
}

/// Compiles the plugin and the C# that wraps it.
pub fn build(plugin_dir: &Path, release: bool) -> Result<Built, String> {
    // Absolute from here on. Several steps run with a different working directory, and a
    // relative path silently resolves against the wrong root: the binding generator ignores
    // a config file it cannot find rather than reporting it.
    let plugin_dir = &plugin_dir
        .canonicalize()
        .map_err(|e| format!("could not resolve {}: {e}", plugin_dir.display()))?;

    let config = Config::load(plugin_dir)?;
    let layout = Layout::new(plugin_dir);

    toolchain::ensure_dotnet()?;

    // 1. The Rust library. Must not be stripped yet: the binding generator reads metadata
    //    from its symbols. Stripping happens at packaging time.
    println!("Building the plugin library");
    let native_library = cargo_build(plugin_dir, release)?;

    // 2. Native dependencies, before anything else is built. A plugin needing a library
    //    BTCPay does not provide would install and then fail to load, so it is better to
    //    refuse to build it than to hand an operator a broken package.
    let missing = native::unsatisfied_dependencies(&native_library)?;
    if !missing.is_empty() {
        return Err(native::explain(&missing));
    }

    // 3. What the plugin says it is. Everything downstream is derived from this, so the
    //    generated C# cannot disagree with the library it wraps.
    let md = metadata::read(&native_library)?;
    println!(
        "  {} {} (ABI {})",
        md.identifier, md.version, md.abi_version
    );

    // 4. Host assemblies, written from this binary so no checkout of btcpay-rs is needed.
    let host_projects = host::materialise(&layout)?;

    // 5. Bindings, generated from the library just built.
    println!("Generating bindings");
    generate_bindings(plugin_dir, &native_library, &host_projects.bindings_file)?;

    // 6. The plugin's own C# project.
    let shim_dir = layout.shim_dir();
    shim::generate(&md, &config, &shim_dir)?;
    let shim_project = shim_dir.join(format!("{}.csproj", md.identifier));

    // 7. BTCPay itself, for Abstractions.
    let btcpayserver_project = toolchain::ensure_btcpayserver(&config.build.btcpay_tag)?;

    println!("Building the plugin wrapper");
    run_streaming(
        "dotnet build",
        Command::new("dotnet")
            .arg("build")
            .arg(&shim_project)
            .args(["-c", if release { "Release" } else { "Debug" }])
            .args(msbuild_properties(
                &host_projects,
                &btcpayserver_project,
                &native_library,
            ))
            .args(["--nologo", "-v", "quiet"]),
    )?;

    Ok(Built {
        metadata: md,
        native_library,
        shim_project,
        btcpayserver_project,
        layout,
    })
}

/// Builds the plugin and assembles an installable `.btcpay`.
pub fn package(plugin_dir: &Path, out_dir: &Path) -> Result<PathBuf, String> {
    let built = build(plugin_dir, true)?;
    let publish_dir = built.layout.publish_dir();

    println!("Publishing");
    let _ = std::fs::remove_dir_all(&publish_dir);
    let host_projects = host::materialise(&built.layout)?;
    run_streaming(
        "dotnet publish",
        Command::new("dotnet")
            .arg("publish")
            .arg(&built.shim_project)
            .args(["-c", "Release"])
            .arg("-o")
            .arg(&publish_dir)
            .args(msbuild_properties(
                &host_projects,
                &built.btcpayserver_project,
                &built.native_library,
            ))
            .args(["--nologo", "-v", "quiet"]),
    )?;

    verify_publish_output(&publish_dir, &built.metadata)?;
    strip_native_library(&publish_dir)?;

    // BTCPay's own packer, rather than a reimplementation: it instantiates the plugin type
    // to build the metadata sidecar, so the result matches what BTCPay expects by
    // construction. Note that this is why the generated C# must not need the native library
    // to report its identity.
    println!("Packing");
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("could not create {}: {e}", out_dir.display()))?;
    let config = Config::load(plugin_dir)?;
    let packer =
        toolchain::ensure_plugin_packer(&built.btcpayserver_project, &config.build.btcpay_tag)?;
    run_streaming(
        "BTCPayServer.PluginPacker",
        Command::new("dotnet")
            .arg(&packer)
            .arg(&publish_dir)
            .arg(&built.metadata.identifier)
            .arg(out_dir),
    )?;

    let package = out_dir
        .join(&built.metadata.identifier)
        .join(format!("{}.0", built.metadata.version))
        .join(format!("{}.btcpay", built.metadata.identifier));

    // The packer derives the version directory from the assembly version, which may render
    // differently from the crate version. Find the file rather than assume the path.
    let package = if package.exists() {
        package
    } else {
        find_package(
            &out_dir.join(&built.metadata.identifier),
            &built.metadata.identifier,
        )?
    };

    let size = std::fs::metadata(&package).map(|m| m.len()).unwrap_or(0);
    println!();
    println!("Packaged {} ({} KB)", package.display(), size / 1024);
    warn_if_large(size);
    Ok(package)
}

fn cargo_build(plugin_dir: &Path, release: bool) -> Result<PathBuf, String> {
    let mut command = Command::new("cargo");
    command.arg("build").current_dir(plugin_dir);
    if release {
        command.arg("--release");
    }
    run_streaming("cargo build", &mut command)?;

    let profile = if release { "release" } else { "debug" };
    // Ask cargo where output goes rather than assuming `<plugin>/target`. A plugin that is a
    // workspace member builds into the workspace root's target directory instead.
    let library = cargo_target_dir(plugin_dir)?
        .join(profile)
        .join("libbtcpay_plugin_native.so");

    if !library.exists() {
        return Err(format!(
            "expected {} after building. A btcpay-rs plugin must set \
             [lib] crate-type = [\"cdylib\"] and name = \"btcpay_plugin_native\" in Cargo.toml.",
            library.display()
        ));
    }
    Ok(library)
}

/// Kestrel's default maximum request body, which BTCPay does not override.
///
/// It bounds the admin UI's plugin upload. Extracting a package into the plugin directory is
/// not affected, which is how a deployment tool or `dev/run-btcpay.sh` installs one, but an
/// operator uploading through the browser is.
const UPLOAD_LIMIT_BYTES: u64 = 30_000_000;

/// Warns when a package is close to, or past, what an operator could upload.
///
/// A warning rather than an error: the limit applies only to the upload path, and a plugin
/// installed by other means works regardless.
fn warn_if_large(size: u64) {
    if size > UPLOAD_LIMIT_BYTES {
        println!();
        println!(
            "warning: at {} MB this exceeds the {} MB request limit BTCPay's plugin upload \n\
             inherits from Kestrel, so an operator cannot install it through the admin UI.\n\
             Extracting it into the plugin directory still works.",
            size / 1_000_000,
            UPLOAD_LIMIT_BYTES / 1_000_000
        );
    } else if size > UPLOAD_LIMIT_BYTES / 2 {
        println!(
            "  note: {} MB, over half the {} MB upload limit",
            size / 1_000_000,
            UPLOAD_LIMIT_BYTES / 1_000_000
        );
    }
}

/// Asks cargo where this package's build output goes.
fn cargo_target_dir(plugin_dir: &Path) -> Result<PathBuf, String> {
    let json = run(
        "cargo metadata",
        Command::new("cargo")
            .args(["metadata", "--format-version", "1", "--no-deps"])
            .current_dir(plugin_dir),
    )?;

    metadata::string_field(&json, "target_directory")
        .map(PathBuf::from)
        .ok_or_else(|| "cargo metadata did not report a target directory".to_string())
}

/// Runs the binding generator, then checks it actually produced something.
///
/// The generator exits successfully when it can find no metadata, which is what a stripped
/// library looks like. Trusting the exit code here would produce an empty bindings file and a
/// compile error many steps later, pointing nowhere near the cause.
fn generate_bindings(plugin_dir: &Path, library: &Path, output: &Path) -> Result<(), String> {
    let bindgen = toolchain::ensure_bindgen()?;
    let out_dir = output
        .parent()
        .ok_or_else(|| "bindings path has no parent directory".to_string())?;

    // The generator defaults to `internal` visibility, which would compile the bindings into
    // a single assembly. The host is a separate assembly and must see them.
    let config = out_dir.join("uniffi.toml");
    std::fs::write(&config, "[bindings.csharp]\naccess_modifier = \"public\"\n")
        .map_err(|e| format!("could not write {}: {e}", config.display()))?;

    run(
        "uniffi-bindgen-cs",
        Command::new(&bindgen)
            .arg("--library")
            .arg(library)
            .arg("--config")
            .arg(&config)
            .arg("--out-dir")
            .arg(out_dir)
            // --config makes the generator read cargo metadata from the working directory.
            .current_dir(plugin_dir),
    )?;

    let written = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    if written == 0 {
        return Err(format!(
            "the binding generator produced nothing from {}.\n\
             This is what a stripped library looks like: it reads metadata from the library's \
             symbols. Set `strip = false` under [profile.release] in Cargo.toml.",
            library.display()
        ));
    }
    Ok(())
}

/// Confirms the publish output has what BTCPay needs before packing it.
fn verify_publish_output(publish_dir: &Path, md: &PluginMetadata) -> Result<(), String> {
    let required = [
        format!("{}.dll", md.identifier),
        "BtcpayRs.Host.dll".to_string(),
        "BtcpayRs.Host.BTCPay.dll".to_string(),
        // Flat beside the plugin assembly, not under runtimes/: BTCPay's load context
        // resolves natives from deps.json, which a copied file never populates.
        "libbtcpay_plugin_native.so".to_string(),
    ];

    let missing: Vec<&String> = required
        .iter()
        .filter(|name| !publish_dir.join(name).exists())
        .collect();

    if !missing.is_empty() {
        return Err(format!(
            "the published plugin is missing: {}. It would install but fail to load.",
            missing
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(())
}

/// Strips the shipped library, now that the bindings have been generated from it.
fn strip_native_library(publish_dir: &Path) -> Result<(), String> {
    let library = publish_dir.join("libbtcpay_plugin_native.so");
    let before = std::fs::metadata(&library).map(|m| m.len()).unwrap_or(0);

    // Not fatal: a missing `strip` costs size, not correctness.
    if Command::new("strip")
        .arg(&library)
        .status()
        .is_ok_and(|s| s.success())
    {
        let after = std::fs::metadata(&library).map(|m| m.len()).unwrap_or(0);
        if after > 0 && after < before {
            println!(
                "  stripped the library, {} KB to {} KB",
                before / 1024,
                after / 1024
            );
        }
    }
    Ok(())
}

fn find_package(dir: &Path, identifier: &str) -> Result<PathBuf, String> {
    let wanted = format!("{identifier}.btcpay");
    let versions =
        std::fs::read_dir(dir).map_err(|e| format!("could not read {}: {e}", dir.display()))?;

    for version in versions.flatten() {
        let candidate = version.path().join(&wanted);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "the packer did not produce {wanted} under {}",
        dir.display()
    ))
}

fn msbuild_properties(
    host: &host::HostProjects,
    btcpayserver_project: &Path,
    native_library: &Path,
) -> Vec<String> {
    vec![
        format!("-p:BtcpayServerProject={}", absolute(btcpayserver_project)),
        format!("-p:BtcpayRsHostProject={}", absolute(&host.core)),
        format!("-p:BtcpayRsHostBtcpayProject={}", absolute(&host.btcpay)),
        format!("-p:BtcpayRsNativeLibrary={}", absolute(native_library)),
    ]
}

/// MSBuild resolves relative paths against each project's own directory, so every path handed
/// across project boundaries has to be absolute.
fn absolute(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}
