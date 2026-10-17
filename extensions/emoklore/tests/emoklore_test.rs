//! Tests for emoklore extension.

use emoklore::abi::alhalte_abi_version;
use emoklore::parser::judge_emoklore;
use emoklore::process_ast;
use generast::{
    DiceExpr, Generast, Rational, decode_ext_io, diceroll_tin, encode_ext_io, extract_diceroll,
    extract_judgement,
};

// spec: extensions/emoklore degree and judgment rules
#[test]
fn test_emoklore_judgement_degrees() {
    let target = 5;

    // 1 gives +2 (critical)
    // 2..=5 gives +1 (success)
    // 6..=9 gives +0 (failure)
    // 10 gives -1 (fumble)

    // Fumble: total <= -1
    assert_eq!(
        judge_emoklore(&[10], target),
        (-1, "ファンブル".to_string())
    );
    assert_eq!(
        judge_emoklore(&[10, 10], target),
        (-2, "ファンブル".to_string())
    );
    assert_eq!(
        judge_emoklore(&[10, 8], target),
        (-1, "ファンブル".to_string())
    );

    // Failure: total == 0
    assert_eq!(judge_emoklore(&[6], target), (0, "失敗".to_string()));
    assert_eq!(
        judge_emoklore(&[1, 10, 10], target),
        (0, "失敗".to_string())
    );

    // Single: total == 1
    assert_eq!(judge_emoklore(&[2], target), (1, "シングル".to_string()));
    assert_eq!(judge_emoklore(&[5], target), (1, "シングル".to_string()));
    assert_eq!(
        judge_emoklore(&[1, 10], target),
        (1, "シングル".to_string())
    );

    // Double: total == 2
    assert_eq!(judge_emoklore(&[1], target), (2, "ダブル".to_string()));
    assert_eq!(judge_emoklore(&[2, 3], target), (2, "ダブル".to_string()));

    // Triple: total == 3
    assert_eq!(judge_emoklore(&[1, 2], target), (3, "トリプル".to_string()));
    assert_eq!(
        judge_emoklore(&[2, 3, 4], target),
        (3, "トリプル".to_string())
    );

    // Miracle: total 4..=9
    assert_eq!(judge_emoklore(&[1, 1], target), (4, "ミラクル".to_string()));
    assert_eq!(
        judge_emoklore(&[1, 1, 1, 1], target),
        (8, "ミラクル".to_string())
    );

    // Catastrophe: total >= 10
    assert_eq!(
        judge_emoklore(&[1, 1, 1, 1, 1], target),
        (10, "カタストロフ".to_string())
    );
    assert_eq!(
        judge_emoklore(&[1, 1, 1, 1, 1, 1], target),
        (12, "カタストロフ".to_string())
    );
}

// spec: extensions/emoklore parse string command {X}DM<=Y〈技能〉
#[test]
fn test_emoklore_parse_string() {
    let speaker_id = 99;

    // {2}DM<=5〈調査〉
    let ast1 = process_ast(Generast::string("{2}DM<=5〈調査〉"), speaker_id);
    let tin1 = match ast1 {
        Generast::Element(t) => t,
        other => panic!("expected Element, got {:?}", other),
    };
    let judge1 = extract_judgement(&tin1).unwrap();
    assert_eq!(judge1.subject.as_deref(), Some("調査"));
    assert_eq!(judge1.target, Some(Rational::from_integer(5)));
    assert!((-2..=4).contains(&judge1.degree));

    // Inner roll must be a 2D10 diceroll
    let roll_tin1 = match judge1.roll {
        Generast::Element(t) => t,
        other => panic!("expected roll Element, got {:?}", other),
    };
    let dice1 = extract_diceroll(&roll_tin1).unwrap();
    assert!(matches!(
        dice1.expr,
        DiceExpr::RollN { count, .. } if *count == DiceExpr::Integer(2)
    ));

    // 3DM<=6〈共鳴〉
    let ast2 = process_ast(Generast::string("3DM<=6〈共鳴〉"), speaker_id);
    let tin2 = match ast2 {
        Generast::Element(t) => t,
        other => panic!("expected Element, got {:?}", other),
    };
    let judge2 = extract_judgement(&tin2).unwrap();
    assert_eq!(judge2.subject.as_deref(), Some("共鳴"));
    assert_eq!(judge2.target, Some(Rational::from_integer(6)));

    // DM<=4
    let ast3 = process_ast(Generast::string("DM<=4"), speaker_id);
    let tin3 = match ast3 {
        Generast::Element(t) => t,
        other => panic!("expected Element, got {:?}", other),
    };
    let judge3 = extract_judgement(&tin3).unwrap();
    assert_eq!(judge3.subject, None);
    assert_eq!(judge3.target, Some(Rational::from_integer(4)));

    // Trailing text
    let ast4 = process_ast(Generast::string("{2}DM<=5〈調査〉 結果待ち"), speaker_id);
    match ast4 {
        Generast::Nested(nested) => {
            assert_eq!(nested.contents.len(), 2);
            assert!(matches!(&nested.contents[0], Generast::Element(_)));
            assert_eq!(nested.contents[1], Generast::string(" 結果待ち"));
        }
        other => panic!("expected Nested, got {:?}", other),
    }
}

// spec: extensions/emoklore combine diceroll and suffix bracket
#[test]
fn test_emoklore_combine_diceroll_and_suffix() {
    let speaker_id = 7;
    let seed = [12u8; 32];
    let dice_expr = DiceExpr::RollN {
        dice: Box::new(DiceExpr::Dice(generast::types::Dice::new(10))),
        count: Box::new(DiceExpr::Integer(2)),
    };
    let roll_tin = diceroll_tin(seed, dice_expr).unwrap();

    let input_ast = Generast::nested(
        tin::Tin::need_not_know(Vec::new()),
        vec![
            Generast::element(roll_tin),
            Generast::string("<=5〈直感〉 ロール完了"),
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
    assert_eq!(judge.subject.as_deref(), Some("直感"));
    assert_eq!(judge.target, Some(Rational::from_integer(5)));
    assert_eq!(nested.contents[1], Generast::string(" ロール完了"));
}

// spec: extensions/emoklore ABI roundtrip
#[test]
fn test_emoklore_abi_exports() {
    assert_eq!(alhalte_abi_version(), 1);

    let speaker_id = 777u64;
    let input_ast = Generast::string("{1}DM<=7〈観察眼〉");
    let input_tin = encode_ext_io(&input_ast, speaker_id).unwrap();
    let input_bytes = input_tin.to_plaintext();

    let out_bytes = emoklore::abi::process_raw_bytes(&input_bytes, process_ast);
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
    assert_eq!(judge.subject.as_deref(), Some("観察眼"));
}

// spec: extensions/emoklore self-introduction metadata
#[test]
fn test_self_introduction() {
    let (name, icon_svg, description) = emoklore::self_introduction_info();
    assert_eq!(name, "エモクロアTRPG");
    assert!(icon_svg.starts_with("<svg") && icon_svg.ends_with("</svg>"));
    assert_eq!(
        description,
        "エモクロアTRPGのxDM<=y判定（共鳴判定・成功数カウント・ダブル/トリプル等）を提供する公式拡張機能"
    );

    let bytes = emoklore::abi::encode_self_introduction();
    let decoded: (String, String, String) = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(decoded.0, name);
    assert_eq!(decoded.1, icon_svg);
    assert_eq!(decoded.2, description);

    let packed = emoklore::abi::self_introduction();
    assert_ne!(packed, 0);
    let len = (packed & 0xffff_ffff) as usize;
    assert_eq!(len, bytes.len());
}
