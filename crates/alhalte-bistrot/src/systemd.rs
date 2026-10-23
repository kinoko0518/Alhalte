//! Pure functions for generating systemd unit configuration files.
//!
//! Generates unit files for:
//! - `alhalte-cellar.service`
//! - `alhalte-maitre-d.service`
//! - `alhalte.target` (orchestrating target)
//!
//! The bundled SPA is served by `alhalte-maitre-d --web-root`, so there is no
//! separate argenterie service.

use alhalte_restaurant::{
    MaitreDArgsOptions, RestaurantToml, generate_cellar_args, generate_maitre_d_args,
};
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
    /// Directory containing the built SPA, passed as `maitre-d --web-root`.
    pub web_root: Option<PathBuf>,
    /// Optional Linux user under which to execute services.
    pub user: Option<String>,
    /// Optional Linux group under which to execute services.
    pub group: Option<String>,
    /// Restart policy for the service (default: `"on-failure"`).
    pub restart: String,
    /// Time to wait before restarting (default: `"5s"`).
    pub restart_sec: String,
}

impl Default for SystemdOptions {
    fn default() -> Self {
        Self {
            bin_dir: PathBuf::from(DEFAULT_SYSTEMD_BIN_DIR),
            working_dir: None,
            web_root: None,
            user: None,
            group: None,
            restart: "on-failure".to_string(),
            restart_sec: "5s".to_string(),
        }
    }
}

/// Collection of generated systemd unit configuration files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemdUnits {
    /// Content of `alhalte-cellar.service`.
    pub cellar_service: String,
    /// Content of `alhalte-maitre-d.service`.
    pub maitre_d_service: String,
    /// Content of `alhalte.target`.
    pub target: String,
}

impl SystemdUnits {
    /// Writes all generated unit files to the specified directory.
    ///
    /// # Errors
    ///
    /// Returns [`std::io::Error`] if writing files fails.
    pub fn write_to_dir(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("alhalte-cellar.service"), &self.cellar_service)?;
        std::fs::write(dir.join("alhalte-maitre-d.service"), &self.maitre_d_service)?;
        std::fs::write(dir.join("alhalte.target"), &self.target)?;
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

/// Generates the content of `alhalte-cellar.service`.
///
/// Deterministic and pure function.
pub fn generate_cellar_service(config: &RestaurantToml, options: &SystemdOptions) -> String {
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
PartOf=alhalte.target
After=network.target

[Service]
{service_section}

[Install]
WantedBy=alhalte.target
"#,
        service_section = service_lines.join("\n")
    )
}

/// Generates the content of `alhalte-maitre-d.service`.
///
/// `maitre-d` serves the built SPA on the same origin as `/api` and `/auth`
/// when `options.web_root` is set.
///
/// Deterministic and pure function.
pub fn generate_maitre_d_service(config: &RestaurantToml, options: &SystemdOptions) -> String {
    let bin_path = options.bin_dir.join("alhalte-maitre-d");
    let args_options = MaitreDArgsOptions {
        web_root: options.web_root.clone(),
        banquette_bin: None,
    };
    let args = generate_maitre_d_args(config, &args_options);
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
PartOf=alhalte.target
After=network.target

[Service]
{service_section}

[Install]
WantedBy=alhalte.target
"#,
        service_section = service_lines.join("\n")
    )
}

/// Generates the content of `alhalte.target`.
///
/// Deterministic and pure function.
pub fn generate_alhalte_target() -> String {
    r#"[Unit]
Description=Alhalte Restaurant Suite Target
Documentation=https://github.com/kinoko0518/Alhalte
Wants=alhalte-cellar.service alhalte-maitre-d.service
After=network.target

[Install]
WantedBy=multi-user.target
"#
    .to_string()
}

/// Generates all systemd unit configuration files.
///
/// Deterministic and pure function.
pub fn generate_systemd_units(config: &RestaurantToml, options: &SystemdOptions) -> SystemdUnits {
    SystemdUnits {
        cellar_service: generate_cellar_service(config, options),
        maitre_d_service: generate_maitre_d_service(config, options),
        target: generate_alhalte_target(),
    }
}
