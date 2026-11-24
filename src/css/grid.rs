//! CSS Grid values: track lists, `grid-template-areas`, line placements and
//! the `grid`/`grid-template` shorthands (CSS Grid 2, minus `subgrid` and
//! `masonry`). The cascade maps these to Bevy's grid fields
//! (`cascade::track_sizing`, `parse_areas`, `grid_line`).

use cssparser::{Parser, Token};

use super::value::{Lp, parse_integer, parse_lp};
use super::{CssError, Parsed, invalid};

/// `grid-template-rows`/`-columns`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TrackSizing {
    None,
    TrackList(TrackList),
}

/// A track list: `line_names[i]` precedes `items[i]`; one more names list
/// follows the last item.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TrackList {
    pub(crate) line_names: Vec<Vec<String>>,
    pub(crate) items: Vec<TrackListItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TrackListItem {
    TrackSize(TrackSize),
    TrackRepeat(TrackRepeat),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TrackRepeat {
    pub(crate) count: RepeatCount,
    pub(crate) line_names: Vec<Vec<String>>,
    pub(crate) track_sizes: Vec<TrackSize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RepeatCount {
    Number(i32),
    AutoFill,
    AutoFit,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TrackSize {
    TrackBreadth(TrackBreadth),
    MinMax {
        min: TrackBreadth,
        max: TrackBreadth,
    },
    FitContent(Lp),
}

impl Default for TrackSize {
    fn default() -> Self {
        TrackSize::TrackBreadth(TrackBreadth::Auto)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TrackBreadth {
    Length(Lp),
    /// `fr`.
    Flex(f32),
    Auto,
    MinContent,
    MaxContent,
}

/// `grid-template-areas`: the cells row by row, `None` for a `.` cell.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GridTemplateAreas {
    None,
    Areas {
        columns: u32,
        areas: Vec<Option<String>>,
    },
}

/// One placement side.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GridLine {
    Auto,
    /// A bare name: an area or a named line.
    Area {
        name: String,
    },
    Line {
        index: i32,
        name: Option<String>,
    },
    Span {
        index: i32,
        name: Option<String>,
    },
}

impl GridLine {
    /// The end a shorthand gives an omitted side: a bare name repeats,
    /// anything else is `auto`.
    fn default_end(&self) -> GridLine {
        match self {
            GridLine::Area { .. } => self.clone(),
            _ => GridLine::Auto,
        }
    }
}

/// `grid-auto-flow`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct GridAutoFlow {
    pub(crate) column: bool,
    pub(crate) dense: bool,
}

/// The `grid` shorthand's longhands.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Grid {
    pub(crate) rows: TrackSizing,
    pub(crate) columns: TrackSizing,
    pub(crate) areas: GridTemplateAreas,
    pub(crate) auto_rows: Vec<TrackSize>,
    pub(crate) auto_columns: Vec<TrackSize>,
    pub(crate) auto_flow: GridAutoFlow,
}

/// The `grid-template` shorthand's longhands.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GridTemplate {
    pub(crate) rows: TrackSizing,
    pub(crate) columns: TrackSizing,
    pub(crate) areas: GridTemplateAreas,
}

/// A `<custom-ident>`: any identifier but the CSS-wide keywords and
/// `default` (and, in grid, `span` and `auto`).
fn custom_ident<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, String> {
    let location = input.current_source_location();
    let ident = input.expect_ident()?.as_ref().to_owned();
    match ident.to_ascii_lowercase().as_str() {
        "initial" | "inherit" | "unset" | "revert" | "revert-layer" | "default" | "span"
        | "auto" => Err(location.new_custom_error(CssError::Invalid)),
        _ => Ok(ident),
    }
}

/// `[name name …]`.
fn line_names<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Vec<String>> {
    input.expect_square_bracket_block()?;
    input.parse_nested_block(|names| {
        let mut list = Vec::new();
        while let Ok(name) = names.try_parse(custom_ident) {
            list.push(name);
        }
        names.expect_exhausted()?;
        Ok(list)
    })
}

/// A breadth; `fr` only where `flex` allows it.
fn track_breadth<'i>(input: &mut Parser<'i, '_>, flex: bool) -> Parsed<'i, TrackBreadth> {
    if let Ok(lp) = input.try_parse(parse_lp) {
        return Ok(TrackBreadth::Length(lp));
    }
    if flex
        && let Ok(fr) = input.try_parse(|i| {
            let location = i.current_source_location();
            match *i.next()? {
                Token::Dimension {
                    value, ref unit, ..
                } if unit.eq_ignore_ascii_case("fr")
                    && value.is_sign_positive()
                    && value.is_finite() =>
                {
                    Ok(value)
                }
                _ => Err::<f32, super::Error<'i>>(location.new_custom_error(CssError::Invalid)),
            }
        })
    {
        return Ok(TrackBreadth::Flex(fr));
    }
    let location = input.current_source_location();
    match input.expect_ident()?.to_ascii_lowercase().as_str() {
        "auto" => Ok(TrackBreadth::Auto),
        "min-content" => Ok(TrackBreadth::MinContent),
        "max-content" => Ok(TrackBreadth::MaxContent),
        _ => Err(location.new_custom_error(CssError::Invalid)),
    }
}

/// A track size: a breadth, `minmax()` or `fit-content()`.
pub(crate) fn track_size<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, TrackSize> {
    if let Ok(breadth) = input.try_parse(|i| track_breadth(i, true)) {
        return Ok(TrackSize::TrackBreadth(breadth));
    }
    let location = input.current_source_location();
    let name = input.expect_function()?.to_ascii_lowercase();
    match name.as_str() {
        "minmax" => input.parse_nested_block(|args| {
            let min = track_breadth(args, false)?;
            args.expect_comma()?;
            let max = track_breadth(args, true)?;
            args.expect_exhausted()?;
            Ok(TrackSize::MinMax { min, max })
        }),
        "fit-content" => input.parse_nested_block(|args| {
            let lp = parse_lp(args)?;
            args.expect_exhausted()?;
            Ok(TrackSize::FitContent(lp))
        }),
        _ => Err(location.new_custom_error(CssError::Invalid)),
    }
}

fn track_repeat<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, TrackRepeat> {
    input.expect_function_matching("repeat")?;
    input.parse_nested_block(|args| {
        let count = if let Ok(n) = args.try_parse(parse_integer) {
            RepeatCount::Number(n)
        } else {
            let location = args.current_source_location();
            match args.expect_ident()?.to_ascii_lowercase().as_str() {
                "auto-fill" => RepeatCount::AutoFill,
                "auto-fit" => RepeatCount::AutoFit,
                _ => return Err(location.new_custom_error(CssError::Invalid)),
            }
        };
        args.expect_comma()?;
        let mut line_names_list = Vec::new();
        let mut track_sizes = Vec::new();
        loop {
            line_names_list.push(args.try_parse(line_names).unwrap_or_default());
            match args.try_parse(track_size) {
                Ok(size) => track_sizes.push(size),
                Err(_) => break,
            }
        }
        args.expect_exhausted()?;
        if track_sizes.is_empty() {
            return invalid(args);
        }
        Ok(TrackRepeat {
            count,
            line_names: line_names_list,
            track_sizes,
        })
    })
}

fn track_list<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, TrackList> {
    let mut names = Vec::new();
    let mut items = Vec::new();
    loop {
        names.push(input.try_parse(line_names).unwrap_or_default());
        if let Ok(size) = input.try_parse(track_size) {
            items.push(TrackListItem::TrackSize(size));
        } else if let Ok(repeat) = input.try_parse(track_repeat) {
            items.push(TrackListItem::TrackRepeat(repeat));
        } else {
            break;
        }
    }
    if items.is_empty() {
        return invalid(input);
    }
    Ok(TrackList {
        line_names: names,
        items,
    })
}

/// `grid-template-rows`/`-columns`.
pub(crate) fn track_sizing<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, TrackSizing> {
    if input.try_parse(|i| i.expect_ident_matching("none")).is_ok() {
        return Ok(TrackSizing::None);
    }
    Ok(TrackSizing::TrackList(track_list(input)?))
}

/// `grid-auto-rows`/`-columns`: one or more track sizes (a lone `auto`,
/// the initial value, is the empty list).
pub(crate) fn track_size_list<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Vec<TrackSize>> {
    let mut sizes = Vec::new();
    while let Ok(size) = input.try_parse(track_size) {
        sizes.push(size);
    }
    if sizes.is_empty() {
        return invalid(input);
    }
    if sizes.len() == 1 && sizes[0] == TrackSize::default() {
        sizes.clear();
    }
    Ok(sizes)
}

/// The cells of one `grid-template-areas` string: its column count.
fn area_string(string: &str, cells: &mut Vec<Option<String>>) -> Option<u32> {
    const SPACE: &[char] = &[' ', '\t', '\n', '\x0c', '\r'];
    let name_char = |c: char| c.is_ascii_alphanumeric() || c >= '\u{80}' || c == '_' || c == '-';
    let mut rest = string;
    let mut columns = 0;
    loop {
        rest = rest.trim_start_matches(SPACE);
        if rest.is_empty() {
            return (columns > 0).then_some(columns);
        }
        columns += 1;
        if rest.starts_with('.') {
            rest = rest.trim_start_matches('.');
            cells.push(None);
            continue;
        }
        let len = rest.find(|c| !name_char(c)).unwrap_or(rest.len());
        if len == 0 {
            return None;
        }
        cells.push(Some(rest[..len].to_owned()));
        rest = &rest[len..];
    }
}

/// `grid-template-areas`.
pub(crate) fn template_areas<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, GridTemplateAreas> {
    if input.try_parse(|i| i.expect_ident_matching("none")).is_ok() {
        return Ok(GridTemplateAreas::None);
    }
    let mut cells = Vec::new();
    let mut columns = 0;
    let mut rows = 0;
    while let Ok(string) = input.try_parse(|i| i.expect_string().map(|s| s.as_ref().to_owned())) {
        let Some(count) = area_string(&string, &mut cells) else {
            return invalid(input);
        };
        if rows == 0 {
            columns = count;
        } else if count != columns {
            return invalid(input);
        }
        rows += 1;
    }
    if rows == 0 {
        return invalid(input);
    }
    Ok(GridTemplateAreas::Areas {
        columns,
        areas: cells,
    })
}

/// Whether a track list has no `auto-fill`/`auto-fit` repeat.
fn is_explicit(list: &TrackList) -> bool {
    list.items.iter().all(|item| {
        !matches!(
            item,
            TrackListItem::TrackRepeat(TrackRepeat {
                count: RepeatCount::AutoFill | RepeatCount::AutoFit,
                ..
            })
        )
    })
}

/// `grid-template`: `none`, `<rows> / <columns>`, or the areas form
/// (`[names]? "cells" <size>? [names]?`… `[/ <columns>]?`).
pub(crate) fn template<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, GridTemplate> {
    if input.try_parse(|i| i.expect_ident_matching("none")).is_ok() {
        return Ok(GridTemplate {
            rows: TrackSizing::None,
            columns: TrackSizing::None,
            areas: GridTemplateAreas::None,
        });
    }
    let start = input.state();
    let mut names: Vec<Vec<String>> = Vec::new();
    let mut items = Vec::new();
    let mut cells = Vec::new();
    let mut columns = 0;
    let mut rows = 0;
    loop {
        if let Ok(first) = input.try_parse(line_names) {
            match names.last_mut() {
                Some(last) => last.extend(first),
                None => names.push(first),
            }
        }
        let Ok(string) = input.try_parse(|i| i.expect_string().map(|s| s.as_ref().to_owned()))
        else {
            break;
        };
        let Some(count) = area_string(&string, &mut cells) else {
            return invalid(input);
        };
        if rows == 0 {
            columns = count;
        } else if count != columns {
            return invalid(input);
        }
        rows += 1;
        if names.len() == items.len() {
            names.push(Vec::new());
        }
        items.push(TrackListItem::TrackSize(
            input.try_parse(track_size).unwrap_or_default(),
        ));
        names.push(input.try_parse(line_names).unwrap_or_default());
    }
    if !cells.is_empty() {
        if names.len() == items.len() {
            names.push(Vec::new());
        }
        let areas = GridTemplateAreas::Areas {
            columns,
            areas: cells,
        };
        let rows = TrackSizing::TrackList(TrackList {
            line_names: names,
            items,
        });
        let columns = if input.try_parse(|i| i.expect_delim('/')).is_ok() {
            let list = track_list(input)?;
            if !is_explicit(&list) {
                return invalid(input);
            }
            TrackSizing::TrackList(list)
        } else {
            TrackSizing::None
        };
        return Ok(GridTemplate {
            rows,
            columns,
            areas,
        });
    }
    input.reset(&start);
    let rows = track_sizing(input)?;
    input.expect_delim('/')?;
    let columns = track_sizing(input)?;
    Ok(GridTemplate {
        rows,
        columns,
        areas: GridTemplateAreas::None,
    })
}

/// `grid-auto-flow`: `[row | column] || dense` (a bare `dense` is
/// `row dense`, as CSS defines; lightningcss refused it, UPSTREAM U10).
pub(crate) fn auto_flow<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, GridAutoFlow> {
    let mut flow = GridAutoFlow::default();
    let mut axis = false;
    let mut seen = false;
    while let Ok(ident) = input.try_parse(|i| i.expect_ident().map(|s| s.to_ascii_lowercase())) {
        match ident.as_str() {
            "row" | "column" if !axis => {
                axis = true;
                flow.column = ident == "column";
            }
            "dense" if !flow.dense => flow.dense = true,
            _ => return invalid(input),
        }
        seen = true;
    }
    if !seen {
        return invalid(input);
    }
    Ok(flow)
}

/// `auto-flow && dense?` in the `grid` shorthand.
fn shorthand_auto_flow<'i>(input: &mut Parser<'i, '_>, column: bool) -> Parsed<'i, GridAutoFlow> {
    let mut flow = GridAutoFlow {
        column,
        dense: false,
    };
    if input
        .try_parse(|i| i.expect_ident_matching("auto-flow"))
        .is_ok()
    {
        flow.dense = input
            .try_parse(|i| i.expect_ident_matching("dense"))
            .is_ok();
        return Ok(flow);
    }
    input.expect_ident_matching("dense")?;
    input.expect_ident_matching("auto-flow")?;
    flow.dense = true;
    Ok(flow)
}

/// `grid`.
pub(crate) fn grid<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, Grid> {
    if let Ok(template) = input.try_parse(|i| {
        let template = template(i)?;
        i.expect_exhausted()?;
        Ok::<_, super::Error<'i>>(template)
    }) {
        return Ok(Grid {
            rows: template.rows,
            columns: template.columns,
            areas: template.areas,
            auto_rows: Vec::new(),
            auto_columns: Vec::new(),
            auto_flow: GridAutoFlow::default(),
        });
    }
    if let Ok(rows) = input.try_parse(track_sizing) {
        input.expect_delim('/')?;
        let auto_flow = shorthand_auto_flow(input, true)?;
        let auto_columns = input.try_parse(track_size_list).unwrap_or_default();
        return Ok(Grid {
            rows,
            columns: TrackSizing::None,
            areas: GridTemplateAreas::None,
            auto_rows: Vec::new(),
            auto_columns,
            auto_flow,
        });
    }
    let auto_flow = shorthand_auto_flow(input, false)?;
    let auto_rows = input.try_parse(track_size_list).unwrap_or_default();
    input.expect_delim('/')?;
    let columns = track_sizing(input)?;
    Ok(Grid {
        rows: TrackSizing::None,
        columns,
        areas: GridTemplateAreas::None,
        auto_rows,
        auto_columns: Vec::new(),
        auto_flow,
    })
}

/// One placement side: `auto`, `span <n>? <name>?`, `<n> <name>?`,
/// `<name> <n>?`.
pub(crate) fn line<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, GridLine> {
    if input.try_parse(|i| i.expect_ident_matching("auto")).is_ok() {
        return Ok(GridLine::Auto);
    }
    if input.try_parse(|i| i.expect_ident_matching("span")).is_ok() {
        let (index, name) = if let Ok(n) = input.try_parse(parse_integer) {
            (n, input.try_parse(custom_ident).ok())
        } else if let Ok(name) = input.try_parse(custom_ident) {
            (input.try_parse(parse_integer).unwrap_or(1), Some(name))
        } else {
            return invalid(input);
        };
        if index <= 0 {
            return invalid(input);
        }
        return Ok(GridLine::Span { index, name });
    }
    if let Ok(index) = input.try_parse(parse_integer) {
        if index == 0 {
            return invalid(input);
        }
        let name = input.try_parse(custom_ident).ok();
        return Ok(GridLine::Line { index, name });
    }
    let name = custom_ident(input)?;
    if let Ok(index) = input.try_parse(parse_integer) {
        if index == 0 {
            return invalid(input);
        }
        return Ok(GridLine::Line {
            index,
            name: Some(name),
        });
    }
    Ok(GridLine::Area { name })
}

/// `grid-row`/`grid-column`: `start [/ end]`.
pub(crate) fn line_pair<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, [GridLine; 2]> {
    let start = line(input)?;
    let end = if input.try_parse(|i| i.expect_delim('/')).is_ok() {
        line(input)?
    } else {
        start.default_end()
    };
    Ok([start, end])
}

/// `grid-area`: `row-start [/ column-start [/ row-end [/ column-end]]]`,
/// as `[row-start, row-end, column-start, column-end]`.
pub(crate) fn area<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, [GridLine; 4]> {
    let mut sides = vec![line(input)?];
    while sides.len() < 4 && input.try_parse(|i| i.expect_delim('/')).is_ok() {
        sides.push(line(input)?);
    }
    let row_start = sides[0].clone();
    let column_start = sides
        .get(1)
        .cloned()
        .unwrap_or_else(|| row_start.default_end());
    let row_end = sides
        .get(2)
        .cloned()
        .unwrap_or_else(|| row_start.default_end());
    let column_end = sides
        .get(3)
        .cloned()
        .unwrap_or_else(|| column_start.default_end());
    Ok([row_start, row_end, column_start, column_end])
}
