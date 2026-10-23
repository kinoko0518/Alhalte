//! Specification conformance tests for `alhalte-restaurant`.

use age::secrecy::ExposeSecret;
use age::x25519::Identity;
use alhalte_restaurant::args::{
    MaitreDArgsOptions, generate_all_args, generate_cellar_args, generate_maitre_d_args,
};
use alhalte_restaurant::config::{
    BanquetteConfig, CellarConfig, ExtensionManifestEntry, MaitreDConfig, RestaurantConfig,
    RestaurantToml,
};
use alhalte_restaurant::rops::{
    ParseOptions, decrypt_rops_toml_with_key, encrypt_age_armored_value, encrypt_restaurant_toml,
    is_rops_encrypted, load_restaurant_from_path_with_options, parse_restaurant_toml,
    parse_restaurant_toml_with_options,
};
use alhalte_restaurant::secrets::{secret_files, write_secret_files};
use std::path::PathBuf;

/// Options that allow plaintext secrets (development only).
fn dev_options(age_key: Option<&str>) -> ParseOptions<'_> {
    ParseOptions {
        age_key,
        allow_plaintext_secrets: true,
    }
}

fn dev_parse(content: &str) -> Result<RestaurantToml, alhalte_restaurant::RestaurantError> {
    parse_restaurant_toml_with_options(content, &dev_options(None))
}

fn dev_load(path: &std::path::Path) -> Result<RestaurantToml, alhalte_restaurant::RestaurantError> {
    load_restaurant_from_path_with_options(path, &dev_options(None))
}

fn dev_load_with_key(
    path: &std::path::Path,
    age_key: Option<&str>,
) -> Result<RestaurantToml, alhalte_restaurant::RestaurantError> {
    load_restaurant_from_path_with_options(path, &dev_options(age_key))
}

/// Returns the value following `flag` in a generated argument list.
fn arg_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn sample_config() -> RestaurantToml {
    dev_parse(
        r#"
[restaurant]
domain = "vtt.example.com"
host = "127.0.0.1"
data-dir = "/data"
socket-dir = "/run/sockets"

[maitre-d]
port = 8080
client-id = "google-client"
client-secret = "google-secret"
salt = "instancesalt"

[cellar]
port = 8081
anti-oracle = true
service-token = "internal-service-token"
"#,
    )
    .unwrap()
}

// spec: alhalte-restaurant/config-parser
#[test]
fn parses_full_configuration_and_defaults() {
    let config = dev_parse(
        r#"
[restaurant]
domain = "vtt.alhalte.example"
host = "0.0.0.0"
data-dir = "/data/alhalte"
socket-dir = "/run/alhalte"

[maitre-d]
port = 9000
client-id = "google-client-12345"
client-secret = "super-secret-oauth"
salt = "instance-salt-67890"

[cellar]
port = 9001
anti-oracle = true

[banquette]
chunk-size-exponent = 10
heartbeat-interval-secs = 60
"#,
    )
    .unwrap();

    assert_eq!(config.restaurant.domain, "vtt.alhalte.example");
    assert_eq!(config.restaurant.host, "0.0.0.0");
    assert_eq!(config.restaurant.data_dir, PathBuf::from("/data/alhalte"));
    assert_eq!(config.restaurant.socket_dir, PathBuf::from("/run/alhalte"));
    assert_eq!(config.maitre_d.port, 9000);
    assert_eq!(
        config.maitre_d_db_path(),
        PathBuf::from("/data/alhalte/maitre-d.sqlite3")
    );
    assert_eq!(config.maitre_d_socket_dir(), PathBuf::from("/run/alhalte"));
    assert_eq!(config.cellar.port, 9001);
    assert_eq!(
        config.cellar_storage_path(),
        PathBuf::from("/data/alhalte/cellar/tin")
    );
    assert_eq!(
        config.cellar_db_path(),
        PathBuf::from("/data/alhalte/cellar/cellar.sqlite3")
    );
    assert!(config.cellar.anti_oracle);
    assert_eq!(config.banquette.chunk_size_exponent, 10);
    assert_eq!(config.banquette.heartbeat_interval_secs, 60);
}

// spec: alhalte-restaurant/config-parser
#[test]
fn parses_defaults_when_omitted() {
    let config = dev_parse(
        r#"
[maitre-d]
client-id = "test-client"
client-secret = "test-secret"
salt = "test-salt"

[cellar]
"#,
    )
    .unwrap();

    assert_eq!(config.restaurant.domain, "localhost");
    assert_eq!(config.restaurant.host, "127.0.0.1");
    assert_eq!(config.maitre_d.port, 8080);
    assert_eq!(config.cellar.port, 8081);
    assert_eq!(config.banquette.chunk_size_exponent, 8);
    assert_eq!(config.banquette.heartbeat_interval_secs, 240);
}

// spec: alhalte-restaurant/config-parser
#[test]
fn accepts_snake_case_aliases() {
    let config = dev_parse(
        r#"
[restaurant]
data_dir = "/srv/alhalte"
socket_dir = "/var/run/alhalte"

[maitre_d]
client_id = "client-snake"
client_secret = "secret-snake"
salt = "salt-snake"

[cellar]
anti_oracle = true
"#,
    )
    .unwrap();

    assert_eq!(config.restaurant.data_dir, PathBuf::from("/srv/alhalte"));
    assert_eq!(
        config.restaurant.socket_dir,
        PathBuf::from("/var/run/alhalte")
    );
    assert_eq!(config.maitre_d.client_id, "client-snake");
    assert!(config.cellar.anti_oracle);
}

// spec: alhalte-restaurant/config-parser removed fields are rejected
#[test]
fn rejects_removed_and_unknown_fields() {
    let base = r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = "salt"

[cellar]
"#;

    let argenterie = format!("{base}\n[argenterie]\nport = 5173\n");
    let error = dev_parse(&argenterie).unwrap_err();
    assert!(error.to_string().contains("unknown field"), "got {error}");

    let cellar_host = r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = "salt"

[cellar]
host = "127.0.0.1"
"#;
    let error = dev_parse(cellar_host).unwrap_err();
    assert!(error.to_string().contains("unknown field"), "got {error}");

    let extension_name = r#"
[maitre-d]
client-id = "id"
client-secret = "secret"
salt = "salt"

[cellar]

[[extensions]]
id = "alhalte-dsl"
name = "Alhalte DSL"
path = "alhalte_dsl.wasm"
"#;
    let error = dev_parse(extension_name).unwrap_err();
    assert!(error.to_string().contains("unknown field"), "got {error}");
}

// spec: alhalte-restaurant/config-parser
#[test]
fn validates_required_fields() {
    let local_login = r#"
[maitre-d]
salt = "salt"
local-login = true

[cellar]
"#;
    assert!(
        dev_parse(local_login).is_ok(),
        "local-login does not need OAuth credentials"
    );

    let google_without_id = r#"
[maitre-d]
client-secret = "secret"
salt = "salt"

[cellar]
"#;
    assert!(dev_parse(google_without_id).is_err());

    let empty_salt = r#"
[maitre-d]
salt = ""

[cellar]
"#;
    assert!(dev_parse(empty_salt).is_err());

    let invalid_exponent = r#"
[maitre-d]
salt = "salt"
local-login = true

[cellar]

[banquette]
chunk-size-exponent = 0
"#;
    assert!(dev_parse(invalid_exponent).is_err());

    let empty_token = r#"
[maitre-d]
salt = "salt"
local-login = true

[cellar]
service-token = ""
"#;
    assert!(dev_parse(empty_token).is_err());
}

// spec: alhalte-maitre-d/oauth-public-url
#[test]
fn derives_public_url_and_cors_origins() {
    let mut config = sample_config();
    config.maitre_d.public_url = Some("https://auth.example.com/".to_string());
    assert_eq!(config.maitre_d_public_url(), "https://auth.example.com");

    config.maitre_d.public_url = None;
    assert_eq!(config.maitre_d_public_url(), "https://vtt.example.com");

    // Loopback development uses the maitre-d origin (the SPA is same-origin).
    config.restaurant.domain = "localhost".to_string();
    config.restaurant.host = "127.0.0.1".to_string();
    assert_eq!(config.maitre_d_public_url(), "http://127.0.0.1:8080");
    assert_eq!(config.maitre_d_url(), "http://127.0.0.1:8080/");
    let origins = config.cors_origins();
    assert!(origins.contains(&"http://127.0.0.1:8080".to_string()));
    // The optional Vite dev server stays allowed on loopback instances.
    assert!(origins.contains(&"http://127.0.0.1:5173".to_string()));
    assert!(!origins.iter().any(|origin| origin.contains('*')));

    // A real domain also produces https origins.
    config.restaurant.domain = "vtt.example.com".to_string();
    let origins = config.cors_origins();
    assert!(origins.contains(&"https://vtt.example.com".to_string()));

    // An explicitly empty value is rejected by validation.
    config.maitre_d.public_url = Some("  ".to_string());
    assert!(config.validate().is_err());
}

// spec: alhalte-restaurant/lan 0001 LAN exposure is derived from restaurant.host
#[test]
fn derives_lan_exposure_from_the_bind_host() {
    let loopback = sample_config();
    assert!(!loopback.is_lan_exposed());
    assert!(loopback.lan_origin_ports().is_empty());

    let mut exposed = sample_config();
    exposed.restaurant.host = "0.0.0.0".to_string();
    assert!(exposed.is_lan_exposed());
    assert_eq!(exposed.lan_origin_ports(), vec![8080]);
    assert_eq!(exposed.maitre_d_url(), "http://127.0.0.1:8080/");

    let mut private_ip = sample_config();
    private_ip.restaurant.host = "192.168.1.10".to_string();
    assert!(private_ip.is_lan_exposed());
    assert_eq!(private_ip.lan_origin_ports(), vec![8080]);
}

// spec: alhalte-maitre-d/oauth-args
// spec: alhalte-restaurant/secrets 0003 secrets are passed as file paths
#[test]
fn generates_maitre_d_and_cellar_arguments() {
    let config = sample_config();
    let options = MaitreDArgsOptions {
        web_root: Some(PathBuf::from("/srv/alhalte/web")),
        banquette_bin: None,
    };
    let args = generate_maitre_d_args(&config, &options);

    assert_eq!(arg_after(&args, "--host"), Some("127.0.0.1"));
    assert_eq!(arg_after(&args, "--port"), Some("8080"));
    assert_eq!(
        arg_after(&args, "--db-path"),
        Some("/data/maitre-d.sqlite3")
    );
    assert_eq!(arg_after(&args, "--salt"), Some("instancesalt"));
    assert_eq!(
        arg_after(&args, "--public-url"),
        Some("https://vtt.example.com")
    );
    assert_eq!(arg_after(&args, "--socket-dir"), Some("/run/sockets"));
    assert_eq!(
        arg_after(&args, "--cellar-url"),
        Some("http://127.0.0.1:8081")
    );
    assert_eq!(arg_after(&args, "--chunk-size-exponent"), Some("8"));
    assert_eq!(
        arg_after(&args, "--banquette-heartbeat-interval-secs"),
        Some("240")
    );
    assert_eq!(arg_after(&args, "--client-id"), Some("google-client"));
    assert_eq!(
        arg_after(&args, "--client-secret-file"),
        Some("/data/secrets/maitre-d-client-secret")
    );
    assert_eq!(
        arg_after(&args, "--service-token-file"),
        Some("/data/secrets/cellar-service-token")
    );
    assert_eq!(arg_after(&args, "--web-root"), Some("/srv/alhalte/web"));
    assert!(!args.iter().any(|arg| arg == "google-secret"));
    assert!(!args.iter().any(|arg| arg == "internal-service-token"));
    assert!(!args.iter().any(|arg| arg == "--local-db-path"));

    let cellar = generate_cellar_args(&config);
    assert_eq!(arg_after(&cellar, "--host"), Some("127.0.0.1"));
    assert_eq!(arg_after(&cellar, "--port"), Some("8081"));
    assert_eq!(
        arg_after(&cellar, "--storage-path"),
        Some("/data/cellar/tin")
    );
    assert_eq!(
        arg_after(&cellar, "--db-path"),
        Some("/data/cellar/cellar.sqlite3")
    );
    assert_eq!(
        arg_after(&cellar, "--maitre-d-url"),
        Some("http://127.0.0.1:8080")
    );
    assert!(cellar.contains(&"--anti-oracle".to_string()));
    assert!(!cellar.contains(&"--allow-all-sessions".to_string()));
    assert!(!cellar.iter().any(|arg| arg == "--maitre-d-socket"));
}

#[test]
fn generate_all_args_matches_the_individual_generators() {
    let config = sample_config();
    let options = MaitreDArgsOptions::default();
    let all = generate_all_args(&config, &options);
    assert_eq!(all.maitre_d, generate_maitre_d_args(&config, &options));
    assert_eq!(all.cellar, generate_cellar_args(&config));
}

// spec: alhalte-maitre-d/local-login
#[test]
fn local_login_omits_google_credentials_and_local_db_override() {
    let config = dev_parse(
        r#"
[maitre-d]
salt = "salt"
local-login = true

[cellar]
"#,
    )
    .unwrap();
    let args = generate_maitre_d_args(&config, &MaitreDArgsOptions::default());
    assert!(args.contains(&"--local-login".to_string()));
    assert!(!args.contains(&"--client-id".to_string()));
    assert!(!args.contains(&"--client-secret-file".to_string()));
    assert!(!args.contains(&"--local-db-path".to_string()));
}

// spec: alhalte-restaurant/rops-decryption
#[test]
fn rops_roundtrip_and_load_from_path() {
    let identity = Identity::generate();
    let recipient = identity.to_public().to_string();
    let secret = identity.to_string();

    let plain_toml = r#"
[restaurant]
domain = "vtt.secure.example"

[maitre-d]
client-id = "secure-client-id"
client-secret = "super-secret-token-xyz"
salt = "random-salt-12345678"

[cellar]
anti-oracle = true
"#;

    let encrypted_toml = encrypt_restaurant_toml(plain_toml, &recipient).unwrap();
    assert!(is_rops_encrypted(&encrypted_toml));
    assert!(encrypted_toml.contains("sops"));
    assert!(!encrypted_toml.contains("super-secret-token-xyz"));

    let decrypted = decrypt_rops_toml_with_key(&encrypted_toml, secret.expose_secret()).unwrap();
    assert!(decrypted.contains("super-secret-token-xyz"));

    let config = parse_restaurant_toml_with_options(
        &encrypted_toml,
        &ParseOptions {
            age_key: Some(secret.expose_secret()),
            allow_plaintext_secrets: false,
        },
    )
    .unwrap();
    assert_eq!(config.restaurant.domain, "vtt.secure.example");
    assert_eq!(config.maitre_d.client_secret, "super-secret-token-xyz");

    let bad = Identity::generate().to_string();
    let err = parse_restaurant_toml_with_options(
        &encrypted_toml,
        &ParseOptions {
            age_key: Some(bad.expose_secret()),
            allow_plaintext_secrets: false,
        },
    )
    .unwrap_err();
    assert!(matches!(
        err,
        alhalte_restaurant::RestaurantError::RopsDecrypt(_)
    ));

    // File loading round-trips both plain and encrypted documents.
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("Restaurant.toml");
    std::fs::write(&config_path, plain_toml).unwrap();
    let loaded = dev_load(&config_path).unwrap();
    assert_eq!(loaded.maitre_d.client_id, "secure-client-id");

    let enc_path = temp_dir.path().join("EncRestaurant.toml");
    std::fs::write(&enc_path, encrypted_toml).unwrap();
    let loaded = dev_load_with_key(&enc_path, Some(secret.expose_secret())).unwrap();
    assert_eq!(loaded.maitre_d.client_id, "secure-client-id");
}

// spec: alhalte-restaurant/secrets 0001 per-value age armor
#[test]
fn decrypts_per_value_age_armor() {
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
    assert!(!toml_str.contains("oauth-client-secret"));
    assert!(!toml_str.contains("internal-service-token"));

    let config = parse_restaurant_toml_with_options(
        &toml_str,
        &ParseOptions {
            age_key: Some(secret_key.expose_secret()),
            allow_plaintext_secrets: false,
        },
    )
    .unwrap();
    assert_eq!(config.maitre_d.client_secret, "oauth-client-secret");
    assert_eq!(
        config.cellar.service_token.as_deref(),
        Some("internal-service-token")
    );

    let args = generate_maitre_d_args(&config, &MaitreDArgsOptions::default());
    assert_eq!(
        arg_after(&args, "--client-secret-file"),
        Some("/data/alhalte/secrets/maitre-d-client-secret")
    );
    assert!(!args.iter().any(|arg| arg == "oauth-client-secret"));
}

// spec: alhalte-restaurant/secrets 0002 missing/wrong identity fails closed
#[test]
fn armored_value_failures_are_fail_closed() {
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

    if std::env::var("ROPS_AGE").is_err() && std::env::var("ALHALTE_AGE_KEY").is_err() {
        let err =
            parse_restaurant_toml_with_options(&toml_str, &ParseOptions::default()).unwrap_err();
        assert!(matches!(
            err,
            alhalte_restaurant::RestaurantError::AgeKeyMissing
        ));
    }

    let other = Identity::generate().to_string();
    let err = parse_restaurant_toml_with_options(
        &toml_str,
        &ParseOptions {
            age_key: Some(other.expose_secret()),
            allow_plaintext_secrets: false,
        },
    )
    .unwrap_err();
    assert!(matches!(
        err,
        alhalte_restaurant::RestaurantError::AgeDecrypt(_)
    ));

    let err = parse_restaurant_toml_with_options(
        &toml_str,
        &ParseOptions {
            age_key: Some("not-an-age-secret-key"),
            allow_plaintext_secrets: false,
        },
    )
    .unwrap_err();
    assert!(matches!(
        err,
        alhalte_restaurant::RestaurantError::AgeIdentity(_)
    ));
}

// spec: alhalte-restaurant/secrets 0005 plaintext secrets are fail-closed
#[test]
fn plaintext_secrets_are_fail_closed() {
    let plaintext = r#"
[maitre-d]
client-id = "id"
client-secret = "plain-oauth-secret"
salt = "salt"

[cellar]
service-token = "plain-service-token"
"#;

    let err = parse_restaurant_toml_with_options(plaintext, &ParseOptions::default()).unwrap_err();
    match err {
        alhalte_restaurant::RestaurantError::PlaintextSecret(fields) => {
            assert!(
                fields.contains("maitre-d.client-secret"),
                "fields: {fields}"
            );
            assert!(fields.contains("cellar.service-token"), "fields: {fields}");
        }
        other => panic!("expected PlaintextSecret, got {other:?}"),
    }

    let config = parse_restaurant_toml_with_options(
        plaintext,
        &ParseOptions {
            age_key: None,
            allow_plaintext_secrets: true,
        },
    )
    .unwrap();
    assert_eq!(config.maitre_d.client_secret, "plain-oauth-secret");

    let err = parse_restaurant_toml(plaintext).unwrap_err();
    assert!(matches!(
        err,
        alhalte_restaurant::RestaurantError::PlaintextSecret(_)
    ));
}

// spec: alhalte-restaurant/secrets 0004 decrypted secrets are 0600 in a 0700 dir
#[test]
fn writes_secret_files_with_restrictive_permissions() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config = RestaurantToml {
        restaurant: RestaurantConfig {
            data_dir: temp_dir.path().join("data"),
            socket_dir: temp_dir.path().join("sockets"),
            ..RestaurantConfig::default()
        },
        maitre_d: MaitreDConfig {
            port: 8080,
            client_id: "id".to_string(),
            client_secret: "oauth-secret-value".to_string(),
            salt: "salt".to_string(),
            public_url: None,
            local_login: false,
        },
        cellar: CellarConfig {
            port: 8081,
            anti_oracle: false,
            service_token: Some("service-token-value".to_string()),
        },
        banquette: BanquetteConfig::default(),
        extensions: vec![],
    };

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
}

// spec: alhalte-restaurant/extensions
#[test]
fn parses_extensions_and_rejects_an_empty_id() {
    let config = dev_parse(
        r#"
[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]

[[extensions]]
id = "alhalte-dsl"
path = "alhalte_dsl.wasm"

[[extensions]]
id = "custom-ext"
path = "custom.wasm"
enabled = false
"#,
    )
    .unwrap();
    assert_eq!(config.extensions.len(), 2);
    assert!(config.extensions[0].enabled);
    assert!(!config.extensions[1].enabled);

    let empty_id = r#"
[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]

[[extensions]]
id = "   "
path = "dummy.wasm"
"#;
    assert!(dev_parse(empty_id).is_err());
}

// spec: alhalte-restaurant/stage-extensions
#[test]
fn stages_extensions_and_writes_the_manifest() {
    let temp_dir = tempfile::tempdir().unwrap();
    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let out_dir = temp_dir.path().join("staged");
    let wasm1 = src_dir.join("ext_1.wasm");
    let wasm2 = src_dir.join("ext_2.wasm");
    std::fs::write(&wasm1, b"\0asm\x01\0\0\0dummy1").unwrap();
    std::fs::write(&wasm2, b"\0asm\x01\0\0\0dummy2").unwrap();

    let config = dev_parse(&format!(
        r#"
[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]

[[extensions]]
id = "first-ext"
path = "{}"
enabled = true

[[extensions]]
id = "second-ext"
path = "{}"
enabled = false
"#,
        wasm1.display(),
        wasm2.display(),
    ))
    .unwrap();

    let entries = config.stage_extensions(&out_dir).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].url, "/extensions/first-ext.wasm");
    assert_eq!(entries[1].priority, 1);
    assert!(out_dir.join("first-ext.wasm").exists());
    assert!(out_dir.join("second-ext.wasm").exists());

    let manifest: Vec<ExtensionManifestEntry> =
        serde_json::from_str(&std::fs::read_to_string(out_dir.join("manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest, entries);
}

#[test]
fn a_missing_wasm_fails_without_partial_output() {
    let temp_dir = tempfile::tempdir().unwrap();
    let src_dir = temp_dir.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let present = src_dir.join("present.wasm");
    std::fs::write(&present, b"\0asm\x01\0\0\0present").unwrap();
    let missing = src_dir.join("missing.wasm");
    let out_dir = temp_dir.path().join("staged");

    let config = dev_parse(&format!(
        r#"
[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]

[[extensions]]
id = "present-ext"
path = "{}"

[[extensions]]
id = "missing-ext"
path = "{}"
"#,
        present.display(),
        missing.display(),
    ))
    .unwrap();

    let err = config.stage_extensions(&out_dir).unwrap_err();
    match err {
        alhalte_restaurant::RestaurantError::ExtensionWasmMissing { id, path } => {
            assert_eq!(id, "missing-ext");
            assert_eq!(path, missing);
        }
        other => panic!("expected ExtensionWasmMissing, got {other:?}"),
    }
    assert!(!out_dir.join("present-ext.wasm").exists());
    assert!(!out_dir.join("missing-ext.wasm").exists());
    assert!(!out_dir.join("manifest.json").exists());
}

#[test]
fn stages_relative_paths_against_the_base_dir() {
    let temp_dir = tempfile::tempdir().unwrap();
    let base_dir = temp_dir.path().join("config");
    let wasm_dir = base_dir.join("wasm");
    std::fs::create_dir_all(&wasm_dir).unwrap();
    std::fs::write(wasm_dir.join("rel.wasm"), b"\0asm\x01\0\0\0rel").unwrap();
    let out_dir = temp_dir.path().join("staged");

    let config = dev_parse(
        r#"
[maitre-d]
client-id = "id"
client-secret = "sec"
salt = "salt"

[cellar]

[[extensions]]
id = "rel-ext"
path = "wasm/rel.wasm"
"#,
    )
    .unwrap();
    let entries = config
        .stage_extensions_with_base(&out_dir, Some(&base_dir))
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert!(out_dir.join("rel-ext.wasm").exists());
}

// spec: alhalte-restaurant/secrets 0007 identity resolution order
#[test]
fn resolves_age_identity_precedence() {
    use alhalte_restaurant::secrets::resolve_age_identity_source;

    assert_eq!(
        resolve_age_identity_source(Some("  explicit-key  ")).as_deref(),
        Some("explicit-key")
    );
    assert_ne!(
        resolve_age_identity_source(Some("   ")).as_deref(),
        Some("   ")
    );

    if std::env::var("ROPS_AGE").is_err() {
        alhalte_restaurant::ffi::with_scoped_env_var(
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

// spec: alhalte-restaurant/config files
#[test]
fn the_bundled_restaurant_toml_is_parseable_and_has_the_official_extensions() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent().unwrap().parent().unwrap();
    let config = dev_load(&repo_root.join("Restaurant.toml")).unwrap();

    assert!(config.maitre_d.local_login);
    assert_eq!(config.extensions.len(), 6);
    assert_eq!(config.extensions[0].id, "alhalte-dsl");
    assert_eq!(config.extensions[5].id, "emoklore");
    assert!(config.cellar.service_token.is_some());

    let example = std::fs::read_to_string(repo_root.join("Restaurant.example.toml"))
        .expect("Restaurant.example.toml must be checked in");
    assert!(!example.contains("alhalte-local-"));
    assert!(example.contains("alhalte-bistrot encrypt-value"));
}

// spec: alhalte-restaurant/secrets 0008 the example template is fail-closed
#[test]
fn the_example_configuration_requires_a_key() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let example_path = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("Restaurant.example.toml");
    let content =
        std::fs::read_to_string(&example_path).expect("Restaurant.example.toml must be checked in");

    if std::env::var("ROPS_AGE").is_err() && std::env::var("ALHALTE_AGE_KEY").is_err() {
        let err =
            parse_restaurant_toml_with_options(&content, &ParseOptions::default()).unwrap_err();
        assert!(
            matches!(err, alhalte_restaurant::RestaurantError::AgeKeyMissing),
            "expected AgeKeyMissing for the template, got {err:?}"
        );
    }
}
