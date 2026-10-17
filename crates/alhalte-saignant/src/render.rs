//! Generast syntax tree to HTML rendering implementation for all 23 labels.

use generast::{
    DiceValue, Generast, LABEL_ALHALTE_GENERAST_ASSIGN, LABEL_ALHALTE_GENERAST_CHARACTER_ID,
    LABEL_ALHALTE_GENERAST_DICEROLL, LABEL_ALHALTE_GENERAST_FLUCT,
    LABEL_ALHALTE_GENERAST_STYLE_BOLD, LABEL_ALHALTE_GENERAST_STYLE_BOTEN,
    LABEL_ALHALTE_GENERAST_STYLE_GROUNDED, LABEL_ALHALTE_GENERAST_STYLE_H_IN_V,
    LABEL_ALHALTE_GENERAST_STYLE_H1, LABEL_ALHALTE_GENERAST_STYLE_H2,
    LABEL_ALHALTE_GENERAST_STYLE_H3, LABEL_ALHALTE_GENERAST_STYLE_H4,
    LABEL_ALHALTE_GENERAST_STYLE_HANGING, LABEL_ALHALTE_GENERAST_STYLE_HIDDEN,
    LABEL_ALHALTE_GENERAST_STYLE_INDENT, LABEL_ALHALTE_GENERAST_STYLE_ITALIC,
    LABEL_ALHALTE_GENERAST_STYLE_KERNING, LABEL_ALHALTE_GENERAST_STYLE_PILL,
    LABEL_ALHALTE_GENERAST_STYLE_RUBY, LABEL_ALHALTE_GENERAST_STYLE_TEXT_COLOR,
    LABEL_ALHALTE_GENERAST_SYNTAX_ERROR, LABEL_ALHALTE_JUDGEMENT, Nested, SeededEntropy,
    extract_assign, extract_character_id, extract_diceroll, extract_fluct, extract_hanging,
    extract_indent, extract_judgement, extract_kerning, extract_pill, extract_ruby,
    extract_syntax_error, extract_text_color,
};
use tin::Tin;

use crate::error::SaignantResult;
use crate::escape::{escape_html, sanitize_css_color};
use crate::resolver::CharacterResolver;

/// Formats a [`DiceValue`] into a safe human-readable string.
fn format_dice_value(val: &DiceValue) -> String {
    match val {
        DiceValue::Integer(i) => format!("{i}"),
        DiceValue::Rational(r) => format!("{r}"),
        DiceValue::Bool(b) => format!("{b}"),
        DiceValue::String(s) => escape_html(s),
        DiceValue::Dice(d) => format!("D{}", d.faces),
        DiceValue::List(list) => {
            let items: Vec<String> = list.iter().map(format_dice_value).collect();
            format!("[{}]", items.join(", "))
        }
    }
}

/// Renders a [`Generast`] syntax tree into HTML using the specified [`CharacterResolver`].
pub fn render_generast(ast: &Generast, resolver: &impl CharacterResolver) -> String {
    let mut out = String::new();
    render_generast_into(ast, resolver, &mut out);
    out
}

/// Renders a `Tin` container holding an `alhalte/generast` payload into HTML.
pub fn render_generast_tin(tin: &Tin, resolver: &impl CharacterResolver) -> SaignantResult<String> {
    let ast = Generast::from_tin(tin)?;
    Ok(render_generast(&ast, resolver))
}

fn render_generast_into(ast: &Generast, resolver: &impl CharacterResolver, out: &mut String) {
    match ast {
        Generast::String(s) => {
            out.push_str(&escape_html(s));
        }
        Generast::Nested(nested) => {
            render_nested_into(nested, resolver, out);
        }
        Generast::Element(tin) => {
            render_element_into(tin, resolver, out);
        }
    }
}

fn render_children_into(
    contents: &[Generast],
    resolver: &impl CharacterResolver,
    out: &mut String,
) {
    for child in contents {
        render_generast_into(child, resolver, out);
    }
}

fn render_nested_into(nested: &Nested, resolver: &impl CharacterResolver, out: &mut String) {
    match nested.meta.label {
        LABEL_ALHALTE_GENERAST_STYLE_BOLD => {
            out.push_str("<strong class=\"saignant-bold\">");
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</strong>");
        }
        LABEL_ALHALTE_GENERAST_STYLE_ITALIC => {
            out.push_str("<em class=\"saignant-italic\">");
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</em>");
        }
        LABEL_ALHALTE_GENERAST_STYLE_H1 => {
            out.push_str("<h1 class=\"saignant-h1\">");
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</h1>");
        }
        LABEL_ALHALTE_GENERAST_STYLE_H2 => {
            out.push_str("<h2 class=\"saignant-h2\">");
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</h2>");
        }
        LABEL_ALHALTE_GENERAST_STYLE_H3 => {
            out.push_str("<h3 class=\"saignant-h3\">");
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</h3>");
        }
        LABEL_ALHALTE_GENERAST_STYLE_H4 => {
            out.push_str("<h4 class=\"saignant-h4\">");
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</h4>");
        }
        LABEL_ALHALTE_GENERAST_STYLE_HIDDEN => {
            out.push_str("<span class=\"saignant-hidden\" data-spoiler=\"true\">");
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</span>");
        }
        LABEL_ALHALTE_GENERAST_STYLE_TEXT_COLOR => {
            let color_opt = extract_text_color(&nested.meta)
                .ok()
                .and_then(sanitize_css_color);
            if let Some(color) = color_opt {
                out.push_str("<span class=\"saignant-text-color\" style=\"color: ");
                out.push_str(&color);
                out.push_str(";\">");
            } else {
                out.push_str("<span class=\"saignant-text-color\">");
            }
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</span>");
        }
        LABEL_ALHALTE_GENERAST_STYLE_PILL => {
            let bg_opt = extract_pill(&nested.meta).ok().and_then(sanitize_css_color);
            if let Some(bg) = bg_opt {
                out.push_str("<span class=\"saignant-pill\" style=\"background-color: ");
                out.push_str(&bg);
                out.push_str(";\">");
            } else {
                out.push_str("<span class=\"saignant-pill\">");
            }
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</span>");
        }
        LABEL_ALHALTE_GENERAST_STYLE_RUBY => {
            let ruby_text = extract_ruby(&nested.meta).unwrap_or("");
            out.push_str("<ruby class=\"saignant-ruby\">");
            render_children_into(&nested.contents, resolver, out);
            out.push_str("<rt class=\"saignant-rt\">");
            out.push_str(&escape_html(ruby_text));
            out.push_str("</rt></ruby>");
        }
        LABEL_ALHALTE_GENERAST_STYLE_BOTEN => {
            out.push_str("<span class=\"saignant-boten\">");
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</span>");
        }
        LABEL_ALHALTE_GENERAST_STYLE_INDENT => {
            let indent = extract_indent(&nested.meta).unwrap_or(0);
            out.push_str("<div class=\"saignant-indent\" style=\"--saignant-indent: ");
            out.push_str(&indent.to_string());
            out.push_str("em;\">");
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</div>");
        }
        LABEL_ALHALTE_GENERAST_STYLE_HANGING => {
            let (first, rest) = extract_hanging(&nested.meta).unwrap_or((0, 0));
            out.push_str("<div class=\"saignant-hanging\" style=\"--saignant-hanging-indent: ");
            out.push_str(&first.to_string());
            out.push_str("em; --saignant-hanging-wrap: ");
            out.push_str(&rest.to_string());
            out.push_str("em;\">");
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</div>");
        }
        LABEL_ALHALTE_GENERAST_STYLE_GROUNDED => {
            out.push_str("<div class=\"saignant-grounded\">");
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</div>");
        }
        LABEL_ALHALTE_GENERAST_STYLE_KERNING => {
            let kerning = extract_kerning(&nested.meta).unwrap_or(80);
            out.push_str("<div class=\"saignant-kerning\" style=\"--saignant-kerning-chars: ");
            out.push_str(&kerning.to_string());
            out.push_str(";\">");
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</div>");
        }
        LABEL_ALHALTE_GENERAST_STYLE_H_IN_V => {
            out.push_str("<span class=\"saignant-h-in-v\">");
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</span>");
        }
        unknown => {
            out.push_str("<span class=\"saignant-nested-unknown\" data-label=\"");
            out.push_str(&unknown.to_string());
            out.push_str("\">");
            render_children_into(&nested.contents, resolver, out);
            out.push_str("</span>");
        }
    }
}

fn render_element_into(tin: &Tin, resolver: &impl CharacterResolver, out: &mut String) {
    match tin.label {
        LABEL_ALHALTE_GENERAST_DICEROLL => match extract_diceroll(tin) {
            Ok(data) => {
                let mut entropy = SeededEntropy::from_seed(data.seed);
                match data.expr.eval_with_trace(&mut entropy) {
                    Ok((val, trace)) => {
                        out.push_str("<span class=\"saignant-diceroll\">");
                        if !trace.is_empty() {
                            out.push_str("<span class=\"saignant-diceroll-trace\">");
                            for record in trace {
                                out.push_str("<span class=\"saignant-diceroll-die\" data-faces=\"");
                                out.push_str(&record.faces.to_string());
                                out.push_str("\">");
                                out.push_str(&record.roll.to_string());
                                out.push_str("</span>");
                            }
                            out.push_str("</span>");
                            out.push_str("<span class=\"saignant-diceroll-separator\"> = </span>");
                        }
                        out.push_str("<span class=\"saignant-diceroll-result\">");
                        out.push_str(&format_dice_value(&val));
                        out.push_str("</span>");
                        out.push_str("</span>");
                    }
                    Err(e) => {
                        out.push_str("<span class=\"saignant-diceroll saignant-diceroll--error\">");
                        out.push_str(&escape_html(&e.to_string()));
                        out.push_str("</span>");
                    }
                }
            }
            Err(e) => {
                out.push_str("<span class=\"saignant-diceroll saignant-diceroll--error\">");
                out.push_str(&escape_html(&e.to_string()));
                out.push_str("</span>");
            }
        },
        LABEL_ALHALTE_JUDGEMENT => match extract_judgement(tin) {
            Ok(judgement) => {
                let modifier = if judgement.degree > 0 {
                    "saignant-judgement--success"
                } else if judgement.degree < 0 {
                    "saignant-judgement--failure"
                } else {
                    "saignant-judgement--neutral"
                };

                out.push_str("<span class=\"saignant-judgement ");
                out.push_str(modifier);
                out.push_str("\" data-degree=\"");
                out.push_str(&judgement.degree.to_string());
                out.push_str("\">");

                if let Some(ref subject) = judgement.subject {
                    out.push_str("<span class=\"saignant-judgement-subject\">");
                    out.push_str(&escape_html(subject));
                    out.push_str("</span>");
                }

                if let Some(ref target) = judgement.target {
                    out.push_str("<span class=\"saignant-judgement-target\">&lt;=");
                    out.push_str(&escape_html(&target.to_string()));
                    out.push_str("</span>");
                }

                out.push_str("<span class=\"saignant-judgement-roll\">");
                render_generast_into(&judgement.roll, resolver, out);
                out.push_str("</span>");

                out.push_str("<span class=\"saignant-judgement-level\">");
                out.push_str(&escape_html(&judgement.level_name));
                out.push_str("</span>");

                out.push_str("</span>");
            }
            Err(e) => {
                out.push_str("<span class=\"saignant-syntax-error\">");
                out.push_str(&escape_html(&e.to_string()));
                out.push_str("</span>");
            }
        },
        LABEL_ALHALTE_GENERAST_CHARACTER_ID => {
            let id = extract_character_id(tin).unwrap_or(0);
            let name = resolver
                .resolve_name(id)
                .unwrap_or_else(|| "Anonymous".to_string());
            out.push_str("<span class=\"saignant-mention\" data-character-id=\"");
            out.push_str(&id.to_string());
            out.push_str("\">@");
            out.push_str(&escape_html(&name));
            out.push_str("</span>");
        }
        LABEL_ALHALTE_GENERAST_FLUCT => match extract_fluct(tin) {
            Ok(fluct) => {
                let name = resolver
                    .resolve_name(fluct.character_id)
                    .unwrap_or_else(|| "Anonymous".to_string());
                out.push_str("<span class=\"saignant-fluct\" data-character-id=\"");
                out.push_str(&fluct.character_id.to_string());
                out.push_str("\">");
                out.push_str(&escape_html(&name));
                out.push('.');
                out.push_str(&escape_html(&fluct.status));
                if fluct.delta >= 0 {
                    out.push('+');
                }
                out.push_str(&fluct.delta.to_string());
                out.push_str("</span>");
            }
            Err(e) => {
                out.push_str("<span class=\"saignant-syntax-error\">");
                out.push_str(&escape_html(&e.to_string()));
                out.push_str("</span>");
            }
        },
        LABEL_ALHALTE_GENERAST_ASSIGN => match extract_assign(tin) {
            Ok(assign) => {
                let name = resolver
                    .resolve_name(assign.character_id)
                    .unwrap_or_else(|| "Anonymous".to_string());
                out.push_str("<span class=\"saignant-assign\" data-character-id=\"");
                out.push_str(&assign.character_id.to_string());
                out.push_str("\">");
                out.push_str(&escape_html(&name));
                out.push('.');
                out.push_str(&escape_html(&assign.status));
                out.push('=');
                out.push_str(&assign.value.to_string());
                out.push_str("</span>");
            }
            Err(e) => {
                out.push_str("<span class=\"saignant-syntax-error\">");
                out.push_str(&escape_html(&e.to_string()));
                out.push_str("</span>");
            }
        },
        LABEL_ALHALTE_GENERAST_SYNTAX_ERROR => {
            let msg = extract_syntax_error(tin).unwrap_or_else(|e| e.to_string());
            out.push_str("<span class=\"saignant-syntax-error\">");
            out.push_str(&escape_html(&msg));
            out.push_str("</span>");
        }
        unknown => {
            out.push_str("<span class=\"saignant-element-unknown\" data-label=\"");
            out.push_str(&unknown.to_string());
            out.push_str("\"></span>");
        }
    }
}
