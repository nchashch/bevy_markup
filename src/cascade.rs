//! Stylesheet → declared style per element (the subset documented in
//! [`crate::style`]): compound selectors matched against tag, id and classes.

use std::cell::RefCell;

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use lightningcss::properties::Property;
use lightningcss::properties::border::BorderSideWidth;
use lightningcss::properties::border_image::{
    BorderImageRepeat, BorderImageRepeatKeyword, BorderImageSlice,
};
use lightningcss::properties::font::{
    AbsoluteFontSize, AbsoluteFontWeight, FontFamily, FontSize, FontStyle, FontWeight,
    GenericFontFamily, RelativeFontSize,
};
use lightningcss::rules::CssRule;
use lightningcss::stylesheet::{PrinterOptions, StyleSheet};
use lightningcss::traits::ToCss;
use lightningcss::values::color::{CssColor, RGBA};
use lightningcss::values::image::Image;
use lightningcss::values::length::{LengthPercentage, LengthPercentageOrAuto, LengthValue};
use lightningcss::values::percentage::NumberOrPercentage;

use crate::fonts::{FamilyRef, GenericFamily};
use crate::html::HtmlElement;

/// A declared `font-size`, resolved against inherited/root sizes later.
#[derive(Clone, Copy, Debug)]
pub(crate) enum FontSizeSpec {
    Px(f32),
    /// Multiple of the inherited size (`em`, `%`, `smaller`/`larger`).
    Inherited(f32),
    /// Multiple of the root (`html`) size.
    Root(f32),
}

impl FontSizeSpec {
    pub fn resolve(self, inherited: f32, root: f32) -> f32 {
        match self {
            Self::Px(px) => px,
            Self::Inherited(factor) => inherited * factor,
            Self::Root(factor) => root * factor,
        }
    }
}

/// One `border-image-slice` offset.
#[derive(Clone, Copy, Debug)]
pub(crate) enum SliceValue {
    /// Image pixels (a plain number).
    Px(f32),
    /// Fraction of the image's width (left/right) or height (top/bottom).
    Fraction(f32),
}

/// Declared `border-image` (shorthand and/or longhands; each part optional so
/// longhands override parts of an earlier shorthand).
#[derive(Clone, Debug, Default)]
pub(crate) struct BorderImageDecl {
    /// The `url(...)` as written; `Some(None)` = `none`.
    pub source: Option<Option<String>>,
    /// `[top, right, bottom, left]` and `fill`.
    pub slice: Option<([SliceValue; 4], bool)>,
    /// `repeat`/`round`/`space` (tiled) vs `stretch`.
    pub tile: Option<bool>,
}

/// Declared (not computed) style for one element type.
#[derive(Clone, Default, Debug)]
pub(crate) struct ElementStyle {
    pub color: Option<Color>,
    pub background: Option<Color>,
    /// The `font-family` list as written.
    pub font_family: Option<Vec<FamilyRef>>,
    pub font_size: Option<FontSizeSpec>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub border_image: Option<BorderImageDecl>,
    /// `[top, right, bottom, left]` in px.
    pub border_width: [Option<f32>; 4],
    /// `[top, right, bottom, left]` in px.
    pub padding: [Option<f32>; 4],
}

/// Every `border-image-source` URL in `sheet`, as written.
pub(crate) fn image_urls(sheet: &StyleSheet) -> Vec<String> {
    let mut urls = Vec::new();
    for rule in &sheet.rules.0 {
        let CssRule::Style(rule) = rule else {
            continue;
        };
        let declarations = rule
            .declarations
            .declarations
            .iter()
            .chain(&rule.declarations.important_declarations);
        for declaration in declarations {
            let image = match declaration {
                Property::BorderImage(border_image, _) => &border_image.source,
                Property::BorderImageSource(image) => image,
                _ => continue,
            };
            if let Some(url) = image_url(image)
                && !urls.contains(&url)
            {
                urls.push(url);
            }
        }
    }
    urls
}

/// A compound selector: optional type (or `*`), then any number of `.class`
/// and `#id` parts, e.g. `p.note`, `.a.b`, `#title`.
#[derive(Clone, Debug)]
struct Compound {
    /// Lowercase; `None` for `*` or no type.
    tag: Option<String>,
    ids: Vec<String>,
    classes: Vec<String>,
}

impl Compound {
    /// `None` for anything else (combinators, attributes, pseudo-classes).
    fn parse(selector: &str) -> Option<Self> {
        let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
        let mut rest = selector;
        let mut tag = None;
        if let Some(after) = rest.strip_prefix('*') {
            rest = after;
        } else {
            let end = rest.find(|c| !is_ident(c)).unwrap_or(rest.len());
            if end > 0 {
                tag = Some(rest[..end].to_ascii_lowercase());
                rest = &rest[end..];
            }
        }
        let mut ids = Vec::new();
        let mut classes = Vec::new();
        while let Some(kind) = rest.chars().next() {
            let body = &rest[1..];
            let end = body.find(|c| !is_ident(c)).unwrap_or(body.len());
            if end == 0 {
                return None;
            }
            match kind {
                '.' => classes.push(body[..end].to_owned()),
                '#' => ids.push(body[..end].to_owned()),
                _ => return None,
            }
            rest = &body[end..];
        }
        if tag.is_none() && ids.is_empty() && classes.is_empty() && selector != "*" {
            return None;
        }
        Some(Self { tag, ids, classes })
    }

    /// CSS specificity: (ids, classes, types).
    fn specificity(&self) -> (u32, u32, u32) {
        (
            self.ids.len() as u32,
            self.classes.len() as u32,
            self.tag.is_some() as u32,
        )
    }

    fn matches(&self, element: &HtmlElement) -> bool {
        self.tag.as_ref().is_none_or(|tag| *tag == element.tag)
            && self.ids.iter().all(|id| element.id.as_ref() == Some(id))
            && self.classes.iter().all(|class| element.has_class(class))
    }
}

/// One selector of a style rule with one importance level's declarations.
struct Rule<'a> {
    selector: Compound,
    /// Sort key: importance, then specificity, then source order.
    rank: (bool, (u32, u32, u32), usize),
    declarations: &'a [Property<'static>],
}

/// A stylesheet's rules, matched per element (cached by tag + id + classes).
#[derive(Default)]
pub(crate) struct HtmlStyles<'a> {
    rules: Vec<Rule<'a>>,
    cache: RefCell<HashMap<(String, Option<String>, Vec<String>), ElementStyle>>,
}

impl<'a> HtmlStyles<'a> {
    pub fn from_sheet(sheet: &'a StyleSheet<'static>) -> Self {
        let mut rules = Vec::new();
        for (order, rule) in sheet.rules.0.iter().enumerate() {
            let CssRule::Style(rule) = rule else {
                continue;
            };
            for selector in rule.selectors.0.iter() {
                let Ok(text) = selector.to_css_string(PrinterOptions::default()) else {
                    continue;
                };
                let Some(compound) = Compound::parse(&text) else {
                    debug!("html css: skipping unsupported selector `{text}`");
                    continue;
                };
                let specificity = compound.specificity();
                for (important, declarations) in [
                    (false, &rule.declarations.declarations),
                    (true, &rule.declarations.important_declarations),
                ] {
                    if declarations.is_empty() {
                        continue;
                    }
                    rules.push(Rule {
                        selector: compound.clone(),
                        rank: (important, specificity, order),
                        declarations,
                    });
                }
            }
        }
        // Apply in ascending rank: later application wins.
        rules.sort_by_key(|rule| rule.rank);
        Self {
            rules,
            cache: RefCell::default(),
        }
    }

    /// The declared style for `element`: every matching rule's declarations,
    /// applied in cascade order.
    pub fn get(&self, element: &HtmlElement) -> ElementStyle {
        let key = (element.tag.clone(), element.id.clone(), element.classes.clone());
        if let Some(style) = self.cache.borrow().get(&key) {
            return style.clone();
        }
        let mut style = ElementStyle::default();
        for rule in self.rules.iter().filter(|rule| rule.selector.matches(element)) {
            for declaration in rule.declarations {
                apply(&mut style, declaration);
            }
        }
        self.cache.borrow_mut().insert(key, style.clone());
        style
    }
}

fn apply(style: &mut ElementStyle, declaration: &Property) {
    match declaration {
        Property::Color(color) => {
            if let Some(color) = to_color(color) {
                style.color = Some(color);
            }
        }
        Property::BackgroundColor(color) => {
            if let Some(color) = to_color(color) {
                style.background = Some(color);
            }
        }
        Property::FontFamily(families) => {
            let list: Vec<FamilyRef> = families.iter().filter_map(family_ref).collect();
            if !list.is_empty() {
                style.font_family = Some(list);
            }
        }
        Property::FontSize(size) => {
            if let Some(size) = font_size(size) {
                style.font_size = Some(size);
            }
        }
        Property::FontWeight(weight) => {
            style.bold = Some(match weight {
                FontWeight::Absolute(AbsoluteFontWeight::Bold) | FontWeight::Bolder => true,
                FontWeight::Absolute(AbsoluteFontWeight::Normal) | FontWeight::Lighter => false,
                FontWeight::Absolute(AbsoluteFontWeight::Weight(weight)) => *weight >= 600.0,
            });
        }
        Property::FontStyle(font_style) => {
            style.italic = Some(!matches!(font_style, FontStyle::Normal));
        }
        Property::BorderImage(border_image, _) => {
            style.border_image = Some(BorderImageDecl {
                source: Some(image_url(&border_image.source)),
                slice: Some(slice(&border_image.slice)),
                tile: Some(tile(&border_image.repeat)),
            });
        }
        Property::BorderImageSource(image) => {
            style.border_image.get_or_insert_default().source = Some(image_url(image));
        }
        Property::BorderImageSlice(value) => {
            style.border_image.get_or_insert_default().slice = Some(slice(value));
        }
        Property::BorderImageRepeat(repeat) => {
            style.border_image.get_or_insert_default().tile = Some(tile(repeat));
        }
        Property::BorderImageWidth(_) | Property::BorderImageOutset(_) => {
            debug!("html css: border-image-width/-outset unsupported (corners draw at image size)");
        }
        Property::BorderWidth(width) => {
            style.border_width = [&width.top, &width.right, &width.bottom, &width.left]
                .map(side_width);
        }
        Property::BorderTopWidth(width) => style.border_width[0] = side_width(width),
        Property::BorderRightWidth(width) => style.border_width[1] = side_width(width),
        Property::BorderBottomWidth(width) => style.border_width[2] = side_width(width),
        Property::BorderLeftWidth(width) => style.border_width[3] = side_width(width),
        Property::Padding(padding) => {
            style.padding = [&padding.top, &padding.right, &padding.bottom, &padding.left]
                .map(length_px);
        }
        Property::PaddingTop(value) => style.padding[0] = length_px(value),
        Property::PaddingRight(value) => style.padding[1] = length_px(value),
        Property::PaddingBottom(value) => style.padding[2] = length_px(value),
        Property::PaddingLeft(value) => style.padding[3] = length_px(value),
        _ => {}
    }
}

fn image_url(image: &Image) -> Option<String> {
    match image {
        Image::Url(url) => Some(url.url.to_string()),
        Image::None => None,
        _ => {
            debug!("html css: only url() border-image sources are supported");
            None
        }
    }
}

fn slice(value: &BorderImageSlice) -> ([SliceValue; 4], bool) {
    let offsets = &value.offsets;
    let side = |offset: &NumberOrPercentage| match offset {
        NumberOrPercentage::Number(px) => SliceValue::Px(*px),
        NumberOrPercentage::Percentage(percent) => SliceValue::Fraction(percent.0),
    };
    (
        [side(&offsets.0), side(&offsets.1), side(&offsets.2), side(&offsets.3)],
        value.fill,
    )
}

fn tile(repeat: &BorderImageRepeat) -> bool {
    // Bevy has one scale mode for all sides: tile if either axis tiles.
    [repeat.horizontal, repeat.vertical]
        .iter()
        .any(|keyword| !matches!(keyword, BorderImageRepeatKeyword::Stretch))
}

fn side_width(width: &BorderSideWidth) -> Option<f32> {
    match width {
        BorderSideWidth::Thin => Some(1.0),
        BorderSideWidth::Medium => Some(3.0),
        BorderSideWidth::Thick => Some(5.0),
        BorderSideWidth::Length(length) => length.to_px(),
    }
}

/// `padding` in px; `auto`, `%` and `calc()` are unsupported.
fn length_px(value: &LengthPercentageOrAuto) -> Option<f32> {
    match value {
        LengthPercentageOrAuto::LengthPercentage(LengthPercentage::Dimension(length)) => {
            length.to_px()
        }
        _ => {
            debug!("html css: only absolute lengths are supported for padding");
            None
        }
    }
}

/// `None` for colors without a fixed sRGB value (`currentColor`, system colors).
fn to_color(color: &CssColor) -> Option<Color> {
    let rgba = RGBA::try_from(color).ok()?;
    Some(Color::srgba_u8(rgba.red, rgba.green, rgba.blue, rgba.alpha))
}

fn family_ref(family: &FontFamily) -> Option<FamilyRef> {
    Some(match family {
        FontFamily::FamilyName(name) => FamilyRef::Named(
            name.to_css_string(PrinterOptions::default())
                .ok()?
                .trim_matches(['"', '\''])
                .to_owned(),
        ),
        FontFamily::Generic(generic) => FamilyRef::Generic(match generic {
            GenericFontFamily::Serif => GenericFamily::Serif,
            GenericFontFamily::SansSerif => GenericFamily::SansSerif,
            GenericFontFamily::Monospace => GenericFamily::Monospace,
            GenericFontFamily::Cursive => GenericFamily::Cursive,
            GenericFontFamily::Fantasy => GenericFamily::Fantasy,
            GenericFontFamily::SystemUI => GenericFamily::SystemUi,
            _ => return None,
        }),
    })
}

fn font_size(size: &FontSize) -> Option<FontSizeSpec> {
    Some(match size {
        FontSize::Length(LengthPercentage::Dimension(length)) => match length {
            LengthValue::Px(px) => FontSizeSpec::Px(*px),
            LengthValue::Em(em) => FontSizeSpec::Inherited(*em),
            LengthValue::Rem(rem) => FontSizeSpec::Root(*rem),
            _ => {
                debug!("html css: unsupported font-size unit");
                return None;
            }
        },
        FontSize::Length(LengthPercentage::Percentage(percent)) => {
            FontSizeSpec::Inherited(percent.0)
        }
        FontSize::Length(LengthPercentage::Calc(_)) => {
            debug!("html css: calc() font-size unsupported");
            return None;
        }
        // CSS Fonts 4 keyword scale with `medium` = 16px.
        FontSize::Absolute(keyword) => FontSizeSpec::Px(match keyword {
            AbsoluteFontSize::XXSmall => 9.0,
            AbsoluteFontSize::XSmall => 10.0,
            AbsoluteFontSize::Small => 13.0,
            AbsoluteFontSize::Medium => 16.0,
            AbsoluteFontSize::Large => 18.0,
            AbsoluteFontSize::XLarge => 24.0,
            AbsoluteFontSize::XXLarge => 32.0,
            AbsoluteFontSize::XXXLarge => 48.0,
        }),
        FontSize::Relative(RelativeFontSize::Smaller) => FontSizeSpec::Inherited(1.0 / 1.2),
        FontSize::Relative(RelativeFontSize::Larger) => FontSizeSpec::Inherited(1.2),
    })
}

#[cfg(test)]
mod tests {
    use lightningcss::stylesheet::ParserOptions;

    use super::*;

    fn sheet(css: &'static str) -> StyleSheet<'static> {
        StyleSheet::parse(css, ParserOptions::default()).expect("valid css")
    }

    fn element(tag: &str, id: Option<&str>, classes: &[&str]) -> HtmlElement {
        HtmlElement {
            tag: tag.to_owned(),
            id: id.map(str::to_owned),
            classes: classes.iter().map(|class| (*class).to_owned()).collect(),
        }
    }

    const RED: Color = Color::srgb_u8(255, 0, 0);
    const GREEN: Color = Color::srgb_u8(0, 128, 0);
    const BLUE: Color = Color::srgb_u8(0, 0, 255);

    fn color(css: &'static str, element: &HtmlElement) -> Option<Color> {
        let sheet = sheet(css);
        HtmlStyles::from_sheet(&sheet).get(element).color
    }

    #[test]
    fn more_specific_selector_wins_regardless_of_order() {
        let note = element("p", None, &["note"]);
        assert_eq!(color(".note { color: green } p { color: red }", &note), Some(GREEN));
        assert_eq!(color(".note { color: red } p.note { color: green }", &note), Some(GREEN));
        let titled = element("p", Some("title"), &["note", "big"]);
        assert_eq!(color("#title { color: green } p.note.big { color: red }", &titled), Some(GREEN));
    }

    #[test]
    fn equal_specificity_later_rule_wins() {
        let both = element("p", None, &["a", "b"]);
        assert_eq!(color(".a { color: red } .b { color: green }", &both), Some(GREEN));
        assert_eq!(color(".b { color: green } .a { color: red }", &both), Some(RED));
    }

    #[test]
    fn important_beats_specificity() {
        let titled = element("p", Some("title"), &[]);
        assert_eq!(color("p { color: green !important } #title { color: red }", &titled), Some(GREEN));
    }

    #[test]
    fn compound_requires_every_part() {
        let css = "p.note.big { color: red } h1#x { color: blue }";
        assert_eq!(color(css, &element("p", None, &["note"])), None);
        assert_eq!(color(css, &element("div", None, &["note", "big"])), None);
        assert_eq!(color(css, &element("p", None, &["big", "note", "extra"])), Some(RED));
        assert_eq!(color(css, &element("h1", Some("y"), &[])), None);
        assert_eq!(color(css, &element("h1", Some("x"), &[])), Some(BLUE));
    }

    #[test]
    fn comma_list_matches_each_selector_with_its_own_specificity() {
        // `.note` in the list beats the later, less specific `p`.
        let css = "h1, .note { color: green } p { color: red }";
        assert_eq!(color(css, &element("p", None, &["note"])), Some(GREEN));
        assert_eq!(color(css, &element("p", None, &[])), Some(RED));
        assert_eq!(color(css, &element("h1", None, &[])), Some(GREEN));
    }

    #[test]
    fn universal_loses_to_everything_and_unsupported_selectors_are_skipped() {
        let css = "* { color: red } p { color: green } div p { color: blue } p:hover { color: blue }";
        assert_eq!(color(css, &element("p", None, &[])), Some(GREEN));
        assert_eq!(color(css, &element("span", None, &[])), Some(RED));
    }
}
