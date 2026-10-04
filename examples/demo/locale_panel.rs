//! Language selector: loads every demo locale and sets [`ActiveLocale`] to the
//! chosen one; every `HtmlUi` re-localizes.

use bevy::prelude::*;
use p23::prelude::*;

use crate::selector::{Selector, spawn_selector_panel};

/// (directory under `assets/locales/`, native name). The first is active at
/// startup.
const LOCALES: &[(&str, &str)] = &[
    ("en-US", "English"),
    ("ru", "Русский"),
    ("de", "Deutsch"),
    ("ja", "日本語"),
];

/// Every demo locale's bundle, preloaded so switching is immediate.
#[derive(Resource)]
pub struct Locales(Vec<Handle<BundleAsset>>);

#[derive(Component)]
pub struct LocaleSelector;

pub fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
    let bundles: Vec<Handle<BundleAsset>> = LOCALES
        .iter()
        .map(|(id, _)| asset_server.load(format!("locales/{id}/main.ftl.ron")))
        .collect();
    commands.insert_resource(ActiveLocale::new(bundles[0].clone()));
    commands.insert_resource(Locales(bundles));
    spawn_selector_panel(
        &mut commands,
        &asset_server,
        "Language",
        LOCALES.iter().map(|(_, name)| *name),
        0,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(16.0),
            top: Val::Px(232.0),
            ..default()
        },
        LocaleSelector,
    );
}

pub fn apply_locale_selection(
    selectors: Query<&Selector, (With<LocaleSelector>, Changed<Selector>)>,
    locales: Res<Locales>,
    mut active: ResMut<ActiveLocale>,
) {
    for selector in &selectors {
        let bundle = &locales.0[selector.active];
        if active.0.as_ref() != Some(bundle) {
            active.set(bundle.clone());
            info!("locale -> {}", LOCALES[selector.active].0);
        }
    }
}
