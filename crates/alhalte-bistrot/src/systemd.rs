//! Pure functions for generating systemd unit configuration files.
//!
//! Generates unit files for:
//! - `alhalte-maitre-d.service`
//! - `alhalte-cellar.service`
//! - `alhalte-bistrot.target` (orchestrating target)

use crate::args::{generate_cellar_args, generate_maitre_d_args};
use crate::config::BistrotConfig;
use std::path::{Path, PathBuf};

/// Default binary installation directory for systemd units.
pub const DEFAULT_SYSTEMD_BIN_DIR: &str = "/usr/local/bin";

/// Options for generating systemd unit files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemdOptions {
    /// Directory where the module binaries are located (e.g. `/usr/local/bin`).
    pub bin_dir: PathBuf,
    /// Optional working directory for the services.
    pub working_dir: Option<PathBuf>,
    /// Optional directory for the argenterie web frontend.
    pub web_dir: Option<PathBuf>,
    /// Optional path to pnpm executable.
    pub pnpm_bin: Option<PathBuf>,
    /// Optional custom command for running argenterie.
    pub argenterie_cmd: Option<String>,
    /// Optional Linux user under which to execute services.
    pub user: Option<String>,
    /// Optional Linux group under which to execute services.
    pub group: Option<String>,
    /// Restart policy for the service (default: `"on-failure"`).
    pub restart: String,
    /// Time to wait before restarting (default: `"5s"`).
    pub restart_sec: String,
    /// Optional extra PATH environment variable for services.
    pub extra_path: Option<String>,
}

impl Default for SystemdOptions {
    fn default() -> Self {
        Self {
            bin_dir: PathBuf::from(DEFAULT_SYSTEMD_BIN_DIR),
            working_dir: None,
            web_dir: None,
            pnpm_bin: None,
            argenterie_cmd: None,
            user: None,
            group: None,
            restart: "on-failure".to_string(),
            restart_sec: "5s".to_string(),
            extra_path: None,
        }
    }
}

/// Collection of generated systemd unit configuration files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemdUnits {
    /// Content of `alhalte-maitre-d.service`.
    pub maitre_d_service: String,
    /// Content of `alhalte-cellar.service`.
    pub cellar_service: String,
    /// Content of `alhalte-argenterie.service`.
    pub argenterie_service: String,
    /// Content of `alhalte-bistrot.target`.
    pub bistrot_target: String,
}

impl SystemdUnits {
    /// Writes all generated unit files to the specified directory.
    ///
    /// # Errors
    ///
    /// Returns [`std::io::Error`] if writing files fails.
    pub fn write_to_dir(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("alhalte-maitre-d.service"), &self.maitre_d_service)?;
        std::fs::write(dir.join("alhalte-cellar.service"), &self.cellar_service)?;
        std::fs::write(
            dir.join("alhalte-argenterie.service"),
            &self.argenterie_service,
        )?;
        std::fs::write(dir.join("alhalte-bistrot.target"), &self.bistrot_target)?;
        Ok(())
    }
}

/// Escapes a command-line argument for inclusion in systemd `ExecStart`.
fn systemd_escape_arg(arg: &str) -> String {
    if arg.contains(' ') || arg.contains('\t') || arg.contains('"') || arg.contains('\\') {
        let escaped = arg.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    } else {
        arg.to_string()
    }
}

/// Formats an `ExecStart` line given a binary path and argument list.
fn format_exec_start(bin_path: &Path, args: &[String]) -> String {
    let mut parts = vec![bin_path.display().to_string()];
    for arg in args {
        parts.push(systemd_escape_arg(arg));
    }
    parts.join(" ")
}

/// Generates the content of `alhalte-maitre-d.service`.
///
/// Deterministic and pure function.
pub fn generate_maitre_d_service(config: &BistrotConfig, options: &SystemdOptions) -> String {
    let bin_path = options.bin_dir.join("alhalte-maitre-d");
    let args = generate_maitre_d_args(config);
    let exec_start = format_exec_start(&bin_path, &args);

    let mut service_lines = Vec::new();
    service_lines.push("Type=simple".to_string());
    service_lines.push(format!("ExecStart={exec_start}"));
    service_lines.push(format!("Restart={}", options.restart));
    service_lines.push(format!("RestartSec={}", options.restart_sec));

    if let Some(ref dir) = options.working_dir {
        service_lines.push(format!("WorkingDirectory={}", dir.display()));
    }
    if let Some(ref user) = options.user {
        service_lines.push(format!("User={user}"));
    }
    if let Some(ref group) = options.group {
        service_lines.push(format!("Group={group}"));
    }

    format!(
        r#"[Unit]
Description=Alhalte Maitre-d Authentication and WebSocket Gateway
Documentation=https://github.com/kinoko0518/Alhalte
PartOf=alhalte-bistrot.target
After=network.target

[Service]
{service_section}

[Install]
WantedBy=alhalte-bistrot.target
"#,
        service_section = service_lines.join("\n")
    )
}

/// Generates the content of `alhalte-cellar.service`.
///
/// Deterministic and pure function.
pub fn generate_cellar_service(config: &BistrotConfig, options: &SystemdOptions) -> String {
    let bin_path = options.bin_dir.join("alhalte-cellar");
    let args = generate_cellar_args(config);
    let exec_start = format_exec_start(&bin_path, &args);

    let mut service_lines = Vec::new();
    service_lines.push("Type=simple".to_string());
    service_lines.push(format!("ExecStart={exec_start}"));
    service_lines.push(format!("Restart={}", options.restart));
    service_lines.push(format!("RestartSec={}", options.restart_sec));

    if let Some(ref dir) = options.working_dir {
        service_lines.push(format!("WorkingDirectory={}", dir.display()));
    }
    if let Some(ref user) = options.user {
        service_lines.push(format!("User={user}"));
    }
    if let Some(ref group) = options.group {
        service_lines.push(format!("Group={group}"));
    }

    format!(
        r#"[Unit]
Description=Alhalte Cellar File Storage and Tin Service
Documentation=https://github.com/kinoko0518/Alhalte
PartOf=alhalte-bistrot.target
After=network.target

[Service]
{service_section}

[Install]
WantedBy=alhalte-bistrot.target
"#,
        service_section = service_lines.join("\n")
    )
}

/// Generates the content of `alhalte-argenterie.service`.
///
/// Deterministic and pure function.
pub fn generate_argenterie_service(config: &BistrotConfig, options: &SystemdOptions) -> String {
    let working_dir = options.web_dir.clone().or_else(|| {
        options
            .working_dir
            .as_ref()
            .map(|wd| wd.join("web").join("alhalte-argenterie"))
    });

    let exec_start = if let Some(ref cmd) = options.argenterie_cmd {
        cmd.clone()
    } else {
        let pnpm = options
            .pnpm_bin
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "pnpm".to_string());
        format!(
            "{pnpm} run dev --host {} --port {}",
            config.restaurant.host, config.argenterie.port
        )
    };

    let mut service_lines = Vec::new();
    service_lines.push("Type=simple".to_string());
    service_lines.push(format!("ExecStart={exec_start}"));
    service_lines.push(format!("Restart={}", options.restart));
    service_lines.push(format!("RestartSec={}", options.restart_sec));

    if let Some(ref dir) = working_dir {
        service_lines.push(format!("WorkingDirectory={}", dir.display()));
    }
    if let Some(ref path) = options.extra_path {
        service_lines.push(format!("Environment=\"PATH={path}\""));
    }
    if let Some(ref user) = options.user {
        service_lines.push(format!("User={user}"));
    }
    if let Some(ref group) = options.group {
        service_lines.push(format!("Group={group}"));
    }

    format!(
        r#"[Unit]
Description=Alhalte Argenterie Frontend Development Server
Documentation=https://github.com/kinoko0518/Alhalte
PartOf=alhalte-bistrot.target
After=network.target

[Service]
{service_section}

[Install]
WantedBy=alhalte-bistrot.target
"#,
        service_section = service_lines.join("\n")
    )
}

/// Generates the content of `alhalte-bistrot.target`.
///
/// Deterministic and pure function.
pub fn generate_bistrot_target() -> String {
    r#"[Unit]
Description=Alhalte Bistrot Restaurant Suite Target
Documentation=https://github.com/kinoko0518/Alhalte
Wants=alhalte-maitre-d.service alhalte-cellar.service alhalte-argenterie.service
After=network.target

[Install]
WantedBy=multi-user.target
"#
    .to_string()
}

/// Generates all systemd unit configuration files.
///
/// Deterministic and pure function.
pub fn generate_systemd_units(config: &BistrotConfig, options: &SystemdOptions) -> SystemdUnits {
    SystemdUnits {
        maitre_d_service: generate_maitre_d_service(config, options),
        cellar_service: generate_cellar_service(config, options),
        argenterie_service: generate_argenterie_service(config, options),
        bistrot_target: generate_bistrot_target(),
    }
}
