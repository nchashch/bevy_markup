//! Element styles for [`super::html_ui`], read from a lightningcss stylesheet.
//!
//! Deliberately small subset:
//! - selectors: type selectors only (`h1`, `p`, `code`, …), incl. comma lists;
//!   anything else is skipped (logged at `debug`)
//! - inherited properties: `color`, `font-family`, `font-size`, `font-weight`,
//!   `font-style`
//! - non-inherited: `background-color` (blocks only)
//! - cascade: later rules win; `!important` beats normal declarations
//!
//! A rule for `html` sets the document's starting values, also for fragments
//! that have no `<html>` element.
//!
//! `font-family` picks the first listed name found in
//! [`FONT_FAMILIES`] (or mapped by [`FONT_GENERIC_FAMILIES`]); unknown-only
//! lists are ignored. `font-size`: `px`, `em`/`%` (of the inherited size),
//! `rem` (of the root size), absolute keywords (`medium` = 16px),
//! `smaller`/`larger`. `font-weight`: bold at 600+, `bolder`/`lighter`.
//! `font-style`: `italic`/`oblique` vs `normal`.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use lightningcss::properties::Property;
use lightningcss::properties::font::{
    AbsoluteFontSize, AbsoluteFontWeight, FontFamily, FontSize, FontStyle, FontWeight,
    GenericFontFamily, RelativeFontSize,
};
use lightningcss::rules::CssRule;
use lightningcss::stylesheet::{PrinterOptions, StyleSheet};
use lightningcss::traits::ToCss;
use lightningcss::values::color::{CssColor, RGBA};
use lightningcss::values::length::{LengthPercentage, LengthValue};

use crate::consts::{FONT_FAMILIES, FONT_GENERIC_FAMILIES};

/// A declared `font-size`, resolved against inherited/root sizes later.
#[derive(Clone, Copy)]
pub(super) enum FontSizeSpec {
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

/// Declared (not computed) style for one element type.
#[derive(Clone, Copy, Default)]
pub(super) struct ElementStyle {
    pub color: Option<Color>,
    pub background: Option<Color>,
    /// Index into [`FONT_FAMILIES`].
    pub font_family: Option<usize>,
    pub font_size: Option<FontSizeSpec>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
}

/// Element type (lowercase tag name) → declared style.
#[derive(Default)]
pub(super) struct HtmlStyles(HashMap<String, ElementStyle>);

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
        Self(styles)
    }

    pub fn get(&self, tag: &str) -> ElementStyle {
        self.0.get(tag).copied().unwrap_or_default()
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
            match families.iter().find_map(family_index) {
                Some(index) => style.font_family = Some(index),
                None => debug!("html css: no known family in `font-family`"),
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
        _ => {}
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

fn family_index(family: &FontFamily) -> Option<usize> {
    let name = match family {
        FontFamily::FamilyName(name) => name
            .to_css_string(PrinterOptions::default())
            .ok()?
            .trim_matches(['"', '\''])
            .to_owned(),
        FontFamily::Generic(generic) => {
            let keyword = match generic {
                GenericFontFamily::Serif => "serif",
                GenericFontFamily::Monospace => "monospace",
                _ => return None,
            };
            FONT_GENERIC_FAMILIES
                .iter()
                .find(|(generic, _)| *generic == keyword)?
                .1
                .to_owned()
        }
    };
    FONT_FAMILIES
        .iter()
        .position(|(family, _)| family.eq_ignore_ascii_case(&name))
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
