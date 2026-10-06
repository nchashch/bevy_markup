//! The demo shell: one full-screen [`HtmlUi`] whose document holds every
//! panel (see `assets/ui/content/shell.html`; the looks are the themes'
//! `Demo chrome` rules). Nothing about the layout is hand-built here.
//!
//! The content documents (the rendered inventory and the three DOM outlines)
//! go into the shell's `<div is="content-slot">` viewports: the
//! `content-slot` definition ([`fill_content_slot`]) runs once per spawned
//! slot, adds wheel/trackpad scrolling (`ScrollArea`; the viewports'
//! `overflow-y: scroll` is CSS) and spawns the slot's document. Shell updates
//! (language, active option) keep the slots — and the documents in them;
//! the documents inherit the HTML layout instead of being positioned by the
//! app. The viewports are `tabindex="0"`. Keyboard / gamepad: left / right
//! step focus through the panels and buttons in document order, up / down
//! (and the right stick, PageUp / PageDown) scroll the focused panel or the
//! column around the focused button ([`scroll_focused`]).

use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::ui_widgets::{ScrollArea, ScrollIntoView};
use bevy_markup::prelude::*;
use serde::Serialize;

const SHELL_PATH: &str = "ui/content/shell.html";

/// Marks the demo shell's `HtmlUi` (the wiring observers ignore builds of
/// the content documents).
#[derive(Component)]
pub struct Shell;

/// A content document the shell spawns into its `#slot-*` viewport.
struct Content {
    slot: &'static str,
    template: &'static str,
    /// Show the DOM outline instead of the rendered UI.
    outline: bool,
}

const CONTENT: &[Content] = &[
    Content {
        slot: "slot-inventory",
        template: "ui/content/l10n.html",
        outline: false,
    },
    Content {
        slot: "slot-outline-plain",
        template: "ui/content/test.html",
        outline: true,
    },
    Content {
        slot: "slot-outline-template",
        template: "ui/content/inventory.html",
        outline: true,
    },
    Content {
        slot: "slot-outline-l10n",
        template: "ui/content/l10n.html",
        outline: true,
    },
];

#[derive(Serialize)]
struct Item {
    name: &'static str,
    count: u32,
}

/// Tera variables for the content templates.
fn demo_context() -> TemplateContext {
    TemplateContext::new()
        .with("player", "ada <the brave>")
        .with("hp", &7)
        .with("max_hp", &10)
        .with(
            "items",
            &[
                Item {
                    name: "torch",
                    count: 3,
                },
                Item {
                    name: "rope",
                    count: 1,
                },
                Item {
                    name: "key",
                    count: 0,
                },
            ],
        )
}

/// Tera variables for the shell: the selector rows' labels and the active
/// options (`controls::wire_buttons` re-renders the shell with a new one on
/// every click).
pub fn shell_context(lang_active: usize, theme_active: usize) -> TemplateContext {
    TemplateContext::new()
        .with(
            "langs",
            &crate::controls::LOCALES
                .iter()
                .map(|(_, name)| *name)
                .collect::<Vec<_>>(),
        )
        .with("lang_active", &lang_active)
        .with(
            "themes",
            &crate::controls::THEMES
                .iter()
                .map(|(label, _)| *label)
                .collect::<Vec<_>>(),
        )
        .with("theme_active", &theme_active)
        // The Scrollbars toggle; off hides every scrollbar.
        .with("scrollbars_on", &true)
        .with(
            "outline_slots",
            &CONTENT
                .iter()
                .filter(|content| content.outline)
                .map(|content| content.slot)
                .collect::<Vec<_>>(),
        )
        // Per scrolling area (by `id`): whether its scrollbar shows, and the
        // thumb's `top` / `height` in % of the track ([`update_scrollbars`]).
        .with(
            "scrollbars",
            &SCROLL_AREAS
                .iter()
                .map(|id| (*id, ScrollbarState::default()))
                .collect::<std::collections::BTreeMap<_, _>>(),
        )
}

pub fn spawn(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        HtmlUi::new(asset_server.load(SHELL_PATH)),
        shell_context(0, 0),
        Shell,
        Node {
            // The `HtmlUi` node itself: one full-window column; the
            // document's `.top` and `.bottom` sections do the rest.
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            ..default()
        },
    ));
}

/// Up / down scroll what's focused: the focused viewport, or the scroll
/// area around the focused element (the middle column for its options).
/// Arrows / D-pad / left stick: a tap scrolls one step, holding scrolls
/// smoothly; the right stick scrolls smoothly too; PageUp / PageDown jump by
/// most of the height. Left / right move focus (`ArrowMode::Linear`).
/// Clamped to the content (Bevy clamps only the computed offset, so an
/// unclamped position would overshoot and make the way back feel dead).
#[allow(clippy::too_many_arguments)]
pub fn scroll_focused(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    arrows: crate::input::Arrows,
    focus: Res<bevy::input_focus::InputFocus>,
    parents: Query<&ChildOf>,
    mut areas: Query<(&mut ScrollPosition, &ComputedNode), With<ScrollArea>>,
    mut held: Local<f32>,
) {
    /// One tap's scroll, and the held / stick speed, in px.
    const STEP: f32 = 60.0;
    const SPEED: f32 = 900.0;
    let Some(area) = focus.get().and_then(|focused| {
        std::iter::once(focused)
            .chain(parents.iter_ancestors(focused))
            .find(|entity| areas.contains(*entity))
    }) else {
        return;
    };
    let Ok((mut scroll, node)) = areas.get_mut(area) else {
        return;
    };
    let page = node.size.y * node.inverse_scale_factor * 0.8;
    let mut delta = 0.0;
    if keys.just_pressed(KeyCode::PageDown) {
        delta += page;
    }
    if keys.just_pressed(KeyCode::PageUp) {
        delta -= page;
    }
    // Arrows / D-pad / left stick (up = +1): a step on press, then smooth
    // after a short hold.
    let direction = arrows.axes().y as f32;
    if direction == 0.0 {
        *held = 0.0;
    } else {
        if *held == 0.0 {
            delta -= direction * STEP;
        } else if *held > 0.25 {
            delta -= direction * SPEED * time.delta_secs();
        }
        *held += time.delta_secs();
    }
    // Right stick (up = positive y = scroll up).
    if let Some(y) = gamepads
        .iter()
        .map(|pad| pad.right_stick().y)
        .find(|y| y.abs() > 0.2)
    {
        delta -= y * SPEED * time.delta_secs();
    }
    if delta != 0.0 {
        let max = ((node.content_size.y - node.size.y) * node.inverse_scale_factor).max(0.0);
        scroll.y = (scroll.y + delta).clamp(0.0, max);
    }
}

/// `<div class="scrollbar" is="scrollbar" data-target="<id>">`: a styled
/// scrollbar for the scrolling element with that `id` in the same document.
/// The theme styles the track and its `.thumb` child; [`update_scrollbars`]
/// feeds the thumb's position and the track's visibility to the template;
/// [`drag_thumb`] and [`press_track`] scroll with the pointer.
#[derive(Component)]
pub struct DemoScrollbar {
    root: Entity,
    target: String,
}

pub fn scrollbar(track: In<ElementConnected>, mut commands: Commands) {
    if let Some(target) = track.data("target") {
        commands.entity(track.entity).insert(DemoScrollbar {
            root: track.root,
            target: target.to_owned(),
        });
    }
}

/// A scrollbar's target element.
fn target_of(track: &DemoScrollbar, elements: &HtmlElements) -> Option<Entity> {
    elements.by_id(track.root, &track.target)
}

/// A scroll area's state: position, visible height and content height
/// (logical px).
fn scroll_state(scroll: &ScrollPosition, node: &ComputedNode) -> (f32, f32, f32) {
    let scale = node.inverse_scale_factor;
    (scroll.y, node.size.y * scale, node.content_size.y * scale)
}

/// The ids of the shell's scrolling areas (each has a scrollbar).
const SCROLL_AREAS: [&str; 5] = [
    "slot-inventory",
    "side-column",
    "slot-outline-plain",
    "slot-outline-template",
    "slot-outline-l10n",
];

/// One scrollbar's template values (`scrollbars[<id>]` in the shell).
#[derive(Serialize, Clone, Copy, PartialEq, Default)]
struct ScrollbarState {
    /// The area overflows: the scrollbar shows (else `.hidden`).
    show: bool,
    /// The thumb's `top` and `height`, in % of the track.
    top: f32,
    height: f32,
    /// The thumb is being dragged (`.dragging`: highlighted even when the
    /// pointer leaves it, like a browser's).
    dragging: bool,
}

/// The target `id` of the scrollbar whose thumb is being dragged.
#[derive(Resource, Default)]
pub struct DraggedThumb(Option<String>);

/// The scrollbar of a thumb entity (its parent track), if it is a thumb.
fn scrollbar_of_thumb<'a>(
    entity: Entity,
    elements: &Query<&HtmlElement>,
    parents: &Query<&ChildOf>,
    tracks: &'a Query<(&DemoScrollbar, &ComputedNode)>,
) -> Option<(&'a DemoScrollbar, &'a ComputedNode)> {
    let is_thumb = elements
        .get(entity)
        .is_ok_and(|element| element.classes.iter().any(|class| class == "thumb"));
    parents
        .get(entity)
        .ok()
        .filter(|_| is_thumb)
        .and_then(|parent| tracks.get(parent.parent()).ok())
}

/// A thumb drag starts / ends: remember which, for `.dragging`.
pub fn start_thumb_drag(
    drag: On<Pointer<DragStart>>,
    elements: Query<&HtmlElement>,
    parents: Query<&ChildOf>,
    tracks: Query<(&DemoScrollbar, &ComputedNode)>,
    mut dragged: ResMut<DraggedThumb>,
) {
    if let Some((scrollbar, _)) = scrollbar_of_thumb(drag.entity, &elements, &parents, &tracks) {
        dragged.0 = Some(scrollbar.target.clone());
    }
}

pub fn end_thumb_drag(drag: On<Pointer<DragEnd>>, mut dragged: ResMut<DraggedThumb>) {
    if drag.original_event_target() == drag.entity {
        dragged.0 = None;
    }
}

/// Every scrollbar follows its target, as template values: shown while the
/// content overflows, with a thumb whose height is the visible fraction and
/// whose top is the scrolled fraction of the track (`style="top: …%;
/// height: …%"` in the shell, rounded to 0.1% so the shell only re-renders
/// when a thumb visibly moves). Template values rather than writing `Node`
/// fields: a `:hover` restyle rebuilds an element's `Node` from its CSS, so
/// app-written fields wouldn't survive hovering the thumb.
pub fn update_scrollbars(
    tracks: Query<&DemoScrollbar>,
    html: HtmlElements,
    areas: Query<(&ScrollPosition, &ComputedNode)>,
    mut shells: Query<&mut TemplateContext, With<Shell>>,
    dragged: Res<DraggedThumb>,
) {
    let round = |percent: f32| (percent * 10.0).round() / 10.0;
    // Every area, hidden until measured: the template indexes all of them,
    // also before the scrollbar elements exist (the first render).
    let mut states: std::collections::BTreeMap<String, ScrollbarState> = SCROLL_AREAS
        .iter()
        .map(|id| (id.to_string(), ScrollbarState::default()))
        .collect();
    for scrollbar in &tracks {
        let Some((scroll, visible, content)) = target_of(scrollbar, &html)
            .and_then(|target| areas.get(target).ok())
            .map(|(scroll, node)| scroll_state(scroll, node))
        else {
            continue;
        };
        let state = if content > visible + 0.5 {
            ScrollbarState {
                show: true,
                top: round(scroll.clamp(0.0, content - visible) / content * 100.0),
                height: round((visible / content * 100.0).max(8.0)),
                dragging: dragged.0.as_ref() == Some(&scrollbar.target),
            }
        } else {
            ScrollbarState::default()
        };
        states.insert(scrollbar.target.clone(), state);
    }
    let Ok(value) = serde_json::to_value(&states) else {
        return;
    };
    for mut context in &mut shells {
        let current = context
            .get("scrollbars")
            .and_then(|v| serde_json::to_value(v).ok());
        if current.as_ref() != Some(&value) {
            context.insert("scrollbars", &states);
        }
    }
}

/// Dragging a thumb scrolls its target by the same fraction of the content.
pub fn drag_thumb(
    drag: On<Pointer<Drag>>,
    parents: Query<&ChildOf>,
    tracks: Query<(&DemoScrollbar, &ComputedNode)>,
    elements: Query<&HtmlElement>,
    html: HtmlElements,
    mut areas: Query<(&mut ScrollPosition, &ComputedNode)>,
) {
    let Some((scrollbar, track_node)) =
        scrollbar_of_thumb(drag.entity, &elements, &parents, &tracks)
    else {
        return;
    };
    let Some((mut position, node)) =
        target_of(scrollbar, &html).and_then(|target| areas.get_mut(target).ok())
    else {
        return;
    };
    let (scroll, visible, content) = scroll_state(&position, node);
    let track = track_node.size.y * track_node.inverse_scale_factor;
    if track <= 0.0 {
        return;
    }
    let max = (content - visible).max(0.0);
    let next = (scroll + drag.delta.y * content / track).clamp(0.0, max);
    position.y = next;
}

/// Pressing the track (not the thumb) pages toward the pointer.
pub fn press_track(
    press: On<Pointer<Press>>,
    tracks: Query<(&DemoScrollbar, &ComputedNode, &UiGlobalTransform)>,
    html: HtmlElements,
    mut areas: Query<(&mut ScrollPosition, &ComputedNode)>,
) {
    // Only the press on the track itself, not one bubbling up from its thumb.
    if press.original_event_target() != press.entity || press.button != PointerButton::Primary {
        return;
    }
    let Ok((scrollbar, track_node, transform)) = tracks.get(press.entity) else {
        return;
    };
    let Some((mut position, node)) =
        target_of(scrollbar, &html).and_then(|target| areas.get_mut(target).ok())
    else {
        return;
    };
    let (scroll, visible, content) = scroll_state(&position, node);
    // Where the thumb would be, in the track's logical px, vs the pointer.
    let scale = track_node.inverse_scale_factor;
    let track_top = transform.translation.y * scale - track_node.size.y * scale / 2.0;
    let track = track_node.size.y * scale;
    let thumb_mid = track_top + (scroll + visible / 2.0) / content * track;
    let direction = if press.pointer_location.position.y < thumb_mid {
        -1.0
    } else {
        1.0
    };
    let max = (content - visible).max(0.0);
    let next = (scroll + direction * visible * 0.8).clamp(0.0, max);
    position.y = next;
}

/// `<section is="scroll-column">`: wheel / trackpad scrolling for a column
/// that's taller than the window (`overflow-y: scroll` is CSS).
pub fn scroll_column(column: In<ElementConnected>, mut commands: Commands) {
    commands.entity(column.entity).insert(ScrollArea);
}

/// Keyboard / gamepad focus moved: scroll the newly focused element into
/// view inside its nearest `ScrollArea` (the middle column; a focused
/// viewport sits in a non-scrolling column, where this does nothing). Only
/// while the focus ring shows (`InputFocusVisible`, i.e. after navigating):
/// startup `autofocus` would otherwise scroll by a layout that hasn't
/// settled yet, and a mouse click needs no scrolling.
pub fn scroll_focus_into_view(
    focus: Res<bevy::input_focus::InputFocus>,
    visible: Res<bevy::input_focus::InputFocusVisible>,
    mut last: Local<Option<Entity>>,
    mut commands: Commands,
) {
    if focus.get() == *last {
        return;
    }
    *last = focus.get();
    if let Some(entity) = focus.get()
        && visible.0
    {
        commands.trigger(ScrollIntoView { entity });
    }
}

/// `<div is="content-slot">`: scrolling plus the slot's content document
/// (looked up by the slot's `id` in [`CONTENT`]). Runs once per spawned slot.
pub fn fill_content_slot(
    slot: In<ElementConnected>,
    elements: Query<&HtmlElement>,
    asset_server: Res<AssetServer>,
    mut commands: Commands,
) {
    let id = elements
        .get(slot.entity)
        .ok()
        .and_then(|element| element.id.as_deref());
    let Some(content) = CONTENT.iter().find(|content| Some(content.slot) == id) else {
        return;
    };
    commands
        .entity(slot.entity)
        .insert(ScrollArea)
        .with_children(|slot| {
            let mut document = slot.spawn((
                HtmlUi::new(asset_server.load(content.template)),
                demo_context(),
                Node {
                    // Fill the slot's width so the text wraps like a page.
                    width: Val::Percent(100.0),
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
            ));
            if content.outline {
                document.insert(HtmlDebugOutline);
            }
        });
}
