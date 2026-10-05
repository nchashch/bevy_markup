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
//! app.

use bevy::prelude::*;
use bevy::ui_widgets::ScrollArea;
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
        .with(
            "outline_slots",
            &CONTENT
                .iter()
                .filter(|content| content.outline)
                .map(|content| content.slot)
                .collect::<Vec<_>>(),
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
