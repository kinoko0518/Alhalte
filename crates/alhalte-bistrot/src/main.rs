//! Command-line interface for `alhalte-bistrot`.

#![deny(unsafe_code)]

use alhalte_bistrot::{
    BistrotError, SystemdOptions, encrypt_bistrot_toml, generate_all_args, generate_module_args,
    generate_systemd_units, load_from_path_with_key,
};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "alhalte-bistrot",
    author,
    version,
    about = "Alhalte restaurant configuration manager, argument generator, and systemd unit generator"
)]
struct Cli {
    /// Path to the Bistrot.toml configuration file.
    #[arg(short, long, default_value = "Bistrot.toml")]
    config: PathBuf,

    /// Age secret key to decrypt secrets (overrides ROPS_AGE environment variable).
    #[arg(long, env = "ROPS_AGE")]
    age_key: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate command-line arguments for Alhalte modules.
    Args {
        /// Target module name ("maitre-d", "cellar", "banquette", or "all").
        #[arg(default_value = "all")]
        module: String,
    },

    /// Generate systemd unit configuration files.
    Systemd {
        /// Directory where module binaries are installed.
        #[arg(long, default_value = "/usr/local/bin")]
        bin_dir: PathBuf,

        /// Output directory to write unit files into (if omitted, prints to stdout).
        #[arg(short, long)]
        output_dir: Option<PathBuf>,

        /// Working directory for the services.
        #[arg(long)]
        working_dir: Option<PathBuf>,

        /// Directory for alhalte-argenterie web frontend.
        #[arg(long)]
        web_dir: Option<PathBuf>,

        /// Optional path to pnpm executable.
        #[arg(long)]
        pnpm_bin: Option<PathBuf>,

        /// Linux user under which to execute services.
        #[arg(long)]
        user: Option<String>,

        /// Linux group under which to execute services.
        #[arg(long)]
        group: Option<String>,

        /// Optional extra PATH environment variable for services.
        #[arg(long)]
        extra_path: Option<String>,
    },

    /// Print the argenterie frontend URL.
    Url,

    /// Validate configuration and print parsed summary.
    Validate,

    /// Encrypt a plain Bistrot.toml using an age public recipient key.
    Encrypt {
        /// Age recipient public key (e.g. `age1...`).
        #[arg(short, long)]
        recipient: String,

        /// Destination file to write encrypted configuration (if omitted, prints to stdout).
        #[arg(short, long)]
        output: Option<PathBuf>,
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

fn run(cli: Cli) -> Result<(), BistrotError> {
    match cli.command {
        Commands::Args { module } => {
            let config = load_from_path_with_key(&cli.config, cli.age_key.as_deref())?;

            if module.eq_ignore_ascii_case("all") {
                let all_args = generate_all_args(&config);
                println!("[alhalte-maitre-d]");
                println!("{}", all_args.maitre_d.join(" "));
                println!("\n[alhalte-cellar]");
                println!("{}", all_args.cellar.join(" "));
                println!("\n[alhalte-banquette]");
                println!("{}", all_args.banquette.join(" "));
            } else {
                let args = generate_module_args(&config, &module)?;
                println!("{}", args.join(" "));
            }
        }
        Commands::Systemd {
            bin_dir,
            output_dir,
            working_dir,
            web_dir,
            pnpm_bin,
            user,
            group,
            extra_path,
        } => {
            let config = load_from_path_with_key(&cli.config, cli.age_key.as_deref())?;
            let pnpm_bin = pnpm_bin.or_else(|| {
                std::env::var_os("PATH").and_then(|paths| {
                    std::env::split_paths(&paths).find_map(|dir| {
                        let candidate = dir.join("pnpm");
                        if candidate.is_file() {
                            Some(candidate)
                        } else {
                            None
                        }
                    })
                })
            });
            let extra_path = extra_path.or_else(|| std::env::var("PATH").ok());
            let options = SystemdOptions {
                bin_dir,
                working_dir,
                web_dir,
                pnpm_bin,
                user,
                group,
                extra_path,
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
                println!(
                    "=== alhalte-maitre-d.service ===\n{}",
                    units.maitre_d_service
                );
                println!("=== alhalte-cellar.service ===\n{}", units.cellar_service);
                println!(
                    "=== alhalte-argenterie.service ===\n{}",
                    units.argenterie_service
                );
                println!("=== alhalte-bistrot.target ===\n{}", units.bistrot_target);
            }
        }
        Commands::Url => {
            let config = load_from_path_with_key(&cli.config, cli.age_key.as_deref())?;
            println!("{}", config.argenterie_url());
        }
        Commands::Validate => {
            let config = load_from_path_with_key(&cli.config, cli.age_key.as_deref())?;
            println!("Configuration in '{}' is valid.", cli.config.display());
            println!("  Domain:       {}", config.restaurant.domain);
            println!("  Host:         {}", config.restaurant.host);
            println!("  Data dir:     {}", config.restaurant.data_dir.display());
            println!("  Socket dir:   {}", config.restaurant.socket_dir.display());
            println!("  maitre-d port: {}", config.maitre_d.port);
            println!("  cellar port:   {}", config.cellar.port);
            println!("  argenterie port: {}", config.argenterie.port);
            println!("  argenterie url:  {}", config.argenterie_url());
            println!(
                "  banquette N:   {} (chunk size: {})",
                config.banquette.chunk_size_exponent,
                1 << config.banquette.chunk_size_exponent
            );
        }
        Commands::Encrypt { recipient, output } => {
            let plain_content = std::fs::read_to_string(&cli.config)?;
            let encrypted = encrypt_bistrot_toml(&plain_content, &recipient)?;

            if let Some(out_path) = output {
                std::fs::write(&out_path, encrypted)?;
                println!("Encrypted configuration written to {}", out_path.display());
            } else {
                print!("{encrypted}");
            }
        }
    }

    Ok(())
}
