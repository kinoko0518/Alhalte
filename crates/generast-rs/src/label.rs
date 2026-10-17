//! Format identifier constants and typed helper functions for all 23 labels.

use tin::Tin;
use xxhash_rust::const_xxh64::xxh64;

use crate::dice::DiceExpr;
use crate::error::GenerastError;
use crate::types::{AssignData, DicerollData, FluctData, JudgementData};

/// Identifier for `alhalte/generast`.
pub const IDENT_ALHALTE_GENERAST: &str = "alhalte/generast";
/// Label hash for `alhalte/generast`.
pub const LABEL_ALHALTE_GENERAST: u64 = xxh64(IDENT_ALHALTE_GENERAST.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/bold`.
pub const IDENT_ALHALTE_GENERAST_STYLE_BOLD: &str = "alhalte/generast/style/bold";
/// Label hash for `alhalte/generast/style/bold`.
pub const LABEL_ALHALTE_GENERAST_STYLE_BOLD: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_BOLD.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/italic`.
pub const IDENT_ALHALTE_GENERAST_STYLE_ITALIC: &str = "alhalte/generast/style/italic";
/// Label hash for `alhalte/generast/style/italic`.
pub const LABEL_ALHALTE_GENERAST_STYLE_ITALIC: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_ITALIC.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/h1`.
pub const IDENT_ALHALTE_GENERAST_STYLE_H1: &str = "alhalte/generast/style/h1";
/// Label hash for `alhalte/generast/style/h1`.
pub const LABEL_ALHALTE_GENERAST_STYLE_H1: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_H1.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/h2`.
pub const IDENT_ALHALTE_GENERAST_STYLE_H2: &str = "alhalte/generast/style/h2";
/// Label hash for `alhalte/generast/style/h2`.
pub const LABEL_ALHALTE_GENERAST_STYLE_H2: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_H2.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/h3`.
pub const IDENT_ALHALTE_GENERAST_STYLE_H3: &str = "alhalte/generast/style/h3";
/// Label hash for `alhalte/generast/style/h3`.
pub const LABEL_ALHALTE_GENERAST_STYLE_H3: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_H3.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/h4`.
pub const IDENT_ALHALTE_GENERAST_STYLE_H4: &str = "alhalte/generast/style/h4";
/// Label hash for `alhalte/generast/style/h4`.
pub const LABEL_ALHALTE_GENERAST_STYLE_H4: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_H4.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/hidden`.
pub const IDENT_ALHALTE_GENERAST_STYLE_HIDDEN: &str = "alhalte/generast/style/hidden";
/// Label hash for `alhalte/generast/style/hidden`.
pub const LABEL_ALHALTE_GENERAST_STYLE_HIDDEN: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_HIDDEN.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/text-color`.
pub const IDENT_ALHALTE_GENERAST_STYLE_TEXT_COLOR: &str = "alhalte/generast/style/text-color";
/// Label hash for `alhalte/generast/style/text-color`.
pub const LABEL_ALHALTE_GENERAST_STYLE_TEXT_COLOR: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_TEXT_COLOR.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/pill`.
pub const IDENT_ALHALTE_GENERAST_STYLE_PILL: &str = "alhalte/generast/style/pill";
/// Label hash for `alhalte/generast/style/pill`.
pub const LABEL_ALHALTE_GENERAST_STYLE_PILL: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_PILL.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/ruby`.
pub const IDENT_ALHALTE_GENERAST_STYLE_RUBY: &str = "alhalte/generast/style/ruby";
/// Label hash for `alhalte/generast/style/ruby`.
pub const LABEL_ALHALTE_GENERAST_STYLE_RUBY: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_RUBY.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/boten`.
pub const IDENT_ALHALTE_GENERAST_STYLE_BOTEN: &str = "alhalte/generast/style/boten";
/// Label hash for `alhalte/generast/style/boten`.
pub const LABEL_ALHALTE_GENERAST_STYLE_BOTEN: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_BOTEN.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/indent`.
pub const IDENT_ALHALTE_GENERAST_STYLE_INDENT: &str = "alhalte/generast/style/indent";
/// Label hash for `alhalte/generast/style/indent`.
pub const LABEL_ALHALTE_GENERAST_STYLE_INDENT: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_INDENT.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/hanging`.
pub const IDENT_ALHALTE_GENERAST_STYLE_HANGING: &str = "alhalte/generast/style/hanging";
/// Label hash for `alhalte/generast/style/hanging`.
pub const LABEL_ALHALTE_GENERAST_STYLE_HANGING: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_HANGING.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/grounded`.
pub const IDENT_ALHALTE_GENERAST_STYLE_GROUNDED: &str = "alhalte/generast/style/grounded";
/// Label hash for `alhalte/generast/style/grounded`.
pub const LABEL_ALHALTE_GENERAST_STYLE_GROUNDED: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_GROUNDED.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/kerning`.
pub const IDENT_ALHALTE_GENERAST_STYLE_KERNING: &str = "alhalte/generast/style/kerning";
/// Label hash for `alhalte/generast/style/kerning`.
pub const LABEL_ALHALTE_GENERAST_STYLE_KERNING: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_KERNING.as_bytes(), 0);

/// Identifier for `alhalte/generast/style/h-in-v`.
pub const IDENT_ALHALTE_GENERAST_STYLE_H_IN_V: &str = "alhalte/generast/style/h-in-v";
/// Label hash for `alhalte/generast/style/h-in-v`.
pub const LABEL_ALHALTE_GENERAST_STYLE_H_IN_V: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_STYLE_H_IN_V.as_bytes(), 0);

/// Identifier for `alhalte/generast/diceroll`.
pub const IDENT_ALHALTE_GENERAST_DICEROLL: &str = "alhalte/generast/diceroll";
/// Label hash for `alhalte/generast/diceroll`.
pub const LABEL_ALHALTE_GENERAST_DICEROLL: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_DICEROLL.as_bytes(), 0);

/// Identifier for `alhalte/generast/judgement`.
pub const IDENT_ALHALTE_JUDGEMENT: &str = "alhalte/generast/judgement";
/// Label hash for `alhalte/generast/judgement`.
pub const LABEL_ALHALTE_JUDGEMENT: u64 = xxh64(IDENT_ALHALTE_JUDGEMENT.as_bytes(), 0);

/// Identifier for `alhalte/generast/character-id`.
pub const IDENT_ALHALTE_GENERAST_CHARACTER_ID: &str = "alhalte/generast/character-id";
/// Label hash for `alhalte/generast/character-id`.
pub const LABEL_ALHALTE_GENERAST_CHARACTER_ID: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_CHARACTER_ID.as_bytes(), 0);

/// Identifier for `alhalte/generast/fluct`.
pub const IDENT_ALHALTE_GENERAST_FLUCT: &str = "alhalte/generast/fluct";
/// Label hash for `alhalte/generast/fluct`.
pub const LABEL_ALHALTE_GENERAST_FLUCT: u64 = xxh64(IDENT_ALHALTE_GENERAST_FLUCT.as_bytes(), 0);

/// Identifier for `alhalte/generast/assign`.
pub const IDENT_ALHALTE_GENERAST_ASSIGN: &str = "alhalte/generast/assign";
/// Label hash for `alhalte/generast/assign`.
pub const LABEL_ALHALTE_GENERAST_ASSIGN: u64 = xxh64(IDENT_ALHALTE_GENERAST_ASSIGN.as_bytes(), 0);

/// Identifier for `alhalte/generast/syntax-error`.
pub const IDENT_ALHALTE_GENERAST_SYNTAX_ERROR: &str = "alhalte/generast/syntax-error";
/// Label hash for `alhalte/generast/syntax-error`.
pub const LABEL_ALHALTE_GENERAST_SYNTAX_ERROR: u64 =
    xxh64(IDENT_ALHALTE_GENERAST_SYNTAX_ERROR.as_bytes(), 0);

// --- Style metadata helpers (0-byte payload) ---

macro_rules! impl_zero_payload_meta {
    ($meta_fn:ident, $is_fn:ident, $label:ident, $name:expr) => {
        #[doc = concat!("Creates a 0-byte metadata `Tin` for `", $name, "`.")]
        pub fn $meta_fn() -> Tin {
            Tin::new($label, Vec::new())
        }

        #[doc = concat!("Returns `true` if the `Tin` has the label for `", $name, "`.")]
        pub fn $is_fn(tin: &Tin) -> bool {
            tin.label == $label
        }
    };
}

impl_zero_payload_meta!(
    bold_meta,
    is_bold_meta,
    LABEL_ALHALTE_GENERAST_STYLE_BOLD,
    "bold"
);
impl_zero_payload_meta!(
    italic_meta,
    is_italic_meta,
    LABEL_ALHALTE_GENERAST_STYLE_ITALIC,
    "italic"
);
impl_zero_payload_meta!(h1_meta, is_h1_meta, LABEL_ALHALTE_GENERAST_STYLE_H1, "h1");
impl_zero_payload_meta!(h2_meta, is_h2_meta, LABEL_ALHALTE_GENERAST_STYLE_H2, "h2");
impl_zero_payload_meta!(h3_meta, is_h3_meta, LABEL_ALHALTE_GENERAST_STYLE_H3, "h3");
impl_zero_payload_meta!(h4_meta, is_h4_meta, LABEL_ALHALTE_GENERAST_STYLE_H4, "h4");
impl_zero_payload_meta!(
    hidden_meta,
    is_hidden_meta,
    LABEL_ALHALTE_GENERAST_STYLE_HIDDEN,
    "hidden"
);
impl_zero_payload_meta!(
    boten_meta,
    is_boten_meta,
    LABEL_ALHALTE_GENERAST_STYLE_BOTEN,
    "boten"
);
impl_zero_payload_meta!(
    grounded_meta,
    is_grounded_meta,
    LABEL_ALHALTE_GENERAST_STYLE_GROUNDED,
    "grounded"
);
impl_zero_payload_meta!(
    h_in_v_meta,
    is_h_in_v_meta,
    LABEL_ALHALTE_GENERAST_STYLE_H_IN_V,
    "h-in-v"
);

// --- Text color ---

/// Creates a metadata `Tin` for text color.
pub fn text_color_meta(color: impl Into<String>) -> Tin {
    Tin::new(
        LABEL_ALHALTE_GENERAST_STYLE_TEXT_COLOR,
        color.into().into_bytes(),
    )
}

/// Extracts the color string from a text color metadata `Tin`.
pub fn extract_text_color(tin: &Tin) -> Result<&str, GenerastError> {
    if tin.label != LABEL_ALHALTE_GENERAST_STYLE_TEXT_COLOR {
        return Err(GenerastError::LabelMismatch {
            expected: LABEL_ALHALTE_GENERAST_STYLE_TEXT_COLOR,
            actual: tin.label,
        });
    }
    std::str::from_utf8(&tin.payload).map_err(GenerastError::InvalidUtf8)
}

// --- Pill ---

/// Creates a metadata `Tin` for a pill with a custom background color.
pub fn pill_meta(bg_color: impl Into<String>) -> Tin {
    Tin::new(
        LABEL_ALHALTE_GENERAST_STYLE_PILL,
        bg_color.into().into_bytes(),
    )
}

/// Extracts the background color string from a pill metadata `Tin`.
pub fn extract_pill(tin: &Tin) -> Result<&str, GenerastError> {
    if tin.label != LABEL_ALHALTE_GENERAST_STYLE_PILL {
        return Err(GenerastError::LabelMismatch {
            expected: LABEL_ALHALTE_GENERAST_STYLE_PILL,
            actual: tin.label,
        });
    }
    std::str::from_utf8(&tin.payload).map_err(GenerastError::InvalidUtf8)
}

// --- Ruby ---

/// Creates a metadata `Tin` for ruby annotation text.
pub fn ruby_meta(ruby: impl Into<String>) -> Tin {
    Tin::new(LABEL_ALHALTE_GENERAST_STYLE_RUBY, ruby.into().into_bytes())
}

/// Extracts the ruby string from a ruby metadata `Tin`.
pub fn extract_ruby(tin: &Tin) -> Result<&str, GenerastError> {
    if tin.label != LABEL_ALHALTE_GENERAST_STYLE_RUBY {
        return Err(GenerastError::LabelMismatch {
            expected: LABEL_ALHALTE_GENERAST_STYLE_RUBY,
            actual: tin.label,
        });
    }
    std::str::from_utf8(&tin.payload).map_err(GenerastError::InvalidUtf8)
}

// --- Indent ---

/// Creates a metadata `Tin` for indentation by `n` characters (`u32` LE).
pub fn indent_meta(n: u32) -> Tin {
    Tin::new(
        LABEL_ALHALTE_GENERAST_STYLE_INDENT,
        n.to_le_bytes().to_vec(),
    )
}

/// Extracts the indent amount (`u32`) from an indent metadata `Tin`.
pub fn extract_indent(tin: &Tin) -> Result<u32, GenerastError> {
    if tin.label != LABEL_ALHALTE_GENERAST_STYLE_INDENT {
        return Err(GenerastError::LabelMismatch {
            expected: LABEL_ALHALTE_GENERAST_STYLE_INDENT,
            actual: tin.label,
        });
    }
    if tin.payload.len() != 4 {
        return Err(GenerastError::InvalidPayloadLength {
            expected: 4,
            actual: tin.payload.len(),
        });
    }
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&tin.payload);
    Ok(u32::from_le_bytes(buf))
}

// --- Hanging ---

/// Creates a metadata `Tin` for hanging indent `(first, rest)` encoded with postcard.
pub fn hanging_meta(first: u32, rest: u32) -> Result<Tin, GenerastError> {
    let payload = postcard::to_allocvec(&(first, rest))?;
    Ok(Tin::new(LABEL_ALHALTE_GENERAST_STYLE_HANGING, payload))
}

/// Extracts `(first, rest)` indentation from a hanging indent metadata `Tin`.
pub fn extract_hanging(tin: &Tin) -> Result<(u32, u32), GenerastError> {
    if tin.label != LABEL_ALHALTE_GENERAST_STYLE_HANGING {
        return Err(GenerastError::LabelMismatch {
            expected: LABEL_ALHALTE_GENERAST_STYLE_HANGING,
            actual: tin.label,
        });
    }
    postcard::from_bytes(&tin.payload).map_err(GenerastError::Postcard)
}

// --- Kerning ---

/// Creates a metadata `Tin` for kerning / auto-wrap every `n` characters (`u32` LE).
pub fn kerning_meta(n: u32) -> Tin {
    Tin::new(
        LABEL_ALHALTE_GENERAST_STYLE_KERNING,
        n.to_le_bytes().to_vec(),
    )
}

/// Extracts the kerning character count (`u32`) from a kerning metadata `Tin`.
pub fn extract_kerning(tin: &Tin) -> Result<u32, GenerastError> {
    if tin.label != LABEL_ALHALTE_GENERAST_STYLE_KERNING {
        return Err(GenerastError::LabelMismatch {
            expected: LABEL_ALHALTE_GENERAST_STYLE_KERNING,
            actual: tin.label,
        });
    }
    if tin.payload.len() != 4 {
        return Err(GenerastError::InvalidPayloadLength {
            expected: 4,
            actual: tin.payload.len(),
        });
    }
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&tin.payload);
    Ok(u32::from_le_bytes(buf))
}

// --- Diceroll ---

/// Encodes a [`DicerollData`] payload into a `Tin` with label `alhalte/generast/diceroll`.
pub fn diceroll_tin(seed: [u8; 32], expr: DiceExpr) -> Result<Tin, GenerastError> {
    let data = DicerollData { seed, expr };
    let payload = postcard::to_allocvec(&data)?;
    Ok(Tin::new(LABEL_ALHALTE_GENERAST_DICEROLL, payload))
}

/// Extracts [`DicerollData`] from a `Tin` with label `alhalte/generast/diceroll`.
pub fn extract_diceroll(tin: &Tin) -> Result<DicerollData, GenerastError> {
    if tin.label != LABEL_ALHALTE_GENERAST_DICEROLL {
        return Err(GenerastError::LabelMismatch {
            expected: LABEL_ALHALTE_GENERAST_DICEROLL,
            actual: tin.label,
        });
    }
    postcard::from_bytes(&tin.payload).map_err(GenerastError::Postcard)
}

// --- Judgement ---

/// Encodes a [`JudgementData`] payload into a `Tin` with label `alhalte/generast/judgement`.
pub fn judgement_tin(data: &JudgementData) -> Result<Tin, GenerastError> {
    let payload = postcard::to_allocvec(data)?;
    Ok(Tin::new(LABEL_ALHALTE_JUDGEMENT, payload))
}

/// Extracts [`JudgementData`] from a `Tin` with label `alhalte/generast/judgement`.
pub fn extract_judgement(tin: &Tin) -> Result<JudgementData, GenerastError> {
    if tin.label != LABEL_ALHALTE_JUDGEMENT {
        return Err(GenerastError::LabelMismatch {
            expected: LABEL_ALHALTE_JUDGEMENT,
            actual: tin.label,
        });
    }
    postcard::from_bytes(&tin.payload).map_err(GenerastError::Postcard)
}

// --- Character ID ---

/// Creates a `Tin` containing a character ID mention (`u64` LE).
pub fn character_id_tin(id: u64) -> Tin {
    Tin::new(
        LABEL_ALHALTE_GENERAST_CHARACTER_ID,
        id.to_le_bytes().to_vec(),
    )
}

/// Extracts the character ID (`u64`) from a character ID `Tin`.
pub fn extract_character_id(tin: &Tin) -> Result<u64, GenerastError> {
    if tin.label != LABEL_ALHALTE_GENERAST_CHARACTER_ID {
        return Err(GenerastError::LabelMismatch {
            expected: LABEL_ALHALTE_GENERAST_CHARACTER_ID,
            actual: tin.label,
        });
    }
    if tin.payload.len() != 8 {
        return Err(GenerastError::InvalidPayloadLength {
            expected: 8,
            actual: tin.payload.len(),
        });
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&tin.payload);
    Ok(u64::from_le_bytes(buf))
}

// --- Fluct ---

/// Encodes a status fluctuation `(character_id, status, delta)` into a `Tin`.
pub fn fluct_tin(
    character_id: u64,
    status: impl Into<String>,
    delta: i32,
) -> Result<Tin, GenerastError> {
    let data = FluctData {
        character_id,
        status: status.into(),
        delta,
    };
    let payload = postcard::to_allocvec(&data)?;
    Ok(Tin::new(LABEL_ALHALTE_GENERAST_FLUCT, payload))
}

/// Extracts [`FluctData`] from a status fluctuation `Tin`.
pub fn extract_fluct(tin: &Tin) -> Result<FluctData, GenerastError> {
    if tin.label != LABEL_ALHALTE_GENERAST_FLUCT {
        return Err(GenerastError::LabelMismatch {
            expected: LABEL_ALHALTE_GENERAST_FLUCT,
            actual: tin.label,
        });
    }
    postcard::from_bytes(&tin.payload).map_err(GenerastError::Postcard)
}

// --- Assign ---

/// Encodes a status assignment `(character_id, status, value)` into a `Tin`.
pub fn assign_tin(
    character_id: u64,
    status: impl Into<String>,
    value: i32,
) -> Result<Tin, GenerastError> {
    let data = AssignData {
        character_id,
        status: status.into(),
        value,
    };
    let payload = postcard::to_allocvec(&data)?;
    Ok(Tin::new(LABEL_ALHALTE_GENERAST_ASSIGN, payload))
}

/// Extracts [`AssignData`] from a status assignment `Tin`.
pub fn extract_assign(tin: &Tin) -> Result<AssignData, GenerastError> {
    if tin.label != LABEL_ALHALTE_GENERAST_ASSIGN {
        return Err(GenerastError::LabelMismatch {
            expected: LABEL_ALHALTE_GENERAST_ASSIGN,
            actual: tin.label,
        });
    }
    postcard::from_bytes(&tin.payload).map_err(GenerastError::Postcard)
}

// --- Syntax Error ---

/// Creates a `Tin` representing a syntax error with an error message string.
pub fn syntax_error_tin(msg: impl Into<String>) -> Tin {
    Tin::new(LABEL_ALHALTE_GENERAST_SYNTAX_ERROR, msg.into().into_bytes())
}

/// Extracts the error message string from a syntax error `Tin`.
pub fn extract_syntax_error(tin: &Tin) -> Result<String, GenerastError> {
    if tin.label != LABEL_ALHALTE_GENERAST_SYNTAX_ERROR {
        return Err(GenerastError::LabelMismatch {
            expected: LABEL_ALHALTE_GENERAST_SYNTAX_ERROR,
            actual: tin.label,
        });
    }
    match std::str::from_utf8(&tin.payload) {
        Ok(s) => Ok(s.to_string()),
        Err(_) => postcard::from_bytes(&tin.payload).map_err(GenerastError::Postcard),
    }
}
