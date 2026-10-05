//! Interaction signals end to end: the harness writes `WindowEvent`s (the
//! input winit sends), Bevy's real picking stack runs (`PointerInputPlugin`
//! → UI picking backend → pointer events), and the tests assert the
//! [`ElementSignal`]s and `PseudoState`s bevy_markup derives from it.
//!
//! Needs [`TestUi::with_pointer`] (layout for hit testing, a primary window
//! and a camera targeting it); every pointer step runs one frame, like real
//! input, so clicks span three frames (move, press, release).

mod common;

use bevy::prelude::*;
use bevy_markup::prelude::*;
use common::{TestUi, node_rect};

const VIEWPORT: UVec2 = UVec2::new(640, 480);

/// A page under a full-width column root, the way apps set up their `HtmlUi`.
fn page_ui(name: &str, page: &str, css: &str, context: TemplateContext) -> TestUi {
    TestUi::with_pointer(name, &[("page.html", page), ("style.css", css)], VIEWPORT)
        .stylesheet("style.css")
        .spawn(
            "page.html",
            context,
            Node {
                flex_direction: FlexDirection::Column,
                ..default()
            },
        )
}

fn element_by_id(world: &mut World, root: Entity, id: &str) -> Entity {
    let mut elements = world.query::<(Entity, &HtmlElement)>();
    let candidates: Vec<Entity> = elements
        .iter(world)
        .filter(|(_, element)| element.id.as_deref() == Some(id))
        .map(|(entity, _)| entity)
        .collect();
    candidates
        .into_iter()
        .find(|&entity| {
            std::iter::successors(Some(entity), |&current| {
                world.get::<ChildOf>(current).map(ChildOf::parent)
            })
            .any(|ancestor| ancestor == root)
        })
        .unwrap_or_else(|| panic!("no #{id} below {root}"))
}

/// The element's laid-out center, for pointer moves that must land on it.
fn center(world: &mut World, root: Entity, id: &str) -> (Entity, Vec2) {
    let entity = element_by_id(world, root, id);
    let rect = node_rect(world, entity).expect("laid out");
    (entity, rect.center())
}

fn pseudo(world: &World, entity: Entity) -> PseudoState {
    world
        .get::<PseudoState>(entity)
        .copied()
        .unwrap_or_default()
}

#[test]
fn click_fires_press_release_and_click_with_payload_position_and_element() {
    let page = r#"<div id="btn" data-on-click="buy" data-on-press="down"
                         data-on-release="up" data-with='{ "n": {{ n }} }'><p>Buy</p></div>"#;
    let mut ui = page_ui(
        "signals-click",
        page,
        "",
        TemplateContext::new().with("n", &3),
    );
    ui.settle();
    ui.update(1);
    let (btn, at) = {
        let root = ui.root();
        let world = ui.world_mut();
        center(world, root, "btn")
    };

    // Moving onto the element emits nothing: no enter/leave declared.
    ui.move_pointer(at);
    assert!(
        ui.take_signals().is_empty(),
        "hover emits no pointer signals"
    );

    // Pressing fires the press hook only.
    ui.press_pointer();
    let signals = ui.take_signals();
    assert_eq!(signals.len(), 1, "press");
    let signal = &signals[0];
    assert_eq!(
        (signal.name.as_ref(), signal.trigger),
        ("down", SignalTrigger::Press)
    );
    assert_eq!(signal.target, btn);
    assert_eq!(signal.element.tag, "div");
    assert_eq!(signal.element.id.as_deref(), Some("btn"));
    assert_eq!(signal.payload["n"].as_i64(), Some(3), "data-with rendered");
    assert_eq!(signal.position, Some(at), "pointer position in viewport px");

    // Releasing fires click (first) and release, at the same position.
    ui.release_pointer();
    let signals = ui.take_signals();
    eprintln!("DEBUG release frame: {signals:?}");
    assert_eq!(signals.len(), 2, "click then release");
    assert_eq!(
        (signals[0].name.as_ref(), signals[0].trigger),
        ("buy", SignalTrigger::Click)
    );
    assert_eq!(
        (signals[1].name.as_ref(), signals[1].trigger),
        ("up", SignalTrigger::Release)
    );
    assert_eq!(signals[1].position, Some(at));
}

/// Nested hooks: the deepest bound element under the pointer wins — but only
/// for its own trigger, and only where it actually covers the pointer.
#[test]
fn the_deepest_bound_element_wins_the_click() {
    let page = r#"<div id="outer" data-on-click="outer">
        <div id="inner" data-on-click="inner"><p>In</p></div>
    </div>"#;
    let css = "#outer { padding: 12px }";
    let mut ui = page_ui("signals-nested", page, css, TemplateContext::new());
    ui.settle();
    ui.update(1);
    let root = ui.root();

    // Over the inner element (its text): only the inner hook fires; the
    // outer's observer sees the same event bubble past but is suppressed.
    let (inner, at) = {
        let world = ui.world_mut();
        center(world, root, "inner")
    };
    ui.click_at(at);
    let signals = ui.take_signals();
    assert_eq!(signals.len(), 1, "only the deepest hook fires");
    assert_eq!(
        (signals[0].name.as_ref(), signals[0].target),
        ("inner", inner)
    );

    // Over the outer's own padding: the outer hook fires.
    let (_, at) = {
        let world = ui.world_mut();
        let outer = element_by_id(world, root, "outer");
        let rect = node_rect(world, outer).unwrap();
        (outer, Vec2::new(rect.center().x, rect.max.y - 6.0))
    };
    ui.click_at(at);
    let signals = ui.take_signals();
    assert_eq!(signals.len(), 1, "uncovered area: the outer hook fires");
    assert_eq!(signals[0].name, "outer");
}

/// `enter`/`leave` count for a whole element subtree (like CSS `:hover`),
/// and each bound element on the way enters and leaves.
#[test]
fn enter_and_leave_track_the_hovered_subtree() {
    let page = r#"<div id="parent" data-on-enter="enter-parent" data-on-leave="leave-parent">
        <div id="child" data-on-enter="enter-child" data-on-leave="leave-child"
             data-with='{ "k": "v" }'><p>In</p></div>
    </div>"#;
    let mut ui = page_ui("signals-hover", page, "", TemplateContext::new());
    ui.settle();
    ui.update(1);
    let root = ui.root();
    let (child, at) = {
        let world = ui.world_mut();
        center(world, root, "child")
    };

    // Moving onto the child enters it and its bound ancestor.
    ui.move_pointer(at);
    let signals = ui.take_signals();
    let mut names: Vec<_> = signals.iter().map(|s| s.name.as_ref()).collect();
    names.sort_unstable();
    assert_eq!(names, ["enter-child", "enter-parent"]);
    assert!(
        signals.iter().all(|s| s.position.is_none()),
        "enter has no position"
    );
    let entered_child = signals
        .iter()
        .find(|s| s.name == "enter-child")
        .expect("child enter");
    assert_eq!(entered_child.target, child);
    assert_eq!(entered_child.payload["k"], "v");

    // Moving within the same subtree changes nothing.
    ui.move_pointer(at + Vec2::new(2.0, 0.0));
    assert!(ui.take_signals().is_empty(), "still inside");

    // Moving away leaves every entered element, child first or not
    // (HashMap order): compare as a set.
    let (_, outside) = {
        let world = ui.world_mut();
        let rect = node_rect(world, root).expect("laid out root");
        (root, Vec2::new(rect.center().x, rect.max.y + 40.0))
    };
    ui.move_pointer(outside);
    let signals = ui.take_signals();
    let mut names: Vec<_> = signals.iter().map(|s| s.name.as_ref()).collect();
    names.sort_unstable();
    assert_eq!(names, ["leave-child", "leave-parent"]);
    assert!(signals.iter().all(|s| s.position.is_none()));
}

/// Under a stationary cursor, a content update that keeps the hovered
/// element (new text) emits nothing. One that replaces it (another tag)
/// despawns it: its `leave` still fires, from the snapshot taken at enter —
/// and the new element under the same pointer enters afresh.
#[test]
fn a_replacement_under_the_cursor_emits_the_snapshot_leave() {
    let page = r#"<{{ tag }} id="btn" data-on-enter="enter" data-on-leave="leave"><p>Hi {{ n }}</p></{{ tag }}>"#;
    let mut ui = page_ui(
        "signals-rebuild",
        page,
        "",
        TemplateContext::new().with("n", &1).with("tag", "div"),
    );
    ui.settle();
    let root = ui.root();
    let (btn, at) = {
        let world = ui.world_mut();
        center(world, root, "btn")
    };
    ui.move_pointer(at);
    let signals = ui.take_signals();
    assert_eq!(signals.len(), 1);
    assert_eq!(
        (signals[0].trigger, signals[0].target),
        (SignalTrigger::Enter, btn)
    );

    // New text: updated in place, still hovered, no signals.
    ui.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("n", &2);
    let signals = ui.update_collecting_signals(3);
    assert!(signals.is_empty(), "kept in place: {signals:?}");
    assert_eq!(center(ui.world_mut(), root, "btn").0, btn);

    // Another tag: the element is replaced.
    ui.world_mut()
        .get_mut::<TemplateContext>(root)
        .unwrap()
        .insert("tag", "section");
    let signals = ui.update_collecting_signals(3);

    let leaves: Vec<&ElementSignal> = signals
        .iter()
        .filter(|s| s.trigger == SignalTrigger::Leave)
        .collect();
    assert_eq!(leaves.len(), 1, "the despawned element leaves: {signals:?}");
    assert_eq!((leaves[0].name.as_ref(), leaves[0].target), ("leave", btn));
    assert_eq!(
        leaves[0].element.id.as_deref(),
        Some("btn"),
        "snapshot identity"
    );

    let enters: Vec<&ElementSignal> = signals
        .iter()
        .filter(|s| s.trigger == SignalTrigger::Enter)
        .collect();
    assert_eq!(enters.len(), 1, "the rebuilt element enters: {signals:?}");
    assert_ne!(enters[0].target, btn, "a fresh entity");
}

/// `:hover` counts for the whole subtree; `:active` while the press is down,
/// on the pressed element and its ancestors (like CSS).
#[test]
fn hover_and_press_pseudo_states_track_the_pointer() {
    let page = r#"<div id="btn" data-on-click="x"><p id="label">Hi</p></div>"#;
    let mut ui = page_ui("signals-pseudo", page, "", TemplateContext::new());
    ui.settle();
    ui.update(1);
    let root = ui.root();
    let (btn, at) = {
        let world = ui.world_mut();
        center(world, root, "btn")
    };
    let label = {
        let world = ui.world_mut();
        element_by_id(world, root, "label")
    };

    // The button holds the initial focus (first focusable in scope, see
    // `focus.rs`); the pointer hasn't touched it yet.
    let initial = pseudo(ui.world_mut(), btn);
    assert!(
        !initial.hovered && !initial.active,
        "untouched by the pointer"
    );
    ui.move_pointer(at);
    let world = ui.world_mut();
    assert!(pseudo(world, btn).hovered, "the button hovers");
    assert!(pseudo(world, label).hovered, "its block hovers with it");
    assert!(!pseudo(world, btn).active);

    // Pressing activates the pressed chain; a click hook alone stays quiet
    // until the click.
    ui.press_pointer();
    assert!(ui.take_signals().is_empty(), "click fires on release");
    let world = ui.world_mut();
    assert!(pseudo(world, btn).active);
    assert!(pseudo(world, label).active);

    // Releasing deactivates, hovering stays.
    ui.release_pointer();
    let world = ui.world_mut();
    assert!(!pseudo(world, btn).active);
    assert!(pseudo(world, btn).hovered);

    // Moving off the UI (below the root) unhovers.
    let (_, below) = {
        let world = ui.world_mut();
        let rect = node_rect(world, root).expect("laid out root");
        (root, Vec2::new(rect.center().x, rect.max.y + 40.0))
    };
    ui.move_pointer(below);
    let world = ui.world_mut();
    assert!(!pseudo(world, btn).hovered);
    assert!(!pseudo(world, label).hovered);
}

/// `pointer-events: none` (inherited by the subtree) makes the element and
/// its hooks untouchable; hover passes through to whatever is behind.
#[test]
fn pointer_events_none_silences_an_element() {
    let page = r#"<div id="wrap"><div id="btn" data-on-click="x"><p>Nope</p></div></div>"#;
    let css = "#btn { pointer-events: none }";
    let mut ui = page_ui("signals-events-none", page, css, TemplateContext::new());
    ui.settle();
    ui.update(1);
    let root = ui.root();
    let (btn, at) = {
        let world = ui.world_mut();
        center(world, root, "btn")
    };

    ui.click_at(at);
    assert!(
        ui.take_signals().is_empty(),
        "the ignored element stays silent"
    );
    assert!(!pseudo(ui.world_mut(), btn).hovered, "and is never hovered");
}

/// Hooks on elements that produce no entity (inline, walked-through) never
/// fire — the block they live in isn't bound either.
#[test]
fn hooks_on_elements_without_entities_stay_dead() {
    let page = r#"<p><b data-on-click="x">Hi</b> there</p>"#;
    let mut ui = page_ui("signals-inline", page, "", TemplateContext::new());
    ui.settle();
    ui.update(1);
    let (_, at) = {
        let world = ui.world_mut();
        // The bold "Hi" starts the paragraph's line; click its glyphs.
        let mut elements = world.query::<(Entity, &HtmlElement)>();
        let (p, rect) = elements
            .iter(world)
            .find(|(_, element)| element.tag == "p")
            .map(|(entity, _)| (entity, node_rect(world, entity).unwrap()))
            .expect("paragraph");
        (p, Vec2::new(rect.min.x + 6.0, rect.center().y))
    };
    ui.click_at(at);
    assert!(ui.take_signals().is_empty(), "no entity, no signal");
}
