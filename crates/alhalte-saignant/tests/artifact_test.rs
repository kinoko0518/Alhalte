//! Verifies the committed `alhalte_saignant.wasm` artifact against the Rust ABI
//! source so the checked-in binary cannot silently drift from the code.
//!
//! The test parses the WebAssembly binary directly (no JS runtime / network) and
//! reads the `i32.const` returned by the exported `alhalte_abi_version`.

use std::path::PathBuf;

use wasmparser::{Imports, Operator, Parser, Payload, TypeRef};

const WASM_RELATIVE_PATH: &str = "../../web/alhalte-argenterie/public/wasm/alhalte_saignant.wasm";
const CLIENT_SAIGNANT_RELATIVE_PATH: &str = "../../web/alhalte-argenterie/src/utils/saignant.ts";

fn workspace_file(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

/// Counts imported functions, which offset function indices in the code section.
fn imported_function_count(reader: wasmparser::ImportSectionReader<'_>) -> u32 {
    let mut count = 0u32;
    for group in reader {
        match group.expect("valid import section") {
            Imports::Single(_, import) => {
                if matches!(import.ty, TypeRef::Func(_)) {
                    count += 1;
                }
            }
            Imports::Compact1 { items, .. } => {
                for item in items {
                    if matches!(item.expect("valid import item").ty, TypeRef::Func(_)) {
                        count += 1;
                    }
                }
            }
            Imports::Compact2 { ty, names, .. } => {
                if matches!(ty, TypeRef::Func(_)) {
                    count += names.count();
                }
            }
        }
    }
    count
}

// spec: alhalte-saignant/abi v2 0012 committed wasm ABI version / exports match source (Issue #163)
#[test]
fn committed_wasm_matches_rust_abi_version() {
    let wasm_path = workspace_file(WASM_RELATIVE_PATH);
    let bytes = std::fs::read(&wasm_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", wasm_path.display()));

    let mut imported_functions = 0u32;
    let mut code_index = 0u32;
    let mut version_function: Option<u32> = None;
    let mut has_v1 = false;
    let mut has_v2 = false;
    let mut has_css = false;
    let mut returned_version: Option<i32> = None;

    for payload in Parser::new(0).parse_all(&bytes) {
        match payload.expect("valid wasm payload") {
            Payload::ImportSection(reader) => {
                imported_functions = imported_function_count(reader);
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.expect("valid export");
                    match export.name {
                        "alhalte_abi_version" => version_function = Some(export.index),
                        "render_generast_abi" => has_v1 = true,
                        "render_generast_abi_v2" => has_v2 = true,
                        "get_full_css_abi" => has_css = true,
                        _ => {}
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let function_index = imported_functions + code_index;
                code_index += 1;
                if Some(function_index) == version_function {
                    let mut operators = body.get_operators_reader().expect("valid code body");
                    while !operators.eof() {
                        if let Operator::I32Const { value } =
                            operators.read().expect("valid operator")
                        {
                            returned_version = Some(value);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    assert!(
        has_v1,
        "committed wasm must still export render_generast_abi (v1 compatibility)"
    );
    assert!(
        has_v2,
        "committed wasm must export render_generast_abi_v2; rebuild with scripts/build-saignant-wasm.sh"
    );
    assert!(has_css, "committed wasm must export get_full_css_abi");
    assert_eq!(
        returned_version,
        Some(alhalte_saignant::ABI_VERSION),
        "committed wasm alhalte_abi_version does not match the Rust ABI_VERSION; rebuild with scripts/build-saignant-wasm.sh"
    );

    // The client constant is the other half of the ABI contract: if the wasm
    // and Rust move to v3, the TypeScript side must move too.
    let client_source = std::fs::read_to_string(workspace_file(CLIENT_SAIGNANT_RELATIVE_PATH))
        .expect("read client saignant.ts");
    let client_version = client_source
        .lines()
        .find_map(|line| {
            let rest = line
                .trim()
                .strip_prefix("export const CURRENT_ALHALTE_ABI_VERSION")?;
            rest.split('=')
                .nth(1)?
                .trim()
                .trim_end_matches(';')
                .trim()
                .parse::<i32>()
                .ok()
        })
        .expect("CURRENT_ALHALTE_ABI_VERSION in client saignant.ts");

    assert_eq!(
        client_version,
        alhalte_saignant::ABI_VERSION,
        "client CURRENT_ALHALTE_ABI_VERSION must match the Rust ABI_VERSION"
    );
}
