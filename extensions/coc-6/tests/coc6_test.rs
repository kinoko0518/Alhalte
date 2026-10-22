//! Tests for coc-6 extension.

use coc_6::abi::alhalte_abi_version;
use coc_6::parser::judge_coc6;
use coc_6::process_ast;
use generast::{
    DiceExpr, Generast, Rational, SeededEntropy, decode_ext_io, diceroll_tin, encode_ext_io,
    extract_diceroll, extract_judgement,
};

// spec: extensions/coc-6 degree and judgment rules
#[test]
fn test_coc6_judgement_degrees() {
    // target = 50
    // 1..=5 => 決定的成功, degree: 2
    assert_eq!(judge_coc6(1, 50), (2, "決定的成功".to_string()));
    assert_eq!(judge_coc6(5, 50), (2, "決定的成功".to_string()));

    // roll <= 50/5 = 10 (and > 5) => スペシャル, degree: 2
    assert_eq!(judge_coc6(6, 50), (2, "スペシャル".to_string()));
    assert_eq!(judge_coc6(10, 50), (2, "スペシャル".to_string()));

    // 11..=50 => 成功, degree: 1
    assert_eq!(judge_coc6(11, 50), (1, "成功".to_string()));
    assert_eq!(judge_coc6(50, 50), (1, "成功".to_string()));

    // 51..=95 => 失敗, degree: -1
    assert_eq!(judge_coc6(51, 50), (-1, "失敗".to_string()));
    assert_eq!(judge_coc6(95, 50), (-1, "失敗".to_string()));

    // 96..=100 => 致命的失敗, degree: -2
    assert_eq!(judge_coc6(96, 50), (-2, "致命的失敗".to_string()));
    assert_eq!(judge_coc6(100, 50), (-2, "致命的失敗".to_string()));

    // If target = 98: roll 97 <= 98 is 成功
    assert_eq!(judge_coc6(97, 98), (1, "成功".to_string()));
    // If target = 98: roll 99 > 98 is 致命的失敗
    assert_eq!(judge_coc6(99, 98), (-2, "致命的失敗".to_string()));
}

// spec: extensions/coc-6 parse string command 1D100<=70【目星】
#[test]
fn test_coc6_parse_string() {
    let speaker_id = 1234;
    let ast = process_ast(Generast::string("1D100<=70【目星】"), speaker_id);
    let tin = match ast {
        Generast::Element(t) => t,
        other => panic!("expected Element, got {:?}", other),
    };
    let judge = extract_judgement(&tin).unwrap();
    assert_eq!(judge.subject.as_deref(), Some("目星"));
    assert_eq!(judge.target, Some(Rational::from_integer(70)));
    assert!([-2, -1, 1, 2].contains(&judge.degree));

    // Verify inner roll is a diceroll element
    let roll_tin = match judge.roll {
        Generast::Element(t) => t,
        other => panic!("expected roll Element, got {:?}", other),
    };
    let dice = extract_diceroll(&roll_tin).unwrap();
    let mut entropy = SeededEntropy::from_seed(dice.seed);
    let val = dice.expr.eval(&mut entropy).unwrap();
    assert!(val.as_bool().is_ok());

    // Trailing text
    let ast2 = process_ast(Generast::string("1D100<=70【目星】 判定成功！"), speaker_id);
    match ast2 {
        Generast::Nested(nested) => {
            assert_eq!(nested.contents.len(), 2);
            assert!(matches!(&nested.contents[0], Generast::Element(_)));
            assert_eq!(nested.contents[1], Generast::string(" 判定成功！"));
        }
        other => panic!("expected Nested, got {:?}", other),
    }
}

// spec: extensions/coc-6 combine diceroll and suffix bracket
#[test]
fn test_coc6_combine_diceroll_and_suffix() {
    let speaker_id = 999;
    let seed = [42u8; 32];
    let dice_expr = DiceExpr::Le(
        Box::new(DiceExpr::Sum(Box::new(DiceExpr::RollN {
            dice: Box::new(DiceExpr::Dice(generast::types::Dice::new(100))),
            count: Box::new(DiceExpr::Integer(1)),
        }))),
        Box::new(DiceExpr::Integer(60)),
    );
    let roll_tin = diceroll_tin(seed, dice_expr).unwrap();

    let input_ast = Generast::nested(
        tin::Tin::need_not_know(Vec::new()),
        vec![
            Generast::element(roll_tin),
            Generast::string("【聞き耳】 判定中"),
        ],
    );

    let result = process_ast(input_ast, speaker_id);
    let nested = match result {
        Generast::Nested(n) => n,
        other => panic!("expected Nested, got {:?}", other),
    };
    assert_eq!(nested.contents.len(), 2);

    let judge_tin = match &nested.contents[0] {
        Generast::Element(t) => t,
        other => panic!("expected Element, got {:?}", other),
    };
    let judge = extract_judgement(judge_tin).unwrap();
    assert_eq!(judge.subject.as_deref(), Some("聞き耳"));
    assert_eq!(judge.target, Some(Rational::from_integer(60)));
    assert_eq!(nested.contents[1], Generast::string(" 判定中"));
}

// spec: extensions/coc-6 ABI roundtrip
#[test]
fn test_coc6_abi_exports() {
    assert_eq!(alhalte_abi_version(), 2);

    let speaker_id = 1010u64;
    let input_ast = Generast::string("1D100<=50【図書館】");
    let input_tin = encode_ext_io(&input_ast, speaker_id).unwrap();
    let input_bytes = input_tin.to_plaintext();

    let out_bytes = coc_6::abi::process_raw_bytes(&input_bytes, process_ast);
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
        other => panic!("expected Element, got {:?}", other),
    };
    let judge = extract_judgement(&tin).unwrap();
    assert_eq!(judge.subject.as_deref(), Some("図書館"));
}

// spec: extensions/coc-6 self-introduction metadata
#[test]
fn test_self_introduction() {
    let (id, name, icon_svg, description) = coc_6::self_introduction_info();
    assert_eq!(id, "coc-6");
    assert_eq!(name, "クトゥルフ神話TRPG (第6版)");
    assert!(icon_svg.starts_with("<svg") && icon_svg.ends_with("</svg>"));
    assert_eq!(
        description,
        "クトゥルフ神話TRPG第6版の記法や処理のための公式拡張機能です。"
    );

    assert_eq!(coc_6::abi::alhalte_abi_version(), 2);

    let bytes = coc_6::abi::encode_self_introduction();
    let decoded: (String, String, String, String) = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(decoded.0, id);
    assert_eq!(decoded.1, name);
    assert_eq!(decoded.2, icon_svg);
    assert_eq!(decoded.3, description);

    let packed = coc_6::abi::self_introduction();
    assert_ne!(packed, 0);
    let len = (packed & 0xffff_ffff) as usize;
    assert_eq!(len, bytes.len());
}
