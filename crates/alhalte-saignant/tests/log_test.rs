//! Tests for log entry interpretation, duck-typing, and HTML rendering.
//!
//! // spec: alhalte-saignant/log entry interpretation and fallbacks

use std::collections::HashMap;

use alhalte_saignant::{
    LogEntry, NoopCharacterResolver, RenderOptions, TimestampFormat, format_iso8601,
    format_japanese, format_standard, render_log_entry, render_log_entry_with_options,
    render_log_map,
};
use generast::Generast;
use tin::Tin;

#[test]
fn test_log_entry_full_rendering() {
    // spec: alhalte-saignant/log structured LogEntry
    let ast = Generast::italic(vec![Generast::string("Let us enter the dungeon.")]);
    let entry = LogEntry::new()
        .with_content_ast(&ast)
        .with_speaker_id(1)
        .with_unix_time(1700000000);

    let mut map_resolver = HashMap::new();
    map_resolver.insert(
        1u64,
        (
            "Paladin".to_string(),
            Some("https://example.com/avatar.webp".to_string()),
        ),
    );

    let html = render_log_entry(&entry, &map_resolver);

    assert!(html.contains("<article class=\"saignant-log-entry\" data-speaker-id=\"1\">"));
    assert!(html.contains("<img class=\"saignant-log-avatar\" src=\"https://example.com/avatar.webp\" alt=\"Paladin\" />"));
    assert!(html.contains("<span class=\"saignant-log-speaker\">Paladin</span>"));
    assert!(html.contains("<time class=\"saignant-log-timestamp\" datetime=\""));
    assert!(html.contains("<em class=\"saignant-italic\">Let us enter the dungeon.</em>"));
}

#[test]
fn test_log_entry_fallbacks() {
    // spec: alhalte-saignant/log fallbacks (content="", speaker="Anonymous", timestamp=0)
    let empty_entry = LogEntry::new();

    let html = render_log_entry(&empty_entry, &NoopCharacterResolver);

    // Speaker fallback: "Anonymous"
    assert!(html.contains("data-speaker-id=\"anonymous\""));
    assert!(html.contains("<span class=\"saignant-log-speaker\">Anonymous</span>"));
    assert!(
        html.contains(
            "<span class=\"saignant-log-avatar-placeholder\" aria-hidden=\"true\">A</span>"
        )
    );

    // Timestamp fallback: 0 (1970-01-01T00:00:00Z)
    assert!(html.contains("datetime=\"1970-01-01T00:00:00Z\""));

    // Content fallback: empty string
    assert!(html.contains("<div class=\"saignant-log-content\"></div>"));
}

#[test]
fn test_log_duck_typing_map() {
    // spec: alhalte-saignant/log duck typing HashMap<String, Tin>
    let mut map = HashMap::new();

    let content_ast = Generast::bold(vec![Generast::string("Duck Typing Message")]);
    map.insert("content".to_string(), content_ast.to_tin().unwrap());
    map.insert("speaker".to_string(), generast::character_id_tin(77));
    map.insert("timestamp".to_string(), Tin::from_unix_time(0));

    let resolver = |id: u64| {
        if id == 77 {
            Some("Mage".to_string())
        } else {
            None
        }
    };
    let html = render_log_map(&map, &resolver);

    assert!(html.contains("data-speaker-id=\"77\""));
    assert!(html.contains("Mage"));
    assert!(html.contains("1970-01-01T00:00:00Z"));
    assert!(html.contains("<strong class=\"saignant-bold\">Duck Typing Message</strong>"));
}

#[test]
fn test_log_timestamp_formats() {
    // spec: alhalte-saignant/log timestamp formatting
    let secs = 0; // 1970-01-01 00:00:00 UTC
    assert_eq!(format_iso8601(secs), "1970-01-01T00:00:00Z");
    assert_eq!(format_japanese(secs), "1970年1月1日0時0分0秒");
    assert_eq!(format_standard(secs), "1970-01-01 00:00:00");

    let entry = LogEntry::new().with_unix_time(0);

    // Japanese options
    let opt_jp = RenderOptions::new().with_japanese_timestamps();
    let html_jp = render_log_entry_with_options(&entry, &NoopCharacterResolver, &opt_jp);
    assert!(html_jp.contains("datetime=\"1970-01-01T00:00:00Z\""));
    assert!(html_jp.contains("1970年1月1日0時0分0秒"));

    // Standard options
    let opt_std = RenderOptions {
        timestamp_format: TimestampFormat::Standard,
    };
    let html_std = render_log_entry_with_options(&entry, &NoopCharacterResolver, &opt_std);
    assert!(html_std.contains("datetime=\"1970-01-01T00:00:00Z\""));
    assert!(html_std.contains("1970-01-01 00:00:00"));
}
