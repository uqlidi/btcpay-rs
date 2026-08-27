//! Packaging inside a pinned container.
//!
//! Building a plugin needs Rust and the .NET SDK together, which is an awkward pair to
//! install. Running the same pipeline in a pinned image means a developer needs only Docker,
//! and that the artifact does not depend on whichever versions happen to be on the machine.

use std::path::{Path, PathBuf};
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

    // Resolved before the command is assembled, because a `-v` has to precede the image name and
    // everything after the image name is the command to run inside it.
    let checkout = dev_checkout(&plugin_dir)?;

    let mut command = Command::new("docker");
    command
        .arg("run")
        .arg("--rm")
        // Mounted at its own host path, not at /plugin, so that a relative path inside the
        // plugin's Cargo.toml resolves in the container exactly as it does outside it.
        .args(["-v", &same_path(&plugin_dir)])
        .args(["-v", &format!("{}:/out", out_dir.display())])
        .args(["-v", &format!("{}:/cache", cache.display())])
        .args(["-e", "BTCPAY_RS_CACHE=/cache"])
        .args(["-e", "NUGET_PACKAGES=/cache/nuget"])
        // A target directory of its own. The container's rustc is pinned and the host's is
        // whatever the developer has, and pointing both at one directory makes each build
        // invalidate the other's fingerprints and start over.
        .args(["-e", "CARGO_TARGET_DIR=/cache/target"])
        // Files written inside the container would otherwise be owned by root.
        .args(["-u", &format!("{}:{}", user_id(), group_id())])
        .args(["-w", &plugin_dir.display().to_string()]);

    // `btcpay-plugin` is not published yet, so a plugin depends on it by path. That checkout has
    // to be in the container too, at the same path, or Cargo cannot resolve it. It also carries
    // this CLI, which is what then gets run: the image has no `cargo btcpay` of its own, and
    // building from source means the embedded C# can never be a stale copy.
    if let Some(checkout) = &checkout {
        command.args(["-v", &same_path(checkout)]);
    }

    command.arg(IMAGE);

    match &checkout {
        Some(checkout) => {
            println!(
                "Packaging in {IMAGE}, using the btcpay-rs checkout at {}",
                checkout.display()
            );
            command
                .args(["cargo", "run", "--quiet", "--release"])
                .args([
                    "--manifest-path",
                    &checkout.join("Cargo.toml").display().to_string(),
                ])
                .args(["-p", "cargo-btcpay", "--"])
                .args(["btcpay", "package", "--out", "/out"]);
        }
        None => {
            println!("Packaging in {IMAGE}");
            command.args(["cargo", "btcpay", "package", "--out", "/out"]);
        }
    }

    run_streaming("docker run", &mut command)
}

/// Finds `btcpay-plugin`'s path in the workspace that `plugin_dir` belongs to.
///
/// Returns the manifest's directory too: a path in `[workspace.dependencies]` is relative to the
/// workspace root, not to the member.
fn workspace_dependency_path(plugin_dir: &Path) -> Result<Option<(PathBuf, String)>, String> {
    let mut candidate = plugin_dir;
    loop {
        let manifest = candidate.join("Cargo.toml");
        if let Ok(text) = std::fs::read_to_string(&manifest) {
            let parsed: toml::Value = text
                .parse()
                .map_err(|e| format!("could not parse {}: {e}", manifest.display()))?;
            if let Some(path) = parsed
                .get("workspace")
                .and_then(|workspace| workspace.get("dependencies"))
                .and_then(|deps| deps.get("btcpay-plugin"))
                .and_then(|dep| dep.get("path"))
                .and_then(toml::Value::as_str)
            {
                return Ok(Some((candidate.to_path_buf(), path.to_string())));
            }
        }
        candidate = match candidate.parent() {
            Some(parent) => parent,
            None => return Ok(None),
        };
    }
}

/// A bind mount that puts a host directory at the same path inside the container.
fn same_path(dir: &Path) -> String {
    format!("{0}:{0}", dir.display())
}

/// The btcpay-rs checkout a plugin depends on by path, if it does.
///
/// Returns the workspace root rather than the crate directory: the crate inherits settings and a
/// lockfile from the workspace above it, so mounting the crate alone would not build.
fn dev_checkout(plugin_dir: &Path) -> Result<Option<PathBuf>, String> {
    let manifest = plugin_dir.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest)
        .map_err(|e| format!("could not read {}: {e}", manifest.display()))?;
    let parsed: toml::Value = text
        .parse()
        .map_err(|e| format!("could not parse {}: {e}", manifest.display()))?;

    let dependency = parsed
        .get("dependencies")
        .and_then(|deps| deps.get("btcpay-plugin"));

    let (base, path) = match dependency {
        Some(dep) if dep.get("workspace").and_then(toml::Value::as_bool) == Some(true) => {
            match workspace_dependency_path(plugin_dir)? {
                Some(found) => found,
                None => return Ok(None),
            }
        }
        Some(dep) => match dep.get("path").and_then(toml::Value::as_str) {
            Some(path) => (plugin_dir.to_path_buf(), path.to_string()),
            None => return Ok(None),
        },
        None => return Ok(None),
    };

    let crate_dir = base.join(&path).canonicalize().map_err(|e| {
        format!(
            "the btcpay-plugin path dependency points at {path}, which could not be resolved: {e}"
        )
    })?;

    let mut candidate = crate_dir.as_path();
    while let Some(parent) = candidate.parent() {
        let manifest = parent.join("Cargo.toml");
        if std::fs::read_to_string(&manifest)
            .map(|text| text.contains("[workspace]"))
            .unwrap_or(false)
        {
            return Ok(Some(parent.to_path_buf()));
        }
        candidate = parent;
    }

    Err(format!(
        "the btcpay-plugin path dependency at {} is not inside a Cargo workspace, so there is \
         nothing to mount into the container",
        crate_dir.display()
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a workspace whose member declares `btcpay-plugin.workspace = true`.
    fn workspace_member(root: &Path) -> PathBuf {
        let checkout = root.join("btcpay-rs");
        std::fs::create_dir_all(checkout.join("crates/btcpay-plugin/src")).unwrap();
        std::fs::write(checkout.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();

        let member = root.join("myplugin");
        std::fs::create_dir_all(member.join("plugin")).unwrap();
        std::fs::write(
            member.join("Cargo.toml"),
            "[workspace]\nmembers = [\"plugin\"]\n\n[workspace.dependencies]\n\
             btcpay-plugin = { path = \"../btcpay-rs/crates/btcpay-plugin\" }\n",
        )
        .unwrap();
        std::fs::write(
            member.join("plugin/Cargo.toml"),
            "[package]\nname = \"plugin\"\n\n[dependencies]\nbtcpay-plugin.workspace = true\n",
        )
        .unwrap();
        member.join("plugin")
    }

    #[test]
    fn a_workspace_dependency_is_recognised_as_a_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let plugin_dir = workspace_member(temp.path());

        let found = dev_checkout(&plugin_dir).unwrap();

        assert_eq!(
            found.map(|p| p.canonicalize().unwrap()),
            Some(temp.path().join("btcpay-rs").canonicalize().unwrap()),
            "should resolve to the workspace root of the checkout"
        );
    }

    #[test]
    fn a_direct_path_dependency_is_still_recognised() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path().join("btcpay-rs");
        std::fs::create_dir_all(checkout.join("crates/btcpay-plugin")).unwrap();
        std::fs::write(checkout.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();

        let plugin_dir = temp.path().join("standalone");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("Cargo.toml"),
            "[package]\nname = \"standalone\"\n\n[dependencies]\n\
             btcpay-plugin = { path = \"../btcpay-rs/crates/btcpay-plugin\" }\n",
        )
        .unwrap();

        assert_eq!(
            dev_checkout(&plugin_dir)
                .unwrap()
                .map(|p| p.canonicalize().unwrap()),
            Some(checkout.canonicalize().unwrap())
        );
    }

    #[test]
    fn a_published_dependency_is_not_a_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let plugin_dir = temp.path().join("published");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("Cargo.toml"),
            "[package]\nname = \"published\"\n\n[dependencies]\nbtcpay-plugin = \"0.1\"\n",
        )
        .unwrap();

        assert_eq!(dev_checkout(&plugin_dir).unwrap(), None);
    }
}
