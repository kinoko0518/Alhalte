//! Tests for aozora extension.

use aozora::abi::alhalte_abi_version;
use aozora::process_ast;
use generast::{
    Generast, decode_ext_io, encode_ext_io, extract_ruby, is_bold_meta, is_boten_meta,
    is_grounded_meta, is_h_in_v_meta, is_h1_meta, is_h2_meta, is_h3_meta, is_italic_meta,
};

// spec: extensions/aozora ruby and boten
#[test]
fn test_ruby_and_boten() {
    let speaker = 1;

    // Ruby: ｜東京《とうきょう》
    let ast_ruby = process_ast(Generast::string("｜東京《とうきょう》"), speaker);
    match ast_ruby {
        Generast::Nested(n) => {
            assert_eq!(extract_ruby(&n.meta).unwrap(), "とうきょう");
            assert_eq!(n.contents, vec![Generast::string("東京")]);
        }
        other => panic!("expected nested ruby, got {:?}", other),
    }

    // Boten: ［＃傍点］強調［＃傍点終わり］
    let ast_boten = process_ast(Generast::string("［＃傍点］強調［＃傍点終わり］"), speaker);
    match ast_boten {
        Generast::Nested(n) => {
            assert!(is_boten_meta(&n.meta));
            assert_eq!(n.contents, vec![Generast::string("強調")]);
        }
        other => panic!("expected nested boten, got {:?}", other),
    }
}

// spec: extensions/aozora headings (h1, h2, h3)
#[test]
fn test_headings() {
    let speaker = 1;

    // h1: ［＃大見出し］第１章［＃大見出し終わり］
    let ast_h1 = process_ast(
        Generast::string("［＃大見出し］第１章［＃大見出し終わり］"),
        speaker,
    );
    match ast_h1 {
        Generast::Nested(n) => {
            assert!(is_h1_meta(&n.meta));
            assert_eq!(n.contents, vec![Generast::string("第１章")]);
        }
        other => panic!("expected nested h1, got {:?}", other),
    }

    // h2: ［＃中見出し］節１［＃中見出し終わり］
    let ast_h2 = process_ast(
        Generast::string("［＃中見出し］節１［＃中見出し終わり］"),
        speaker,
    );
    match ast_h2 {
        Generast::Nested(n) => {
            assert!(is_h2_meta(&n.meta));
            assert_eq!(n.contents, vec![Generast::string("節１")]);
        }
        other => panic!("expected nested h2, got {:?}", other),
    }

    // h3: ［＃小見出し］項１［＃小見出し終わり］
    let ast_h3 = process_ast(
        Generast::string("［＃小見出し］項１［＃小見出し終わり］"),
        speaker,
    );
    match ast_h3 {
        Generast::Nested(n) => {
            assert!(is_h3_meta(&n.meta));
            assert_eq!(n.contents, vec![Generast::string("項１")]);
        }
        other => panic!("expected nested h3, got {:?}", other),
    }
}

// spec: extensions/aozora styles: bold, italic, grounded, hin-v, indent, hanging, kerning
#[test]
fn test_styles() {
    let speaker = 1;

    // Bold: ［＃太字］太字本文［＃太字終わり］
    let ast_bold = process_ast(
        Generast::string("［＃太字］太字本文［＃太字終わり］"),
        speaker,
    );
    match ast_bold {
        Generast::Nested(n) => {
            assert!(is_bold_meta(&n.meta));
            assert_eq!(n.contents, vec![Generast::string("太字本文")]);
        }
        other => panic!("expected nested bold, got {:?}", other),
    }

    // Italic: ［＃斜体］斜体本文［＃斜体終わり］
    let ast_italic = process_ast(
        Generast::string("［＃斜体］斜体本文［＃斜体終わり］"),
        speaker,
    );
    match ast_italic {
        Generast::Nested(n) => {
            assert!(is_italic_meta(&n.meta));
            assert_eq!(n.contents, vec![Generast::string("斜体本文")]);
        }
        other => panic!("expected nested italic, got {:?}", other),
    }

    // Grounded: ［＃地付き］奥付
    let ast_grounded = process_ast(Generast::string("［＃地付き］奥付"), speaker);
    match ast_grounded {
        Generast::Nested(n) => {
            assert!(is_grounded_meta(&n.meta));
            assert_eq!(n.contents, vec![Generast::string("奥付")]);
        }
        other => panic!("expected nested grounded, got {:?}", other),
    }

    // H-in-V: 50［＃「50」は縦中横］
    let ast_hinv = process_ast(Generast::string("50［＃「50」は縦中横］"), speaker);
    match ast_hinv {
        Generast::Nested(n) => {
            assert!(is_h_in_v_meta(&n.meta));
            assert_eq!(n.contents, vec![Generast::string("50")]);
        }
        other => panic!("expected nested h-in-v, got {:?}", other),
    }
}

// spec: extensions/aozora ABI roundtrip
#[test]
fn test_aozora_abi() {
    assert_eq!(alhalte_abi_version(), 1);

    let speaker_id = 999u64;
    let input_ast = Generast::string("［＃太字］重要［＃太字終わり］");
    let input_tin = encode_ext_io(&input_ast, speaker_id).unwrap();
    let input_bytes = input_tin.to_plaintext();

    let out_bytes = aozora::abi::process_raw_bytes(&input_bytes, process_ast);
    assert!(!out_bytes.is_empty());

    let (out_tin, _, _) = tin::Tin::decode(&out_bytes, None)
        .or_else(|_| {
            tin::Tin::from_plaintext(&out_bytes).map(|t| {
                (
                    t,
                    tin::Opener::new(tin::Encryption::Plaintext, tin::Compression::None),
                    [0u8; 32],
                )
            })
        })
        .unwrap();

    let (out_ast, out_speaker) = decode_ext_io(&out_tin).unwrap();
    assert_eq!(out_speaker, speaker_id);
    match out_ast {
        Generast::Nested(n) => assert!(is_bold_meta(&n.meta)),
        other => panic!("expected nested bold, got {:?}", other),
    }
}

// spec: extensions/aozora self-introduction metadata
#[test]
fn test_self_introduction() {
    let (name, icon_svg, description) = aozora::self_introduction_info();
    assert_eq!(name, "青空文庫ルビ・傍点");
    assert!(icon_svg.starts_with("<svg") && icon_svg.ends_with("</svg>"));
    assert_eq!(
        description,
        "青空文庫形式のルビ（｜親文字《ルビ》）と傍点（［＃「…」に傍点］）を変換する公式拡張機能"
    );

    let bytes = aozora::abi::encode_self_introduction();
    let decoded: (String, String, String) = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(decoded.0, name);
    assert_eq!(decoded.1, icon_svg);
    assert_eq!(decoded.2, description);

    let packed = aozora::abi::self_introduction();
    assert_ne!(packed, 0);
    let len = (packed & 0xffff_ffff) as usize;
    assert_eq!(len, bytes.len());
}
