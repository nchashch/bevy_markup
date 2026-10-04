use bevy::prelude::*;

pub mod css;
pub mod html;
pub mod l10n;

pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((css::CssPlugin, html::HtmlPlugin, l10n::L10nPlugin));
    }
}
