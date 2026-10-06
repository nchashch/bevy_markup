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
//! - `<div is="icon" data-src="…">` runs [`icon`], which inserts the image.
//! - Volume is a slider. The bar is focusable (`tabindex="0"`): while it has
//!   focus, ← → / D-pad / left stick step it (held: repeating), up and down
//!   still move focus. `<div is="slider">` runs [`slider`], which observes
//!   picking presses and drags on the bar to set the value where the pointer
//!   is. The ◀ ▶ buttons step it with the mouse (`tabindex="-1"`: focus
//!   skips them). The fill is `style="width: {{ volume }}%"`. Changing a
//!   setting updates the menu in place: focus and hover stay where they are.
//! - Difficulty is ordinal, so it's a stepper: ◀ [focusable label] ▶. The
//!   label isn't a button (`tabindex="0"`, no `data-on-click`); ← → / D-pad
//!   / left stick step it while focused, stopping at Easy and Hard. Both
//!   controls carry `data-setting`, which is how [`navigate`] knows a focused
//!   element takes left/right itself.
//! - Every signal says what produced it (`ElementSignal::source`): a pointer
//!   and button, or the key / gamepad button the app passed to `activate`;
//!   the menu shows what produced the last click.
//!
//! `cargo run --example menu` — L switches the language (English/German),
//! Esc closes the dialog.

use bevy::input_focus::{FocusCause, InputFocus};
use bevy::math::CompassOctant;
use bevy::picking::pointer::{PointerButton, PointerId};
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
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
        .define_html_element("slider", slider)
        .on_html_click(
            "volume-up",
            |_: In<ElementSignal>, mut settings: ResMut<Settings>| {
                settings.step_volume(1);
            },
        )
        .on_html_click(
            "volume-down",
            |_: In<ElementSignal>, mut settings: ResMut<Settings>| {
                settings.step_volume(-1);
            },
        )
        .on_html_click(
            "difficulty-up",
            |_: In<ElementSignal>, mut settings: ResMut<Settings>| {
                settings.step("difficulty", 1);
            },
        )
        .on_html_click(
            "difficulty-down",
            |_: In<ElementSignal>, mut settings: ResMut<Settings>| {
                settings.step("difficulty", -1);
            },
        )
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

/// One slider step (arrows, keys, D-pad, stick), in percent.
const VOLUME_STEP: i32 = 5;

#[derive(Resource)]
struct Settings {
    volume: u32,
    difficulty: usize,
    /// What produced the last click (a `menu-last-input` variant).
    last_input: &'static str,
}

impl Settings {
    /// `steps` slider steps up (+) or down (-), clamped to 0–100.
    fn step_volume(&mut self, steps: i32) {
        self.volume = (self.volume as i32 + steps * VOLUME_STEP).clamp(0, 100) as u32;
    }

    /// Steps the setting a focused control's `data-setting` names. Difficulty
    /// is ordinal: it stops at Easy and Hard rather than wrapping.
    fn step(&mut self, setting: &str, steps: i32) {
        match setting {
            "volume" => self.step_volume(steps),
            "difficulty" => {
                let last = DIFFICULTIES.len() as i32 - 1;
                self.difficulty = (self.difficulty as i32 + steps).clamp(0, last) as usize;
            }
            _ => {}
        }
    }
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

/// `<div is="slider">`: pressing or dragging on the bar sets the volume to
/// the pointer's position along it. Observers attached once per spawned
/// element; updates keep the element, so they stay.
fn slider(slider: In<ElementConnected>, mut commands: Commands) {
    let bar = slider.entity;
    commands
        .entity(bar)
        .observe(
            move |press: On<Pointer<Press>>,
                  bars: Query<(&ComputedNode, &UiGlobalTransform)>,
                  mut settings: ResMut<Settings>| {
                if press.button == PointerButton::Primary {
                    settings.last_input = match press.pointer_id {
                        PointerId::Touch(_) => "touch",
                        _ => "mouse-primary",
                    };
                    set_volume_at(bar, press.pointer_location.position, &bars, &mut settings);
                }
            },
        )
        .observe(
            move |drag: On<Pointer<Drag>>,
                  bars: Query<(&ComputedNode, &UiGlobalTransform)>,
                  mut settings: ResMut<Settings>| {
                if drag.button == PointerButton::Primary {
                    set_volume_at(bar, drag.pointer_location.position, &bars, &mut settings);
                }
            },
        );
}

/// The volume at `position` (viewport px) along `bar`, in whole steps.
fn set_volume_at(
    bar: Entity,
    position: Vec2,
    bars: &Query<(&ComputedNode, &UiGlobalTransform)>,
    settings: &mut Settings,
) {
    let Ok((node, transform)) = bars.get(bar) else {
        return;
    };
    // Layout is in physical px; pointers report logical px.
    let width = node.size.x * node.inverse_scale_factor;
    let left = transform.translation.x * node.inverse_scale_factor - width / 2.0;
    if width <= 0.0 {
        return;
    }
    let fraction = ((position.x - left) / width).clamp(0.0, 1.0);
    let steps = (fraction * 100.0 / VOLUME_STEP as f32).round() as u32;
    settings.volume = steps * VOLUME_STEP as u32;
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

/// Hold-to-repeat for one axis of directional input: fires on press, then
/// after [`Self::DELAY`] every [`Self::INTERVAL`] while held.
#[derive(Default)]
struct Repeat {
    held: i32,
    next: f32,
}

impl Repeat {
    const DELAY: f32 = 0.35;
    const INTERVAL: f32 = 0.08;

    /// `direction` is -1, 0 or 1 this frame; returns the direction to act on.
    fn update(&mut self, direction: i32, now: f32) -> i32 {
        if direction == 0 {
            self.held = 0;
            return 0;
        }
        if direction != self.held {
            self.held = direction;
            self.next = now + Self::DELAY;
            return direction;
        }
        if now >= self.next {
            self.next = now + Self::INTERVAL;
            return direction;
        }
        0
    }
}

/// Keyboard and gamepad drive bevy_markup's focus; the mouse needs nothing.
/// Arrows / D-pad / left stick move focus — except left and right on the
/// focused volume slider, which step it. Activation reports which input did
/// it.
#[allow(clippy::too_many_arguments)]
fn navigate(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<(Entity, &Gamepad)>,
    elements: Query<&HtmlElement>,
    mut settings: ResMut<Settings>,
    mut repeat: Local<(Repeat, Repeat)>,
    mut focus: HtmlFocus,
) {
    // -1/0/1 per axis from keys, D-pad and left stick (any gamepad).
    let axis = |negative: KeyCode,
                positive: KeyCode,
                pad_negative,
                pad_positive,
                stick: fn(&Gamepad) -> f32| {
        let pad = |button| gamepads.iter().any(|(_, pad)| pad.pressed(button));
        let stick = gamepads
            .iter()
            .map(|(_, pad)| stick(pad))
            .find(|value| value.abs() > 0.5);
        let positive =
            keys.pressed(positive) || pad(pad_positive) || stick.is_some_and(|v| v > 0.0);
        let negative =
            keys.pressed(negative) || pad(pad_negative) || stick.is_some_and(|v| v < 0.0);
        positive as i32 - negative as i32
    };
    let horizontal = axis(
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        GamepadButton::DPadLeft,
        GamepadButton::DPadRight,
        |pad| pad.left_stick().x,
    );
    // Up is positive on the stick.
    let vertical = axis(
        KeyCode::ArrowDown,
        KeyCode::ArrowUp,
        GamepadButton::DPadDown,
        GamepadButton::DPadUp,
        |pad| pad.left_stick().y,
    );
    let now = time.elapsed_secs();
    let (horizontal_repeat, vertical_repeat) = &mut *repeat;
    match vertical_repeat.update(vertical, now) {
        1 => drop(focus.navigate(CompassOctant::North)),
        -1 => drop(focus.navigate(CompassOctant::South)),
        _ => {}
    }
    // A focused control with `data-setting` (the volume slider, the
    // difficulty stepper) takes left/right itself.
    let setting = focus
        .focused()
        .and_then(|entity| elements.get(entity).ok())
        .and_then(|element| element.data("setting").map(str::to_owned));
    match (horizontal_repeat.update(horizontal, now), setting) {
        (0, _) => {}
        (direction, Some(setting)) => settings.step(&setting, direction),
        (1, None) => drop(focus.navigate(CompassOctant::East)),
        (_, None) => drop(focus.navigate(CompassOctant::West)),
    }

    let pad = |button: GamepadButton| {
        gamepads
            .iter()
            .find(|(_, pad)| pad.just_pressed(button))
            .map(|(gamepad, _)| ActivationInput::GamepadButton { gamepad, button })
    };
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
