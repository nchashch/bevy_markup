//! Language, theme and scrollbar selection. The shell's buttons are plain HTML: each
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
    /// The Scrollbars toggle (on/off).
    pub scrollbars: bool,
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
    commands.insert_resource(Selection {
        lang: 0,
        theme: 0,
        scrollbars: true,
    });
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
        let index = signal.payload.get("index").and_then(|index| index.as_u64());
        match (signal.name.as_ref(), index) {
            ("select-language", Some(index)) => selection.lang = index as usize,
            ("select-theme", Some(index)) => selection.theme = index as usize,
            ("set-scrollbars", _) => {
                if let Some(on) = signal.payload.get("on").and_then(|on| on.as_bool()) {
                    selection.scrollbars = on;
                }
            }
            _ => {}
        }
    }
    if selection.is_changed() {
        for shell in &shells {
            if let Ok(mut context) = contexts.get_mut(shell) {
                // Only the selection: the rest (scrollbar state) stays.
                context.insert("lang_active", &selection.lang);
                context.insert("theme_active", &selection.theme);
                context.insert("scrollbars_on", &selection.scrollbars);
            }
        }
    }
}

/// L / Y: next language; T / X: next theme. The shell re-renders through
/// [`read_signals`]'s change check on the next frame.
pub fn hotkeys(
    hotkeys: crate::input::Hotkeys,
    mut selection: ResMut<Selection>,
    mut contexts: Query<&mut TemplateContext>,
    shells: Query<Entity, With<crate::shell::Shell>>,
) {
    let mut changed = false;
    if hotkeys.just_pressed(KeyCode::KeyL, GamepadButton::North) {
        selection.lang = (selection.lang + 1) % LOCALES.len();
        changed = true;
    }
    if hotkeys.just_pressed(KeyCode::KeyT, GamepadButton::West) {
        selection.theme = (selection.theme + 1) % THEMES.len();
        changed = true;
    }
    if changed {
        for shell in &shells {
            if let Ok(mut context) = contexts.get_mut(shell) {
                // Only the selection: the rest (scrollbar state) stays.
                context.insert("lang_active", &selection.lang);
                context.insert("theme_active", &selection.theme);
                context.insert("scrollbars_on", &selection.scrollbars);
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
