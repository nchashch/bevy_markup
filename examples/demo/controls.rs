//! Language and theme selection. The shell's buttons are plain HTML: each
//! one declares `data-on-click="select-language"` / `"select-theme"` with
//! its option index as `data-with`, and the library turns clicks into
//! [`ElementSignal`]s. `read_signals` drains the message queue, updates
//! [`Selection`], and re-renders the shell (the `active` class moves,
//! because the rows are Tera-rendered from the shell's context); the apply
//! systems translate the selection into the `ActiveLocale` and
//! `DefaultStylesheet` resources, which re-localize and restyle every
//! `HtmlUi`.

use bevy::prelude::*;
use bevy_markup::prelude::*;

/// (directory under `examples/assets/locales/`, native name). The first is
/// active at startup.
pub const LOCALES: &[(&str, &str)] = &[
    ("en-US", "English"),
    ("ru", "Русский"),
    ("de", "Deutsch"),
    ("ja", "日本語"),
];

/// (label, stylesheet path). The first is the default theme.
pub const THEMES: &[(&str, &str)] = &[
    ("Crimson", "ui/themes/crimson.css"),
    ("Parchment", "ui/themes/parchment.css"),
    ("Terminal", "ui/themes/terminal.css"),
    ("Large print", "ui/themes/large_print.css"),
];

/// Which options are active. Signals update it; the apply systems follow.
#[derive(Resource)]
pub struct Selection {
    pub lang: usize,
    pub theme: usize,
}

/// Every demo locale's bundle, preloaded so switching is immediate.
#[derive(Resource)]
pub struct Locales(Vec<Handle<BundleAsset>>);

/// Every theme's stylesheet, preloaded so switching is immediate.
#[derive(Resource)]
pub struct Themes(Vec<Handle<Stylesheet>>);

pub fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
    let bundles: Vec<Handle<BundleAsset>> = LOCALES
        .iter()
        .map(|(id, _)| asset_server.load(format!("locales/{id}/main.ftl.ron")))
        .collect();
    let sheets: Vec<Handle<Stylesheet>> = THEMES
        .iter()
        .map(|(_, path)| asset_server.load(*path))
        .collect();
    commands.insert_resource(ActiveLocale::new(bundles[0].clone()));
    commands.insert_resource(Locales(bundles));
    commands.insert_resource(DefaultStylesheet::new(sheets[0].clone()));
    commands.insert_resource(Themes(sheets));
    commands.insert_resource(Selection { lang: 0, theme: 0 });
}

/// Drains the UI's signals and re-renders the shell when the selection
/// changed, so the `active` class moves to the clicked option.
pub fn read_signals(
    mut signals: MessageReader<ElementSignal>,
    mut selection: ResMut<Selection>,
    mut contexts: Query<&mut TemplateContext>,
    shells: Query<Entity, With<crate::shell::Shell>>,
) {
    for signal in signals.read() {
        let Some(index) = signal.payload.get("index").and_then(|index| index.as_u64()) else {
            continue;
        };
        let index = index as usize;
        match signal.name.as_ref() {
            "select-language" => selection.lang = index,
            "select-theme" => selection.theme = index,
            _ => {}
        }
    }
    if selection.is_changed() {
        for shell in &shells {
            if let Ok(mut context) = contexts.get_mut(shell) {
                *context = crate::shell::shell_context(selection.lang, selection.theme);
            }
        }
    }
}

pub fn apply_locale_selection(
    selection: Res<Selection>,
    locales: Res<Locales>,
    mut active: ResMut<ActiveLocale>,
) {
    if !selection.is_changed() {
        return;
    }
    let bundle = &locales.0[selection.lang];
    if active.0.as_ref() != Some(bundle) {
        active.set(bundle.clone());
        info!("locale -> {}", LOCALES[selection.lang].0);
    }
}

pub fn apply_theme_selection(
    selection: Res<Selection>,
    themes: Res<Themes>,
    mut default_sheet: ResMut<DefaultStylesheet>,
) {
    if !selection.is_changed() {
        return;
    }
    let sheet = &themes.0[selection.theme];
    if default_sheet.0.as_ref() != Some(sheet) {
        default_sheet.0 = Some(sheet.clone());
        info!("theme -> {}", THEMES[selection.theme].0);
    }
}
