//! Core syntax tree data types and encoding/decoding functions.

use serde::{Deserialize, Serialize};
use tin::Tin;

use crate::error::GenerastError;
use crate::label::{
    LABEL_ALHALTE_GENERAST, assign_tin, bold_meta, boten_meta, character_id_tin, fluct_tin,
    grounded_meta, h_in_v_meta, h1_meta, h2_meta, h3_meta, h4_meta, hanging_meta, hidden_meta,
    indent_meta, italic_meta, kerning_meta, pill_meta, ruby_meta, syntax_error_tin,
    text_color_meta,
};

/// A nested tree node containing styling/formatting metadata and children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nested {
    /// Formatting or semantics metadata container.
    pub meta: Tin,
    /// Ordered child nodes.
    pub contents: Vec<Generast>,
}

impl Nested {
    /// Creates a new `Nested` node.
    pub const fn new(meta: Tin, contents: Vec<Generast>) -> Self {
        Self { meta, contents }
    }
}

/// Generic abstract syntax tree node for chat messages and rich text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Generast {
    /// Leaf element container wrapping a typed [`Tin`].
    Element(Tin),
    /// Plain text string.
    String(String),
    /// Nested formatting block.
    Nested(Nested),
}

impl Generast {
    /// Creates a plain text string node.
    pub fn string(s: impl Into<String>) -> Self {
        Self::String(s.into())
    }

    /// Creates an element node from a [`Tin`].
    pub const fn element(tin: Tin) -> Self {
        Self::Element(tin)
    }

    /// Creates a nested node with metadata and contents.
    pub const fn nested(meta: Tin, contents: Vec<Generast>) -> Self {
        Self::Nested(Nested::new(meta, contents))
    }

    /// Encodes this AST into a [`Tin`] with label `alhalte/generast`.
    pub fn to_tin(&self) -> Result<Tin, GenerastError> {
        encode_generast(self)
    }

    /// Decodes an AST from a [`Tin`] with label `alhalte/generast`.
    pub fn from_tin(tin: &Tin) -> Result<Self, GenerastError> {
        decode_generast(tin)
    }

    /// Creates a bold nested node.
    pub fn bold(contents: Vec<Generast>) -> Self {
        Self::nested(bold_meta(), contents)
    }

    /// Creates an italic nested node.
    pub fn italic(contents: Vec<Generast>) -> Self {
        Self::nested(italic_meta(), contents)
    }

    /// Creates a heading 1 nested node.
    pub fn h1(contents: Vec<Generast>) -> Self {
        Self::nested(h1_meta(), contents)
    }

    /// Creates a heading 2 nested node.
    pub fn h2(contents: Vec<Generast>) -> Self {
        Self::nested(h2_meta(), contents)
    }

    /// Creates a heading 3 nested node.
    pub fn h3(contents: Vec<Generast>) -> Self {
        Self::nested(h3_meta(), contents)
    }

    /// Creates a heading 4 nested node.
    pub fn h4(contents: Vec<Generast>) -> Self {
        Self::nested(h4_meta(), contents)
    }

    /// Creates a hidden/spoiler nested node.
    pub fn hidden(contents: Vec<Generast>) -> Self {
        Self::nested(hidden_meta(), contents)
    }

    /// Creates a boten (emphasis dot) nested node.
    pub fn boten(contents: Vec<Generast>) -> Self {
        Self::nested(boten_meta(), contents)
    }

    /// Creates a grounded (bottom-aligned) nested node.
    pub fn grounded(contents: Vec<Generast>) -> Self {
        Self::nested(grounded_meta(), contents)
    }

    /// Creates a horizontal-in-vertical (tate-chū-yoko) nested node.
    pub fn h_in_v(contents: Vec<Generast>) -> Self {
        Self::nested(h_in_v_meta(), contents)
    }

    /// Creates a text-color nested node.
    pub fn text_color(color: impl Into<String>, contents: Vec<Generast>) -> Self {
        Self::nested(text_color_meta(color), contents)
    }

    /// Creates a pill nested node with a custom background color.
    pub fn pill(bg_color: impl Into<String>, contents: Vec<Generast>) -> Self {
        Self::nested(pill_meta(bg_color), contents)
    }

    /// Creates a ruby annotation nested node.
    pub fn ruby(ruby: impl Into<String>, contents: Vec<Generast>) -> Self {
        Self::nested(ruby_meta(ruby), contents)
    }

    /// Creates an indent nested node indented by `n` characters.
    pub fn indent(n: u32, contents: Vec<Generast>) -> Self {
        Self::nested(indent_meta(n), contents)
    }

    /// Creates a hanging indent nested node (`first` chars, `rest` chars).
    pub fn hanging(first: u32, rest: u32, contents: Vec<Generast>) -> Result<Self, GenerastError> {
        Ok(Self::nested(hanging_meta(first, rest)?, contents))
    }

    /// Creates a kerning (auto-break every `n` characters) nested node.
    pub fn kerning(n: u32, contents: Vec<Generast>) -> Self {
        Self::nested(kerning_meta(n), contents)
    }

    /// Creates a character mention element node.
    pub fn character_id(id: u64) -> Self {
        Self::element(character_id_tin(id))
    }

    /// Creates a status fluctuation element node.
    pub fn fluct(
        character_id: u64,
        status: impl Into<String>,
        delta: i32,
    ) -> Result<Self, GenerastError> {
        Ok(Self::element(fluct_tin(character_id, status, delta)?))
    }

    /// Creates a status assignment element node.
    pub fn assign(
        character_id: u64,
        status: impl Into<String>,
        value: i32,
    ) -> Result<Self, GenerastError> {
        Ok(Self::element(assign_tin(character_id, status, value)?))
    }

    /// Creates a syntax error element node.
    pub fn syntax_error(msg: impl Into<String>) -> Self {
        Self::element(syntax_error_tin(msg))
    }
}

/// Encodes a [`Generast`] syntax tree into a [`Tin`] with label `alhalte/generast`.
pub fn encode_generast(generast: &Generast) -> Result<Tin, GenerastError> {
    let payload = postcard::to_allocvec(generast)?;
    Ok(Tin::new(LABEL_ALHALTE_GENERAST, payload))
}

/// Decodes a [`Generast`] syntax tree from a [`Tin`] with label `alhalte/generast`.
pub fn decode_generast(tin: &Tin) -> Result<Generast, GenerastError> {
    if tin.label != LABEL_ALHALTE_GENERAST {
        return Err(GenerastError::LabelMismatch {
            expected: LABEL_ALHALTE_GENERAST,
            actual: tin.label,
        });
    }
    postcard::from_bytes(&tin.payload).map_err(GenerastError::Postcard)
}

/// Encodes a `(Generast, speaker_character_id)` pair for extension ABI I/O into a `Tin`.
pub fn encode_ext_io(generast: &Generast, speaker_id: u64) -> Result<Tin, GenerastError> {
    let payload = postcard::to_allocvec(&(generast, speaker_id))?;
    Ok(Tin::new(LABEL_ALHALTE_GENERAST, payload))
}

/// Decodes a `(Generast, speaker_character_id)` pair for extension ABI I/O from a `Tin`.
pub fn decode_ext_io(tin: &Tin) -> Result<(Generast, u64), GenerastError> {
    if tin.label != LABEL_ALHALTE_GENERAST {
        return Err(GenerastError::LabelMismatch {
            expected: LABEL_ALHALTE_GENERAST,
            actual: tin.label,
        });
    }
    postcard::from_bytes(&tin.payload).map_err(GenerastError::Postcard)
}
