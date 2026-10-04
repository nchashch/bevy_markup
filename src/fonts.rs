//! Font families for CSS `font-family`.
//!
//! Register each family's faces in the [`FontFamilies`] resource; CSS then
//! picks the first registered name in a `font-family` list, and the face by
//! `font-weight`/`font-style`. Changing the resource rebuilds every `HtmlUi`.
//!
//! ```no_run
//! # use bevy::prelude::*;
//! # use p23::prelude::*;
//! fn register(asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
//!     fonts
//!         .insert(
//!             "Iosevka Slab Mono",
//!             FontFaces::new(asset_server.load("fonts/IosevkaSlabMono-Regular.ttf"))
//!                 .with_bold(asset_server.load("fonts/IosevkaSlabMono-Bold.ttf")),
//!         )
//!         .set_generic(GenericFamily::Monospace, "Iosevka Slab Mono");
//! }
//! ```

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

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

/// The faces of one family. Missing faces fall back: bold-italic → bold →
/// italic → regular.
#[derive(Clone, Debug)]
pub struct FontFaces {
    pub regular: Handle<Font>,
    pub bold: Option<Handle<Font>>,
    pub italic: Option<Handle<Font>>,
    pub bold_italic: Option<Handle<Font>>,
}

impl FontFaces {
    pub fn new(regular: Handle<Font>) -> Self {
        Self {
            regular,
            bold: None,
            italic: None,
            bold_italic: None,
        }
    }

    pub fn with_bold(mut self, bold: Handle<Font>) -> Self {
        self.bold = Some(bold);
        self
    }

    pub fn with_italic(mut self, italic: Handle<Font>) -> Self {
        self.italic = Some(italic);
        self
    }

    pub fn with_bold_italic(mut self, bold_italic: Handle<Font>) -> Self {
        self.bold_italic = Some(bold_italic);
        self
    }

    /// The face for this weight/style, with fallbacks.
    pub fn face(&self, bold: bool, italic: bool) -> Handle<Font> {
        let pick = match (bold, italic) {
            (true, true) => self
                .bold_italic
                .as_ref()
                .or(self.bold.as_ref())
                .or(self.italic.as_ref()),
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

    fn index_of(&self, name: &str) -> Option<usize> {
        self.families
            .iter()
            .position(|(family, _)| family.eq_ignore_ascii_case(name))
    }

    /// The first family in a CSS `font-family` list that's registered.
    pub(crate) fn resolve(&self, list: &[FamilyRef]) -> Option<usize> {
        list.iter().find_map(|family| match family {
            FamilyRef::Named(name) => self.index_of(name),
            FamilyRef::Generic(generic) => self.index_of(self.generics.get(generic)?),
        })
    }

    pub(crate) fn faces(&self, index: usize) -> Option<&FontFaces> {
        self.families.get(index).map(|(_, faces)| faces)
    }
}

/// One entry of a CSS `font-family` list.
#[derive(Clone, Debug)]
pub(crate) enum FamilyRef {
    Named(String),
    Generic(GenericFamily),
}
