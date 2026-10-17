//! Tests for HTML escaping and CSS color sanitization.
//!
//! // spec: alhalte-saignant/escape XSS and CSS injection prevention

use alhalte_saignant::{escape_html, sanitize_css_color};

#[test]
fn test_html_escaping_spec() {
    // spec: alhalte-saignant/escape HTML special characters
    assert_eq!(escape_html("Hello, World!"), "Hello, World!");
    assert_eq!(
        escape_html("<script>alert('XSS & \"injection\"')</script>"),
        "&lt;script&gt;alert(&#39;XSS &amp; &quot;injection&quot;&#39;)&lt;/script&gt;"
    );
    assert_eq!(
        escape_html("a & b < c > d \" e ' f"),
        "a &amp; b &lt; c &gt; d &quot; e &#39; f"
    );
}

#[test]
fn test_css_color_sanitization_valid() {
    // spec: alhalte-saignant/escape valid CSS colors
    assert_eq!(sanitize_css_color("red"), Some("red".to_string()));
    assert_eq!(sanitize_css_color("BLUE"), Some("blue".to_string()));
    assert_eq!(
        sanitize_css_color("transparent"),
        Some("transparent".to_string())
    );
    assert_eq!(
        sanitize_css_color("currentColor"),
        Some("currentcolor".to_string())
    );
    assert_eq!(sanitize_css_color("#fff"), Some("#fff".to_string()));
    assert_eq!(sanitize_css_color("#123456"), Some("#123456".to_string()));
    assert_eq!(
        sanitize_css_color("#A1B2C3D4"),
        Some("#a1b2c3d4".to_string())
    );
    assert_eq!(
        sanitize_css_color("rgb(255, 0, 128)"),
        Some("rgb(255, 0, 128)".to_string())
    );
    assert_eq!(
        sanitize_css_color("rgba(0, 0, 0, 0.5)"),
        Some("rgba(0, 0, 0, 0.5)".to_string())
    );
    assert_eq!(
        sanitize_css_color("hsl(240, 100%, 50%)"),
        Some("hsl(240, 100%, 50%)".to_string())
    );
    assert_eq!(
        sanitize_css_color("var(--md-sys-color-primary)"),
        Some("var(--md-sys-color-primary)".to_string())
    );
}

#[test]
fn test_css_color_sanitization_injection_attacks() {
    // spec: alhalte-saignant/escape CSS injection prevention
    // Semicolon injection
    assert_eq!(
        sanitize_css_color("red; background-image: url('https://evil.com')"),
        None
    );
    // Quote breakout
    assert_eq!(sanitize_css_color("red\" onclick=\"alert(1)"), None);
    assert_eq!(sanitize_css_color("red' onmouseover='alert(1)"), None);
    // Tag injection
    assert_eq!(
        sanitize_css_color("red</style><script>alert(1)</script>"),
        None
    );
    // url() keyword
    assert_eq!(sanitize_css_color("url(http://evil.com/leak)"), None);
    // javascript:
    assert_eq!(sanitize_css_color("javascript:alert(1)"), None);
    // expression()
    assert_eq!(sanitize_css_color("expression(alert(1))"), None);
    // Braces
    assert_eq!(sanitize_css_color("red { color: blue }"), None);
    // Empty or excessively long
    assert_eq!(sanitize_css_color(""), None);
    assert_eq!(sanitize_css_color("   "), None);
    assert_eq!(sanitize_css_color(&"a".repeat(101)), None);
}
