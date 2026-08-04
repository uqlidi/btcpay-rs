//! Packaging inside a pinned container.
//!
//! Building a plugin needs Rust and the .NET SDK together, which is an awkward pair to
//! install. Running the same pipeline in a pinned image means a developer needs only Docker,
//! and that the artifact does not depend on whichever versions happen to be on the machine.

use std::path::Path;
use std::process::Command;

use crate::workspace::{cache_root, run_streaming};

/// Image holding the pinned toolchains. Built from `docker/build.Dockerfile`.
pub const IMAGE: &str = "btcpay-rs-build:local";

/// Runs `cargo btcpay package` inside the container.
pub fn package(plugin_dir: &Path, out_dir: &Path) -> Result<(), String> {
    ensure_image()?;

    let plugin_dir = plugin_dir
        .canonicalize()
        .map_err(|e| format!("could not resolve {}: {e}", plugin_dir.display()))?;

    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("could not create {}: {e}", out_dir.display()))?;
    let out_dir = out_dir
        .canonicalize()
        .map_err(|e| format!("could not resolve {}: {e}", out_dir.display()))?;

    // The caches are mounted rather than lived in the image, so the BTCPay checkout and any
    // NuGet packages survive between runs. Without them every build re-downloads everything.
    let cache = cache_root();
    std::fs::create_dir_all(cache.join("nuget"))
        .map_err(|e| format!("could not create the cache directory: {e}"))?;

    println!("Packaging in {IMAGE}");
    run_streaming(
        "docker run",
        Command::new("docker")
            .arg("run")
            .arg("--rm")
            .args(["-v", &format!("{}:/plugin", plugin_dir.display())])
            .args(["-v", &format!("{}:/out", out_dir.display())])
            .args(["-v", &format!("{}:/cache", cache.display())])
            .args(["-e", "BTCPAY_RS_CACHE=/cache"])
            .args(["-e", "NUGET_PACKAGES=/cache/nuget"])
            // Files written inside the container would otherwise be owned by root.
            .args(["-u", &format!("{}:{}", user_id(), group_id())])
            .args(["-w", "/plugin"])
            .arg(IMAGE)
            .args(["cargo", "btcpay", "package", "--out", "/out"]),
    )
}

fn ensure_image() -> Result<(), String> {
    let exists = Command::new("docker")
        .args(["image", "inspect", IMAGE])
        .output()
        .map_err(|e| format!("could not run docker: {e}. Is it installed and running?"))?
        .status
        .success();

    if exists {
        return Ok(());
    }

    Err(format!(
        "the build image {IMAGE} does not exist. Build it once with:\n\
         \n\
         \x20 docker build -f docker/build.Dockerfile -t {IMAGE} .\n"
    ))
}

fn user_id() -> u32 {
    // SAFETY: getuid cannot fail and touches no memory we own.
    unsafe { libc_getuid() }
}

fn group_id() -> u32 {
    // SAFETY: as above.
    unsafe { libc_getgid() }
}

extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
    #[link_name = "getgid"]
    fn libc_getgid() -> u32;
}
