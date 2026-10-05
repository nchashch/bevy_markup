//! Declarative interaction signals: elements declare hooks in the HTML, and
//! the library turns interactions into buffered [`ElementSignal`]s.
//!
//! ```html
//! <div class="opt" data-on-click="select-language"
//!      data-with='{ "index": {{ loop.index0 }} }'>English</div>
//! ```
//!
//! - **`data-on-click` / `-press` / `-release` / `-enter` / `-leave`** name
//!   the signal: the value is an app-side constant, sent as
//!   [`ElementSignal::name`]. Unknown `data-on-*` attributes are skipped
//!   (logged at `debug`), as are hooks on elements that produce no entity
//!   (`html`, `body`, unknown tags outside blocks; `button` is not a
//!   container — use `div`).
//! - **`data-with`** is a JSON object rendered by Tera with the template's
//!   context, like `data-l10n-args`: a snapshot of the data at render time.
//!   Numbers stay numbers; invalid JSON becomes `null` (logged at `debug`).
//!   It applies to all of the element's hooks; absent means [`Value::Null`].
//!
//! Signals arrive as one buffered message type; drain it with
//! `MessageReader<ElementSignal>` wherever the app likes (usually an
//! `Update` system). Nested bound elements: the deepest one under the
//! pointer wins — a parent's hook is suppressed when the interaction hit a
//! bound child — so boxes can nest buttons. Hover (`enter`/`leave`) counts
//! for a whole element subtree, like CSS `:hover`.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use bevy::picking::events::{Click, Pointer, Press, Release};
use bevy::picking::hover::{HoverMap, PickingInteraction};
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use serde_json::Value;

use crate::html::HtmlElement;
use crate::template::decode_entities;

/// Which interaction emits a signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalTrigger {
    Click,
    Press,
    Release,
    Enter,
    Leave,
}

/// One declared hook: the signal name and its rendered payload.
#[derive(Clone, Debug, PartialEq)]
pub struct SignalBinding {
    pub trigger: SignalTrigger,
    pub name: String,
    pub payload: Value,
}

/// The `data-on-*` hooks of one element.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ElementSignals(pub Vec<SignalBinding>);

/// A UI interaction on an element that declares a `data-on-*` hook.
#[derive(Message, Clone, Debug)]
pub struct ElementSignal {
    /// The `data-on-*` value, an app-side constant.
    pub name: Cow<'static, str>,
    /// The trigger that fired.
    pub trigger: SignalTrigger,
    /// The bound element.
    pub target: Entity,
    /// The element's `tag`/`id`/`classes`.
    pub element: HtmlElement,
    /// The rendered `data-with`, or `null`.
    pub payload: Value,
    /// The pointer position in viewport px, for the pointer triggers.
    pub position: Option<Vec2>,
}

const TRIGGERS: [(&str, SignalTrigger); 5] = [
    ("data-on-click", SignalTrigger::Click),
    ("data-on-press", SignalTrigger::Press),
    ("data-on-release", SignalTrigger::Release),
    ("data-on-enter", SignalTrigger::Enter),
    ("data-on-leave", SignalTrigger::Leave),
];

/// The `data-on-*`/`data-with` attributes of `tag`, parsed.
pub(crate) fn signal_bindings(tag: &tl::HTMLTag) -> Vec<SignalBinding> {
    let attributes = tag.attributes();
    let mut bindings = Vec::new();
    let mut payload = Value::Null;
    let mut payload_parsed = false;
    for (attribute, trigger) in TRIGGERS {
        let Some(Some(value)) = attributes.get(attribute) else {
            continue;
        };
        let name = decode_entities(&value.as_utf8_str());
        if name.trim().is_empty() {
            debug!("html signals: empty {attribute} value; skipped");
            continue;
        }
        if !payload_parsed {
            payload = match attributes.get("data-with").flatten() {
                Some(value) => {
                    let json = decode_entities(&value.as_utf8_str());
                    match serde_json::from_str(&json) {
                        Ok(payload) => payload,
                        Err(error) => {
                            debug!("html signals: invalid data-with JSON ({error}): {json:?}");
                            Value::Null
                        }
                    }
                }
                None => Value::Null,
            };
            payload_parsed = true;
        }
        bindings.push(SignalBinding {
            trigger,
            name,
            payload: payload.clone(),
        });
    }
    bindings
}

/// Attaches the pointer hooks of `spec.signals` to `entity` (`enter`/`leave`
/// are handled by [`hover_signals`] instead: pointer `Over`/`Out` events
/// don't reach ancestors — the hover map holds only the deepest picked
/// node, since nodes block picking by default).
pub(crate) fn attach_pointer_signals(entity: &mut EntityCommands, signals: &ElementSignals) {
    macro_rules! on_pointer {
        ($event:ty, $trigger:expr, $binding:expr) => {{
            let binding = $binding.clone();
            entity.observe(
                move |trigger: On<Pointer<$event>>,
                      writer: MessageWriter<ElementSignal>,
                      signals: Query<&ElementSignals>,
                      parents: Query<&ChildOf>,
                      elements: Query<&HtmlElement>| {
                    pointer_signal::<$event>(
                        &binding,
                        trigger.original_event_target(),
                        trigger.entity,
                        trigger.pointer_location.position,
                        $trigger,
                        writer,
                        &signals,
                        &parents,
                        &elements,
                    );
                },
            );
        }};
    }
    for binding in &signals.0 {
        let binding = std::sync::Arc::new(binding.clone());
        match binding.trigger {
            SignalTrigger::Click => on_pointer!(Click, binding.trigger, binding),
            SignalTrigger::Press => on_pointer!(Press, binding.trigger, binding),
            SignalTrigger::Release => on_pointer!(Release, binding.trigger, binding),
            // Handled by `hover_signals`: pointer `Over`/`Out` events don't
            // reach ancestors — the hover map holds only the deepest picked
            // node, since nodes block picking by default.
            SignalTrigger::Enter | SignalTrigger::Leave => {}
        }
    }
}

/// Emits `binding` for a pointer trigger, unless a deeper bound element owns
/// the interaction (nested hooks: the deepest wins).
#[allow(clippy::too_many_arguments)]
fn pointer_signal<E>(
    binding: &SignalBinding,
    hit: Entity,
    target: Entity,
    position: Vec2,
    trigger: SignalTrigger,
    mut writer: MessageWriter<ElementSignal>,
    signals: &Query<&ElementSignals>,
    parents: &Query<&ChildOf>,
    elements: &Query<&HtmlElement>,
) where
    Pointer<E>: Message,
    E: std::fmt::Debug + Clone + Reflect,
{
    if covered_by_deeper(hit, target, trigger, signals, parents) {
        return;
    }
    writer.write(ElementSignal {
        name: Cow::Owned(binding.name.clone()),
        trigger,
        target,
        element: elements.get(target).cloned().unwrap_or_default(),
        payload: binding.payload.clone(),
        position: Some(position),
    });
}

/// Whether the interaction hit a bound element between `hit` and `target`
/// (excluding `target`, which is checked by its own invocation): that
/// element is deeper and owns the interaction. `false` when `target` isn't
/// an ancestor of `hit` at all.
fn covered_by_deeper(
    hit: Entity,
    target: Entity,
    trigger: SignalTrigger,
    signals: &Query<&ElementSignals>,
    parents: &Query<&ChildOf>,
) -> bool {
    let mut current = hit;
    let mut deeper = false;
    loop {
        if current == target {
            return deeper;
        }
        deeper |= signals
            .get(current)
            .is_ok_and(|signals| signals.0.iter().any(|binding| binding.trigger == trigger));
        match parents.get(current) {
            Ok(parent) => current = parent.0,
            // `target` isn't an ancestor of `hit`; don't suppress.
            Err(_) => return false,
        }
    }
}

/// Tracks pointers over `data-on-enter`/`-leave` elements and emits their
/// signals. An element counts as hovered while the pointer is over it *or
/// any descendant* (like CSS `:hover`): the hover map only holds the
/// deepest picked node, so ancestors are matched by hand.
///
/// A `leave` must fire even when the element was despawned in the meantime
/// (a rebuild under a stationary cursor), so entering snapshots the
/// element's `leave` bindings and identity for the emit.
pub(crate) fn hover_signals(
    hover_map: Option<Res<HoverMap>>,
    mut entered: Local<HashMap<PointerId, HashMap<Entity, Hovering>>>,
    bound: Query<(Entity, &ElementSignals)>,
    parents: Query<&ChildOf>,
    elements: Query<&HtmlElement>,
    mut writer: MessageWriter<ElementSignal>,
) {
    let Some(hover_map) = hover_map else {
        // No picking plugins (headless tests).
        return;
    };

    // Where the pointers are, in terms of bound elements: each hovered node
    // plus its ancestors.
    let mut now: HashMap<PointerId, HashSet<Entity>> = HashMap::new();
    for (pointer, hovered) in hover_map.iter() {
        let set = now.entry(*pointer).or_default();
        for entity in hovered.keys() {
            let mut current = Some(*entity);
            while let Some(node) = current {
                if let Ok((_, signals)) = bound.get(node)
                    && signals.0.iter().any(|binding| {
                        matches!(binding.trigger, SignalTrigger::Enter | SignalTrigger::Leave)
                    })
                {
                    set.insert(node);
                }
                current = parents.get(node).ok().map(|parent| parent.0);
            }
        }
    }

    for (pointer, set) in &now {
        for &entity in set {
            if entered
                .get(pointer)
                .is_some_and(|previous| previous.contains_key(&entity))
            {
                continue;
            }
            // Entering: remember the `leave` side for the (possible) emit.
            let Ok((_, signals)) = bound.get(entity) else {
                continue;
            };
            let leaves = signals
                .0
                .iter()
                .filter(|binding| binding.trigger == SignalTrigger::Leave)
                .cloned()
                .collect::<Vec<_>>();
            let element = elements.get(entity).cloned().unwrap_or_default();
            for binding in &signals.0 {
                if binding.trigger == SignalTrigger::Enter {
                    writer.write(ElementSignal {
                        name: Cow::Owned(binding.name.clone()),
                        trigger: SignalTrigger::Enter,
                        target: entity,
                        element: element.clone(),
                        payload: binding.payload.clone(),
                        position: None,
                    });
                }
            }
            entered
                .entry(*pointer)
                .or_default()
                .insert(entity, Hovering { element, leaves });
        }
    }
    for (pointer, previous) in entered.iter_mut() {
        let Some(set) = now.get(pointer) else {
            continue;
        };
        for entity in previous.keys().copied().collect::<Vec<_>>() {
            if set.contains(&entity) {
                continue;
            }
            // Leaving (or despawned by a rebuild): emit from the snapshot.
            let hovering = previous.remove(&entity).expect("keyed");
            for binding in &hovering.leaves {
                writer.write(ElementSignal {
                    name: Cow::Owned(binding.name.clone()),
                    trigger: SignalTrigger::Leave,
                    target: entity,
                    element: hovering.element.clone(),
                    payload: binding.payload.clone(),
                    position: None,
                });
            }
        }
    }
}

/// What a hovering pointer remembers about one entered element.
pub(crate) struct Hovering {
    element: HtmlElement,
    leaves: Vec<SignalBinding>,
}

/// The interaction pseudo-state of an element: `:hover` / `:active` /
/// `:focus` / `:focus-visible` for the cascade. Hover and active are
/// maintained from picking, focus from `InputFocus` (see [`crate::focus`]);
/// a change restyles the element's UI (in place). Apps may also set it to
/// force the styles.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PseudoState {
    pub hovered: bool,
    pub active: bool,
    pub focused: bool,
    pub focus_visible: bool,
}

/// Maintains [`PseudoState`] on every element from the picking hover map
/// and the pressed entities: an element is hovered while the pointer is
/// over it or any descendant, and active while a press started on it or a
/// descendant (like CSS).
pub(crate) fn update_pseudo_states(
    hover_map: Option<Res<HoverMap>>,
    interactions: Query<(Entity, &PickingInteraction)>,
    elements: Query<(Entity, &HtmlElement, Option<&PseudoState>)>,
    parents: Query<&ChildOf>,
    mut commands: Commands,
) {
    let Some(hover_map) = hover_map else {
        // No picking plugins (headless tests).
        return;
    };
    let chain = |root: Entity, set: &mut HashSet<Entity>| {
        let mut current = Some(root);
        while let Some(node) = current {
            set.insert(node);
            current = parents.get(node).ok().map(|parent| parent.0);
        }
    };
    let mut hovered = HashSet::new();
    for hovered_entities in hover_map.values() {
        for entity in hovered_entities.keys() {
            chain(*entity, &mut hovered);
        }
    }
    let mut active = HashSet::new();
    for (entity, interaction) in &interactions {
        if *interaction == PickingInteraction::Pressed {
            chain(entity, &mut active);
        }
    }
    for (entity, _, state) in &elements {
        let current = state.copied().unwrap_or_default();
        let desired = PseudoState {
            hovered: hovered.contains(&entity),
            active: active.contains(&entity),
            ..current
        };
        if state.copied() != Some(desired) {
            // `try_`: an app system may despawn the UI this frame, its
            // commands applied before these (bug_0021).
            commands.entity(entity).try_insert(desired);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    /// bug_0021: an app system despawning a UI in the same frame (its
    /// command buffer applied first) must not make the queued `PseudoState`
    /// insert panic.
    #[test]
    fn pseudo_states_tolerate_a_same_frame_despawn() {
        let mut app = App::new();
        app.init_resource::<HoverMap>();
        let element = app.world_mut().spawn(HtmlElement::default()).id();
        app.add_systems(
            Update,
            (
                move |mut commands: Commands| commands.entity(element).despawn(),
                update_pseudo_states,
            )
                .chain_ignore_deferred(),
        );
        app.update();
        assert!(app.world().get_entity(element).is_err());
    }
    use tl::parse;

    fn bindings(html: &str) -> Vec<SignalBinding> {
        let dom = parse(html, tl::ParserOptions::default()).expect("parse");
        let tag = dom.children().iter().find_map(|handle| {
            handle.get(dom.parser()).and_then(|node| match node {
                tl::Node::Tag(tag) => Some(tag),
                _ => None,
            })
        });
        signal_bindings(tag.expect("tag"))
    }

    #[test]
    fn hooks_parse_with_payloads() {
        let bindings = bindings(
            r#"<div data-on-click="buy" data-on-enter="peek"
                     data-with='{ "n": 3, "id": "torch" }'></div>"#,
        );
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].trigger, SignalTrigger::Click);
        assert_eq!(bindings[0].name, "buy");
        assert_eq!(bindings[0].payload["n"], 3);
        assert_eq!(bindings[0].payload["id"], "torch");
        assert_eq!(bindings[1].trigger, SignalTrigger::Enter);
        assert_eq!(bindings[1].payload["id"], "torch");
    }

    #[test]
    fn absent_or_invalid_payload_is_null() {
        let parsed = bindings(r#"<div data-on-leave="unpeek"></div>"#);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].trigger, SignalTrigger::Leave);
        assert_eq!(parsed[0].payload, Value::Null);

        let parsed = bindings(r#"<div data-on-click="x" data-with="{oops}"></div>"#);
        assert_eq!(parsed[0].payload, Value::Null);
    }

    #[test]
    fn unknown_triggers_and_empty_names_are_skipped() {
        let parsed =
            bindings(r#"<div data-on-drag="x" data-on-click="" data-on-press="  "></div>"#);
        assert!(parsed.is_empty());
    }

    #[test]
    fn entities_are_decoded_in_names_and_payloads() {
        let parsed =
            bindings(r#"<div data-on-click="a &amp; b" data-with='{"s": "x &lt; y"}'></div>"#);
        assert_eq!(parsed[0].name, "a & b");
        assert_eq!(parsed[0].payload["s"], "x < y");
    }

    /// A deeper bound element suppresses its ancestors, and only for its own
    /// trigger; an unrelated hit doesn't suppress.
    #[test]
    fn deeper_bound_element_wins() {
        let mut world = World::new();
        let binding = |trigger: SignalTrigger| SignalBinding {
            trigger,
            name: "x".into(),
            payload: Value::Null,
        };
        // parent ← middle ← hit; only `hit` (the deepest node) is bound for
        // click, `parent` for click and enter.
        let parent = world
            .spawn(ElementSignals(vec![
                binding(SignalTrigger::Click),
                binding(SignalTrigger::Enter),
            ]))
            .id();
        let middle = world.spawn_empty().id();
        world.entity_mut(middle).insert(ChildOf(parent));
        let hit = world
            .spawn(ElementSignals(vec![binding(SignalTrigger::Click)]))
            .insert(ChildOf(middle))
            .id();
        let unrelated = world.spawn_empty().id();

        let (click, enter, outside) = world
            .run_system_once(
                move |signals: Query<&ElementSignals>, parents: Query<&ChildOf>| {
                    (
                        covered_by_deeper(hit, parent, SignalTrigger::Click, &signals, &parents),
                        covered_by_deeper(hit, parent, SignalTrigger::Enter, &signals, &parents),
                        covered_by_deeper(hit, unrelated, SignalTrigger::Click, &signals, &parents),
                    )
                },
            )
            .unwrap();
        // The child is bound for click: the parent's click is suppressed,
        // its enter isn't. An unrelated hit suppresses nothing.
        assert!(click);
        assert!(!enter);
        assert!(!outside);
    }
}
