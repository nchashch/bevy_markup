//! Keyboard and gamepad input shared by the examples (`#[path]`-included;
//! not an example itself). bevy_markup owns focus and activation but reads
//! no input (the app picks its bindings), so this is the app side:
//!
//! - arrows / D-pad / left stick move focus ([`HtmlFocus::navigate`]), with
//!   hold-to-repeat; on a focused element with `data-setting="<name>"`,
//!   left/right instead send a [`SettingStep`] for the app to apply
//!   (sliders, steppers). With [`ArrowMode::Linear`] (an app resource),
//!   left/right instead step through the focusable elements in document
//!   order and up/down are left to the app (the demo scrolls with them);
//! - Enter / gamepad A activate the focused element
//!   ([`HtmlFocus::activate`]): the same `ElementSignal` a click sends, with
//!   the key or button as its source;
//! - [`Hotkeys`] maps an example's shortcut key to a gamepad button too.
//!
//! The mouse needs nothing: picking drives hover, clicks and `:focus-visible`
//! (a press hides the ring, navigating shows it).

// Each example uses the parts it needs.
#![allow(dead_code)]

use bevy::ecs::system::SystemParam;
use bevy::input_focus::{FocusCause, InputFocus, InputFocusVisible};
use bevy::math::CompassOctant;
use bevy::prelude::*;
use bevy_markup::prelude::*;

pub struct ExampleInputPlugin;

impl Plugin for ExampleInputPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SettingStep>()
            .init_resource::<ArrowMode>()
            .add_systems(
                Update,
                (
                    navigate.run_if(resource_equals(ArrowMode::Directional)),
                    step_linear.run_if(resource_equals(ArrowMode::Linear)),
                    activate,
                ),
            );
    }
}

/// What the arrows / D-pad / left stick do.
#[derive(Resource, Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum ArrowMode {
    /// All four directions move focus to the nearest element that way.
    #[default]
    Directional,
    /// Left/right step to the previous/next focusable element in document
    /// order; up/down do nothing here (the app uses them, e.g. to scroll).
    Linear,
}

/// Left/right on a focused `data-setting` element: step that setting.
#[derive(Message, Clone, Debug)]
pub struct SettingStep {
    /// The element's `data-setting` value.
    pub setting: String,
    /// -1 or 1.
    pub steps: i32,
}

/// An example's shortcut on the keyboard *and* a gamepad button.
#[derive(SystemParam)]
pub struct Hotkeys<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    gamepads: Query<'w, 's, &'static Gamepad>,
}

impl Hotkeys<'_, '_> {
    pub fn just_pressed(&self, key: KeyCode, button: GamepadButton) -> bool {
        self.keys.just_pressed(key) || self.gamepads.iter().any(|pad| pad.just_pressed(button))
    }
}

/// Hold-to-repeat for one axis: fires on press, then after [`Self::DELAY`]
/// every [`Self::INTERVAL`] while held.
#[derive(Default)]
pub struct Repeat {
    held: i32,
    next: f32,
}

impl Repeat {
    pub const DELAY: f32 = 0.35;
    pub const INTERVAL: f32 = 0.08;

    /// `direction` is -1, 0 or 1 this frame; returns the direction to act on.
    pub fn update(&mut self, direction: i32, now: f32) -> i32 {
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

/// Held directional input: -1/0/1 per axis from the arrow keys, the D-pad
/// and the left stick (any gamepad). `x` is right-positive, `y` up-positive.
#[derive(SystemParam)]
pub struct Arrows<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    gamepads: Query<'w, 's, &'static Gamepad>,
}

impl Arrows<'_, '_> {
    pub fn axes(&self) -> IVec2 {
        let axis = |negative: KeyCode,
                    positive: KeyCode,
                    pad_negative,
                    pad_positive,
                    stick: fn(&Gamepad) -> f32| {
            let pad = |button| self.gamepads.iter().any(|pad| pad.pressed(button));
            let stick = self
                .gamepads
                .iter()
                .map(stick)
                .find(|value| value.abs() > 0.5);
            let positive =
                self.keys.pressed(positive) || pad(pad_positive) || stick.is_some_and(|v| v > 0.0);
            let negative =
                self.keys.pressed(negative) || pad(pad_negative) || stick.is_some_and(|v| v < 0.0);
            positive as i32 - negative as i32
        };
        IVec2::new(
            axis(
                KeyCode::ArrowLeft,
                KeyCode::ArrowRight,
                GamepadButton::DPadLeft,
                GamepadButton::DPadRight,
                |pad| pad.left_stick().x,
            ),
            axis(
                KeyCode::ArrowDown,
                KeyCode::ArrowUp,
                GamepadButton::DPadDown,
                GamepadButton::DPadUp,
                |pad| pad.left_stick().y,
            ),
        )
    }
}

fn navigate(
    time: Res<Time>,
    arrows: Arrows,
    elements: Query<&HtmlElement>,
    #[cfg(feature = "text-input")] text_fields: Query<(), With<bevy::ui_widgets::TextInput>>,
    mut repeat: Local<(Repeat, Repeat)>,
    mut steps: MessageWriter<SettingStep>,
    mut focus: HtmlFocus,
) {
    let IVec2 {
        x: horizontal,
        y: vertical,
    } = arrows.axes();
    // While a text field holds the focus, left/right move its caret (the
    // widget reads the arrow keys through `FocusedInput`), so they don't
    // navigate; up/down still leave the field.
    #[cfg(feature = "text-input")]
    let horizontal = if focus
        .focused()
        .is_some_and(|entity| text_fields.contains(entity))
    {
        0
    } else {
        horizontal
    };
    let now = time.elapsed_secs();
    let (horizontal_repeat, vertical_repeat) = &mut *repeat;
    match vertical_repeat.update(vertical, now) {
        1 => drop(focus.navigate(CompassOctant::North)),
        -1 => drop(focus.navigate(CompassOctant::South)),
        _ => {}
    }
    let setting = focus
        .focused()
        .and_then(|entity| elements.get(entity).ok())
        .and_then(|element| element.data("setting").map(str::to_owned));
    match (horizontal_repeat.update(horizontal, now), setting) {
        (0, _) => {}
        (steps_by, Some(setting)) => {
            steps.write(SettingStep {
                setting,
                steps: steps_by,
            });
        }
        (1, None) => drop(focus.navigate(CompassOctant::East)),
        (_, None) => drop(focus.navigate(CompassOctant::West)),
    }
}

/// The entity's (global, local) z-order, for picking the topmost modal: a
/// missing component counts as 0, like the library's `active_modal` (both
/// were required, and most roots have no `GlobalZIndex`, so every modal
/// tied and the last one iterated won — review_0003 G40).
fn z_order(entity: Entity, global: &Query<&GlobalZIndex>, local: &Query<&ZIndex>) -> (i32, i32) {
    (
        global.get(entity).map_or(0, |z| z.0),
        local.get(entity).map_or(0, |z| z.0),
    )
}

/// Whether `entity` shows: the nearest ancestor-or-self with an explicit
/// `Visibility` decides (`Inherited` defers to the parent), as Bevy's
/// visibility propagation and the library's focus scope read it.
fn shown(entity: Entity, visibility: &Query<&Visibility>, parents: &Query<&ChildOf>) -> bool {
    for ancestor in std::iter::once(entity).chain(parents.iter_ancestors(entity)) {
        match visibility.get(ancestor) {
            Ok(Visibility::Hidden) => return false,
            Ok(Visibility::Visible) => return true,
            _ => {}
        }
    }
    true
}

/// The roots the linear walk visits, in order — the library's
/// `active_modal` rule: hidden and `HtmlNoFocus` roots are skipped, and an
/// open modal (the visible one, topmost by z-order) confines the walk to
/// its subtree.
fn walk_roots(
    roots: impl Iterator<Item = Entity>,
    no_focus_roots: &Query<(), With<HtmlNoFocus>>,
    modals: &Query<(), With<HtmlModal>>,
    visibility: &Query<&Visibility>,
    parents: &Query<&ChildOf>,
    global_z: &Query<&GlobalZIndex>,
    z_index: &Query<&ZIndex>,
) -> Vec<Entity> {
    let candidates: Vec<Entity> = roots
        .filter(|&root| !no_focus_roots.contains(root) && shown(root, visibility, parents))
        .collect();
    let modal = candidates
        .iter()
        .copied()
        .filter(|&root| modals.contains(root))
        .max_by_key(|&root| z_order(root, global_z, z_index));
    match modal {
        Some(modal) => vec![modal],
        None => candidates,
    }
}

/// [`ArrowMode::Linear`]: left/right step through the focusable elements in
/// document order (a depth-first walk of the top-level UI roots), stopping
/// at the ends, and show the focus ring like navigating does.
#[allow(clippy::too_many_arguments)]
fn step_linear(
    time: Res<Time>,
    arrows: Arrows,
    mut repeat: Local<Repeat>,
    roots: Query<Entity, (With<HtmlUi>, Without<ChildOf>)>,
    children: Query<&Children>,
    focusable: Query<(), (With<Focusable>, Without<HtmlNoFocus>)>,
    no_focus_roots: Query<(), With<HtmlNoFocus>>,
    modals: Query<(), With<HtmlModal>>,
    visibility: Query<&Visibility>,
    parents: Query<&ChildOf>,
    global_z: Query<&GlobalZIndex>,
    z_index: Query<&ZIndex>,
    mut focus: ResMut<InputFocus>,
    mut visible: ResMut<InputFocusVisible>,
) {
    let step = repeat.update(arrows.axes().x, time.elapsed_secs());
    if step == 0 {
        return;
    }
    let topmost = walk_roots(
        roots.iter(),
        &no_focus_roots,
        &modals,
        &visibility,
        &parents,
        &global_z,
        &z_index,
    );
    let mut order = Vec::new();
    for root in &topmost {
        let mut stack = vec![*root];
        while let Some(entity) = stack.pop() {
            if focusable.contains(entity) {
                order.push(entity);
            }
            if let Ok(kids) = children.get(entity) {
                stack.extend(kids.iter().rev());
            }
        }
    }
    let next = match focus
        .get()
        .and_then(|current| order.iter().position(|e| *e == current))
    {
        Some(index) => (index as i32 + step).clamp(0, order.len() as i32 - 1) as usize,
        None => 0,
    };
    if let Some(&entity) = order.get(next) {
        focus.set(entity, FocusCause::Navigated);
        visible.0 = true;
    }
}

/// Enter / gamepad A: activate the focused element.
fn activate(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<(Entity, &Gamepad)>,
    mut focus: HtmlFocus,
) {
    let activation = if keys.just_pressed(KeyCode::Enter) {
        Some(ActivationInput::Key(KeyCode::Enter))
    } else {
        gamepads
            .iter()
            .find(|(_, pad)| pad.just_pressed(GamepadButton::South))
            .map(|(gamepad, _)| ActivationInput::GamepadButton {
                gamepad,
                button: GamepadButton::South,
            })
    };
    if let Some(input) = activation {
        focus.activate(input);
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    /// The topmost modal is chosen by z-order with a missing component as 0
    /// (review_0003 G40: requiring both made every modal without a
    /// `GlobalZIndex` tie, so the last one iterated won), and a hidden or
    /// `HtmlNoFocus` root never captures the walk.
    #[test]
    fn linear_walk_confines_to_the_topmost_visible_modal() {
        let mut world = World::new();
        let low = world.spawn((HtmlModal, ZIndex(10))).id();
        let high = world.spawn((HtmlModal, ZIndex(20))).id();
        let plain = world.spawn_empty().id();
        let hidden = world
            .spawn((HtmlModal, ZIndex(30), Visibility::Hidden))
            .id();
        let no_focus = world.spawn((HtmlModal, ZIndex(40), HtmlNoFocus)).id();
        let walk = |world: &mut World, roots: Vec<Entity>| {
            world
                .run_system_once(
                    move |no_focus_roots: Query<(), With<HtmlNoFocus>>,
                          modals: Query<(), With<HtmlModal>>,
                          visibility: Query<&Visibility>,
                          parents: Query<&ChildOf>,
                          global_z: Query<&GlobalZIndex>,
                          z_index: Query<&ZIndex>| {
                        walk_roots(
                            roots.clone().into_iter(),
                            &no_focus_roots,
                            &modals,
                            &visibility,
                            &parents,
                            &global_z,
                            &z_index,
                        )
                    },
                )
                .unwrap()
        };
        // Either iteration order: z 20 wins, whatever comes last.
        assert_eq!(
            walk(&mut world, vec![high, low, plain, hidden, no_focus]),
            vec![high]
        );
        assert_eq!(
            walk(&mut world, vec![no_focus, hidden, plain, high, low]),
            vec![high]
        );
        // No visible modal: every shown, focus-taking root participates.
        assert_eq!(walk(&mut world, vec![plain, hidden, no_focus]), vec![plain]);
    }
}
