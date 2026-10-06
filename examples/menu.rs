//! A settings menu for mouse, keyboard and gamepad: focus and directional
//! navigation, a modal confirm dialog, built-in tooltips, a shared component
//! library, signals routed to systems, custom elements and templated inline
//! styles.
//!
//! - `autofocus` picks the first focused button; arrows / D-pad move focus
//!   (`HtmlFocus::navigate`), Enter / A activates it (`HtmlFocus::activate`,
//!   the same `ElementSignal` a click sends). `.button:focus-visible` draws
//!   the focus ring, which a mouse press hides again.
//! - "Reset" opens `menu/dialog.html` with `HtmlModal`: focus moves to its
//!   `autofocus` button and can't leave it until the dialog closes.
//! - Buttons are one Tera 2 component, `ui.button` in `menu/components.html`,
//!   which `menu.html` and `dialog.html` include (paths relative to the
//!   including file) and call with their content as the body.
//! - Hovering a button with `data-tooltip="<Fluent key>"` shows
//!   `menu/tooltip.html` beside it: `HtmlTooltips` is all the app does.
//! - Each button's `data-on-click` name is routed to its own system with
//!   `on_html_click` / `on_html_signal`; the dialog's two buttons share one
//!   handler and tell themselves apart by `data-answer`
//!   (`ElementSignal::data`). A plain `MessageReader` sees the same signals
//!   ([`record_last_input`]).
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
        .on_html_click("volume", volume_up)
        .on_html_signal("volume-down", volume_down)
        .on_html_click("difficulty", next_difficulty)
        .on_html_click("reset", open_dialog)
        .on_html_click("dialog", answer_dialog)
        .insert_resource(ClearColor(Color::srgb_u8(0x10, 0x10, 0x14)))
        .init_resource::<Settings>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                navigate,
                record_last_input,
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

/// Both locales, preloaded so switching is immediate.
#[derive(Resource)]
struct Languages(Vec<Handle<BundleAsset>>);

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
    commands.spawn(Camera2d);
    fonts
        .insert("System Serif", FontFaces::new(FontSource::Serif))
        .set_generic(GenericFamily::Serif, "System Serif");
    commands.insert_resource(DefaultStylesheet::new(asset_server.load("menu/style.css")));
    // `data-tooltip` elements show this template (context: `key`, `args`,
    // `placement`) beside them while hovered.
    commands
        .insert_resource(HtmlTooltips::new(asset_server.load("menu/tooltip.html")).with_gap(12.0));

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

/// Every signal is still an `ElementSignal` message: a reader sees the
/// routed ones too. Records what produced the last click.
fn record_last_input(mut signals: MessageReader<ElementSignal>, mut settings: ResMut<Settings>) {
    for signal in signals.read() {
        if matches!(
            signal.trigger,
            SignalTrigger::Click | SignalTrigger::AuxClick
        ) {
            settings.last_input = input_name(&signal.source);
        }
    }
}

/// `data-on-click="volume"`: primary click, Enter or A.
fn volume_up(_: In<ElementSignal>, mut settings: ResMut<Settings>) {
    settings.volume = (settings.volume + 25) % 125;
}

/// `data-on-auxclick="volume-down"`: right or middle click.
fn volume_down(_: In<ElementSignal>, mut settings: ResMut<Settings>) {
    settings.volume = (settings.volume + 100) % 125;
}

fn next_difficulty(_: In<ElementSignal>, mut settings: ResMut<Settings>) {
    settings.difficulty = (settings.difficulty + 1) % DIFFICULTIES.len();
}

/// Opens the confirm dialog; `HtmlModal` keeps focus inside it.
fn open_dialog(
    _: In<ElementSignal>,
    dialogs: Query<(), With<Dialog>>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
) {
    if dialogs.is_empty() {
        commands.spawn((
            Dialog,
            HtmlUi::new(asset_server.load("menu/dialog.html")),
            HtmlModal,
        ));
    }
}

/// Both dialog buttons send `dialog`; `data-answer` says which.
fn answer_dialog(
    signal: In<ElementSignal>,
    mut settings: ResMut<Settings>,
    dialogs: Query<Entity, With<Dialog>>,
    menus: Query<Entity, With<Menu>>,
    elements: HtmlElements,
    mut focus: ResMut<InputFocus>,
    mut commands: Commands,
) {
    if signal.data("answer") == Some("yes") {
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
