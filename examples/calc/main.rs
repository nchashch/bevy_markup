//! CSS `calc()` in layout properties: a sidebar and a fluid content
//! column whose width is `calc(100% - {{ sidebar }}px - 12px)`, a card
//! row that divides the column with `calc(100% / {{ cards }} - 8px)`,
//! and a headline indented by `calc({{ indent }}px + 1em)`. Mixed-unit
//! expressions resolve per frame — `%` against the containing block's
//! content box, `em` against the element's own font size — so stepping a
//! setting reflows the layout live (same-unit expressions collapse at
//! style time; here every value is templated, so each change restyles).
//!
//! `cargo run --example calc` — click the − / + buttons, focus a value
//! and press ← → / D-pad, or use the gamepad. L / Y switches English/German.

use bevy::prelude::*;
use bevy_markup::prelude::*;

#[path = "../shared/input.rs"]
mod input;
use input::{Hotkeys, SettingStep};

#[path = "../shared/harness.rs"]
mod harness;

#[hotpath::main]
fn main() {
    harness::run(
        harness::ExampleWindow {
            title: "bevy_markup: CSS calc() in layout",
            ..Default::default()
        },
        |app| {
            app.init_resource::<Settings>()
                .add_systems(Startup, setup)
                .add_systems(
                    Update,
                    (
                        handle_signals,
                        apply_setting_steps,
                        switch_language,
                        show_settings,
                    ),
                );
        },
    );
}

/// The values the CSS expressions are built from. Stepping one updates
/// the template's context; the calc() expressions re-resolve in place.
#[derive(Resource, serde::Serialize, Clone, Copy)]
struct Settings {
    /// The sidebar's width in px; the content column gives it back plus
    /// the layout gap.
    sidebar: i32,
    /// How many cards share the content column's width.
    cards: i32,
    /// Extra headline indent in px (on top of `1em` of its own font).
    indent: i32,
}

impl Default for Settings {
    // The midpoints of the steppers' ranges: a sensible-looking start.
    fn default() -> Self {
        Self {
            sidebar: 160,
            cards: 3,
            indent: 8,
        }
    }
}

impl Settings {
    /// The steppers' ranges and per-step sizes.
    const RANGES: [(&'static str, i32, i32, i32); 3] = [
        // name, min, max, step
        ("sidebar", 80, 280, 20),
        ("cards", 2, 5, 1),
        ("indent", 0, 48, 8),
    ];

    fn step(&mut self, name: &str, direction: i32) {
        let Some((_, min, max, size)) = Self::RANGES.iter().find(|r| r.0 == name) else {
            return;
        };
        let value = match name {
            "sidebar" => &mut self.sidebar,
            "cards" => &mut self.cards,
            "indent" => &mut self.indent,
            _ => return,
        };
        *value = (*value + direction * size).clamp(*min, *max);
    }
}

/// Both locales, preloaded so switching is immediate.
#[derive(Resource)]
struct Languages(Vec<Handle<LocaleBundle>>);

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
    commands.spawn(Camera2d);

    // CSS generic keywords → the system's fonts (Bevy's system_font_discovery).
    fonts
        .insert("System Serif", FontFaces::new(FontSource::serif()))
        .insert("System Mono", FontFaces::new(FontSource::monospace()))
        .set_generic(GenericFamily::Serif, "System Serif")
        .set_generic(GenericFamily::Monospace, "System Mono");

    commands.insert_resource(DefaultStylesheet::new(asset_server.load("calc/style.css")));

    let languages: Vec<Handle<LocaleBundle>> = ["en-US", "de"]
        .iter()
        .map(|id| asset_server.load(format!("calc/locales/{id}/main.ftl.ron")))
        .collect();
    commands.insert_resource(ActiveLocale::new(languages[0].clone()));
    commands.insert_resource(Languages(languages));

    commands.spawn((
        HtmlUi::new(asset_server.load("calc/page.html")),
        TemplateContext::new().with("settings", &Settings::default()),
        Node {
            width: Val::Px(720.0),
            margin: UiRect::all(Val::Auto),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(14.0),
            ..default()
        },
    ));
}

/// − / + clicks step the setting whose name the button carries.
fn handle_signals(mut signals: MessageReader<ElementSignal>, mut settings: ResMut<Settings>) {
    for signal in signals.read() {
        if signal.trigger != SignalTrigger::Click {
            continue;
        }
        let name = signal.name.as_ref();
        let (setting, direction) = match name.strip_suffix("-down") {
            Some(setting) => (setting, -1),
            None => match name.strip_suffix("-up") {
                Some(setting) => (setting, 1),
                None => continue,
            },
        };
        settings.step(setting, direction);
    }
}

/// Left/right on a focused value (`data-setting`, from the shared input
/// module) steps that setting.
fn apply_setting_steps(mut steps: MessageReader<SettingStep>, mut settings: ResMut<Settings>) {
    for step in steps.read() {
        settings.step(&step.setting, step.steps);
    }
}

/// L / gamepad Y switches the language.
fn switch_language(hotkeys: Hotkeys, languages: Res<Languages>, mut active: ResMut<ActiveLocale>) {
    if hotkeys.just_pressed(KeyCode::KeyL, GamepadButton::North) {
        let current = languages
            .0
            .iter()
            .position(|bundle| active.0.as_ref() == Some(bundle))
            .unwrap_or(0);
        active.set(languages.0[(current + 1) % languages.0.len()].clone());
    }
}

/// Writes the settings into the template every frame: bevy_markup only
/// updates the UI when the rendered output changes, and then in place.
fn show_settings(settings: Res<Settings>, mut contexts: Query<&mut TemplateContext>) {
    for mut context in &mut contexts {
        context.insert("settings", &*settings);
    }
}
