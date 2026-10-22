//! Tests for markdown extension.

use generast::{
    Generast, decode_ext_io, encode_ext_io, extract_pill, extract_text_color, is_bold_meta,
    is_h1_meta, is_h2_meta, is_h3_meta, is_h4_meta, is_hidden_meta, is_italic_meta,
};
use markdown::abi::alhalte_abi_version;
use markdown::process_ast;

// spec: extensions/markdown headings (h1 to h4)
#[test]
fn test_headings() {
    let speaker = 1;

    // h1
    let ast1 = process_ast(Generast::string("# Title 1"), speaker);
    match ast1 {
        Generast::Nested(n) => {
            assert!(is_h1_meta(&n.meta));
            assert_eq!(n.contents, vec![Generast::string("Title 1")]);
        }
        other => panic!("expected h1, got {:?}", other),
    }

    // h2
    let ast2 = process_ast(Generast::string("## Title 2"), speaker);
    match ast2 {
        Generast::Nested(n) => {
            assert!(is_h2_meta(&n.meta));
            assert_eq!(n.contents, vec![Generast::string("Title 2")]);
        }
        other => panic!("expected h2, got {:?}", other),
    }

    // h3
    let ast3 = process_ast(Generast::string("### Title 3"), speaker);
    match ast3 {
        Generast::Nested(n) => {
            assert!(is_h3_meta(&n.meta));
            assert_eq!(n.contents, vec![Generast::string("Title 3")]);
        }
        other => panic!("expected h3, got {:?}", other),
    }

    // h4
    let ast4 = process_ast(Generast::string("#### Title 4"), speaker);
    match ast4 {
        Generast::Nested(n) => {
            assert!(is_h4_meta(&n.meta));
            assert_eq!(n.contents, vec![Generast::string("Title 4")]);
        }
        other => panic!("expected h4, got {:?}", other),
    }
}

// spec: extensions/markdown inlines: bold, italic, hidden, text-color, pill
#[test]
fn test_inlines() {
    let speaker = 1;

    // Bold
    let ast_bold = process_ast(Generast::string("**bold text**"), speaker);
    match ast_bold {
        Generast::Nested(n) => {
            assert!(is_bold_meta(&n.meta));
            assert_eq!(n.contents, vec![Generast::string("bold text")]);
        }
        other => panic!("expected bold, got {:?}", other),
    }

    // Italic
    let ast_italic = process_ast(Generast::string("*italic text*"), speaker);
    match ast_italic {
        Generast::Nested(n) => {
            assert!(is_italic_meta(&n.meta));
            assert_eq!(n.contents, vec![Generast::string("italic text")]);
        }
        other => panic!("expected italic, got {:?}", other),
    }

    // Spoiler / Hidden
    let ast_hidden = process_ast(Generast::string("||secret spoiler||"), speaker);
    match ast_hidden {
        Generast::Nested(n) => {
            assert!(is_hidden_meta(&n.meta));
            assert_eq!(n.contents, vec![Generast::string("secret spoiler")]);
        }
        other => panic!("expected hidden, got {:?}", other),
    }

    // Text color
    let ast_color = process_ast(Generast::string("[colored text]{color:#ff0000}"), speaker);
    match ast_color {
        Generast::Nested(n) => {
            assert_eq!(extract_text_color(&n.meta).unwrap(), "#ff0000");
            assert_eq!(n.contents, vec![Generast::string("colored text")]);
        }
        other => panic!("expected text_color, got {:?}", other),
    }

    // Pill
    let ast_pill = process_ast(Generast::string("[tag]{pill:#0055aa}"), speaker);
    match ast_pill {
        Generast::Nested(n) => {
            assert_eq!(extract_pill(&n.meta).unwrap(), "#0055aa");
            assert_eq!(n.contents, vec![Generast::string("tag")]);
        }
        other => panic!("expected pill, got {:?}", other),
    }
}

// spec: extensions/markdown nested formatting and recursive AST processing
#[test]
fn test_nested_and_recursive_processing() {
    let speaker = 1;

    // Nested bold and italic
    let ast_nested = process_ast(Generast::string("**bold and *italic* inside**"), speaker);
    match ast_nested {
        Generast::Nested(n) => {
            assert!(is_bold_meta(&n.meta));
            assert_eq!(n.contents.len(), 3);
            assert_eq!(n.contents[0], Generast::string("bold and "));
            assert!(
                matches!(&n.contents[1], Generast::Nested(inner) if is_italic_meta(&inner.meta))
            );
            assert_eq!(n.contents[2], Generast::string(" inside"));
        }
        other => panic!("expected nested bold, got {:?}", other),
    }

    // Recursion on existing Nested structure
    let pre_existing = Generast::bold(vec![Generast::string("*italic text*")]);
    let processed = process_ast(pre_existing, speaker);
    match processed {
        Generast::Nested(outer) => {
            assert!(is_bold_meta(&outer.meta));
            assert_eq!(outer.contents.len(), 1);
            match &outer.contents[0] {
                Generast::Nested(inner) => {
                    assert!(is_italic_meta(&inner.meta));
                    assert_eq!(inner.contents, vec![Generast::string("italic text")]);
                }
                other => panic!("expected inner italic, got {:?}", other),
            }
        }
        other => panic!("expected outer bold, got {:?}", other),
    }
}

// spec: extensions/markdown ABI roundtrip
#[test]
fn test_markdown_abi() {
    assert_eq!(alhalte_abi_version(), 2);

    let speaker_id = 123u64;
    let input_ast = Generast::string("# Header\n**bold**");
    let input_tin = encode_ext_io(&input_ast, speaker_id).unwrap();
    let input_bytes = input_tin.to_plaintext();

    let out_bytes = markdown::abi::process_raw_bytes(&input_bytes, process_ast);
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
        Generast::Nested(n) => {
            assert_eq!(n.contents.len(), 3);
            assert!(matches!(&n.contents[0], Generast::Nested(h) if is_h1_meta(&h.meta)));
            assert_eq!(n.contents[1], Generast::string("\n"));
            assert!(matches!(&n.contents[2], Generast::Nested(b) if is_bold_meta(&b.meta)));
        }
        other => panic!("expected nested root, got {:?}", other),
    }
}

// spec: extensions/markdown self-introduction metadata
#[test]
fn test_self_introduction() {
    let (id, name, icon_svg, description) = markdown::self_introduction_info();
    assert_eq!(id, "markdown");
    assert_eq!(name, "Markdown");
    assert!(icon_svg.starts_with("<svg") && icon_svg.ends_with("</svg>"));
    assert_eq!(
        description,
        "太字（**）・斜体（*）・打ち消し線（~~）などのMarkdown記法を処理します。"
    );

    assert_eq!(markdown::abi::alhalte_abi_version(), 2);

    let bytes = markdown::abi::encode_self_introduction();
    let decoded: (String, String, String, String) = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(decoded.0, id);
    assert_eq!(decoded.1, name);
    assert_eq!(decoded.2, icon_svg);
    assert_eq!(decoded.3, description);

    let packed = markdown::abi::self_introduction();
    assert_ne!(packed, 0);
    let len = (packed & 0xffff_ffff) as usize;
    assert_eq!(len, bytes.len());
}
