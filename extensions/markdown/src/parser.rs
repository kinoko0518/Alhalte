//! Markdown parsing using Winnow combinators.

use generast::Generast;
use winnow::combinator::{alt, delimited};
use winnow::prelude::*;
use winnow::token::{literal, take_until};

use crate::error::MarkdownError;

/// Parses a Markdown document or message into a [`Generast`] syntax tree.
pub fn parse_markdown(input: &str) -> Result<Generast, MarkdownError> {
    if input.is_empty() {
        return Ok(Generast::string(""));
    }

    let mut blocks: Vec<Generast> = Vec::new();
    let lines: Vec<&str> = input.split('\n').collect();

    for (i, line) in lines.iter().enumerate() {
        let block_node = parse_line(line);
        blocks.push(block_node);
        if i + 1 < lines.len() {
            blocks.push(Generast::string("\n"));
        }
    }

    if blocks.len() == 1 {
        Ok(blocks.remove(0))
    } else {
        Ok(Generast::nested(
            tin::Tin::need_not_know(Vec::new()),
            blocks,
        ))
    }
}

fn parse_line(line: &str) -> Generast {
    let trimmed_start = line.trim_start();
    if let Some(rest) = trimmed_start.strip_prefix("#### ") {
        Generast::h4(parse_inlines(rest))
    } else if let Some(rest) = trimmed_start.strip_prefix("### ") {
        Generast::h3(parse_inlines(rest))
    } else if let Some(rest) = trimmed_start.strip_prefix("## ") {
        Generast::h2(parse_inlines(rest))
    } else if let Some(rest) = trimmed_start.strip_prefix("# ") {
        Generast::h1(parse_inlines(rest))
    } else {
        let inlines = parse_inlines(line);
        if inlines.len() == 1 {
            inlines.into_iter().next().unwrap_or(Generast::string(""))
        } else {
            Generast::nested(tin::Tin::need_not_know(Vec::new()), inlines)
        }
    }
}

/// Parses inline Markdown formatting (bold, italic, spoiler, color, pill, plain text).
pub fn parse_inlines(mut input: &str) -> Vec<Generast> {
    let mut inlines = Vec::new();
    let mut plain_accum = String::new();

    while !input.is_empty() {
        // Try color / pill: [text]{color:#ff0000} or [text]{pill:#0055aa}
        if input.starts_with('[') {
            let mut test_cursor = input;
            if let Ok(pill_or_color) = parse_pill_or_color.parse_next(&mut test_cursor) {
                if !plain_accum.is_empty() {
                    inlines.push(Generast::string(std::mem::take(&mut plain_accum)));
                }
                inlines.push(pill_or_color);
                input = test_cursor;
                continue;
            }
        }

        // Try spoiler: ||...||
        if input.starts_with("||") {
            let mut test_cursor = input;
            if let Ok(content) = parse_spoiler.parse_next(&mut test_cursor) {
                if !plain_accum.is_empty() {
                    inlines.push(Generast::string(std::mem::take(&mut plain_accum)));
                }
                inlines.push(Generast::hidden(parse_inlines(content)));
                input = test_cursor;
                continue;
            }
        }

        // Try bold: **...**
        if input.starts_with("**") {
            let mut test_cursor = input;
            if let Ok(content) = parse_bold.parse_next(&mut test_cursor) {
                if !plain_accum.is_empty() {
                    inlines.push(Generast::string(std::mem::take(&mut plain_accum)));
                }
                inlines.push(Generast::bold(parse_inlines(content)));
                input = test_cursor;
                continue;
            }
        }

        // Try italic: *...*
        if input.starts_with('*') {
            let mut test_cursor = input;
            if let Ok(content) = parse_italic.parse_next(&mut test_cursor) {
                if !plain_accum.is_empty() {
                    inlines.push(Generast::string(std::mem::take(&mut plain_accum)));
                }
                inlines.push(Generast::italic(parse_inlines(content)));
                input = test_cursor;
                continue;
            }
        }

        // Consume one character into plain_accum
        let first_char = input.chars().next().unwrap();
        plain_accum.push(first_char);
        input = &input[first_char.len_utf8()..];
    }

    if !plain_accum.is_empty() {
        inlines.push(Generast::string(plain_accum));
    }

    inlines
}

fn parse_bold<'s>(input: &mut &'s str) -> ModalResult<&'s str> {
    delimited("**", take_until(1.., "**"), "**").parse_next(input)
}

fn parse_italic<'s>(input: &mut &'s str) -> ModalResult<&'s str> {
    delimited("*", take_until(1.., "*"), "*").parse_next(input)
}

fn parse_spoiler<'s>(input: &mut &'s str) -> ModalResult<&'s str> {
    delimited("||", take_until(1.., "||"), "||").parse_next(input)
}

fn parse_pill_or_color(input: &mut &str) -> ModalResult<Generast> {
    let content = delimited('[', take_until(1.., ']'), ']').parse_next(input)?;
    let _ = literal('{').parse_next(input)?;
    let prop = alt(("color:", "COLOR:", "pill:", "PILL:")).parse_next(input)?;
    let val = take_until(1.., '}').parse_next(input)?;
    let _ = literal('}').parse_next(input)?;

    let is_color = prop.to_ascii_lowercase().starts_with("color");
    let inner_children = parse_inlines(content);

    if is_color {
        Ok(Generast::text_color(val, inner_children))
    } else {
        Ok(Generast::pill(val, inner_children))
    }
}
