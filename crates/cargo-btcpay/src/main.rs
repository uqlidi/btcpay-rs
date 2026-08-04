//! `cargo btcpay`: scaffold, inspect and generate the C# for BTCPay Server plugins written
//! in Rust.

mod build;
mod config;
mod docker;
mod host;
mod metadata;
mod scaffold;
mod shim;
mod toolchain;
mod workspace;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use config::Config;
use scaffold::NewPlugin;

/// Invoked as `cargo btcpay ...`, so cargo passes `btcpay` as the first argument.
#[derive(Parser)]
#[command(name = "cargo-btcpay", bin_name = "cargo btcpay", version, about)]
enum Cargo {
    Btcpay(Cli),
}

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new plugin project.
    New {
        /// Crate name, also the directory name unless --path is given.
        name: String,

        /// Where to create the project. Defaults to ./<name>.
        #[arg(long)]
        path: Option<PathBuf>,

        /// Reverse-DNS plugin identifier, e.g. Acme.Plugins.Thing.
        /// Defaults to one derived from the crate name.
        #[arg(long)]
        identifier: Option<String>,

        /// Human-readable name shown in BTCPay. Defaults to one derived from the crate name.
        #[arg(long)]
        display_name: Option<String>,

        /// One-line description.
        #[arg(long, default_value = "A BTCPay Server plugin written in Rust.")]
        description: String,

        /// Depend on a local btcpay-plugin checkout rather than the published crate.
        /// For testing this repository against itself; not for normal use.
        #[arg(long, hide = true)]
        dev_path: Option<PathBuf>,
    },

    /// Print what a compiled plugin library reports about itself.
    Inspect {
        /// Path to the built library, e.g. target/release/libbtcpay_plugin_native.so.
        library: PathBuf,
    },

    /// Compile the plugin and the C# that wraps it.
    Build {
        /// Plugin directory. Defaults to the current directory.
        #[arg(long, default_value = ".")]
        manifest_dir: PathBuf,

        /// Build with optimisations.
        #[arg(long)]
        release: bool,
    },

    /// Build and assemble an installable .btcpay file.
    Package {
        /// Plugin directory. Defaults to the current directory.
        #[arg(long, default_value = ".")]
        manifest_dir: PathBuf,

        /// Where to write the package.
        #[arg(long, default_value = "artifacts")]
        out: PathBuf,

        /// Build inside a pinned container, so only Docker is needed locally.
        #[arg(long)]
        docker: bool,
    },

    /// Generate the C# project for a compiled plugin library.
    ///
    /// Normally run for you by the build; useful on its own for inspecting the output.
    Shim {
        /// Path to the built library.
        library: PathBuf,

        /// Where to write the generated project.
        #[arg(long, default_value = "target/btcpay-shim")]
        out: PathBuf,

        /// Plugin directory holding btcpay.toml. Defaults to the current directory.
        #[arg(long, default_value = ".")]
        manifest_dir: PathBuf,
    },
}

fn main() -> ExitCode {
    let Cargo::Btcpay(cli) = Cargo::parse();

    let result = match cli.command {
        Command::New {
            name,
            path,
            identifier,
            display_name,
            description,
            dev_path,
        } => run_new(name, path, identifier, display_name, description, dev_path),
        Command::Build {
            manifest_dir,
            release,
        } => build::build(&manifest_dir, release).map(|built| {
            println!();
            println!("Built {}", built.metadata.identifier);
        }),
        Command::Package {
            manifest_dir,
            out,
            docker,
        } => {
            if docker {
                docker::package(&manifest_dir, &out)
            } else {
                build::package(&manifest_dir, &out).map(|_| ())
            }
        }
        Command::Inspect { library } => run_inspect(library),
        Command::Shim {
            library,
            out,
            manifest_dir,
        } => run_shim(library, out, manifest_dir),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run_new(
    name: String,
    path: Option<PathBuf>,
    identifier: Option<String>,
    display_name: Option<String>,
    description: String,
    dev_path: Option<PathBuf>,
) -> Result<(), String> {
    let spec = NewPlugin {
        directory: path.unwrap_or_else(|| scaffold::default_directory(&name)),
        identifier: identifier.unwrap_or_else(|| scaffold::suggest_identifier(&name)),
        display_name: display_name.unwrap_or_else(|| scaffold::suggest_display_name(&name)),
        crate_name: name,
        description,
        btcpay_plugin_path: dev_path,
    };

    let files = scaffold::create(&spec)?;

    println!(
        "Created {} in {}",
        spec.identifier,
        spec.directory.display()
    );
    for file in &files {
        println!("  {}", file.display());
    }
    println!();
    println!("Next:");
    println!("  cd {}", spec.directory.display());
    println!("  cargo build");
    println!();
    println!("There is no C# to write: it is generated from your compiled plugin at build");
    println!("time. Note the first build fetches a BTCPay Server checkout (about 40 MB),");
    println!("because BTCPayServer.Abstractions is not published on NuGet. It is cached");
    println!("afterwards.");
    Ok(())
}

fn run_inspect(library: PathBuf) -> Result<(), String> {
    let md = metadata::read(&library)?;

    println!("identifier   {}", md.identifier);
    println!("name         {}", md.name);
    println!("version      {}", md.version);
    println!("description  {}", md.description);
    println!("ABI          {}", md.abi_version);
    if md.dependencies.is_empty() {
        println!("dependencies (none)");
    } else {
        println!("dependencies");
        for dep in &md.dependencies {
            println!("  {} {}", dep.identifier, dep.condition);
        }
    }
    Ok(())
}

fn run_shim(library: PathBuf, out: PathBuf, manifest_dir: PathBuf) -> Result<(), String> {
    let md = metadata::read(&library)?;
    let config = Config::load(&manifest_dir)?;

    shim::generate(&md, &config, &out)?;

    println!(
        "Generated the C# project for {} in {}",
        md.identifier,
        out.display()
    );
    println!("  {}.csproj", md.identifier);
    println!("  Plugin.cs");
    Ok(())
}
