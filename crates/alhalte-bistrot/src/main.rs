//! Command-line interface for `alhalte-bistrot`.

#![deny(unsafe_code)]

use alhalte_bistrot::{DEFAULT_SYSTEMD_BIN_DIR, SystemdOptions, generate_systemd_units};
use alhalte_restaurant::{
    encrypt_age_armored_value, encrypt_restaurant_toml, load_restaurant_from_path_with_key,
    write_secret_files,
};
use clap::{Parser, Subcommand};
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "alhalte-bistrot",
    author,
    version,
    about = "Generate Alhalte systemd units from Restaurant.toml"
)]
struct Cli {
    /// Path to the Restaurant.toml configuration file (default: Restaurant.toml).
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Age secret key to decrypt secrets (overrides ROPS_AGE and ALHALTE_AGE_KEY).
    #[arg(long, env = "ROPS_AGE")]
    age_key: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate systemd unit configuration files.
    Systemd {
        /// Directory where module binaries are installed.
        #[arg(long, default_value = DEFAULT_SYSTEMD_BIN_DIR)]
        bin_dir: PathBuf,

        /// Output directory to write unit files into (if omitted, prints to stdout).
        #[arg(short, long)]
        output_dir: Option<PathBuf>,

        /// Working directory for the services.
        #[arg(long)]
        working_dir: Option<PathBuf>,

        /// Directory containing the built SPA, passed as maitre-d --web-root.
        ///
        /// Defaults to `<data-dir>/web`.
        #[arg(long)]
        web_root: Option<PathBuf>,

        /// Linux user under which to execute services.
        #[arg(long)]
        user: Option<String>,

        /// Linux group under which to execute services.
        #[arg(long)]
        group: Option<String>,
    },

    /// Validate configuration and print parsed summary.
    Validate,

    /// Encrypt a plain Restaurant.toml using an age public recipient key.
    Encrypt {
        /// Age recipient public key (e.g. `age1...`).
        #[arg(short, long)]
        recipient: String,

        /// Destination file to write encrypted configuration (if omitted, prints to stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Encrypt one secret value from stdin into an age ASCII-armored block.
    ///
    /// Paste the output into Restaurant.toml as the value of
    /// `maitre-d.client-secret` or `cellar.service-token` using a TOML
    /// multi-line string.
    EncryptValue {
        /// Age recipient public key (e.g. `age1...`).
        #[arg(short, long)]
        recipient: String,
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

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(|| PathBuf::from("Restaurant.toml"));
    match cli.command {
        Commands::Systemd {
            bin_dir,
            output_dir,
            working_dir,
            web_root,
            user,
            group,
        } => {
            let mut config =
                load_restaurant_from_path_with_key(&config_path, cli.age_key.as_deref())?;
            let config_dir = config_path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            config.resolve_relative_paths(&config_dir);
            // The units only reference secret files, so they must exist before
            // systemd starts the services.
            write_secret_files(&config)?;

            let web_root = web_root.unwrap_or_else(|| config.restaurant.data_dir.join("web"));
            let options = SystemdOptions {
                bin_dir,
                working_dir,
                web_root: Some(web_root),
                user,
                group,
                ..Default::default()
            };
            let units = generate_systemd_units(&config, &options);

            if let Some(dir) = output_dir {
                units.write_to_dir(&dir)?;
                println!(
                    "Successfully generated systemd unit files in {}",
                    dir.display()
                );
            } else {
                println!("=== alhalte-cellar.service ===\n{}", units.cellar_service);
                println!(
                    "=== alhalte-maitre-d.service ===\n{}",
                    units.maitre_d_service
                );
                println!("=== alhalte.target ===\n{}", units.target);
            }
            Ok(())
        }
        Commands::Validate => {
            let config = load_restaurant_from_path_with_key(&config_path, cli.age_key.as_deref())?;
            println!("Configuration in '{}' is valid.", config_path.display());
            println!("  Domain:       {}", config.restaurant.domain);
            println!("  Host:         {}", config.restaurant.host);
            println!("  Data dir:     {}", config.restaurant.data_dir.display());
            println!("  Socket dir:   {}", config.restaurant.socket_dir.display());
            println!("  maitre-d port: {}", config.maitre_d.port);
            println!("  cellar port:   {}", config.cellar.port);
            println!("  UI url:        {}", config.maitre_d_url());
            println!("  public url:    {}", config.maitre_d_public_url());
            println!(
                "  LAN exposed:   {}",
                if config.is_lan_exposed() { "yes" } else { "no" }
            );
            println!(
                "  banquette N:   {} (chunk size: {})",
                config.banquette.chunk_size_exponent,
                1 << config.banquette.chunk_size_exponent
            );
            println!("  Extensions:    {} configured", config.extensions.len());
            for ext in &config.extensions {
                println!(
                    "    - [{}] (path: {}, enabled: {})",
                    ext.id,
                    ext.path.display(),
                    ext.enabled
                );
            }
            Ok(())
        }
        Commands::Encrypt { recipient, output } => {
            let plain_content = std::fs::read_to_string(&config_path)?;
            let encrypted = encrypt_restaurant_toml(&plain_content, &recipient)?;
            if let Some(out_path) = output {
                std::fs::write(&out_path, encrypted)?;
                println!("Encrypted configuration written to {}", out_path.display());
            } else {
                print!("{encrypted}");
            }
            Ok(())
        }
        Commands::EncryptValue { recipient } => {
            let mut plaintext = String::new();
            std::io::stdin().read_to_string(&mut plaintext)?;
            // A trailing newline from `echo` is never part of the secret.
            let plaintext = plaintext.trim_end_matches(['\n', '\r']);
            let encrypted = encrypt_age_armored_value(plaintext, &recipient)?;
            println!("{encrypted}");
            Ok(())
        }
    }
}
