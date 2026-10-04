//! Element styles for [`super::html_ui`], read from a lightningcss stylesheet.
//!
//! Deliberately small subset:
//! - selectors: type selectors only (`h1`, `p`, `code`, …), incl. comma lists;
//!   anything else is skipped (logged at `debug`)
//! - properties: `color` (inherited) and `background-color` (blocks only)
//! - cascade: later rules win; `!important` beats normal declarations
//!
//! A rule for `html` sets the document's starting color, also for fragments
//! that have no `<html>` element.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use lightningcss::properties::Property;
use lightningcss::rules::CssRule;
use lightningcss::stylesheet::{PrinterOptions, StyleSheet};
use lightningcss::traits::ToCss;
use lightningcss::values::color::{CssColor, RGBA};

/// Declared (not computed) style for one element type.
#[derive(Clone, Copy, Default)]
pub(super) struct ElementStyle {
    pub color: Option<Color>,
    pub background: Option<Color>,
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
                            _ => {}
                        }
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
