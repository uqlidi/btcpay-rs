//! Materialises the host assemblies into a plugin's build directory.
//!
//! The C# sources are embedded in this binary, so building a plugin needs no checkout of
//! btcpay-rs. That also ties the host to the CLI that generates the bindings: both come from
//! the same build, so the host cannot be a version out of step with the contract it wraps.
//!
//! Once `BtcpayRs.Host` ships as a NuGet package this becomes a package reference instead.

use std::path::Path;

use crate::workspace::Layout;

/// One embedded file: where it goes, and what is in it.
struct Embedded {
    relative_path: &'static str,
    contents: &'static str,
}

const HOST_FILES: &[Embedded] = &[
    Embedded {
        relative_path: "BtcpayRs.Host/BtcpayRs.Host.csproj",
        contents: include_str!("../../../dotnet/BtcpayRs.Host/BtcpayRs.Host.csproj"),
    },
    Embedded {
        relative_path: "BtcpayRs.Host/NativeLoader.cs",
        contents: include_str!("../../../dotnet/BtcpayRs.Host/NativeLoader.cs"),
    },
    Embedded {
        relative_path: "BtcpayRs.Host/HostServicesImpl.cs",
        contents: include_str!("../../../dotnet/BtcpayRs.Host/HostServicesImpl.cs"),
    },
    Embedded {
        relative_path: "BtcpayRs.Host/IPluginBackend.cs",
        contents: include_str!("../../../dotnet/BtcpayRs.Host/IPluginBackend.cs"),
    },
    Embedded {
        relative_path: "BtcpayRs.Host/RustPluginRuntime.cs",
        contents: include_str!("../../../dotnet/BtcpayRs.Host/RustPluginRuntime.cs"),
    },
    Embedded {
        relative_path: "BtcpayRs.Host.BTCPay/BtcpayRs.Host.BTCPay.csproj",
        contents: include_str!("../../../dotnet/BtcpayRs.Host.BTCPay/BtcpayRs.Host.BTCPay.csproj"),
    },
    Embedded {
        relative_path: "BtcpayRs.Host.BTCPay/RustPluginBase.cs",
        contents: include_str!("../../../dotnet/BtcpayRs.Host.BTCPay/RustPluginBase.cs"),
    },
    Embedded {
        relative_path: "BtcpayRs.Host.BTCPay/RustPluginHostedService.cs",
        contents: include_str!("../../../dotnet/BtcpayRs.Host.BTCPay/RustPluginHostedService.cs"),
    },
    Embedded {
        relative_path: "BtcpayRs.Host.BTCPay/SettingsRepositoryBackend.cs",
        contents: include_str!("../../../dotnet/BtcpayRs.Host.BTCPay/SettingsRepositoryBackend.cs"),
    },
];

/// Paths to the two host projects, once written out.
pub struct HostProjects {
    pub core: std::path::PathBuf,
    pub btcpay: std::path::PathBuf,
    /// Where the generated bindings must be written for the core project to compile.
    pub bindings_file: std::path::PathBuf,
}

/// Writes the host sources into `layout.host_dir()`.
pub fn materialise(layout: &Layout) -> Result<HostProjects, String> {
    let root = layout.host_dir();

    for file in HOST_FILES {
        let path = root.join(file.relative_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
        }
        write_if_changed(&path, file.contents)?;
    }

    let generated = root.join("BtcpayRs.Host/Generated");
    std::fs::create_dir_all(&generated)
        .map_err(|e| format!("could not create {}: {e}", generated.display()))?;

    Ok(HostProjects {
        core: root.join("BtcpayRs.Host/BtcpayRs.Host.csproj"),
        btcpay: root.join("BtcpayRs.Host.BTCPay/BtcpayRs.Host.BTCPay.csproj"),
        bindings_file: generated.join("btcpay.cs"),
    })
}

/// Writes only when the content differs, so unchanged files keep their timestamps and MSBuild
/// can skip work it has already done.
fn write_if_changed(path: &Path, contents: &str) -> Result<(), String> {
    if let Ok(existing) = std::fs::read_to_string(path) {
        if existing == contents {
            return Ok(());
        }
    }
    std::fs::write(path, contents).map_err(|e| format!("could not write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_file_has_content() {
        // include_str! of a path that exists but is empty would compile and fail much later.
        for file in HOST_FILES {
            assert!(
                !file.contents.trim().is_empty(),
                "{} is embedded but empty",
                file.relative_path
            );
        }
    }

    #[test]
    fn writing_produces_both_projects_and_a_place_for_bindings() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());

        let projects = materialise(&layout).unwrap();

        assert!(projects.core.exists(), "core project should exist");
        assert!(projects.btcpay.exists(), "BTCPay project should exist");
        assert!(
            projects.bindings_file.parent().unwrap().exists(),
            "the bindings directory must exist before generation writes into it"
        );
    }

    #[test]
    fn rewriting_unchanged_files_leaves_them_alone() {
        // MSBuild keys off timestamps: rewriting identical content would rebuild everything.
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        let projects = materialise(&layout).unwrap();

        let before = std::fs::metadata(&projects.core)
            .unwrap()
            .modified()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        materialise(&layout).unwrap();
        let after = std::fs::metadata(&projects.core)
            .unwrap()
            .modified()
            .unwrap();

        assert_eq!(before, after, "unchanged file should not be rewritten");
    }
}
