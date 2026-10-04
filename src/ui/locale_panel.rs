//! Language selector: one toggle per [`Locales`] entry. Choosing one makes its
//! bundle the [`ActiveLocale`]; every `HtmlView` re-localizes.

use bevy::prelude::*;

use super::selector::{Selector, spawn_selector_panel};
use crate::assets::l10n::{ActiveLocale, Locales};

#[derive(Component)]
pub(super) struct LocaleSelector;

/// PostStartup: needs [`Locales`], inserted by `L10nPlugin` during Startup.
pub(super) fn spawn(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    locales: Res<Locales>,
    active: Res<ActiveLocale>,
) {
    let active = locales
        .0
        .iter()
        .position(|locale| locale.bundle == active.0)
        .unwrap_or(0);
    spawn_selector_panel(
        &mut commands,
        &asset_server,
        "Language",
        locales.0.iter().map(|locale| locale.name),
        active,
        Node {
            position_type: PositionType::Absolute,
            right: Val::Px(16.0),
            top: Val::Px(232.0),
            ..default()
        },
        LocaleSelector,
    );
}

pub(super) fn apply_locale_selection(
    selectors: Query<&Selector, (With<LocaleSelector>, Changed<Selector>)>,
    locales: Res<Locales>,
    mut active: ResMut<ActiveLocale>,
) {
    for selector in &selectors {
        let locale = &locales.0[selector.active];
        if active.0 != locale.bundle {
            active.0 = locale.bundle.clone();
            info!("locale -> {}", locale.id);
        }
    }
}
