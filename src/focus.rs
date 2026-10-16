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
//! - **Updates keep focus**: content updates keep matching elements (and
//!   focus on them) in place; when the focused element is replaced, focus
//!   returns to the element with the same `id` in the same `HtmlUi` (give
//!   focusable elements an `id`).
//! - **Scope**: every visible `HtmlUi` takes part, except roots marked
//!   [`HtmlNoFocus`] (e.g. UI rendered onto a 3D quad and driven by a laser).
//!   A visible [`HtmlModal`] root confines focus and navigation to itself
//!   (like a modal `<dialog>`); with several, the topmost wins (highest
//!   `GlobalZIndex`, then `ZIndex` — e.g. the root rule's `z-index`).
//!   An element's scope is its nearest `HtmlUi` ancestor.
//! - **Input is the app's**: bevy_markup reads no keys or buttons. Bind your
//!   own input and call [`HtmlFocus::navigate`] / [`HtmlFocus::activate`].
//!   Activation emits the element's `data-on-click` signals as
//!   [`ElementSignal`] messages, like a pointer click, with the input the
//!   app reports as the source ([`SignalSource::Activation`]: a key, a
//!   gamepad button, or [`ActivationInput::Synthetic`] for harnesses). Navigating off the edge in a direction with no
//!   neighbour triggers [`FocusEdge`] on the focused element (e.g. to page a
//!   list).

use std::borrow::Cow;

use bevy::ecs::system::SystemParam;
use bevy::input_focus::directional_navigation::DirectionalNavigationPlugin;
use bevy::input_focus::{FocusCause, InputFocus, InputFocusVisible};
use bevy::math::CompassOctant;
use bevy::picking::events::PointerPress;
use bevy::prelude::*;
use bevy::ui::UiSystems;
use bevy::ui::auto_directional_navigation::{AutoDirectionalNavigation, AutoDirectionalNavigator};

use crate::HtmlUiSystems;
use crate::html::{HtmlElement, HtmlUi};
use crate::signals::{
    ActivationInput, ActivationParams, ElementSignal, ElementSignals, PseudoState, SignalBinding,
    SignalSource, SignalTrigger,
};

/// A focusable element (see the [module docs](self)). `autofocus` is the
/// HTML attribute.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
#[reflect(Component, Default)]
pub struct Focusable {
    pub autofocus: bool,
}

/// On an `HtmlUi` root: while it's visible, focus and navigation stay inside
/// it. The topmost (`GlobalZIndex`, then `ZIndex`) wins among several.
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
/// as [`ElementSignal`]s, like a pointer click, with
/// [`SignalSource::Activation`]`(input)`. [`HtmlFocus::activate`] triggers
/// it on the focused element.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct ActivateElement {
    pub entity: Entity,
    /// What activated it.
    pub input: ActivationInput,
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

    /// Activates the focused element (see [`ActivateElement`]); `input` is
    /// what the app saw (the signals' [`SignalSource::Activation`]).
    pub fn activate(&mut self, input: ActivationInput) {
        if let Some(entity) = self.navigator.input_focus() {
            self.commands.trigger(ActivateElement { entity, input });
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

/// Directional navigation: focus memory, press-to-focus, activation and
/// the app's `HtmlFocus` API. The navigation plugin itself is added in
/// [`add_navigation_plugin`] (from the crate plugin's `finish`), so an app
/// adding Bevy's [`DirectionalNavigationPlugin`] itself is detected
/// instead of hitting Bevy's duplicate-plugin error.
pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<InputFocus>()
        .init_resource::<InputFocusVisible>()
        .init_resource::<FocusMemory>()
        .add_observer(activate_element)
        .add_observer(focus_on_press)
        .add_systems(
            PostUpdate,
            (
                (remember_focus, update_focus_states)
                    // After the dispatcher: a signal handler that moves
                    // focus shows the new state the same frame.
                    .after(crate::signals::dispatch_signals)
                    .before(HtmlUiSystems::Render),
                (sync_navigation, repair_focus)
                    .chain()
                    .after(HtmlUiSystems::Build)
                    .before(UiSystems::Prepare),
            ),
        );
}

/// Adds Bevy's [`DirectionalNavigationPlugin`] when the app hasn't (its
/// neighbor lookup backs the focus navigation). Called from the crate
/// plugin's `finish`, after every plugin's `build`: an app adding the
/// plugin itself anywhere in its `build` is detected here.
pub(crate) fn add_navigation_plugin(app: &mut App) {
    if !app.is_plugin_added::<DirectionalNavigationPlugin>() {
        app.add_plugins(DirectionalNavigationPlugin);
    }
}

/// The first navigable element in scope, in document order: a depth-first
/// walk of each in-scope root's subtree (the same order the build
/// spawns/updates in). `Entity`-ordered `.min()` picked recycled ids by
/// generation, which could be the page's *last* element.
fn first_focusable_in_scope(
    roots_in_scope: &Query<ScopeRoot, (With<HtmlUi>, Without<HtmlNoFocus>)>,
    navigable: &Query<(Entity, &HtmlElement, &Focusable), With<AutoDirectionalNavigation>>,
    children: &Query<&'static Children>,
    visibility: &Query<&Visibility>,
    parents: &Query<&ChildOf>,
    roots: &Query<(), With<HtmlUi>>,
) -> Option<Entity> {
    let in_scope = |entity: Entity| {
        visible_in_hierarchy(entity, visibility, parents)
            && html_root(entity, parents, roots).is_some_and(|root| {
                active_modal(roots_in_scope, visibility, parents).is_none_or(|modal| root == modal)
            })
    };
    // Nothing is navigable: the walk below (a stack of every in-scope
    // entity) costs nothing to skip, and repair_focus runs it every frame
    // while nothing is focused (HUD-only UIs).
    if navigable.is_empty() {
        return None;
    }
    let mut stack: Vec<Entity> = roots_in_scope
        .iter()
        .map(|(root, ..)| root)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    while let Some(entity) = stack.pop() {
        if navigable.contains(entity) && in_scope(entity) {
            return Some(entity);
        }
        if let Ok(children) = children.get(entity) {
            // Depth-first, document order: children in order, revisited
            // after the current level (hence the reverse push).
            stack.extend(children.iter().rev());
        }
    }
    None
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

/// The visible modal root in scope, if any (Bevy's order of UI roots:
/// `GlobalZIndex`, then `ZIndex` — CSS `z-index` on the root rule).
fn active_modal(
    roots_in_scope: &Query<ScopeRoot, (With<HtmlUi>, Without<HtmlNoFocus>)>,
    visibility: &Query<&Visibility>,
    parents: &Query<&ChildOf>,
) -> Option<Entity> {
    let visible = |entity| visible_in_hierarchy(entity, visibility, parents);
    roots_in_scope
        .iter()
        .filter(|&(entity, modal, ..)| modal && visible(entity))
        .max_by_key(|(_, _, global, local)| (global.map_or(0, |z| z.0), local.map_or(0, |z| z.0)))
        .map(|(entity, ..)| entity)
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
            // `try_insert`, not `insert`: a signal handler that despawned
            // the UI this frame (same frame as a focus change) leaves dead
            // entities here — the bug_0021 class, which missed this
            // system.
            commands.entity(entity).try_insert(desired);
        }
    }
}

/// An `HtmlUi` root that takes part in focus: entity, modal, stacking.
type ScopeRoot = (
    Entity,
    Has<HtmlModal>,
    Option<&'static GlobalZIndex>,
    Option<&'static ZIndex>,
);

/// Gives exactly the focusable elements in scope `AutoDirectionalNavigation`.
#[hotpath::measure]
fn sync_navigation(
    roots_in_scope: Query<ScopeRoot, (With<HtmlUi>, Without<HtmlNoFocus>)>,
    elements: Query<(Entity, Has<AutoDirectionalNavigation>), With<Focusable>>,
    visibility: Query<&Visibility>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<HtmlUi>>,
    mut commands: Commands,
) {
    let visible = |entity| visible_in_hierarchy(entity, &visibility, &parents);
    let modal = active_modal(&roots_in_scope, &visibility, &parents);
    for (entity, navigable) in &elements {
        let wanted = html_root(entity, &parents, &roots).is_some_and(|root| match modal {
            Some(modal) => root == modal,
            None => roots_in_scope.contains(root) && visible(root),
        });
        if wanted && !navigable {
            // `try_`: the app may despawn the UI in the same frame (bug_0021).
            commands
                .entity(entity)
                .try_insert(AutoDirectionalNavigation::default());
        } else if !wanted && navigable {
            commands
                .entity(entity)
                .try_remove::<AutoDirectionalNavigation>();
        }
    }
}

/// Puts focus back on a focusable element in scope when it isn't on one:
/// the remembered element (same root and `id`), else the `autofocus`
/// element, else the first one. Focus on another UI node (the app's own UI)
/// is left alone; focus on something that isn't UI — Bevy's input dispatch
/// focuses the primary window at startup — counts as none (bug_0022). With
/// nothing focusable in scope, focus on a markup element is cleared.
#[hotpath::measure]
fn repair_focus(
    mut focus: ResMut<InputFocus>,
    memory: Res<FocusMemory>,
    navigable: Query<(Entity, &HtmlElement, &Focusable), With<AutoDirectionalNavigation>>,
    markup: Query<(), With<Focusable>>,
    targets: Query<(), With<ElementSignals>>,
    ui_nodes: Query<(), With<Node>>,
    roots_in_scope: Query<ScopeRoot, (With<HtmlUi>, Without<HtmlNoFocus>)>,
    children: Query<&'static Children>,
    visibility: Query<&Visibility>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<HtmlUi>>,
) {
    let current = focus.get();
    // An activation target the app focused itself (a link span, which has
    // bindings but no box to navigate by) keeps the focus — but only in
    // scope: an open modal takes it, like it takes a navigable element's.
    let in_scope = |entity| {
        visible_in_hierarchy(entity, &visibility, &parents)
            && html_root(entity, &parents, &roots).is_some_and(|root| {
                active_modal(&roots_in_scope, &visibility, &parents)
                    .is_none_or(|modal| root == modal)
            })
    };
    if current.is_some_and(|entity| {
        navigable.contains(entity)
            || (ui_nodes.contains(entity) && !markup.contains(entity))
            || (targets.contains(entity) && in_scope(entity))
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
        // The "first focusable element in scope" fallback: document order
        // (a depth-first walk of the in-scope roots), not `Entity` order —
        // recycled ids sort by generation, and the old `.min()` could
        // focus the last element of the page.
        .or_else(|| {
            first_focusable_in_scope(
                &roots_in_scope,
                &navigable,
                &children,
                &visibility,
                &parents,
                &roots,
            )
        });
    match target {
        Some(entity) => focus.set(entity, FocusCause::Navigated),
        None if current.is_some() => focus.clear(),
        None => {}
    }
}

/// A pointer press focuses the pressed focusable element (the deepest one
/// under the pointer) and hides the focus ring, like a browser's mousedown.
fn focus_on_press(
    press: On<PointerPress>,
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

fn activate_element(activate: On<ActivateElement>, mut params: ActivationParams) {
    let target = activate.entity;
    // A submit button's activation is the form's submit (Enter / gamepad A
    // on `<button type="submit">`), with the activation as its source —
    // the same path a click on it takes.
    if params.submits.get(target).is_ok() {
        crate::signals::emit_form(
            target,
            SignalSource::Activation(activate.input),
            &mut params.writer,
            &params.parents,
            &params.elements,
            &params.signals,
            #[cfg(feature = "text-input")]
            &params.children,
            #[cfg(feature = "text-input")]
            &params.editable,
            #[cfg(feature = "text-input")]
            &params.names,
        );
    }
    let Ok((signals, element)) = params.elements_and_state.get(target) else {
        return;
    };
    for binding in signals
        .0
        .iter()
        .filter(|binding| binding.trigger == SignalTrigger::Click)
    {
        params.writer.write(ElementSignal {
            name: Cow::Owned(binding.name.clone()),
            trigger: SignalTrigger::Click,
            target: activate.entity,
            element: element.cloned().unwrap_or_default(),
            payload: binding.payload.clone(),
            source: SignalSource::Activation(activate.input),
        });
    }
}
