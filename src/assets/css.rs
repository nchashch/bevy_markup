//! `.css` files as assets, parsed by lightningcss into its typed stylesheet
//! tree (a CSSOM equivalent: rules → selectors + declarations).
//!
//! Parsing is strict: a syntax error fails the load (with line/column).
//! Consumed by `ui::html_style` (colors for HTML rendered as Bevy UI).

use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;
use lightningcss::stylesheet::{ParserOptions, StyleSheet};
use lightningcss::traits::IntoOwned;

pub struct CssPlugin;

impl Plugin for CssPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<CssStyleSheet>()
            .init_asset_loader::<CssLoader>();
    }
}

/// A parsed stylesheet. Owned (`'static`) via lightningcss's `into_owned`.
#[derive(Asset, TypePath)]
pub struct CssStyleSheet {
    sheet: StyleSheet<'static>,
}

impl CssStyleSheet {
    pub fn sheet(&self) -> &StyleSheet<'static> {
        &self.sheet
    }
}

#[derive(Default, TypePath)]
struct CssLoader;

impl AssetLoader for CssLoader {
    type Asset = CssStyleSheet;
    type Settings = ();
    type Error = BevyError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<CssStyleSheet, BevyError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source = String::from_utf8(bytes)?;

        let options = ParserOptions {
            filename: load_context.path().to_string(),
            ..ParserOptions::default()
        };
        // The parse error borrows `source`; render it before `source` drops.
        let sheet = StyleSheet::parse(&source, options).map_err(|err| err.to_string())?;
        Ok(CssStyleSheet {
            sheet: sheet.into_owned(),
        })
    }

    fn extensions(&self) -> &[&str] {
        &["css"]
    }
}
