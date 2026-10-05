//! The demo's three typefaces, all from the fonts installed on the system
//! (Bevy's `system_font_discovery`): the CSS themes use the generic keywords
//! `sans-serif` (headings), `serif` (body) and `monospace` (code), and
//! `main.rs` maps them to these families.

use bevy::prelude::*;

pub const HEADER_FONT: FontSource = FontSource::SansSerif;
pub const BODY_FONT: FontSource = FontSource::Serif;
pub const MONO_FONT: FontSource = FontSource::Monospace;
