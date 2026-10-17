//! Tests for coc-7 extension.

use coc_7::abi::alhalte_abi_version;
use coc_7::parser::judge_coc7;
use coc_7::process_ast;
use generast::{
    DiceExpr, Generast, Rational, decode_ext_io, diceroll_tin, encode_ext_io, extract_judgement,
};

// spec: extensions/coc-7 degree and judgment rules
#[test]
fn test_coc7_judgement_degrees() {
    // target = 60 (>= 50)
    // roll 1 => クリティカル, degree: 4
    assert_eq!(judge_coc7(1, 60), (4, "クリティカル".to_string()));
    // roll <= 60 / 5 = 12 => イクストリーム成功, degree: 3
    assert_eq!(judge_coc7(2, 60), (3, "イクストリーム成功".to_string()));
    assert_eq!(judge_coc7(12, 60), (3, "イクストリーム成功".to_string()));
    // roll <= 60 / 2 = 30 => ハード成功, degree: 2
    assert_eq!(judge_coc7(13, 60), (2, "ハード成功".to_string()));
    assert_eq!(judge_coc7(30, 60), (2, "ハード成功".to_string()));
    // roll <= 60 => レギュラー成功, degree: 1
    assert_eq!(judge_coc7(31, 60), (1, "レギュラー成功".to_string()));
    assert_eq!(judge_coc7(60, 60), (1, "レギュラー成功".to_string()));
    // roll > 60 and != 100 => 失敗, degree: -1
    assert_eq!(judge_coc7(61, 60), (-1, "失敗".to_string()));
    assert_eq!(judge_coc7(99, 60), (-1, "失敗".to_string()));
    // roll 100 => ファンブル, degree: -2
    assert_eq!(judge_coc7(100, 60), (-2, "ファンブル".to_string()));

    // target = 40 (< 50)
    // 96..=100 => ファンブル, degree: -2
    assert_eq!(judge_coc7(96, 40), (-2, "ファンブル".to_string()));
    assert_eq!(judge_coc7(100, 40), (-2, "ファンブル".to_string()));
    // 41..=95 => 失敗, degree: -1
    assert_eq!(judge_coc7(95, 40), (-1, "失敗".to_string()));
    assert_eq!(judge_coc7(41, 40), (-1, "失敗".to_string()));
    // <= 40 => レギュラー成功
    assert_eq!(judge_coc7(40, 40), (1, "レギュラー成功".to_string()));
    // <= 20 => ハード成功
    assert_eq!(judge_coc7(20, 40), (2, "ハード成功".to_string()));
    // <= 8 => イクストリーム成功
    assert_eq!(judge_coc7(8, 40), (3, "イクストリーム成功".to_string()));
    // roll 1 => クリティカル
    assert_eq!(judge_coc7(1, 40), (4, "クリティカル".to_string()));
}

// spec: extensions/coc-7 parse string command CC<=70【目星】 and bonus/penalty dice
#[test]
fn test_coc7_parse_string() {
    let speaker_id = 42;

    // Normal roll: CC<=70【目星】
    let ast = process_ast(Generast::string("CC<=70【目星】"), speaker_id);
    let tin = match ast {
        Generast::Element(t) => t,
        other => panic!("expected Element, got {:?}", other),
    };
    let judge = extract_judgement(&tin).unwrap();
    assert_eq!(judge.subject.as_deref(), Some("目星"));
    assert_eq!(judge.target, Some(Rational::from_integer(70)));
    assert!([-2, -1, 1, 2, 3, 4].contains(&judge.degree));

    // Bonus die: CCB<=70【目星】
    let ast_bonus = process_ast(Generast::string("CCB<=70【目星】"), speaker_id);
    let tin_b = match ast_bonus {
        Generast::Element(t) => t,
        other => panic!("expected Element, got {:?}", other),
    };
    let judge_b = extract_judgement(&tin_b).unwrap();
    assert_eq!(judge_b.subject.as_deref(), Some("目星"));
    assert_eq!(judge_b.target, Some(Rational::from_integer(70)));
    assert!([-2, -1, 1, 2, 3, 4].contains(&judge_b.degree));

    // Penalty die: CCP<=70【目星】
    let ast_pen = process_ast(Generast::string("CCP<=70【目星】"), speaker_id);
    let tin_p = match ast_pen {
        Generast::Element(t) => t,
        other => panic!("expected Element, got {:?}", other),
    };
    let judge_p = extract_judgement(&tin_p).unwrap();
    assert_eq!(judge_p.subject.as_deref(), Some("目星"));
    assert_eq!(judge_p.target, Some(Rational::from_integer(70)));
    assert!([-2, -1, 1, 2, 3, 4].contains(&judge_p.degree));

    // Trailing text
    let ast_trail = process_ast(Generast::string("CC<=70【目星】 合格！"), speaker_id);
    match ast_trail {
        Generast::Nested(nested) => {
            assert_eq!(nested.contents.len(), 2);
            assert!(matches!(&nested.contents[0], Generast::Element(_)));
            assert_eq!(nested.contents[1], Generast::string(" 合格！"));
        }
        other => panic!("expected Nested, got {:?}", other),
    }
}

// spec: extensions/coc-7 combine diceroll and suffix bracket
#[test]
fn test_coc7_combine_diceroll_and_suffix() {
    let speaker_id = 100;
    let seed = [77u8; 32];
    let dice_expr = DiceExpr::Le(
        Box::new(DiceExpr::Sum(Box::new(DiceExpr::RollN {
            dice: Box::new(DiceExpr::Dice(generast::types::Dice::new(100))),
            count: Box::new(DiceExpr::Integer(1)),
        }))),
        Box::new(DiceExpr::Integer(50)),
    );
    let roll_tin = diceroll_tin(seed, dice_expr).unwrap();

    let input_ast = Generast::nested(
        tin::Tin::need_not_know(Vec::new()),
        vec![
            Generast::element(roll_tin),
            Generast::string("【隠密】 行動開始"),
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
    assert_eq!(judge.subject.as_deref(), Some("隠密"));
    assert_eq!(judge.target, Some(Rational::from_integer(50)));
    assert_eq!(nested.contents[1], Generast::string(" 行動開始"));
}

// spec: extensions/coc-7 ABI roundtrip
#[test]
fn test_coc7_abi_exports() {
    assert_eq!(alhalte_abi_version(), 1);

    let speaker_id = 888u64;
    let input_ast = Generast::string("CC<=65【心理学】");
    let input_tin = encode_ext_io(&input_ast, speaker_id).unwrap();
    let input_bytes = input_tin.to_plaintext();

    let out_bytes = coc_7::abi::process_raw_bytes(&input_bytes, process_ast);
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
    assert_eq!(judge.subject.as_deref(), Some("心理学"));
}
