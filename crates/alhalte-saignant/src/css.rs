//! Stylesheet management: category separation, Structural/Expressive layers, and Material 3 Expressive styling.

/// Categories of styling in the Alhalte Saignant design system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CssCategory {
    /// Material 3 Expressive system tokens (colors, shapes, elevation, motion).
    Tokens,
    /// Chat log entry card, avatar, header, and content layout.
    Log,
    /// Headings, bold, italic, and base typography scale.
    Typography,
    /// Ruby annotations and boten emphasis dots.
    RubyBoten,
    /// Indent, hanging indent, grounded alignment, kerning, and horizontal-in-vertical layout.
    Layout,
    /// Dice roll results, trace tokens, and judgement outcome badges.
    DiceJudgement,
    /// Character mentions, status fluctuations, and assignments.
    CharacterStatus,
    /// Custom color pills and spoiler hidden text.
    PillSpoiler,
    /// Syntax errors and dice evaluation error indicators.
    Error,
}

/// Architectural layer of styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CssLayer {
    /// Structural CSS ensuring geometric and semantic correctness.
    Structural,
    /// Expressive CSS providing Material 3 visual appearance, colors, and motion.
    Expressive,
}

// Embedded Structural CSS
const STRUCTURAL_LOG: &str = include_str!("../css/structural/log.css");
const STRUCTURAL_TYPOGRAPHY: &str = include_str!("../css/structural/typography.css");
const STRUCTURAL_RUBY_BOTEN: &str = include_str!("../css/structural/ruby_boten.css");
const STRUCTURAL_LAYOUT: &str = include_str!("../css/structural/layout.css");
const STRUCTURAL_DICE_JUDGEMENT: &str = include_str!("../css/structural/dice_judgement.css");
const STRUCTURAL_CHARACTER_STATUS: &str = include_str!("../css/structural/character_status.css");
const STRUCTURAL_PILL_SPOILER: &str = include_str!("../css/structural/pill_spoiler.css");
const STRUCTURAL_ERROR: &str = include_str!("../css/structural/error.css");

// Embedded Expressive CSS
const EXPRESSIVE_TOKENS: &str = include_str!("../css/expressive/tokens.css");
const EXPRESSIVE_LOG: &str = include_str!("../css/expressive/log.css");
const EXPRESSIVE_TYPOGRAPHY: &str = include_str!("../css/expressive/typography.css");
const EXPRESSIVE_RUBY_BOTEN: &str = include_str!("../css/expressive/ruby_boten.css");
const EXPRESSIVE_LAYOUT: &str = include_str!("../css/expressive/layout.css");
const EXPRESSIVE_DICE_JUDGEMENT: &str = include_str!("../css/expressive/dice_judgement.css");
const EXPRESSIVE_CHARACTER_STATUS: &str = include_str!("../css/expressive/character_status.css");
const EXPRESSIVE_PILL_SPOILER: &str = include_str!("../css/expressive/pill_spoiler.css");
const EXPRESSIVE_ERROR: &str = include_str!("../css/expressive/error.css");

/// Returns the static CSS string for the specified category and architectural layer.
///
/// For `CssCategory::Tokens` under `CssLayer::Structural`, an empty string is returned
/// because design tokens belong strictly to the expressive layer.
pub fn category_css(category: CssCategory, layer: CssLayer) -> &'static str {
    match (category, layer) {
        (CssCategory::Tokens, CssLayer::Structural) => "",
        (CssCategory::Tokens, CssLayer::Expressive) => EXPRESSIVE_TOKENS,

        (CssCategory::Log, CssLayer::Structural) => STRUCTURAL_LOG,
        (CssCategory::Log, CssLayer::Expressive) => EXPRESSIVE_LOG,

        (CssCategory::Typography, CssLayer::Structural) => STRUCTURAL_TYPOGRAPHY,
        (CssCategory::Typography, CssLayer::Expressive) => EXPRESSIVE_TYPOGRAPHY,

        (CssCategory::RubyBoten, CssLayer::Structural) => STRUCTURAL_RUBY_BOTEN,
        (CssCategory::RubyBoten, CssLayer::Expressive) => EXPRESSIVE_RUBY_BOTEN,

        (CssCategory::Layout, CssLayer::Structural) => STRUCTURAL_LAYOUT,
        (CssCategory::Layout, CssLayer::Expressive) => EXPRESSIVE_LAYOUT,

        (CssCategory::DiceJudgement, CssLayer::Structural) => STRUCTURAL_DICE_JUDGEMENT,
        (CssCategory::DiceJudgement, CssLayer::Expressive) => EXPRESSIVE_DICE_JUDGEMENT,

        (CssCategory::CharacterStatus, CssLayer::Structural) => STRUCTURAL_CHARACTER_STATUS,
        (CssCategory::CharacterStatus, CssLayer::Expressive) => EXPRESSIVE_CHARACTER_STATUS,

        (CssCategory::PillSpoiler, CssLayer::Structural) => STRUCTURAL_PILL_SPOILER,
        (CssCategory::PillSpoiler, CssLayer::Expressive) => EXPRESSIVE_PILL_SPOILER,

        (CssCategory::Error, CssLayer::Structural) => STRUCTURAL_ERROR,
        (CssCategory::Error, CssLayer::Expressive) => EXPRESSIVE_ERROR,
    }
}

/// Returns all Structural CSS stylesheets concatenated together.
pub fn structural_css() -> String {
    let mut out = String::with_capacity(4096);
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
        let css = category_css(cat, CssLayer::Structural);
        if !css.is_empty() {
            out.push_str(css);
            out.push('\n');
        }
    }
    out
}

/// Returns all Expressive CSS stylesheets (including Material 3 tokens) concatenated together.
pub fn expressive_css() -> String {
    let mut out = String::with_capacity(8192);
    let categories = [
        CssCategory::Tokens,
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
        let css = category_css(cat, CssLayer::Expressive);
        if !css.is_empty() {
            out.push_str(css);
            out.push('\n');
        }
    }
    out
}

/// Returns the complete combined stylesheet containing both Structural and Expressive layers.
pub fn full_css() -> String {
    let mut out = String::with_capacity(12288);
    out.push_str(&structural_css());
    out.push('\n');
    out.push_str(&expressive_css());
    out
}
