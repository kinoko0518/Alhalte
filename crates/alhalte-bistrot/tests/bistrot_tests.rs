//! Integration and specification conformance tests for `alhalte-bistrot`.

use age::secrecy::ExposeSecret;
use age::x25519::Identity;
use alhalte_bistrot::args::{
    generate_all_args, generate_argenterie_args, generate_cellar_args, generate_maitre_d_args,
    generate_module_args,
};
use alhalte_bistrot::config::{
    ArgenterieConfig, BanquetteConfig, BistrotConfig, CellarConfig, ExtensionManifestEntry,
    MaitreDConfig, RestaurantConfig,
};
use alhalte_bistrot::rops::{
    ParseOptions, decrypt_rops_toml_with_key, encrypt_age_armored_value, encrypt_bistrot_toml,
    is_rops_encrypted, load_from_path_with_options, parse_bistrot_toml,
    parse_bistrot_toml_with_options,
};
use alhalte_bistrot::secrets::{secret_files, write_secret_files};
use alhalte_bistrot::systemd::{
    SystemdOptions, generate_argenterie_service, generate_bistrot_target, generate_cellar_service,
    generate_maitre_d_service, generate_systemd_units,
};
use std::path::PathBuf;

/// Options that allow plaintext secrets (development only).
///
/// The bundled local development configuration and most parser tests use
/// plaintext secrets. Production-style tests call [`parse_bistrot_toml`] /
/// [`parse_bistrot_toml_with_options`] directly to assert the fail-closed path.
fn dev_options(age_key: Option<&str>) -> ParseOptions<'_> {
    ParseOptions {
        age_key,
        allow_plaintext_secrets: true,
    }
}

fn dev_parse(content: &str) -> Result<BistrotConfig, alhalte_bistrot::BistrotError> {
    parse_bistrot_toml_with_options(content, &dev_options(None))
}

fn dev_load(path: &std::path::Path) -> Result<BistrotConfig, alhalte_bistrot::BistrotError> {
    load_from_path_with_options(path, &dev_options(None))
}

fn dev_load_with_key(
    path: &std::path::Path,
    age_key: Option<&str>,
) -> Result<BistrotConfig, alhalte_bistrot::BistrotError> {
    load_from_path_with_options(path, &dev_options(age_key))
}

/// Returns the value following `flag` in a generated argument list.
fn arg_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

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

    let config = dev_parse(toml_str).unwrap();

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

    let config = dev_parse(toml_str).unwrap();

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

    let config = dev_parse(toml_str).unwrap();
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
    assert!(dev_parse(empty_client_id).is_err());

    let empty_secret = r#"
[maitre-d]
client-id = "id"
client-secret = ""
salt = "salt"
[cellar]
"#;
    assert!(dev_parse(empty_secret).is_err());

    let empty_salt = r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = ""
[cellar]
"#;
    assert!(dev_parse(empty_salt).is_err());

    let invalid_exp = r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = "salt"
[cellar]
[banquette]
chunk-size-exponent = 0
"#;
    assert!(dev_parse(invalid_exp).is_err());
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
    let config = parse_bistrot_toml_with_options(
        &encrypted_toml,
        &ParseOptions {
            age_key: Some(secret.expose_secret()),
            allow_plaintext_secrets: false,
        },
    )
    .unwrap();
    assert_eq!(config.restaurant.domain, "vtt.secure.example");
    assert_eq!(config.maitre_d.client_secret, "super-secret-token-xyz");
    assert_eq!(config.maitre_d.salt, "random-salt-12345678");
    assert!(config.cellar.anti_oracle);

    // Verifying decryption failure with invalid key
    let bad_identity = Identity::generate();
    let bad_secret = bad_identity.to_string();
    let err = parse_bistrot_toml_with_options(
        &encrypted_toml,
        &ParseOptions {
            age_key: Some(bad_secret.expose_secret()),
            allow_plaintext_secrets: false,
        },
    )
    .unwrap_err();
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

    let config = dev_load(&config_path).unwrap();
    assert_eq!(config.maitre_d.client_id, "id");
    assert_eq!(config.maitre_d.client_secret, "secret");

    // Test load_from_path_with_key on encrypted file
    let identity = Identity::generate();
    let recipient = identity.to_public().to_string();
    let secret = identity.to_string();

    let enc_path = temp_dir.path().join("EncBistrot.toml");
    let encrypted = encrypt_bistrot_toml(plain_toml, &recipient).unwrap();
    std::fs::write(&enc_path, encrypted).unwrap();

    let config_dec = dev_load_with_key(&enc_path, Some(secret.expose_secret())).unwrap();
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
            public_url: None,
            debug_login: false,
        },
        cellar: CellarConfig {
            port: 8081,
            storage_path: None,
            db_path: None,
            anti_oracle: false,
            maitre_d_url: None,
            maitre_d_socket: None,
            allow_all_sessions: false,
            service_token: None,
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
            "--salt",
            "instancesalt",
            "--public-url",
            "https://vtt.example.com",
            "--socket-dir",
            "/run/sockets",
            "--cellar-url",
            "http://127.0.0.1:8081",
            "--chunk-size-exponent",
            "8",
            // spec: alhalte-bistrot/secrets 0003 the client secret is passed as a
            // file path, never as an argument value.
            "--client-secret-file",
            "/data/secrets/maitre-d-client-secret",
            "--cors-origin",
            "http://127.0.0.1:5173",
            "--cors-origin",
            "http://vtt.example.com:5173",
            "--cors-origin",
            "https://vtt.example.com:5173",
            "--cors-origin",
            "http://vtt.example.com",
            "--cors-origin",
            "https://vtt.example.com",
        ]
    );
    assert!(
        !args.iter().any(|arg| arg == "google-secret"),
        "the OAuth client secret must never be passed on the command line"
    );
    assert_eq!(
        secret_files(&config).maitre_d_client_secret,
        Some(PathBuf::from("/data/secrets/maitre-d-client-secret"))
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

// spec: alhalte-maitre-d/oauth-public-url
#[test]
fn test_maitre_d_public_url_derivation() {
    // spec: alhalte-bistrot/oauth-public-url 0001 an explicit public-url wins
    let mut config = BistrotConfig {
        restaurant: RestaurantConfig {
            domain: "vtt.example.com".to_string(),
            host: "127.0.0.1".to_string(),
            data_dir: PathBuf::from("/data"),
            socket_dir: PathBuf::from("/run/sockets"),
        },
        maitre_d: MaitreDConfig {
            port: 8080,
            db_path: None,
            client_id: "id".to_string(),
            client_secret: "secret".to_string(),
            salt: "salt".to_string(),
            socket_dir: None,
            banquette_bin: None,
            cellar_url: None,
            public_url: Some("https://auth.example.com/".to_string()),
            debug_login: false,
        },
        cellar: CellarConfig {
            port: 8081,
            storage_path: None,
            db_path: None,
            anti_oracle: false,
            maitre_d_url: None,
            maitre_d_socket: None,
            allow_all_sessions: false,
            service_token: None,
        },
        banquette: BanquetteConfig::default(),
        argenterie: ArgenterieConfig::default(),
        extensions: vec![],
    };

    assert_eq!(config.maitre_d_public_url(), "https://auth.example.com");
    let args = generate_maitre_d_args(&config);
    let index = args
        .iter()
        .position(|arg| arg == "--public-url")
        .expect("maitre-d must receive --public-url");
    assert_eq!(args[index + 1], "https://auth.example.com");

    // spec: alhalte-bistrot/oauth-public-url 0002 a real domain uses https
    config.maitre_d.public_url = None;
    assert_eq!(config.maitre_d_public_url(), "https://vtt.example.com");

    // spec: alhalte-bistrot/oauth-public-url 0003 loopback development uses the
    // argenterie origin so the Vite /auth proxy receives the callback.
    config.restaurant.domain = "localhost".to_string();
    assert_eq!(config.maitre_d_public_url(), "http://127.0.0.1:5173");
    config.restaurant.domain = "[::1]".to_string();
    assert_eq!(config.maitre_d_public_url(), "http://127.0.0.1:5173");

    // An explicitly empty value is rejected by validation.
    config.maitre_d.public_url = Some("  ".to_string());
    assert!(config.validate().is_err());
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
            public_url: None,
            debug_login: false,
        },
        cellar: CellarConfig {
            port: 8081,
            storage_path: Some(PathBuf::from("/custom/tin")),
            db_path: Some(PathBuf::from("/custom/cellar.db")),
            anti_oracle: false,
            maitre_d_url: None,
            maitre_d_socket: None,
            allow_all_sessions: false,
            service_token: None,
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
    let config = dev_parse(toml_str).unwrap();
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
    let config = dev_parse(toml_str).unwrap();
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
    let config = dev_parse(toml_str).unwrap();
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
    let config = dev_parse(toml_str).unwrap();
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
            public_url: None,
            debug_login: false,
        },
        cellar: CellarConfig {
            port: 8081,
            storage_path: None,
            db_path: None,
            anti_oracle: false,
            maitre_d_url: None,
            maitre_d_socket: None,
            allow_all_sessions: false,
            service_token: None,
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
    // Issue #229: the runner's fallback uses `args argenterie` so the bind host
    // is the configured one (loopback unless `[argenterie] lan`).
    assert_eq!(
        generate_module_args(&config, "argenterie").unwrap(),
        generate_argenterie_args(&config)
    );
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
            public_url: None,
            debug_login: false,
        },
        cellar: CellarConfig {
            port: 8081,
            storage_path: None,
            db_path: None,
            anti_oracle: true,
            maitre_d_url: None,
            maitre_d_socket: None,
            allow_all_sessions: false,
            service_token: None,
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
    // The bundled local development configuration keeps plaintext secrets, so
    // the CLI runs with the documented development escape hatch.
    let bistrot_cmd = || {
        let mut cmd = std::process::Command::new(bin_path);
        cmd.env("ALHALTE_ALLOW_PLAINTEXT_SECRETS", "1");
        cmd
    };
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("Bistrot.toml");
    let data_dir = temp_dir.path().join("data");

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

[banquette]
chunk-size-exponent = 8
"#,
        data_dir.display()
    );
    std::fs::write(&config_path, plain_toml).unwrap();

    // 1. Test validate command
    let output = bistrot_cmd()
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
    let output = bistrot_cmd()
        .arg("-c")
        .arg(&config_path)
        .arg("args")
        .arg("maitre-d")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--client-id cli-client"));
    assert!(stdout.contains("--client-secret-file"));
    assert!(!stdout.contains("--client-secret "));
    assert!(!stdout.contains("cli-secret-oauth"));
    assert!(stdout.contains("--chunk-size-exponent 8"));
    assert!(stdout.contains("--public-url https://cli.example.com"));

    // spec: alhalte-bistrot/secrets 0004 the client secret is materialized as a
    // 0600 file that the argument list points at.
    let client_secret_path = data_dir.join("secrets/maitre-d-client-secret");
    assert_eq!(
        std::fs::read_to_string(&client_secret_path).unwrap().trim(),
        "cli-secret-oauth"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&client_secret_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "secret files must be owner-only");
    }

    // 3. Test args command for all
    let output = bistrot_cmd()
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
    let output = bistrot_cmd()
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
    let output = bistrot_cmd()
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
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cli.example.com"));
    assert!(stdout.contains("argenterie port: 5173"));

    // 7. Test encrypt-value: stdin plaintext -> armored age value on stdout.
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
        let stdin = child.stdin.as_mut().unwrap();
        stdin.write_all(b"value-secret\n").unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let armored = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(armored.starts_with(alhalte_bistrot::AGE_ARMOR_BEGIN));
    assert!(armored.contains(alhalte_bistrot::AGE_ARMOR_END));
    assert!(!armored.contains("value-secret"));

    // The armored value round-trips into a config that no longer has a plaintext
    // secret, so no development escape hatch is needed to parse it.
    let armored_config_path = temp_dir.path().join("ArmoredBistrot.toml");
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
        "decrypting an armored value must not require the plaintext escape hatch: {}",
        String::from_utf8_lossy(&output.stderr)
    );
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
    let config = dev_parse(toml_str).unwrap();
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
    let config_custom = dev_parse(toml_str_custom).unwrap();
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
    assert!(dev_parse(toml_invalid_port).is_err());
}

// spec: alhalte-bistrot/lan 0001 LAN exposure is opt-in; defaults stay loopback
#[test]
fn test_argenterie_lan_defaults_to_loopback() {
    let config = dev_parse(
        r#"
[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]
"#,
    )
    .unwrap();

    assert!(!config.argenterie.lan, "LAN must be opt-in");
    assert!(!ArgenterieConfig::default().lan);
    assert_eq!(config.argenterie_bind_host(), "127.0.0.1");
    assert!(config.lan_origin_ports().is_empty());

    // The local URL and the OAuth public URL are unchanged.
    assert_eq!(config.argenterie_url(), "http://127.0.0.1:5173/");
    assert_eq!(config.maitre_d_public_url(), "http://127.0.0.1:5173");

    // No LAN origin switch reaches maitre-d, and the CORS list stays explicit.
    let args = generate_maitre_d_args(&config);
    assert!(!args.iter().any(|arg| arg == "--allow-lan-origin-port"));
    assert!(args.contains(&"http://127.0.0.1:5173".to_string()));
    assert!(
        !config
            .cors_origins()
            .iter()
            .any(|origin| origin.contains("*"))
    );

    // The Vite bind host is the configured restaurant host (loopback here).
    assert_eq!(
        generate_argenterie_args(&config),
        vec!["--host", "127.0.0.1", "--port", "5173"]
    );
    assert_eq!(
        generate_module_args(&config, "argenterie").unwrap(),
        generate_argenterie_args(&config)
    );
}

// spec: alhalte-bistrot/lan 0002 the dev switch binds Vite on 0.0.0.0 and
// scopes the maitre-d private-origin allowance to the argenterie port
#[test]
fn test_argenterie_lan_switch_enables_lan_bind_and_origin_port() {
    let config = dev_parse(
        r#"
[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]

[argenterie]
port = 5173
lan = true
"#,
    )
    .unwrap();

    assert!(config.argenterie.lan);
    assert_eq!(config.argenterie_bind_host(), "0.0.0.0");
    assert_eq!(config.lan_origin_ports(), vec![5173]);

    // The browser URL printed/opened locally stays loopback; the LAN URL is a
    // separate best-effort detection result.
    assert_eq!(config.argenterie_url(), "http://127.0.0.1:5173/");
    assert_eq!(config.maitre_d_public_url(), "http://127.0.0.1:5173");

    // maitre-d receives the dev-only private-network origin switch, not a
    // wildcard CORS origin.
    let args = generate_maitre_d_args(&config);
    assert_eq!(arg_after(&args, "--allow-lan-origin-port"), Some("5173"));
    assert_eq!(
        args.iter()
            .filter(|arg| *arg == "--allow-lan-origin-port")
            .count(),
        1
    );
    assert!(!args.iter().any(|arg| arg == "*" || arg.contains('*')));

    // Vite/runner arguments use the LAN bind host.
    assert_eq!(
        generate_argenterie_args(&config),
        vec!["--host", "0.0.0.0", "--port", "5173"]
    );

    // The systemd unit uses the same arguments (single source of truth).
    let options = SystemdOptions::default();
    let unit = generate_argenterie_service(&config, &options);
    assert!(unit.contains("ExecStart=pnpm run dev --host 0.0.0.0 --port 5173"));

    // A non-default port is propagated to both the bind and the origin rule.
    let mut custom = config.clone();
    custom.argenterie.port = 3000;
    assert_eq!(custom.lan_origin_ports(), vec![3000]);
    let custom_args = generate_maitre_d_args(&custom);
    assert_eq!(
        arg_after(&custom_args, "--allow-lan-origin-port"),
        Some("3000")
    );
}

// spec: alhalte-bistrot/lan 0003 the bundled dev config enables LAN, the
// production example keeps the loopback-safe default
#[test]
fn test_bundled_dev_config_enables_lan_but_example_stays_loopback() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent().unwrap().parent().unwrap();

    let dev_config = dev_load(&repo_root.join("Bistrot.toml"))
        .expect("the bundled local-development configuration must parse");
    assert!(
        dev_config.argenterie.lan,
        "nix run .#bistrot must expose the UI to the LAN without editing files"
    );
    assert_eq!(dev_config.argenterie_bind_host(), "0.0.0.0");

    let example = std::fs::read_to_string(repo_root.join("Bistrot.example.toml"))
        .expect("Bistrot.example.toml must be checked in");
    assert!(
        example.contains("# lan = false"),
        "the production template must document the loopback-safe default"
    );
    assert!(
        !example
            .lines()
            .any(|line| line.trim_start().starts_with("lan = true")),
        "the production template must not enable LAN exposure"
    );
}

// spec: alhalte-bistrot/lan 0004 LAN URLs keep the argenterie port
#[test]
fn test_lan_url_formatting_keeps_the_argenterie_port() {
    assert_eq!(
        alhalte_bistrot::lan_url(std::net::Ipv4Addr::new(192, 168, 1, 42), 5173),
        "http://192.168.1.42:5173/"
    );
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

    let config = dev_parse(toml_str).unwrap();
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
    assert!(dev_parse(toml_str).is_err());
}

// spec: alhalte-bistrot/stage-extensions 0001 every configured wasm is copied and listed in manifest.json
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
"#,
        wasm1_path.display(),
        wasm2_path.display(),
    );

    let config = dev_parse(&toml_str).unwrap();
    let manifest_entries = config.stage_extensions(&out_dir).unwrap();

    assert_eq!(manifest_entries.len(), 2);

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

    // Verify files copied
    assert_eq!(
        std::fs::read(out_dir.join("first-ext.wasm")).unwrap(),
        b"\0asm\x01\0\0\0dummy1"
    );
    assert_eq!(
        std::fs::read(out_dir.join("second-ext.wasm")).unwrap(),
        b"\0asm\x01\0\0\0dummy2"
    );

    // Verify manifest.json on disk
    let manifest_content = std::fs::read_to_string(out_dir.join("manifest.json")).unwrap();
    let parsed_manifest: Vec<ExtensionManifestEntry> =
        serde_json::from_str(&manifest_content).unwrap();
    assert_eq!(parsed_manifest, manifest_entries);
}

// spec: alhalte-bistrot/stage-extensions 0002 a missing wasm fails staging without partial output
#[test]
fn test_stage_extensions_missing_wasm_is_an_error_without_partial_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let out_dir = temp_dir.path().join("staged");

    let wasm_path = src_dir.join("present.wasm");
    std::fs::write(&wasm_path, b"\0asm\x01\0\0\0present").unwrap();
    let missing_wasm_path = src_dir.join("missing.wasm");

    let toml_str = format!(
        r#"
[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]

[[extensions]]
id = "present-ext"
path = "{}"
enabled = true

[[extensions]]
id = "missing-ext"
name = "Missing Ext"
path = "{}"
enabled = true
"#,
        wasm_path.display(),
        missing_wasm_path.display(),
    );

    let config = dev_parse(&toml_str).unwrap();
    let err = config.stage_extensions(&out_dir).unwrap_err();

    match err {
        alhalte_bistrot::BistrotError::ExtensionWasmMissing { id, path } => {
            assert_eq!(id, "missing-ext");
            assert_eq!(path, missing_wasm_path);
        }
        other => panic!("expected ExtensionWasmMissing, got {other:?}"),
    }

    // Nothing is staged and no manifest entry can point at the missing wasm.
    assert!(!out_dir.join("missing-ext.wasm").exists());
    assert!(!out_dir.join("present-ext.wasm").exists());
    assert!(!out_dir.join("manifest.json").exists());
}

// spec: alhalte-bistrot/stage-extensions 0003 relative paths resolve against base_dir and must exist
#[test]
fn test_stage_extensions_with_base_resolves_relative_paths() {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_dir = temp_dir.path().join("config");
    let wasm_dir = base_dir.join("wasm");
    std::fs::create_dir_all(&wasm_dir).unwrap();
    std::fs::write(wasm_dir.join("rel.wasm"), b"\0asm\x01\0\0\0rel").unwrap();
    let out_dir = temp_dir.path().join("staged");

    let relative_toml = r#"
[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]

[[extensions]]
id = "rel-ext"
path = "wasm/rel.wasm"
"#;
    let config = dev_parse(relative_toml).unwrap();
    let entries = config
        .stage_extensions_with_base(&out_dir, Some(&base_dir))
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].url, "/extensions/rel-ext.wasm");
    assert!(out_dir.join("rel-ext.wasm").exists());

    // A relative path that does not resolve inside base_dir is a configuration
    // error, reported with the path that was actually checked.
    let missing_toml = r#"
[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]

[[extensions]]
id = "gone-ext"
path = "wasm/gone.wasm"
"#;
    let missing_config = dev_parse(missing_toml).unwrap();
    let missing_out = temp_dir.path().join("staged-missing");
    let err = missing_config
        .stage_extensions_with_base(&missing_out, Some(&base_dir))
        .unwrap_err();
    match err {
        alhalte_bistrot::BistrotError::ExtensionWasmMissing { id, path } => {
            assert_eq!(id, "gone-ext");
            assert_eq!(path, base_dir.join("wasm/gone.wasm"));
        }
        other => panic!("expected ExtensionWasmMissing, got {other:?}"),
    }
    assert!(!missing_out.join("manifest.json").exists());
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
    let config = dev_parse(&content).unwrap();

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

// spec: alhalte-bistrot/service-token 0001 internal cellar service token is passed to maitre-d and cellar
// spec: alhalte-bistrot/secrets 0003 the token is passed as a file path, not as a value
#[test]
fn test_generate_args_includes_service_token() {
    let toml_str = r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = "salt"

[cellar]
service-token = "internal-service-token"
"#;
    let config = dev_parse(toml_str).unwrap();
    assert_eq!(
        config.cellar.service_token.as_deref(),
        Some("internal-service-token")
    );

    let token_path = config
        .secrets_dir()
        .join("cellar-service-token")
        .display()
        .to_string();

    let maitre_d_args = generate_maitre_d_args(&config);
    assert_eq!(
        arg_after(&maitre_d_args, "--service-token-file"),
        Some(token_path.as_str()),
        "maitre-d must receive --service-token-file"
    );
    let cellar_args = generate_cellar_args(&config);
    assert_eq!(
        arg_after(&cellar_args, "--service-token-file"),
        Some(token_path.as_str()),
        "cellar must receive --service-token-file"
    );

    // The value itself never appears in the argument list of either service.
    for args in [&maitre_d_args, &cellar_args] {
        assert!(
            !args.iter().any(|arg| arg == "internal-service-token"),
            "the service token must never be passed on the command line"
        );
        assert!(!args.iter().any(|arg| arg == "--service-token"));
    }

    // Without a token no flag is emitted.
    let plain = dev_parse(
        r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = "salt"

[cellar]
"#,
    )
    .unwrap();
    assert!(
        !generate_maitre_d_args(&plain)
            .iter()
            .any(|arg| arg == "--service-token-file")
    );
    assert!(
        !generate_cellar_args(&plain)
            .iter()
            .any(|arg| arg == "--service-token-file")
    );

    // An explicitly empty token is rejected.
    let empty = r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = "salt"

[cellar]
service-token = ""
"#;
    assert!(dev_parse(empty).is_err());
}

// spec: alhalte-bistrot/service-token 0002 the default Bistrot.toml shares the token
// spec: alhalte-bistrot/secrets 0006 the systemd units never contain the secret value
#[test]
fn test_root_bistrot_toml_shares_the_local_service_token() {
    // The checked-in local development configuration must set the token so
    // `nix run .#bistrot` persists room state out of the box.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root_bistrot = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("Bistrot.toml");
    let config = dev_load(&root_bistrot).unwrap();
    let token = config
        .cellar
        .service_token
        .as_deref()
        .expect("the default Bistrot.toml must configure cellar.service-token")
        .to_string();
    assert!(!token.trim().is_empty());

    let token_path = config
        .secrets_dir()
        .join("cellar-service-token")
        .display()
        .to_string();
    let maitre_d_args = generate_maitre_d_args(&config);
    let cellar_args = generate_cellar_args(&config);
    assert_eq!(
        arg_after(&maitre_d_args, "--service-token-file"),
        Some(token_path.as_str())
    );
    assert_eq!(
        arg_after(&cellar_args, "--service-token-file"),
        Some(token_path.as_str())
    );

    // The generated systemd units only reference the shared secret file.
    let units = generate_systemd_units(&config, &SystemdOptions::default());
    assert!(
        units
            .maitre_d_service
            .contains(&format!("--service-token-file {token_path}"))
    );
    assert!(
        units
            .cellar_service
            .contains(&format!("--service-token-file {token_path}"))
    );
    for unit in [&units.maitre_d_service, &units.cellar_service] {
        assert!(
            !unit.contains(&token),
            "the generated unit must not contain the service token value"
        );
    }
}

// spec: alhalte-bistrot/secrets 0001 a `-----BEGIN AGE ENCRYPTED FILE-----` value
// is decrypted recursively before deserialization
#[test]
fn test_per_value_age_armor_decryption() {
    let identity = Identity::generate();
    let recipient = identity.to_public().to_string();
    let secret_key = identity.to_string();

    let client_secret = encrypt_age_armored_value("oauth-client-secret", &recipient).unwrap();
    let service_token = encrypt_age_armored_value("internal-service-token", &recipient).unwrap();

    let toml_str = format!(
        r#"
[restaurant]
data-dir = "/data/alhalte"

[maitre-d]
client-id = "google-client"
client-secret = """
{client_secret}"""
salt = "instance-salt"

[cellar]
service-token = """
{service_token}"""

[[extensions]]
id = "alhalte-dsl"
path = "target/wasm32-unknown-unknown/release/alhalte_dsl.wasm"
"#
    );

    // The encrypted document does not contain the plaintext secrets.
    assert!(!toml_str.contains("oauth-client-secret"));
    assert!(!toml_str.contains("internal-service-token"));

    let options = ParseOptions {
        age_key: Some(secret_key.expose_secret()),
        allow_plaintext_secrets: false,
    };
    let config = parse_bistrot_toml_with_options(&toml_str, &options).unwrap();
    assert_eq!(config.maitre_d.client_id, "google-client");
    assert_eq!(config.maitre_d.client_secret, "oauth-client-secret");
    assert_eq!(
        config.cellar.service_token.as_deref(),
        Some("internal-service-token")
    );
    // Non-secret values survive the recursive walk untouched.
    assert_eq!(config.extensions.len(), 1);
    assert_eq!(config.extensions[0].id, "alhalte-dsl");

    // The decrypted secrets become 0600 files and the argument lists only carry
    // the file paths.
    let maitre_d_args = generate_maitre_d_args(&config);
    let cellar_args = generate_cellar_args(&config);
    assert_eq!(
        arg_after(&maitre_d_args, "--client-secret-file"),
        Some("/data/alhalte/secrets/maitre-d-client-secret")
    );
    assert_eq!(
        arg_after(&maitre_d_args, "--service-token-file"),
        Some("/data/alhalte/secrets/cellar-service-token")
    );
    assert_eq!(
        arg_after(&cellar_args, "--service-token-file"),
        Some("/data/alhalte/secrets/cellar-service-token")
    );
    for args in [&maitre_d_args, &cellar_args] {
        assert!(!args.iter().any(|arg| arg == "oauth-client-secret"));
        assert!(!args.iter().any(|arg| arg == "internal-service-token"));
    }

    let units = generate_systemd_units(&config, &SystemdOptions::default());
    for unit in [
        &units.maitre_d_service,
        &units.cellar_service,
        &units.argenterie_service,
        &units.bistrot_target,
    ] {
        assert!(!unit.contains("oauth-client-secret"));
        assert!(!unit.contains("internal-service-token"));
        assert!(!unit.contains(alhalte_bistrot::AGE_ARMOR_BEGIN));
    }
}

// spec: alhalte-bistrot/secrets 0002 missing identity, wrong identity, and
// corrupted armor all fail closed
#[test]
fn test_armored_value_failures() {
    let identity = Identity::generate();
    let recipient = identity.to_public().to_string();
    let armored = encrypt_age_armored_value("some-secret", &recipient).unwrap();

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

    // 1. No identity configured anywhere -> AgeKeyMissing.
    if std::env::var("ROPS_AGE").is_err() && std::env::var("ALHALTE_AGE_KEY").is_err() {
        let err = parse_bistrot_toml_with_options(&toml_str, &ParseOptions::default()).unwrap_err();
        assert!(
            matches!(err, alhalte_bistrot::BistrotError::AgeKeyMissing),
            "expected AgeKeyMissing, got {err:?}"
        );
    }

    // 2. A different identity cannot decrypt the value -> AgeDecrypt.
    let other = Identity::generate().to_string();
    let err = parse_bistrot_toml_with_options(
        &toml_str,
        &ParseOptions {
            age_key: Some(other.expose_secret()),
            allow_plaintext_secrets: false,
        },
    )
    .unwrap_err();
    assert!(
        matches!(err, alhalte_bistrot::BistrotError::AgeDecrypt(_)),
        "expected AgeDecrypt, got {err:?}"
    );

    // 3. A malformed armor block is rejected instead of being treated as a
    // plaintext secret.
    let broken = format!(
        "[maitre-d]\nclient-id = \"id\"\nclient-secret = \"\"\"\n{}\nbm90LWEtcmVhbC1jaXBoZXJ0ZXh0\n{}\n\"\"\"\nsalt = \"salt\"\n\n[cellar]\n",
        alhalte_bistrot::AGE_ARMOR_BEGIN,
        alhalte_bistrot::AGE_ARMOR_END
    );
    let key = identity.to_string();
    let err = parse_bistrot_toml_with_options(
        &broken,
        &ParseOptions {
            age_key: Some(key.expose_secret()),
            allow_plaintext_secrets: false,
        },
    )
    .unwrap_err();
    assert!(
        matches!(err, alhalte_bistrot::BistrotError::AgeDecrypt(_)),
        "expected AgeDecrypt for corrupted armor, got {err:?}"
    );

    // 4. An invalid identity string is reported as such.
    let err = parse_bistrot_toml_with_options(
        &toml_str,
        &ParseOptions {
            age_key: Some("not-an-age-secret-key"),
            allow_plaintext_secrets: false,
        },
    )
    .unwrap_err();
    assert!(
        matches!(err, alhalte_bistrot::BistrotError::AgeIdentity(_)),
        "expected AgeIdentity, got {err:?}"
    );
}

// spec: alhalte-bistrot/secrets 0005 plaintext secrets are fail-closed unless
// ALHALTE_ALLOW_PLAINTEXT_SECRETS=1
#[test]
fn test_plaintext_secrets_are_fail_closed() {
    let plaintext = r#"
[maitre-d]
client-id = "id"
client-secret = "plain-oauth-secret"
salt = "salt"

[cellar]
service-token = "plain-service-token"
"#;

    // Production default: both fields are reported.
    let err = parse_bistrot_toml_with_options(plaintext, &ParseOptions::default()).unwrap_err();
    match err {
        alhalte_bistrot::BistrotError::PlaintextSecret(fields) => {
            assert!(
                fields.contains("maitre-d.client-secret"),
                "fields: {fields}"
            );
            assert!(fields.contains("cellar.service-token"), "fields: {fields}");
        }
        other => panic!("expected PlaintextSecret, got {other:?}"),
    }

    // The `--config`/library entry point without options also fails closed for a
    // real file.
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("Bistrot.toml");
    std::fs::write(&path, plaintext).unwrap();
    let err = parse_bistrot_toml(&std::fs::read_to_string(&path).unwrap()).unwrap_err();
    assert!(matches!(
        err,
        alhalte_bistrot::BistrotError::PlaintextSecret(_)
    ));

    // Development escape hatch (explicit, never implicit).
    let config = parse_bistrot_toml_with_options(
        plaintext,
        &ParseOptions {
            age_key: None,
            allow_plaintext_secrets: true,
        },
    )
    .unwrap();
    assert_eq!(config.maitre_d.client_secret, "plain-oauth-secret");

    // An armored secret alongside a plaintext one still reports only the
    // plaintext field.
    let identity = Identity::generate();
    let armored =
        encrypt_age_armored_value("encrypted-secret", &identity.to_public().to_string()).unwrap();
    let key = identity.to_string();
    let mixed = format!(
        "[maitre-d]\nclient-id = \"id\"\nclient-secret = \"\"\"\n{armored}\"\"\"\nsalt = \"salt\"\n\n[cellar]\nservice-token = \"plain-service-token\"\n"
    );
    let err = parse_bistrot_toml_with_options(
        &mixed,
        &ParseOptions {
            age_key: Some(key.expose_secret()),
            allow_plaintext_secrets: false,
        },
    )
    .unwrap_err();
    match err {
        alhalte_bistrot::BistrotError::PlaintextSecret(fields) => {
            assert_eq!(fields, "cellar.service-token");
        }
        other => panic!("expected PlaintextSecret, got {other:?}"),
    }
}

// spec: alhalte-bistrot/secrets 0004 decrypted secrets are materialized with
// mode 0600 in a 0700 directory
#[test]
fn test_write_secret_files_permissions() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config = BistrotConfig {
        restaurant: RestaurantConfig {
            domain: "localhost".to_string(),
            host: "127.0.0.1".to_string(),
            data_dir: temp_dir.path().join("data"),
            socket_dir: temp_dir.path().join("sockets"),
        },
        maitre_d: MaitreDConfig {
            port: 8080,
            db_path: None,
            client_id: "id".to_string(),
            client_secret: "oauth-secret-value".to_string(),
            salt: "salt".to_string(),
            socket_dir: None,
            banquette_bin: None,
            cellar_url: None,
            public_url: None,
            debug_login: false,
        },
        cellar: CellarConfig {
            port: 8081,
            storage_path: None,
            db_path: None,
            anti_oracle: false,
            maitre_d_url: None,
            maitre_d_socket: None,
            allow_all_sessions: false,
            service_token: Some("service-token-value".to_string()),
        },
        banquette: BanquetteConfig::default(),
        argenterie: ArgenterieConfig::default(),
        extensions: vec![],
    };

    // The pure path computation does not touch the filesystem.
    let planned = secret_files(&config);
    assert!(!planned.dir.exists());

    let written = write_secret_files(&config).unwrap();
    assert_eq!(written, planned);

    let client_secret_path = written.maitre_d_client_secret.unwrap();
    let token_path = written.service_token.unwrap();
    assert_eq!(
        std::fs::read_to_string(&client_secret_path).unwrap().trim(),
        "oauth-secret-value"
    );
    assert_eq!(
        std::fs::read_to_string(&token_path).unwrap().trim(),
        "service-token-value"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode =
            |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&client_secret_path), 0o600);
        assert_eq!(mode(&token_path), 0o600);
        assert_eq!(mode(&written.dir), 0o700);
    }

    // Rewriting keeps the restrictive mode even though the file already exists.
    write_secret_files(&config).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&client_secret_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    // A configuration without secrets creates no directory at all.
    let mut bare = config.clone();
    bare.maitre_d.client_secret = String::new();
    bare.cellar.service_token = None;
    let bare_files = write_secret_files(&bare).unwrap();
    assert!(bare_files.maitre_d_client_secret.is_none());
    assert!(bare_files.service_token.is_none());
}

// spec: alhalte-bistrot/secrets 0007 identity resolution order is
// --age-key -> ROPS_AGE -> ALHALTE_AGE_KEY
#[test]
fn test_age_identity_resolution_precedence() {
    use alhalte_bistrot::secrets::resolve_age_identity_source;

    // An explicit key always wins and is trimmed.
    assert_eq!(
        resolve_age_identity_source(Some("  explicit-key  ")).as_deref(),
        Some("explicit-key")
    );
    // Blank values are treated as unset.
    assert_ne!(
        resolve_age_identity_source(Some("   ")).as_deref(),
        Some("   ")
    );

    // ALHALTE_AGE_KEY is the last-resort fallback.
    if std::env::var("ROPS_AGE").is_err() {
        alhalte_bistrot::ffi::with_scoped_env_var(
            "ALHALTE_AGE_KEY",
            "AGE-SECRET-KEY-FROM-ENV",
            || {
                assert_eq!(
                    resolve_age_identity_source(None).as_deref(),
                    Some("AGE-SECRET-KEY-FROM-ENV")
                );
            },
        );
    }
}

// spec: alhalte-bistrot/secrets 0008 the example configuration documents the
// armored format and contains no plaintext secret
#[test]
fn test_example_configuration_is_fail_closed() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let example_path = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("Bistrot.example.toml");
    let content =
        std::fs::read_to_string(&example_path).expect("Bistrot.example.toml must be checked in");

    // The template uses placeholders, never a real local development secret.
    assert!(!content.contains("alhalte-local-"));
    assert!(content.contains("alhalte-bistrot encrypt-value"));

    // Regardless of the identity, the unedited template cannot be used: without a
    // key the armored placeholder is reported as such instead of being treated as
    // a plaintext secret.
    if std::env::var("ROPS_AGE").is_err() && std::env::var("ALHALTE_AGE_KEY").is_err() {
        let err = parse_bistrot_toml_with_options(&content, &ParseOptions::default()).unwrap_err();
        assert!(
            matches!(err, alhalte_bistrot::BistrotError::AgeKeyMissing),
            "expected AgeKeyMissing for the template, got {err:?}"
        );
    }
}

// spec: alhalte-maitre-d/debug-login 0004 Bistrot passes --debug-login only when
// [maitre-d] debug-login is enabled
#[test]
fn test_generate_maitre_d_args_passes_debug_login() {
    // Off by default: no flag, and the setting is not in the TOML at all.
    let plain = dev_parse(
        r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = "salt"

[cellar]
"#,
    )
    .unwrap();
    assert!(!plain.maitre_d.debug_login);
    assert!(
        !generate_maitre_d_args(&plain)
            .iter()
            .any(|arg| arg == "--debug-login"),
        "the debug login must stay off unless it is configured"
    );

    // Enabled through the TOML key (kebab-case and snake_case alias).
    let enabled = dev_parse(
        r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = "salt"
debug-login = true

[cellar]
"#,
    )
    .unwrap();
    assert!(enabled.maitre_d.debug_login);
    assert!(
        generate_maitre_d_args(&enabled)
            .iter()
            .any(|arg| arg == "--debug-login")
    );

    let snake = dev_parse(
        r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = "salt"
debug_login = true

[cellar]
"#,
    )
    .unwrap();
    assert!(snake.maitre_d.debug_login);

    // The flag does not change any other argument.
    let mut with_flag = generate_maitre_d_args(&enabled);
    with_flag.retain(|arg| arg != "--debug-login");
    assert_eq!(with_flag, generate_maitre_d_args(&plain));
}
