//! Language and theme selection: two HTML button rows in the shell document
//! (`.opt` options inside `#lang-row` / `#theme-row`). Clicking an option
//! updates [`Selection`] and re-renders the shell (the `active` class moves,
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

/// Which options are active. Clicks update it; the apply systems follow.
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

/// Which row a clicked `.opt` belongs to.
#[derive(Clone, Copy)]
enum Row {
    Lang,
    Theme,
}

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

/// Wire the shell's `.opt` buttons on every shell build (children are
/// replaced each time, so behaviour is attached per build): each button gets
/// a click observer that updates `Selection` and re-renders the shell.
pub fn wire_buttons(
    built: On<HtmlUiBuilt>,
    shell: Query<(), With<crate::shell::Shell>>,
    elements: HtmlElements,
    rows: Query<&Children>,
    elements_only: Query<(), With<HtmlElement>>,
    mut commands: Commands,
) {
    if shell.get(built.entity).is_err() {
        return;
    }
    for (row_id, kind) in [("lang-row", Row::Lang), ("theme-row", Row::Theme)] {
        let Some(row) = elements.by_id(built.entity, row_id) else {
            continue;
        };
        let Ok(children) = rows.get(row) else {
            continue;
        };
        // The row's element children are the options, in document order —
        // the same order the template rendered them.
        let options = children
            .iter()
            .filter(|child| elements_only.contains(*child))
            .collect::<Vec<_>>();
        for (index, option) in options.into_iter().enumerate() {
            commands.entity(option).observe(
                move |_: On<Pointer<Click>>,
                      mut selection: ResMut<Selection>,
                      mut contexts: Query<&mut TemplateContext>,
                      shells: Query<Entity, With<crate::shell::Shell>>| {
                    match kind {
                        Row::Lang => selection.lang = index,
                        Row::Theme => selection.theme = index,
                    }
                    // Touching the shell's context re-renders it, so the
                    // `active` class moves to the clicked option.
                    for shell in &shells {
                        if let Ok(mut context) = contexts.get_mut(shell) {
                            *context =
                                crate::shell::shell_context(selection.lang, selection.theme);
                        }
                    }
                },
            );
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
