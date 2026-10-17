//! Tests for CSS category and layer separation, and Material 3 Expressive compliance.
//!
//! // spec: alhalte-saignant/css category and structural/expressive split

use alhalte_saignant::{
    CssCategory, CssLayer, category_css, expressive_css, full_css, structural_css,
};

#[test]
fn test_css_categories_and_layers() {
    // spec: alhalte-saignant/css structural and expressive split
    let categories = [
        CssCategory::Log,
        CssCategory::Typography,
        CssCategory::RubyBoten,
        CssCategory::Layout,
        CssCategory::DiceJudgement,
        CssCategory::CharacterStatus,
        CssCategory::PillSpoiler,
        CssCategory::Error,
    ];

    for cat in categories {
        let structural = category_css(cat, CssLayer::Structural);
        assert!(
            !structural.is_empty(),
            "Structural CSS for {:?} should not be empty",
            cat
        );

        let expressive = category_css(cat, CssLayer::Expressive);
        assert!(
            !expressive.is_empty(),
            "Expressive CSS for {:?} should not be empty",
            cat
        );
    }
}

#[test]
fn test_tokens_css() {
    // spec: alhalte-saignant/css tokens
    assert_eq!(category_css(CssCategory::Tokens, CssLayer::Structural), "");

    let tokens = category_css(CssCategory::Tokens, CssLayer::Expressive);
    assert!(!tokens.is_empty());
    assert!(tokens.contains("--md-sys-color-primary"));
    assert!(tokens.contains("--md-sys-color-surface"));
    assert!(tokens.contains("--md-sys-shape-corner-medium"));
    assert!(tokens.contains("--md-sys-elevation-1"));
}

#[test]
fn test_structural_css_integrity() {
    // spec: alhalte-saignant/css structural CSS guarantees
    let css = structural_css();
    // Ruby & boten structural properties
    assert!(css.contains("ruby-position: over"));
    assert!(css.contains("text-emphasis: filled circle"));
    // Layout structural properties
    assert!(css.contains("--saignant-indent"));
    assert!(css.contains("--saignant-hanging-indent"));
    assert!(css.contains("--saignant-hanging-wrap"));
    assert!(css.contains("text-combine-upright: all"));
    assert!(css.contains("--saignant-kerning-chars"));
    // Log entry flex structure
    assert!(css.contains(".saignant-log-entry"));
    assert!(css.contains(".saignant-log-body"));
}

#[test]
fn test_expressive_css_material3() {
    // spec: alhalte-saignant/css expressive Material 3 styling
    let css = expressive_css();
    // Material 3 color variables
    assert!(css.contains("var(--md-sys-color-primary"));
    assert!(css.contains("var(--md-sys-color-primary-container"));
    assert!(css.contains("var(--md-sys-color-error-container"));
    // Rounded corners
    assert!(css.contains("var(--md-sys-shape-corner-small"));
    assert!(css.contains("var(--md-sys-shape-corner-medium"));
    assert!(css.contains("var(--md-sys-shape-corner-full"));
    // Judgement badges
    assert!(css.contains(".saignant-judgement--success"));
    assert!(css.contains(".saignant-judgement--failure"));
    assert!(css.contains(".saignant-judgement--neutral"));
}

#[test]
fn test_full_css_combination() {
    // spec: alhalte-saignant/css full_css
    let full = full_css();
    assert!(full.contains("ruby-position: over")); // from structural
    assert!(full.contains("--md-sys-color-primary")); // from expressive tokens
    assert!(full.len() > structural_css().len());
    assert!(full.len() > expressive_css().len());
}
