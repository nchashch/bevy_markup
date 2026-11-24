//! CSS colors (CSS Color 4) → 8-bit sRGB, the precision lightningcss's
//! `RGBA` had: hex, the named colors, `rgb()`/`rgba()`, `hsl()`/`hsla()`
//! (both syntaxes), `hwb()`, `lab()`, `lch()`, `oklab()`, `oklch()` and
//! `color()` in `srgb`, `srgb-linear`, `display-p3`, `xyz`, `xyz-d50` and
//! `xyz-d65`. Out-of-gamut values clip. `currentColor`, `light-dark()`,
//! `color-mix()`, the system colors and the other `color()` spaces parse
//! but have no fixed value (`CssColor::Unresolved`), which the cascade
//! treats as it treated lightningcss's: no color.

use bevy::color::Color;
use cssparser::{Parser, Token};

use super::value::{Numeric, parse_numeric};
use super::{CssError, Parsed, invalid};

/// A parsed color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum CssColor {
    /// A fixed sRGB color.
    Rgba(Color),
    /// `currentColor`.
    Current,
    /// Valid, but without a fixed value here.
    Unresolved,
}

impl CssColor {
    /// The fixed color, if any.
    pub(crate) fn fixed(self) -> Option<Color> {
        match self {
            CssColor::Rgba(color) => Some(color),
            CssColor::Current | CssColor::Unresolved => None,
        }
    }
}

/// Whether the next token starts a color (for shorthands that try one).
pub(crate) fn parse_color<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, CssColor> {
    let location = input.current_source_location();
    let token = input.next()?.clone();
    let error = || Err(location.new_custom_error(CssError::Invalid));
    match token {
        Token::Hash(hex) | Token::IDHash(hex) => match hex_color(&hex) {
            Some(color) => Ok(CssColor::Rgba(color)),
            None => error(),
        },
        Token::Ident(name) => {
            let name = name.to_ascii_lowercase();
            if name == "currentcolor" {
                return Ok(CssColor::Current);
            }
            if name == "transparent" {
                return Ok(CssColor::Rgba(Color::srgba_u8(0, 0, 0, 0)));
            }
            if let Some(rgb) = named(&name) {
                return Ok(CssColor::Rgba(Color::srgb_u8(rgb[0], rgb[1], rgb[2])));
            }
            if SYSTEM_COLORS.contains(&name.as_str()) {
                return Ok(CssColor::Unresolved);
            }
            error()
        }
        Token::Function(name) => {
            let name = name.to_ascii_lowercase();
            input.parse_nested_block(|args| color_function(&name, args))
        }
        _ => error(),
    }
}

fn color_function<'i>(name: &str, args: &mut Parser<'i, '_>) -> Parsed<'i, CssColor> {
    match name {
        "rgb" | "rgba" => rgb_function(args),
        "hsl" | "hsla" => hsl_function(args),
        "hwb" => {
            let [h, w, b, alpha] = modern(
                args,
                [Kind::Hue, Kind::Percent(100.0), Kind::Percent(100.0)],
            )?;
            let (w, b) = (w / 100.0, b / 100.0);
            Ok(rgba(hwb_to_rgb(h, w, b), alpha))
        }
        "lab" => {
            let [l, a, b, alpha] = modern(
                args,
                [
                    Kind::Percent(100.0),
                    Kind::Percent(125.0),
                    Kind::Percent(125.0),
                ],
            )?;
            Ok(rgba(xyz_d50_to_srgb(lab_to_xyz_d50(l, a, b)), alpha))
        }
        "lch" => {
            let [l, c, h, alpha] = modern(
                args,
                [Kind::Percent(100.0), Kind::Percent(150.0), Kind::Hue],
            )?;
            let (a, b) = polar(c, h);
            Ok(rgba(xyz_d50_to_srgb(lab_to_xyz_d50(l, a, b)), alpha))
        }
        "oklab" => {
            let [l, a, b, alpha] = modern(
                args,
                [Kind::Percent(1.0), Kind::Percent(0.4), Kind::Percent(0.4)],
            )?;
            Ok(rgba(oklab_to_srgb(l, a, b), alpha))
        }
        "oklch" => {
            let [l, c, h, alpha] =
                modern(args, [Kind::Percent(1.0), Kind::Percent(0.4), Kind::Hue])?;
            let (a, b) = polar(c, h);
            Ok(rgba(oklab_to_srgb(l, a, b), alpha))
        }
        "color" => color_space_function(args),
        "light-dark" | "color-mix" | "contrast-color" | "device-cmyk" => {
            while args.next().is_ok() {}
            Ok(CssColor::Unresolved)
        }
        _ => invalid(args),
    }
}

/// How a color function's component reads.
#[derive(Clone, Copy)]
enum Kind {
    /// A number, or a percentage scaled so that 100% is the given value.
    Percent(f32),
    /// A hue: a number (degrees) or an angle.
    Hue,
}

/// One component in the modern (space-separated) syntax; `none` is 0.
fn component<'i>(input: &mut Parser<'i, '_>, kind: Kind) -> Parsed<'i, f32> {
    if input.try_parse(|i| i.expect_ident_matching("none")).is_ok() {
        return Ok(0.0);
    }
    match (parse_numeric(input)?, kind) {
        (Numeric::Number(n), _) => Ok(n),
        (Numeric::Percent(p), Kind::Percent(full)) => Ok(p / 100.0 * full),
        (Numeric::Angle(degrees), Kind::Hue) => Ok(degrees),
        _ => invalid(input),
    }
}

/// An alpha value: a number or a percentage, clamped to 0–1.
fn alpha<'i>(input: &mut Parser<'i, '_>) -> Parsed<'i, f32> {
    if input.try_parse(|i| i.expect_ident_matching("none")).is_ok() {
        return Ok(0.0);
    }
    match parse_numeric(input)? {
        Numeric::Number(n) => Ok(n.clamp(0.0, 1.0)),
        Numeric::Percent(p) => Ok((p / 100.0).clamp(0.0, 1.0)),
        _ => invalid(input),
    }
}

/// Three space-separated components and an optional `/ alpha`.
fn modern<'i>(input: &mut Parser<'i, '_>, kinds: [Kind; 3]) -> Parsed<'i, [f32; 4]> {
    let a = component(input, kinds[0])?;
    let b = component(input, kinds[1])?;
    let c = component(input, kinds[2])?;
    let alpha = if input.try_parse(|i| i.expect_delim('/')).is_ok() {
        alpha(input)?
    } else {
        1.0
    };
    input.expect_exhausted()?;
    Ok([a, b, c, alpha])
}

/// `rgb()`: the legacy comma syntax (all numbers or all percentages) or
/// the modern one. Channels in 0–255.
fn rgb_function<'i>(args: &mut Parser<'i, '_>) -> Parsed<'i, CssColor> {
    let channel = |n: Numeric| match n {
        Numeric::Number(v) => Some((v, false)),
        Numeric::Percent(p) => Some((p / 100.0 * 255.0, true)),
        _ => None,
    };
    let state = args.state();
    // Legacy: `r, g, b[, a]`.
    if let Ok(first) = parse_numeric(args)
        && args.try_parse(|i| i.expect_comma()).is_ok()
    {
        let Some((r, percent)) = channel(first) else {
            return invalid(args);
        };
        let mut rest = [0.0; 2];
        for (index, value) in rest.iter_mut().enumerate() {
            if index > 0 {
                args.expect_comma()?;
            }
            match channel(parse_numeric(args)?) {
                Some((v, p)) if p == percent => *value = v,
                _ => return invalid(args),
            }
        }
        let alpha = if args.try_parse(|i| i.expect_comma()).is_ok() {
            alpha(args)?
        } else {
            1.0
        };
        args.expect_exhausted()?;
        return Ok(rgba_255([r, rest[0], rest[1]], alpha));
    }
    args.reset(&state);
    let [r, g, b, alpha] = modern(
        args,
        [
            Kind::Percent(255.0),
            Kind::Percent(255.0),
            Kind::Percent(255.0),
        ],
    )?;
    Ok(rgba_255([r, g, b], alpha))
}

/// `hsl()`: the legacy comma syntax (percent saturation and lightness) or
/// the modern one.
fn hsl_function<'i>(args: &mut Parser<'i, '_>) -> Parsed<'i, CssColor> {
    let state = args.state();
    if let Ok(hue) = component(args, Kind::Hue)
        && args.try_parse(|i| i.expect_comma()).is_ok()
    {
        let mut sl = [0.0; 2];
        for (index, value) in sl.iter_mut().enumerate() {
            if index > 0 {
                args.expect_comma()?;
            }
            match parse_numeric(args)? {
                Numeric::Percent(p) => *value = p / 100.0,
                _ => return invalid(args),
            }
        }
        let alpha = if args.try_parse(|i| i.expect_comma()).is_ok() {
            alpha(args)?
        } else {
            1.0
        };
        args.expect_exhausted()?;
        return Ok(rgba(hsl_to_rgb(hue, sl[0], sl[1]), alpha));
    }
    args.reset(&state);
    let [h, s, l, alpha] = modern(
        args,
        [Kind::Hue, Kind::Percent(100.0), Kind::Percent(100.0)],
    )?;
    Ok(rgba(hsl_to_rgb(h, s / 100.0, l / 100.0), alpha))
}

/// `color(<space> c1 c2 c3 [/ alpha])`.
fn color_space_function<'i>(args: &mut Parser<'i, '_>) -> Parsed<'i, CssColor> {
    let space = args.expect_ident()?.as_ref().to_ascii_lowercase();
    let [a, b, c, alpha] = modern(
        args,
        [Kind::Percent(1.0), Kind::Percent(1.0), Kind::Percent(1.0)],
    )?;
    let rgb = match space.as_str() {
        "srgb" => [a, b, c],
        "srgb-linear" => [a, b, c].map(gamma),
        "display-p3" => {
            let linear = [a, b, c].map(linear);
            xyz_d65_to_srgb(multiply(&P3_TO_XYZ, linear))
        }
        "xyz" | "xyz-d65" => xyz_d65_to_srgb([a, b, c]),
        "xyz-d50" => xyz_d50_to_srgb([a, b, c]),
        "a98-rgb" | "prophoto-rgb" | "rec2020" => return Ok(CssColor::Unresolved),
        _ => return invalid(args),
    };
    Ok(rgba(rgb, alpha))
}

/// A gamma-encoded sRGB triple (0–1) and alpha, quantized to 8 bits.
fn rgba(rgb: [f32; 3], alpha: f32) -> CssColor {
    rgba_255(rgb.map(|c| c * 255.0), alpha)
}

/// sRGB channels in 0–255 and alpha in 0–1, rounded and clipped to 8 bits.
fn rgba_255(rgb: [f32; 3], alpha: f32) -> CssColor {
    let byte = |v: f32| {
        if v.is_nan() {
            0
        } else {
            v.round().clamp(0.0, 255.0) as u8
        }
    };
    CssColor::Rgba(Color::srgba_u8(
        byte(rgb[0]),
        byte(rgb[1]),
        byte(rgb[2]),
        byte(alpha * 255.0),
    ))
}

fn hex_color(hex: &str) -> Option<Color> {
    let digit = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let bytes = hex.as_bytes();
    let digits: Vec<u8> = bytes.iter().map(|&c| digit(c)).collect::<Option<_>>()?;
    let [r, g, b, a] = match digits.as_slice() {
        [r, g, b] => [r * 17, g * 17, b * 17, 255],
        [r, g, b, a] => [r * 17, g * 17, b * 17, a * 17],
        [r1, r2, g1, g2, b1, b2] => [r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2, 255],
        [r1, r2, g1, g2, b1, b2, a1, a2] => {
            [r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2, a1 * 16 + a2]
        }
        _ => return None,
    };
    Some(Color::srgba_u8(r, g, b, a))
}

/// A hue in degrees, normalized to 0–360.
fn normalize_hue(hue: f32) -> f32 {
    let hue = hue % 360.0;
    if hue < 0.0 { hue + 360.0 } else { hue }
}

/// CSS Color 4's `hslToRgb`: saturation and lightness 0–1.
fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
    let hue = normalize_hue(hue);
    let saturation = saturation.clamp(0.0, 1.0);
    let lightness = lightness.clamp(0.0, 1.0);
    let f = |n: f32| {
        let k = (n + hue / 30.0) % 12.0;
        let a = saturation * lightness.min(1.0 - lightness);
        lightness - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
    };
    [f(0.0), f(8.0), f(4.0)]
}

/// CSS Color 4's `hwbToRgb`: whiteness and blackness 0–1.
fn hwb_to_rgb(hue: f32, white: f32, black: f32) -> [f32; 3] {
    let (white, black) = (white.clamp(0.0, 1.0), black.clamp(0.0, 1.0));
    if white + black >= 1.0 {
        let gray = white / (white + black);
        return [gray; 3];
    }
    hsl_to_rgb(hue, 1.0, 0.5).map(|c| c * (1.0 - white - black) + white)
}

/// Chroma and hue (degrees) as `a`/`b` axes.
fn polar(chroma: f32, hue: f32) -> (f32, f32) {
    let hue = normalize_hue(hue).to_radians();
    let chroma = chroma.max(0.0);
    (chroma * hue.cos(), chroma * hue.sin())
}

/// CIE Lab (D50) → XYZ (D50), CSS Color 4's `Lab_to_XYZ`.
fn lab_to_xyz_d50(lightness: f32, a: f32, b: f32) -> [f32; 3] {
    let kappa = 24389.0 / 27.0;
    let epsilon = 216.0 / 24389.0;
    let f1 = (lightness + 16.0) / 116.0;
    let f0 = a / 500.0 + f1;
    let f2 = f1 - b / 200.0;
    let x = if f0.powi(3) > epsilon {
        f0.powi(3)
    } else {
        (116.0 * f0 - 16.0) / kappa
    };
    let y = if lightness > kappa * epsilon {
        f1.powi(3)
    } else {
        lightness / kappa
    };
    let z = if f2.powi(3) > epsilon {
        f2.powi(3)
    } else {
        (116.0 * f2 - 16.0) / kappa
    };
    let white = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];
    [x * white[0], y * white[1], z * white[2]]
}

/// Bradford D50 → D65.
const D50_TO_D65: [[f32; 3]; 3] = [
    [0.955_473_4, -0.023_098_455, 0.063_259_24],
    [-0.028_369_709, 1.009_995_4, 0.021_041_44],
    [0.012_314_015, -0.020_507_65, 1.330_365_9],
];

/// XYZ (D65) → linear sRGB.
const XYZ_TO_LINEAR_SRGB: [[f32; 3]; 3] = [
    [3.240_97, -1.537_383_2, -0.498_610_76],
    [-0.969_243_65, 1.875_967_5, 0.041_555_06],
    [0.055_630_08, -0.203_976_96, 1.056_971_5],
];

/// Linear Display P3 → XYZ (D65).
const P3_TO_XYZ: [[f32; 3]; 3] = [
    [0.486_570_95, 0.265_667_7, 0.198_217_29],
    [0.228_974_56, 0.691_738_5, 0.079_286_91],
    [0.0, 0.045_113_38, 1.043_944_4],
];

fn multiply(matrix: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    matrix.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
}

fn xyz_d50_to_srgb(xyz: [f32; 3]) -> [f32; 3] {
    xyz_d65_to_srgb(multiply(&D50_TO_D65, xyz))
}

fn xyz_d65_to_srgb(xyz: [f32; 3]) -> [f32; 3] {
    multiply(&XYZ_TO_LINEAR_SRGB, xyz).map(gamma)
}

/// Linear → gamma-encoded sRGB (sign-preserving, as CSS Color 4).
fn gamma(c: f32) -> f32 {
    let magnitude = c.abs();
    let encoded = if magnitude > 0.003_130_8 {
        1.055 * magnitude.powf(1.0 / 2.4) - 0.055
    } else {
        12.92 * magnitude
    };
    encoded.copysign(c)
}

/// Gamma-encoded → linear (sRGB's and Display P3's transfer function).
fn linear(c: f32) -> f32 {
    let magnitude = c.abs();
    let decoded = if magnitude <= 0.04045 {
        magnitude / 12.92
    } else {
        ((magnitude + 0.055) / 1.055).powf(2.4)
    };
    decoded.copysign(c)
}

/// Oklab → gamma-encoded sRGB.
fn oklab_to_srgb(lightness: f32, a: f32, b: f32) -> [f32; 3] {
    let l = (lightness + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m = (lightness - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s = (lightness - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_93 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_4 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
    .map(gamma)
}

/// CSS's system colors: valid, without a fixed value here.
const SYSTEM_COLORS: &[&str] = &[
    "accentcolor",
    "accentcolortext",
    "activetext",
    "buttonborder",
    "buttonface",
    "buttontext",
    "canvas",
    "canvastext",
    "field",
    "fieldtext",
    "graytext",
    "highlight",
    "highlighttext",
    "linktext",
    "mark",
    "marktext",
    "selecteditem",
    "selecteditemtext",
    "visitedtext",
];

/// CSS's named colors (CSS Color 4 §6.1).
fn named(name: &str) -> Option<[u8; 3]> {
    Some(match name {
        "aliceblue" => [240, 248, 255],
        "antiquewhite" => [250, 235, 215],
        "aqua" => [0, 255, 255],
        "aquamarine" => [127, 255, 212],
        "azure" => [240, 255, 255],
        "beige" => [245, 245, 220],
        "bisque" => [255, 228, 196],
        "black" => [0, 0, 0],
        "blanchedalmond" => [255, 235, 205],
        "blue" => [0, 0, 255],
        "blueviolet" => [138, 43, 226],
        "brown" => [165, 42, 42],
        "burlywood" => [222, 184, 135],
        "cadetblue" => [95, 158, 160],
        "chartreuse" => [127, 255, 0],
        "chocolate" => [210, 105, 30],
        "coral" => [255, 127, 80],
        "cornflowerblue" => [100, 149, 237],
        "cornsilk" => [255, 248, 220],
        "crimson" => [220, 20, 60],
        "cyan" => [0, 255, 255],
        "darkblue" => [0, 0, 139],
        "darkcyan" => [0, 139, 139],
        "darkgoldenrod" => [184, 134, 11],
        "darkgray" | "darkgrey" => [169, 169, 169],
        "darkgreen" => [0, 100, 0],
        "darkkhaki" => [189, 183, 107],
        "darkmagenta" => [139, 0, 139],
        "darkolivegreen" => [85, 107, 47],
        "darkorange" => [255, 140, 0],
        "darkorchid" => [153, 50, 204],
        "darkred" => [139, 0, 0],
        "darksalmon" => [233, 150, 122],
        "darkseagreen" => [143, 188, 143],
        "darkslateblue" => [72, 61, 139],
        "darkslategray" | "darkslategrey" => [47, 79, 79],
        "darkturquoise" => [0, 206, 209],
        "darkviolet" => [148, 0, 211],
        "deeppink" => [255, 20, 147],
        "deepskyblue" => [0, 191, 255],
        "dimgray" | "dimgrey" => [105, 105, 105],
        "dodgerblue" => [30, 144, 255],
        "firebrick" => [178, 34, 34],
        "floralwhite" => [255, 250, 240],
        "forestgreen" => [34, 139, 34],
        "fuchsia" => [255, 0, 255],
        "gainsboro" => [220, 220, 220],
        "ghostwhite" => [248, 248, 255],
        "gold" => [255, 215, 0],
        "goldenrod" => [218, 165, 32],
        "gray" | "grey" => [128, 128, 128],
        "green" => [0, 128, 0],
        "greenyellow" => [173, 255, 47],
        "honeydew" => [240, 255, 240],
        "hotpink" => [255, 105, 180],
        "indianred" => [205, 92, 92],
        "indigo" => [75, 0, 130],
        "ivory" => [255, 255, 240],
        "khaki" => [240, 230, 140],
        "lavender" => [230, 230, 250],
        "lavenderblush" => [255, 240, 245],
        "lawngreen" => [124, 252, 0],
        "lemonchiffon" => [255, 250, 205],
        "lightblue" => [173, 216, 230],
        "lightcoral" => [240, 128, 128],
        "lightcyan" => [224, 255, 255],
        "lightgoldenrodyellow" => [250, 250, 210],
        "lightgray" | "lightgrey" => [211, 211, 211],
        "lightgreen" => [144, 238, 144],
        "lightpink" => [255, 182, 193],
        "lightsalmon" => [255, 160, 122],
        "lightseagreen" => [32, 178, 170],
        "lightskyblue" => [135, 206, 250],
        "lightslategray" | "lightslategrey" => [119, 136, 153],
        "lightsteelblue" => [176, 196, 222],
        "lightyellow" => [255, 255, 224],
        "lime" => [0, 255, 0],
        "limegreen" => [50, 205, 50],
        "linen" => [250, 240, 230],
        "magenta" => [255, 0, 255],
        "maroon" => [128, 0, 0],
        "mediumaquamarine" => [102, 205, 170],
        "mediumblue" => [0, 0, 205],
        "mediumorchid" => [186, 85, 211],
        "mediumpurple" => [147, 112, 219],
        "mediumseagreen" => [60, 179, 113],
        "mediumslateblue" => [123, 104, 238],
        "mediumspringgreen" => [0, 250, 154],
        "mediumturquoise" => [72, 209, 204],
        "mediumvioletred" => [199, 21, 133],
        "midnightblue" => [25, 25, 112],
        "mintcream" => [245, 255, 250],
        "mistyrose" => [255, 228, 225],
        "moccasin" => [255, 228, 181],
        "navajowhite" => [255, 222, 173],
        "navy" => [0, 0, 128],
        "oldlace" => [253, 245, 230],
        "olive" => [128, 128, 0],
        "olivedrab" => [107, 142, 35],
        "orange" => [255, 165, 0],
        "orangered" => [255, 69, 0],
        "orchid" => [218, 112, 214],
        "palegoldenrod" => [238, 232, 170],
        "palegreen" => [152, 251, 152],
        "paleturquoise" => [175, 238, 238],
        "palevioletred" => [219, 112, 147],
        "papayawhip" => [255, 239, 213],
        "peachpuff" => [255, 218, 185],
        "peru" => [205, 133, 63],
        "pink" => [255, 192, 203],
        "plum" => [221, 160, 221],
        "powderblue" => [176, 224, 230],
        "purple" => [128, 0, 128],
        "rebeccapurple" => [102, 51, 153],
        "red" => [255, 0, 0],
        "rosybrown" => [188, 143, 143],
        "royalblue" => [65, 105, 225],
        "saddlebrown" => [139, 69, 19],
        "salmon" => [250, 128, 114],
        "sandybrown" => [244, 164, 96],
        "seagreen" => [46, 139, 87],
        "seashell" => [255, 245, 238],
        "sienna" => [160, 82, 45],
        "silver" => [192, 192, 192],
        "skyblue" => [135, 206, 235],
        "slateblue" => [106, 90, 205],
        "slategray" | "slategrey" => [112, 128, 144],
        "snow" => [255, 250, 250],
        "springgreen" => [0, 255, 127],
        "steelblue" => [70, 130, 180],
        "tan" => [210, 180, 140],
        "teal" => [0, 128, 128],
        "thistle" => [216, 191, 216],
        "tomato" => [255, 99, 71],
        "turquoise" => [64, 224, 208],
        "violet" => [238, 130, 238],
        "wheat" => [245, 222, 179],
        "white" => [255, 255, 255],
        "whitesmoke" => [245, 245, 245],
        "yellow" => [255, 255, 0],
        "yellowgreen" => [154, 205, 50],
        _ => return None,
    })
}
