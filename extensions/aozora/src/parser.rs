//! Parsing Aozora Bunko format into Generast syntax trees using `aozora-rs-core`.

use aozora_rs_core::retokenizer::{Retokenized, retokenize};
use aozora_rs_core::scopenizer::scopenize;
use aozora_rs_core::tokenizer::tokenize;
use aozora_rs_core::{Deco, Input};
use generast::Generast;

use crate::error::AozoraError;

/// Parses an Aozora Bunko formatted text string into a [`Generast`] syntax tree.
pub fn parse_aozora(text: &str) -> Result<Generast, AozoraError> {
    if text.is_empty() {
        return Ok(Generast::string(""));
    }

    let mut loc = Input::new(text);
    let tokenized = match tokenize(&mut loc) {
        Ok(t) => t,
        Err(_) => return Ok(Generast::string(text)),
    };

    let ((scopes, flattoken), _) = scopenize(tokenized).into_tuple();
    let (pages, _) = retokenize(flattoken, scopes);

    // If pages are empty, fallback to plain text
    if pages.is_empty() {
        return Ok(Generast::string(text));
    }

    // Build the AST using a stack of frames
    struct Frame<'s> {
        deco: Option<Deco<'s>>,
        children: Vec<Generast>,
    }

    let mut stack: Vec<Frame> = vec![Frame {
        deco: None,
        children: Vec::new(),
    }];

    for page in pages {
        for item in page.content {
            match item {
                Retokenized::Text(t) => {
                    if let Some(top) = stack.last_mut() {
                        top.children.push(Generast::string(t));
                    }
                }
                Retokenized::Br => {
                    if let Some(top) = stack.last_mut() {
                        top.children.push(Generast::string("\n"));
                    }
                }
                Retokenized::Kunten(k) => {
                    if let Some(top) = stack.last_mut() {
                        top.children.push(Generast::string(k));
                    }
                }
                Retokenized::Okurigana(o) => {
                    if let Some(top) = stack.last_mut() {
                        top.children.push(Generast::string(o));
                    }
                }
                Retokenized::Figure(f) => {
                    if let Some(top) = stack.last_mut() {
                        top.children
                            .push(Generast::string(format!("［＃挿絵（{}）入る］", f.path)));
                    }
                }
                Retokenized::DecoBegin(deco) => {
                    stack.push(Frame {
                        deco: Some(deco),
                        children: Vec::new(),
                    });
                }
                Retokenized::DecoEnd(_) => {
                    if stack.len() > 1
                        && let Some(frame) = stack.pop()
                    {
                        let node = if let Some(deco) = frame.deco {
                            convert_deco_to_generast(deco, frame.children)
                        } else if frame.children.len() == 1 {
                            frame
                                .children
                                .into_iter()
                                .next()
                                .unwrap_or(Generast::string(""))
                        } else {
                            Generast::nested(tin::Tin::need_not_know(Vec::new()), frame.children)
                        };
                        if let Some(top) = stack.last_mut() {
                            top.children.push(node);
                        }
                    }
                }
            }
        }
    }

    // Close any unclosed frames
    while stack.len() > 1 {
        if let Some(frame) = stack.pop() {
            let node = if let Some(deco) = frame.deco {
                convert_deco_to_generast(deco, frame.children)
            } else if frame.children.len() == 1 {
                frame
                    .children
                    .into_iter()
                    .next()
                    .unwrap_or(Generast::string(""))
            } else {
                Generast::nested(tin::Tin::need_not_know(Vec::new()), frame.children)
            };
            if let Some(top) = stack.last_mut() {
                top.children.push(node);
            }
        }
    }

    let mut root_children = stack.pop().map(|f| f.children).unwrap_or_default();

    if root_children.len() == 1 {
        Ok(root_children.remove(0))
    } else if root_children.is_empty() {
        Ok(Generast::string(""))
    } else {
        Ok(Generast::nested(
            tin::Tin::need_not_know(Vec::new()),
            root_children,
        ))
    }
}

fn convert_deco_to_generast(deco: Deco, children: Vec<Generast>) -> Generast {
    match deco {
        Deco::Bold => Generast::bold(children),
        Deco::Italic => Generast::italic(children),
        Deco::Ruby(r) => Generast::ruby(r, children),
        Deco::Boten(_) => Generast::boten(children),
        Deco::AHead => Generast::h1(children),
        Deco::BHead => Generast::h2(children),
        Deco::CHead => Generast::h3(children),
        Deco::Indent(n) => Generast::indent(n as u32, children),
        Deco::Hanging((f, r)) => {
            let fallback = children.clone();
            Generast::hanging(f as u32, r as u32, children)
                .unwrap_or_else(|_| Generast::nested(tin::Tin::need_not_know(Vec::new()), fallback))
        }
        Deco::Grounded => Generast::grounded(children),
        Deco::Kerning(k) => Generast::kerning(k as u32, children),
        Deco::HinV => Generast::h_in_v(children),
        _ => Generast::nested(tin::Tin::need_not_know(Vec::new()), children),
    }
}
