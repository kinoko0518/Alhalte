//! Comprehensive tests for Generast AST rendering covering all 23 labels.
//!
//! // spec: alhalte-saignant/generast rendering

use alhalte_saignant::{NoopCharacterResolver, render_generast, render_generast_tin};
use generast::{Dice, DiceExpr, Generast, JudgementData, Rational, diceroll_tin, judgement_tin};

#[test]
fn test_render_string() {
    // spec: alhalte-saignant/generast string
    let ast = Generast::string("<Hello & Welcome!>");
    let html = render_generast(&ast, &NoopCharacterResolver);
    assert_eq!(html, "&lt;Hello &amp; Welcome!&gt;");
}

#[test]
fn test_render_bold() {
    // spec: alhalte-saignant/generast style/bold
    let ast = Generast::bold(vec![Generast::string("Important")]);
    let html = render_generast(&ast, &NoopCharacterResolver);
    assert_eq!(html, "<strong class=\"saignant-bold\">Important</strong>");
}

#[test]
fn test_render_italic() {
    // spec: alhalte-saignant/generast style/italic
    let ast = Generast::italic(vec![Generast::string("Whisper")]);
    let html = render_generast(&ast, &NoopCharacterResolver);
    assert_eq!(html, "<em class=\"saignant-italic\">Whisper</em>");
}

#[test]
fn test_render_headings_h1_to_h4() {
    // spec: alhalte-saignant/generast style/h1..h4
    let h1 = Generast::h1(vec![Generast::string("Chapter 1")]);
    assert_eq!(
        render_generast(&h1, &NoopCharacterResolver),
        "<h1 class=\"saignant-h1\">Chapter 1</h1>"
    );

    let h2 = Generast::h2(vec![Generast::string("Section 2")]);
    assert_eq!(
        render_generast(&h2, &NoopCharacterResolver),
        "<h2 class=\"saignant-h2\">Section 2</h2>"
    );

    let h3 = Generast::h3(vec![Generast::string("Topic 3")]);
    assert_eq!(
        render_generast(&h3, &NoopCharacterResolver),
        "<h3 class=\"saignant-h3\">Topic 3</h3>"
    );

    let h4 = Generast::h4(vec![Generast::string("Detail 4")]);
    assert_eq!(
        render_generast(&h4, &NoopCharacterResolver),
        "<h4 class=\"saignant-h4\">Detail 4</h4>"
    );
}

#[test]
fn test_render_hidden_spoiler() {
    // spec: alhalte-saignant/generast style/hidden
    let ast = Generast::hidden(vec![Generast::string("Secret Boss HP: 100")]);
    let html = render_generast(&ast, &NoopCharacterResolver);
    assert_eq!(
        html,
        "<span class=\"saignant-hidden\" data-spoiler=\"true\">Secret Boss HP: 100</span>"
    );
}

#[test]
fn test_render_text_color() {
    // spec: alhalte-saignant/generast style/text-color
    let safe = Generast::text_color("#ff00aa", vec![Generast::string("Vibrant")]);
    assert_eq!(
        render_generast(&safe, &NoopCharacterResolver),
        "<span class=\"saignant-text-color\" style=\"color: #ff00aa;\">Vibrant</span>"
    );

    // CSS injection attempt sanitized
    let attack = Generast::text_color(
        "red; background: url('evil.png')",
        vec![Generast::string("Sanitized")],
    );
    assert_eq!(
        render_generast(&attack, &NoopCharacterResolver),
        "<span class=\"saignant-text-color\">Sanitized</span>"
    );
}

#[test]
fn test_render_pill() {
    // spec: alhalte-saignant/generast style/pill
    let safe = Generast::pill("#eaddff", vec![Generast::string("Buff")]);
    assert_eq!(
        render_generast(&safe, &NoopCharacterResolver),
        "<span class=\"saignant-pill\" style=\"background-color: #eaddff;\">Buff</span>"
    );

    // Malicious pill background sanitized
    let attack = Generast::pill(
        "blue\" onclick=\"alert(1)",
        vec![Generast::string("Attack")],
    );
    assert_eq!(
        render_generast(&attack, &NoopCharacterResolver),
        "<span class=\"saignant-pill\">Attack</span>"
    );
}

#[test]
fn test_render_ruby() {
    // spec: alhalte-saignant/generast style/ruby
    let ast = Generast::ruby("あるはるて", vec![Generast::string("宴")]);
    let html = render_generast(&ast, &NoopCharacterResolver);
    assert_eq!(
        html,
        "<ruby class=\"saignant-ruby\">宴<rt class=\"saignant-rt\">あるはるて</rt></ruby>"
    );
}

#[test]
fn test_render_boten() {
    // spec: alhalte-saignant/generast style/boten
    let ast = Generast::boten(vec![Generast::string("強調")]);
    let html = render_generast(&ast, &NoopCharacterResolver);
    assert_eq!(html, "<span class=\"saignant-boten\">強調</span>");
}

#[test]
fn test_render_indent() {
    // spec: alhalte-saignant/generast style/indent
    let ast = Generast::indent(2, vec![Generast::string("Indented text")]);
    let html = render_generast(&ast, &NoopCharacterResolver);
    assert_eq!(
        html,
        "<div class=\"saignant-indent\" style=\"--saignant-indent: 2em;\">Indented text</div>"
    );
}

#[test]
fn test_render_hanging() {
    // spec: alhalte-saignant/generast style/hanging
    let ast = Generast::hanging(1, 3, vec![Generast::string("Hanging indent text")]).unwrap();
    let html = render_generast(&ast, &NoopCharacterResolver);
    assert_eq!(
        html,
        "<div class=\"saignant-hanging\" style=\"--saignant-hanging-indent: 1em; --saignant-hanging-wrap: 3em;\">Hanging indent text</div>"
    );
}

#[test]
fn test_render_grounded() {
    // spec: alhalte-saignant/generast style/grounded
    let ast = Generast::grounded(vec![Generast::string("Bottom text")]);
    let html = render_generast(&ast, &NoopCharacterResolver);
    assert_eq!(html, "<div class=\"saignant-grounded\">Bottom text</div>");
}

#[test]
fn test_render_kerning() {
    // spec: alhalte-saignant/generast style/kerning
    let ast = Generast::kerning(20, vec![Generast::string("20-character auto wrap line")]);
    let html = render_generast(&ast, &NoopCharacterResolver);
    assert_eq!(
        html,
        "<div class=\"saignant-kerning\" style=\"--saignant-kerning-chars: 20;\">20-character auto wrap line</div>"
    );
}

#[test]
fn test_render_h_in_v() {
    // spec: alhalte-saignant/generast style/h-in-v
    let ast = Generast::h_in_v(vec![Generast::string("42")]);
    let html = render_generast(&ast, &NoopCharacterResolver);
    assert_eq!(html, "<span class=\"saignant-h-in-v\">42</span>");
}

#[test]
fn test_render_diceroll_deterministic() {
    // spec: alhalte-saignant/generast diceroll
    let seed = [7u8; 32];
    let expr = DiceExpr::Take(Box::new(DiceExpr::Dice(Dice::new(6))));
    let tin = diceroll_tin(seed, expr).unwrap();
    let ast = Generast::element(tin);

    let html = render_generast(&ast, &NoopCharacterResolver);
    assert!(html.starts_with("<span class=\"saignant-diceroll\">"));
    assert!(html.contains("data-faces=\"6\""));
    assert!(html.contains("class=\"saignant-diceroll-result\""));
    assert!(html.ends_with("</span>"));
}

#[test]
fn test_render_judgement_degrees() {
    // spec: alhalte-saignant/generast judgement
    let seed = [1u8; 32];
    let roll_tin = diceroll_tin(seed, DiceExpr::Integer(50)).unwrap();

    // Success (degree = 1)
    let success = JudgementData {
        degree: 1,
        level_name: "Success".to_string(),
        subject: Some("Stealth".to_string()),
        target: Some(Rational::from_integer(60)),
        roll: Generast::element(roll_tin.clone()),
    };
    let success_tin = judgement_tin(&success).unwrap();
    let success_html = render_generast(&Generast::element(success_tin), &NoopCharacterResolver);
    assert!(success_html.contains("saignant-judgement--success"));
    assert!(success_html.contains("data-degree=\"1\""));
    assert!(success_html.contains("Stealth"));
    assert!(success_html.contains("&lt;=60"));
    assert!(success_html.contains("Success"));

    // Failure (degree = -1)
    let failure = JudgementData {
        degree: -1,
        level_name: "Failure".to_string(),
        subject: None,
        target: None,
        roll: Generast::element(roll_tin.clone()),
    };
    let failure_tin = judgement_tin(&failure).unwrap();
    let failure_html = render_generast(&Generast::element(failure_tin), &NoopCharacterResolver);
    assert!(failure_html.contains("saignant-judgement--failure"));
    assert!(failure_html.contains("data-degree=\"-1\""));
    assert!(failure_html.contains("Failure"));

    // Neutral (degree = 0)
    let neutral = JudgementData {
        degree: 0,
        level_name: "Tied".to_string(),
        subject: Some("Contest".to_string()),
        target: None,
        roll: Generast::element(roll_tin),
    };
    let neutral_tin = judgement_tin(&neutral).unwrap();
    let neutral_html = render_generast(&Generast::element(neutral_tin), &NoopCharacterResolver);
    assert!(neutral_html.contains("saignant-judgement--neutral"));
    assert!(neutral_html.contains("data-degree=\"0\""));
    assert!(neutral_html.contains("Tied"));
}

#[test]
fn test_render_character_id_mention() {
    // spec: alhalte-saignant/generast character-id
    let ast = Generast::character_id(42);

    // Resolved
    let resolver = |id: u64| {
        if id == 42 {
            Some("Bob".to_string())
        } else {
            None
        }
    };
    let html = render_generast(&ast, &resolver);
    assert_eq!(
        html,
        "<span class=\"saignant-mention\" data-character-id=\"42\">@Bob</span>"
    );

    // Fallback to Anonymous
    let html_fallback = render_generast(&ast, &NoopCharacterResolver);
    assert_eq!(
        html_fallback,
        "<span class=\"saignant-mention\" data-character-id=\"42\">@Anonymous</span>"
    );
}

#[test]
fn test_render_fluct() {
    // spec: alhalte-saignant/generast fluct
    let ast_pos = Generast::fluct(100, "HP", 5).unwrap();
    let resolver = |id: u64| {
        if id == 100 {
            Some("Hero".to_string())
        } else {
            None
        }
    };
    let html_pos = render_generast(&ast_pos, &resolver);
    assert_eq!(
        html_pos,
        "<span class=\"saignant-fluct\" data-character-id=\"100\">Hero.HP+5</span>"
    );

    let ast_neg = Generast::fluct(100, "MP", -12).unwrap();
    let html_neg = render_generast(&ast_neg, &resolver);
    assert_eq!(
        html_neg,
        "<span class=\"saignant-fluct\" data-character-id=\"100\">Hero.MP-12</span>"
    );
}

#[test]
fn test_render_assign() {
    // spec: alhalte-saignant/generast assign
    let ast = Generast::assign(100, "Sanity", 80).unwrap();
    let resolver = |id: u64| {
        if id == 100 {
            Some("Hero".to_string())
        } else {
            None
        }
    };
    let html = render_generast(&ast, &resolver);
    assert_eq!(
        html,
        "<span class=\"saignant-assign\" data-character-id=\"100\">Hero.Sanity=80</span>"
    );
}

#[test]
fn test_render_syntax_error() {
    // spec: alhalte-saignant/generast syntax-error
    let ast = Generast::syntax_error("Unexpected token in roll command");
    let html = render_generast(&ast, &NoopCharacterResolver);
    assert_eq!(
        html,
        "<span class=\"saignant-syntax-error\">Unexpected token in roll command</span>"
    );
}

#[test]
fn test_render_generast_tin_roundtrip() {
    // spec: alhalte-saignant/generast decode and render from Tin
    let ast = Generast::bold(vec![Generast::string("Wire Test")]);
    let tin = ast.to_tin().unwrap();
    let html = render_generast_tin(&tin, &NoopCharacterResolver).unwrap();
    assert_eq!(html, "<strong class=\"saignant-bold\">Wire Test</strong>");
}
