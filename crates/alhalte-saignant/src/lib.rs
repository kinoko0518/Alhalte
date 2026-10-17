//! # alhalte-saignant
//!
//! `alhalte-saignant` is a library crate responsible for interpreting scene log entries
//! and Generast abstract syntax trees, and rendering them into semantic HTML with
//! Material 3 Expressive styling.
//!
//! # Architecture
//!
//! - **Log Interpretation**: Duck-typed `HashMap<String, Tin>` or structured [`LogEntry`],
//!   interpreting `"content"` (`alhalte/generast`), `"speaker"` (`alhalte/generast/character-id`),
//!   and `"timestamp"` (`alhalte/unix-time`) with guaranteed fallbacks.
//! - **Generast 23-Label Rendering**: HTML rendering with strict XSS escaping and CSS injection
//!   sanitization for all 23 Generast labels (typography, ruby/boten, layout, dice rolls, judgements,
//!   mentions, status fluctuations, and syntax errors).
//! - **Categorized 2-Layer CSS**: Split into Structural (geometry, positioning, emphasis)
//!   and Expressive (Material 3 Expressive color tokens, shapes, elevation, transitions) stylesheets.
//!
//! # Example
//!
//! ```
//! use alhalte_saignant::{LogEntry, NoopCharacterResolver, render_log_entry};
//! use generast::Generast;
//!
//! let ast = Generast::bold(vec![Generast::string("Critical Hit!")]);
//! let entry = LogEntry::new()
//!     .with_content_ast(&ast)
//!     .with_speaker_id(42)
//!     .with_unix_time(0);
//!
//! let resolver = |id: u64| if id == 42 { Some("Alice".to_string()) } else { None };
//! let html = render_log_entry(&entry, &resolver);
//!
//! assert!(html.contains("Alice"));
//! assert!(html.contains("saignant-bold"));
//! assert!(html.contains("1970-01-01T00:00:00Z"));
//! ```

#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod abi;
pub mod css;
pub mod datetime;
pub mod error;
pub mod escape;
pub mod log;
pub mod render;
pub mod resolver;

pub use css::{CssCategory, CssLayer, category_css, expressive_css, full_css, structural_css};
pub use datetime::{
    TimestampFormat, format_iso8601, format_japanese, format_standard, format_timestamp,
    unix_secs_to_datetime,
};
pub use error::{SaignantError, SaignantResult};
pub use escape::{escape_attribute, escape_html, sanitize_css_color};
pub use log::{
    LogEntry, RenderOptions, render_log_entry, render_log_entry_with_options, render_log_map,
    render_log_map_with_options,
};
pub use render::{render_generast, render_generast_tin};
pub use resolver::{CharacterResolver, NoopCharacterResolver};
