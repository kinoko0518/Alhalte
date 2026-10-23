//! Command-line interface for `alhalte-cafe`.

#![deny(unsafe_code)]

use alhalte_cafe::{CafeOptions, Result, run_cafe};
use alhalte_restaurant::load_restaurant_from_path_with_key;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "alhalte-cafe",
    author,
    version,
    about = "Start a local Alhalte instance without systemd"
)]
struct Cli {
    /// Path to the Restaurant.toml configuration file (default: Restaurant.toml).
    ///
    /// The launcher also falls back to the Restaurant.toml next to the
    /// executable, so a portable folder can be launched by double-clicking.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Age secret key to decrypt secrets (overrides ROPS_AGE and ALHALTE_AGE_KEY).
    #[arg(long, env = "ROPS_AGE")]
    age_key: Option<String>,

    /// Do not open a browser after the services are ready.
    #[arg(long)]
    no_browser: bool,

    /// Directory containing the built SPA (default: `<config-dir>/web`).
    #[arg(long)]
    web_root: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Stage compiled extensions into an output directory and write manifest.json.
    StageExtensions {
        /// Target directory to copy .wasm files and write manifest.json.
        #[arg(short, long)]
        output_dir: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Some(Commands::StageExtensions { output_dir }) => {
            let config_path = cli
                .config
                .clone()
                .unwrap_or_else(|| PathBuf::from("Restaurant.toml"));
            let config = load_restaurant_from_path_with_key(&config_path, cli.age_key.as_deref())?;
            let base_dir = config_path.parent();
            let entries = config.stage_extensions_with_base(&output_dir, base_dir)?;
            println!(
                "Successfully staged {} extension(s) to {}",
                entries.len(),
                output_dir.display()
            );
            Ok(())
        }
        // No subcommand (or a bare `serve`-less invocation) starts the instance.
        None => run_cafe(
            cli.config.as_deref(),
            cli.age_key.as_deref(),
            CafeOptions {
                no_browser: cli.no_browser,
                web_root: cli.web_root,
            },
        ),
    }
}
