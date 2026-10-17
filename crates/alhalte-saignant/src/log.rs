//! Log entry structures, duck-typing map interpretation, and HTML translation.

use std::collections::HashMap;

use generast::{Generast, LABEL_ALHALTE_GENERAST, extract_character_id};
use tin::Tin;
use tin::label::{LABEL_ALHALTE_UNIX_TIME, LABEL_PRIMITIVE_U64};

use crate::datetime::{TimestampFormat, format_iso8601, format_timestamp};
use crate::escape::{escape_attribute, escape_html};
use crate::render::render_generast;
use crate::resolver::CharacterResolver;

/// Rendering configuration options for log entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderOptions {
    /// Display format for the human-readable text inside `<time>`.
    pub timestamp_format: TimestampFormat,
}

impl RenderOptions {
    /// Creates options with default ISO 8601 timestamp format.
    pub const fn new() -> Self {
        Self {
            timestamp_format: TimestampFormat::Iso8601,
        }
    }

    /// Sets the timestamp format to Japanese notation.
    pub const fn with_japanese_timestamps(mut self) -> Self {
        self.timestamp_format = TimestampFormat::Japanese;
        self
    }
}

/// Structured representation of a scene log entry.
///
/// Log entries follow duck-typing via `HashMap<String, Tin>` with three standard fields:
/// - `"content"`: `alhalte/generast` container (fallback: empty string `""`)
/// - `"speaker"`: `alhalte/generast/character-id` container (fallback: `"Anonymous"`)
/// - `"timestamp"`: `alhalte/unix-time` container (fallback: `0` / `1970-01-01T00:00:00Z`)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LogEntry {
    /// Message content AST wrapped in a [`Tin`].
    pub content: Option<Tin>,
    /// Speaker character ID wrapped in a [`Tin`].
    pub speaker: Option<Tin>,
    /// Creation timestamp in UNIX seconds wrapped in a [`Tin`].
    pub timestamp: Option<Tin>,
    /// Additional or extension-specific log fields.
    pub extra: HashMap<String, Tin>,
}

impl LogEntry {
    /// Creates an empty log entry with all standard fields set to `None`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a log entry from a duck-typed map `HashMap<String, Tin>`.
    pub fn from_map(mut map: HashMap<String, Tin>) -> Self {
        let content = map.remove("content");
        let speaker = map.remove("speaker");
        let timestamp = map.remove("timestamp");
        Self {
            content,
            speaker,
            timestamp,
            extra: map,
        }
    }

    /// Converts this log entry back into a duck-typed map `HashMap<String, Tin>`.
    pub fn to_map(&self) -> HashMap<String, Tin> {
        let mut map = self.extra.clone();
        if let Some(ref c) = self.content {
            map.insert("content".to_string(), c.clone());
        }
        if let Some(ref s) = self.speaker {
            map.insert("speaker".to_string(), s.clone());
        }
        if let Some(ref t) = self.timestamp {
            map.insert("timestamp".to_string(), t.clone());
        }
        map
    }

    /// Sets the `"content"` field to an encoded [`Generast`] syntax tree.
    pub fn with_content_ast(mut self, ast: &Generast) -> Self {
        if let Ok(tin) = ast.to_tin() {
            self.content = Some(tin);
        }
        self
    }

    /// Sets the `"speaker"` field to a character ID.
    pub fn with_speaker_id(mut self, id: u64) -> Self {
        self.speaker = Some(generast::character_id_tin(id));
        self
    }

    /// Sets the `"timestamp"` field to UNIX time seconds.
    pub fn with_unix_time(mut self, secs: u64) -> Self {
        self.timestamp = Some(Tin::from_unix_time(secs));
        self
    }

    /// Extracts the numeric character ID from the `"speaker"` container, if valid.
    pub fn speaker_id(&self) -> Option<u64> {
        let tin = self.speaker.as_ref()?;
        extract_character_id(tin).ok()
    }

    /// Extracts the UNIX time in seconds, or returns `0` as fallback.
    pub fn unix_time(&self) -> u64 {
        let Some(ref tin) = self.timestamp else {
            return 0;
        };
        if (tin.label == LABEL_ALHALTE_UNIX_TIME || tin.label == LABEL_PRIMITIVE_U64)
            && tin.payload.len() == 8
        {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&tin.payload);
            return u64::from_le_bytes(buf);
        }
        0
    }

    /// Decodes the `"content"` field as a [`Generast`] syntax tree, if present and valid.
    pub fn content_ast(&self) -> Option<Generast> {
        let tin = self.content.as_ref()?;
        if tin.label != LABEL_ALHALTE_GENERAST {
            return None;
        }
        Generast::from_tin(tin).ok()
    }
}

/// Renders a [`LogEntry`] to HTML using the specified [`CharacterResolver`] and default options.
pub fn render_log_entry(entry: &LogEntry, resolver: &impl CharacterResolver) -> String {
    render_log_entry_with_options(entry, resolver, &RenderOptions::default())
}

/// Renders a [`LogEntry`] to HTML using the specified [`CharacterResolver`] and [`RenderOptions`].
pub fn render_log_entry_with_options(
    entry: &LogEntry,
    resolver: &impl CharacterResolver,
    options: &RenderOptions,
) -> String {
    let mut out = String::with_capacity(512);

    // Resolve speaker
    let (speaker_name, avatar_url, speaker_id_attr) = match entry.speaker_id() {
        Some(id) => {
            let name = resolver
                .resolve_name(id)
                .unwrap_or_else(|| "Anonymous".to_string());
            let avatar = resolver.resolve_avatar_url(id);
            (name, avatar, id.to_string())
        }
        None => ("Anonymous".to_string(), None, "anonymous".to_string()),
    };

    // Resolve timestamp
    let secs = entry.unix_time();
    let iso_datetime = format_iso8601(secs);
    let display_time = format_timestamp(secs, options.timestamp_format);

    // Resolve content
    let rendered_content = match entry.content_ast() {
        Some(ast) => render_generast(&ast, resolver),
        None => String::new(),
    };

    out.push_str("<article class=\"saignant-log-entry\" data-speaker-id=\"");
    out.push_str(&escape_attribute(&speaker_id_attr));
    out.push_str("\">");

    // Avatar icon
    out.push_str("<div class=\"saignant-log-icon\">");
    if let Some(ref url) = avatar_url {
        out.push_str("<img class=\"saignant-log-avatar\" src=\"");
        out.push_str(&escape_attribute(url));
        out.push_str("\" alt=\"");
        out.push_str(&escape_attribute(&speaker_name));
        out.push_str("\" />");
    } else {
        let initial = speaker_name.chars().next().unwrap_or('A');
        out.push_str("<span class=\"saignant-log-avatar-placeholder\" aria-hidden=\"true\">");
        out.push_str(&escape_html(&initial.to_string()));
        out.push_str("</span>");
    }
    out.push_str("</div>");

    // Body
    out.push_str("<div class=\"saignant-log-body\">");

    // Header (Speaker + Timestamp)
    out.push_str("<div class=\"saignant-log-header\">");
    out.push_str("<span class=\"saignant-log-speaker\">");
    out.push_str(&escape_html(&speaker_name));
    out.push_str("</span>");
    out.push_str("<time class=\"saignant-log-timestamp\" datetime=\"");
    out.push_str(&iso_datetime);
    out.push_str("\">");
    out.push_str(&escape_html(&display_time));
    out.push_str("</time>");
    out.push_str("</div>");

    // Content
    out.push_str("<div class=\"saignant-log-content\">");
    out.push_str(&rendered_content);
    out.push_str("</div>");

    out.push_str("</div>"); // .saignant-log-body
    out.push_str("</article>");

    out
}

/// Renders a duck-typed log map `HashMap<String, Tin>` to HTML using the specified [`CharacterResolver`].
pub fn render_log_map(map: &HashMap<String, Tin>, resolver: &impl CharacterResolver) -> String {
    let entry = LogEntry::from_map(map.clone());
    render_log_entry(&entry, resolver)
}

/// Renders a duck-typed log map with custom [`RenderOptions`].
pub fn render_log_map_with_options(
    map: &HashMap<String, Tin>,
    resolver: &impl CharacterResolver,
    options: &RenderOptions,
) -> String {
    let entry = LogEntry::from_map(map.clone());
    render_log_entry_with_options(&entry, resolver, options)
}
