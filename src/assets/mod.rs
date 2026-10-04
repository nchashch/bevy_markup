use bevy::prelude::*;

pub mod html;
pub mod l10n;

pub struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((html::HtmlPlugin, l10n::L10nPlugin));
    }
}
