//! Tests for alhalte-saignant WebAssembly ABI exports.

use alhalte_saignant::abi::{
    ABI_VERSION, alhalte_abi_version, alloc, dealloc, get_full_css_abi, render_generast_abi,
    render_generast_abi_v2, unpack_ptr_len,
};
use generast::Generast;

/// Copies `bytes` into a wasm-allocated input buffer.
fn alloc_bytes(bytes: &[u8]) -> *mut u8 {
    let ptr = alloc(bytes.len());
    assert!(!ptr.is_null());
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    }
    ptr
}

/// Reads and deallocates the packed `(ptr << 32) | len` output.
fn take_output(packed: u64) -> String {
    assert_ne!(packed, 0);
    let (out_ptr, out_len) = unsafe { unpack_ptr_len(packed) };
    assert!(!out_ptr.is_null());
    assert!(out_len > 0);
    let html_bytes = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
    let html = std::str::from_utf8(html_bytes).unwrap().to_string();
    unsafe {
        dealloc(out_ptr, out_len);
    }
    html
}

/// Renders `ast` through the v2 ABI with the given JSON names object.
fn render_v2(ast: &Generast, names_json: &str) -> String {
    let tin = ast.to_tin().unwrap();
    let plaintext = tin.to_plaintext();
    let in_ptr = alloc_bytes(&plaintext);
    let names_ptr = alloc_bytes(names_json.as_bytes());

    let packed =
        unsafe { render_generast_abi_v2(in_ptr, plaintext.len(), names_ptr, names_json.len()) };
    unsafe {
        dealloc(in_ptr, plaintext.len());
        dealloc(names_ptr, names_json.len());
    }
    take_output(packed)
}

// spec: alhalte-saignant/abi 0001 ABI version and memory management
#[test]
fn test_abi_version_and_alloc() {
    assert_eq!(ABI_VERSION, 2);
    assert_eq!(alhalte_abi_version(), ABI_VERSION);

    let ptr = alloc(64);
    assert!(!ptr.is_null());
    unsafe {
        dealloc(ptr, 64);
    }
}

// spec: alhalte-saignant/abi 0002 render Generast Tin to HTML
#[test]
fn test_render_generast_abi() {
    let ast = Generast::bold(vec![Generast::string("Critical Hit!")]);
    let tin = ast.to_tin().unwrap();
    let plaintext = tin.to_plaintext();

    let in_ptr = alloc_bytes(&plaintext);
    let packed = unsafe { render_generast_abi(in_ptr, plaintext.len()) };
    unsafe {
        dealloc(in_ptr, plaintext.len());
    }

    let html = take_output(packed);
    assert!(html.contains("<strong class=\"saignant-bold\">Critical Hit!</strong>"));
}

// spec: alhalte-saignant/abi 0003 get full CSS stylesheet
#[test]
fn test_get_full_css_abi() {
    let packed = get_full_css_abi();
    let css = take_output(packed);
    assert!(css.contains(".saignant-bold"));
}

// spec: alhalte-saignant/abi 0004 render extension I/O Tin (Generast, speaker_id) to HTML (Issue #127)
#[test]
fn test_render_generast_ext_io_tin() {
    let ast = Generast::bold(vec![Generast::string("ほげほげ")]);
    let tin = generast::encode_ext_io(&ast, 42).unwrap();
    let plaintext = tin.to_plaintext();

    let in_ptr = alloc_bytes(&plaintext);
    let packed = unsafe { render_generast_abi(in_ptr, plaintext.len()) };
    unsafe {
        dealloc(in_ptr, plaintext.len());
    }

    let html = take_output(packed);
    assert!(
        html.contains("<strong class=\"saignant-bold\">ほげほげ</strong>"),
        "expected rendered bold Japanese string, got: {html}"
    );
}

// spec: alhalte-saignant/abi 0005 render raw postcard (Generast, u64) fallback to HTML (Issue #127)
#[test]
fn test_render_generast_raw_postcard_ext_io() {
    let ast = Generast::string("こんにちは世界");
    let bytes = postcard::to_allocvec(&(ast, 999u64)).unwrap();

    let in_ptr = alloc_bytes(&bytes);
    let packed = unsafe { render_generast_abi(in_ptr, bytes.len()) };
    unsafe {
        dealloc(in_ptr, bytes.len());
    }

    let html = take_output(packed);
    assert!(
        html.contains("こんにちは世界"),
        "expected rendered Japanese text, got: {html}"
    );
}

// spec: alhalte-saignant/abi v2 0006 mention resolves the character name (Issue #163)
#[test]
fn test_render_generast_abi_v2_resolves_mention() {
    let ast = Generast::character_id(12);
    let html = render_v2(&ast, r#"{"12":"Alice"}"#);

    assert!(
        html.contains("<span class=\"saignant-mention\" data-character-id=\"12\">@Alice</span>"),
        "expected @Alice mention, got: {html}"
    );
    assert!(!html.contains("Anonymous"), "unexpected fallback: {html}");
}

// spec: alhalte-saignant/abi v2 0007 fluct / assign status resolves the character name (Issue #163)
#[test]
fn test_render_generast_abi_v2_resolves_fluct_and_assign() {
    let fluct = Generast::fluct(12, "HP", -5).unwrap();
    let fluct_html = render_v2(&fluct, r#"{"12":"Alice"}"#);
    assert!(
        fluct_html.contains("saignant-fluct"),
        "expected fluct markup, got: {fluct_html}"
    );
    assert!(
        fluct_html.contains("Alice.HP-5"),
        "expected Alice.HP-5, got: {fluct_html}"
    );
    assert!(!fluct_html.contains("Anonymous"));

    let assign = Generast::assign(12, "SAN", 42).unwrap();
    let assign_html = render_v2(&assign, r#"{"12":"Alice"}"#);
    assert!(
        assign_html.contains("saignant-assign"),
        "expected assign markup, got: {assign_html}"
    );
    assert!(
        assign_html.contains("Alice.SAN=42"),
        "expected Alice.SAN=42, got: {assign_html}"
    );
    assert!(!assign_html.contains("Anonymous"));
}

// spec: alhalte-saignant/abi v2 0008 v1 keeps the Anonymous fallback for compatibility (Issue #163)
#[test]
fn test_render_generast_abi_v1_keeps_anonymous_fallback() {
    let ast = Generast::character_id(12);
    let tin = ast.to_tin().unwrap();
    let plaintext = tin.to_plaintext();
    let in_ptr = alloc_bytes(&plaintext);

    let packed = unsafe { render_generast_abi(in_ptr, plaintext.len()) };
    unsafe {
        dealloc(in_ptr, plaintext.len());
    }

    let html = take_output(packed);
    assert!(
        html.contains("@Anonymous"),
        "expected v1 fallback, got: {html}"
    );
}

// spec: alhalte-saignant/abi v2 0009 malformed / missing names degrade to Anonymous (Issue #163)
#[test]
fn test_render_generast_abi_v2_tolerates_bad_names() {
    let ast = Generast::character_id(12);

    let empty = render_v2(&ast, "{}");
    assert!(
        empty.contains("@Anonymous"),
        "expected empty map fallback: {empty}"
    );

    let malformed = render_v2(&ast, "not json");
    assert!(
        malformed.contains("@Anonymous"),
        "expected malformed map fallback: {malformed}"
    );

    // Non-numeric keys (local test character IDs) are ignored, not fatal.
    let non_numeric = render_v2(&ast, r#"{"char-1":"Lucia"}"#);
    assert!(
        non_numeric.contains("@Anonymous"),
        "expected non-numeric key fallback: {non_numeric}"
    );
}

// spec: alhalte-saignant/abi v2 0010 names are HTML-escaped (Issue #163)
#[test]
fn test_render_generast_abi_v2_escapes_names() {
    let ast = Generast::character_id(7);
    let html = render_v2(&ast, r#"{"7":"<script>alert(1)</script>"}"#);

    assert!(
        !html.contains("<script>"),
        "name must be HTML-escaped, got: {html}"
    );
    assert!(html.contains("&lt;script&gt;"), "got: {html}");
}

// spec: alhalte-saignant/abi v2 0011 null names buffer renders like v1 (Issue #163)
#[test]
fn test_render_generast_abi_v2_null_names_buffer() {
    let ast = Generast::character_id(12);
    let tin = ast.to_tin().unwrap();
    let plaintext = tin.to_plaintext();
    let in_ptr = alloc_bytes(&plaintext);

    let packed = unsafe { render_generast_abi_v2(in_ptr, plaintext.len(), std::ptr::null(), 0) };
    unsafe {
        dealloc(in_ptr, plaintext.len());
    }

    let html = take_output(packed);
    assert!(html.contains("@Anonymous"), "got: {html}");
}
