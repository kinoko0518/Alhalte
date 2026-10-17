//! Tests for generast AST and extension ABI encoding/decoding.

use generast::{Generast, decode_ext_io, decode_generast, encode_ext_io, encode_generast};
use tin::Tin;

// spec: generast-rs/ast nested and generast representation
#[test]
fn test_ast_construction_and_serde() {
    let text = Generast::string("Hello world");
    let bold_node = Generast::bold(vec![text]);
    let nested = match &bold_node {
        Generast::Nested(n) => n,
        _ => panic!("expected nested node"),
    };
    assert_eq!(nested.contents.len(), 1);

    // Serialization via postcard
    let bytes = postcard::to_allocvec(&bold_node).unwrap();
    let decoded: Generast = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(bold_node, decoded);
}

// spec: generast-rs/ast encode_generast and decode_generast helpers
#[test]
fn test_generast_tin_codec() {
    let ast = Generast::h1(vec![
        Generast::string("Chapter 1: "),
        Generast::bold(vec![Generast::string("The Beginning")]),
    ]);

    let tin = encode_generast(&ast).unwrap();
    assert_eq!(tin.label, generast::LABEL_ALHALTE_GENERAST);

    let decoded = decode_generast(&tin).unwrap();
    assert_eq!(ast, decoded);

    // Also test member methods
    let tin2 = ast.to_tin().unwrap();
    let decoded2 = Generast::from_tin(&tin2).unwrap();
    assert_eq!(ast, decoded2);
}

// spec: generast-rs/ast extension ABI IO codec (Generast, u64 speaker)
#[test]
fn test_extension_abi_io_codec() {
    let speaker_id = 9876543210u64;
    let ast = Generast::string("test message from speaker");

    let tin = encode_ext_io(&ast, speaker_id).unwrap();
    assert_eq!(tin.label, generast::LABEL_ALHALTE_GENERAST);

    let (decoded_ast, decoded_speaker) = decode_ext_io(&tin).unwrap();
    assert_eq!(decoded_ast, ast);
    assert_eq!(decoded_speaker, speaker_id);
}

// spec: generast-rs/ast invalid label mismatch error
#[test]
fn test_decode_label_mismatch() {
    let invalid_tin = Tin::new(0x1234, vec![1, 2, 3]);
    let err = decode_generast(&invalid_tin).unwrap_err();
    assert!(matches!(err, generast::GenerastError::LabelMismatch { .. }));

    let err_io = decode_ext_io(&invalid_tin).unwrap_err();
    assert!(matches!(
        err_io,
        generast::GenerastError::LabelMismatch { .. }
    ));
}
