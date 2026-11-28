//! `.html` files as MiniJinja templates (ADR 0049), and the parsed result.
//!
//! The loader compiles each file as a MiniJinja (Jinja2) template, so syntax
//! errors fail the load. `.html`/`.htm`/`.md` templates get HTML
//! autoescaping of `{{ }}` values. A plain `.html` file with no template
//! syntax is a template that renders to itself. Every render runs on a
//! fuel budget ([`HtmlTemplateLimits`]): work and memory are bounded per
//! render, so an untrusted template can't stall or exhaust the app.
//!
//! The crate registers a `json` filter ([`new_environment`]): argument attributes
//! (`data-l10n-args`, `data-with`, `data-tooltip-args`) print their value
//! with it, `'{{ {"name": player} | json }}'`.
//!
//! Templates compose. `{% extends "base.html" %}` and `{% include
//! "part.html" %}` name other templates by path relative to the template's
//! own file (like a URL: `../lib/ui.html`, `/ui/lib.html` from the asset
//! root; `embedded://` templates resolve within their source). The loader
//! loads them with it — with Bevy's file watcher, editing one reloads every
//! template using it (a manual `AssetServer::reload` must name the using
//! template: Bevy reloads only the given path) — and
//! compiles them into one environment, so a template can `{% import
//! "ui.html" as ui %}` a macro library and call `{{ ui.button(label="Go")
//! }}` (with a body: `{% call ui.button(label="Go") %}…{% endcall %}`).

use std::borrow::Cow;
use std::fmt::Write;

use bevy::asset::{AssetLoader, AssetPath, LoadContext, io::Reader};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::html::TemplateContext;
use crate::l10n::LocalizedText;
use crate::md;

/// A compiled MiniJinja template loaded from an `.html`/`.htm` file — or a
/// Markdown `.md`/`.markdown` file ([ADR 0014]), whose Markdown converts
/// to HTML once, at asset load (template syntax passes through; ADR 0020).
/// Use it via [`HtmlUi`](crate::html::HtmlUi).
///
/// [ADR 0014]: docs/agents/adr/0014-markdown-via-pulldown-cmark.md
#[derive(Asset, TypePath)]
pub struct HtmlTemplate {
    pub(crate) env: minijinja::Environment<'static>,
    /// Template name inside `env`: the asset path.
    pub(crate) name: String,
}

impl HtmlTemplate {
    /// Renders with `context`, then parses the output into a DOM.
    pub fn render(
        &self,
        context: &TemplateContext,
    ) -> Result<HtmlDocument, Box<dyn std::error::Error + Send + Sync>> {
        let source = self.render_source(context)?;
        Ok(HtmlDocument::parse(source)?)
    }

    /// Renders with `context` to HTML text, without parsing it. A render
    /// past its fuel ([`HtmlTemplateLimits`]) or past
    /// [`MAX_RENDERED_BYTES`] of output fails (bug_0220).
    pub fn render_source(&self, context: &TemplateContext) -> Result<String, minijinja::Error> {
        let template = self.env.get_template(&self.name)?;
        let mut out = BoundedOutput::default();
        let result = template.render_captured_to(context.as_value(), &mut out);
        match result {
            Err(_) if out.over => Err(minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!(
                    "the rendered HTML is larger than MAX_RENDERED_BYTES ({MAX_RENDERED_BYTES} bytes)"
                ),
            )),
            Err(err) => Err(err),
            Ok(_) => String::from_utf8(out.bytes).map_err(|_| {
                minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    "the rendered HTML isn't UTF-8",
                )
            }),
        }
    }

    /// The template's name (its asset path).
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// The most HTML text a template may render, in bytes (16 MiB). Templates
/// are untrusted input (ADR 0044): fuel bounds the work, this the output
/// (two nested loops in 92 bytes once rendered 10¹⁰ characters, bug_0220).
/// A render past it fails with an error saying so, like a template error.
pub const MAX_RENDERED_BYTES: usize = 16 << 20;

/// The render's output, refused past [`MAX_RENDERED_BYTES`] (the output
/// streams, so the render stops at the write that crosses it).
#[derive(Default)]
struct BoundedOutput {
    bytes: Vec<u8>,
    over: bool,
}

impl std::io::Write for BoundedOutput {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len() + buf.len() > MAX_RENDERED_BYTES {
            self.over = true;
            return Err(std::io::ErrorKind::FileTooLarge.into());
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Parsed HTML. Owns its source text.
pub struct HtmlDocument {
    /// Boxed: a `VDomGuard` is ~200 bytes, and `RenderedHtml` holds this.
    dom: Box<tl::VDomGuard>,
    /// The HTML text `dom` was parsed from (`dom` borrows its own leaked copy).
    source: String,
}

impl HtmlDocument {
    pub fn parse(html: String) -> Result<Self, tl::ParseError> {
        let html = match normalize_self_closing(&html) {
            std::borrow::Cow::Borrowed(_) => html,
            std::borrow::Cow::Owned(normalized) => normalized,
        };
        let source = html.clone();
        // SAFETY: `parse_owned` leaks `html` and frees it when the returned
        // `VDomGuard` drops; `VDomGuard::get_ref` ties borrows to the guard.
        let dom = unsafe { tl::parse_owned(html, tl::ParserOptions::default()) }?;
        Ok(Self {
            dom: Box::new(dom),
            source,
        })
    }

    /// The HTML text this document was parsed from.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The parsed DOM.
    pub fn dom(&self) -> &tl::VDom<'_> {
        self.dom.get_ref()
    }

    /// Indented outline of the DOM: one line per element (with attributes),
    /// text node, and comment. Whitespace-only text is skipped. Elements in
    /// `localized` show their translation (`l10n "…"`) instead of children.
    /// Stops at 1 MiB with a closing note: indentation makes an outline
    /// quadratic in the document's depth (bug_0244).
    pub fn outline(&self, localized: &LocalizedText) -> String {
        let dom = self.dom();
        let parser = dom.parser();
        let mut out = String::new();
        let mut writer = OutlineWriter(&mut out);
        let complete = dom
            .children()
            .iter()
            .all(|handle| write_node(&mut writer, parser, &localized.0, *handle).is_ok());
        if !complete {
            out.push_str(OUTLINE_TRUNCATED);
        }
        out
    }
}

/// `tl` leaves character references as written: decode them, so text
/// reads as authored (ADR 0036, the spec's algorithm via `htmlize`). One
/// level: a decoded `&` is not re-scanned (`&amp;lt;` reads back `&lt;`);
/// an unknown reference stays verbatim; legacy names decode without `;`
/// (`&copy2` → "©2"), as in a browser. Borrows when there's nothing to
/// decode.
pub(crate) fn decode_text<'a>(text: impl Into<Cow<'a, str>>) -> Cow<'a, str> {
    htmlize::unescape(text)
}

/// [`decode_text`] for an attribute value: the spec's attribute rule keeps
/// a legacy name followed by an alphanumeric or `=` verbatim, so a query
/// string like `?a=1&copy=2` survives.
pub(crate) fn decode_attribute<'a>(value: impl Into<Cow<'a, str>>) -> Cow<'a, str> {
    htmlize::unescape_attribute(value)
}
/// The template environment every load builds (and the fuzz harness, so it
/// compiles templates exactly as the loader does): HTML autoescaping for
/// the markup extensions; the crate's `json` filter, which prints a value
/// as JSON for the argument attributes (`data-l10n-args`, `data-with`,
/// `data-tooltip-args`): `'{{ {"name": player} | json }}'`; a fuel budget
/// per render (`fuel`, see [`HtmlTemplateLimits`]); Tera's handling of
/// undefined values (printing or iterating one is an error, testing it in
/// `{% if %}` is false); and the source's trailing newline kept.
pub fn new_environment(fuel: u64) -> minijinja::Environment<'static> {
    let mut env = minijinja::Environment::new();
    env.set_auto_escape_callback(|name| {
        if [".html", ".htm", ".xml", ".md", ".markdown"]
            .iter()
            .any(|extension| name.ends_with(extension))
        {
            minijinja::AutoEscape::Html
        } else {
            minijinja::AutoEscape::None
        }
    });
    env.set_undefined_behavior(minijinja::UndefinedBehavior::SemiStrict);
    if let Ok(syntax) = minijinja::syntax::SyntaxConfig::builder()
        .keep_trailing_newline(true)
        .build()
    {
        env.set_syntax(syntax);
    }
    env.add_filter("json", json_filter);
    // Filters that build a string or list from their input charge the
    // result's size up front, before building it.
    env.add_filter("join", fueled_join);
    env.add_filter("split", fueled_split);
    env.add_filter("lines", fueled_lines);
    env.add_filter("escape", fueled_escape);
    env.add_filter("e", fueled_escape);
    env.add_filter("format", checked_format);
    // Filters that write a value's text into a string check that it could
    // be printed and charge its estimated bytes against remaining fuel first
    // (bug_0253, bug_0256, bug_0258).
    env.add_filter("string", checked_string);
    env.add_filter("tojson", checked_tojson);
    env.add_filter("pprint", checked_pprint);
    env.add_filter("safe", checked_safe);
    env.set_formatter(streaming_formatter);
    env.set_fuel(Some(fuel));
    env
}

/// Prints a value into the render's output as MiniJinja's own formatter
/// does, but bounded by the output's limit ([`MAX_RENDERED_BYTES`]) before
/// the work: a list or map (or a lazily repeated one, up to 10⁸ items,
/// nested) holding more items than could fit is refused without printing
/// ([`fits_printed`]), and one printed with HTML escaping is escaped as it
/// is written instead of being written into a string first.
fn streaming_formatter(
    out: &mut minijinja::Output,
    state: &mut minijinja::State,
    value: &minijinja::Value,
) -> Result<(), minijinja::Error> {
    if value.is_safe() || value.as_object().is_none() {
        return minijinja::escape_formatter(out, state, value);
    }
    check_printable(value)?;
    if !matches!(state.auto_escape(), minijinja::AutoEscape::Html) {
        return minijinja::escape_formatter(out, state, value);
    }
    /// Escapes each piece of a value's text on its way to the output.
    struct Escaping<'a, 'b>(&'a mut minijinja::Output<'b>);
    impl std::fmt::Write for Escaping<'_, '_> {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            write!(self.0, "{}", minijinja::HtmlEscape(text))
        }
    }
    use std::fmt::Write as _;
    write!(Escaping(out), "{value}").map_err(|_| {
        minijinja::Error::new(
            minijinja::ErrorKind::WriteFailure,
            "the value doesn't fit in the output",
        )
    })
}

/// Refuses a value whose text couldn't fit in the output ([`fits_printed`]):
/// printing it, or a filter writing it into a string, would iterate all of
/// it first.
fn check_printable(value: &minijinja::Value) -> Result<(), minijinja::Error> {
    check_printable_indented(value, 0)
}

/// [`check_printable`] for a value printed as JSON indented by `indent`
/// spaces a level (`tojson(indent)`).
fn check_printable_indented(value: &minijinja::Value, indent: u64) -> Result<(), minijinja::Error> {
    if fits_printed(value, indent) {
        Ok(())
    } else {
        Err(minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!(
                "the value holds too many items to print \
                 (MAX_RENDERED_BYTES is {MAX_RENDERED_BYTES} bytes)"
            ),
        ))
    }
}

/// Escaping growth model when estimating serialized byte sizes before
/// allocating (`estimate_printed_bytes`).
#[derive(Clone, Copy)]
enum EscapeMode {
    Plain,
    Json,
    Html,
}

/// Estimates how many bytes `value`'s serialized form will take, stopping
/// early and returning `None` as soon as the running total passes `limit`.
///
/// Counts:
/// - container item overhead (`3 + 2 * indent * depth` bytes per nested item),
///   rejecting sequences/maps whose known length alone exceeds the budget in
///   O(1) without iterating (bug_0253, bug_0256);
/// - string and byte-slice lengths inside containers, plus JSON or HTML
///   escape expansion, so `[s] * N` is refused before allocating (bug_0258).
///
/// Iterative: values nest arbitrarily deep.
fn estimate_printed_bytes(
    value: &minijinja::Value,
    indent: u64,
    escape: EscapeMode,
    limit: u64,
) -> Option<u64> {
    use minijinja::value::ObjectRepr;
    let cost = |depth: usize| 3u64.saturating_add(indent.saturating_mul(2 * depth as u64));
    let mut used = 0u64;
    // Pin up to 16 large strings (MiniJinja stores them as Arc<str>) so a
    // lazily repeated `[s] * N` scans `s`'s escape delta only once while
    // guaranteeing its address cannot be reused for the duration of this walk.
    let mut escape_cache: Vec<(usize, usize, u64, minijinja::Value)> = Vec::new();
    let mut pending: Vec<Box<dyn Iterator<Item = minijinja::Value>>> =
        vec![Box::new(std::iter::once(value.clone()))];
    while let Some(items) = pending.last_mut() {
        let Some(item) = items.next() else {
            pending.pop();
            continue;
        };
        let depth = pending.len() - 1;
        if depth > 0 || item.as_object().is_some() {
            used = used.saturating_add(cost(depth));
            if used > limit {
                return None;
            }
        }
        if let Some(text) = item.as_str() {
            used = used.saturating_add(text.len() as u64);
            if used > limit {
                return None;
            }
            if !matches!(escape, EscapeMode::Plain) && !text.is_empty() {
                let key = (text.as_ptr() as usize, text.len());
                let cached = escape_cache
                    .iter()
                    .find(|(ptr, len, _, _)| *ptr == key.0 && *len == key.1)
                    .map(|(_, _, extra, _)| *extra);
                if let Some(extra) = cached {
                    used = used.saturating_add(extra);
                    if used > limit {
                        return None;
                    }
                } else {
                    let mut extra_total = 0u64;
                    match escape {
                        EscapeMode::Plain => {}
                        EscapeMode::Html => {
                            for byte in text.bytes() {
                                let extra = match byte {
                                    b'"' | b'\'' | b'/' => 5,
                                    b'<' | b'>' => 3,
                                    b'&' => 4,
                                    _ => 0,
                                };
                                if extra > 0 {
                                    extra_total = extra_total.saturating_add(extra);
                                    if used.saturating_add(extra_total) > limit {
                                        return None;
                                    }
                                }
                            }
                        }
                        EscapeMode::Json => {
                            for byte in text.bytes() {
                                let extra = match byte {
                                    b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 0x08 | 0x0c => 1,
                                    0x00..=0x1f => 5,
                                    _ => 0,
                                };
                                if extra > 0 {
                                    extra_total = extra_total.saturating_add(extra);
                                    if used.saturating_add(extra_total) > limit {
                                        return None;
                                    }
                                }
                            }
                        }
                    }
                    used = used.saturating_add(extra_total);
                    if text.len() >= 128 && escape_cache.len() < 16 {
                        escape_cache.push((key.0, key.1, extra_total, item));
                    }
                }
            }
            continue;
        }
        if let Some(bytes) = item.as_bytes() {
            used = used.saturating_add(bytes.len() as u64);
            if used > limit {
                return None;
            }
            continue;
        }
        let Some(object) = item.as_object() else {
            continue;
        };
        let mult = if matches!(object.repr(), ObjectRepr::Map) {
            2u64
        } else {
            1u64
        };
        if object.enumerator_len().is_some_and(|len| {
            (len as u64)
                .saturating_mul(mult)
                .saturating_mul(cost(depth + 1))
                > limit.saturating_sub(used)
        }) {
            return None;
        }
        match object.repr() {
            ObjectRepr::Map => {
                if let Some(pairs) = object.try_iter_pairs() {
                    pending.push(Box::new(pairs.flat_map(|(key, value)| [key, value])));
                }
            }
            // MiniJinja prints a list without a known length by its
            // `Debug`, never iterating it.
            ObjectRepr::Seq | ObjectRepr::Iterable if object.enumerator_len().is_some() => {
                if let Some(iter) = object.try_iter() {
                    pending.push(Box::new(iter));
                }
            }
            _ => {}
        }
    }
    Some(used)
}

/// Whether `value`'s printed form could fit in [`MAX_RENDERED_BYTES`].
fn fits_printed(value: &minijinja::Value, indent: u64) -> bool {
    estimate_printed_bytes(value, indent, EscapeMode::Plain, MAX_RENDERED_BYTES as u64).is_some()
}

/// Maximum serialized bytes a single filter call may allocate: the smaller
/// of [`MAX_RENDERED_BYTES`] and what the remaining fuel can pay for.
fn serialized_byte_limit(state: &minijinja::State) -> (u64, bool) {
    let max = MAX_RENDERED_BYTES as u64;
    match state.fuel_levels() {
        Some((_, remaining)) => {
            let fuel_bytes = remaining.saturating_mul(64).saturating_add(63);
            if fuel_bytes < max {
                (fuel_bytes, true)
            } else {
                (max, false)
            }
        }
        None => (max, false),
    }
}

/// Fails with `OutOfFuel` when fuel was the tighter limit, else
/// `InvalidOperation` naming `MAX_RENDERED_BYTES`.
fn over_serialization_limit(
    state: &minijinja::State,
    fuel_limited: bool,
) -> minijinja::Error {
    if fuel_limited
        && let Some((_, remaining)) = state.fuel_levels()
        && let Err(err) = state.consume_fuel(remaining.saturating_add(1))
    {
        return err;
    }
    minijinja::Error::new(
        minijinja::ErrorKind::InvalidOperation,
        format!(
            "the value holds too many items or bytes to print \
             (MAX_RENDERED_BYTES is {MAX_RENDERED_BYTES} bytes)"
        ),
    )
}

/// Estimates `value`'s serialized size against both remaining fuel and
/// [`MAX_RENDERED_BYTES`] and charges fuel *before* the serializer allocates
/// (bug_0258).
fn charge_serialized_value(
    state: &minijinja::State,
    value: &minijinja::Value,
    indent: u64,
    escape: EscapeMode,
) -> Result<(), minijinja::Error> {
    let (limit, fuel_limited) = serialized_byte_limit(state);
    let Some(bytes) = estimate_printed_bytes(value, indent, escape, limit) else {
        return Err(over_serialization_limit(state, fuel_limited));
    };
    charge_bytes(state, bytes)
}

/// `string`, bounded and pre-charged before serializing.
fn checked_string(
    state: &minijinja::State,
    value: &minijinja::Value,
) -> Result<minijinja::Value, minijinja::Error> {
    charge_serialized_value(state, value, 0, EscapeMode::Plain)?;
    minijinja::filters::string(state, value)
}

/// `safe`, bounded and pre-charged before converting a non-safe value to a
/// `String`.
fn checked_safe(
    state: &minijinja::State,
    value: &minijinja::Value,
) -> Result<minijinja::Value, minijinja::Error> {
    if value.is_safe() {
        return Ok(value.clone());
    }
    charge_serialized_value(state, value, 0, EscapeMode::Plain)?;
    Ok(minijinja::Value::from_safe_string(value.to_string()))
}

/// `tojson`, bounded and pre-charged before serializing.
///
/// The indent is MiniJinja's (`true` = 2, `false` = none, else a `usize`,
/// positional or `indent=`): unbounded there, it writes `indent × depth`
/// spaces per item (UPSTREAM U64), so it counts toward the size.
fn checked_tojson(
    state: &minijinja::State,
    value: &minijinja::Value,
    indent: Option<minijinja::Value>,
    args: minijinja::value::Kwargs,
) -> Result<minijinja::Value, minijinja::Error> {
    let spaces = match indent
        .clone()
        .or(args.peek::<Option<minijinja::Value>>("indent")?)
    {
        None => 0,
        Some(value) => match bool::try_from(value.clone()) {
            Ok(true) => 2,
            Ok(false) => 0,
            // MiniJinja refuses it with its own error.
            Err(_) => usize::try_from(value).map_or(0, |n| n as u64),
        },
    };
    charge_serialized_value(state, value, spaces, EscapeMode::Json)?;
    minijinja::filters::tojson(value, indent, args)
}

/// `pprint`, bounded and pre-charged before serializing.
fn checked_pprint(
    state: &minijinja::State,
    value: &minijinja::Value,
) -> Result<String, minijinja::Error> {
    charge_serialized_value(state, value, 2, EscapeMode::Plain)?;
    Ok(minijinja::filters::pprint(value))
}

/// Bytes charged per item a filter creates: a `Value` slot and a small
/// allocation (a slot is 24 bytes).
const ITEM_BYTES: u64 = 48;

/// Charges `bytes` of a result about to be built (fuel's resolution: one
/// unit per 64 bytes, as MiniJinja charges data).
fn charge_bytes(state: &minijinja::State, bytes: u64) -> Result<(), minijinja::Error> {
    state.consume_fuel(bytes / 64)
}

/// `join`, charged for its result (including nested non-string items) before
/// building it (bug_0258).
fn fueled_join(
    state: &mut minijinja::State,
    value: &minijinja::Value,
    joiner: Option<minijinja::value::StringInput<'_>>,
    attribute: Option<&str>,
    kwargs: minijinja::value::Kwargs,
) -> Result<minijinja::Value, minijinja::Error> {
    if value.as_str().is_some() || value.is_undefined() || value.is_none() {
        return minijinja::filters::join(state, value, joiner, attribute, kwargs);
    }
    // Collected once: an iterator (`map`'s, `select`'s) can only be walked
    // once, and the built-in walks what it is given. The collection is
    // charged first when its length is known (a lazily repeated list can
    // hold 10⁸ items).
    if let Some(len) = value.len() {
        charge_bytes(state, (len as u64).saturating_mul(ITEM_BYTES))?;
    }
    let items: Vec<minijinja::Value> = value.try_iter()?.collect();
    let (limit, fuel_limited) = serialized_byte_limit(state);
    let joiner_bytes = joiner.as_ref().map_or(0, |joiner| joiner.as_str().len()) as u64;
    let mut total = joiner_bytes.saturating_mul(items.len().saturating_sub(1) as u64);
    if total > limit {
        return Err(over_serialization_limit(state, fuel_limited));
    }
    for item in &items {
        let target = match attribute {
            Some(attr) => item.get_attr(attr).ok(),
            None => None,
        };
        let target = target.as_ref().unwrap_or(item);
        let bytes = if let Some(text) = target.as_str() {
            text.len() as u64
        } else {
            let Some(est) = estimate_printed_bytes(
                target,
                0,
                EscapeMode::Plain,
                limit.saturating_sub(total),
            ) else {
                return Err(over_serialization_limit(state, fuel_limited));
            };
            est.max(ITEM_BYTES)
        };
        total = total.saturating_add(bytes);
        if total > limit {
            return Err(over_serialization_limit(state, fuel_limited));
        }
    }
    charge_bytes(state, total)?;
    minijinja::filters::join(
        state,
        &minijinja::Value::from(items),
        joiner,
        attribute,
        kwargs,
    )
}

/// `split`, charged for its pieces before building them.
fn fueled_split(
    state: &minijinja::State,
    value: &minijinja::Value,
    split: Option<std::sync::Arc<str>>,
    maxsplits: Option<i64>,
) -> Result<minijinja::Value, minijinja::Error> {
    if let Some(text) = value.as_str() {
        let pieces = match split.as_deref() {
            Some("") | None => text.split_whitespace().count(),
            Some(separator) => text.matches(separator).count() + 1,
        };
        let pieces = match maxsplits {
            Some(max) if max >= 0 => pieces.min(max as usize + 1),
            _ => pieces,
        } as u64;
        charge_bytes(
            state,
            (text.len() as u64).saturating_add(pieces.saturating_mul(ITEM_BYTES)),
        )?;
    }
    minijinja::filters::split(value, split, maxsplits)
}

/// `lines`, charged for its lines before building them.
fn fueled_lines(
    state: &minijinja::State,
    value: &minijinja::Value,
) -> Result<minijinja::Value, minijinja::Error> {
    if let Some(text) = value.as_str() {
        let lines = text.bytes().filter(|byte| *byte == b'\n').count() as u64 + 1;
        charge_bytes(
            state,
            (text.len() as u64).saturating_add(lines.saturating_mul(ITEM_BYTES)),
        )?;
    }
    minijinja::filters::lines(value)
}

/// `escape`, charged for its escaped text (including non-string containers)
/// before building it: HTML escaping turns one byte into up to six
/// (`"` → `&quot;`, bug_0258).
fn fueled_escape(
    state: &mut minijinja::State,
    value: &minijinja::Value,
) -> Result<minijinja::Value, minijinja::Error> {
    if value.is_safe() {
        return Ok(value.clone());
    }
    charge_serialized_value(state, value, 0, EscapeMode::Html)?;
    minijinja::filters::escape(state, value)
}

/// `format`, checked before MiniJinja parses the format string
/// ([`check_format`]): `%(key)` names are ASCII, widths and precisions at
/// most [`MAX_FORMAT_NUMBER`], and total formatted bytes (specifier widths
/// plus serialized argument sizes × repetition counts) are budgeted before
/// formatting (bug_0258).
fn checked_format(
    state: &mut minijinja::State,
    format: &minijinja::Value,
    args: minijinja::value::Rest<minijinja::value::ValueOrKwargs>,
) -> Result<minijinja::Value, minijinja::Error> {
    if let Some(text) = format.as_str() {
        let budget = match check_format(text) {
            Ok(budget) => budget,
            Err(reason) => {
                return Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    format!("format: {reason}"),
                ));
            }
        };
        let (limit, fuel_limited) = serialized_byte_limit(state);
        let mut total = budget.min_bytes;
        if total > limit {
            return Err(over_serialization_limit(state, fuel_limited));
        }
        let mut pos_index = 0u64;
        for arg in args.iter() {
            let value: &minijinja::Value = arg;
            if !budget.named_specs.is_empty()
                && (value.is_kwargs() || value.kind() == minijinja::value::ValueKind::Map)
            {
                for (key, count) in &budget.named_specs {
                    let item = value
                        .get_item(&minijinja::Value::from(key.as_str()))
                        .ok()
                        .filter(|v| !v.is_undefined())
                        .or_else(|| value.get_attr(key).ok().filter(|v| !v.is_undefined()));
                    if let Some(item) = item {
                        let rem = limit.saturating_sub(total);
                        let Some(est) = estimate_printed_bytes(
                            &item,
                            0,
                            EscapeMode::Plain,
                            rem / (*count).max(1),
                        ) else {
                            return Err(over_serialization_limit(state, fuel_limited));
                        };
                        total = total.saturating_add(est.saturating_mul(*count));
                        if total > limit {
                            return Err(over_serialization_limit(state, fuel_limited));
                        }
                    }
                }
            }
            if !value.is_kwargs() {
                if pos_index < budget.positional_specs {
                    let rem = limit.saturating_sub(total);
                    let Some(est) =
                        estimate_printed_bytes(value, 0, EscapeMode::Plain, rem)
                    else {
                        return Err(over_serialization_limit(state, fuel_limited));
                    };
                    total = total.saturating_add(est);
                    if total > limit {
                        return Err(over_serialization_limit(state, fuel_limited));
                    }
                }
                pos_index += 1;
            }
        }
        charge_bytes(state, total)?;
    }
    minijinja::filters::format(state, format, args)
}

/// The largest width or precision `format` takes: Rust's own formatting
/// takes no larger one.
const MAX_FORMAT_NUMBER: u64 = u16::MAX as u64;

/// Size summary collected by [`check_format`].
struct FormatBudget {
    min_bytes: u64,
    positional_specs: u64,
    named_specs: std::collections::HashMap<String, u64>,
}

/// Checks a printf-style format string's conversion specifiers: every
/// `%(key)` name (up to its `)`, or the end) is ASCII, and every width and
/// precision is at most [`MAX_FORMAT_NUMBER`]. `%%` is a literal percent
/// sign. Returns the minimum output bytes from literals and widths/precisions
/// plus specifier reference counts.
fn check_format(text: &str) -> Result<FormatBudget, &'static str> {
    let bytes = text.as_bytes();
    // The number starting at `at` (saturating) and the index after it.
    let number = |mut at: usize| {
        let mut value = 0u64;
        while let Some(digit) = bytes.get(at).filter(|byte| byte.is_ascii_digit()) {
            value = value
                .saturating_mul(10)
                .saturating_add(u64::from(digit - b'0'));
            at += 1;
        }
        (value, at)
    };
    let mut min_bytes = text.len() as u64;
    let mut positional_specs = 0u64;
    let mut named_specs: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'%' {
            at += 1;
            continue;
        }
        at += 1;
        if bytes.get(at) == Some(&b'%') {
            at += 1;
            continue;
        }
        let mut named = None;
        if bytes.get(at) == Some(&b'(') {
            let key = &bytes[at + 1..];
            let end = key.iter().position(|&b| b == b')').unwrap_or(key.len());
            if !key[..end].is_ascii() {
                return Err("a %(key) name must be ASCII");
            }
            if let Ok(name) = std::str::from_utf8(&key[..end]) {
                named = Some(name.to_owned());
            }
            at += 1 + end + 1;
        }
        while matches!(bytes.get(at), Some(b'#' | b'0' | b'-' | b' ' | b'+')) {
            at += 1;
        }
        let (width, after) = number(at);
        at = after;
        let mut precision = 0;
        if bytes.get(at) == Some(&b'.') {
            (precision, at) = number(at + 1);
        }
        if width > MAX_FORMAT_NUMBER || precision > MAX_FORMAT_NUMBER {
            return Err("a width or precision is over 65535");
        }
        min_bytes = min_bytes.saturating_add(width.max(precision));
        if let Some(name) = named {
            *named_specs.entry(name).or_insert(0) += 1;
        } else {
            positional_specs = positional_specs.saturating_add(1);
        }
    }
    Ok(FormatBudget {
        min_bytes,
        positional_specs,
        named_specs,
    })
}

/// The template limits an app may set (ADR 0049): how much work and
/// memory one render may use. Every render is charged fuel: one unit per
/// instruction, one per 64 bytes of strings, lists and maps it produces,
/// per item a filter visits, per sort comparison. A render that runs out
/// fails like a template error, and the UI shows the failure. Templates
/// are untrusted input (ADR 0044): fuel is what bounds them.
///
/// Read when a template loads: set it before loading templates (a change
/// applies to templates loaded or reloaded afterwards).
#[derive(Resource, Clone, Debug, PartialEq, Reflect)]
pub struct HtmlTemplateLimits {
    /// Fuel per render. Default 2²² (≈ 4.2 M): at most ~250 MB of data
    /// produced and ~0.1 s of work per render, about 40× what the largest
    /// example template needs.
    pub fuel: u64,
}

impl Default for HtmlTemplateLimits {
    fn default() -> Self {
        Self { fuel: 1 << 22 }
    }
}

/// The fuel the loaders compile templates with: [`HtmlTemplateLimits`],
/// copied each frame by `sync_template_fuel` (loaders run off the ECS).
#[derive(Resource, Clone, Debug)]
pub(crate) struct TemplateFuel(pub(crate) std::sync::Arc<std::sync::atomic::AtomicU64>);

impl Default for TemplateFuel {
    fn default() -> Self {
        Self(std::sync::Arc::new(std::sync::atomic::AtomicU64::new(
            HtmlTemplateLimits::default().fuel,
        )))
    }
}

impl TemplateFuel {
    pub(crate) fn get(&self) -> u64 {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// Compiles `source` as template `name` (the loader's environment, default
/// fuel) and renders it with `context` (tests).
#[cfg(test)]
pub(crate) fn test_render(
    name: &str,
    source: &str,
    context: serde_json::Value,
) -> Result<String, minijinja::Error> {
    let mut env = new_environment(HtmlTemplateLimits::default().fuel);
    env.add_template_owned(name.to_owned(), source.to_owned())?;
    let template = HtmlTemplate {
        env,
        name: name.to_owned(),
    };
    template.render_source(&TemplateContext::from_serialize(&context))
}

/// Copies a changed [`HtmlTemplateLimits`] to the loaders.
pub(crate) fn sync_template_fuel(limits: Res<HtmlTemplateLimits>, fuel: Res<TemplateFuel>) {
    if limits.is_changed() {
        fuel.0
            .store(limits.fuel, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Immediately copies [`HtmlTemplateLimits`] to [`TemplateFuel`] during
/// plugin `finish` so `Startup` template loads see resources inserted during
/// app setup (bug_0259).
pub(crate) fn sync_template_fuel_now(world: &World) {
    if let (Some(limits), Some(fuel)) = (
        world.get_resource::<HtmlTemplateLimits>(),
        world.get_resource::<TemplateFuel>(),
    ) {
        fuel.0
            .store(limits.fuel, std::sync::atomic::Ordering::Relaxed);
    }
}

/// The operator bound of one Tera tag (see [`tera_tag_over_limit`]):
/// the studio's busiest tag holds 12 (a `ui.button` call with its
/// arguments); a debug build overflows a 2 MB thread from 136 (bug_0182).
/// 64 operators in the innermost of 39 nested blocks, Tera's own block
/// limit, fit a 2 MB debug thread.
pub(crate) const MAX_TERA_TAG_OPERATORS: usize = 64;

/// The operator characters a Tera tag is counted by: every binary and
/// unary operator, `|` filters, `.`/`[` access, `?` optional access, calls
/// and groups, array and map literals, comparisons and component calls
/// (`<`).
const TERA_OPERATOR_BYTES: &[u8] = b"+-*/%~|.[({<>=!?";

/// The Tera keywords that build an expression node: logic, `in`, tests,
/// ternaries and comprehensions.
const TERA_OPERATOR_WORDS: [&[u8]; 8] =
    [b"and", b"or", b"not", b"in", b"is", b"if", b"else", b"for"];

/// Whether a `{{ … }}` or `{% … %}` tag of `source` holds more than
/// [`MAX_TERA_TAG_OPERATORS`] operators. Tera compiles an operator chain
/// (`a - b - c …`, `a | f | g …`, `a.b.c …`) with one stack frame per
/// operator, and its own cap (256 per expression) is sized for an 8 MB
/// main thread: a few hundred bytes of template abort a 2 MB thread in a
/// debug build at load (bug_0182, UPSTREAM.md U16). A rejected template
/// fails its load before Tera sees it.
///
/// The count can only be too high: every operator character and keyword
/// of a tag counts once (`==` twice, a float's `.` too), whatever node it
/// builds. The scan mirrors Tera's lexer where a mistake could hide an
/// operator: strings (with their escapes) are skipped, a tag ends only at
/// its own closer, and `{# … #}` comments and `{% raw %}` bodies are text.
/// Tera compiles only a template that parses, so the scan only has to be
/// right on templates that do.
pub(crate) fn tera_tag_over_limit(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        match &bytes[i..i + 2] {
            b"{#" => {
                i = source[i + 2..]
                    .find("#}")
                    .map_or(bytes.len(), |end| i + 2 + end + 2);
            }
            b"{%"
                if tera_skip_tag(
                    bytes,
                    i + 2 + usize::from(bytes.get(i + 2) == Some(&b'-')),
                    b"raw",
                )
                .is_some() =>
            {
                // A raw body is text to Tera, up to the first `{%` that
                // is an `endraw` tag. Without one, the template fails to
                // parse and nothing compiles.
                let mut from = i + 2;
                loop {
                    let Some(start) = source[from..].find("{%") else {
                        return false;
                    };
                    let tag = from + start + 2;
                    if let Some(end) = tera_skip_tag(bytes, tag, b"endraw") {
                        i = end;
                        break;
                    }
                    from = tag;
                }
            }
            opener @ (b"{{" | b"{%") => {
                // A tag ends only at its own closer: `}}` closes a map
                // literal inside `{% … %}`, as in Tera's lexer.
                let closer = if opener == b"{{" { b'}' } else { b'%' };
                let (end, operators) = tera_tag_operators(bytes, i + 2, closer);
                if operators > MAX_TERA_TAG_OPERATORS {
                    return true;
                }
                i = end;
            }
            _ => i += 1,
        }
    }
    false
}

/// The operators of the tag whose body starts at `i`, and where the tag
/// ends: after `closer` followed by `}` (`}}` or `%}`), or at the end of
/// the text.
fn tera_tag_operators(bytes: &[u8], mut i: usize, closer: u8) -> (usize, usize) {
    let mut operators = 0;
    while i < bytes.len() {
        match bytes[i] {
            byte if byte == closer && bytes.get(i + 1) == Some(&b'}') => {
                return (i + 2, operators);
            }
            // A string, with Tera's backslash escapes, ends at its quote.
            quote @ (b'"' | b'\'' | b'`') => {
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                operators += usize::from(TERA_OPERATOR_WORDS.contains(&&bytes[start..i]));
            }
            byte => {
                operators += usize::from(TERA_OPERATOR_BYTES.contains(&byte));
                i += 1;
            }
        }
    }
    (i, operators)
}

/// Tera's `skip_tag`: whether the text at `i` (just after `{%`) is
/// `-? {name} -?%}`, with ASCII whitespace around `name`; and where that
/// tag ends.
fn tera_skip_tag(bytes: &[u8], mut i: usize, name: &[u8]) -> Option<usize> {
    let skip_ws = |mut i: usize| {
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        i
    };
    i += usize::from(bytes.get(i) == Some(&b'-'));
    i = skip_ws(i);
    if !bytes[i.min(bytes.len())..].starts_with(name) {
        return None;
    }
    i = skip_ws(i + name.len());
    i += usize::from(bytes.get(i) == Some(&b'-'));
    bytes[i.min(bytes.len())..]
        .starts_with(b"%}")
        .then_some(i + 2)
}

/// [`tera_tag_over_limit`] as the error a load fails with.
pub(crate) fn check_tera_operators(name: &str, source: &str) -> Result<(), String> {
    if tera_tag_over_limit(source) {
        Err(format!(
            "{name}: a template tag holds more than {MAX_TERA_TAG_OPERATORS} operators"
        ))
    } else {
        Ok(())
    }
}

/// The `json` filter: the value as compact JSON (escaped like any output),
/// bounded and pre-charged before serializing (bug_0258).
fn json_filter(
    state: &minijinja::State,
    value: minijinja::Value,
) -> Result<String, minijinja::Error> {
    charge_serialized_value(state, &value, 0, EscapeMode::Json)?;
    serde_json::to_string(&value).map_err(|err| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("json: {err}"),
        )
    })
}

/// Parses an argument attribute's JSON (`data-l10n-args`, `data-with`,
/// `data-tooltip-args`), as printed by the `json` filter.
pub(crate) fn parse_attribute_json(text: &str) -> serde_json::Result<serde_json::Value> {
    serde_json::from_str(text)
}

/// `err: source: source…` — the template engine puts detail in the sources.
pub(crate) fn error_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut message = err.to_string();
    let mut source = err.source();
    while let Some(err) = source {
        let _ = write!(message, ": {err}");
        source = err.source();
    }
    message
}

/// The most text a DOM outline holds, before [`OUTLINE_TRUNCATED`].
const MAX_OUTLINE_BYTES: usize = 1 << 20;

/// Closes an outline cut at [`MAX_OUTLINE_BYTES`].
const OUTLINE_TRUNCATED: &str = "\n… outline truncated\n";

/// A `String` that takes [`MAX_OUTLINE_BYTES`], then fails the write.
struct OutlineWriter<'a>(&'a mut String);

impl Write for OutlineWriter<'_> {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        let room = MAX_OUTLINE_BYTES.saturating_sub(self.0.len());
        if text.len() <= room {
            self.0.push_str(text);
            return Ok(());
        }
        self.0.push_str(&text[..text.floor_char_boundary(room)]);
        Err(std::fmt::Error)
    }
}

/// Two spaces per level; the writer's limit ends a deep indent early.
fn write_indent(out: &mut OutlineWriter, depth: usize) -> std::fmt::Result {
    for _ in 0..depth {
        out.write_str("  ")?;
    }
    Ok(())
}

/// The DOM outline (depth-first, document order), iterative: the
/// document's depth costs heap, not stack. Stack entries carry the depth,
/// not the indent text, which would cost depth × width (bug_0244).
fn write_node(
    out: &mut OutlineWriter,
    parser: &tl::Parser,
    localized: &HashMap<tl::NodeHandle, Result<String, String>>,
    handle: tl::NodeHandle,
) -> std::fmt::Result {
    let mut stack = vec![(handle, 0usize)];
    while let Some((handle, depth)) = stack.pop() {
        let Some(node) = handle.get(parser) else {
            continue;
        };
        match node {
            tl::Node::Tag(tag) => {
                write_indent(out, depth)?;
                write!(out, "{}", tag.name().as_utf8_str())?;
                for (key, value) in tag.attributes().iter() {
                    match value {
                        Some(value) => write!(out, " {key}=\"{value}\"")?,
                        None => write!(out, " {key}")?,
                    }
                }
                out.write_char('\n')?;
                match localized.get(&handle) {
                    Some(Ok(text)) => {
                        write_indent(out, depth + 1)?;
                        writeln!(out, "l10n {text:?}")?;
                    }
                    Some(Err(err)) => {
                        write_indent(out, depth + 1)?;
                        writeln!(out, "l10n error: {err}")?;
                    }
                    None => {
                        // Reversed: the leftmost child pops first.
                        let children: Vec<tl::NodeHandle> =
                            tag.children().top().iter().copied().collect();
                        for child in children.into_iter().rev() {
                            stack.push((child, depth + 1));
                        }
                    }
                }
            }
            tl::Node::Raw(text) => {
                let text = text.as_utf8_str();
                let text = text.trim();
                if !text.is_empty() {
                    write_indent(out, depth)?;
                    writeln!(out, "{text:?}")?;
                }
            }
            tl::Node::Comment(text) => {
                write_indent(out, depth)?;
                writeln!(out, "{}", text.as_utf8_str())?;
            }
        }
    }
    Ok(())
}

/// Whether `path` names one of Bevy's network asset sources (`http://`,
/// `https://`, with Bevy's `http`/`https` features): content must not
/// fetch remote files — a template include, a Fluent resource, a
/// stylesheet or 9-slice image from a mod or a user would reach the
/// network (review_0003 G19). `<img>` already refused any `://`; other
/// sources (`embedded://`, an app's own named sources) stay usable.
pub(crate) fn is_remote(path: &AssetPath<'_>) -> bool {
    match path.source() {
        bevy::asset::io::AssetSourceId::Name(name) => {
            name.eq_ignore_ascii_case("http") || name.eq_ignore_ascii_case("https")
        }
        bevy::asset::io::AssetSourceId::Default => false,
    }
}

/// The keywords of the tags naming another template (`{% import "x" as
/// ui %}` and `{% from "x" import button %}` bring in macro libraries).
const REFERENCE_TAGS: [&str; 4] = ["include", "extends", "import", "from"];

/// The index just past the `{% endraw %}` group closing the raw block
/// whose opener ends at `after`; `None` when there is none.
fn tera_raw_end(source: &str, after: usize) -> Option<usize> {
    // Plain scan: raw content is verbatim (no strings to respect — a
    // quote inside used to swallow the endraw and drop every later
    // include, review_0003 G13).
    let mut cursor = after;
    while let Some(next) = source[cursor..].find("{%") {
        let at = cursor + next;
        let end = source[at..]
            .find("%}")
            .map_or(source.len(), |offset| at + offset + 2);
        let body = source[at + 2..end - 2]
            .trim()
            .trim_matches(['-', '+'])
            .trim();
        if body == "endraw" {
            return Some(end);
        }
        cursor = end;
    }
    None
}

/// The index just past the closer (`%}` for a tag, `}}` for an
/// expression) of the Tera span at `source` (which starts with its
/// two-byte opener); quoted strings inside are skipped.
fn tera_span_end(source: &str, closer: [u8; 2]) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut cursor = 2;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'"' | b'\'' => {
                let quote = bytes[cursor];
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor] != quote {
                    cursor += if bytes[cursor] == b'\\' { 2 } else { 1 };
                }
                cursor += 1;
            }
            first
                if first == closer[0]
                    && cursor + 1 < bytes.len()
                    && bytes[cursor + 1] == closer[1] =>
            {
                return Some(cursor + 2);
            }
            _ => cursor += 1,
        }
    }
    None
}

/// Finds the templates `source` (the template at `path`) includes or
/// extends — `{% include "x.html" %}`, `{% extends "base.html" %}`, with
/// either quote — resolves each relative to `path`'s directory (like a
/// URL; `/x` is from the asset root, `embedded://…` keeps its source),
/// and rewrites the reference to the resolved path, which is the name
/// it's registered under. Returns the rewritten source and the resolved
/// paths. In a Markdown template, code (as its conversion reads it:
/// [`md::template_code_ranges`]) is literal text, so a reference there
/// is none.
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub(crate) fn resolve_references(
    path: &AssetPath<'static>,
    source: &str,
    allow_unapproved: bool,
) -> Result<(String, Vec<AssetPath<'static>>), BevyError> {
    let mut out = String::with_capacity(source.len());
    let mut references = Vec::new();
    let mut rest = source;
    // Markdown code (code spans, fenced and indented blocks) protects its
    // content: a code sample showing `{% include %}` syntax isn't a
    // reference. pulldown decides what is code (review_0003 G13/G17,
    // review_0004 H04 — the hand-written fence, indent and backtick rules
    // disagreed with it both ways).
    let code = if md::is_markdown(&path.to_string()) {
        md::template_code_ranges(source)
    } else {
        Vec::new()
    };
    let mut code_index = 0;
    // The three stop positions, cached in absolute coordinates: `rest` is
    // always a suffix of `source`, and re-finding a needle that was
    // already located (or one that never occurs) is what made this loop
    // quadratic — 40k `{{ x }}` lines scanned 320 KB twenty times over
    // (review_0003 G14). A cached position is valid until the cursor
    // passes it; a `None` stays `None` (the needle is absent from every
    // suffix too).
    let mut tag_at = rest.find("{%");
    let mut expression_at = rest.find("{{");
    let mut comment_at = rest.find("{#");
    while !rest.is_empty() {
        let base = source.len() - rest.len();
        let refresh = |cached: &mut Option<usize>, needle: &str| {
            if cached.is_some_and(|at| at < base) {
                *cached = rest.find(needle).map(|at| base + at);
            }
        };
        refresh(&mut tag_at, "{%");
        refresh(&mut expression_at, "{{");
        refresh(&mut comment_at, "{#");
        let tag_start = tag_at.map(|at| at - base);
        let expression_start = expression_at.map(|at| at - base);
        let comment_start = comment_at.map(|at| at - base);
        // The earliest of: a comment (its examples are commented out, not
        // references), or a Tera tag/expression — copied as a unit (its
        // quoted strings can contain `{#`, `{%` or `{{`), with a
        // reference tag resolved and a raw block verbatim.
        let start = [tag_start, expression_start, comment_start]
            .into_iter()
            .flatten()
            .min();
        let Some(start) = start else {
            out.push_str(rest);
            return Ok((out, references));
        };
        // A stop inside code: copied verbatim through the code's end.
        while code
            .get(code_index)
            .is_some_and(|range| range.end <= base + start)
        {
            code_index += 1;
        }
        if let Some(range) = code
            .get(code_index)
            .filter(|range| range.start <= base + start)
        {
            let end = range.end - base;
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        }
        if Some(start) == comment_start {
            // A `{# … #}` comment: copied verbatim, its contents skipped.
            let Some(end) = rest[start..].find("#}") else {
                out.push_str(rest);
                return Ok((out, references));
            };
            let end = start + end + 2;
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        }
        // A Tera tag or expression; the text before it is copied as is.
        out.push_str(&rest[..start]);
        if Some(start) == expression_start {
            // A `{{ … }}` expression: copied verbatim (its strings can
            // contain anything).
            let Some(span_len) = tera_span_end(&rest[start..], *b"}}") else {
                out.push_str(&rest[start..start + 2]);
                rest = &rest[start + 2..];
                continue;
            };
            out.push_str(&rest[start..start + span_len]);
            rest = &rest[start + span_len..];
            continue;
        }
        let Some(tag_len) = tera_span_end(&rest[start..], *b"%}") else {
            // A lone `{%` without its closer: text; keep scanning after it.
            out.push_str(&rest[start..start + 2]);
            rest = &rest[start + 2..];
            continue;
        };
        let tag = &rest[start..start + tag_len];
        let body = tag[2..tag.len() - 2]
            .trim()
            .trim_start_matches(['-', '+'])
            .trim_start();
        let keyword = REFERENCE_TAGS.into_iter().find(|keyword| {
            body.strip_prefix(keyword)
                .is_some_and(|after| after.starts_with(char::is_whitespace))
        });
        let literal = keyword.and_then(|keyword| {
            let after = body[keyword.len()..].trim_start();
            let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
            let end = after[1..].find(quote)?;
            Some((after, &after[1..1 + end], quote))
        });
        let Some((after, literal, quote)) = literal else {
            let raw_word = body.split_whitespace().next().unwrap_or("");
            let keyword_word = raw_word.trim_matches(['-', '+']);
            if keyword_word == "raw" {
                // `{% raw %} … {% endraw %}`: verbatim through the endraw
                // group (whitespace-control variants included).
                let Some(end) = tera_raw_end(rest, start + tag_len) else {
                    out.push_str(&rest[start..]);
                    return Ok((out, references));
                };
                out.push_str(&rest[start..end]);
                rest = &rest[end..];
            } else {
                out.push_str(tag);
                rest = &rest[start + tag_len..];
            }
            continue;
        };
        let resolved = path.resolve_embed_str(literal)?;
        if is_remote(&resolved) {
            return Err(format!("template include names a remote source: {literal}").into());
        }
        // `read_asset_bytes` skips Bevy's own unapproved-path check
        // (`../..` out of the asset root would load): enforce the app's
        // `AssetPlugin::unapproved_path_mode` here (bug_0087). `Deny` and
        // `Forbid` refuse alike — an include can't ask for Bevy's
        // `override_unapproved`; only `Allow` reads outside the root.
        if resolved.is_unapproved() && !allow_unapproved {
            return Err(format!("template include escapes the asset root: {literal}").into());
        }
        // The resolved path replaces the literal; everything else in the
        // tag (opening, closing quote, whitespace control, `%}`) keeps
        // its original bytes.
        let quote_offset = after.as_ptr() as usize - tag.as_ptr() as usize;
        out.push_str(&tag[..quote_offset]);
        out.push(quote);
        out.push_str(&resolved.to_string());
        out.push_str(&tag[quote_offset + 1 + literal.len()..]);
        rest = &rest[start + tag_len..];
        references.push(resolved);
    }
    out.push_str(rest);
    Ok((out, references))
}

/// `allow_unapproved`: the app's `AssetPlugin::unapproved_path_mode` is
/// `Allow` (read by `BevyMarkupPlugin::build`; bug_0087).
#[derive(Default, TypePath)]
pub(crate) struct HtmlTemplateLoader {
    pub(crate) allow_unapproved: bool,
    pub(crate) fuel: TemplateFuel,
}

impl AssetLoader for HtmlTemplateLoader {
    type Asset = HtmlTemplate;
    type Settings = ();
    type Error = BevyError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<HtmlTemplate, BevyError> {
        load_template(reader, self.allow_unapproved, self.fuel.get(), load_context).await
    }

    fn extensions(&self) -> &[&str] {
        &["html", "htm"]
    }
}

/// Loads Markdown templates: the same Tera machinery (includes, extends,
/// one Tera set per template); each file's Markdown converts to HTML at
/// load ([`md::to_html_template`](crate::md::to_html_template)), so
/// [`HtmlTemplate::render`] is Tera-only.
#[derive(Default, TypePath)]
pub(crate) struct MarkdownTemplateLoader {
    pub(crate) allow_unapproved: bool,
    pub(crate) fuel: TemplateFuel,
}

impl AssetLoader for MarkdownTemplateLoader {
    type Asset = HtmlTemplate;
    type Settings = ();
    type Error = BevyError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<HtmlTemplate, BevyError> {
        load_template(reader, self.allow_unapproved, self.fuel.get(), load_context).await
    }

    fn extensions(&self) -> &[&str] {
        &["md", "markdown"]
    }
}

/// The shared loader body: the template and everything it includes, extends
/// or imports, by resolved asset path — one Tera instance renders them
/// together. Reading them through the load context makes them dependencies:
/// editing one reloads every template that uses it. Autoescaping is forced
/// on Markdown too (Tera's default list is `.html`/`.htm`/`.xml` only):
/// without it, `{{ value }}` in a `.md` template would inject raw text into
/// the HTML.
#[cfg_attr(feature = "hotpath", hotpath::measure)]
async fn load_template(
    reader: &mut dyn Reader,
    allow_unapproved: bool,
    fuel: u64,
    load_context: &mut LoadContext<'_>,
) -> Result<HtmlTemplate, BevyError> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await?;
    let source = String::from_utf8(bytes)?;

    let root = load_context.path().clone_owned();
    let name = root.to_string();
    let mut templates: Vec<(String, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    // Who references whom, for the cycle check below.
    let mut edges: HashMap<String, Vec<String>> = HashMap::default();
    let mut pending = vec![(root, source)];
    while let Some((path, source)) = pending.pop() {
        if !seen.insert(path.to_string()) {
            continue;
        }
        let (source, references) = resolve_references(&path, &source, allow_unapproved)?;
        // ADR 0020: a `.md` template's Markdown converts once, at load —
        // Tera syntax survives verbatim (`to_html_template`), so the
        // per-frame path is Tera-only. Per file: an `.html` partial
        // included from a `.md` page stays HTML, and a `.md` partial
        // included from an `.html` page converts.
        let source = if md::is_markdown(&path.to_string()) {
            md::to_html_template(&source)
        } else {
            source
        };
        edges.insert(
            path.to_string(),
            references.iter().map(ToString::to_string).collect(),
        );
        for reference in references {
            if !seen.contains(&reference.to_string()) {
                let bytes = load_context.read_asset_bytes(reference.clone()).await?;
                pending.push((reference, String::from_utf8(bytes)?));
            }
        }
        check_tera_operators(&path.to_string(), &source)?;
        templates.push((path.to_string(), source));
    }
    // A template that includes, extends or imports itself, directly or
    // not, fails its load (as with Tera): it could only recurse until the
    // recursion limit.
    if let Some(cycle) = reference_cycle(&name, &edges) {
        return Err(format!("{name}: circular template reference: {cycle}").into());
    }
    // ADR 0044, 0049: templates are untrusted; every render of what
    // compiles here runs on `fuel`.
    let mut env = new_environment(fuel);
    for (path, source) in templates {
        env.add_template_owned(path, source)?;
    }
    Ok(HtmlTemplate { env, name })
}

/// A cycle of references reachable from `root`, as `a → b → a`.
fn reference_cycle(root: &str, edges: &HashMap<String, Vec<String>>) -> Option<String> {
    // Iterative depth-first search: the path on the stack, its members.
    let mut path: Vec<(&str, usize)> = vec![(root, 0)];
    let mut on_path: std::collections::HashSet<&str> = [root].into();
    let mut done: std::collections::HashSet<&str> = Default::default();
    while let Some((node, next)) = path.last_mut() {
        let targets = edges.get(*node).map_or(&[][..], Vec::as_slice);
        if let Some(target) = targets.get(*next) {
            *next += 1;
            let target = target.as_str();
            if on_path.contains(target) {
                let start = path.iter().position(|(n, _)| *n == target).unwrap_or(0);
                let mut names: Vec<&str> = path[start..].iter().map(|(n, _)| *n).collect();
                names.push(target);
                return Some(names.join(" → "));
            }
            if !done.contains(target) {
                on_path.insert(target);
                path.push((target, 0));
            }
        } else {
            let (finished, _) = path.pop().expect("non-empty");
            on_path.remove(finished);
            done.insert(finished);
        }
    }
    None
}

/// Includes naming a network source fail the load (review_0003 G19:
/// with Bevy's `http`/`https` features a template reached the network);
/// other sources stay usable.
#[test]
fn remote_includes_are_refused() {
    let path = AssetPath::from("page.html");
    for remote in ["https://evil.example/x.html", "HTTP://evil.example/x.html"] {
        let source = format!(r#"{{% include "{remote}" %}}"#);
        assert!(
            resolve_references(&path, &source, false).is_err(),
            "{remote}"
        );
    }
    let (_, references) = resolve_references(
        &path,
        r#"{% include "embedded://crate/part.html" %}"#,
        false,
    )
    .unwrap();
    assert_eq!(references[0].to_string(), "embedded://crate/part.html");
}

/// The `json` filter prints JSON that survives autoescaping and the
/// attribute's entity decoding unchanged, whatever the string holds
/// (bug_0128: emoji sequences, combining accents, control characters).
/// Without the filter Tera prints Rust `Debug` escapes, which aren't JSON:
/// the attribute fails to parse (the removed `\u{…}` rewrite accepted
/// them; templates add `| json` instead).
#[test]
fn json_filter_round_trips_any_string() {
    let render = |template: &str, value: &str| {
        let html = test_render("t.html", template, serde_json::json!({ "s": value })).unwrap();
        decode_text(&html).into_owned()
    };
    for value in [
        "Ada \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
        "Jose\u{301}",
        "bell\u{7} nul\u{0}",
        "quotes \" ' and <tags> & \\u{41}",
        "",
    ] {
        let json = render(r#"{{ {"v": s, "n": 2} | json }}"#, value);
        let parsed = parse_attribute_json(&json).unwrap_or_else(|err| panic!("{json:?}: {err}"));
        assert_eq!(parsed["v"], value, "{json:?}");
        assert_eq!(parsed["n"], 2, "numbers stay numbers (plural selectors)");
    }
    let debug = render(r#"{{ {"v": s} }}"#, "a\u{200D}b");
    assert!(parse_attribute_json(&debug).is_err(), "{debug:?}");
}

/// The scanner's code protection is pulldown's: the shapes the
/// hand-written fence/indent/backtick rules got wrong (review_0003 G17,
/// review_0004 H04) — a dropped real include fails the render ("Unknown
/// template"), a code sample taken for a reference fails the load
/// (NotFound). Each case states what CommonMark makes of the include.
#[test]
fn scanner_code_is_commonmark_code() {
    let path = AssetPath::from("page.md");
    let resolve = |source: &str| {
        resolve_references(&path, source, false)
            .map(|(_, refs)| refs.iter().map(|r| r.to_string()).collect::<Vec<_>>())
            .unwrap()
    };
    let real = vec!["real.md".to_owned()];
    let none = Vec::<String>::new();
    for (shape, source, expected) in [
        (
            "lazy continuation",
            "Intro\n    {% include \"real.md\" %}\n",
            &real,
        ),
        (
            "list item paragraph",
            "- item\n\n    {% include \"real.md\" %}\n",
            &real,
        ),
        (
            "tab-indented code",
            "Intro\n\n\t{% include \"gone.md\" %}\n",
            &none,
        ),
        (
            "escaped backticks",
            "\\`{% include \"real.md\" %}\\`\n",
            &real,
        ),
        (
            "longer fence closer",
            "```\ncode\n````\n\n{% include \"real.md\" %}\n",
            &real,
        ),
        (
            "shorter fence closer",
            "````\n```\n{% include \"gone.md\" %}\n````\n",
            &none,
        ),
        (
            "fence in a quote",
            "> ```\n> {% include \"gone.md\" %}\n> ```\n",
            &none,
        ),
        (
            "fence in a list",
            "- item\n\n  ```\n  {% include \"gone.md\" %}\n  ```\n",
            &none,
        ),
        (
            "double-backtick span",
            "``a ` {% include \"gone.md\" %} ``\n",
            &none,
        ),
        (
            "unclosed span",
            "a ` b\n\n{% include \"real.md\" %}\n",
            &real,
        ),
        // Indentation inside a Tera block isn't Markdown (review_0004
        // H11): this body line is a paragraph, so the include runs.
        (
            "indented block body",
            "{% if a %}\n    see {% include \"real.md\" %}\n{% endif %}\n",
            &real,
        ),
    ] {
        assert_eq!(&resolve(source), expected, "{shape}");
    }
}

/// Malformed attribute JSON is an error, not a crash (a backslash before
/// a non-ASCII character once panicked the removed escape rewrite).
#[test]
fn malformed_attribute_json_is_an_error() {
    assert!(parse_attribute_json("{\"a\": \"\\u{41}\\é\"}").is_err());
    assert!(parse_attribute_json("{\"a\": ").is_err());
}

/// The scanner's code protection agrees with pulldown on the shapes G13
/// found: a fence after prose, indented code, whitespace-control raw
/// variants, code spans across blank lines (their content is scanned),
/// and raw blocks holding quotes (the endraw scan is plain). Fails when
/// the scanner's protection is reverted.
#[test]
fn scanner_protects_code_like_pulldown() {
    let path = AssetPath::from("page.md");
    let resolve = |source: &str| {
        resolve_references(&path, source, false)
            .map(|(_, refs)| refs.iter().map(|r| r.to_string()).collect::<Vec<_>>())
    };
    // Protected content: the include inside is not a reference.
    assert_eq!(
        resolve("Intro\n\n~~~\n{% include \"gone.md\" %}\n~~~").unwrap(),
        Vec::<String>::new(),
        "a fence after prose protects"
    );
    assert_eq!(
        resolve("Intro\n\n    {% include \"gone.md\" %}\n").unwrap(),
        Vec::<String>::new(),
        "indented code protects"
    );
    assert_eq!(
        resolve("{%raw-%}\n{% include \"gone.md\" %}\n{% endraw -%}").unwrap(),
        Vec::<String>::new(),
        "the whitespace-control raw variant protects"
    );
    assert_eq!(
        resolve("{% raw %}{% it's %}{% endraw %}{% include \"real.html\" %}").unwrap(),
        vec!["real.html".to_owned()],
        "a quote inside a raw block doesn't swallow the endraw"
    );
    // Unprotected: the span's content is scanned.
    assert_eq!(
        resolve("a `\n\n b {% include \"gone.md\" %} c ` d").unwrap(),
        vec!["gone.md".to_owned()],
        "a code span can't span a blank line"
    );
}
/// Puts a space before a `/>` that ends a tag name or a value-less
/// attribute (`<br/>` → `<br />`, `<input disabled/>` → `<input disabled
/// />`), the spelling `tl` parses (bug_0185). `tl` counts `/` as a name
/// character, so `<b/>` became an element named `b/`, never closed: the
/// rest of the document nested inside it (a `<br/>` merged the following
/// paragraphs into one). Only tag names and attribute names are touched:
/// text, comments and attribute values stay as written, and an unquoted
/// value keeps its slash, as HTML reads it (`<a href=/x/>`: the value is
/// `/x/`).
pub(crate) fn normalize_self_closing(html: &str) -> std::borrow::Cow<'_, str> {
    let bytes = html.as_bytes();
    let mut out: Option<String> = None;
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        // A comment or a `<!…>`/`<?…>` declaration: skip it whole.
        if bytes[i..].starts_with(b"<!--") {
            i = html[i + 4..]
                .find("-->")
                .map_or(bytes.len(), |end| i + 4 + end + 3);
            continue;
        }
        if !bytes.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
            i += 1;
            continue;
        }
        // A start tag: its name, then attributes, up to `>`.
        let mut j = i + 1;
        let mut in_name = true; // the tag name, or an attribute name
        while j < bytes.len() && bytes[j] != b'>' {
            match bytes[j] {
                b'/' if in_name
                    && bytes.get(j + 1) == Some(&b'>')
                    && !bytes[j - 1].is_ascii_whitespace() =>
                {
                    let out = out.get_or_insert_with(|| String::with_capacity(html.len() + 16));
                    out.push_str(&html[copied..j]);
                    out.push(' ');
                    copied = j;
                    j += 1;
                }
                byte if byte.is_ascii_whitespace() => {
                    in_name = true;
                    j += 1;
                }
                b'=' => {
                    // The value: quoted up to its quote, unquoted up to
                    // whitespace or `>` (a `/` there is part of it).
                    j += 1;
                    while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
                        j += 1;
                    }
                    match bytes.get(j) {
                        Some(&quote @ (b'"' | b'\'')) => {
                            j += 1;
                            while j < bytes.len() && bytes[j] != quote {
                                j += 1;
                            }
                            j += 1;
                        }
                        _ => {
                            while j < bytes.len()
                                && bytes[j] != b'>'
                                && !bytes[j].is_ascii_whitespace()
                            {
                                j += 1;
                            }
                        }
                    }
                    in_name = false;
                }
                _ => j += 1,
            }
        }
        i = j;
    }
    match out {
        Some(mut out) => {
            out.push_str(&html[copied..]);
            std::borrow::Cow::Owned(out)
        }
        None => std::borrow::Cow::Borrowed(html),
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_self_closing as norm;

    /// Printed values come out exactly as MiniJinja's own formatter prints
    /// them, escaped or not.
    #[test]
    fn printed_values_match_minijinja() {
        let context = serde_json::json!({
            "list": ["<a>", 1, true, null, {"k": "v&w"}],
            "map": {"x": "'q'", "y": [1.5, "/"]},
            "text": "<b>&", "n": 3, "none": null,
        });
        let mut plain = minijinja::Environment::new();
        plain.set_auto_escape_callback(|name| {
            if name.ends_with(".html") {
                minijinja::AutoEscape::Html
            } else {
                minijinja::AutoEscape::None
            }
        });
        for source in [
            "{{ list }}|{{ map }}|{{ text }}|{{ n }}|{{ none }}|{{ list | safe }}",
            "{{ [1, '<'] * 3 }}|{{ range(3) }}|{{ map.y }}|{{ {'a': [map]} }}",
        ] {
            for name in ["t.html", "t.txt"] {
                plain.add_template_owned(name, source.to_owned()).unwrap();
                let expected = plain
                    .get_template(name)
                    .unwrap()
                    .render(TemplateContext::from_serialize(&context).as_value())
                    .unwrap();
                assert_eq!(
                    test_render(name, source, context.clone()).unwrap(),
                    expected,
                    "{name}: {source}"
                );
            }
        }
    }

    /// `tojson`'s indent counts toward the printed size: MiniJinja takes
    /// any `usize` and writes `indent × depth` spaces per item before any
    /// limit applies (a 93-byte template, `tojson(5E13)`, allocated
    /// gigabytes; UPSTREAM U64). Small indents still print.
    #[test]
    fn tojson_indents_count_toward_the_printed_size() {
        let render = |template: &str| test_render("t.html", template, serde_json::json!({}));
        for template in [
            "{{ [1] | tojson(5E13) }}",
            "{{ {'a': 1} | tojson(indent=50000000000000) }}",
            "{{ [[[1]]] | tojson(18446744073709551615) }}",
            "{{ ([1] * 3000000) | tojson(4) }}",
        ] {
            let err = render(template).unwrap_err().to_string();
            assert!(
                err.contains("too many items") || err.contains("fuel"),
                "{template}: {err}"
            );
        }
        assert_eq!(
            render("{{ {'a': [1]} | tojson(2) }}").unwrap(),
            "{\n  \"a\": [\n    1\n  ]\n}"
        );
        assert_eq!(render("{{ {} | tojson(5E13) }}").unwrap(), "{}");
        assert_eq!(
            render("{{ [1, 2] | tojson(true) }}").unwrap(),
            "[\n  1,\n  2\n]"
        );
    }

    /// A list or map holding more items than the output could take is
    /// refused before it is read: a 50-million-item sequence isn't
    /// iterated, nor is a lazily repeated list of repeated lists (10¹⁶
    /// items). bevy_markup's own check, which protects apps on a MiniJinja
    /// that doesn't charge such walks itself (this repository patches one
    /// that does, so a render may fail on fuel first).
    #[test]
    fn printed_values_are_checked_before_they_are_read() {
        use minijinja::value::{Enumerator, Object, ObjectRepr, Value};
        use std::sync::atomic::{AtomicUsize, Ordering};
        #[derive(Debug, Default)]
        struct Counted(AtomicUsize);
        impl Object for Counted {
            fn repr(self: &std::sync::Arc<Self>) -> ObjectRepr {
                ObjectRepr::Seq
            }
            fn get_value(self: &std::sync::Arc<Self>, _: &Value) -> Option<Value> {
                self.0.fetch_add(1, Ordering::Relaxed);
                Some(Value::from(7))
            }
            fn enumerate(self: &std::sync::Arc<Self>) -> Enumerator {
                Enumerator::Seq(50_000_000)
            }
        }
        let counted = std::sync::Arc::new(Counted::default());
        let err = check_printable(&Value::from_dyn_object(counted.clone())).unwrap_err();
        assert!(err.to_string().contains("too many items"), "{err}");
        // "7, " per item: 16 MiB holds ~5.6 million; none was read.
        assert_eq!(counted.0.load(Ordering::Relaxed), 0);
        let env = new_environment(HtmlTemplateLimits::default().fuel);
        let value = |expression: &str| {
            env.compile_expression(expression)
                .unwrap()
                .eval(())
                .unwrap()
        };
        for expression in [
            "[1] * 100000000",
            "[[1] * 100000000] * 100000000",
            "[[1] * 3000000, [2] * 3000000]",
            "{'a': [1] * 6000000}",
        ] {
            assert!(check_printable(&value(expression)).is_err(), "{expression}");
        }
        check_printable(&value("[['<'] * 100000, {'a': [1, 2]}]")).unwrap();

        // Rendered, they fail (on this check or on MiniJinja's fuel); the
        // filters writing a value's text check it too.
        let render = |template: &str| test_render("t.html", template, serde_json::json!({}));
        for template in [
            "{{ [1] * 100000000 }}",
            "{{ [[1] * 100000000] * 100000000 }}",
            "{{ {'a': [1] * 6000000} }}",
            "{{ ([1] * 100000000) | string | length }}",
            "{{ ([1] * 100000000) | json | length }}",
            "{{ ([1] * 100000000) | tojson | length }}",
            "{{ ([1] * 100000000) | pprint | length }}",
            "{{ ([1] * 100000000) | join | length }}",
        ] {
            let err = render(template).unwrap_err().to_string();
            assert!(
                err.contains("too many items") || err.contains("fuel"),
                "{template}: {err}"
            );
        }
        // Under the limit, a large list prints, escaped as it is written.
        let printed = render("{{ ['<'] * 100000 }}").unwrap();
        assert_eq!(printed.len(), 100_000 * "&#x27;&lt;&#x27;, ".len() - 2 + 2);
    }

    /// `format` takes printf-style formats with ASCII `%(key)` names and
    /// widths and precisions up to 65535; anything else fails the render.
    #[test]
    fn format_specifiers_are_checked() {
        let render = |template: &str| test_render("t.html", template, serde_json::json!({}));
        assert_eq!(render(r#"{{ "%s-%d" | format("a", 2) }}"#).unwrap(), "a-2");
        assert_eq!(
            render(r#"{{ "%(name)s!" | format(name="Ann") }}"#).unwrap(),
            "Ann!"
        );
        assert_eq!(render(r#"{{ "%%(é)" | format() }}"#).unwrap(), "%(é)");
        assert_eq!(render(r#"{{ "é %s" | format(1) }}"#).unwrap(), "é 1");
        for template in [
            r#"{{ "%(é" | format(1) }}"#,
            r#"{{ "%(aé)s" | format(1) }}"#,
            r#"{{ "%s %(名)s" | format(1) }}"#,
        ] {
            let err = render(template).unwrap_err();
            assert!(
                err.to_string().contains("must be ASCII"),
                "{template}: {err}"
            );
        }
        // Widths and precisions up to 65535 format; larger ones fail.
        assert_eq!(
            render(r#"{{ "%.2f|%5d" | format(1, 2) }}"#).unwrap(),
            "1.00|    2"
        );
        assert_eq!(
            render(r#"{{ "%.65535f" | format(1) | length }}"#).unwrap(),
            "65537"
        );
        for template in [
            r#"{{ "%.70000f" | format(1) }}"#,
            r#"{{ "%(n).65536f" | format(n=1) }}"#,
            r#"{{ "%-099999999999999999999999d" | format(1) }}"#,
        ] {
            let err = render(template).unwrap_err();
            assert!(err.to_string().contains("over 65535"), "{template}: {err}");
        }
    }

    /// bug_0185: what `tl` misreads gets a space; nothing else changes.
    #[test]
    fn self_closing_tags_get_the_space_tl_needs() {
        for (html, expected) in [
            ("<p>A<br/>B</p>", "<p>A<br />B</p>"),
            (
                "<svg><rect/><g><path/></g></svg>",
                "<svg><rect /><g><path /></g></svg>",
            ),
            ("<input disabled/>", "<input disabled />"),
            (
                "<input type=\"text\" disabled/>",
                "<input type=\"text\" disabled />",
            ),
            // Already parsed right, or not a tag name / attribute name.
            ("<br />", "<br />"),
            ("<rect width=\"1\"/>", "<rect width=\"1\"/>"),
            ("<a href=/x/>link</a>", "<a href=/x/>link</a>"),
            ("<a title=\"a/>b\">x</a>", "<a title=\"a/>b\">x</a>"),
            ("<!-- <b/> -->text a/> b", "<!-- <b/> -->text a/> b"),
            ("1 </> 2 < b/>", "1 </> 2 < b/>"),
        ] {
            assert_eq!(norm(html), expected, "{html}");
        }
        assert!(matches!(
            norm("<p>plain</p>"),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    /// Every tag `tl` closes after normalizing: the second paragraph is a
    /// sibling, not nested in a `br/`.
    #[test]
    fn a_self_closing_tag_no_longer_swallows_the_document() {
        for html in [
            "<p>A<br/>B</p><p>C</p>",
            "<p>A <b/> B</p><p>C</p>",
            "<svg><rect/></svg><p>C</p>",
        ] {
            let document = super::HtmlDocument::parse(html.to_owned()).unwrap();
            let dom = document.dom();
            let tops: Vec<String> = dom
                .children()
                .iter()
                .filter_map(|handle| handle.get(dom.parser())?.as_tag())
                .map(|tag| tag.name().as_utf8_str().into_owned())
                .collect();
            assert_eq!(tops.len(), 2, "{html}: top-level tags {tops:?}");
            assert!(
                !tops.iter().any(|name| name.contains('/')),
                "{html}: {tops:?}"
            );
        }
    }

    /// The scanners are linear (review_0003 G14: they used to re-find each
    /// stop needle, or re-scan the line, per statement — a 320 KB template
    /// cost seconds to load). Asserted as *scaling*, not wall time (review_0004
    /// H09: a 2 s bound passed a quadratic rewrite at 1.25 s): 4× the input
    /// may cost at most 8× the time — linear is ~4×, quadratic ~16× — best of
    /// three runs each, so machine speed and load cancel out.
    /// review_0005 I04: each `&#` scanned to the next `;` (or the end of the
    /// text) before checking the digit bound, so a text of `&#` references
    /// without terminators decoded in O(n²) — on every render. Kept for the
    /// dependency that decodes now (ADR 0036). Scaling, not wall time: 4×
    /// the input may cost at most 8× (best of three).
    #[test]
    fn entity_decoding_stays_linear_without_terminators() {
        use std::time::{Duration, Instant};
        let best = |text: &str| {
            (0..3)
                .map(|_| {
                    let start = Instant::now();
                    std::hint::black_box(decode_text(text));
                    start.elapsed()
                })
                .min()
                .unwrap()
        };
        for (shape, unit) in [("numeric", "&#1"), ("hex", "&#x1"), ("named", "&am")] {
            let (small, large) = (best(&unit.repeat(10_000)), best(&unit.repeat(40_000)));
            assert!(
                large < small * 8 + Duration::from_millis(5),
                "decode_text scales superlinearly on {shape}: 10k {small:?}, 40k {large:?}"
            );
        }
    }

    /// bug_0182: a tag over the operator bound fails the check, in every
    /// shape Tera compiles recursively; one at the bound passes; and what
    /// Tera reads as text or strings doesn't count. The CI corpus inputs
    /// (`fuzz/seeds/html/2405b86b…`) are 150-operator chains.
    #[test]
    fn long_operator_chains_are_rejected_before_tera() {
        let over = MAX_TERA_TAG_OPERATORS + 1;
        let chains = |count: usize| {
            [
                format!("{{{{ n{} }}}}", " - n".repeat(count)),
                format!("{{{{ n{} }}}}", " | upper".repeat(count)),
                format!("{{{{ n{} }}}}", ".x".repeat(count)),
                format!("{{{{ n{} }}}}", "[0]".repeat(count)),
                format!("{{{{ n{} }}}}", " and n".repeat(count)),
                format!("{{{{ {}n }}}}", "n if n else ".repeat(count.div_ceil(2))),
                format!("{{% set x = n{} %}}", " ~ n".repeat(count)),
                // `}}` closes a map, not the `{% … %}` tag.
                format!(
                    r#"{{% set x = {{"a": {{"b": 1}}}}{} %}}"#,
                    " - n".repeat(count)
                ),
                // A comment before and a raw block before don't hide it.
                format!(
                    "{{# {{{{ #}}{{% raw %}}\"{{% endraw %}}{{{{ n{} }}}}\"",
                    " - n".repeat(count)
                ),
            ]
        };
        for template in chains(over) {
            assert!(tera_tag_over_limit(&template), "rejected: {template:.60}");
        }
        assert!(!tera_tag_over_limit(&format!(
            "{{{{ n{} }}}}",
            " - n".repeat(MAX_TERA_TAG_OPERATORS)
        )));
        let many = " - n".repeat(2 * over);
        for text in [
            // Strings, text, comments and raw bodies aren't expressions.
            format!("{{{{ \"{many}\" }}}}"),
            format!("{{{{ 'a\\'{many}' }}}}"),
            format!("<p>{many}</p>{{{{ n }}}}"),
            format!("{{# {{{{ n{many} }}}} #}}"),
            format!("{{%- raw -%}}{{{{ n{many} }}}}{{%- endraw %}}{{{{ n }}}}"),
            // One bounded tag per expression, many tags.
            "{{ a - b }}".repeat(100),
            // The studio's busiest tag (prototype_19's menu buttons).
            r#"{{ <ui.button id="connect" class="primary" autofocus={true} signal="main-menu.connect" key="main-menu-connect" label="Connect" /> }}"#.to_owned(),
        ] {
            assert!(!tera_tag_over_limit(&text), "accepted: {text:.60}");
        }
    }

    /// What the check lets through compiles on a 2 MB thread (the test
    /// harness's), in a debug build: the bound's every shape at the bound,
    /// in the innermost of 39 nested blocks (Tera rejects 40).
    #[test]
    fn chains_at_the_bound_compile_on_a_small_stack() {
        let at = MAX_TERA_TAG_OPERATORS;
        for chain in [
            format!("n{}", " - n".repeat(at)),
            format!("n{}", " | upper".repeat(at)),
            format!("n{}", ".x".repeat(at)),
            format!("n{}", "[0]".repeat(at)),
            format!("n{}", " ~ n".repeat(at)),
            format!("{}n", "n if n else ".repeat(at / 2)),
            format!("{}n{}", "(".repeat(at / 2), ")".repeat(at / 2)),
        ] {
            let template = format!(
                "{}{{{{ {chain} }}}}{}",
                "{% if n %}".repeat(39),
                "{% endif %}".repeat(39)
            );
            assert!(!tera_tag_over_limit(&template));
            new_environment(HtmlTemplateLimits::default().fuel)
                .add_template_owned("t.html", template.clone())
                .unwrap_or_else(|err| panic!("{err}: {chain:.40}"));
        }
    }

    #[test]
    fn scanners_stay_linear_on_pathological_input() {
        use std::time::{Duration, Instant};
        let path = AssetPath::from("p.md");
        let best = |text: &str, run: &dyn Fn(&str)| -> Duration {
            (0..3)
                .map(|_| {
                    let start = Instant::now();
                    run(text);
                    start.elapsed()
                })
                .min()
                .unwrap()
        };
        let resolve = |text: &str| drop(resolve_references(&path, text, false));
        let separate = |text: &str| drop(md::separate_statements(text));
        let operators = |text: &str| {
            std::hint::black_box(tera_tag_over_limit(text));
        };
        // Deeply nested, indented blocks: the dedent re-reads each body per
        // level (review_0004 H11), bounded by its depth cap.
        let nested = |count: usize| {
            let open: String = (0..count)
                .map(|depth| format!("{}{{% if a %}}\n", " ".repeat(depth % 40)))
                .collect();
            let close: String = (0..count)
                .rev()
                .map(|depth| format!("{}{{% endif %}}\n", " ".repeat(depth % 40)))
                .collect();
            format!("{open}text\n{close}")
        };
        for (shape, small, large) in [
            (
                "expressions",
                "{{ x }}\n".repeat(10_000),
                "{{ x }}\n".repeat(40_000),
            ),
            ("code spans", "`c`\n".repeat(10_000), "`c`\n".repeat(40_000)),
            (
                "own-line statements",
                "{% if a %}x{% endif %}\n".repeat(10_000),
                "{% if a %}x{% endif %}\n".repeat(40_000),
            ),
            (
                "one line of inline statements",
                "{% if a %}x{% endif %} ".repeat(10_000),
                "{% if a %}x{% endif %} ".repeat(40_000),
            ),
            (
                "deeply nested indented blocks",
                nested(1_000),
                nested(4_000),
            ),
            (
                "raw blocks without an end",
                "{% raw %}{{ ".repeat(10_000),
                "{% raw %}{{ ".repeat(40_000),
            ),
            (
                "an unclosed tag",
                format!("{{{{ {}", "\"a\" ".repeat(10_000)),
                format!("{{{{ {}", "\"a\" ".repeat(40_000)),
            ),
        ] {
            for (scanner, run) in [
                ("resolve_references", &resolve as &dyn Fn(&str)),
                ("separate_statements", &separate),
                ("tera_tag_over_limit", &operators),
            ] {
                let (small_time, large_time) = (best(&small, run), best(&large, run));
                assert!(
                    large_time < small_time * 8 + Duration::from_millis(20),
                    "{scanner} scales superlinearly on {shape}: 10k {small_time:?}, 40k {large_time:?}"
                );
            }
        }
    }

    use super::*;
    use proptest::prelude::*;

    /// Text dense in markup characters and entity look-alikes (`&lt;`
    /// typed as text), plus arbitrary characters.
    const MARKUP_TEXT: &str = "(?s)(&lt;|&gt;|&amp;|&quot;|&#39;|[<>&\"';# a-z]|.){0,24}";

    /// References resolve like URLs from the template's directory and are
    /// rewritten to the resolved path; other tags and text stay as written.
    #[test]
    fn references_resolve_relative_to_the_template() {
        let path = AssetPath::parse("ui/pages/main.html").clone_owned();
        let source = r#"{% extends "base.html" %}{%- include '../lib/ui.html' -%}
{% include "/shared/foot.html" %}{% if include %}{{ "include" }}{% endif %}{% block x %}{% endblock %}"#;
        let (out, references) = resolve_references(&path, source, false).unwrap();
        assert_eq!(
            references
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["ui/pages/base.html", "ui/lib/ui.html", "shared/foot.html"]
        );
        assert_eq!(
            out,
            r#"{% extends "ui/pages/base.html" %}{%- include 'ui/lib/ui.html' -%}
{% include "shared/foot.html" %}{% if include %}{{ "include" }}{% endif %}{% block x %}{% endblock %}"#
        );
        let embedded = AssetPath::parse("embedded://app/ui/main.html").clone_owned();
        let (_, references) =
            resolve_references(&embedded, r#"{% include "part.html" %}"#, false).unwrap();
        assert_eq!(references[0].to_string(), "embedded://app/ui/part.html");
    }

    /// The scanner's edge cases: an unmatched backtick run is text (it
    /// used to skip to EOF, dropping later includes), a fence at the
    /// file's start closes (it never did), and a `"{#"` inside a Tera
    /// string — in a tag or an expression — is not a comment (it
    /// swallowed everything to the next `#}` or to EOF).
    #[test]
    fn scanner_edge_cases_keep_later_references() {
        let markdown = AssetPath::parse("main.md").clone_owned();
        let (_, references) =
            resolve_references(&markdown, "It`s here\n{% include \"a.md\" %}\n", false).unwrap();
        assert_eq!(references.len(), 1);

        let (_, references) = resolve_references(
            &markdown,
            "```rust\nx\n```\n{% include \"a.md\" %}\n",
            false,
        )
        .unwrap();
        assert_eq!(references.len(), 1);

        let html = AssetPath::parse("main.html").clone_owned();
        let (out, references) = resolve_references(
            &html,
            "{% set x = \"{#\" %}\n{% include \"a.html\" %}\n",
            false,
        )
        .unwrap();
        assert_eq!(references.len(), 1);
        assert!(out.contains("x = \"{#\""), "the tag is verbatim: {out}");

        let (out, references) = resolve_references(
            &html,
            "{% include \"a.html\" %}{{ \"{#\" }}{% include \"b.html\" %}\n",
            false,
        )
        .unwrap();
        assert_eq!(references.len(), 2, "{references:?}");
        assert!(out.contains("{{ \"{#\" }}"), "{out}");
    }

    /// Each of the five references Tera's autoescaping emits decodes to its
    /// character. Catches a dropped or misspelled entry (e.g. `&#x27;`
    /// instead of `&#39;`), which would show raw entities in the UI.
    #[test]
    fn decodes_the_five_escapes() {
        assert_eq!(
            decode_text("&lt;b&gt; &quot;x&quot; &#39;y&#39; &amp;"),
            "<b> \"x\" 'y' &"
        );
    }

    /// Decoding undoes exactly one level of escaping: an escaped entity
    /// (`&amp;lt;`, what Tera emits for a literal `&lt;`) becomes the entity
    /// text, not the character. Catches decoding `&amp;` before the others
    /// (or decoding repeatedly), which turns authored `&lt;` into `<`.
    #[test]
    fn decodes_one_level_only() {
        for decode in [decode_text, decode_attribute] {
            assert_eq!(decode("&amp;lt;"), "&lt;");
            assert_eq!(decode("&amp;amp;"), "&amp;");
            assert_eq!(decode("&amp;quot;&amp;#39;"), "&quot;&#39;");
        }
    }

    /// Known references decode; unknown ones stay verbatim; the spec's
    /// legacy forms decode without `;` in text but not where an attribute
    /// value continues with an alphanumeric or `=` (ADR 0036; Chrome agrees,
    /// the `text_props` vector). The one-level rule keeps a partial decode
    /// from re-decoding (`&amp;nbsp;` reads `&nbsp;`, not the space).
    #[test]
    fn references_decode_as_the_spec_says() {
        // (input, as text, as an attribute value)
        for (input, text, attribute) in [
            ("&nbsp;", "\u{a0}", "\u{a0}"),
            ("&#169;", "©", "©"),
            ("&#x3C;", "<", "<"),
            ("&NotEqualTilde;", "\u{2242}\u{338}", "\u{2242}\u{338}"), // two code points
            ("a & b", "a & b", "a & b"),
            ("&unknown;", "&unknown;", "&unknown;"),
            ("&;", "&;", "&;"),
            // Legacy names without `;`: decoded, except in an attribute
            // where an alphanumeric or `=` follows.
            ("&lt", "<", "<"),
            ("&copy2", "©2", "&copy2"),
            ("?a=1&copy=2", "?a=1©=2", "?a=1&copy=2"),
            ("&notit;", "¬it;", "&notit;"),
            // Numeric references without `;` decode in both.
            ("&#39", "'", "'"),
            // Invalid code points read U+FFFD (0, surrogates, past the top).
            ("&#0;", "\u{FFFD}", "\u{FFFD}"),
            ("&#xD800;", "\u{FFFD}", "\u{FFFD}"),
            ("&#x110000;", "\u{FFFD}", "\u{FFFD}"),
            // The C1 controls read their Windows-1252 replacements.
            ("&#151;", "\u{2014}", "\u{2014}"),
            // Digits only: a sign is no reference.
            ("&#+65;", "&#+65;", "&#+65;"),
            ("&#x41;&#66;", "AB", "AB"),
        ] {
            assert_eq!(decode_text(input), text, "text {input:?}");
            assert_eq!(decode_attribute(input), attribute, "attribute {input:?}");
        }
    }

    /// Nothing to decode borrows: most values (class names, ids, text)
    /// have no `&`.
    #[test]
    fn decoding_borrows_without_references() {
        assert!(matches!(decode_text("plain text"), Cow::Borrowed(_)));
        assert!(matches!(decode_attribute("card wide"), Cow::Borrowed(_)));
    }

    /// Output past `MAX_RENDERED_BYTES` fails the render, at the write that
    /// crosses it (bug_0220: two nested loops in 92 bytes rendered n²
    /// characters, 10¹⁰ at Tera's `range` limit). The rest renders.
    #[test]
    fn rendered_output_is_bounded() {
        let template = |source: &str| {
            // Fuel enough for the output itself (the limit under test).
            let mut env = new_environment(1 << 30);
            env.add_template_owned("t.html", source.to_owned()).unwrap();
            HtmlTemplate {
                env,
                name: "t.html".to_owned(),
            }
        };
        let loops = |n: usize| {
            template(&format!(
                "{{% for i in range({n}) %}}{{% for j in range({n}) %}}xxxx{{% endfor %}}{{% endfor %}}"
            ))
        };
        // 4 × 2 100² = 17.6 MB.
        let err = loops(2100)
            .render_source(&TemplateContext::new())
            .expect_err("over the limit");
        assert!(err.to_string().contains("MAX_RENDERED_BYTES"), "{err}");
        // 4 × 2 000² = 16 MB, just under 16 MiB.
        let ok = loops(2000).render_source(&TemplateContext::new()).unwrap();
        assert_eq!(ok.len(), 16_000_000);
        // Other errors stay what they were.
        let err = template("{{ missing }}")
            .render_source(&TemplateContext::new())
            .expect_err("undefined");
        assert!(!err.to_string().contains("MAX_RENDERED_BYTES"), "{err}");
    }

    /// Concatenated text of every raw node in `document`, decoded.
    fn decoded_text(document: &HtmlDocument) -> String {
        let dom = document.dom();
        dom.nodes()
            .iter()
            .filter_map(|node| match node {
                tl::Node::Raw(text) => Some(decode_text(text.as_utf8_str()).into_owned()),
                _ => None,
            })
            .collect()
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        /// Any string from a Tera variable (`.html` name → autoescaping)
        /// reads back verbatim after `tl` parsing and `decode_text`:
        /// Tera's escape set and ours must match exactly (a Tera upgrade
        /// emitting `&#x27;`/`&apos;`, or a missing decode, shows entities
        /// in the UI; a missing escape lets data inject markup).
        #[test]
        fn rendered_variables_round_trip(s in MARKUP_TEXT) {
            let mut env = new_environment(HtmlTemplateLimits::default().fuel);
            env.add_template_owned("t.html", "<p>{{ s }}</p>").unwrap();
            let template = HtmlTemplate { env, name: "t.html".to_owned() };
            let context = TemplateContext::new().with("s", &s);
            let document = template.render(&context).unwrap();
            prop_assert_eq!(document.dom().query_selector("p").unwrap().count(), 1);
            prop_assert_eq!(decoded_text(&document), s);
        }

        /// bug_0258: `fits_printed` accounts for string bytes inside
        /// containers (nested or flat), refusing any container whose string
        /// payloads exceed `MAX_RENDERED_BYTES` before serialization runs.
        #[test]
        fn bug_0258_serialized_containers_respect_fuel_and_output_limit(
            str_len in 100_000usize..200_000,
            count in 200usize..400,
            nested in proptest::bool::ANY,
        ) {
            let s = minijinja::Value::from("x".repeat(str_len));
            let list: minijinja::Value = vec![s; count].into();
            let val = if nested {
                minijinja::Value::from(vec![list])
            } else {
                list
            };
            // str_len * count >= 20_000_000 > MAX_RENDERED_BYTES (16 MiB),
            // even though item count is only 200..400.
            proptest::prop_assert!(
                !fits_printed(&val, 0),
                "str_len={} count={} nested={} passed fits_printed",
                str_len,
                count,
                nested
            );
        }
    }

    /// bug_0258: container-of-strings serialization (`[s] * N`) is refused
    /// *before* allocating past either `MAX_RENDERED_BYTES` or remaining
    /// fuel across every filter, plus nested `join` and repeated-width
    /// `format`.
    #[test]
    fn bug_0258_container_of_strings_is_bounded_before_allocating() {
        let render_with = |fuel: u64, source: &str| {
            let mut env = new_environment(fuel);
            env.add_template_owned("t.html", source.to_owned()).unwrap();
            let template = HtmlTemplate {
                env,
                name: "t.html".to_owned(),
            };
            let start = std::time::Instant::now();
            let result = template.render_source(&TemplateContext::new());
            (result, start.elapsed())
        };

        // Low fuel (~640 KB budget): 30 MB containers must fail *fast*
        // (before building a 30 MB String, which took 35–330 ms unfixed).
        let low_cases = [
            "{% set s = 'x' * 50000 %}{{ ([s] * 600) | json | length }}",
            "{% set s = 'x' * 50000 %}{{ ([s] * 600) | tojson | length }}",
            "{% set s = 'x' * 50000 %}{{ ([s] * 600) | pprint | length }}",
            "{% set s = 'x' * 50000 %}{{ ([s] * 600) | string | length }}",
            "{% set s = 'x' * 50000 %}{{ ([s] * 600) | safe | length }}",
            "{% set s = 'x' * 50000 %}{{ ([s] * 600) | escape | length }}",
            "{% set s = 'x' * 50000 %}{{ ([s] * 600) | e | length }}",
            "{% set s = 'x' * 50000 %}{{ '%s' | format([s] * 600) | length }}",
            "{% set s = 'x' * 50000 %}{{ [[s] * 600] | join | length }}",
            "{% set s = 'x' * 50000 %}{{ [s] * 600 }}",
        ];
        for source in low_cases {
            let (result, elapsed) = render_with(10_000, source);
            assert!(result.is_err(), "{source} should fail under low fuel");
            assert!(
                elapsed < std::time::Duration::from_millis(25),
                "{source} took {elapsed:?} — serialized before refusing"
            );
        }

        // Default fuel: containers over MAX_RENDERED_BYTES (16 MiB) and
        // multi-specifier format expansions must be refused rather than
        // succeeding with 30+ MB intermediate strings.
        let default_fuel = HtmlTemplateLimits::default().fuel;
        let default_cases = [
            "{% set s = 'x' * 100000 %}{{ ([s] * 300) | json | length }}",
            "{% set s = 'x' * 100000 %}{{ ([s] * 300) | tojson | length }}",
            "{% set s = 'x' * 100000 %}{{ ([s] * 300) | pprint | length }}",
            "{% set s = 'x' * 100000 %}{{ ([s] * 300) | safe | length }}",
            "{% set s = 'x' * 100000 %}{{ ([s] * 300) | escape | length }}",
            "{% set s = 'x' * 100000 %}{{ '%s' | format([s] * 300) | length }}",
            "{% set s = 'x' * 100000 %}{{ [[s] * 300] | join | length }}",
            "{{ ('%(a)65535d' * 500) | format(a=1) | length }}",
            "{% set s = 'x' * 100000 %}{{ ('%(a)s' * 300) | format(a=s) | length }}",
        ];
        for source in default_cases {
            let (result, elapsed) = render_with(default_fuel, source);
            assert!(
                result.is_err(),
                "{source} should be refused over MAX_RENDERED_BYTES, got {result:?}"
            );
            assert!(
                elapsed < std::time::Duration::from_millis(50),
                "{source} took {elapsed:?} — serialized before refusing"
            );
        }

        // Legitimate small uses of all wrapped filters keep working.
        for (source, expected) in [
            ("{{ ['a', 'b'] | json }}", "[&quot;a&quot;,&quot;b&quot;]"),
            ("{{ ['a', 'b'] | tojson }}", "[\"a\", \"b\"]"),
            ("{{ '<b>hi</b>' | safe }}", "<b>hi</b>"),
            ("{{ '<b>hi</b>' | escape }}", "&lt;b&gt;hi&lt;&#x2f;b&gt;"),
            ("{{ ['a', 'b'] | join('-') }}", "a-b"),
            ("{{ '%(a)s-%(b)d' | format(a='ok', b=7) }}", "ok-7"),
        ] {
            let (result, _) = render_with(default_fuel, source);
            assert_eq!(result.unwrap(), expected, "{source}");
        }
    }

    /// The DOM outline indents each level by two spaces, skips blank text
    /// (formatting whitespace between tags), quotes text, and shows a
    /// translated element's translation (or error) instead of its children.
    #[test]
    fn outline_size_is_bounded() {
        let limit = MAX_OUTLINE_BYTES + OUTLINE_TRUNCATED.len();
        // Deep (2 × 20 000² / 2 = 400 MB of indent unbounded, bug_0244),
        // wide, and one long line cut inside a multi-byte character.
        for source in [
            "<b>".repeat(20_000),
            "<i></i>".repeat(600_000),
            format!("<p>x{}</p>", "é".repeat(MAX_OUTLINE_BYTES)),
        ] {
            let outline = HtmlDocument::parse(source)
                .unwrap()
                .outline(&LocalizedText::default());
            assert!(outline.len() <= limit, "{} bytes", outline.len());
            assert!(outline.ends_with(OUTLINE_TRUNCATED));
        }
        // An outline that fits has no note.
        let outline = HtmlDocument::parse("<p>short</p>".to_owned())
            .unwrap()
            .outline(&LocalizedText::default());
        assert_eq!(outline, "p\n  \"short\"\n");
    }

    #[test]
    fn outline_indents_levels_and_shows_translations() {
        let document = HtmlDocument::parse(
            "<div class=\"a\">\n  <p>Hi <b>there</b></p>\n  <p data-l10n-id=\"t\">x</p>\n  <p data-l10n-id=\"e\">y</p>\n</div>".to_owned(),
        )
        .unwrap();
        let paragraphs: Vec<tl::NodeHandle> = document.dom().query_selector("p").unwrap().collect();
        let mut localized = LocalizedText::default();
        localized
            .0
            .insert(paragraphs[1], Ok("Translated".to_owned()));
        localized.0.insert(paragraphs[2], Err("missing".to_owned()));
        let expected = "\
div class=\"a\"
  p
    \"Hi\"
    b
      \"there\"
  p data-l10n-id=\"t\"
    l10n \"Translated\"
  p data-l10n-id=\"e\"
    l10n error: missing
";
        assert_eq!(document.outline(&localized), expected);
    }

    /// `aria-*` (and every other) attribute shows verbatim in the outline:
    /// the debug view surfaces accessibility attributes without the
    /// pipeline interpreting them. (Attribute order in `tl` is a map
    /// iteration, so assert per attribute, not on the whole line.)
    #[test]
    fn outline_shows_aria_attributes() {
        let document = HtmlDocument::parse(
            "<button aria-label=\"Close\" aria-disabled=\"true\" data-on-click=\"close\">X</button>"
                .to_owned(),
        )
        .unwrap();
        let outline = document.outline(&LocalizedText::default());
        let line = outline.lines().next().unwrap();
        assert!(line.starts_with("button"), "{line}");
        assert!(line.contains("aria-label=\"Close\""), "{line}");
        assert!(line.contains("aria-disabled=\"true\""), "{line}");
        assert!(line.contains("data-on-click=\"close\""), "{line}");
        assert_eq!(outline.lines().nth(1).unwrap(), "  \"X\"");
    }

    /// Error messages include every `source()` in the chain (Tera puts the
    /// useful detail — the missing variable — in a source).
    #[test]
    fn error_chain_includes_all_sources() {
        #[derive(Debug)]
        struct Layer(&'static str, Option<Box<Layer>>);
        impl std::fmt::Display for Layer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.0)
            }
        }
        impl std::error::Error for Layer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                self.1.as_deref().map(|layer| layer as _)
            }
        }
        let error = Layer(
            "render failed",
            Some(Box::new(Layer(
                "in t.html",
                Some(Box::new(Layer("unknown variable `x`", None))),
            ))),
        );
        assert_eq!(
            error_chain(&error),
            "render failed: in t.html: unknown variable `x`"
        );
    }

    /// Comments, raw blocks and — in Markdown — fenced code blocks and
    /// inline code spans protect their content: an include inside them is
    /// copied verbatim and not resolved (a commented-out include of a
    /// deleted file must not fail the load; bug_0067). In an `.html` file
    /// backticks are text: an include between them is still a reference.
    #[test]
    fn comments_raw_and_code_are_not_references() {
        let markdown = AssetPath::parse("page.md").clone_owned();
        for source in [
            r#"{# {%- include "gone.html" -%} #}"#,
            r#"{% raw %}{% include "gone.html" %}{% endraw %}"#,
            "```html\n{% include \"gone.html\" %}\n```",
            r#"`{% include "gone.html" %}`"#,
        ] {
            let (out, references) = resolve_references(&markdown, source, false).unwrap();
            assert!(references.is_empty(), "{source:?}: {references:?}");
            assert_eq!(out, source, "{source:?}: rewritten");
        }
        let html = AssetPath::parse("page.html").clone_owned();
        let (_, references) =
            resolve_references(&html, r#"`{% include "part.html" %}`"#, false).unwrap();
        assert_eq!(references.len(), 1, "{references:?}");
    }
}
/// bug_0087: an include escaping the asset root is refused unless the
/// app's `AssetPlugin::unapproved_path_mode` is `Allow`.
#[test]
fn includes_outside_the_root_follow_the_unapproved_mode() {
    let path = AssetPath::from("pages/page.html");
    let source = r#"{% include "../../outside.html" %}"#;
    assert!(resolve_references(&path, source, false).is_err());
    let (out, references) = resolve_references(&path, source, true).unwrap();
    assert_eq!(references.len(), 1, "{references:?}");
    assert!(references[0].is_unapproved(), "{out}");
}
