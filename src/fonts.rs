//! Font families for CSS `font-family`.
//!
//! Register each family's faces in the [`FontFamilies`] resource; a CSS
//! `font-family` list resolves to every registered family in order (not
//! just the first hit), and each family's face is picked by
//! `font-weight`/`font-style`. The chain composes into Bevy 0.20's
//! fallback list: fontique picks the first face that covers a glyph, so
//! mixed scripts (Latin + CJK) render from different fonts without
//! bundling. Changing the resource rebuilds every `HtmlUi`.
//!
//! A face is any [`FontSource`]: a loaded font file, or a family resolved
//! from the fonts installed on the system (a name like `"DejaVu Serif"` or a
//! generic like [`FontSource::serif`]; needs Bevy's `system_font_discovery`,
//! which the `system_fonts` feature enables). For system families, bold and
//! italic are requested from the system, which picks the family's real faces.
//!
//! ```no_run
//! # use bevy::prelude::*;
//! # use bevy_markup::prelude::*;
//! fn register(asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
//!     fonts
//!         // Font files: one per face.
//!         .insert(
//!             "Iosevka Slab Mono",
//!             FontFaces::new(asset_server.load("fonts/IosevkaSlabMono-Regular.ttf"))
//!                 .with_bold(asset_server.load("fonts/IosevkaSlabMono-Bold.ttf")),
//!         )
//!         // The system's serif family, all faces.
//!         .insert("Body", FontFaces::new(FontSource::serif()))
//!         .set_generic(GenericFamily::Monospace, "Iosevka Slab Mono");
//! }
//! ```

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

/// Bounds on the text bevy_markup asks Bevy to rasterize, so a page's CSS
/// can't exhaust memory through font sizes.
///
/// Bevy rasterizes glyphs into font atlases kept per exact font size and
/// never frees them (UPSTREAM U55): each size used costs at least a
/// 512×512 RGBA atlas (1 MiB, held in main memory and on the GPU), and a
/// glyph gets an atlas as large as itself — up to twice the font size for
/// tall glyphs (`j`, `$`). Measured on Bevy 0.20: a thousand elements at
/// slightly different sizes took 1 GB; sixteen letters at 8192 px took
/// 2.3 GB.
///
/// Set it before spawning `HtmlUi`s: a change applies to styles computed
/// afterwards. Raise the limits for apps that need more sizes (large
/// displays, typography-heavy documents); lower them on memory-constrained
/// platforms (WebGL 2's textures stop at 2048 px).
///
/// ```no_run
/// # use bevy::prelude::*;
/// # use bevy_markup::prelude::*;
/// App::new()
///     .add_plugins((DefaultPlugins, BevyMarkupPlugin))
///     .insert_resource(HtmlFontLimits {
///         max_font_size: 1024.0,
///         max_font_sizes: 32,
///     });
/// ```
#[derive(Resource, Clone, Debug, PartialEq, Reflect)]
pub struct HtmlFontLimits {
    /// The largest computed `font-size`, in logical px; larger sizes are
    /// clamped to it. Default 2048: the tallest glyphs' atlases then stay
    /// within 8192 px — the default texture size limit on WebGPU and many
    /// GPUs — at a scale factor up to 2. Values above 8192 count as 8192:
    /// larger glyphs overflow the glyph rasterizer (UPSTREAM U54).
    pub max_font_size: f32,
    /// How many distinct computed font sizes the app may use, all `HtmlUi`s
    /// together (default 128). Past it, a new size is replaced by the
    /// nearest size already in use (by ratio): Bevy keeps those sizes'
    /// atlases anyway, so reusing them costs no memory. The sizes counted
    /// stay counted for the app's lifetime, as Bevy's atlases do. At least
    /// one size is always allowed.
    pub max_font_sizes: usize,
}

impl Default for HtmlFontLimits {
    fn default() -> Self {
        Self {
            max_font_size: 2048.0,
            max_font_sizes: 128,
        }
    }
}

/// The font sizes in use, against [`HtmlFontLimits`] (copied in each build).
#[derive(Resource, Default, Debug)]
pub(crate) struct FontSizeBudget {
    pub(crate) limits: HtmlFontLimits,
    /// Sorted, distinct.
    sizes: Vec<f32>,
}

impl FontSizeBudget {
    /// `size` as it may be used: within `[0, max_font_size]`, and one of the
    /// sizes in use once `max_font_sizes` are (the nearest by ratio).
    pub(crate) fn admit(&mut self, size: f32) -> f32 {
        let ceiling = crate::cascade::MAX_FONT_SIZE;
        let max = match self.limits.max_font_size {
            max if max.is_nan() => ceiling,
            max => max.clamp(0.0, ceiling),
        };
        let size = if size.is_nan() {
            0.0
        } else {
            size.clamp(0.0, max)
        };
        if size == 0.0 {
            // Draws nothing: no atlas.
            return size;
        }
        match self.sizes.binary_search_by(|used| used.total_cmp(&size)) {
            Ok(_) => size,
            Err(at) if self.sizes.len() < self.limits.max_font_sizes.max(1) => {
                self.sizes.insert(at, size);
                size
            }
            Err(at) => {
                let below = at.checked_sub(1).map(|i| self.sizes[i]);
                let above = self.sizes.get(at).copied();
                match (below, above) {
                    (Some(below), Some(above)) if above / size < size / below => above,
                    (Some(below), _) => below,
                    (None, Some(above)) => above,
                    (None, None) => unreachable!("at least one size is admitted"),
                }
            }
        }
    }

    /// How many distinct sizes are in use.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.sizes.len()
    }
}

/// CSS generic family keywords that can be mapped to a registered family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Reflect)]
pub enum GenericFamily {
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
    SystemUi,
}

/// The faces of one family. Missing faces fall back as CSS font matching
/// does (style narrows before weight): bold-italic → italic → bold → regular.
/// A system family (any [`FontSource`] other than a handle) usually needs
/// only `regular`: bold and italic are then requested from the system.
#[derive(Clone, Debug)]
pub struct FontFaces {
    pub regular: FontSource,
    pub bold: Option<FontSource>,
    pub italic: Option<FontSource>,
    pub bold_italic: Option<FontSource>,
}

impl FontFaces {
    pub fn new(regular: impl Into<FontSource>) -> Self {
        Self {
            regular: regular.into(),
            bold: None,
            italic: None,
            bold_italic: None,
        }
    }

    pub fn with_bold(mut self, bold: impl Into<FontSource>) -> Self {
        self.bold = Some(bold.into());
        self
    }

    pub fn with_italic(mut self, italic: impl Into<FontSource>) -> Self {
        self.italic = Some(italic.into());
        self
    }

    pub fn with_bold_italic(mut self, bold_italic: impl Into<FontSource>) -> Self {
        self.bold_italic = Some(bold_italic.into());
        self
    }

    /// The face for this weight/style, with fallbacks.
    pub fn face(&self, bold: bool, italic: bool) -> FontSource {
        let pick = match (bold, italic) {
            // CSS font matching: an italic face (bold synthesized) beats an
            // upright bold one.
            (true, true) => self
                .bold_italic
                .as_ref()
                .or(self.italic.as_ref())
                .or(self.bold.as_ref()),
            (true, false) => self.bold.as_ref(),
            (false, true) => self.italic.as_ref(),
            (false, false) => None,
        };
        pick.unwrap_or(&self.regular).clone()
    }
}

/// Registered font families, by CSS name (matched case-insensitively).
#[derive(Resource, Default, Debug)]
pub struct FontFamilies {
    families: Vec<(String, FontFaces)>,
    generics: HashMap<GenericFamily, String>,
}

impl FontFamilies {
    /// Registers (or replaces) the family `name`.
    pub fn insert(&mut self, name: impl Into<String>, faces: FontFaces) -> &mut Self {
        let name = name.into();
        match self.index_of(&name) {
            Some(index) => self.families[index].1 = faces,
            None => self.families.push((name, faces)),
        }
        self
    }

    /// Makes the CSS generic keyword (`serif`, `monospace`, …) mean the
    /// registered family `name`.
    pub fn set_generic(&mut self, generic: GenericFamily, name: impl Into<String>) -> &mut Self {
        self.generics.insert(generic, name.into());
        self
    }

    pub fn get(&self, name: &str) -> Option<&FontFaces> {
        self.index_of(name).map(|index| &self.families[index].1)
    }

    /// Every registered family, in registration order.
    #[cfg(feature = "svg-text")]
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, &FontFaces)> {
        self.families
            .iter()
            .map(|(name, faces)| (name.as_str(), faces))
    }

    /// The family a generic keyword maps to, if any.
    #[cfg(feature = "svg-text")]
    pub(crate) fn generic(&self, generic: GenericFamily) -> Option<&str> {
        self.generics.get(&generic).map(String::as_str)
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        self.families
            .iter()
            .position(|(family, _)| family.eq_ignore_ascii_case(name))
    }

    pub(crate) fn faces(&self, index: usize) -> Option<&FontFaces> {
        self.families.get(index).map(|(_, faces)| faces)
    }

    /// The CSS `font-family` list resolved to registered family indexes, in
    /// CSS order (generics through their mapped names, unregistered names
    /// skipped): the text font composes the chain into Bevy 0.20's
    /// per-glyph fallback list (ADR 0015).
    pub(crate) fn resolve_chain(&self, list: &[FamilyRef]) -> Vec<usize> {
        list.iter()
            .filter_map(|family| match family {
                FamilyRef::Named(name) => self.index_of(name),
                FamilyRef::Generic(generic) => self.index_of(self.generics.get(generic)?),
            })
            .collect()
    }
}

/// One entry of a CSS `font-family` list.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FamilyRef {
    Named(String),
    Generic(GenericFamily),
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::uuid::Uuid;

    fn font(id: u128) -> Handle<Font> {
        Handle::Uuid(Uuid::from_u128(id), std::marker::PhantomData)
    }

    fn named(name: &str) -> FamilyRef {
        FamilyRef::Named(name.to_owned())
    }

    /// Every combination of present faces × requested weight/style follows
    /// the documented fallback (bold-italic → italic → bold → regular; bold
    /// or italic alone → regular), and a present exact face always wins.
    /// Catches a wrong arm (e.g. italic requests getting the bold face).
    #[test]
    fn face_fallback_chain() {
        let (regular, bold, italic, bold_italic) = (font(1), font(2), font(3), font(4));
        for mask in 0..8u8 {
            let mut faces = FontFaces::new(regular.clone());
            let has = |bit: u8| mask & bit != 0;
            if has(1) {
                faces = faces.with_bold(bold.clone());
            }
            if has(2) {
                faces = faces.with_italic(italic.clone());
            }
            if has(4) {
                faces = faces.with_bold_italic(bold_italic.clone());
            }
            let pick = |chain: &[(u8, &Handle<Font>)]| {
                chain
                    .iter()
                    .find(|(bit, _)| has(*bit))
                    .map_or(regular.clone(), |(_, face)| (*face).clone())
            };
            let expected = [
                ((false, false), regular.clone()),
                ((true, false), pick(&[(1, &bold)])),
                ((false, true), pick(&[(2, &italic)])),
                (
                    (true, true),
                    pick(&[(4, &bold_italic), (2, &italic), (1, &bold)]),
                ),
            ];
            for ((b, i), face) in expected {
                assert_eq!(
                    faces.face(b, i),
                    face.into(),
                    "faces mask {mask:03b}, bold {b}, italic {i}"
                );
            }
        }
    }

    fn families() -> FontFamilies {
        let mut families = FontFamilies::default();
        families
            .insert("Spectral", FontFaces::new(font(1)))
            .insert("Iosevka Slab", FontFaces::new(font(2)))
            .set_generic(GenericFamily::Monospace, "iosevka slab")
            // Mapped to a name that was never registered.
            .set_generic(GenericFamily::Serif, "Missing");
        families
    }

    /// The *list* order decides, not registration order; names match ASCII
    /// case-insensitively (CSS family names are case-insensitive).
    #[test]
    fn registered_names_resolve_in_list_order() {
        let families = families();
        let spectral = families.resolve_chain(&[named("spectral")]);
        let iosevka = families.resolve_chain(&[named("IOSEVKA SLAB")]);
        assert!(!spectral.is_empty() && !iosevka.is_empty() && spectral != iosevka);
        // Case-insensitive; the first hit no longer shadows later names —
        // the whole chain resolves (ADR 0015's per-glyph fallback).
        assert_eq!(
            families.resolve_chain(&[named("Iosevka Slab"), named("Spectral")]),
            [iosevka[0], spectral[0]]
        );
        assert_eq!(
            families.resolve_chain(&[named("Nope"), named("Spectral")]),
            spectral
        );
        assert_eq!(families.faces(spectral[0]).unwrap().regular, font(1).into());
    }

    /// Unregistered names, unmapped generics and generics mapped to
    /// unregistered names are skipped, and a list with nothing registered
    /// resolves to `None` (Bevy's default font), not the first entry.
    #[test]
    fn unresolvable_entries_are_skipped() {
        let families = families();
        let iosevka = families.resolve_chain(&[named("Iosevka Slab")]);
        let serif = FamilyRef::Generic(GenericFamily::Serif);
        let cursive = FamilyRef::Generic(GenericFamily::Cursive);
        let mono = FamilyRef::Generic(GenericFamily::Monospace);
        assert_eq!(
            families.resolve_chain(&[serif.clone(), cursive.clone(), mono]),
            iosevka
        );
        assert!(
            families
                .resolve_chain(&[named("Nope"), serif, cursive])
                .is_empty()
        );
        assert!(families.resolve_chain(&[]).is_empty());
    }

    /// The chain keeps every registered family in CSS order — the fallback
    /// list fontique resolves per glyph (ADR 0015) — with duplicates
    /// deduplicated by the registry's in-place replace.
    #[test]
    fn chains_keep_all_registered_families() {
        let families = families();
        let chain = families.resolve_chain(&[
            named("Spectral"),
            named("Nope"),
            FamilyRef::Generic(GenericFamily::Monospace),
        ]);
        assert_eq!(chain.len(), 2, "{chain:?}");
    }

    /// Re-registering a name (in any case) replaces its faces in place:
    /// no duplicate entry shadows it, and other families keep resolving.
    #[test]
    fn insert_replaces_case_insensitively() {
        let mut families = families();
        let before = families.resolve_chain(&[named("Spectral")]);
        families.insert("SPECTRAL", FontFaces::new(font(9)));
        assert_eq!(families.resolve_chain(&[named("Spectral")]), before);
        assert_eq!(families.get("spectral").unwrap().regular, font(9).into());
        assert_eq!(
            families.get("Iosevka Slab").unwrap().regular,
            font(2).into()
        );
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    fn budget(max_font_size: f32, max_font_sizes: usize) -> FontSizeBudget {
        FontSizeBudget {
            limits: HtmlFontLimits {
                max_font_size,
                max_font_sizes,
            },
            ..default()
        }
    }

    /// bug_0250: sizes clamp to the configured maximum, which never goes
    /// past the rasterizer's ceiling (UPSTREAM U54).
    #[test]
    fn sizes_clamp_to_the_maximum() {
        let mut b = budget(100.0, 16);
        assert_eq!(b.admit(16.0), 16.0);
        assert_eq!(b.admit(1e8), 100.0);
        assert_eq!(b.admit(-3.0), 0.0);
        assert_eq!(b.admit(f32::NAN), 0.0);
        let mut b = budget(1e9, 16);
        assert_eq!(b.admit(1e8), crate::cascade::MAX_FONT_SIZE);
        let mut b = budget(f32::NAN, 16);
        assert_eq!(b.admit(1e8), crate::cascade::MAX_FONT_SIZE);
        assert_eq!(HtmlFontLimits::default().max_font_size, 2048.0);
    }

    /// bug_0250: past the budget, a new size reuses the nearest size in
    /// use (by ratio); sizes in use and zero don't count again.
    #[test]
    fn sizes_past_the_budget_reuse_the_nearest() {
        let mut b = budget(2048.0, 3);
        for size in [10.0, 20.0, 40.0] {
            assert_eq!(b.admit(size), size);
        }
        assert_eq!(b.admit(20.0), 20.0, "a size in use is free");
        assert_eq!(b.admit(0.0), 0.0, "zero draws nothing and isn't counted");
        assert_eq!(b.len(), 3);
        assert_eq!(b.admit(5.0), 10.0);
        assert_eq!(b.admit(13.0), 10.0, "13/10 < 20/13");
        assert_eq!(b.admit(15.0), 20.0, "20/15 < 15/10");
        assert_eq!(b.admit(1000.0), 40.0);
        assert_eq!(b.len(), 3);
        // A budget of zero still admits one size.
        let mut b = budget(2048.0, 0);
        assert_eq!(b.admit(12.0), 12.0);
        assert_eq!(b.admit(30.0), 12.0);
    }

    proptest::proptest! {
        /// Whatever sizes a page asks for, at most `max_font_sizes`
        /// distinct sizes come out, each within the maximum.
        #[test]
        fn distinct_sizes_stay_within_the_budget(
            sizes in proptest::collection::vec(-10.0f32..1e6, 0..300),
            max_font_sizes in 1usize..40,
        ) {
            let mut b = budget(2048.0, max_font_sizes);
            let mut out: Vec<f32> = sizes.into_iter().map(|size| b.admit(size)).collect();
            out.retain(|size| *size != 0.0);
            out.sort_by(f32::total_cmp);
            out.dedup();
            proptest::prop_assert!(out.len() <= max_font_sizes);
            proptest::prop_assert!(out.iter().all(|size| (0.0..=2048.0).contains(size)));
        }
    }
}
