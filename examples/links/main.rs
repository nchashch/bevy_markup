//! Links: `<a href="…">` becomes a picked inline run that emits the `open`
//! signal with the href when clicked (ADR 0011) — bevy_markup has no HTTP
//! client, so the app decides what opening means. This one records it: the
//! status line at the bottom shows the last opened href.
//!
//! The mouse needs nothing. For the keyboard and gamepad, the app cycles
//! the links itself (left/right through the document's link runs, setting
//! Bevy's `InputFocus` — the focused link lights up like a hovered one, and
//! Enter / A opens it with `HtmlFocus::activate`).
//!
//! `cargo run --example links` — click a link, or ← → / LB RB to cycle and
//! Enter / A to open. Space / Y switches between English and German.

use bevy::input_focus::{FocusCause, InputFocus, InputFocusVisible};
use bevy::prelude::*;
use bevy_markup::prelude::*;

#[path = "../shared/harness.rs"]
mod harness;

fn main() {
    App::new()
        .add_plugins((
            // The example assets live beside the examples, not in ./assets.
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: "examples/assets".into(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "bevy_markup: links".into(),
                        ..default()
                    }),
                    ..default()
                }),
            BevyMarkupPlugin,
            harness::HarnessPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (cycle_links, activate_link, open_links, switch_language),
        )
        .run();
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

    commands.insert_resource(DefaultStylesheet::new(asset_server.load("links/style.css")));

    let languages: Vec<Handle<LocaleBundle>> = ["en-US", "de"]
        .iter()
        .map(|id| asset_server.load(format!("links/locales/{id}/main.ftl.ron")))
        .collect();
    commands.insert_resource(ActiveLocale::new(languages[0].clone()));
    commands.insert_resource(Languages(languages));

    commands.spawn((
        HtmlUi::new(asset_server.load("links/report.html")),
        TemplateContext::new().with("opened", "never"),
        Node {
            width: Val::Px(560.0),
            margin: UiRect::all(Val::Auto),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(14.0),
            ..default()
        },
    ));
}

/// Keyboard and gamepad: step through the document's links in order and
/// focus them. The link spans aren't `Focusable` blocks (inline runs have no
/// box to lay out), so the app keeps the cycle itself — `InputFocus` is all
/// the focus machinery needs: the focused span gets `:focus-visible`
/// styling, and activation reads its bindings.
fn cycle_links(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    root: Query<Entity, With<HtmlUi>>,
    elements: HtmlElements,
    spans: Query<&ElementSignals>,
    mut focus: ResMut<InputFocus>,
    mut visible: ResMut<InputFocusVisible>,
    mut last: Local<usize>,
) {
    let step = if keys.just_pressed(KeyCode::ArrowRight)
        || gamepads
            .iter()
            .any(|pad| pad.just_pressed(GamepadButton::RightTrigger))
    {
        1
    } else if keys.just_pressed(KeyCode::ArrowLeft)
        || gamepads
            .iter()
            .any(|pad| pad.just_pressed(GamepadButton::LeftTrigger))
    {
        -1
    } else {
        return;
    };
    let Ok(root) = root.single() else {
        return;
    };
    // The link runs in document order.
    let order: Vec<Entity> = elements
        .by_tag(root, "a")
        .filter(|entity| {
            spans
                .get(*entity)
                .is_ok_and(|signals| signals.0.iter().any(|binding| binding.name == "open"))
        })
        .collect();
    if order.is_empty() {
        return;
    }
    let index = focus
        .get()
        .and_then(|current| order.iter().position(|&entity| entity == current))
        .unwrap_or_else(|| (*last).min(order.len() - 1));
    let next = ((index as i32 + step).rem_euclid(order.len() as i32)) as usize;
    *last = next;
    focus.set(order[next], FocusCause::Navigated);
    visible.0 = true;
}

/// Enter / gamepad A: activate the focused link — the same `open` signal a
/// click sends, with the key or button as its source.
fn activate_link(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<(Entity, &Gamepad)>,
    mut focus: HtmlFocus,
) {
    let pad = |button| gamepads.iter().find(|(_, pad)| pad.just_pressed(button));
    if keys.just_pressed(KeyCode::Enter) {
        focus.activate(ActivationInput::Key(KeyCode::Enter));
    } else if let Some((gamepad, _)) = pad(GamepadButton::South) {
        focus.activate(ActivationInput::GamepadButton {
            gamepad,
            button: GamepadButton::South,
        });
    }
}

/// Clicks (and activations) on a link arrive as the `open` signal with the
/// href; the app records it in the context, and the status paragraph
/// re-renders.
fn open_links(
    mut signals: MessageReader<ElementSignal>,
    mut contexts: Query<&mut TemplateContext>,
) {
    for signal in signals.read() {
        if signal.name == "open"
            && let Some(href) = signal.payload.get("href").and_then(|href| href.as_str())
        {
            for mut context in &mut contexts {
                context.insert("opened", href);
            }
        }
    }
}

fn switch_language(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    languages: Res<Languages>,
    mut active: ResMut<ActiveLocale>,
) {
    let pad = gamepads
        .iter()
        .any(|pad| pad.just_pressed(GamepadButton::North));
    if keys.just_pressed(KeyCode::Space) || pad {
        next_language(&languages, &mut active);
    }
}

fn next_language(languages: &Languages, active: &mut ActiveLocale) {
    let current = languages
        .0
        .iter()
        .position(|bundle| active.0.as_ref() == Some(bundle))
        .unwrap_or(0);
    active.set(languages.0[(current + 1) % languages.0.len()].clone());
}
