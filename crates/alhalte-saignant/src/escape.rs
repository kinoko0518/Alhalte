//! HTML escaping and CSS injection prevention utilities.

/// Escapes special HTML characters (`&`, `<`, `>`, `"`, `'`) to prevent XSS.
pub fn escape_html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Escapes attribute values to ensure valid and safe HTML attribute strings.
pub fn escape_attribute(input: &str) -> String {
    escape_html(input)
}

/// Sanitizes a CSS color string to prevent CSS injection, attribute breakout, and XSS.
///
/// Supported valid color formats:
/// - Named colors: `[a-zA-Z]{1,30}` (e.g. `red`, `blue`, `rebeccapurple`, `transparent`, `currentColor`)
/// - Hex colors: `#[0-9a-fA-F]{3,8}` (e.g. `#fff`, `#123456`, `#12345678`)
/// - Functional notation: `rgb(...)`, `rgba(...)`, `hsl(...)`, `hsla(...)`, `oklch(...)`, `oklab(...)`, `lab(...)`, `lch(...)`
/// - CSS custom properties: `var(--[a-zA-Z0-9_-]+)`
///
/// Explicitly rejects:
/// - Semicolons `;`, quotes `'`, `"`, backslashes `\`, curly braces `{`, `}`
/// - HTML tags `<`, `>`, comments `/*`, `*/`
/// - Functions attempting network or script execution: `url(...)`, `expression(...)`, `javascript:...`
/// - Control characters and newlines
///
/// Returns `Some(color)` if strictly safe, or `None` if the input is malformed or dangerous.
pub fn sanitize_css_color(color: &str) -> Option<String> {
    let trimmed = color.trim();
    if trimmed.is_empty() || trimmed.len() > 100 {
        return None;
    }

    // Disallow dangerous characters outright
    for b in trimmed.bytes() {
        if b.is_ascii_control()
            || matches!(
                b,
                b';' | b'"' | b'\'' | b'\\' | b'{' | b'}' | b'<' | b'>' | b'`'
            )
        {
            return None;
        }
    }

    let lower = trimmed.to_ascii_lowercase();

    // Check for dangerous keywords anywhere
    if lower.contains("url")
        || lower.contains("javascript")
        || lower.contains("expression")
        || lower.contains("import")
        || lower.contains("behavior")
        || lower.contains("/*")
        || lower.contains("*/")
    {
        return None;
    }

    // 1. Hex color: #rgb, #rgba, #rrggbb, #rrggbbaa
    if let Some(hex_part) = lower.strip_prefix('#') {
        let len = hex_part.len();
        if (len == 3 || len == 4 || len == 6 || len == 8)
            && hex_part.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Some(lower);
        }
        return None;
    }

    // 2. Named color (ASCII alphabetic, length 1..=30)
    if lower.chars().all(|c| c.is_ascii_alphabetic()) && lower.len() <= 30 {
        return Some(lower);
    }

    // 3. CSS variable: var(--custom-ident)
    if lower.starts_with("var(--") && lower.ends_with(')') {
        let inner = &lower[6..lower.len() - 1];
        if !inner.is_empty()
            && inner
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Some(lower);
        }
        return None;
    }

    // 4. Color function: rgb, rgba, hsl, hsla, oklch, oklab, lab, lch
    let allowed_prefixes = [
        "rgb(", "rgba(", "hsl(", "hsla(", "oklch(", "oklab(", "lab(", "lch(",
    ];
    for prefix in allowed_prefixes {
        if lower.starts_with(prefix) && lower.ends_with(')') {
            let inner = &lower[prefix.len()..lower.len() - 1];
            // Verify balanced parens and safe allowed characters inside function:
            // digits, dots, commas, spaces, percent, slash, plus, minus, deg, rad, turn, none
            let is_safe = inner.chars().all(|c| {
                c.is_ascii_digit()
                    || c.is_ascii_whitespace()
                    || matches!(c, '.' | ',' | '%' | '/' | '+' | '-')
                    || matches!(c, 'd' | 'e' | 'g' | 'r' | 'a' | 't' | 'u' | 'n' | 'o')
            });
            if is_safe {
                return Some(lower);
            }
            return None;
        }
    }

    None
}
