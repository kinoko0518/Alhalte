//! Tests for alhalte-saignant WebAssembly ABI exports.

use alhalte_saignant::abi::{
    alhalte_abi_version, alloc, dealloc, get_full_css_abi, render_generast_abi, unpack_ptr_len,
};
use generast::Generast;

// spec: alhalte-saignant/abi 0001 ABI version and memory management
#[test]
fn test_abi_version_and_alloc() {
    assert_eq!(alhalte_abi_version(), 1);

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

    let in_ptr = alloc(plaintext.len());
    assert!(!in_ptr.is_null());
    unsafe {
        std::ptr::copy_nonoverlapping(plaintext.as_ptr(), in_ptr, plaintext.len());
    }

    let packed = unsafe { render_generast_abi(in_ptr, plaintext.len()) };
    unsafe {
        dealloc(in_ptr, plaintext.len());
    }

    assert_ne!(packed, 0);
    let (out_ptr, out_len) = unsafe { unpack_ptr_len(packed) };
    assert!(!out_ptr.is_null());
    assert!(out_len > 0);

    let html_bytes = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
    let html = std::str::from_utf8(html_bytes).unwrap();
    assert!(html.contains("<strong class=\"saignant-bold\">Critical Hit!</strong>"));

    unsafe {
        dealloc(out_ptr, out_len);
    }
}

// spec: alhalte-saignant/abi 0003 get full CSS stylesheet
#[test]
fn test_get_full_css_abi() {
    let packed = get_full_css_abi();
    assert_ne!(packed, 0);
    let (out_ptr, out_len) = unsafe { unpack_ptr_len(packed) };
    assert!(!out_ptr.is_null());
    assert!(out_len > 0);

    let css_bytes = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
    let css = std::str::from_utf8(css_bytes).unwrap();
    assert!(css.contains(".saignant-bold"));

    unsafe {
        dealloc(out_ptr, out_len);
    }
}

// spec: alhalte-saignant/abi 0004 render extension I/O Tin (Generast, speaker_id) to HTML (Issue #127)
#[test]
fn test_render_generast_ext_io_tin() {
    let ast = Generast::bold(vec![Generast::string("ほげほげ")]);
    let tin = generast::encode_ext_io(&ast, 42).unwrap();
    let plaintext = tin.to_plaintext();

    let in_ptr = alloc(plaintext.len());
    assert!(!in_ptr.is_null());
    unsafe {
        std::ptr::copy_nonoverlapping(plaintext.as_ptr(), in_ptr, plaintext.len());
    }

    let packed = unsafe { render_generast_abi(in_ptr, plaintext.len()) };
    unsafe {
        dealloc(in_ptr, plaintext.len());
    }

    assert_ne!(packed, 0);
    let (out_ptr, out_len) = unsafe { unpack_ptr_len(packed) };
    assert!(!out_ptr.is_null());
    assert!(out_len > 0);

    let html_bytes = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
    let html = std::str::from_utf8(html_bytes).unwrap();
    assert!(
        html.contains("<strong class=\"saignant-bold\">ほげほげ</strong>"),
        "expected rendered bold Japanese string, got: {html}"
    );

    unsafe {
        dealloc(out_ptr, out_len);
    }
}

// spec: alhalte-saignant/abi 0005 render raw postcard (Generast, u64) fallback to HTML (Issue #127)
#[test]
fn test_render_generast_raw_postcard_ext_io() {
    let ast = Generast::string("こんにちは世界");
    let bytes = postcard::to_allocvec(&(ast, 999u64)).unwrap();

    let in_ptr = alloc(bytes.len());
    assert!(!in_ptr.is_null());
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), in_ptr, bytes.len());
    }

    let packed = unsafe { render_generast_abi(in_ptr, bytes.len()) };
    unsafe {
        dealloc(in_ptr, bytes.len());
    }

    assert_ne!(packed, 0);
    let (out_ptr, out_len) = unsafe { unpack_ptr_len(packed) };
    assert!(!out_ptr.is_null());
    assert!(out_len > 0);

    let html_bytes = unsafe { std::slice::from_raw_parts(out_ptr, out_len) };
    let html = std::str::from_utf8(html_bytes).unwrap();
    assert!(
        html.contains("こんにちは世界"),
        "expected rendered Japanese text, got: {html}"
    );

    unsafe {
        dealloc(out_ptr, out_len);
    }
}
