//! Tests for alhalte-dsl extension.

use alhalte_dsl::abi::alhalte_abi_version;
use alhalte_dsl::process_ast;
use generast::{
    Generast, SeededEntropy, decode_ext_io, encode_ext_io, extract_assign, extract_character_id,
    extract_diceroll, extract_fluct, extract_syntax_error,
};

// spec: extensions/alhalte-dsl dice rolling expressions
#[test]
fn test_dice_expressions() {
    let speaker_id = 42;

    // 2D6
    let ast = process_ast(Generast::string("2D6"), speaker_id);
    let tin = match ast {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    let dice_data = extract_diceroll(&tin).unwrap();
    let mut entropy = SeededEntropy::from_seed(dice_data.seed);
    let val = dice_data.expr.eval(&mut entropy).unwrap();
    let sum = val.as_rational().unwrap();
    assert!(
        sum >= generast::Rational::from_integer(2) && sum <= generast::Rational::from_integer(12)
    );

    // Complex: (1+3)D(2+2)
    let ast2 = process_ast(Generast::string("(1+3)D(2+2)"), speaker_id);
    let tin2 = match ast2 {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    let data2 = extract_diceroll(&tin2).unwrap();
    let mut entropy2 = SeededEntropy::from_seed(data2.seed);
    let val2 = data2.expr.eval(&mut entropy2).unwrap();
    let sum2 = val2.as_rational().unwrap();
    assert!(
        sum2 >= generast::Rational::from_integer(4) && sum2 <= generast::Rational::from_integer(16)
    );

    // Comparison: 2D6+3>=10
    let ast3 = process_ast(Generast::string("2D6+3>=10"), speaker_id);
    let tin3 = match ast3 {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    let data3 = extract_diceroll(&tin3).unwrap();
    let mut entropy3 = SeededEntropy::from_seed(data3.seed);
    let val3 = data3.expr.eval(&mut entropy3).unwrap();
    assert!(val3.as_bool().is_ok());

    // Keep highest: 4D6KH3
    let ast4 = process_ast(Generast::string("4D6KH3"), speaker_id);
    let tin4 = match ast4 {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    let data4 = extract_diceroll(&tin4).unwrap();
    let mut entropy4 = SeededEntropy::from_seed(data4.seed);
    let val4 = data4.expr.eval(&mut entropy4).unwrap();
    let sum4 = val4.as_rational().unwrap();
    assert!(
        sum4 >= generast::Rational::from_integer(3) && sum4 <= generast::Rational::from_integer(18)
    );
}

// spec: extensions/alhalte-dsl special dice: D66, Reroll, Upper, Tally, Choice, Repeat
#[test]
fn test_special_dice_features() {
    let speaker = 100;

    // D66
    let ast_d66 = process_ast(Generast::string("D66S"), speaker);
    let tin_d66 = match ast_d66 {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    let data_d66 = extract_diceroll(&tin_d66).unwrap();
    let mut entropy = SeededEntropy::from_seed(data_d66.seed);
    let val = data_d66
        .expr
        .eval(&mut entropy)
        .unwrap()
        .as_integer()
        .unwrap();
    assert!((11..=66).contains(&val));

    // Choice
    let ast_choice = process_ast(Generast::string("choice[rock, paper, scissors]"), speaker);
    let tin_choice = match ast_choice {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    let data_choice = extract_diceroll(&tin_choice).unwrap();
    let mut entropy_c = SeededEntropy::from_seed(data_choice.seed);
    let val_c = data_choice.expr.eval(&mut entropy_c).unwrap();
    let list_c = val_c.as_list().unwrap();
    assert_eq!(list_c.len(), 1);
    let picked = list_c[0].as_str().unwrap();
    assert!(["rock", "paper", "scissors"].contains(&picked));

    // Repeat: x3 1D6
    let ast_rep = process_ast(Generast::string("x3 1D6"), speaker);
    let tin_rep = match ast_rep {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    let data_rep = extract_diceroll(&tin_rep).unwrap();
    let mut entropy_rep = SeededEntropy::from_seed(data_rep.seed);
    let val_rep = data_rep.expr.eval(&mut entropy_rep).unwrap();
    let list_rep = val_rep.as_list().unwrap();
    assert_eq!(list_rep.len(), 3);
}

// spec: extensions/alhalte-dsl status fluctuation and assignment
#[test]
fn test_status_fluct_and_assign() {
    let speaker = 1;

    // Fluctuation: {1001}.HP+5
    let ast_fluct_pos = process_ast(Generast::string("{1001}.HP+5"), speaker);
    let tin_f1 = match ast_fluct_pos {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    let f1 = extract_fluct(&tin_f1).unwrap();
    assert_eq!(f1.character_id, 1001);
    assert_eq!(f1.status, "HP");
    assert_eq!(f1.delta, 5);

    // Fluctuation negative: {1001}.MP-3
    let ast_fluct_neg = process_ast(Generast::string("{1001}.MP-3"), speaker);
    let tin_f2 = match ast_fluct_neg {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    let f2 = extract_fluct(&tin_f2).unwrap();
    assert_eq!(f2.character_id, 1001);
    assert_eq!(f2.status, "MP");
    assert_eq!(f2.delta, -3);

    // Assignment: {2002}.SAN=50
    let ast_assign = process_ast(Generast::string("{2002}.SAN=50"), speaker);
    let tin_a = match ast_assign {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    let a = extract_assign(&tin_a).unwrap();
    assert_eq!(a.character_id, 2002);
    assert_eq!(a.status, "SAN");
    assert_eq!(a.value, 50);
}

// spec: extensions/alhalte-dsl character mention
#[test]
fn test_character_mentions() {
    let speaker = 1;

    // Standalone @1234
    let ast_at = process_ast(Generast::string("@1234"), speaker);
    let tin_at = match ast_at {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    assert_eq!(extract_character_id(&tin_at).unwrap(), 1234);

    // Standalone {5678}
    let ast_brace = process_ast(Generast::string("{5678}"), speaker);
    let tin_brace = match ast_brace {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    assert_eq!(extract_character_id(&tin_brace).unwrap(), 5678);

    // Embedded in text: "Hello @999 !"
    let ast_mixed = process_ast(Generast::string("Hello @999 !"), speaker);
    let nested = match ast_mixed {
        Generast::Nested(n) => n,
        other => panic!("expected nested, got {:?}", other),
    };
    assert_eq!(nested.contents.len(), 3);
    assert_eq!(nested.contents[0], Generast::string("Hello "));
    assert!(
        matches!(&nested.contents[1], Generast::Element(t) if extract_character_id(t).unwrap() == 999)
    );
    assert_eq!(nested.contents[2], Generast::string(" !"));
}

// spec: extensions/alhalte-dsl syntax errors
#[test]
fn test_syntax_errors() {
    let speaker = 1;

    // Unmatched parenthesis: (1+2D6
    let ast1 = process_ast(Generast::string("(1+2D6"), speaker);
    let tin1 = match ast1 {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    assert!(extract_syntax_error(&tin1).is_ok());

    // Incomplete dice: 2D
    let ast2 = process_ast(Generast::string("2D"), speaker);
    let tin2 = match ast2 {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    assert!(extract_syntax_error(&tin2).is_ok());

    // Incomplete choice: choice[a,b
    let ast3 = process_ast(Generast::string("choice[a,b"), speaker);
    let tin3 = match ast3 {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    assert!(extract_syntax_error(&tin3).is_ok());
}

// spec: extensions/alhalte-dsl ABI roundtrip
#[test]
fn test_abi_exports() {
    assert_eq!(alhalte_abi_version(), 1);

    let speaker_id = 777u64;
    let input_ast = Generast::string("2D6");
    let input_tin = encode_ext_io(&input_ast, speaker_id).unwrap();
    let input_bytes = input_tin.to_plaintext();

    let out_bytes = alhalte_dsl::abi::process_raw_bytes(&input_bytes, process_ast);
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
    let tin = match out_ast {
        Generast::Element(t) => t,
        other => panic!("expected element, got {:?}", other),
    };
    assert!(extract_diceroll(&tin).is_ok());
}

// spec: extensions/alhalte-dsl self-introduction metadata
#[test]
fn test_self_introduction() {
    let (name, icon_svg, description) = alhalte_dsl::self_introduction_info();
    assert_eq!(name, "Alhalte DSL");
    assert!(icon_svg.starts_with("<svg") && icon_svg.ends_with("</svg>"));
    assert_eq!(
        description,
        "ダイスロール（xDy）・四則演算・比較判定・シークレットダイス・変数展開を処理する公式DSL拡張機能"
    );

    let bytes = alhalte_dsl::abi::encode_self_introduction();
    let decoded: (String, String, String) = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(decoded.0, name);
    assert_eq!(decoded.1, icon_svg);
    assert_eq!(decoded.2, description);

    let packed = alhalte_dsl::abi::self_introduction();
    assert_ne!(packed, 0);
    let len = (packed & 0xffff_ffff) as usize;
    assert_eq!(len, bytes.len());
}
