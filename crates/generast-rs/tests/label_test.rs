//! Tests for all 23 label definitions, IDENT/LABEL constants, and typed helper functions.

use generast::*;
use tin::label_hash;

// spec: generast-rs/labels 23 label identifiers and hashes
#[test]
fn test_all_23_label_hashes() {
    let cases: [(&str, u64); 23] = [
        (IDENT_ALHALTE_GENERAST, LABEL_ALHALTE_GENERAST),
        (
            IDENT_ALHALTE_GENERAST_STYLE_BOLD,
            LABEL_ALHALTE_GENERAST_STYLE_BOLD,
        ),
        (
            IDENT_ALHALTE_GENERAST_STYLE_ITALIC,
            LABEL_ALHALTE_GENERAST_STYLE_ITALIC,
        ),
        (
            IDENT_ALHALTE_GENERAST_STYLE_H1,
            LABEL_ALHALTE_GENERAST_STYLE_H1,
        ),
        (
            IDENT_ALHALTE_GENERAST_STYLE_H2,
            LABEL_ALHALTE_GENERAST_STYLE_H2,
        ),
        (
            IDENT_ALHALTE_GENERAST_STYLE_H3,
            LABEL_ALHALTE_GENERAST_STYLE_H3,
        ),
        (
            IDENT_ALHALTE_GENERAST_STYLE_H4,
            LABEL_ALHALTE_GENERAST_STYLE_H4,
        ),
        (
            IDENT_ALHALTE_GENERAST_STYLE_HIDDEN,
            LABEL_ALHALTE_GENERAST_STYLE_HIDDEN,
        ),
        (
            IDENT_ALHALTE_GENERAST_STYLE_TEXT_COLOR,
            LABEL_ALHALTE_GENERAST_STYLE_TEXT_COLOR,
        ),
        (
            IDENT_ALHALTE_GENERAST_STYLE_PILL,
            LABEL_ALHALTE_GENERAST_STYLE_PILL,
        ),
        (
            IDENT_ALHALTE_GENERAST_STYLE_RUBY,
            LABEL_ALHALTE_GENERAST_STYLE_RUBY,
        ),
        (
            IDENT_ALHALTE_GENERAST_STYLE_BOTEN,
            LABEL_ALHALTE_GENERAST_STYLE_BOTEN,
        ),
        (
            IDENT_ALHALTE_GENERAST_STYLE_INDENT,
            LABEL_ALHALTE_GENERAST_STYLE_INDENT,
        ),
        (
            IDENT_ALHALTE_GENERAST_STYLE_HANGING,
            LABEL_ALHALTE_GENERAST_STYLE_HANGING,
        ),
        (
            IDENT_ALHALTE_GENERAST_STYLE_GROUNDED,
            LABEL_ALHALTE_GENERAST_STYLE_GROUNDED,
        ),
        (
            IDENT_ALHALTE_GENERAST_STYLE_KERNING,
            LABEL_ALHALTE_GENERAST_STYLE_KERNING,
        ),
        (
            IDENT_ALHALTE_GENERAST_STYLE_H_IN_V,
            LABEL_ALHALTE_GENERAST_STYLE_H_IN_V,
        ),
        (
            IDENT_ALHALTE_GENERAST_DICEROLL,
            LABEL_ALHALTE_GENERAST_DICEROLL,
        ),
        (IDENT_ALHALTE_JUDGEMENT, LABEL_ALHALTE_JUDGEMENT),
        (
            IDENT_ALHALTE_GENERAST_CHARACTER_ID,
            LABEL_ALHALTE_GENERAST_CHARACTER_ID,
        ),
        (IDENT_ALHALTE_GENERAST_FLUCT, LABEL_ALHALTE_GENERAST_FLUCT),
        (IDENT_ALHALTE_GENERAST_ASSIGN, LABEL_ALHALTE_GENERAST_ASSIGN),
        (
            IDENT_ALHALTE_GENERAST_SYNTAX_ERROR,
            LABEL_ALHALTE_GENERAST_SYNTAX_ERROR,
        ),
    ];

    for (ident, expected_label) in cases {
        assert_eq!(
            label_hash(ident),
            expected_label,
            "hash mismatch for identifier {ident}"
        );
    }
}

// spec: generast-rs/labels zero-byte style metadata helpers
#[test]
fn test_zero_byte_style_metas() {
    let bold = bold_meta();
    assert_eq!(bold.label, LABEL_ALHALTE_GENERAST_STYLE_BOLD);
    assert!(bold.payload.is_empty());
    assert!(is_bold_meta(&bold));

    let italic = italic_meta();
    assert_eq!(italic.label, LABEL_ALHALTE_GENERAST_STYLE_ITALIC);
    assert!(italic.payload.is_empty());
    assert!(is_italic_meta(&italic));

    let h1 = h1_meta();
    assert_eq!(h1.label, LABEL_ALHALTE_GENERAST_STYLE_H1);
    assert!(h1.payload.is_empty());
    assert!(is_h1_meta(&h1));

    let h2 = h2_meta();
    assert_eq!(h2.label, LABEL_ALHALTE_GENERAST_STYLE_H2);
    assert!(is_h2_meta(&h2));

    let h3 = h3_meta();
    assert_eq!(h3.label, LABEL_ALHALTE_GENERAST_STYLE_H3);
    assert!(is_h3_meta(&h3));

    let h4 = h4_meta();
    assert_eq!(h4.label, LABEL_ALHALTE_GENERAST_STYLE_H4);
    assert!(is_h4_meta(&h4));

    let hidden = hidden_meta();
    assert_eq!(hidden.label, LABEL_ALHALTE_GENERAST_STYLE_HIDDEN);
    assert!(is_hidden_meta(&hidden));

    let boten = boten_meta();
    assert_eq!(boten.label, LABEL_ALHALTE_GENERAST_STYLE_BOTEN);
    assert!(is_boten_meta(&boten));

    let grounded = grounded_meta();
    assert_eq!(grounded.label, LABEL_ALHALTE_GENERAST_STYLE_GROUNDED);
    assert!(is_grounded_meta(&grounded));

    let h_in_v = h_in_v_meta();
    assert_eq!(h_in_v.label, LABEL_ALHALTE_GENERAST_STYLE_H_IN_V);
    assert!(is_h_in_v_meta(&h_in_v));
}

// spec: generast-rs/labels text-color, pill, and ruby helpers
#[test]
fn test_string_payload_metas() {
    let color_tin = text_color_meta("#ff5500");
    assert_eq!(color_tin.label, LABEL_ALHALTE_GENERAST_STYLE_TEXT_COLOR);
    assert_eq!(extract_text_color(&color_tin).unwrap(), "#ff5500");

    let pill_tin = pill_meta("#00aaee");
    assert_eq!(pill_tin.label, LABEL_ALHALTE_GENERAST_STYLE_PILL);
    assert_eq!(extract_pill(&pill_tin).unwrap(), "#00aaee");

    let ruby_tin = ruby_meta("かんじ");
    assert_eq!(ruby_tin.label, LABEL_ALHALTE_GENERAST_STYLE_RUBY);
    assert_eq!(extract_ruby(&ruby_tin).unwrap(), "かんじ");
}

// spec: generast-rs/labels indent, hanging, and kerning helpers
#[test]
fn test_indent_hanging_kerning_metas() {
    let indent_tin = indent_meta(4);
    assert_eq!(indent_tin.label, LABEL_ALHALTE_GENERAST_STYLE_INDENT);
    assert_eq!(extract_indent(&indent_tin).unwrap(), 4);

    let hanging_tin = hanging_meta(2, 4).unwrap();
    assert_eq!(hanging_tin.label, LABEL_ALHALTE_GENERAST_STYLE_HANGING);
    assert_eq!(extract_hanging(&hanging_tin).unwrap(), (2, 4));

    let kerning_tin = kerning_meta(20);
    assert_eq!(kerning_tin.label, LABEL_ALHALTE_GENERAST_STYLE_KERNING);
    assert_eq!(extract_kerning(&kerning_tin).unwrap(), 20);
}

// spec: generast-rs/labels character-id mention helper
#[test]
fn test_character_id_label() {
    let cid = 100200300400u64;
    let tin = character_id_tin(cid);
    assert_eq!(tin.label, LABEL_ALHALTE_GENERAST_CHARACTER_ID);
    assert_eq!(extract_character_id(&tin).unwrap(), cid);
}

// spec: generast-rs/labels fluct and assign helpers
#[test]
fn test_fluct_and_assign_labels() {
    let fluct = fluct_tin(42, "HP", -5).unwrap();
    assert_eq!(fluct.label, LABEL_ALHALTE_GENERAST_FLUCT);
    let extracted_fluct = extract_fluct(&fluct).unwrap();
    assert_eq!(
        extracted_fluct,
        FluctData {
            character_id: 42,
            status: "HP".to_string(),
            delta: -5,
        }
    );

    let assign = assign_tin(42, "MP", 100).unwrap();
    assert_eq!(assign.label, LABEL_ALHALTE_GENERAST_ASSIGN);
    let extracted_assign = extract_assign(&assign).unwrap();
    assert_eq!(
        extracted_assign,
        AssignData {
            character_id: 42,
            status: "MP".to_string(),
            value: 100,
        }
    );
}

// spec: generast-rs/labels syntax-error helper
#[test]
fn test_syntax_error_label() {
    let err_tin = syntax_error_tin("unexpected token at column 5");
    assert_eq!(err_tin.label, LABEL_ALHALTE_GENERAST_SYNTAX_ERROR);
    let msg = extract_syntax_error(&err_tin).unwrap();
    assert_eq!(msg, "unexpected token at column 5");
}

// spec: generast-rs/labels diceroll helper
#[test]
fn test_diceroll_label() {
    let seed = [7u8; 32];
    let expr = DiceExpr::MakeDice(Box::new(DiceExpr::Integer(20)));
    let tin = diceroll_tin(seed, expr.clone()).unwrap();
    assert_eq!(tin.label, LABEL_ALHALTE_GENERAST_DICEROLL);

    let data = extract_diceroll(&tin).unwrap();
    assert_eq!(data.seed, seed);
    assert_eq!(data.expr, expr);
}

// spec: generast-rs/labels judgement helper
#[test]
fn test_judgement_label() {
    let judgement_data = JudgementData {
        degree: 1,
        level_name: "Success".to_string(),
        subject: Some("Stealth".to_string()),
        target: Some(Rational::from_integer(60)),
        roll: Generast::string("rolled 45"),
    };

    let tin = judgement_tin(&judgement_data).unwrap();
    assert_eq!(tin.label, LABEL_ALHALTE_JUDGEMENT);

    let extracted = extract_judgement(&tin).unwrap();
    assert_eq!(extracted, judgement_data);
}
