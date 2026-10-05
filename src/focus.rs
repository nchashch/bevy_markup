//! Focus and directional (gamepad / keyboard) navigation, browser-style.
//!
//! - **Focusable elements**: elements with a `data-on-click` hook (the
//!   counterpart of a `<button>`) and elements with `tabindex="0"` (or any
//!   non-negative value); `tabindex="-1"` opts an element out. They get a
//!   [`Focusable`] component.
//! - **Focus** is Bevy's own [`InputFocus`] resource, so markup UIs share it
//!   with any other UI in the app. [`InputFocusVisible`] says whether focus
//!   should be *shown*: directional navigation sets it, a pointer press
//!   clears it (the browser's `:focus-visible` heuristic).
//! - **CSS**: `:focus` matches the focused element, `:focus-visible` the
//!   focused element while focus is shown; `outline` draws a Bevy
//!   [`Outline`] — the usual focus ring:
//!
//!   ```css
//!   .button:focus-visible { outline: 2px solid #f9c74f; outline-offset: 2px }
//!   ```
//! - **Autofocus**: when focus isn't on a focusable element in scope (nothing
//!   focused yet, the focused element was despawned, a modal opened), focus
//!   moves to the element with the `autofocus` attribute, else the first
//!   focusable element in scope.
//! - **Rebuilds keep focus**: a rebuild replaces every element entity; focus
//!   returns to the element with the same `id` in the same `HtmlUi` (give
//!   focusable elements an `id`).
//! - **Scope**: every visible `HtmlUi` takes part, except roots marked
//!   [`HtmlNoFocus`] (e.g. UI rendered onto a 3D quad and driven by a laser).
//!   A visible [`HtmlModal`] root confines focus and navigation to itself
//!   (like a modal `<dialog>`); with several, the highest `GlobalZIndex` wins.
//!   An element's scope is its nearest `HtmlUi` ancestor.
//! - **Input is the app's**: bevy_markup reads no keys or buttons. Bind your
//!   own input and call [`HtmlFocus::navigate`] / [`HtmlFocus::activate`].
//!   Activation emits the element's `data-on-click` signals as
//!   [`ElementSignal`] messages, exactly like a pointer click (with
//!   `position: None`). Navigating off the edge in a direction with no
//!   neighbour triggers [`FocusEdge`] on the focused element (e.g. to page a
//!   list).

use std::borrow::Cow;

use bevy::ecs::system::SystemParam;
use bevy::input_focus::directional_navigation::DirectionalNavigationPlugin;
use bevy::input_focus::{FocusCause, InputFocus, InputFocusVisible};
use bevy::math::CompassOctant;
use bevy::picking::events::{Pointer, Press};
use bevy::prelude::*;
use bevy::ui::UiSystems;
use bevy::ui::auto_directional_navigation::{AutoDirectionalNavigation, AutoDirectionalNavigator};

use crate::HtmlUiSystems;
use crate::html::{HtmlElement, HtmlUi};
use crate::signals::{ElementSignal, ElementSignals, PseudoState, SignalBinding, SignalTrigger};

/// A focusable element (see the [module docs](self)). `autofocus` is the
/// HTML attribute.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
pub struct Focusable {
    pub autofocus: bool,
}

/// On an `HtmlUi` root: while it's visible, focus and navigation stay inside
/// it. The highest `GlobalZIndex` wins among several.
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component, Default)]
pub struct HtmlModal;

/// On an `HtmlUi` root: its elements never take focus or navigation (they
/// stay clickable).
#[derive(Component, Clone, Copy, Debug, Default, Reflect)]
#[reflect(Component, Default)]
pub struct HtmlNoFocus;

/// Triggered on the focused element when [`HtmlFocus::navigate`] finds no
/// focusable neighbour in `direction`.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct FocusEdge {
    pub entity: Entity,
    pub direction: CompassOctant,
}

/// Trigger on an element to activate it: emits its `data-on-click` signals
/// as [`ElementSignal`]s (`position: None`), like a pointer click.
/// [`HtmlFocus::activate`] triggers it on the focused element.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct ActivateElement {
    pub entity: Entity,
}

/// Drive focus from the app's own input bindings.
#[derive(SystemParam)]
pub struct HtmlFocus<'w, 's> {
    navigator: AutoDirectionalNavigator<'w, 's>,
    visible: ResMut<'w, InputFocusVisible>,
    commands: Commands<'w, 's>,
}

impl HtmlFocus<'_, '_> {
    /// Moves focus to the nearest focusable element in `direction` and shows
    /// it (`:focus-visible`). Returns the newly focused element; at an edge
    /// it triggers [`FocusEdge`] instead and returns `None`.
    pub fn navigate(&mut self, direction: CompassOctant) -> Option<Entity> {
        self.visible.0 = true;
        match self.navigator.navigate(direction) {
            Ok(entity) => Some(entity),
            Err(_) => {
                if let Some(entity) = self.navigator.input_focus() {
                    self.commands.trigger(FocusEdge { entity, direction });
                }
                None
            }
        }
    }

    /// Activates the focused element (see [`ActivateElement`]).
    pub fn activate(&mut self) {
        if let Some(entity) = self.navigator.input_focus() {
            self.commands.trigger(ActivateElement { entity });
        }
    }

    /// The focused entity, if any.
    pub fn focused(&mut self) -> Option<Entity> {
        self.navigator.input_focus()
    }
}

/// `tag`'s focusability: `tabindex` decides when present (`>= 0` focusable,
/// negative not); otherwise elements with a `data-on-click` hook are.
pub(crate) fn focusable(tag: &tl::HTMLTag, signals: &[SignalBinding]) -> Option<Focusable> {
    let attributes = tag.attributes();
    let tabindex = attributes
        .get("tabindex")
        .flatten()
        .and_then(|value| value.as_utf8_str().trim().parse::<i32>().ok());
    let focusable = match tabindex {
        Some(index) => index >= 0,
        None => signals
            .iter()
            .any(|binding| binding.trigger == SignalTrigger::Click),
    };
    focusable.then(|| Focusable {
        autofocus: attributes.contains("autofocus"),
    })
}

pub(crate) fn plugin(app: &mut App) {
    if !app.is_plugin_added::<DirectionalNavigationPlugin>() {
        app.add_plugins(DirectionalNavigationPlugin);
    }
    app.init_resource::<InputFocus>()
        .init_resource::<InputFocusVisible>()
        .init_resource::<FocusMemory>()
        .add_observer(activate_element)
        .add_observer(focus_on_press)
        .add_systems(
            PostUpdate,
            (
                (remember_focus, update_focus_states).before(HtmlUiSystems::Render),
                (sync_navigation, repair_focus)
                    .chain()
                    .after(HtmlUiSystems::Build)
                    .before(UiSystems::Prepare),
            ),
        );
}

/// The `HtmlUi` `entity` belongs to (itself included).
fn html_root(
    entity: Entity,
    parents: &Query<&ChildOf>,
    roots: &Query<(), With<HtmlUi>>,
) -> Option<Entity> {
    std::iter::once(entity)
        .chain(parents.iter_ancestors(entity))
        .find(|&ancestor| roots.contains(ancestor))
}

/// Whether `entity` is visible in the hierarchy, resolved from `Visibility`
/// directly (the nearest non-`Inherited` value up the tree decides; all
/// `Inherited` = visible) rather than `InheritedVisibility`, which lags a
/// frame behind spawns and needs the render-side propagation systems.
fn visible_in_hierarchy(
    entity: Entity,
    visibility: &Query<&Visibility>,
    parents: &Query<&ChildOf>,
) -> bool {
    for ancestor in std::iter::once(entity).chain(parents.iter_ancestors(entity)) {
        match visibility.get(ancestor) {
            Ok(Visibility::Hidden) => return false,
            Ok(Visibility::Visible) => return true,
            _ => {}
        }
    }
    true
}

/// The last focused element by root and `id`, to restore after a rebuild.
#[derive(Resource, Default)]
struct FocusMemory {
    root: Option<Entity>,
    id: Option<String>,
}

/// Runs before this frame's rebuilds can despawn the focused element.
fn remember_focus(
    focus: Res<InputFocus>,
    elements: Query<&HtmlElement, With<Focusable>>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<HtmlUi>>,
    mut memory: ResMut<FocusMemory>,
) {
    let Some(entity) = focus.get() else {
        return;
    };
    let Ok(element) = elements.get(entity) else {
        return;
    };
    memory.root = html_root(entity, &parents, &roots);
    memory.id = element.id.clone();
}

/// Keeps `PseudoState`'s focus bits in step with `InputFocus` /
/// `InputFocusVisible` (a change restyles in place).
fn update_focus_states(
    focus: Res<InputFocus>,
    visible: Res<InputFocusVisible>,
    elements: Query<(Entity, Option<&PseudoState>), With<HtmlElement>>,
    added: Query<(), Added<HtmlElement>>,
    mut commands: Commands,
) {
    if !focus.is_changed() && !visible.is_changed() && added.is_empty() {
        return;
    }
    for (entity, state) in &elements {
        let current = state.copied().unwrap_or_default();
        let focused = focus.get() == Some(entity);
        let desired = PseudoState {
            focused,
            focus_visible: focused && visible.0,
            ..current
        };
        if state.copied() != Some(desired) && (state.is_some() || focused) {
            commands.entity(entity).insert(desired);
        }
    }
}

/// An `HtmlUi` root that takes part in focus: entity, modal, stacking.
type ScopeRoot = (Entity, Has<HtmlModal>, Option<&'static GlobalZIndex>);

/// Gives exactly the focusable elements in scope `AutoDirectionalNavigation`.
fn sync_navigation(
    roots_in_scope: Query<ScopeRoot, (With<HtmlUi>, Without<HtmlNoFocus>)>,
    elements: Query<(Entity, Has<AutoDirectionalNavigation>), With<Focusable>>,
    visibility: Query<&Visibility>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<HtmlUi>>,
    mut commands: Commands,
) {
    let visible = |entity| visible_in_hierarchy(entity, &visibility, &parents);
    let modal = roots_in_scope
        .iter()
        .filter(|&(entity, modal, _)| modal && visible(entity))
        .max_by_key(|(_, _, z)| z.map_or(0, |z| z.0))
        .map(|(entity, ..)| entity);
    for (entity, navigable) in &elements {
        let wanted = html_root(entity, &parents, &roots).is_some_and(|root| match modal {
            Some(modal) => root == modal,
            None => roots_in_scope.contains(root) && visible(root),
        });
        if wanted && !navigable {
            commands
                .entity(entity)
                .insert(AutoDirectionalNavigation::default());
        } else if !wanted && navigable {
            commands
                .entity(entity)
                .remove::<AutoDirectionalNavigation>();
        }
    }
}

/// Puts focus back on a focusable element in scope when it isn't on one:
/// the remembered element (same root and `id`), else the `autofocus`
/// element, else the first one. Focus on a non-markup entity is left alone;
/// with nothing focusable in scope, focus on a markup element is cleared.
fn repair_focus(
    mut focus: ResMut<InputFocus>,
    memory: Res<FocusMemory>,
    navigable: Query<(Entity, &HtmlElement, &Focusable), With<AutoDirectionalNavigation>>,
    markup: Query<(), With<Focusable>>,
    entities: Query<()>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<HtmlUi>>,
) {
    let current = focus.get();
    if current.is_some_and(|entity| {
        navigable.contains(entity) || (entities.contains(entity) && !markup.contains(entity))
    }) {
        return;
    }
    let remembered = memory.id.as_deref().and_then(|id| {
        navigable.iter().find_map(|(entity, element, _)| {
            (element.id.as_deref() == Some(id)
                && html_root(entity, &parents, &roots) == memory.root)
                .then_some(entity)
        })
    });
    let target = remembered
        .or_else(|| {
            navigable
                .iter()
                .find_map(|(entity, _, focusable)| focusable.autofocus.then_some(entity))
        })
        .or_else(|| navigable.iter().map(|(entity, ..)| entity).min());
    match target {
        Some(entity) => focus.set(entity, FocusCause::Navigated),
        None if current.is_some() => focus.clear(),
        None => {}
    }
}

/// A pointer press focuses the pressed focusable element (the deepest one
/// under the pointer) and hides the focus ring, like a browser's mousedown.
fn focus_on_press(
    press: On<Pointer<Press>>,
    focusables: Query<(), (With<Focusable>, With<AutoDirectionalNavigation>)>,
    any_focusable: Query<(), With<Focusable>>,
    parents: Query<&ChildOf>,
    mut focus: ResMut<InputFocus>,
    mut visible: ResMut<InputFocusVisible>,
) {
    if !focusables.contains(press.entity) {
        return;
    }
    let deepest = std::iter::once(press.original_event_target())
        .chain(parents.iter_ancestors(press.original_event_target()))
        .find(|&entity| any_focusable.contains(entity));
    if deepest == Some(press.entity) {
        focus.set(press.entity, FocusCause::Pressed);
        visible.0 = false;
    }
}

fn activate_element(
    activate: On<ActivateElement>,
    elements: Query<(&ElementSignals, Option<&HtmlElement>)>,
    mut writer: MessageWriter<ElementSignal>,
) {
    let Ok((signals, element)) = elements.get(activate.entity) else {
        return;
    };
    for binding in signals
        .0
        .iter()
        .filter(|binding| binding.trigger == SignalTrigger::Click)
    {
        writer.write(ElementSignal {
            name: Cow::Owned(binding.name.clone()),
            trigger: SignalTrigger::Click,
            target: activate.entity,
            element: element.cloned().unwrap_or_default(),
            payload: binding.payload.clone(),
            position: None,
        });
    }
}
