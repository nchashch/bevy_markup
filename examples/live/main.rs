//! Live data: a party list rendered from game state every frame and updated
//! in place.
//!
//! - Every frame the app writes the members into the `TemplateContext`
//!   (nothing diffed by hand). bevy_markup re-renders, skips the update when
//!   the HTML is identical, and otherwise reconciles it with the existing
//!   entities: each row is keyed by its `id` (`unit-<name>`), so a member
//!   added at the top or knocked out at the bottom spawns or despawns one
//!   row and leaves the others — their entities, components and state —
//!   alone.
//! - Health bars are `style="width: {{ unit.hp }}%"`, and a knocked-out
//!   member fades with `style="opacity: …"` on its row (CSS group opacity:
//!   text, bar and badge together) before it's removed. The `low` class
//!   comes and goes on the same row entity.
//! - `<div is="badge">` runs [`badge`] once per spawned row: it inserts a
//!   spin start time, so badges spin smoothly — a row that was respawned
//!   instead of updated would jump back — and counts the spawns, shown
//!   bottom-left next to the number of party updates and frames.
//!
//! - Keyboard and gamepad (`examples/shared/input.rs`): the buttons are
//!   `data-on-click` and the rows `tabindex="0"`, so arrows / D-pad / left
//!   stick reach both. Rows are keyed, so a focused row keeps focus while
//!   members join above it; Knock out acts on the focused member.
//!
//! `cargo run --example live` — Recruit (N / X) adds a member at the top,
//! Knock out (K / B) knocks out the focused member (else the last), Language
//! (L / Y) switches between English and German.

use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use bevy_markup::prelude::*;

#[path = "../shared/input.rs"]
mod input;
use input::{ExampleInputPlugin, Hotkeys};

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
                        title: "bevy_markup: live data".into(),
                        ..default()
                    }),
                    ..default()
                }),
            BevyMarkupPlugin,
            ExampleInputPlugin,
        ))
        .define_html_element("badge", badge)
        .add_message::<Action>()
        .insert_resource(ClearColor(Color::srgb_u8(0x10, 0x10, 0x14)))
        .insert_resource(Party::new())
        .init_resource::<Stats>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                ((hotkeys, buttons), act, simulate, show_party, show_stats).chain(),
                spin_badges,
            ),
        )
        .add_observer(count_party_updates)
        .run();
}

const NAMES: [&str; 8] = ["Ada", "Bo", "Cy", "Dee", "Eli", "Fay", "Gus", "Hal"];

struct Member {
    name: &'static str,
    hp: f32,
    /// Health per second; flips at 5 and 100.
    rate: f32,
    /// Knocked out: the row's opacity, fading to 0 before removal.
    fading: Option<f32>,
}

#[derive(Resource)]
struct Party(Vec<Member>);

impl Party {
    fn new() -> Self {
        Self(NAMES[..4].iter().map(|&name| member(name)).collect())
    }
}

/// A new member; health and its rate derive from the name, so the bars
/// start apart.
fn member(name: &'static str) -> Member {
    let seed = name.bytes().map(f32::from).sum::<f32>();
    Member {
        name,
        hp: 20.0 + seed % 80.0,
        rate: if (seed as u32).is_multiple_of(2) {
            9.0
        } else {
            -13.0
        } + seed % 7.0,
        fading: None,
    }
}

/// Party updates (`HtmlUiBuilt` on the party root) and spawned rows
/// (`badge` connections).
#[derive(Resource, Default)]
struct Stats {
    updates: u32,
    spawned: u32,
    frames: u32,
}

#[derive(Component)]
struct PartyUi;

#[derive(Component)]
struct StatsUi;

/// A badge's spin start.
#[derive(Component)]
struct Badge {
    since: f32,
}

/// Both locales, preloaded so switching is immediate.
#[derive(Resource)]
struct Languages(Vec<Handle<BundleAsset>>);

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
    commands.spawn(Camera2d);
    fonts
        .insert("System Serif", FontFaces::new(FontSource::Serif))
        .set_generic(GenericFamily::Serif, "System Serif");
    commands.insert_resource(DefaultStylesheet::new(asset_server.load("live/style.css")));

    let languages: Vec<Handle<BundleAsset>> = ["en-US", "de"]
        .iter()
        .map(|id| asset_server.load(format!("live/locales/{id}/main.ftl.ron")))
        .collect();
    commands.insert_resource(ActiveLocale::new(languages[0].clone()));
    commands.insert_resource(Languages(languages));

    // Placement comes from each template's `<html class>` rule.
    commands.spawn((PartyUi, HtmlUi::new(asset_server.load("live/party.html"))));
    commands.spawn((StatsUi, HtmlUi::new(asset_server.load("live/stats.html"))));
}

/// What the buttons and hotkeys ask for.
#[derive(Message, Clone, Copy, PartialEq)]
enum Action {
    Recruit,
    KnockOut,
    Language,
}

/// N / X, K / B, L / Y.
fn hotkeys(hotkeys: Hotkeys, mut actions: MessageWriter<Action>) {
    for (key, button, action) in [
        (KeyCode::KeyN, GamepadButton::West, Action::Recruit),
        (KeyCode::KeyK, GamepadButton::East, Action::KnockOut),
        (KeyCode::KeyL, GamepadButton::North, Action::Language),
    ] {
        if hotkeys.just_pressed(key, button) {
            actions.write(action);
        }
    }
}

/// The buttons: clicked, or activated with Enter / A.
fn buttons(mut signals: MessageReader<ElementSignal>, mut actions: MessageWriter<Action>) {
    for signal in signals.read().filter(|s| s.trigger == SignalTrigger::Click) {
        let action = match signal.name.as_ref() {
            "recruit" => Action::Recruit,
            "knock-out" => Action::KnockOut,
            "language" => Action::Language,
            _ => continue,
        };
        actions.write(action);
    }
}

/// Recruit: a member joins at the top. Knock out: the focused member (a
/// focused `unit-<name>` row), else the last standing one. Language: next.
fn act(
    mut actions: MessageReader<Action>,
    mut party: ResMut<Party>,
    focus: Res<InputFocus>,
    elements: Query<&HtmlElement>,
    languages: Res<Languages>,
    mut active: ResMut<ActiveLocale>,
) {
    let focused = focus
        .get()
        .and_then(|entity| elements.get(entity).ok())
        .and_then(|element| element.id.as_deref()?.strip_prefix("unit-"));
    for action in actions.read() {
        match action {
            Action::Recruit => {
                if let Some(&name) = NAMES
                    .iter()
                    .find(|name| party.0.iter().all(|member| member.name != **name))
                {
                    party.0.insert(0, member(name));
                }
            }
            Action::KnockOut => {
                let standing = |member: &&mut Member| member.fading.is_none();
                let target = match focused {
                    Some(name) => party
                        .0
                        .iter_mut()
                        .filter(standing)
                        .find(|member| member.name == name),
                    None => party.0.iter_mut().rev().find(standing),
                };
                if let Some(member) = target {
                    member.fading = Some(1.0);
                }
            }
            Action::Language => {
                let current = languages
                    .0
                    .iter()
                    .position(|bundle| active.0.as_ref() == Some(bundle))
                    .unwrap_or(0);
                active.set(languages.0[(current + 1) % languages.0.len()].clone());
            }
        }
    }
}

/// Health drifts between 5 and 100; knocked-out members fade over a second
/// and leave.
fn simulate(time: Res<Time>, mut party: ResMut<Party>) {
    let dt = time.delta_secs();
    for member in &mut party.0 {
        match &mut member.fading {
            Some(opacity) => *opacity -= dt,
            None => {
                member.hp += member.rate * dt;
                if !(5.0..=100.0).contains(&member.hp) {
                    member.rate = -member.rate;
                    member.hp = member.hp.clamp(5.0, 100.0);
                }
            }
        }
    }
    party
        .0
        .retain(|member| member.fading.is_none_or(|opacity| opacity > 0.0));
}

/// The whole party, every frame, rounded to what's visible: bevy_markup
/// updates the UI only when this renders differently, and then in place.
fn show_party(party: Res<Party>, mut uis: Query<&mut TemplateContext, With<PartyUi>>) {
    let units: Vec<serde_json::Value> = party
        .0
        .iter()
        .map(|member| {
            let hp = member.hp.round();
            serde_json::json!({
                "name": member.name,
                "hp": hp,
                "low": hp < 30.0,
                "opacity": (member.fading.unwrap_or(1.0) * 20.0).round() / 20.0,
            })
        })
        .collect();
    for mut context in &mut uis {
        context.insert("units", &units);
    }
}

fn show_stats(mut stats: ResMut<Stats>, mut uis: Query<&mut TemplateContext, With<StatsUi>>) {
    stats.frames += 1;
    for mut context in &mut uis {
        context.insert("updates", &stats.updates);
        context.insert("spawned", &stats.spawned);
        context.insert("frames", &stats.frames);
    }
}

fn count_party_updates(
    built: On<HtmlUiBuilt>,
    party: Query<(), With<PartyUi>>,
    mut stats: ResMut<Stats>,
) {
    if party.contains(built.entity) {
        stats.updates += 1;
    }
}

/// `<div is="badge">`: once per spawned row, never for an updated one.
fn badge(
    badge: In<ElementConnected>,
    time: Res<Time>,
    mut stats: ResMut<Stats>,
    mut commands: Commands,
) {
    stats.spawned += 1;
    commands.entity(badge.entity).insert(Badge {
        since: time.elapsed_secs(),
    });
}

fn spin_badges(time: Res<Time>, mut badges: Query<(&Badge, &mut UiTransform)>) {
    for (badge, mut transform) in &mut badges {
        transform.rotation = Rot2::radians((time.elapsed_secs() - badge.since) * 3.0);
    }
}
