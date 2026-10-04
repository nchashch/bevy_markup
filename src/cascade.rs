//! Stylesheet → declared style per element type (the subset documented in
//! [`crate::style`]).

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

/// Element type (lowercase tag name) → declared style.
#[derive(Default)]
pub(crate) struct HtmlStyles {
    styles: HashMap<String, ElementStyle>,
    empty: ElementStyle,
}

impl HtmlStyles {
    pub fn from_sheet(sheet: &StyleSheet) -> Self {
        let mut styles: HashMap<String, ElementStyle> = HashMap::default();
        // Normal declarations first, then `!important` ones, each pass in
        // source order: later wins, important beats normal.
        for important in [false, true] {
            for rule in &sheet.rules.0 {
                let CssRule::Style(rule) = rule else {
                    continue;
                };
                let declarations = if important {
                    &rule.declarations.important_declarations
                } else {
                    &rule.declarations.declarations
                };
                if declarations.is_empty() {
                    continue;
                }
                for selector in rule.selectors.0.iter() {
                    let Ok(name) = selector.to_css_string(PrinterOptions::default()) else {
                        continue;
                    };
                    if !is_type_selector(&name) {
                        debug!("html css: skipping unsupported selector `{name}`");
                        continue;
                    }
                    let style = styles.entry(name.to_ascii_lowercase()).or_default();
                    for declaration in declarations {
                        apply(style, declaration);
                    }
                }
            }
        }
        Self {
            styles,
            empty: ElementStyle::default(),
        }
    }

    pub fn get(&self, tag: &str) -> &ElementStyle {
        self.styles.get(tag).unwrap_or(&self.empty)
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

fn is_type_selector(selector: &str) -> bool {
    !selector.is_empty()
        && selector
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
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
