//! Specification conformance tests for `alhalte-bistrot`.

use age::secrecy::ExposeSecret;
use age::x25519::Identity;
use alhalte_bistrot::{
    SystemdOptions, generate_alhalte_target, generate_cellar_service, generate_maitre_d_service,
    generate_systemd_units,
};
use alhalte_restaurant::{
    ParseOptions, RestaurantToml, encrypt_age_armored_value, parse_restaurant_toml_with_options,
};
use std::path::PathBuf;

fn dev_config() -> RestaurantToml {
    parse_restaurant_toml_with_options(
        r#"
[restaurant]
domain = "vtt.alhalte.example"
host = "127.0.0.1"
data-dir = "/var/lib/alhalte"
socket-dir = "/run/alhalte"

[maitre-d]
port = 8080
client-id = "google-client"
client-secret = "google-secret"
salt = "salt-value"

[cellar]
port = 8081
anti-oracle = true
service-token = "internal-service-token"
"#,
        &ParseOptions {
            age_key: None,
            allow_plaintext_secrets: true,
        },
    )
    .unwrap()
}

// spec: alhalte-bistrot/systemd-generator
#[test]
fn generates_two_services_and_a_target() {
    let config = dev_config();
    let options = SystemdOptions {
        bin_dir: PathBuf::from("/opt/alhalte/bin"),
        working_dir: Some(PathBuf::from("/var/lib/alhalte")),
        web_root: Some(PathBuf::from("/srv/alhalte/spa")),
        user: Some("alhalte".to_string()),
        group: Some("alhalte".to_string()),
        restart: "always".to_string(),
        restart_sec: "10s".to_string(),
    };

    let maitre = generate_maitre_d_service(&config, &options);
    assert!(maitre.contains("Description=Alhalte Maitre-d"));
    assert!(maitre.contains("PartOf=alhalte.target"));
    assert!(
        maitre.contains("ExecStart=/opt/alhalte/bin/alhalte-maitre-d --host 127.0.0.1 --port 8080")
    );
    assert!(maitre.contains("--web-root /srv/alhalte/spa"));
    assert!(maitre.contains("User=alhalte"));
    assert!(maitre.contains("Group=alhalte"));
    assert!(maitre.contains("WorkingDirectory=/var/lib/alhalte"));
    assert!(maitre.contains("Restart=always"));
    assert!(maitre.contains("RestartSec=10s"));
    assert!(maitre.contains("WantedBy=alhalte.target"));
    assert!(!maitre.contains("argenterie"));
    assert!(!maitre.contains("google-secret"));
    assert!(!maitre.contains("internal-service-token"));

    let cellar = generate_cellar_service(&config, &options);
    assert!(cellar.contains("Description=Alhalte Cellar"));
    assert!(cellar.contains("PartOf=alhalte.target"));
    assert!(
        cellar.contains("ExecStart=/opt/alhalte/bin/alhalte-cellar --host 127.0.0.1 --port 8081")
    );
    assert!(cellar.contains("--anti-oracle"));
    assert!(cellar.contains("--maitre-d-url http://127.0.0.1:8080"));
    assert!(cellar.contains("WantedBy=alhalte.target"));
    assert!(!cellar.contains("internal-service-token"));

    let target = generate_alhalte_target();
    assert!(target.contains("Description=Alhalte Restaurant Suite Target"));
    assert!(target.contains("Wants=alhalte-cellar.service alhalte-maitre-d.service"));
    assert!(target.contains("WantedBy=multi-user.target"));
    assert!(!target.contains("argenterie"));

    let temp_dir = tempfile::tempdir().unwrap();
    let units = generate_systemd_units(&config, &options);
    units.write_to_dir(temp_dir.path()).unwrap();
    assert!(temp_dir.path().join("alhalte-maitre-d.service").exists());
    assert!(temp_dir.path().join("alhalte-cellar.service").exists());
    assert!(temp_dir.path().join("alhalte.target").exists());
    assert!(!temp_dir.path().join("alhalte-argenterie.service").exists());
    assert!(!temp_dir.path().join("alhalte-bistrot.target").exists());
    assert_eq!(
        std::fs::read_to_string(temp_dir.path().join("alhalte.target")).unwrap(),
        target
    );
}

// spec: alhalte-bistrot/systemd-generator escaping
#[test]
fn escapes_exec_start_arguments() {
    let config = dev_config();
    let options = SystemdOptions {
        web_root: Some(PathBuf::from("/srv/with space/\"quoted\"")),
        ..Default::default()
    };
    let unit = generate_maitre_d_service(&config, &options);
    assert!(
        unit.contains("--web-root \"/srv/with space/\\\"quoted\\\"\""),
        "unit was:\n{unit}"
    );
}

#[test]
fn omitted_web_root_does_not_emit_the_flag() {
    let config = dev_config();
    let unit = generate_maitre_d_service(&config, &SystemdOptions::default());
    assert!(!unit.contains("--web-root"));
}

// spec: alhalte-bistrot/cli
#[test]
fn cli_generates_units_validates_and_implements_the_removed_commands() {
    let bin_path = env!("CARGO_BIN_EXE_alhalte-bistrot");
    let bistrot_cmd = || {
        let mut cmd = std::process::Command::new(bin_path);
        cmd.env("ALHALTE_ALLOW_PLAINTEXT_SECRETS", "1");
        cmd
    };
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("Restaurant.toml");
    let data_dir = temp_dir.path().join("data");
    let spa_dir = temp_dir.path().join("spa");
    std::fs::create_dir_all(&spa_dir).unwrap();
    std::fs::write(spa_dir.join("index.html"), b"index").unwrap();

    let plain_toml = format!(
        r#"
[restaurant]
domain = "cli.example.com"
host = "127.0.0.1"
data-dir = "{}"

[maitre-d]
client-id = "cli-client"
client-secret = "cli-secret-oauth"
salt = "cli-salt"

[cellar]
anti-oracle = true
"#,
        data_dir.display()
    );
    std::fs::write(&config_path, plain_toml).unwrap();

    let output = bistrot_cmd()
        .arg("-c")
        .arg(&config_path)
        .arg("validate")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cli.example.com"));
    assert!(stdout.contains("UI url:        http://127.0.0.1:8080/"));

    let units_dir = temp_dir.path().join("systemd_units");
    let output = bistrot_cmd()
        .arg("-c")
        .arg(&config_path)
        .arg("systemd")
        .arg("--output-dir")
        .arg(&units_dir)
        .arg("--web-root")
        .arg(&spa_dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(units_dir.join("alhalte-maitre-d.service").exists());
    assert!(units_dir.join("alhalte-cellar.service").exists());
    assert!(units_dir.join("alhalte.target").exists());
    assert!(!units_dir.join("alhalte-argenterie.service").exists());
    let maitre_unit = std::fs::read_to_string(units_dir.join("alhalte-maitre-d.service")).unwrap();
    assert!(maitre_unit.contains(&format!("--web-root {}", spa_dir.display())));

    // Secrets are materialized as 0600 files, and the unit only points at them.
    let client_secret_path = data_dir.join("secrets/maitre-d-client-secret");
    assert_eq!(
        std::fs::read_to_string(&client_secret_path).unwrap().trim(),
        "cli-secret-oauth"
    );
    assert!(!maitre_unit.contains("cli-secret-oauth"));

    // Removed subcommands are no longer accepted.
    for removed in ["serve", "url", "args", "stage-extensions"] {
        let output = bistrot_cmd()
            .arg("-c")
            .arg(&config_path)
            .arg(removed)
            .output()
            .unwrap();
        assert!(!output.status.success(), "`{removed}` must be removed");
    }

    // Armored values round-trip through `encrypt` / `encrypt-value`.
    let identity = Identity::generate();
    let recipient = identity.to_public().to_string();
    let secret = identity.to_string();
    let enc_path = temp_dir.path().join("EncRestaurant.toml");
    let output = bistrot_cmd()
        .arg("-c")
        .arg(&config_path)
        .arg("encrypt")
        .arg("--recipient")
        .arg(&recipient)
        .arg("-o")
        .arg(&enc_path)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(enc_path.exists());

    let output = bistrot_cmd()
        .arg("-c")
        .arg(&enc_path)
        .arg("--age-key")
        .arg(secret.expose_secret())
        .arg("validate")
        .output()
        .unwrap();
    assert!(output.status.success());

    let mut child = bistrot_cmd()
        .arg("encrypt-value")
        .arg("--recipient")
        .arg(&recipient)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"value-secret\n")
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let armored = String::from_utf8_lossy(&output.stdout);
    assert!(armored.starts_with(alhalte_restaurant::AGE_ARMOR_BEGIN));
    assert!(!armored.contains("value-secret"));

    // The armored value parses without the plaintext escape hatch.
    let armored_config = format!(
        r#"
[restaurant]
data-dir = "{data_dir}"

[maitre-d]
client-id = "cli-client"
client-secret = """
{armored}"""
salt = "cli-salt"

[cellar]
"#,
        data_dir = data_dir.display(),
        armored = armored
    );
    let armored_config_path = temp_dir.path().join("ArmoredRestaurant.toml");
    std::fs::write(&armored_config_path, armored_config).unwrap();
    let output = std::process::Command::new(bin_path)
        .arg("-c")
        .arg(&armored_config_path)
        .arg("--age-key")
        .arg(secret.expose_secret())
        .arg("validate")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// spec: alhalte-bistrot/secrets 0008 armored placeholders stay fail-closed
#[test]
fn armored_placeholder_is_not_a_plaintext_secret() {
    let armored =
        encrypt_age_armored_value("secret", &Identity::generate().to_public().to_string()).unwrap();
    let toml_str = format!(
        r#"
[maitre-d]
client-id = "id"
client-secret = """
{armored}"""
salt = "salt"

[cellar]
"#
    );
    let err = parse_restaurant_toml_with_options(&toml_str, &ParseOptions::default()).unwrap_err();
    assert!(matches!(
        err,
        alhalte_restaurant::RestaurantError::AgeKeyMissing
    ));
}
