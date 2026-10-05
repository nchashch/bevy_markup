use bevy::prelude::*;

pub const FRAME_PATH: &str = "ui/frame.slice.ron";

/// The demo's three typefaces, all from the fonts installed on the system
/// (Bevy's `system_font_discovery`): the CSS themes use the generic keywords
/// `sans-serif` (headings), `serif` (body) and `monospace` (code), and the
/// plain Bevy UI panels use the same families directly.
pub const HEADER_FONT: FontSource = FontSource::SansSerif;
pub const BODY_FONT: FontSource = FontSource::Serif;
pub const MONO_FONT: FontSource = FontSource::Monospace;

/// A `TextFont` for plain Bevy UI text in `font` at `size` px.
pub fn text_font(font: FontSource, size: f32) -> TextFont {
    TextFont {
        font,
        ..TextFont::from_font_size(size)
    }
}

pub const HEADER_COLOR: Color = Color::srgb_u8(220, 50, 50);

pub const BODY_COLOR: Color = Color::srgb_u8(225, 225, 225);
