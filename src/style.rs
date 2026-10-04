//! `.css` stylesheets and how they're assigned.
//!
//! Every [`HtmlUi`](crate::html::HtmlUi) uses its own [`HtmlStylesheet`] if it
//! has one, else the [`DefaultStylesheet`] resource, else no styles (white
//! text, Bevy's default font, 16px). Swapping either handle restyles at
//! runtime (themes); the UI isn't built while its stylesheet is still loading,
//! so it never flashes unstyled.
//!
//! Supported CSS (parsed with lightningcss; a syntax error fails the load):
//! - selectors: type selectors (`h1`, `p`, `code`, …), incl. comma lists;
//!   others are skipped (logged at `debug`)
//! - inherited: `color`, `font-family`, `font-size`, `font-weight`,
//!   `font-style`
//! - blocks only: `background-color`
//! - cascade: later rules win; `!important` beats normal declarations
//! - an `html` rule sets the starting values, also for fragments without `<html>`
//!
//! `font-family` uses the first name registered in
//! [`FontFamilies`](crate::fonts::FontFamilies) (generic keywords via
//! [`FontFamilies::set_generic`](crate::fonts::FontFamilies::set_generic)); a
//! list with no registered name keeps the inherited family. `font-size`:
//! `px`, `em`/`%` (of the inherited size), `rem` (of the root size), keywords
//! (`medium` = 16px), `smaller`/`larger`. `font-weight`: bold at 600+.
//! `font-style`: `italic`/`oblique` vs `normal`.

use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;
use lightningcss::stylesheet::{ParserOptions, StyleSheet};
use lightningcss::traits::IntoOwned;

/// A parsed `.css` file.
#[derive(Asset, TypePath)]
pub struct Stylesheet {
    sheet: StyleSheet<'static>,
}

impl Stylesheet {
    /// The lightningcss stylesheet tree.
    pub fn sheet(&self) -> &StyleSheet<'static> {
        &self.sheet
    }
}

/// Stylesheet for every `HtmlUi` without an [`HtmlStylesheet`]. Swap the
/// handle to switch themes.
#[derive(Resource, Default, Clone, Debug, Reflect)]
#[reflect(Resource, Default)]
pub struct DefaultStylesheet(pub Option<Handle<Stylesheet>>);

impl DefaultStylesheet {
    pub fn new(stylesheet: Handle<Stylesheet>) -> Self {
        Self(Some(stylesheet))
    }
}

/// Per-entity stylesheet, overriding [`DefaultStylesheet`] for this `HtmlUi`.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component)]
pub struct HtmlStylesheet(pub Handle<Stylesheet>);

#[derive(Default, TypePath)]
pub(crate) struct StylesheetLoader;

impl AssetLoader for StylesheetLoader {
    type Asset = Stylesheet;
    type Settings = ();
    type Error = BevyError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<Stylesheet, BevyError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source = String::from_utf8(bytes)?;

        let options = ParserOptions {
            filename: load_context.path().to_string(),
            ..ParserOptions::default()
        };
        // The parse error borrows `source`; render it before `source` drops.
        let sheet = StyleSheet::parse(&source, options).map_err(|err| err.to_string())?;
        Ok(Stylesheet {
            sheet: sheet.into_owned(),
        })
    }

    fn extensions(&self) -> &[&str] {
        &["css"]
    }
}
