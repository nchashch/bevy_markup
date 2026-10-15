//! An interactive torture bench for performance, security and correctness:
//! pick a scenario, change its size, toggle the per-frame churn and watch
//! the frame time, the update rate and the checks.
//!
//! - **Performance:** large and churning documents, all-dirty rebuilds,
//!   restyle storms, indented Markdown, localized rows (cached and
//!   per-value Fluent paths), many UIs, `calc()` layout.
//! - **Security:** pathological inputs that once crashed, hung or injected
//!   (nested CSS rules, a 5 000-deep DOM, `var()` and grid bombs, hostile
//!   Fluent arguments, args-JSON injection, remote asset sources, Markdown
//!   bombs). Each must survive, with a check showing why.
//! - **Correctness:** a UI mutated at random every frame is compared with a
//!   fresh build of the same state, and an indented Markdown template with
//!   its flat twin.
//!
//! The workloads are generated at runtime into an in-memory asset source
//! (`testbench://`), so they go through the real loaders, pre-scans and
//! all, in the real Bevy frame loop. With `--features hotpath` every
//! pipeline system and the bench's own work is timed per call; run
//! `hotpath console` in another terminal for the live view, or read the
//! report printed on exit.
//!
//! `cargo run --release --example testbench --features hotpath` — see the
//! README for the controls and the scripted form
//! (`-- --scenario big-document --size 2000 --no-render --exit-after 600`).

use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::OnceLock;

use bevy::asset::io::memory::{Dir, MemoryAssetReader};
use bevy::asset::io::{AssetSourceBuilder, ErasedAssetReader};
use bevy::diagnostic::FrameCount;
use bevy::prelude::*;
use bevy_markup::prelude::*;
use serde_json::{Value, json};

#[path = "../shared/input.rs"]
mod input;
use input::{Hotkeys, SettingStep};

#[path = "../shared/harness.rs"]
mod harness;

mod scenarios;
use scenarios::{Changed, Model, Role, SIZES};

/// The in-memory files behind `testbench://` (shared with the asset
/// source's reader, so files written at runtime are loadable at once).
static GENERATED: OnceLock<Dir> = OnceLock::new();

fn generated() -> &'static Dir {
    GENERATED.get_or_init(|| Dir::new(PathBuf::new()))
}

/// Registers `testbench://` — before `AssetPlugin`, as Bevy requires.
fn register_generated_source(app: &mut App) {
    let dir = generated().clone();
    app.register_asset_source(
        "testbench",
        AssetSourceBuilder::new(move || -> Box<dyn ErasedAssetReader> {
            Box::new(MemoryAssetReader { root: dir.clone() })
        }),
    );
}

#[hotpath::main]
fn main() {
    harness::run(
        harness::ExampleWindow {
            title: "bevy_markup: testbench",
            resolution: (1600, 900),
            before_plugins: register_generated_source,
            ..Default::default()
        },
        |app| {
            app.insert_resource(Bench::from_args())
                .init_resource::<Stage>()
                .init_resource::<Stats>()
                .init_resource::<Checks>()
                .init_resource::<Twins>()
                .init_resource::<Built>()
                .define_html_element("stage", connect_stage)
                .add_observer(count_builds)
                .add_observer(count_restyles)
                .add_systems(Startup, setup)
                .add_systems(Last, summarize_on_exit)
                .add_systems(
                    Update,
                    (
                        handle_signals,
                        apply_setting_steps,
                        hotkeys,
                        start_scenario,
                        churn,
                        compare_twins,
                        check_scenario,
                        update_stats,
                        show_shell,
                    )
                        .chain(),
                );
        },
    );
}

/// What the bench runs and the model its workloads render.
#[derive(Resource)]
struct Bench {
    scenarios: Vec<&'static str>,
    active: usize,
    /// Index into [`SIZES`].
    size: usize,
    churn: bool,
    /// `--scenario`: the scenario is fixed (scripted runs; the injected
    /// keys of `--exit-after` must not switch it).
    pinned: bool,
    /// A start (or restart) is due; it waits for the stage to exist.
    restart: bool,
    generation: u32,
    model: Model,
    /// Churn held while two UIs are compared.
    paused: bool,
    /// The frame of the current start.
    started: u32,
    /// The worst frame time since the start, past the warm-up.
    worst_ms: f32,
    /// The scenario's UIs sit side by side (the stage's `row` class).
    row: bool,
}

impl Bench {
    fn from_args() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let value = |flag: &str| {
            args.iter()
                .position(|arg| arg == flag)
                .and_then(|index| args.get(index + 1))
                .cloned()
        };
        let scenarios = scenarios::all();
        let pinned = value("--scenario");
        let active = match &pinned {
            Some(id) => scenarios.iter().position(|s| s == id).unwrap_or_else(|| {
                warn!("testbench: unknown --scenario {id}; known: {scenarios:?}");
                0
            }),
            None => 0,
        };
        let size = value("--size")
            .and_then(|size| size.parse::<usize>().ok())
            .map_or(4, |size| {
                SIZES.iter().position(|step| *step >= size).unwrap_or(SIZES.len() - 1)
            });
        Self {
            scenarios,
            active,
            size,
            churn: !args.iter().any(|arg| arg == "--no-churn"),
            pinned: pinned.is_some(),
            restart: false,
            generation: 0,
            model: Model::default(),
            paused: false,
            started: 0,
            worst_ms: 0.0,
            row: false,
        }
    }

    fn id(&self) -> &'static str {
        self.scenarios[self.active]
    }

    fn select(&mut self, index: usize) {
        if !self.pinned {
            self.active = index % self.scenarios.len();
            self.restart = true;
        }
    }

    fn step_size(&mut self, steps: i32) {
        let size = (self.size as i32 + steps).clamp(0, SIZES.len() as i32 - 1) as usize;
        if size != self.size {
            self.size = size;
            self.restart = true;
        }
    }
}

/// The shell's stage element (`is="stage"`): the workloads' parent.
#[derive(Resource, Default)]
struct Stage(Option<Entity>);

#[derive(Component)]
struct ShellUi;

/// A workload UI: what it is for and what it loaded.
#[derive(Component)]
struct Workload {
    role: Role,
    template: Handle<HtmlTemplate>,
    sheet: Option<Handle<Stylesheet>>,
}

/// A fresh build spawned only to be compared, then despawned.
#[derive(Component)]
struct FreshTwin;

/// The frame each UI last built or restyled.
#[derive(Resource, Default)]
struct Built(HashMap<Entity, u32>);

#[derive(Resource)]
struct Stats {
    frame_ms: VecDeque<f32>,
    builds: u32,
    restyles: u32,
    builds_rate: u32,
    restyles_rate: u32,
    window_start: f32,
    /// When the shell next shows the numbers (a few times a second: the
    /// shell's own re-render is in the frame time too).
    shell_at: f32,
    /// Show now (a control changed).
    shell_now: bool,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            frame_ms: VecDeque::with_capacity(120),
            builds: 0,
            restyles: 0,
            builds_rate: 0,
            restyles_rate: 0,
            window_start: 0.0,
            shell_at: 0.0,
            shell_now: true,
        }
    }
}

struct Check {
    id: &'static str,
    state: &'static str,
    detail: String,
}

#[derive(Resource, Default)]
struct Checks(Vec<Check>);

/// The twin comparison in progress, and its tally.
#[derive(Resource, Default)]
struct Twins {
    pending: Option<Pending>,
    matches: u32,
    mismatches: u32,
    last_mismatch: String,
}

struct Pending {
    main: Entity,
    twin: Entity,
    /// Spawned for this comparison (despawned after it).
    fresh: bool,
    since: u32,
}

/// Both locales, preloaded.
#[derive(Resource)]
struct Languages(Vec<Handle<LocaleBundle>>);

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut fonts: ResMut<FontFamilies>) {
    commands.spawn(Camera2d);
    fonts
        .insert("System Sans", FontFaces::new(FontSource::sans_serif()))
        .insert("System Mono", FontFaces::new(FontSource::monospace()))
        .set_generic(GenericFamily::SansSerif, "System Sans")
        .set_generic(GenericFamily::Monospace, "System Mono");
    let languages: Vec<Handle<LocaleBundle>> = ["en-US", "de"]
        .iter()
        .map(|id| asset_server.load(format!("testbench/locales/{id}/main.ftl.ron")))
        .collect();
    commands.insert_resource(ActiveLocale::new(languages[0].clone()));
    commands.insert_resource(Languages(languages));
    commands.spawn((
        ShellUi,
        HtmlUi::new(asset_server.load("testbench/shell.html")),
        HtmlStylesheet(asset_server.load("testbench/style.css")),
        TemplateContext::new(),
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
    ));
}

/// `is="stage"`: the shell built its stage; (re)start the scenario in it.
fn connect_stage(stage: In<ElementConnected>, mut slot: ResMut<Stage>, mut bench: ResMut<Bench>) {
    slot.0 = Some(stage.entity);
    bench.restart = true;
}

fn count_builds(
    built: On<HtmlUiBuilt>,
    shells: Query<(), With<ShellUi>>,
    frames: Res<FrameCount>,
    mut stats: ResMut<Stats>,
    mut last: ResMut<Built>,
) {
    if !shells.contains(built.entity) {
        stats.builds += 1;
        last.0.insert(built.entity, frames.0);
    }
}

fn count_restyles(
    restyled: On<HtmlUiRestyled>,
    shells: Query<(), With<ShellUi>>,
    frames: Res<FrameCount>,
    mut stats: ResMut<Stats>,
    mut last: ResMut<Built>,
) {
    if !shells.contains(restyled.entity) {
        stats.restyles += 1;
        last.0.insert(restyled.entity, frames.0);
    }
}

/// The shell's controls.
fn handle_signals(
    mut signals: MessageReader<ElementSignal>,
    mut bench: ResMut<Bench>,
    mut stats: ResMut<Stats>,
    languages: Res<Languages>,
    mut active: ResMut<ActiveLocale>,
) {
    for signal in signals.read() {
        if signal.trigger != SignalTrigger::Click {
            continue;
        }
        match signal.name.as_ref() {
            "scenario" => {
                let id = signal.payload.get("id").and_then(Value::as_str);
                if let Some(index) = id.and_then(|id| bench.scenarios.iter().position(|s| *s == id)) {
                    bench.select(index);
                }
            }
            "size-down" => bench.step_size(-1),
            "size-up" => bench.step_size(1),
            "churn" => bench.churn ^= true,
            "restart" => bench.restart = true,
            "language" => switch_language(&languages, &mut active),
            _ => continue,
        }
        stats.shell_now = true;
    }
}

/// ← → on the focused size value.
fn apply_setting_steps(
    mut steps: MessageReader<SettingStep>,
    mut bench: ResMut<Bench>,
    mut stats: ResMut<Stats>,
) {
    for step in steps.read() {
        if step.setting == "size" {
            bench.step_size(step.steps);
            stats.shell_now = true;
        }
    }
}

/// Keyboard and gamepad shortcuts.
fn hotkeys(
    hotkeys: Hotkeys,
    mut bench: ResMut<Bench>,
    mut stats: ResMut<Stats>,
    languages: Res<Languages>,
    mut active: ResMut<ActiveLocale>,
) {
    let count = bench.scenarios.len();
    let before = (bench.active, bench.size, bench.churn, bench.restart);
    if hotkeys.just_pressed(KeyCode::KeyN, GamepadButton::RightTrigger) {
        let next = bench.active + 1;
        bench.select(next);
    }
    if hotkeys.just_pressed(KeyCode::KeyP, GamepadButton::LeftTrigger) {
        let previous = bench.active + count - 1;
        bench.select(previous);
    }
    if hotkeys.just_pressed(KeyCode::BracketLeft, GamepadButton::LeftTrigger2) {
        bench.step_size(-1);
    }
    if hotkeys.just_pressed(KeyCode::BracketRight, GamepadButton::RightTrigger2) {
        bench.step_size(1);
    }
    if hotkeys.just_pressed(KeyCode::KeyC, GamepadButton::West) {
        bench.churn ^= true;
    }
    if hotkeys.just_pressed(KeyCode::KeyR, GamepadButton::Select) {
        bench.restart = true;
    }
    if hotkeys.just_pressed(KeyCode::KeyL, GamepadButton::North) {
        switch_language(&languages, &mut active);
        stats.shell_now = true;
    }
    if before != (bench.active, bench.size, bench.churn, bench.restart) {
        stats.shell_now = true;
    }
}

fn switch_language(languages: &Languages, active: &mut ActiveLocale) {
    let current = languages
        .0
        .iter()
        .position(|bundle| active.0.as_ref() == Some(bundle))
        .unwrap_or(0);
    active.set(languages.0[(current + 1) % languages.0.len()].clone());
}

/// A JSON object as a Tera context.
fn template_context(value: &Value) -> TemplateContext {
    let mut context = TemplateContext::new();
    if let Some(map) = value.as_object() {
        for (key, value) in map {
            context = context.with(key.clone(), value);
        }
    }
    context
}

/// Spawns the scenario's workload UIs into the stage.
#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
fn start_scenario(
    mut bench: ResMut<Bench>,
    stage: Res<Stage>,
    workloads: Query<Entity, With<Workload>>,
    server: Res<AssetServer>,
    frames: Res<FrameCount>,
    mut checks: ResMut<Checks>,
    mut twins: ResMut<Twins>,
    mut built: ResMut<Built>,
    mut commands: Commands,
) {
    if !bench.restart {
        return;
    }
    let Some(stage) = stage.0 else {
        return;
    };
    bench.restart = false;
    for workload in &workloads {
        commands.entity(workload).despawn();
    }
    bench.generation += 1;
    let id = bench.id();
    debug!("testbench: start {id} #{} (size {})", bench.generation, SIZES[bench.size]);
    bench.model = Model::new(SIZES[bench.size]);
    bench.paused = false;
    bench.started = frames.0;
    bench.worst_ms = 0.0;
    let prefix = format!("g{}-{id}", bench.generation);
    let plan = scenarios::plan(id, generated(), &prefix);
    checks.0 = plan
        .checks
        .iter()
        .map(|id| Check {
            id,
            state: "wait",
            detail: String::new(),
        })
        .collect();
    *twins = Twins::default();
    built.0.clear();
    // The workload UIs are nested `HtmlUi` roots right in the stage: the
    // shell keeps those across its own updates (an app node there would be
    // reconciled away). The stage's `row` class lays them side by side.
    bench.row = plan.row;
    for spec in plan.specs {
        // The many-UIs scenario's one card spec stands for N cards.
        let roles: Vec<Role> = match spec.role {
            Role::Card(_) => (0..bench.model.rows.len()).map(Role::Card).collect(),
            role => vec![role],
        };
        for role in roles {
            let template = server.load::<HtmlTemplate>(spec.template.clone());
            let sheet = spec.stylesheet.clone().map(|path| server.load::<Stylesheet>(path));
            let context = spec
                .context
                .clone()
                .unwrap_or_else(|| scenarios::context(id, &bench.model, role));
            let node = Node {
                width: match role {
                    Role::Card(_) => Val::Px(110.0),
                    _ if plan.row => Val::Percent(49.0),
                    _ => Val::Percent(100.0),
                },
                flex_direction: FlexDirection::Column,
                ..default()
            };
            let mut ui = commands.spawn((
                HtmlUi::new(template.clone()),
                template_context(&context),
                node,
                HtmlNoFocus,
                Workload {
                    role,
                    template,
                    sheet: sheet.clone(),
                },
                ChildOf(stage),
            ));
            if let Some(sheet) = sheet {
                ui.insert(HtmlStylesheet(sheet));
            }
        }
    }
}

/// One frame of the scenario's churn, written into the workloads.
#[hotpath::measure]
fn churn(
    mut bench: ResMut<Bench>,
    mut workloads: Query<(&Workload, &mut TemplateContext), Without<FreshTwin>>,
) {
    if !bench.churn || bench.paused || bench.restart {
        return;
    }
    let id = bench.id();
    let changed = scenarios::churn(id, &mut bench.model);
    let bench = &*bench;
    let update = |role: Role, context: &mut TemplateContext| {
        *context = hotpath::measure_block!("testbench::serialize_context", {
            template_context(&scenarios::context(id, &bench.model, role))
        });
    };
    match changed {
        Changed::None => {}
        Changed::All => {
            for (workload, mut context) in &mut workloads {
                if matches!(workload.role, Role::Main | Role::Twin) {
                    update(workload.role, &mut context);
                }
            }
        }
        Changed::Card(index) => {
            for (workload, mut context) in &mut workloads {
                if workload.role == Role::Card(index) {
                    update(workload.role, &mut context);
                }
            }
        }
    }
}

/// What a UI built, as text: elements (tag, id, classes), text runs and
/// backgrounds in document order. Two UIs of the same state must agree.
#[hotpath::measure]
fn signature(
    root: Entity,
    tree: &Query<&Children>,
    parts: &Query<(
        Option<&HtmlElement>,
        Option<&Text>,
        Option<&TextSpan>,
        Option<&BackgroundColor>,
    )>,
) -> Vec<String> {
    let mut lines = Vec::new();
    for entity in tree.iter_descendants_depth_first(root) {
        let Ok((element, text, span, background)) = parts.get(entity) else {
            continue;
        };
        let mut line = String::new();
        if let Some(element) = element {
            let _ = write!(line, "<{} {:?} {:?}>", element.tag, element.id, element.classes);
        }
        if let Some(text) = text {
            let _ = write!(line, "{:?}", text.0);
        }
        if let Some(span) = span {
            let _ = write!(line, "{:?}", span.0);
        }
        if let Some(background) = background.filter(|background| background.0 != Color::NONE) {
            let _ = write!(line, " bg={:?}", background.0);
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    lines
}

/// The correctness scenarios: every 20 frames, hold the churn, let both
/// UIs settle, and compare what they built.
#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
fn compare_twins(
    mut bench: ResMut<Bench>,
    frames: Res<FrameCount>,
    mut twins: ResMut<Twins>,
    built: Res<Built>,
    workloads: Query<(Entity, &Workload, &TemplateContext, &ChildOf)>,
    tree: Query<&Children>,
    parts: Query<(
        Option<&HtmlElement>,
        Option<&Text>,
        Option<&TextSpan>,
        Option<&BackgroundColor>,
    )>,
    mut commands: Commands,
) {
    let id = bench.id();
    if !matches!(id, "reuse-vs-fresh" | "indent-vs-flat") || bench.restart {
        return;
    }
    let now = frames.0;
    let Some(pending) = twins.pending.take() else {
        if now.saturating_sub(bench.started) < 30 || now % 20 != 0 {
            return;
        }
        let Some((main, workload, context, parent)) =
            workloads.iter().find(|(_, workload, ..)| workload.role == Role::Main)
        else {
            return;
        };
        let pending = if id == "reuse-vs-fresh" {
            // A fresh UI of the same state, laid out nowhere.
            let mut twin = commands.spawn((
                HtmlUi::new(workload.template.clone()),
                context.clone(),
                Node {
                    display: Display::None,
                    ..default()
                },
                HtmlNoFocus,
                FreshTwin,
                Workload {
                    role: Role::Twin,
                    template: workload.template.clone(),
                    sheet: workload.sheet.clone(),
                },
                ChildOf(parent.parent()),
            ));
            if let Some(sheet) = &workload.sheet {
                twin.insert(HtmlStylesheet(sheet.clone()));
            }
            Pending {
                main,
                twin: twin.id(),
                fresh: true,
                since: now,
            }
        } else {
            let Some((twin, ..)) = workloads.iter().find(|(_, workload, ..)| workload.role == Role::Twin) else {
                return;
            };
            Pending {
                main,
                twin,
                fresh: false,
                since: now,
            }
        };
        bench.paused = true;
        twins.pending = Some(pending);
        return;
    };
    // Settled: the twin has built, and neither UI changed for two frames.
    let last = |entity| built.0.get(&entity).copied();
    let settled = last(pending.twin).is_some_and(|twin| twin >= pending.since)
        && now.saturating_sub(last(pending.twin).unwrap_or(now).max(last(pending.main).unwrap_or(0))) >= 2
        && now.saturating_sub(pending.since) >= 3;
    let timed_out = now.saturating_sub(pending.since) > 300;
    if !settled && !timed_out {
        twins.pending = Some(pending);
        return;
    }
    if timed_out && !settled {
        twins.mismatches += 1;
        twins.last_mismatch = "timeout: the twin never settled".into();
    } else {
        let main = signature(pending.main, &tree, &parts);
        let twin = signature(pending.twin, &tree, &parts);
        if main == twin {
            twins.matches += 1;
        } else {
            twins.mismatches += 1;
            let at = main.iter().zip(&twin).position(|(a, b)| a != b).unwrap_or(main.len().min(twin.len()));
            twins.last_mismatch = format!(
                "line {at}: {} / {}",
                main.get(at).map_or("(end)", String::as_str),
                twin.get(at).map_or("(end)", String::as_str)
            );
            warn!("testbench: twins differ at {}", twins.last_mismatch);
        }
    }
    if pending.fresh {
        commands.entity(pending.twin).despawn();
    }
    bench.paused = false;
}

/// The text a UI element shows (its own text and its spans).
fn element_text(
    entity: Entity,
    tree: &Query<&Children>,
    texts: &Query<(Option<&Text>, Option<&TextSpan>)>,
) -> String {
    std::iter::once(entity)
        .chain(tree.iter_descendants_depth_first(entity))
        .filter_map(|entity| texts.get(entity).ok())
        .map(|(text, span)| {
            format!(
                "{}{}",
                text.map_or("", |text| text.0.as_str()),
                span.map_or("", |span| span.0.as_str())
            )
        })
        .collect()
}

/// Evaluates the scenario's checks.
#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
fn check_scenario(
    bench: Res<Bench>,
    frames: Res<FrameCount>,
    mut checks: ResMut<Checks>,
    twins: Res<Twins>,
    built: Res<Built>,
    server: Res<AssetServer>,
    workloads: Query<(Entity, &Workload, &RenderedHtml), Without<FreshTwin>>,
    tree: Query<&Children>,
    elements: Query<(Entity, &HtmlElement, Option<&ElementSignals>)>,
    texts: Query<(Option<&Text>, Option<&TextSpan>)>,
) {
    let now = frames.0;
    let probe = |name: &str| {
        workloads
            .iter()
            .find(|(_, workload, _)| matches!(workload.role, Role::Probe(probe) if probe == name))
    };
    // The workloads' elements (not the shell's).
    let workload_elements = || {
        workloads.iter().flat_map(|(root, ..)| {
            tree.iter_descendants_depth_first(root)
                .filter_map(|entity| elements.get(entity).ok())
                .collect::<Vec<_>>()
        })
    };
    for check in &mut checks.0 {
        let (state, detail): (&'static str, String) = match check.id {
            "budget" => {
                if now.saturating_sub(bench.started) < 60 {
                    ("wait", format!("{:.1}", bench.worst_ms))
                } else if bench.worst_ms < 250.0 {
                    ("pass", format!("{:.1}", bench.worst_ms))
                } else {
                    ("fail", format!("{:.1}", bench.worst_ms))
                }
            }
            "built" => {
                let mut state = "pass";
                let mut detail = String::new();
                for (entity, _, rendered) in &workloads {
                    match rendered {
                        RenderedHtml::Failed(message) => {
                            state = "fail";
                            detail = message.chars().take(80).collect();
                        }
                        RenderedHtml::Ready(_) if built.0.contains_key(&entity) => {}
                        _ if state == "pass" => state = "wait",
                        _ => {}
                    }
                }
                if workloads.is_empty() {
                    state = "wait";
                }
                if state == "pass" {
                    detail = format!("({} elements)", workload_elements().count());
                }
                (state, detail)
            }
            "survived" => {
                let resolved = workloads.iter().all(|(_, workload, rendered)| {
                    !matches!(rendered, RenderedHtml::Pending) || server.load_state(&workload.template).is_failed()
                });
                if resolved && !workloads.is_empty() {
                    ("pass", format!("({} elements)", workload_elements().count()))
                } else {
                    ("wait", String::new())
                }
            }
            "sheet-refused" => match probe("sheet").and_then(|(_, workload, _)| workload.sheet.as_ref()) {
                Some(sheet) if server.load_state(sheet).is_failed() => ("pass", String::new()),
                Some(sheet) if server.load_state(sheet).is_loaded() => ("fail", "it loaded".into()),
                _ => ("wait", String::new()),
            },
            "no-forged" => {
                let built_all = workloads.iter().all(|(entity, ..)| built.0.contains_key(&entity));
                if !built_all || workloads.is_empty() {
                    ("wait", String::new())
                } else {
                    let forged = workload_elements()
                        .filter(|(_, element, signals)| {
                            matches!(element.tag.as_str(), "a" | "img" | "button" | "script")
                                || signals.is_some()
                        })
                        .count();
                    (if forged == 0 { "pass" } else { "fail" }, format!("({forged} forged)"))
                }
            }
            "no-fallback" => {
                // A payload whose arguments failed to parse shows the
                // element's own content, "x" (bug_0128: Tera's `\u{…}`
                // escapes in a printed map).
                let built_all = workloads.iter().all(|(entity, ..)| built.0.contains_key(&entity));
                if !built_all || workloads.is_empty() {
                    ("wait", String::new())
                } else {
                    let fallbacks = workload_elements()
                        .filter(|(entity, element, _)| {
                            element.tag == "p" && element_text(*entity, &tree, &texts) == "x"
                        })
                        .count();
                    (if fallbacks == 0 { "pass" } else { "fail" }, format!("({fallbacks} fell back)"))
                }
            }
            "map-literal" => {
                let text_of = |id: &str| {
                    workload_elements()
                        .find(|(_, element, _)| element.id.as_deref() == Some(id))
                        .map(|(entity, ..)| element_text(entity, &tree, &texts))
                        .unwrap_or_default()
                };
                let literal = text_of("literal");
                let splice = text_of("splice");
                if literal.is_empty() {
                    ("wait", String::new())
                } else if literal.contains("INJECTED") {
                    ("pass", format!("({literal:?}; splice: {splice:?})"))
                } else {
                    ("fail", format!("({literal:?})"))
                }
            }
            "include-refused" => match probe("include") {
                Some((_, workload, _)) if server.load_state(&workload.template).is_failed() => {
                    ("pass", String::new())
                }
                Some((_, workload, _)) if server.load_state(&workload.template).is_loaded() => {
                    ("fail", "it loaded".into())
                }
                _ => ("wait", String::new()),
            },
            "border-skipped" => match probe("border") {
                Some((entity, workload, _)) => {
                    let sheet = workload.sheet.as_ref().map(|sheet| server.load_state(sheet));
                    if sheet.as_ref().is_some_and(|state| state.is_failed()) {
                        ("fail", "the sheet failed".into())
                    } else if built.0.contains_key(&entity) {
                        ("pass", String::new())
                    } else {
                        ("wait", String::new())
                    }
                }
                None => ("wait", String::new()),
            },
            "twins" => {
                let detail = if twins.mismatches > 0 {
                    format!("({} / {}: {})", twins.matches, twins.mismatches, twins.last_mismatch)
                } else {
                    format!("({} / {})", twins.matches, twins.mismatches)
                };
                let state = if twins.mismatches > 0 {
                    "fail"
                } else if twins.matches > 0 {
                    "pass"
                } else {
                    "wait"
                };
                (state, detail)
            }
            _ => ("wait", String::new()),
        };
        // Scripted runs (`--no-render --exit-after N`) read the checks
        // from the log.
        if state != check.state {
            info!("testbench: {} {}: {} {}", bench.id(), check.id, state, detail);
        }
        check.state = state;
        check.detail = detail;
    }
}

/// On exit, the final state of every check (a scripted run's result).
fn summarize_on_exit(
    mut exits: MessageReader<AppExit>,
    bench: Res<Bench>,
    checks: Res<Checks>,
    twins: Res<Twins>,
) {
    if exits.read().next().is_none() {
        return;
    }
    for check in &checks.0 {
        info!("testbench: final {} {}: {} {}", bench.id(), check.id, check.state, check.detail);
    }
    if twins.matches + twins.mismatches > 0 {
        info!(
            "testbench: final {} twins compared: {} match, {} mismatch",
            bench.id(),
            twins.matches,
            twins.mismatches
        );
    }
}

/// Frame time and update rates.
fn update_stats(time: Res<Time>, frames: Res<FrameCount>, mut stats: ResMut<Stats>, mut bench: ResMut<Bench>) {
    let ms = time.delta_secs() * 1000.0;
    if stats.frame_ms.len() == 120 {
        stats.frame_ms.pop_front();
    }
    stats.frame_ms.push_back(ms);
    // Past the warm-up (loads, the first builds), the worst frame counts.
    if frames.0.saturating_sub(bench.started) > 30 && !bench.restart {
        bench.worst_ms = bench.worst_ms.max(ms);
    }
    let now = time.elapsed_secs();
    if now - stats.window_start >= 1.0 {
        stats.builds_rate = stats.builds;
        stats.restyles_rate = stats.restyles;
        stats.builds = 0;
        stats.restyles = 0;
        stats.window_start = now;
    }
}

/// Writes the numbers and the checks into the shell, a few times a second.
#[hotpath::measure]
#[allow(clippy::too_many_arguments)]
fn show_shell(
    time: Res<Time>,
    mut stats: ResMut<Stats>,
    bench: Res<Bench>,
    checks: Res<Checks>,
    elements: Query<(), With<HtmlElement>>,
    workloads: Query<(), (With<Workload>, Without<FreshTwin>)>,
    mut shell: Query<&mut TemplateContext, (With<ShellUi>, Without<Workload>)>,
) {
    let now = time.elapsed_secs();
    if !stats.shell_now && now < stats.shell_at {
        return;
    }
    stats.shell_now = false;
    stats.shell_at = now + 0.25;
    let Ok(mut context) = shell.single_mut() else {
        return;
    };
    let count = stats.frame_ms.len().max(1) as f32;
    let avg = stats.frame_ms.iter().sum::<f32>() / count;
    let max = stats.frame_ms.iter().copied().fold(0.0, f32::max);
    let groups: Vec<Value> = scenarios::GROUPS
        .iter()
        .map(|(id, ids)| json!({ "id": id, "scenarios": ids }))
        .collect();
    let checks: Vec<Value> = checks
        .0
        .iter()
        .map(|check| json!({ "id": check.id, "state": check.state, "detail": check.detail }))
        .collect();
    *context = template_context(&json!({
        "groups": groups,
        "active": bench.id(),
        "size": SIZES[bench.size],
        "churn": bench.churn,
        "row": bench.row,
        "stats": {
            "avg_ms": format!("{avg:.1}"),
            "max_ms": format!("{max:.1}"),
            "fps": format!("{:.0}", 1000.0 / avg.max(0.001)),
            "builds": stats.builds_rate,
            "restyles": stats.restyles_rate,
            "uis": workloads.iter().count(),
            "elements": elements.iter().count(),
        },
        "checks": checks,
    }));
}
