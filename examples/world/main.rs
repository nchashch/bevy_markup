//! Nameplates over 3D units: UI roots anchored to world points.
//!
//! - Each unit gets a `world/plate.html` root with
//!   `HtmlWorldAnchor::new(unit).with_offset(…)`: bevy_markup projects the
//!   point above the unit through the camera every frame and keeps the
//!   plate's bottom center on it.
//! - The anchor owns the plate's `Visibility`: it hides plates whose unit is
//!   behind the camera, off screen, or invisible (the blinking unit), and
//!   despawns a plate with its unit (K knocks units out).
//! - `HtmlWorldAnchorView` reports each plate's distance and whether it's on
//!   screen: plates fade with distance (`style="opacity: …"`), and the HUD
//!   counts the ones on screen.
//! - Hiding plates for the app's own reasons is CSS, not `Visibility`
//!   (which the anchor owns): Space toggles a `hidden` class.
//! - The plates are ordinary HTML + CSS (`world/plate.html`,
//!   `world/style.css`): a faction class on the root picks a 9-slice
//!   `border-image` frame (hostile), a rounded bordered panel (allies) or an
//!   outlined one (neutral); a monospace level badge, a bold sans-serif name,
//!   an italic serif title line and a three-color health bar. Nearer plates
//!   stack on top through an inline `z-index`.
//! - The level, title and status lines are Fluent messages with markup
//!   (`<b>`, `<i>`, `<span class="{ $faction }">`) and plurals, so L
//!   re-translates every plate.
//! - Name, health, hits and fade are template values written every frame;
//!   a plate only updates when the rendered output changes.
//!
//! - Keyboard and gamepad (`examples/shared/input.rs`): the HUD's buttons
//!   are `data-on-click` (focusable); its root is `pointer-events: none` so
//!   it never blocks the scene, and the button row takes the pointer back
//!   with `pointer-events: auto`. Every action also has a hotkey on both.
//!
//! `cargo run --example world` — Hit nearest (K / B) knocks out the nearest
//! unit, Everyone back (R / X), Plates on/off (Space / Select), Language
//! (L / Y); ← → / D-pad / left stick choose a button, Enter / A press it.

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
                        title: "bevy_markup: world anchors".into(),
                        ..default()
                    }),
                    ..default()
                }),
            BevyMarkupPlugin,
            ExampleInputPlugin,
        ))
        .add_message::<Action>()
        .insert_resource(ClearColor(Color::srgb_u8(0x10, 0x10, 0x14)))
        .insert_resource(PlatesShown(true))
        .add_systems(Startup, (setup, spawn_units).chain())
        .add_systems(
            Update,
            (
                orbit_camera,
                walk,
                blink,
                (hotkeys, buttons, act).chain(),
                show_plates,
                show_hud,
            ),
        )
        .run();
}

/// A unit: who it is, its health (0–100) and hits taken, and the circle it
/// walks.
#[derive(Component)]
struct Unit {
    name: &'static str,
    /// `ally`, `hostile` or `neutral`: the plate's frame and colors.
    faction: &'static str,
    level: u32,
    /// A `world-title` epithet variant.
    epithet: &'static str,
    hp: u32,
    hits: u32,
    radius: f32,
    speed: f32,
    phase: f32,
}

/// The unit that blinks (its `Visibility` toggles), hiding its plate.
#[derive(Component)]
struct Blinking;

/// A nameplate root (its unit is its `HtmlWorldAnchor`'s target).
#[derive(Component)]
struct Plate;

/// The HUD's root.
#[derive(Component)]
struct Hud;

/// Space: plates on or off (a CSS class; the anchor owns `Visibility`).
#[derive(Resource)]
struct PlatesShown(bool);

/// Both locales, preloaded so switching is immediate.
#[derive(Resource)]
struct Languages(Vec<Handle<LocaleBundle>>);

#[derive(Resource)]
struct UnitAssets {
    mesh: Handle<Mesh>,
    /// One per faction: ally, hostile, neutral.
    materials: [Handle<StandardMaterial>; 3],
    plate: Handle<HtmlTemplate>,
}

const FACTIONS: [&str; 3] = ["ally", "hostile", "neutral"];

/// Name, faction, level, epithet, walking circle radius, speed (rad/s),
/// start angle.
const UNITS: [(&str, &str, u32, &str, f32, f32, f32); 6] = [
    ("Ada", "ally", 12, "brave", 3.0, 0.6, 0.0),
    ("Bo", "neutral", 3, "quiet", 5.0, -0.4, 1.0),
    ("Cy", "hostile", 7, "grim", 7.0, 0.3, 2.0),
    ("Dee", "ally", 9, "swift", 9.0, -0.25, 3.0),
    ("Eli", "hostile", 15, "lost", 11.0, 0.2, 4.0),
    ("Fay", "neutral", 5, "wise", 13.0, -0.15, 5.0),
];

/// The plate sits this far above the unit's origin (the cube's top + a bit).
const PLATE_OFFSET: Vec3 = Vec3::new(0.0, 0.8, 0.0);

/// Plates start fading at this camera distance and are gone at the second.
const FADE_START: f32 = 18.0;
const FADE_END: f32 = 30.0;

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut fonts: ResMut<FontFamilies>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    fonts
        .insert("System Serif", FontFaces::new(FontSource::serif()))
        .set_generic(GenericFamily::Serif, "System Serif")
        .insert("System Sans", FontFaces::new(FontSource::sans_serif()))
        .set_generic(GenericFamily::SansSerif, "System Sans")
        .insert("System Mono", FontFaces::new(FontSource::monospace()))
        .set_generic(GenericFamily::Monospace, "System Mono");
    commands.insert_resource(DefaultStylesheet::new(asset_server.load("world/style.css")));
    let languages: Vec<Handle<LocaleBundle>> = ["en-US", "de"]
        .iter()
        .map(|id| asset_server.load(format!("world/locales/{id}/main.ftl.ron")))
        .collect();
    commands.insert_resource(ActiveLocale::new(languages[0].clone()));
    commands.insert_resource(Languages(languages));

    // The 3D camera is also the (default) UI camera the anchors project with.
    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 6.0, 16.0)));
    commands.spawn((
        DirectionalLight {
            illuminance: 6000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 10.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(40.0, 40.0))),
        MeshMaterial3d(materials.add(Color::srgb_u8(0x2a, 0x2d, 0x36))),
    ));
    commands.insert_resource(UnitAssets {
        mesh: meshes.add(Cuboid::new(0.8, 1.2, 0.8)),
        materials: [
            Color::srgb_u8(0x6d, 0xb3, 0xf2),
            Color::srgb_u8(0xe0, 0x5a, 0x47),
            Color::srgb_u8(0xc9, 0xc3, 0xb6),
        ]
        .map(|color| materials.add(color)),
        plate: asset_server.load("world/plate.html"),
    });
    commands.spawn((Hud, HtmlUi::new(asset_server.load("world/hud.html"))));
}

/// Every unit, with its plate. Also R's respawn.
fn spawn_units(mut commands: Commands, assets: Res<UnitAssets>, units: Query<&Unit>) {
    for (index, &(name, faction, level, epithet, radius, speed, phase)) in UNITS.iter().enumerate()
    {
        if units.iter().any(|unit| unit.name == name) {
            continue;
        }
        let unit = Unit {
            name,
            faction,
            level,
            epithet,
            hp: 100,
            hits: 0,
            radius,
            speed,
            phase,
        };
        // Hidden until `show_plates` has the measured distance.
        let context = plate_context(&unit, 0.0, true);
        let material = FACTIONS.iter().position(|f| *f == faction).unwrap_or(2);
        let mut unit = commands.spawn((
            unit,
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(assets.materials[material].clone()),
            Transform::from_xyz(radius * phase.cos(), 0.6, radius * phase.sin()),
        ));
        if index == 2 {
            unit.insert(Blinking);
        }
        let unit = unit.id();
        commands.spawn((
            Plate,
            HtmlUi::new(assets.plate.clone()),
            context,
            HtmlWorldAnchor::new(unit).with_offset(PLATE_OFFSET),
        ));
    }
}

/// The camera circles the field slowly, so units pass behind it and off
/// screen.
fn orbit_camera(time: Res<Time>, mut cameras: Query<&mut Transform, With<Camera3d>>) {
    let angle = time.elapsed_secs() * 0.1;
    for mut transform in &mut cameras {
        *transform = Transform::from_xyz(16.0 * angle.sin(), 6.0, 16.0 * angle.cos())
            .looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y);
    }
}

fn walk(time: Res<Time>, mut units: Query<(&Unit, &mut Transform)>) {
    for (unit, mut transform) in &mut units {
        let angle = unit.phase + time.elapsed_secs() * unit.speed;
        transform.translation =
            Vec3::new(unit.radius * angle.cos(), 0.6, unit.radius * angle.sin());
    }
}

/// The blinking unit is invisible every other 2 s; its plate goes with it.
fn blink(time: Res<Time>, mut units: Query<&mut Visibility, With<Blinking>>) {
    let visible = ((time.elapsed_secs() / 2.0) as u32).is_multiple_of(2);
    for mut visibility in &mut units {
        visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// What the HUD buttons and hotkeys ask for.
#[derive(Message, Clone, Copy)]
enum Action {
    Hit,
    Respawn,
    TogglePlates,
    Language,
}

/// K / B, R / X, Space / Select, L / Y.
fn hotkeys(hotkeys: Hotkeys, mut actions: MessageWriter<Action>) {
    for (key, button, action) in [
        (KeyCode::KeyK, GamepadButton::East, Action::Hit),
        (KeyCode::KeyR, GamepadButton::West, Action::Respawn),
        (KeyCode::Space, GamepadButton::Select, Action::TogglePlates),
        (KeyCode::KeyL, GamepadButton::North, Action::Language),
    ] {
        if hotkeys.just_pressed(key, button) {
            actions.write(action);
        }
    }
}

/// The HUD buttons: clicked, or focused with ← → / D-pad and pressed with
/// Enter / A.
fn buttons(mut signals: MessageReader<ElementSignal>, mut actions: MessageWriter<Action>) {
    for signal in signals.read().filter(|s| s.trigger == SignalTrigger::Click) {
        let action = match signal.name.as_ref() {
            "hit" => Action::Hit,
            "respawn" => Action::Respawn,
            "plates" => Action::TogglePlates,
            "language" => Action::Language,
            _ => continue,
        };
        actions.write(action);
    }
}

/// Hit: the unit nearest the camera loses 35; at 0 it's despawned, and its
/// plate with it. Respawn: everyone back. Plates: on/off. Language: next.
#[allow(clippy::too_many_arguments)]
fn act(
    mut actions: MessageReader<Action>,
    cameras: Query<&Transform, With<Camera3d>>,
    mut units: Query<(Entity, &mut Unit, &Transform)>,
    mut shown: ResMut<PlatesShown>,
    languages: Res<Languages>,
    mut active: ResMut<ActiveLocale>,
    mut commands: Commands,
) {
    for action in actions.read() {
        match action {
            Action::Respawn => commands.run_system_cached(spawn_units),
            Action::TogglePlates => shown.0 = !shown.0,
            Action::Language => {
                let current = languages
                    .0
                    .iter()
                    .position(|bundle| active.0.as_ref() == Some(bundle))
                    .unwrap_or(0);
                active.set(languages.0[(current + 1) % languages.0.len()].clone());
            }
            Action::Hit => {
                let Ok(camera) = cameras.single() else {
                    continue;
                };
                let nearest = units.iter_mut().min_by(|(_, _, a), (_, _, b)| {
                    let distance = |t: &Transform| t.translation.distance(camera.translation);
                    distance(a).total_cmp(&distance(b))
                });
                if let Some((entity, mut unit, _)) = nearest {
                    unit.hp = unit.hp.saturating_sub(35);
                    unit.hits += 1;
                    if unit.hp == 0 {
                        commands.entity(entity).despawn();
                    }
                }
            }
        }
    }
}

/// Name, health and fade for every plate, from its unit and the distance the
/// anchor measured.
fn show_plates(
    shown: Res<PlatesShown>,
    units: Query<&Unit>,
    mut plates: Query<(&HtmlWorldAnchor, &HtmlWorldAnchorView, &mut TemplateContext), With<Plate>>,
) {
    for (anchor, view, mut context) in &mut plates {
        let Ok(unit) = units.get(anchor.target) else {
            continue;
        };
        let fade = ((view.distance - FADE_START) / (FADE_END - FADE_START)).clamp(0.0, 1.0);
        // Rounded to what's visible, so a plate only updates when it changes.
        let alpha = ((1.0 - fade) * 100.0).round() / 100.0;
        let mut plate = plate_context(unit, alpha, !shown.0 || alpha <= 0.0);
        // Nearer plates on top: `z-index` orders UI roots like `ZIndex`.
        plate.insert("z", &(1000 - (view.distance * 10.0) as i32));
        *context = plate;
    }
}

/// `world/plate.html`'s variables for `unit`.
fn plate_context(unit: &Unit, alpha: f32, hidden: bool) -> TemplateContext {
    let health = match unit.hp {
        61.. => "good",
        31..=60 => "hurt",
        _ => "low",
    };
    TemplateContext::new()
        .with("name", unit.name)
        .with("faction", unit.faction)
        .with("level", &unit.level)
        .with("epithet", unit.epithet)
        .with("hp", &unit.hp)
        .with("hits", &unit.hits)
        .with("health", health)
        .with("alpha", &alpha)
        .with("hidden", &hidden)
        .with("z", &0)
}

/// The HUD counts the plates whose point is on screen.
fn show_hud(
    plates: Query<&HtmlWorldAnchorView, With<Plate>>,
    mut huds: Query<&mut TemplateContext, With<Hud>>,
) {
    let on_screen = plates.iter().filter(|view| view.on_screen).count();
    for mut context in &mut huds {
        context.insert("on_screen", &on_screen);
        context.insert("total", &plates.iter().count());
    }
}
