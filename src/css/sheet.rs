//! Stylesheets and `style` attributes: style rules with their selectors and
//! declarations, parsed with `cssparser`'s rule parsers.
//!
//! A syntax error fails the whole sheet or attribute (bevy_markup's long
//! standing behavior, lightningcss's without error recovery); an invalid
//! value drops only its declaration. At-rules (`@media`, `@font-face`, …)
//! and nested style rules are accepted and skipped: the cascade has no use
//! for them.
//!
//! A declaration's value is parsed once, here, into a [`Decl`] — except one
//! holding `var()`, which keeps its text ([`Raw`]) until the cascade
//! substitutes it per element, and a custom property (`--name`), whose
//! text is what `var()` substitutes.

use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, Delimiter, ParseError, Parser, ParserInput,
    ParserState, QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, StyleSheetParser, Token,
};

use super::property::{self, Decl};
use super::select::{self, SelectorList};
use super::{CssError, describe};

/// A parsed stylesheet: its style rules in source order.
#[derive(Clone, Debug, Default)]
pub(crate) struct Sheet {
    pub(crate) rules: Vec<StyleRule>,
}

/// A style rule.
#[derive(Clone, Debug)]
pub(crate) struct StyleRule {
    pub(crate) selectors: SelectorList,
    pub(crate) declarations: Declarations,
}

/// A declaration block, split by importance (each in source order).
#[derive(Clone, Debug, Default)]
pub(crate) struct Declarations {
    pub(crate) normal: Vec<Declaration>,
    pub(crate) important: Vec<Declaration>,
}

impl Declarations {
    /// Every declaration, normal ones first.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Declaration> {
        self.normal.iter().chain(&self.important)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.normal.is_empty() && self.important.is_empty()
    }
}

/// A value's source text, trimmed (comments kept: the tokenizer skips
/// them when it reads the text again).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Raw(pub(crate) String);

/// One declaration.
#[derive(Clone, Debug)]
pub(crate) enum Declaration {
    /// A parsed value.
    Value(Decl),
    /// `initial`/`inherit`/`unset`/`revert` for the named property.
    Keyword(String),
    /// A value holding `var()`: parsed after substitution, per element.
    /// `refs`: the custom properties it references (fallbacks included),
    /// noted when its text was read.
    Var {
        name: String,
        value: Raw,
        refs: Vec<String>,
    },
    /// A custom property `--name` (its name keeps its case), with the
    /// custom properties its value references.
    Custom {
        name: String,
        value: Raw,
        refs: Vec<String>,
    },
}

/// Parses a stylesheet. `Err` carries a message with the error's position.
pub(crate) fn parse_sheet(source: &str) -> Result<Sheet, String> {
    let mut input = ParserInput::new(source);
    let mut parser = Parser::new(&mut input);
    let mut rules = Vec::new();
    let mut top = TopLevel;
    for item in StyleSheetParser::new(&mut parser, &mut top) {
        match item {
            Ok(Some(rule)) => rules.push(rule),
            Ok(None) => {}
            Err((error, _)) => return Err(describe(&error)),
        }
    }
    Ok(Sheet { rules })
}

/// Parses a `style` attribute's declarations.
pub(crate) fn parse_declarations(text: &str) -> Result<Declarations, String> {
    let mut input = ParserInput::new(text);
    let mut parser = Parser::new(&mut input);
    parse_body(&mut parser).map_err(|error| describe(&error))
}

/// The declarations of one block; nested rules are skipped.
fn parse_body<'i>(
    input: &mut Parser<'i, '_>,
) -> Result<Declarations, ParseError<'i, CssError<'i>>> {
    let mut body = Body::default();
    let mut errors = Vec::new();
    for item in RuleBodyParser::new(input, &mut body) {
        if let Err((error, _)) = item {
            errors.push(error);
        }
    }
    match errors.into_iter().next() {
        Some(error) => Err(error),
        None => Ok(body.declarations),
    }
}

/// The top level of a sheet.
struct TopLevel;

impl<'i> QualifiedRuleParser<'i> for TopLevel {
    type Prelude = SelectorList;
    type QualifiedRule = Option<StyleRule>;
    type Error = CssError<'i>;

    fn parse_prelude<'t>(
        &mut self,
        input: &mut Parser<'i, 't>,
    ) -> Result<SelectorList, ParseError<'i, CssError<'i>>> {
        select::parse_list(input)
    }

    fn parse_block<'t>(
        &mut self,
        selectors: SelectorList,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Option<StyleRule>, ParseError<'i, CssError<'i>>> {
        Ok(Some(StyleRule {
            selectors,
            declarations: parse_body(input)?,
        }))
    }
}

impl<'i> AtRuleParser<'i> for TopLevel {
    type Prelude = ();
    type AtRule = Option<StyleRule>;
    type Error = CssError<'i>;

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<(), ParseError<'i, CssError<'i>>> {
        if name.eq_ignore_ascii_case("font-face") {
            bevy::log::debug!(
                "html css: @font-face is a no-op (register the family in the \
                 `FontFamilies` resource instead)"
            );
        }
        while input.next().is_ok() {}
        Ok(())
    }

    fn rule_without_block(&mut self, _: (), _: &ParserState) -> Result<Option<StyleRule>, ()> {
        Ok(None)
    }

    fn parse_block<'t>(
        &mut self,
        _: (),
        _: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<Option<StyleRule>, ParseError<'i, CssError<'i>>> {
        while input.next().is_ok() {}
        Ok(None)
    }
}

/// A declaration block.
#[derive(Default)]
struct Body {
    declarations: Declarations,
}

impl<'i> DeclarationParser<'i> for Body {
    type Declaration = ();
    type Error = CssError<'i>;

    fn parse_value<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
        _start: &ParserState,
    ) -> Result<(), ParseError<'i, CssError<'i>>> {
        let start = input.position();
        let custom = name.starts_with("--");
        let lower = if custom {
            String::new()
        } else {
            name.to_ascii_lowercase()
        };
        // One read: the keyword check and the property's own parser run on
        // the declaration's tokens. Only a value they refuse (one holding
        // `var()`, or an invalid one) and a custom property's value are
        // read again, as text.
        // Filled only when the value is read as text.
        let mut refs = Vec::new();
        let mut var = false;
        let typed = input.parse_until_before(Delimiter::Bang, |value| {
            if !custom && let Ok(declaration) = value.try_parse(|v| typed(&lower, v)) {
                return Ok::<_, ParseError<'i, CssError<'i>>>(Some(declaration));
            }
            var = read_as_text(value, &mut refs);
            Ok(None)
        })?;
        let declaration = match typed {
            Some(declaration) => declaration,
            None => {
                let text = input.slice_from(start).trim().to_owned();
                if custom {
                    Some(Declaration::Custom {
                        name: name.as_ref().to_owned(),
                        value: Raw(text),
                        refs,
                    })
                } else if var {
                    Some(Declaration::Var {
                        name: lower,
                        value: Raw(text),
                        refs,
                    })
                } else {
                    None
                }
            }
        };
        let important = input.try_parse(cssparser::parse_important).is_ok();
        input.expect_exhausted()?;
        if let Some(declaration) = declaration {
            if important {
                self.declarations.important.push(declaration);
            } else {
                self.declarations.normal.push(declaration);
            }
        }
        Ok(())
    }
}

impl<'i> QualifiedRuleParser<'i> for Body {
    type Prelude = ();
    type QualifiedRule = ();
    type Error = CssError<'i>;

    fn parse_prelude<'t>(
        &mut self,
        input: &mut Parser<'i, 't>,
    ) -> Result<(), ParseError<'i, CssError<'i>>> {
        while input.next().is_ok() {}
        Ok(())
    }

    fn parse_block<'t>(
        &mut self,
        _: (),
        _: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<(), ParseError<'i, CssError<'i>>> {
        while input.next().is_ok() {}
        Ok(())
    }
}

impl<'i> AtRuleParser<'i> for Body {
    type Prelude = ();
    type AtRule = ();
    type Error = CssError<'i>;

    fn parse_prelude<'t>(
        &mut self,
        _name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<(), ParseError<'i, CssError<'i>>> {
        while input.next().is_ok() {}
        Ok(())
    }

    fn rule_without_block(&mut self, _: (), _: &ParserState) -> Result<(), ()> {
        Ok(())
    }

    fn parse_block<'t>(
        &mut self,
        _: (),
        _: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<(), ParseError<'i, CssError<'i>>> {
        while input.next().is_ok() {}
        Ok(())
    }
}

impl<'i> RuleBodyItemParser<'i, (), CssError<'i>> for Body {
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        true
    }
}

/// The value read once more, as text: a custom property's, or a refused
/// one: whether it holds a `var()`, each one's name added to `refs`. Cold:
/// ordinary declarations parse in place, and keeping this out of line keeps
/// their path small.
#[cold]
#[inline(never)]
fn read_as_text(value: &mut Parser<'_, '_>, refs: &mut Vec<String>) -> bool {
    consume_noting_vars(value, refs)
}

/// Consumes the rest of `input`, nested blocks included: whether a
/// `var()` appears, with each one's custom-property name added to `refs`
/// (fallbacks included). The pre-scan bounds the nesting it recurses
/// through.
fn consume_noting_vars(input: &mut Parser<'_, '_>, refs: &mut Vec<String>) -> bool {
    let mut found = false;
    loop {
        let (nested, var) = match input.next_including_whitespace_and_comments() {
            Err(_) => return found,
            Ok(Token::Function(name)) => (true, name.eq_ignore_ascii_case("var")),
            Ok(Token::ParenthesisBlock | Token::SquareBracketBlock | Token::CurlyBracketBlock) => {
                (true, false)
            }
            Ok(_) => (false, false),
        };
        found |= var;
        if nested {
            let _ = input.parse_nested_block(|inner| {
                if var
                    && let Ok(name) =
                        inner.try_parse(|i| i.expect_ident().map(|name| name.as_ref().to_owned()))
                    && name.starts_with("--")
                {
                    refs.push(name);
                }
                found |= consume_noting_vars(inner, refs);
                Ok::<_, ParseError<()>>(())
            });
        }
    }
}

/// A declaration read in place: a CSS-wide keyword or the property's
/// value, filling the whole value. `Ok(None)`: a property bevy_markup
/// doesn't map (dropped).
fn typed<'i>(
    name: &str,
    input: &mut Parser<'i, '_>,
) -> Result<Option<Declaration>, ParseError<'i, CssError<'i>>> {
    let keyword = input.try_parse(|i| {
        let ident = i.expect_ident()?.to_ascii_lowercase();
        i.expect_exhausted()?;
        if matches!(ident.as_str(), "initial" | "inherit" | "unset" | "revert") {
            Ok(())
        } else {
            Err(i.new_custom_error::<_, CssError<'i>>(CssError::Invalid))
        }
    });
    if keyword.is_ok() {
        return Ok(Some(Declaration::Keyword(name.to_owned())));
    }
    let decl = property::parse_in(name, input)?;
    input.expect_exhausted()?;
    Ok(decl.map(Declaration::Value))
}

/// The `url(…)`s at `text`'s top level, as written.
pub(crate) fn urls(text: &str) -> Vec<String> {
    let mut input = ParserInput::new(text);
    let mut parser = Parser::new(&mut input);
    let mut urls = Vec::new();
    loop {
        let Ok(token) = parser.next().cloned() else {
            break;
        };
        match token {
            Token::UnquotedUrl(url) => urls.push(url.as_ref().to_owned()),
            Token::Function(name) if name.eq_ignore_ascii_case("url") => {
                let url = parser.parse_nested_block(|args| {
                    let url = args.expect_string()?.as_ref().to_owned();
                    args.expect_exhausted()?;
                    Ok::<_, ParseError<()>>(url)
                });
                if let Ok(url) = url {
                    urls.push(url);
                }
            }
            _ => {}
        }
    }
    urls
}
