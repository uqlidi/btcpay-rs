//! `cargo btcpay new`: creates a plugin project.
//!
//! The template is embedded in the binary, so scaffolding needs no network and cannot drift
//! from the CLI that renders it. It contains Rust and configuration only. The C# is
//! generated at build time from the compiled library, so there is nothing here for an author
//! to keep in sync by hand.

use std::path::{Path, PathBuf};

/// Files written by `new`, before substitution.
const CARGO_TOML: &str = include_str!("../template/Cargo.toml.in");
const LIB_RS: &str = include_str!("../template/lib.rs.in");
const BTCPAY_TOML: &str = include_str!("../template/btcpay.toml.in");
const GITIGNORE: &str = include_str!("../template/gitignore.in");
const README: &str = include_str!("../template/README.md.in");

/// What the caller chose for the new plugin.
pub struct NewPlugin {
    pub directory: PathBuf,
    pub crate_name: String,
    pub identifier: String,
    pub display_name: String,
    pub description: String,
    /// Point the generated crate at a local btcpay-plugin instead of the published one.
    ///
    /// Only for testing this repository against itself: `btcpay-plugin` is not on crates.io
    /// yet, so a scaffolded project cannot resolve its dependency without this.
    pub btcpay_plugin_path: Option<PathBuf>,
}

/// Creates the project on disk, returning the files written.
pub fn create(spec: &NewPlugin) -> Result<Vec<PathBuf>, String> {
    validate_identifier(&spec.identifier)?;
    validate_crate_name(&spec.crate_name)?;

    if spec.directory.exists()
        && spec
            .directory
            .read_dir()
            .is_ok_and(|mut d| d.next().is_some())
    {
        return Err(format!(
            "{} already exists and is not empty",
            spec.directory.display()
        ));
    }

    let src = spec.directory.join("src");
    std::fs::create_dir_all(&src)
        .map_err(|e| format!("could not create {}: {e}", src.display()))?;

    let files = [
        (spec.directory.join("Cargo.toml"), render(CARGO_TOML, spec)),
        (src.join("lib.rs"), render(LIB_RS, spec)),
        (
            spec.directory.join("btcpay.toml"),
            render(BTCPAY_TOML, spec),
        ),
        (spec.directory.join(".gitignore"), render(GITIGNORE, spec)),
        (spec.directory.join("README.md"), render(README, spec)),
    ];

    let mut written = Vec::new();
    for (path, contents) in files {
        std::fs::write(&path, contents)
            .map_err(|e| format!("could not write {}: {e}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

fn render(template: &str, spec: &NewPlugin) -> String {
    let rendered = template
        .replace("{{crate_name}}", &spec.crate_name)
        .replace("{{identifier}}", &spec.identifier)
        .replace("{{display_name}}", &spec.display_name)
        .replace("{{description}}", &spec.description);

    match &spec.btcpay_plugin_path {
        Some(path) => rendered.replace(
            r#"btcpay-plugin = "0.1""#,
            &format!(r#"btcpay-plugin = {{ path = "{}" }}"#, path.display()),
        ),
        None => rendered,
    }
}

/// BTCPay identifiers are reverse-DNS and become both a C# namespace and a directory name,
/// so the rules are stricter than "any string".
fn validate_identifier(identifier: &str) -> Result<(), String> {
    if identifier.is_empty() {
        return Err("plugin identifier must not be empty".into());
    }

    let segments: Vec<&str> = identifier.split('.').collect();
    if segments.len() < 2 {
        return Err(format!(
            "plugin identifier `{identifier}` should be reverse-DNS with at least two \
             segments, such as `Acme.Plugins.Thing`"
        ));
    }

    for segment in segments {
        if segment.is_empty() {
            return Err(format!(
                "plugin identifier `{identifier}` has an empty segment"
            ));
        }
        if !segment
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            return Err(format!(
                "segment `{segment}` in `{identifier}` must start with a letter: the \
                 identifier becomes a C# namespace"
            ));
        }
        if let Some(bad) = segment
            .chars()
            .find(|c| !c.is_ascii_alphanumeric() && *c != '_')
        {
            return Err(format!(
                "character `{bad}` in `{identifier}` is not allowed: use letters, digits and \
                 underscores, separated by dots"
            ));
        }
    }
    Ok(())
}

fn validate_crate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("crate name must not be empty".into());
    }
    if !name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return Err(format!("crate name `{name}` must start with a letter"));
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !c.is_ascii_alphanumeric() && *c != '-' && *c != '_')
    {
        return Err(format!(
            "character `{bad}` in crate name `{name}` is not allowed: use letters, digits, \
             hyphens and underscores"
        ));
    }
    Ok(())
}

/// Derives a plausible identifier from a crate name, for use as a prompt default.
pub fn suggest_identifier(crate_name: &str) -> String {
    let cleaned: String = crate_name
        .split(['-', '_'])
        .filter(|s| !s.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    format!("BTCPayServer.Plugins.{cleaned}")
}

/// Derives a display name from a crate name.
pub fn suggest_display_name(crate_name: &str) -> String {
    crate_name
        .split(['-', '_'])
        .filter(|s| !s.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The directory name for a new plugin, used when no path is given.
pub fn default_directory(crate_name: &str) -> PathBuf {
    Path::new(crate_name).to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(dir: &Path) -> NewPlugin {
        NewPlugin {
            directory: dir.to_path_buf(),
            crate_name: "my-plugin".into(),
            identifier: "Acme.Plugins.Mine".into(),
            display_name: "My Plugin".into(),
            description: "Does something useful.".into(),
            btcpay_plugin_path: None,
        }
    }

    #[test]
    fn a_scaffolded_project_contains_no_csharp() {
        // The whole point: authors write Rust, and the C# is build output.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("my-plugin");

        let written = create(&spec(&target)).unwrap();

        assert!(!written.iter().any(|p| {
            let name = p.to_string_lossy();
            name.ends_with(".cs") || name.ends_with(".csproj")
        }));
        assert!(target.join("Cargo.toml").exists());
        assert!(target.join("src/lib.rs").exists());
        assert!(target.join("btcpay.toml").exists());
    }

    #[test]
    fn substitutions_reach_every_rendered_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("my-plugin");
        create(&spec(&target)).unwrap();

        let lib = std::fs::read_to_string(target.join("src/lib.rs")).unwrap();
        let cargo = std::fs::read_to_string(target.join("Cargo.toml")).unwrap();

        assert!(lib.contains("Acme.Plugins.Mine"));
        assert!(cargo.contains("my-plugin"));
        assert!(
            !lib.contains("{{") && !cargo.contains("{{"),
            "no placeholder should survive rendering"
        );
    }

    #[test]
    fn the_cdylib_is_named_so_the_shared_bindings_can_find_it() {
        // Every plugin's library must be btcpay_plugin_native: the generated C# binds to
        // that name, and a plugin named anything else would fail to load.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("my-plugin");
        create(&spec(&target)).unwrap();

        let cargo = std::fs::read_to_string(target.join("Cargo.toml")).unwrap();
        assert!(
            cargo.contains(r#"name = "btcpay_plugin_native""#),
            "got: {cargo}"
        );
    }

    #[test]
    fn identifiers_that_would_not_be_valid_csharp_namespaces_are_rejected() {
        assert!(validate_identifier("Acme.Plugins.Thing").is_ok());
        assert!(validate_identifier("A.B").is_ok());

        assert!(validate_identifier("").is_err());
        assert!(
            validate_identifier("NoDots").is_err(),
            "needs reverse-DNS form"
        );
        assert!(validate_identifier("Acme..Thing").is_err(), "empty segment");
        assert!(
            validate_identifier("Acme.9Lives").is_err(),
            "segment starts with a digit"
        );
        assert!(
            validate_identifier("Acme.My-Plugin").is_err(),
            "hyphen is not valid in C#"
        );
    }

    #[test]
    fn refuses_to_overwrite_an_existing_project() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("existing");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("Cargo.toml"), "# theirs").unwrap();

        let err = create(&spec(&target)).expect_err("should refuse");

        assert!(err.contains("not empty"));
        assert_eq!(
            std::fs::read_to_string(target.join("Cargo.toml")).unwrap(),
            "# theirs",
            "existing files must be left alone"
        );
    }

    #[test]
    fn a_local_dependency_can_be_substituted_for_the_published_one() {
        // Needed until btcpay-plugin is on crates.io: without it a scaffolded project cannot
        // resolve its dependency, so the scaffolding cannot be tested at all.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("my-plugin");
        let mut s = spec(&target);
        s.btcpay_plugin_path = Some(PathBuf::from("/checkout/crates/btcpay-plugin"));

        create(&s).unwrap();

        let cargo = std::fs::read_to_string(target.join("Cargo.toml")).unwrap();
        assert!(
            cargo.contains(r#"path = "/checkout/crates/btcpay-plugin""#),
            "got: {cargo}"
        );
        assert!(!cargo.contains(r#"btcpay-plugin = "0.1""#));
    }

    #[test]
    fn suggestions_turn_a_crate_name_into_conventional_forms() {
        assert_eq!(
            suggest_identifier("swap-monitor"),
            "BTCPayServer.Plugins.SwapMonitor"
        );
        assert_eq!(suggest_display_name("swap-monitor"), "Swap Monitor");
    }
}
