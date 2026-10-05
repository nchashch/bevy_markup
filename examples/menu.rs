//! A settings menu for mouse, keyboard and gamepad: focus and directional
//! navigation, a modal confirm dialog, tooltips anchored to their buttons,
//! custom elements and templated inline styles.
//!
//! - `autofocus` picks the first focused button; arrows / D-pad move focus
//!   (`HtmlFocus::navigate`), Enter / A activates it (`HtmlFocus::activate`,
//!   the same `ElementSignal` a click sends). `.button:focus-visible` draws
//!   the focus ring, which a mouse press hides again.
//! - "Reset" opens `menu/dialog.html` with `HtmlModal`: focus moves to its
//!   `autofocus` button and can't leave it until the dialog closes.
//! - Hovering a button shows `menu/tooltip.html` beside it with
//!   `HtmlAnchor`, which also despawns it with its button.
//! - Each root's placement, stacking (`z-index`), dimming background and
//!   pickability are its `<html class>` rule in `menu/style.css`.
//! - `<div is="icon" data-src="…">` runs [`icon`], which inserts the image;
//!   the volume meter is `style="width: {{ volume }}%"`. Changing a setting
//!   updates the menu in place: focus and hover stay where they are.
//! - Every signal says what produced it (`ElementSignal::source`): a pointer
//!   and button, or the key / gamepad button the app passed to `activate`.
//!   Right- or middle-clicking Volume (`data-on-auxclick`) lowers it; the
//!   menu shows what produced the last click.
//!
//! `cargo run --example menu` — L switches the language (English/German),
//! Esc closes the dialog.

use bevy::input_focus::{FocusCause, InputFocus};
use bevy::math::CompassOctant;
use bevy::picking::pointer::{PointerButton, PointerId};
use bevy::prelude::*;
use bevy_markup::prelude::*;

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
                        title: "bevy_markup: menu".into(),
                        ..default()
                    }),
                    ..default()
                }),
            BevyMarkupPlugin,
        ))
        .define_html_element("icon", icon)
        .insert_resource(ClearColor(Color::srgb_u8(0x10, 0x10, 0x14)))
        .init_resource::<Settings>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                navigate,
                handle_signals,
                close_dialog_on_escape,
                switch_language,
                show_settings,
            ),
        )
        .run();
}

const DIFFICULTIES: [&str; 3] = ["easy", "normal", "hard"];

#[derive(Resource)]
struct Settings {
    volume: u32,
    difficulty: usize,
    /// What produced the last click (a `menu-last-input` variant).
    last_input: &'static str,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 50,
            difficulty: 1,
            last_input: "none",
        }
    }
}

/// The menu's root.
#[derive(Component)]
struct Menu;

/// The confirm dialog's root.
#[derive(Component)]
struct Dialog;

/// A tooltip root and the element it's for.
#[derive(Component)]
struct Tooltip(Entity);

/// Both locales, preloaded so switching is immediate.
#[derive(Resource)]
struct Languages(Vec<Handle<BundleAsset>>);

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
    commands.spawn(Camera2d);
    fonts
        .insert("System Serif", FontFaces::new(FontSource::Serif))
        .set_generic(GenericFamily::Serif, "System Serif");
    commands.insert_resource(DefaultStylesheet::new(asset_server.load("menu/style.css")));

    let languages: Vec<Handle<BundleAsset>> = ["en-US", "de"]
        .iter()
        .map(|id| asset_server.load(format!("menu/locales/{id}/main.ftl.ron")))
        .collect();
    commands.insert_resource(ActiveLocale::new(languages[0].clone()));
    commands.insert_resource(Languages(languages));

    // No `Node` needed: `<html class="screen">` sizes and centers the root.
    commands.spawn((Menu, HtmlUi::new(asset_server.load("menu/menu.html"))));
}

/// `<div is="icon" data-src="…">`: the element shows the image. Runs once per
/// spawned element; updates keep the element and its image.
fn icon(icon: In<ElementConnected>, asset_server: Res<AssetServer>, mut commands: Commands) {
    if let Some(src) = icon.data("src") {
        let image = asset_server.load(src.to_owned());
        commands.entity(icon.entity).insert(ImageNode::new(image));
    }
}

/// Writes the settings into the menu every frame: bevy_markup only updates
/// it when the rendered output changes, and then in place.
fn show_settings(settings: Res<Settings>, mut menus: Query<&mut TemplateContext, With<Menu>>) {
    for mut context in &mut menus {
        context.insert("volume", &settings.volume);
        context.insert("difficulty", DIFFICULTIES[settings.difficulty]);
        context.insert("last_input", settings.last_input);
    }
}

/// Keyboard and gamepad drive bevy_markup's focus; the mouse needs nothing.
/// Activation reports which input did it.
fn navigate(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<(Entity, &Gamepad)>,
    mut focus: HtmlFocus,
) {
    let pad = |button: GamepadButton| {
        gamepads
            .iter()
            .find(|(_, pad)| pad.just_pressed(button))
            .map(|(gamepad, _)| ActivationInput::GamepadButton { gamepad, button })
    };
    for (key, button, direction) in [
        (
            KeyCode::ArrowUp,
            GamepadButton::DPadUp,
            CompassOctant::North,
        ),
        (
            KeyCode::ArrowDown,
            GamepadButton::DPadDown,
            CompassOctant::South,
        ),
        (
            KeyCode::ArrowLeft,
            GamepadButton::DPadLeft,
            CompassOctant::West,
        ),
        (
            KeyCode::ArrowRight,
            GamepadButton::DPadRight,
            CompassOctant::East,
        ),
    ] {
        if keys.just_pressed(key) || pad(button).is_some() {
            focus.navigate(direction);
        }
    }
    let activation = if keys.just_pressed(KeyCode::Enter) {
        Some(ActivationInput::Key(KeyCode::Enter))
    } else {
        pad(GamepadButton::South)
    };
    if let Some(input) = activation {
        focus.activate(input);
    }
}

/// The `menu-last-input` variant for a click's source.
fn input_name(source: &SignalSource) -> &'static str {
    match source {
        SignalSource::Pointer {
            pointer: PointerId::Touch(_),
            ..
        } => "touch",
        SignalSource::Pointer { button, .. } => match button {
            PointerButton::Primary => "mouse-primary",
            PointerButton::Secondary => "mouse-secondary",
            PointerButton::Middle => "mouse-middle",
        },
        SignalSource::Activation(ActivationInput::Key(_)) => "key",
        SignalSource::Activation(ActivationInput::GamepadButton { .. }) => "gamepad",
        SignalSource::Activation(ActivationInput::Synthetic) => "synthetic",
        SignalSource::Activation(ActivationInput::Other) | SignalSource::Hover { .. } => "none",
    }
}

/// Every button, tooltip and dialog answer arrives as an `ElementSignal`.
#[allow(clippy::too_many_arguments)]
fn handle_signals(
    mut signals: MessageReader<ElementSignal>,
    mut settings: ResMut<Settings>,
    tooltips: Query<(Entity, &Tooltip)>,
    dialogs: Query<Entity, With<Dialog>>,
    menus: Query<Entity, With<Menu>>,
    elements: HtmlElements,
    mut focus: ResMut<InputFocus>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
) {
    for signal in signals.read() {
        if matches!(
            signal.trigger,
            SignalTrigger::Click | SignalTrigger::AuxClick
        ) {
            settings.last_input = input_name(&signal.source);
        }
        match (signal.name.as_ref(), signal.trigger) {
            ("tip", SignalTrigger::Enter) => {
                let Some(key) = signal.payload["tip"].as_str() else {
                    continue;
                };
                commands.spawn((
                    Tooltip(signal.target),
                    HtmlUi::new(asset_server.load("menu/tooltip.html")),
                    TemplateContext::new().with("key", key),
                    HtmlAnchor::new(signal.target, AnchorPlacement::Right).with_gap(12.0),
                ));
            }
            ("tip", SignalTrigger::Leave) => {
                for (tooltip, &Tooltip(element)) in &tooltips {
                    if element == signal.target {
                        commands.entity(tooltip).despawn();
                    }
                }
            }
            ("volume", SignalTrigger::Click) => {
                settings.volume = (settings.volume + 25) % 125;
            }
            ("volume-down", SignalTrigger::AuxClick) => {
                settings.volume = (settings.volume + 100) % 125;
            }
            ("difficulty", SignalTrigger::Click) => {
                settings.difficulty = (settings.difficulty + 1) % DIFFICULTIES.len();
            }
            ("reset", SignalTrigger::Click) if dialogs.is_empty() => {
                commands.spawn((
                    Dialog,
                    HtmlUi::new(asset_server.load("menu/dialog.html")),
                    HtmlModal,
                ));
            }
            (answer @ ("dialog-yes" | "dialog-no"), SignalTrigger::Click) => {
                if answer == "dialog-yes" {
                    *settings = Settings {
                        last_input: settings.last_input,
                        ..default()
                    };
                }
                for dialog in &dialogs {
                    commands.entity(dialog).despawn();
                }
                // Back to the button that opened it.
                if let Some(reset) = menus.iter().find_map(|menu| elements.by_id(menu, "reset")) {
                    focus.set(reset, FocusCause::Navigated);
                }
            }
            _ => {}
        }
    }
}

/// Esc / B: cancel the dialog (focus returns to the menu by itself).
fn close_dialog_on_escape(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    dialogs: Query<Entity, With<Dialog>>,
    mut commands: Commands,
) {
    let cancel = keys.just_pressed(KeyCode::Escape)
        || gamepads
            .iter()
            .any(|pad| pad.just_pressed(GamepadButton::East));
    if cancel {
        for dialog in &dialogs {
            commands.entity(dialog).despawn();
        }
    }
}

/// L: next language.
fn switch_language(
    keys: Res<ButtonInput<KeyCode>>,
    languages: Res<Languages>,
    mut active: ResMut<ActiveLocale>,
) {
    if !keys.just_pressed(KeyCode::KeyL) {
        return;
    }
    let current = languages
        .0
        .iter()
        .position(|bundle| active.0.as_ref() == Some(bundle))
        .unwrap_or(0);
    active.set(languages.0[(current + 1) % languages.0.len()].clone());
}
