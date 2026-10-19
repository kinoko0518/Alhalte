//! Integration and specification conformance tests for `alhalte-bistrot`.

use age::secrecy::ExposeSecret;
use age::x25519::Identity;
use alhalte_bistrot::args::{
    generate_all_args, generate_cellar_args, generate_maitre_d_args, generate_module_args,
};
use alhalte_bistrot::config::{
    ArgenterieConfig, BanquetteConfig, BistrotConfig, CellarConfig, ExtensionManifestEntry,
    MaitreDConfig, RestaurantConfig,
};
use alhalte_bistrot::rops::{
    decrypt_rops_toml_with_key, encrypt_bistrot_toml, is_rops_encrypted, load_from_path,
    load_from_path_with_key, parse_bistrot_toml, parse_bistrot_toml_with_key,
};
use alhalte_bistrot::systemd::{
    SystemdOptions, generate_argenterie_service, generate_bistrot_target, generate_cellar_service,
    generate_maitre_d_service, generate_systemd_units,
};
use std::path::PathBuf;

// spec: alhalte-bistrot/config-parser
#[test]
fn test_config_parser_full_and_defaults() {
    let toml_str = r#"
[restaurant]
domain = "vtt.alhalte.example"
host = "0.0.0.0"
data-dir = "/data/alhalte"
socket-dir = "/run/alhalte"

[maitre-d]
port = 9000
db-path = "/data/alhalte/custom-maitre.sqlite"
client-id = "google-client-12345"
client-secret = "super-secret-oauth"
salt = "instance-salt-67890"

[cellar]
port = 9001
storage-path = "/data/alhalte/tin-store"
db-path = "/data/alhalte/custom-cellar.sqlite"
anti-oracle = true

[banquette]
chunk-size-exponent = 10
"#;

    let config = parse_bistrot_toml(toml_str).unwrap();

    assert_eq!(config.restaurant.domain, "vtt.alhalte.example");
    assert_eq!(config.restaurant.host, "0.0.0.0");
    assert_eq!(config.restaurant.data_dir, PathBuf::from("/data/alhalte"));
    assert_eq!(config.restaurant.socket_dir, PathBuf::from("/run/alhalte"));

    assert_eq!(config.maitre_d.port, 9000);
    assert_eq!(
        config.maitre_d_db_path(),
        PathBuf::from("/data/alhalte/custom-maitre.sqlite")
    );
    assert_eq!(config.maitre_d.client_id, "google-client-12345");
    assert_eq!(config.maitre_d.client_secret, "super-secret-oauth");
    assert_eq!(config.maitre_d.salt, "instance-salt-67890");
    assert_eq!(config.maitre_d_socket_dir(), PathBuf::from("/run/alhalte"));

    assert_eq!(config.cellar.port, 9001);
    assert_eq!(
        config.cellar_storage_path(),
        PathBuf::from("/data/alhalte/tin-store")
    );
    assert_eq!(
        config.cellar_db_path(),
        PathBuf::from("/data/alhalte/custom-cellar.sqlite")
    );
    assert!(config.cellar.anti_oracle);

    assert_eq!(config.banquette.chunk_size_exponent, 10);
    assert_eq!(config.banquette_socket_dir(), PathBuf::from("/run/alhalte"));
}

// spec: alhalte-bistrot/config-parser
#[test]
fn test_config_parser_defaults_when_omitted() {
    let toml_str = r#"
[maitre-d]
client-id = "test-client"
client-secret = "test-secret"
salt = "test-salt"

[cellar]
"#;

    let config = parse_bistrot_toml(toml_str).unwrap();

    assert_eq!(config.restaurant.domain, "localhost");
    assert_eq!(config.restaurant.host, "127.0.0.1");
    assert_eq!(
        config.restaurant.data_dir,
        PathBuf::from("/var/lib/alhalte")
    );
    assert_eq!(config.restaurant.socket_dir, PathBuf::from("/run/alhalte"));

    assert_eq!(config.maitre_d.port, 8080);
    assert_eq!(
        config.maitre_d_db_path(),
        PathBuf::from("/var/lib/alhalte/maitre-d.sqlite3")
    );
    assert_eq!(config.maitre_d_socket_dir(), PathBuf::from("/run/alhalte"));

    assert_eq!(config.cellar.port, 8081);
    assert_eq!(
        config.cellar_storage_path(),
        PathBuf::from("/var/lib/alhalte/cellar/tin")
    );
    assert_eq!(
        config.cellar_db_path(),
        PathBuf::from("/var/lib/alhalte/cellar/cellar.sqlite3")
    );
    assert!(!config.cellar.anti_oracle);

    // spec: alhalte-banquette/chunk-size-exponent
    assert_eq!(config.banquette.chunk_size_exponent, 8);
    assert_eq!(config.banquette_socket_dir(), PathBuf::from("/run/alhalte"));
}

// spec: alhalte-bistrot/config-parser
#[test]
fn test_config_parser_snake_case_compatibility() {
    let toml_str = r#"
[restaurant]
domain = "snake.example"
host = "127.0.0.2"
data_dir = "/srv/alhalte"
socket_dir = "/var/run/alhalte"

[maitre_d]
port = 8888
client_id = "client-snake"
client_secret = "secret-snake"
salt = "salt-snake"

[cellar]
port = 8889
storage_path = "/srv/alhalte/storage"
db_path = "/srv/alhalte/db.sqlite"
anti_oracle = false

[banquette]
chunk_size_exponent = 9
"#;

    let config = parse_bistrot_toml(toml_str).unwrap();
    assert_eq!(config.restaurant.domain, "snake.example");
    assert_eq!(config.maitre_d.port, 8888);
    assert_eq!(config.maitre_d.client_id, "client-snake");
    assert_eq!(config.cellar.port, 8889);
    assert_eq!(config.banquette.chunk_size_exponent, 9);
}

// spec: alhalte-bistrot/config-parser
#[test]
fn test_config_validation_failures() {
    let empty_client_id = r#"
[maitre-d]
client-id = ""
client-secret = "secret"
salt = "salt"
[cellar]
"#;
    assert!(parse_bistrot_toml(empty_client_id).is_err());

    let empty_secret = r#"
[maitre-d]
client-id = "id"
client-secret = ""
salt = "salt"
[cellar]
"#;
    assert!(parse_bistrot_toml(empty_secret).is_err());

    let empty_salt = r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = ""
[cellar]
"#;
    assert!(parse_bistrot_toml(empty_salt).is_err());

    let invalid_exp = r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = "salt"
[cellar]
[banquette]
chunk-size-exponent = 0
"#;
    assert!(parse_bistrot_toml(invalid_exp).is_err());
}

// spec: alhalte-bistrot/rops-decryption
#[test]
fn test_rops_encryption_and_decryption_roundtrip() {
    let identity = Identity::generate();
    let recipient = identity.to_public().to_string();
    let secret = identity.to_string();

    let plain_toml = r#"
[restaurant]
domain = "vtt.secure.example"
host = "127.0.0.1"

[maitre-d]
client-id = "secure-client-id"
client-secret = "super-secret-token-xyz"
salt = "random-salt-12345678"

[cellar]
anti-oracle = true
"#;

    // Encrypt plain TOML using age recipient public key
    let encrypted_toml = encrypt_bistrot_toml(plain_toml, &recipient).unwrap();
    assert!(is_rops_encrypted(&encrypted_toml));
    assert!(encrypted_toml.contains("sops"));
    assert!(!encrypted_toml.contains("super-secret-token-xyz"));

    // Decrypt using the age secret key
    let decrypted_str =
        decrypt_rops_toml_with_key(&encrypted_toml, secret.expose_secret()).unwrap();
    assert!(decrypted_str.contains("super-secret-token-xyz"));

    // Parse into BistrotConfig using parse_bistrot_toml_with_key
    let config =
        parse_bistrot_toml_with_key(&encrypted_toml, Some(secret.expose_secret())).unwrap();
    assert_eq!(config.restaurant.domain, "vtt.secure.example");
    assert_eq!(config.maitre_d.client_secret, "super-secret-token-xyz");
    assert_eq!(config.maitre_d.salt, "random-salt-12345678");
    assert!(config.cellar.anti_oracle);

    // Verifying decryption failure with invalid key
    let bad_identity = Identity::generate();
    let bad_secret = bad_identity.to_string();
    let err =
        parse_bistrot_toml_with_key(&encrypted_toml, Some(bad_secret.expose_secret())).unwrap_err();
    assert!(matches!(err, alhalte_bistrot::BistrotError::RopsDecrypt(_)));
}

// spec: alhalte-bistrot/rops-decryption
#[test]
fn test_load_from_path() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("Bistrot.toml");

    let plain_toml = r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = "salt"
[cellar]
"#;
    std::fs::write(&config_path, plain_toml).unwrap();

    let config = load_from_path(&config_path).unwrap();
    assert_eq!(config.maitre_d.client_id, "id");
    assert_eq!(config.maitre_d.client_secret, "secret");

    // Test load_from_path_with_key on encrypted file
    let identity = Identity::generate();
    let recipient = identity.to_public().to_string();
    let secret = identity.to_string();

    let enc_path = temp_dir.path().join("EncBistrot.toml");
    let encrypted = encrypt_bistrot_toml(plain_toml, &recipient).unwrap();
    std::fs::write(&enc_path, encrypted).unwrap();

    let config_dec = load_from_path_with_key(&enc_path, Some(secret.expose_secret())).unwrap();
    assert_eq!(config_dec.maitre_d.client_id, "id");
    assert_eq!(config_dec.maitre_d.client_secret, "secret");
}

// spec: alhalte-maitre-d/oauth-args
#[test]
fn test_generate_maitre_d_args() {
    let config = BistrotConfig {
        restaurant: RestaurantConfig {
            domain: "vtt.example.com".to_string(),
            host: "0.0.0.0".to_string(),
            data_dir: PathBuf::from("/data"),
            socket_dir: PathBuf::from("/run/sockets"),
        },
        maitre_d: MaitreDConfig {
            port: 8080,
            db_path: None,
            client_id: "google-client".to_string(),
            client_secret: "google-secret".to_string(),
            salt: "instancesalt".to_string(),
            socket_dir: None,
            banquette_bin: None,
            cellar_url: None,
        },
        cellar: CellarConfig {
            port: 8081,
            storage_path: None,
            db_path: None,
            anti_oracle: false,
            maitre_d_url: None,
            maitre_d_socket: None,
            allow_all_sessions: false,
        },
        banquette: BanquetteConfig::default(),
        argenterie: ArgenterieConfig::default(),
        extensions: vec![],
    };

    let args = generate_maitre_d_args(&config);
    assert_eq!(
        args,
        vec![
            "--host",
            "0.0.0.0",
            "--port",
            "8080",
            "--db-path",
            "/data/maitre-d.sqlite3",
            "--client-id",
            "google-client",
            "--client-secret",
            "google-secret",
            "--salt",
            "instancesalt",
            "--socket-dir",
            "/run/sockets",
            "--cellar-url",
            "http://127.0.0.1:8081",
            "--chunk-size-exponent",
            "8",
        ]
    );

    // The chunk size exponent comes from `[banquette]` in `Bistrot.toml`.
    let mut custom_exponent = config.clone();
    custom_exponent.banquette.chunk_size_exponent = 12;
    let custom_args = generate_maitre_d_args(&custom_exponent);
    let exponent_index = custom_args
        .iter()
        .position(|arg| arg == "--chunk-size-exponent")
        .expect("--chunk-size-exponent must be emitted");
    assert_eq!(custom_args[exponent_index + 1], "12");

    // With explicit banquette_bin and cellar_url overrides
    let mut custom_config = config.clone();
    custom_config.maitre_d.banquette_bin = Some(PathBuf::from("/usr/bin/alhalte-banquette"));
    custom_config.maitre_d.cellar_url = Some("http://cellar.internal:9000".to_string());
    let custom_args = generate_maitre_d_args(&custom_config);
    assert!(custom_args.contains(&"--cellar-url".to_string()));
    assert!(custom_args.contains(&"http://cellar.internal:9000".to_string()));
    assert!(custom_args.contains(&"--banquette-bin".to_string()));
    assert!(custom_args.contains(&"/usr/bin/alhalte-banquette".to_string()));
}

// spec: alhalte-cellar/anti-oracle
#[test]
fn test_generate_cellar_args_with_and_without_anti_oracle() {
    let mut config = BistrotConfig {
        restaurant: RestaurantConfig::default(),
        maitre_d: MaitreDConfig {
            port: 8080,
            db_path: None,
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
            salt: "salt".to_string(),
            socket_dir: None,
            banquette_bin: None,
            cellar_url: None,
        },
        cellar: CellarConfig {
            port: 8081,
            storage_path: Some(PathBuf::from("/custom/tin")),
            db_path: Some(PathBuf::from("/custom/cellar.db")),
            anti_oracle: false,
            maitre_d_url: None,
            maitre_d_socket: None,
            allow_all_sessions: false,
        },
        banquette: BanquetteConfig::default(),
        argenterie: ArgenterieConfig::default(),
        extensions: vec![],
    };

    let args_without = generate_cellar_args(&config);
    assert_eq!(
        args_without,
        vec![
            "--host",
            "127.0.0.1",
            "--port",
            "8081",
            "--storage-path",
            "/custom/tin",
            "--db-path",
            "/custom/cellar.db",
            "--maitre-d-url",
            "http://127.0.0.1:8080",
        ]
    );
    assert!(!args_without.contains(&"--anti-oracle".to_string()));
    assert!(!args_without.contains(&"--allow-all-sessions".to_string()));

    // When anti-oracle is true:
    config.cellar.anti_oracle = true;
    let args_with = generate_cellar_args(&config);
    assert_eq!(
        args_with,
        vec![
            "--host",
            "127.0.0.1",
            "--port",
            "8081",
            "--storage-path",
            "/custom/tin",
            "--db-path",
            "/custom/cellar.db",
            "--maitre-d-url",
            "http://127.0.0.1:8080",
            "--anti-oracle",
        ]
    );
}

// spec: alhalte-bistrot/cellar-session-verifier (Issue #153)
#[test]
fn test_cellar_session_verifier_config_and_args() {
    fn arg_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        let index = args.iter().position(|arg| arg == flag)?;
        args.get(index + 1).map(String::as_str)
    }

    // Defaults: the verifier URL is derived from restaurant.host and maitre-d.port,
    // and wildcard bind addresses are dialed through loopback.
    let toml_str = r#"
[restaurant]
host = "0.0.0.0"

[maitre-d]
port = 9000
client-id = "id"
client-secret = "secret"
salt = "salt"

[cellar]
"#;
    let config = parse_bistrot_toml(toml_str).unwrap();
    assert_eq!(config.cellar.maitre_d_url, None);
    assert_eq!(config.cellar.maitre_d_socket, None);
    assert!(!config.cellar.allow_all_sessions);
    assert_eq!(config.cellar_maitre_d_url(), "http://127.0.0.1:9000");

    let args = generate_cellar_args(&config);
    assert_eq!(
        arg_after(&args, "--maitre-d-url"),
        Some("http://127.0.0.1:9000")
    );
    assert!(!args.contains(&"--maitre-d-socket".to_string()));
    assert!(!args.contains(&"--allow-all-sessions".to_string()));

    // The IPv6 wildcard is also rewritten.
    let toml_str = r#"
[restaurant]
host = "::"

[maitre-d]
client-id = "id"
client-secret = "secret"
salt = "salt"

[cellar]
"#;
    let config = parse_bistrot_toml(toml_str).unwrap();
    assert_eq!(config.cellar_maitre_d_url(), "http://127.0.0.1:8080");

    // Explicit HTTP URL and the local-development allow-all switch.
    let toml_str = r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = "salt"

[cellar]
maitre-d-url = "http://maitre-d.internal:8080"
allow-all-sessions = true
"#;
    let config = parse_bistrot_toml(toml_str).unwrap();
    assert_eq!(
        config.cellar.maitre_d_url.as_deref(),
        Some("http://maitre-d.internal:8080")
    );
    assert!(config.cellar.allow_all_sessions);
    let args = generate_cellar_args(&config);
    assert_eq!(
        arg_after(&args, "--maitre-d-url"),
        Some("http://maitre-d.internal:8080")
    );
    assert!(args.contains(&"--allow-all-sessions".to_string()));

    // A UNIX socket takes precedence over the derived URL.
    let toml_str = r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = "salt"

[cellar]
maitre_d_socket = "/run/alhalte/maitre-d.sock"
allow_all_sessions = false
"#;
    let config = parse_bistrot_toml(toml_str).unwrap();
    assert_eq!(
        config.cellar.maitre_d_socket,
        Some(PathBuf::from("/run/alhalte/maitre-d.sock"))
    );
    let args = generate_cellar_args(&config);
    assert_eq!(
        arg_after(&args, "--maitre-d-socket"),
        Some("/run/alhalte/maitre-d.sock")
    );
    assert!(!args.contains(&"--maitre-d-url".to_string()));
    assert!(!args.contains(&"--allow-all-sessions".to_string()));
}

// spec: alhalte-bistrot/module-args
#[test]
fn test_generate_all_args_and_module_args() {
    let config = BistrotConfig {
        restaurant: RestaurantConfig::default(),
        maitre_d: MaitreDConfig {
            port: 8080,
            db_path: None,
            client_id: "id".to_string(),
            client_secret: "sec".to_string(),
            salt: "salt".to_string(),
            socket_dir: None,
            banquette_bin: None,
            cellar_url: None,
        },
        cellar: CellarConfig {
            port: 8081,
            storage_path: None,
            db_path: None,
            anti_oracle: false,
            maitre_d_url: None,
            maitre_d_socket: None,
            allow_all_sessions: false,
        },
        banquette: BanquetteConfig::default(),
        argenterie: ArgenterieConfig::default(),
        extensions: vec![],
    };

    let all = generate_all_args(&config);
    assert_eq!(all.maitre_d, generate_maitre_d_args(&config));
    assert_eq!(all.cellar, generate_cellar_args(&config));

    assert_eq!(
        generate_module_args(&config, "maitre-d").unwrap(),
        all.maitre_d
    );
    assert_eq!(
        generate_module_args(&config, "maitre_d").unwrap(),
        all.maitre_d
    );
    assert_eq!(generate_module_args(&config, "cellar").unwrap(), all.cellar);
    // Banquette is spawned per room by maitre-d and has no static argument list.
    assert!(generate_module_args(&config, "banquette").is_err());
    assert!(generate_module_args(&config, "unknown").is_err());
}

// spec: alhalte-bistrot/systemd-generator
#[test]
fn test_systemd_generator() {
    let config = BistrotConfig {
        restaurant: RestaurantConfig {
            domain: "vtt.alhalte.example".to_string(),
            host: "127.0.0.1".to_string(),
            data_dir: PathBuf::from("/var/lib/alhalte"),
            socket_dir: PathBuf::from("/run/alhalte"),
        },
        maitre_d: MaitreDConfig {
            port: 8080,
            db_path: None,
            client_id: "google-client".to_string(),
            client_secret: "google-secret".to_string(),
            salt: "salt-value".to_string(),
            socket_dir: None,
            banquette_bin: None,
            cellar_url: None,
        },
        cellar: CellarConfig {
            port: 8081,
            storage_path: None,
            db_path: None,
            anti_oracle: true,
            maitre_d_url: None,
            maitre_d_socket: None,
            allow_all_sessions: false,
        },
        banquette: BanquetteConfig::default(),
        argenterie: ArgenterieConfig::default(),
        extensions: vec![],
    };

    let options = SystemdOptions {
        bin_dir: PathBuf::from("/opt/alhalte/bin"),
        working_dir: Some(PathBuf::from("/var/lib/alhalte")),
        user: Some("alhalte".to_string()),
        group: Some("alhalte".to_string()),
        restart: "always".to_string(),
        restart_sec: "10s".to_string(),
        ..Default::default()
    };

    // Maitre-d service unit
    let maitre_service = generate_maitre_d_service(&config, &options);
    assert!(maitre_service.contains("Description=Alhalte Maitre-d"));
    assert!(maitre_service.contains("PartOf=alhalte-bistrot.target"));
    assert!(
        maitre_service
            .contains("ExecStart=/opt/alhalte/bin/alhalte-maitre-d --host 127.0.0.1 --port 8080")
    );
    assert!(maitre_service.contains("User=alhalte"));
    assert!(maitre_service.contains("Group=alhalte"));
    assert!(maitre_service.contains("WorkingDirectory=/var/lib/alhalte"));
    assert!(maitre_service.contains("Restart=always"));
    assert!(maitre_service.contains("RestartSec=10s"));
    assert!(maitre_service.contains("WantedBy=alhalte-bistrot.target"));

    // Cellar service unit
    let cellar_service = generate_cellar_service(&config, &options);
    assert!(cellar_service.contains("Description=Alhalte Cellar"));
    assert!(cellar_service.contains("PartOf=alhalte-bistrot.target"));
    assert!(
        cellar_service
            .contains("ExecStart=/opt/alhalte/bin/alhalte-cellar --host 127.0.0.1 --port 8081")
    );
    assert!(cellar_service.contains("--anti-oracle"));
    assert!(cellar_service.contains("--maitre-d-url http://127.0.0.1:8080"));
    assert!(cellar_service.contains("User=alhalte"));
    assert!(cellar_service.contains("WantedBy=alhalte-bistrot.target"));

    // Argenterie service unit
    let argenterie_service = generate_argenterie_service(&config, &options);
    assert!(argenterie_service.contains("Description=Alhalte Argenterie"));
    assert!(argenterie_service.contains("PartOf=alhalte-bistrot.target"));
    assert!(argenterie_service.contains("ExecStart=pnpm run dev --host 127.0.0.1 --port 5173"));
    assert!(
        argenterie_service.contains("WorkingDirectory=/var/lib/alhalte/web/alhalte-argenterie")
    );
    assert!(argenterie_service.contains("User=alhalte"));
    assert!(argenterie_service.contains("WantedBy=alhalte-bistrot.target"));

    // Target unit
    let target = generate_bistrot_target();
    assert!(target.contains("Description=Alhalte Bistrot Restaurant Suite Target"));
    assert!(target.contains(
        "Wants=alhalte-maitre-d.service alhalte-cellar.service alhalte-argenterie.service"
    ));
    assert!(target.contains("WantedBy=multi-user.target"));

    // Writing to directory
    let temp_dir = tempfile::tempdir().unwrap();
    let units = generate_systemd_units(&config, &options);
    units.write_to_dir(temp_dir.path()).unwrap();

    assert!(temp_dir.path().join("alhalte-maitre-d.service").exists());
    assert!(temp_dir.path().join("alhalte-cellar.service").exists());
    assert!(temp_dir.path().join("alhalte-argenterie.service").exists());
    assert!(temp_dir.path().join("alhalte-bistrot.target").exists());

    let read_target =
        std::fs::read_to_string(temp_dir.path().join("alhalte-bistrot.target")).unwrap();
    assert_eq!(read_target, target);
}

// spec: alhalte-bistrot/cli
#[test]
fn test_cli_subcommands_end_to_end() {
    let bin_path = env!("CARGO_BIN_EXE_alhalte-bistrot");
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("Bistrot.toml");

    let plain_toml = r#"
[restaurant]
domain = "cli.example.com"
host = "127.0.0.1"

[maitre-d]
client-id = "cli-client"
client-secret = "cli-secret-oauth"
salt = "cli-salt"

[cellar]
anti-oracle = true

[banquette]
chunk-size-exponent = 8
"#;
    std::fs::write(&config_path, plain_toml).unwrap();

    // 1. Test validate command
    let output = std::process::Command::new(bin_path)
        .arg("-c")
        .arg(&config_path)
        .arg("validate")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Configuration in"));
    assert!(stdout.contains("cli.example.com"));

    // 2. Test args command for maitre-d
    let output = std::process::Command::new(bin_path)
        .arg("-c")
        .arg(&config_path)
        .arg("args")
        .arg("maitre-d")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--client-id cli-client"));
    assert!(stdout.contains("--client-secret cli-secret-oauth"));
    assert!(stdout.contains("--chunk-size-exponent 8"));

    // 3. Test args command for all
    let output = std::process::Command::new(bin_path)
        .arg("-c")
        .arg(&config_path)
        .arg("args")
        .arg("all")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[alhalte-maitre-d]"));
    assert!(stdout.contains("[alhalte-cellar]"));
    // banquette is spawned per room by maitre-d, not by a static bistrot command.
    assert!(!stdout.contains("[alhalte-banquette]"));
    assert!(stdout.contains("--anti-oracle"));
    assert!(stdout.contains("--maitre-d-url http://127.0.0.1:8080"));
    assert!(!stdout.contains("--allow-all-sessions"));

    // 4. Test systemd command with output-dir
    let units_dir = temp_dir.path().join("systemd_units");
    let output = std::process::Command::new(bin_path)
        .arg("-c")
        .arg(&config_path)
        .arg("systemd")
        .arg("--output-dir")
        .arg(&units_dir)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(units_dir.join("alhalte-maitre-d.service").exists());
    assert!(units_dir.join("alhalte-cellar.service").exists());
    assert!(units_dir.join("alhalte-argenterie.service").exists());
    assert!(units_dir.join("alhalte-bistrot.target").exists());

    // 5. Test url command
    let output = std::process::Command::new(bin_path)
        .arg("-c")
        .arg(&config_path)
        .arg("url")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "http://127.0.0.1:5173/");

    // 6. Test encrypt command and then validate with --age-key
    let identity = Identity::generate();
    let recipient = identity.to_public().to_string();
    let secret = identity.to_string();

    let enc_path = temp_dir.path().join("EncBistrot.toml");
    let output = std::process::Command::new(bin_path)
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

    let output = std::process::Command::new(bin_path)
        .arg("-c")
        .arg(&enc_path)
        .arg("--age-key")
        .arg(secret.expose_secret())
        .arg("validate")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cli.example.com"));
    assert!(stdout.contains("argenterie port: 5173"));
}

// spec: alhalte-bistrot/argenterie
#[test]
fn test_argenterie_config_and_url() {
    // 1. Defaults when omitted
    let toml_str = r#"
[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]
"#;
    let config = parse_bistrot_toml(toml_str).unwrap();
    assert_eq!(config.argenterie.port, 5173);
    assert_eq!(config.argenterie_url(), "http://127.0.0.1:5173/");

    // 2. Explicit port and host 0.0.0.0
    let toml_str_custom = r#"
[restaurant]
host = "0.0.0.0"

[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]

[argenterie]
port = 3000
"#;
    let config_custom = parse_bistrot_toml(toml_str_custom).unwrap();
    assert_eq!(config_custom.argenterie.port, 3000);
    assert_eq!(config_custom.argenterie_url(), "http://127.0.0.1:3000/");

    // 3. Port 0 fails validation
    let toml_invalid_port = r#"
[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]

[argenterie]
port = 0
"#;
    assert!(parse_bistrot_toml(toml_invalid_port).is_err());
}

// spec: alhalte-bistrot/extensions
#[test]
fn test_extensions_config_parsing_and_defaults() {
    let toml_str = r#"
[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]

[[extensions]]
id = "alhalte-dsl"
name = "Alhalte DSL"
path = "target/wasm32-unknown-unknown/release/alhalte_dsl.wasm"
enabled = true

[[extensions]]
id = "custom-ext"
path = "custom.wasm"
"#;

    let config = parse_bistrot_toml(toml_str).unwrap();
    assert_eq!(config.extensions.len(), 2);
    assert_eq!(config.extensions[0].id, "alhalte-dsl");
    assert_eq!(config.extensions[0].name.as_deref(), Some("Alhalte DSL"));
    assert_eq!(
        config.extensions[0].path,
        PathBuf::from("target/wasm32-unknown-unknown/release/alhalte_dsl.wasm")
    );
    assert!(config.extensions[0].enabled);

    assert_eq!(config.extensions[1].id, "custom-ext");
    assert_eq!(config.extensions[1].name, None);
    assert_eq!(config.extensions[1].path, PathBuf::from("custom.wasm"));
    assert!(config.extensions[1].enabled); // defaults to true
}

// spec: alhalte-bistrot/extensions
#[test]
fn test_extensions_validation_empty_id() {
    let toml_str = r#"
[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]

[[extensions]]
id = "   "
path = "dummy.wasm"
"#;
    assert!(parse_bistrot_toml(toml_str).is_err());
}

// spec: alhalte-bistrot/stage-extensions
#[test]
fn test_stage_extensions_files_and_manifest() {
    let temp_dir = tempfile::tempdir().unwrap();
    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let out_dir = temp_dir.path().join("staged");

    let wasm1_path = src_dir.join("ext_1.wasm");
    std::fs::write(&wasm1_path, b"\0asm\x01\0\0\0dummy1").unwrap();

    let wasm2_path = src_dir.join("ext_2.wasm");
    std::fs::write(&wasm2_path, b"\0asm\x01\0\0\0dummy2").unwrap();

    let missing_wasm_path = src_dir.join("missing.wasm");

    let toml_str = format!(
        r#"
[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]

[[extensions]]
id = "first-ext"
name = "First Extension"
path = "{}"
enabled = true

[[extensions]]
id = "second-ext"
path = "{}"
enabled = false

[[extensions]]
id = "missing-ext"
name = "Missing Ext"
path = "{}"
enabled = true
"#,
        wasm1_path.display(),
        wasm2_path.display(),
        missing_wasm_path.display(),
    );

    let config = parse_bistrot_toml(&toml_str).unwrap();
    let manifest_entries = config.stage_extensions(&out_dir).unwrap();

    assert_eq!(manifest_entries.len(), 3);

    // Check entry 0
    assert_eq!(manifest_entries[0].id, "first-ext");
    assert_eq!(manifest_entries[0].name, "First Extension");
    assert_eq!(manifest_entries[0].url, "/extensions/first-ext.wasm");
    assert!(manifest_entries[0].enabled);
    assert_eq!(manifest_entries[0].priority, 0);

    // Check entry 1
    assert_eq!(manifest_entries[1].id, "second-ext");
    assert_eq!(manifest_entries[1].name, "second-ext"); // Fallback to id
    assert_eq!(manifest_entries[1].url, "/extensions/second-ext.wasm");
    assert!(!manifest_entries[1].enabled);
    assert_eq!(manifest_entries[1].priority, 1);

    // Check entry 2
    assert_eq!(manifest_entries[2].id, "missing-ext");
    assert_eq!(manifest_entries[2].name, "Missing Ext");
    assert_eq!(manifest_entries[2].url, "/extensions/missing-ext.wasm");
    assert!(manifest_entries[2].enabled);
    assert_eq!(manifest_entries[2].priority, 2);

    // Verify files copied
    assert_eq!(
        std::fs::read(out_dir.join("first-ext.wasm")).unwrap(),
        b"\0asm\x01\0\0\0dummy1"
    );
    assert_eq!(
        std::fs::read(out_dir.join("second-ext.wasm")).unwrap(),
        b"\0asm\x01\0\0\0dummy2"
    );
    assert!(!out_dir.join("missing-ext.wasm").exists());

    // Verify manifest.json on disk
    let manifest_content = std::fs::read_to_string(out_dir.join("manifest.json")).unwrap();
    let parsed_manifest: Vec<ExtensionManifestEntry> =
        serde_json::from_str(&manifest_content).unwrap();
    assert_eq!(parsed_manifest, manifest_entries);
}

// spec: alhalte-bistrot/extensions
#[test]
fn test_root_bistrot_toml_extensions_configuration() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_bistrot = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("Bistrot.toml");
    let content = std::fs::read_to_string(&root_bistrot).unwrap();
    let config = parse_bistrot_toml(&content).unwrap();

    assert_eq!(config.extensions.len(), 6);

    let expected = [
        (
            "alhalte-dsl",
            Some("Alhalte DSL"),
            "target/wasm32-unknown-unknown/release/alhalte_dsl.wasm",
            true,
        ),
        (
            "markdown",
            Some("Markdown スタイル"),
            "target/wasm32-unknown-unknown/release/markdown.wasm",
            true,
        ),
        (
            "aozora",
            Some("青空文庫ルビ・傍点"),
            "target/wasm32-unknown-unknown/release/aozora.wasm",
            true,
        ),
        (
            "coc-6",
            Some("クトゥルフ神話TRPG (第6版)"),
            "target/wasm32-unknown-unknown/release/coc_6.wasm",
            false,
        ),
        (
            "coc-7",
            Some("新クトゥルフ神話TRPG (第7版)"),
            "target/wasm32-unknown-unknown/release/coc_7.wasm",
            false,
        ),
        (
            "emoklore",
            Some("エモクロアTRPG"),
            "target/wasm32-unknown-unknown/release/emoklore.wasm",
            false,
        ),
    ];

    for (i, (expected_id, expected_name, expected_path, expected_enabled)) in
        expected.iter().enumerate()
    {
        assert_eq!(&config.extensions[i].id, expected_id);
        assert_eq!(config.extensions[i].name.as_deref(), *expected_name);
        assert_eq!(config.extensions[i].path, PathBuf::from(*expected_path));
        assert_eq!(config.extensions[i].enabled, *expected_enabled);
    }
}
