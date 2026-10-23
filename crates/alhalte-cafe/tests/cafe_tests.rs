//! CLI conformance tests for `alhalte-cafe`.

use std::path::PathBuf;

fn write_wasm(path: &std::path::Path) {
    std::fs::write(path, b"\0asm\x01\0\0\0dummy").unwrap();
}

// spec: alhalte-cafe/stage-extensions
#[test]
fn stage_extensions_cli_writes_the_manifest() {
    let bin_path = env!("CARGO_BIN_EXE_alhalte-cafe");
    let temp_dir = tempfile::tempdir().unwrap();
    let config_dir = temp_dir.path().join("config");
    let wasm_dir = config_dir.join("wasm");
    std::fs::create_dir_all(&wasm_dir).unwrap();
    let wasm = wasm_dir.join("ext.wasm");
    write_wasm(&wasm);
    let out_dir = temp_dir.path().join("staged");

    let config_path = config_dir.join("Restaurant.toml");
    std::fs::write(
        &config_path,
        r#"
[maitre-d]
salt = "salt"
local-login = true

[cellar]
service-token = "token"

[[extensions]]
id = "demo-ext"
path = "wasm/ext.wasm"
"#,
    )
    .unwrap();

    let output = std::process::Command::new(bin_path)
        .env("ALHALTE_ALLOW_PLAINTEXT_SECRETS", "1")
        .arg("-c")
        .arg(&config_path)
        .arg("stage-extensions")
        .arg("--output-dir")
        .arg(&out_dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(out_dir.join("demo-ext.wasm").exists());
    let manifest = std::fs::read_to_string(out_dir.join("manifest.json")).unwrap();
    assert!(manifest.contains("\"id\": \"demo-ext\""));
    assert!(manifest.contains("\"url\": \"/extensions/demo-ext.wasm\""));
}

// spec: alhalte-cafe/config resolution
#[test]
fn a_missing_configuration_is_reported() {
    let bin_path = env!("CARGO_BIN_EXE_alhalte-cafe");
    let temp_dir = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(bin_path)
        .current_dir(temp_dir.path())
        .env_remove("ALHALTE_ALLOW_PLAINTEXT_SECRETS")
        .arg("--no-browser")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("configuration not found"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("Restaurant.toml"), "stderr: {stderr}");
}

// spec: alhalte-cafe/config files
#[test]
fn the_bundled_restaurant_configuration_is_parseable() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent().unwrap().parent().unwrap();
    alhalte_restaurant::load_restaurant_from_path_with_options(
        repo_root.join("Restaurant.toml"),
        &alhalte_restaurant::ParseOptions {
            age_key: None,
            allow_plaintext_secrets: true,
        },
    )
    .unwrap();
}
