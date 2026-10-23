//! One-command orchestration for a local Alhalte instance.
//!
//! `alhalte-cafe` loads `Restaurant.toml`, materializes secrets, starts cellar
//! and maitre-d (which spawns banquette per room), waits for both listeners, and
//! opens the browser. It is the portable Windows/Linux/macOS entry point and the
//! no-argument development launcher.
//!
//! The process tree is intentionally simple: no auto-restart and no file
//! logging. Ctrl+C (or a child exiting) terminates the tree. Room persistence is
//! the normal banquette heartbeat; at most one heartbeat interval is lost on a
//! forced shutdown.

use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use alhalte_restaurant::config::RestaurantToml;
use alhalte_restaurant::rops::{
    ParseOptions, is_rops_encrypted, parse_restaurant_toml_with_options,
};
use alhalte_restaurant::secrets::{allow_plaintext_secrets_from_env, write_secret_files};
use alhalte_restaurant::{MaitreDArgsOptions, generate_cellar_args, generate_maitre_d_args};

use crate::error::{CafeError, Result};

/// Time to wait for a service TCP listener.
const READY_TIMEOUT: Duration = Duration::from_secs(30);

/// Options controlling [`run_cafe`].
#[derive(Debug, Clone, Default)]
pub struct CafeOptions {
    /// Do not open a browser after the services are ready.
    pub no_browser: bool,
    /// Directory containing the built SPA. When omitted, `<config-dir>/web` and
    /// `<exe-dir>/web` are tried.
    pub web_root: Option<PathBuf>,
}

/// Fully resolved plan for one `alhalte-cafe` run.
#[derive(Debug, Clone)]
pub struct CafePlan {
    /// Parsed configuration with relative paths made absolute.
    pub config: RestaurantToml,
    /// Configuration file that was read.
    pub config_path: PathBuf,
    /// Directory containing the configuration file (child cwd).
    pub config_dir: PathBuf,
    /// Resolved `alhalte-maitre-d` executable.
    pub maitre_d_bin: PathBuf,
    /// Resolved `alhalte-cellar` executable.
    pub cellar_bin: PathBuf,
    /// Resolved `alhalte-banquette` executable.
    pub banquette_bin: PathBuf,
    /// Arguments for maitre-d.
    pub maitre_d_args: Vec<String>,
    /// Arguments for cellar.
    pub cellar_args: Vec<String>,
    /// Browser URL for the local machine.
    pub url: String,
    /// Best-effort LAN URLs (empty when the instance is loopback-only).
    pub lan_urls: Vec<String>,
    /// Absolute SPA directory.
    pub web_root: PathBuf,
}

/// Loads the configuration, resolves the portable paths, and returns a plan.
///
/// Relative paths in `Restaurant.toml` are resolved against the configuration
/// file's directory, not the process cwd. When `--config` is omitted (or the
/// default `Restaurant.toml` is missing), the file next to the executable is
/// tried.
///
/// # Errors
///
/// Returns [`CafeError::Cafe`] when the configuration is missing, the SPA or a
/// sibling binary cannot be found, or [`RestaurantError`] when the configuration
/// is invalid.
pub fn prepare_cafe(
    config_path: Option<&Path>,
    age_key: Option<&str>,
    web_root_override: Option<&Path>,
) -> Result<CafePlan> {
    let path = resolve_config_path(config_path)?;
    let content = std::fs::read_to_string(&path)?;
    let allow_plaintext = allow_plaintext_secrets_from_env()
        || (!is_rops_encrypted(&content) && declares_local_login(&content));
    if allow_plaintext && !allow_plaintext_secrets_from_env() {
        eprintln!(
            "Notice: allowing plaintext secrets because local-login is enabled. \
             Use age armor for anything reachable outside a trusted LAN."
        );
    }
    let options = ParseOptions {
        age_key,
        allow_plaintext_secrets: allow_plaintext,
    };
    let config = parse_restaurant_toml_with_options(&content, &options)?;
    let config_dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let exe_dir = current_exe_dir()?;
    build_cafe_plan(config, path, config_dir, &exe_dir, web_root_override)
}

/// Builds the cafe plan from an already parsed configuration.
///
/// This is the side-effect-free core used by tests. It absolutizes the paths,
/// resolves the SPA and sibling binaries, validates the result, and generates
/// the two argument lists.
///
/// # Errors
///
/// See [`prepare_cafe`].
pub fn build_cafe_plan(
    mut config: RestaurantToml,
    config_path: PathBuf,
    config_dir: PathBuf,
    exe_dir: &Path,
    web_root_override: Option<&Path>,
) -> Result<CafePlan> {
    config.resolve_relative_paths(&config_dir);

    let web_root = resolve_web_root(&config_dir, exe_dir, web_root_override)?;

    let banquette_bin = exe_dir.join(format!("alhalte-banquette{}", exe_suffix()));
    if !banquette_bin.is_file() {
        return Err(CafeError::Cafe(format!(
            "alhalte-banquette was not found at {}; extract the whole zip/folder",
            banquette_bin.display()
        )));
    }

    config.validate()?;

    let maitre_d_bin = exe_dir.join(format!("alhalte-maitre-d{}", exe_suffix()));
    let cellar_bin = exe_dir.join(format!("alhalte-cellar{}", exe_suffix()));
    for (name, path) in [
        ("alhalte-maitre-d", &maitre_d_bin),
        ("alhalte-cellar", &cellar_bin),
    ] {
        if !path.is_file() {
            return Err(CafeError::Cafe(format!(
                "{name} was not found at {}; extract the whole zip/folder",
                path.display()
            )));
        }
    }

    let url = config.maitre_d_url();
    let lan_urls = if config.is_lan_exposed() {
        crate::lan::detect_lan_urls(config.maitre_d.port)
    } else {
        Vec::new()
    };

    let args_options = MaitreDArgsOptions {
        web_root: Some(web_root.clone()),
        banquette_bin: None,
    };

    Ok(CafePlan {
        maitre_d_args: generate_maitre_d_args(&config, &args_options),
        cellar_args: generate_cellar_args(&config),
        config,
        config_path,
        config_dir,
        maitre_d_bin,
        cellar_bin,
        banquette_bin,
        url,
        lan_urls,
        web_root,
    })
}

/// Runs `alhalte-cafe`.
///
/// # Errors
///
/// Returns [`CafeError`] when preparation fails, a child cannot be spawned, a
/// child exits before becoming ready, or the configuration is invalid.
pub fn run_cafe(
    config_path: Option<&Path>,
    age_key: Option<&str>,
    options: CafeOptions,
) -> Result<()> {
    let plan = prepare_cafe(config_path, age_key, options.web_root.as_deref())?;
    create_directories(&plan)?;
    write_secret_files(&plan.config)?;

    let interrupted = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&interrupted);
    if let Err(error) = ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst)) {
        eprintln!("Warning: could not install a Ctrl+C handler: {error}");
    }

    let mut cellar = spawn_service(&plan.cellar_bin, &plan.cellar_args, &plan.config_dir)?;
    if let Err(error) = wait_for_tcp(
        "127.0.0.1",
        plan.config.cellar.port,
        &mut cellar,
        READY_TIMEOUT,
    ) {
        terminate_child(&mut cellar);
        return Err(error);
    }

    let mut maitre_d =
        match spawn_service(&plan.maitre_d_bin, &plan.maitre_d_args, &plan.config_dir) {
            Ok(child) => child,
            Err(error) => {
                terminate_child(&mut cellar);
                return Err(error);
            }
        };
    if let Err(error) = wait_for_tcp(
        &plan.config.restaurant.host,
        plan.config.maitre_d.port,
        &mut maitre_d,
        READY_TIMEOUT,
    ) {
        terminate_child(&mut maitre_d);
        terminate_child(&mut cellar);
        return Err(error);
    }

    println!("Alhalte is running at {}", plan.url);
    for lan_url in &plan.lan_urls {
        println!("LAN: {lan_url}");
    }
    println!(
        "Press Ctrl+C to stop. Persistence interval: {}s.",
        plan.config.banquette.heartbeat_interval_secs
    );

    if !options.no_browser && std::env::var_os("ALHALTE_NO_BROWSER").is_none() {
        open_browser(&plan.url);
    }

    let mut failure: Option<String> = None;
    loop {
        if interrupted.load(Ordering::SeqCst) {
            break;
        }
        match maitre_d.try_wait() {
            Ok(Some(status)) => {
                failure = Some(format!("alhalte-maitre-d exited: {status}"));
                break;
            }
            Ok(None) => {}
            Err(error) => {
                failure = Some(format!("failed to poll alhalte-maitre-d: {error}"));
                break;
            }
        }
        match cellar.try_wait() {
            Ok(Some(status)) => {
                failure = Some(format!("alhalte-cellar exited: {status}"));
                break;
            }
            Ok(None) => {}
            Err(error) => {
                failure = Some(format!("failed to poll alhalte-cellar: {error}"));
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    terminate_child(&mut maitre_d);
    terminate_child(&mut cellar);

    match failure {
        Some(message) => Err(CafeError::Cafe(message)),
        None => Ok(()),
    }
}

/// Resolves the configuration file, falling back to the executable directory.
fn resolve_config_path(explicit: Option<&Path>) -> Result<PathBuf> {
    let candidate = explicit
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("Restaurant.toml"));
    if candidate.is_file() {
        return Ok(candidate);
    }

    let exe_dir = current_exe_dir()?;
    let fallback = exe_dir.join("Restaurant.toml");
    if fallback.is_file() {
        return Ok(fallback);
    }

    Err(CafeError::Cafe(format!(
        "configuration not found: {} (also tried {})",
        candidate.display(),
        fallback.display()
    )))
}

/// Resolves the SPA directory.
fn resolve_web_root(
    config_dir: &Path,
    exe_dir: &Path,
    web_root_override: Option<&Path>,
) -> Result<PathBuf> {
    let candidates = match web_root_override {
        Some(path) => vec![absolutize_cwd(path)?],
        None => vec![config_dir.join("web"), exe_dir.join("web")],
    };

    for candidate in &candidates {
        if candidate.join("index.html").is_file() {
            return Ok(candidate.clone());
        }
    }

    let checked = candidates
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(CafeError::Cafe(format!(
        "the SPA was not found (index.html is missing; checked {checked}); extract the whole \
         zip/folder or pass --web-root"
    )))
}

/// Creates the data, socket, cellar, and secret directories.
fn create_directories(plan: &CafePlan) -> Result<()> {
    let dirs = [
        plan.config.restaurant.data_dir.clone(),
        plan.config.restaurant.socket_dir.clone(),
        plan.config.cellar_storage_path(),
        plan.config.secrets_dir(),
    ];
    for dir in dirs {
        std::fs::create_dir_all(&dir)?;
    }
    for file in [plan.config.maitre_d_db_path(), plan.config.cellar_db_path()] {
        if let Some(parent) = file.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

/// Spawns a service with inherited stdio in the configuration directory.
fn spawn_service(bin: &Path, args: &[String], cwd: &Path) -> Result<Child> {
    Command::new(bin)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| {
            CafeError::Cafe(format!(
                "failed to start {} ({}): {error}",
                bin.display(),
                args.join(" ")
            ))
        })
}

/// Waits until `host:port` accepts a TCP connection, or the child exits.
fn wait_for_tcp(host: &str, port: u16, child: &mut Child, timeout: Duration) -> Result<()> {
    let host = connect_host(host);
    let addr = (host, port)
        .to_socket_addrs()
        .map_err(|error| CafeError::Cafe(format!("invalid {host}:{port}: {error}")))?
        .next()
        .ok_or_else(|| CafeError::Cafe(format!("could not resolve {host}:{port}")))?;

    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| CafeError::Cafe(format!("failed to poll child: {error}")))?
        {
            return Err(CafeError::Cafe(format!(
                "service exited before listening on {host}:{port}: {status}"
            )));
        }
        if TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok() {
            return Ok(());
        }
        if started.elapsed() >= timeout {
            return Err(CafeError::Cafe(format!(
                "timed out waiting for {host}:{port}"
            )));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Terminates a child, using `taskkill /T` on Windows to include banquettes.
fn terminate_child(child: &mut Child) {
    if matches!(child.try_wait(), Ok(Some(_))) {
        return;
    }
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        let _ = Command::new("taskkill")
            .args(["/PID", pid.as_str(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Opens `url` in the platform browser. Failures are non-fatal.
fn open_browser(url: &str) {
    #[cfg(windows)]
    {
        let _ = Command::new("cmd")
            .args(["/C", "start", "", url])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = Command::new("xdg-open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

/// Returns the current executable's directory.
fn current_exe_dir() -> Result<PathBuf> {
    std::env::current_exe()?
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| CafeError::Cafe("could not determine the executable directory".to_string()))
}

/// Resolves `path` against the process cwd unless it is already absolute.
fn absolutize_cwd(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

/// Platform executable suffix.
fn exe_suffix() -> &'static str {
    std::env::consts::EXE_SUFFIX
}

/// Maps wildcard bind hosts to a connectable loopback address.
fn connect_host(host: &str) -> &str {
    match host {
        "0.0.0.0" | "::" => "127.0.0.1",
        other => other,
    }
}

/// Returns true when the TOML document enables `[maitre-d] local-login`.
///
/// Used only to decide whether plaintext dev secrets may be accepted. A rops
/// document is skipped by the caller.
fn declares_local_login(content: &str) -> bool {
    let Ok(value) = content.parse::<toml::Value>() else {
        return false;
    };
    for section in ["maitre-d", "maitre_d"] {
        let Some(table) = value.get(section).and_then(toml::Value::as_table) else {
            continue;
        };
        for key in ["local-login", "local_login"] {
            if table.get(key).and_then(toml::Value::as_bool) == Some(true) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use alhalte_restaurant::ParseOptions;

    fn dev_config() -> RestaurantToml {
        alhalte_restaurant::parse_restaurant_toml_with_options(
            r#"
[restaurant]
host = "0.0.0.0"
data-dir = "data"
socket-dir = "sockets"

[maitre-d]
port = 8080
salt = "salt"
local-login = true

[cellar]
port = 8081
service-token = "token"

[banquette]
heartbeat-interval-secs = 60
"#,
            &ParseOptions {
                age_key: None,
                allow_plaintext_secrets: true,
            },
        )
        .unwrap()
    }

    #[test]
    fn detects_local_login_in_kebab_and_snake_case() {
        assert!(declares_local_login("[maitre-d]\nlocal-login = true\n"));
        assert!(declares_local_login("[maitre_d]\nlocal_login = true\n"));
        assert!(!declares_local_login("[maitre-d]\nlocal-login = false\n"));
        assert!(!declares_local_login("not toml"));
    }

    #[test]
    fn absolutizes_against_the_config_directory() {
        let mut config = dev_config();
        config.resolve_relative_paths(Path::new("/srv/alhalte"));
        assert_eq!(
            config.restaurant.data_dir,
            PathBuf::from("/srv/alhalte/data")
        );
        assert_eq!(
            config.restaurant.socket_dir,
            PathBuf::from("/srv/alhalte/sockets")
        );
    }

    #[test]
    fn builds_a_plan_with_absolute_paths_and_local_login() {
        let dir = tempfile::tempdir().unwrap();
        let exe_dir = dir.path().join("package");
        let config_dir = exe_dir.clone();
        std::fs::create_dir_all(&config_dir).unwrap();
        for name in ["alhalte-maitre-d", "alhalte-cellar", "alhalte-banquette"] {
            std::fs::write(exe_dir.join(format!("{name}{}", exe_suffix())), b"").unwrap();
        }
        std::fs::create_dir_all(config_dir.join("web")).unwrap();
        std::fs::write(config_dir.join("web/index.html"), b"index").unwrap();

        let plan = build_cafe_plan(
            dev_config(),
            config_dir.join("Restaurant.toml"),
            config_dir.clone(),
            &exe_dir,
            None,
        )
        .unwrap();

        assert_eq!(plan.config.restaurant.data_dir, config_dir.join("data"));
        assert_eq!(plan.web_root, config_dir.join("web"));
        assert_eq!(
            plan.banquette_bin,
            exe_dir.join(format!("alhalte-banquette{}", exe_suffix()))
        );
        assert!(plan.maitre_d_args.iter().any(|arg| arg == "--local-login"));
        assert!(
            plan.maitre_d_args
                .iter()
                .any(|arg| arg == "--banquette-heartbeat-interval-secs")
        );
        assert!(
            plan.maitre_d_args
                .windows(2)
                .any(|pair| pair[0] == "--web-root"
                    && pair[1] == plan.web_root.display().to_string())
        );
        assert!(
            !plan
                .maitre_d_args
                .iter()
                .any(|arg| arg == "--banquette-bin"),
            "maitre-d resolves its sibling banquette binary"
        );
    }

    #[test]
    fn honors_an_explicit_web_root() {
        let dir = tempfile::tempdir().unwrap();
        let exe_dir = dir.path().join("package");
        let config_dir = exe_dir.clone();
        let spa = dir.path().join("dist-app");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::create_dir_all(&spa).unwrap();
        std::fs::write(spa.join("index.html"), b"index").unwrap();
        for name in ["alhalte-maitre-d", "alhalte-cellar", "alhalte-banquette"] {
            std::fs::write(exe_dir.join(format!("{name}{}", exe_suffix())), b"").unwrap();
        }

        let plan = build_cafe_plan(
            dev_config(),
            config_dir.join("Restaurant.toml"),
            config_dir,
            &exe_dir,
            Some(&spa),
        )
        .unwrap();
        assert_eq!(plan.web_root, spa);
    }

    #[test]
    fn rejects_a_missing_spa() {
        let dir = tempfile::tempdir().unwrap();
        let exe_dir = dir.path().join("package");
        std::fs::create_dir_all(&exe_dir).unwrap();
        for name in ["alhalte-maitre-d", "alhalte-cellar", "alhalte-banquette"] {
            std::fs::write(exe_dir.join(format!("{name}{}", exe_suffix())), b"").unwrap();
        }
        let error = build_cafe_plan(
            dev_config(),
            exe_dir.join("Restaurant.toml"),
            exe_dir.clone(),
            &exe_dir,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("index.html is missing"));
    }
}
