use bevy::prelude::*;

pub const MONO_FONT_PATH: &str = "fonts/IosevkaSlabMono-Regular.ttf";
pub const MONO_BOLD_FONT_PATH: &str = "fonts/IosevkaSlabMono-Bold.ttf";
pub const MONO_ITALIC_FONT_PATH: &str = "fonts/IosevkaSlabMono-Italic.ttf";
pub const MONO_BOLD_ITALIC_FONT_PATH: &str = "fonts/IosevkaSlabMono-BoldItalic.ttf";

pub const FRAME_PATH: &str = "ui/frame.slice.ron";

pub const HEADER_FONT_PATH: &str = "fonts/IosevkaSlabQP-Regular.ttf";
pub const HEADER_BOLD_FONT_PATH: &str = "fonts/IosevkaSlabQP-Bold.ttf";
pub const HEADER_ITALIC_FONT_PATH: &str = "fonts/IosevkaSlabQP-Italic.ttf";
pub const HEADER_BOLD_ITALIC_FONT_PATH: &str = "fonts/IosevkaSlabQP-BoldItalic.ttf";

pub const BODY_FONT_PATH: &str = "fonts/Spectral-Regular.ttf";
pub const BODY_BOLD_FONT_PATH: &str = "fonts/Spectral-Bold.ttf";
pub const BODY_ITALIC_FONT_PATH: &str = "fonts/Spectral-Italic.ttf";
pub const BODY_BOLD_ITALIC_FONT_PATH: &str = "fonts/Spectral-BoldItalic.ttf";

/// Font families the demo's CSS themes name, as `[regular, bold, italic,
/// bold-italic]` asset paths; registered in `FontFamilies` at startup.
pub const FONT_FAMILIES: &[(&str, [&str; 4])] = &[
    (
        "Iosevka Slab QP",
        [
            HEADER_FONT_PATH,
            HEADER_BOLD_FONT_PATH,
            HEADER_ITALIC_FONT_PATH,
            HEADER_BOLD_ITALIC_FONT_PATH,
        ],
    ),
    (
        "Spectral",
        [
            BODY_FONT_PATH,
            BODY_BOLD_FONT_PATH,
            BODY_ITALIC_FONT_PATH,
            BODY_BOLD_ITALIC_FONT_PATH,
        ],
    ),
    (
        "Iosevka Slab Mono",
        [
            MONO_FONT_PATH,
            MONO_BOLD_FONT_PATH,
            MONO_ITALIC_FONT_PATH,
            MONO_BOLD_ITALIC_FONT_PATH,
        ],
    ),
];

pub const HEADER_COLOR: Color = Color::srgb_u8(220, 50, 50);

pub const BODY_COLOR: Color = Color::srgb_u8(225, 225, 225);
