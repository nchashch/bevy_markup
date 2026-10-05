//! Property tests for interaction signals: random pages of bound elements
//! (nested, with random trigger subsets and `pointer-events: none`
//! subtrees) and random pointer-op sequences, run through the real picking
//! stack ([`TestUi::with_pointer`]) and compared op by op with a reference
//! model of what bevy_picking + bevy_markup must emit.
//!
//! The model encodes the semantics the unit tests pin individually:
//! - the *deepest* non-ignored node under the pointer owns the hit; an
//!   event bubbles up its element chain and the *deepest* chain element
//!   bound for a trigger emits it (nested hooks: deepest wins, per trigger);
//! - enter/leave count for a whole element chain (like CSS `:hover`): a
//!   move emits enters for newly covered bound elements and leaves for the
//!   uncovered ones, from the snapshot taken at enter;
//! - pressing marks the hovered node; a release emits a click only if the
//!   currently hovered node is the pressed one — and the press is consumed
//!   (bevy_picking clears the pointer's press state per release) — while the
//!   release hooks of the currently hovered chain always run.
//!
//! Pages are plain containers without text, so hit chains are pure node
//! geometry (rect containment) and the model needs no text layout; the
//! text-span path is pinned by the `tests/signals.rs` unit tests.

mod common;

use bevy::prelude::*;
use bevy_markup::prelude::*;
use common::{TestUi, node_rect};
use proptest::prelude::*;
use proptest::prop_assert_eq;
use proptest::strategy::BoxedStrategy;
use test_strategy::proptest;

const VIEWPORT: UVec2 = UVec2::new(640, 480);
const TRIGGERS: [SignalTrigger; 5] = [
    SignalTrigger::Click,
    SignalTrigger::Press,
    SignalTrigger::Release,
    SignalTrigger::Enter,
    SignalTrigger::Leave,
];

/// `data-on-<suffix>` per trigger.
const SUFFIXES: [&str; 5] = ["click", "press", "release", "enter", "leave"];

// ---------------------------------------------------------------- page ----

/// A generated page: containers `a`, `b`, `c` with optional children
/// (`a-0`, `a-0-0`, …), random trigger subsets, random `pointer-events:
/// none` elements.
#[derive(Clone, Debug)]
struct Spec {
    ids: Vec<String>,
    parents: Vec<Option<usize>>,
    /// Per element, the `data-on-*` hooks (by index into [`TRIGGERS`]).
    bindings: Vec<Vec<u8>>,
    /// Elements declaring `pointer-events: none` (inherited by subtrees).
    none: Vec<bool>,
}

fn page_spec() -> BoxedStrategy<Spec> {
    // Per element: has child, has grandchild, binding mask, pointer-events none.
    let shape = (any::<bool>(), any::<bool>(), 0u32..32, any::<bool>());
    proptest::collection::vec(shape, 1..=3)
        .prop_map(|shapes| {
            let mut spec = Spec {
                ids: Vec::new(),
                parents: Vec::new(),
                bindings: Vec::new(),
                none: Vec::new(),
            };
            for (top, &(has_child, has_grandchild, mask, none)) in shapes.iter().enumerate() {
                let name = char::from(b'a' + top as u8);
                fn push(spec: &mut Spec, id: String, parent: Option<usize>, mask: u32, none: bool) {
                    spec.ids.push(id);
                    spec.parents.push(parent);
                    spec.bindings.push(
                        TRIGGERS
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| mask & (1 << i) != 0)
                            .map(|(i, _)| i as u8)
                            .collect(),
                    );
                    spec.none.push(none);
                }
                let top_index = spec.ids.len();
                push(&mut spec, name.to_string(), None, mask, none);
                if has_child {
                    let child = spec.ids.len();
                    push(&mut spec, format!("{name}-0"), Some(top_index), mask, none);
                    if has_grandchild {
                        push(&mut spec, format!("{name}-0-0"), Some(child), mask, none);
                    }
                }
            }
            spec
        })
        .boxed()
}

/// The generated `page.html` + `style.css`: nested empty containers with
/// fixed sizes (top level `180x30`, child `120x16`, grandchild `80x10`).
fn render(spec: &Spec) -> (String, String) {
    fn div(spec: &Spec, i: usize) -> String {
        let attrs: String = spec.bindings[i]
            .iter()
            .map(|&t| format!(" data-on-{}=\"{}\"", SUFFIXES[t as usize], spec.ids[i]))
            .collect();
        let child = (0..spec.ids.len()).find(|&c| spec.parents[c] == Some(i));
        let child = child.map(|c| div(spec, c)).unwrap_or_default();
        format!("<div id=\"{}\"{attrs}>{child}</div>", spec.ids[i])
    }
    let html: String = (0..spec.ids.len())
        .filter(|&i| spec.parents[i].is_none())
        .map(|i| div(spec, i))
        .collect::<Vec<_>>()
        .join("\n");
    let css: String = spec
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let depth = {
                let mut d = 0;
                let mut current = spec.parents[i];
                while let Some(p) = current {
                    d += 1;
                    current = spec.parents[p];
                }
                d
            };
            let size = match depth {
                0 => "width: 180px; height: 30px",
                1 => "width: 120px; height: 16px",
                _ => "width: 80px; height: 10px",
            };
            let none = if spec.none[i] {
                "; pointer-events: none"
            } else {
                ""
            };
            format!("#{id} {{ {size}{none} }}\n")
        })
        .collect();
    (html, css)
}

// ----------------------------------------------------------------- ops ----

/// A pointer op; moves carry a raw choice resolved at runtime against the
/// real rects: `< n` → center of element n, `< 2n` → a point in element
/// `choice - n` below its child, else → below the root, over nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Op {
    Move(usize),
    Press,
    Release,
}

fn ops() -> BoxedStrategy<Vec<Op>> {
    proptest::collection::vec((0..3u8, 0..9u8), 1..=12)
        .prop_map(|raw| {
            raw.into_iter()
                .map(|(kind, choice)| match kind {
                    0 => Op::Move(choice as usize),
                    1 => Op::Press,
                    _ => Op::Release,
                })
                .collect()
        })
        .boxed()
}

// --------------------------------------------------------------- model ----

/// Where the deepest picked node is: an element (its index), the un-styled
/// `HtmlUi` root node (inside the root rect, over no element), or nothing
/// (outside the root).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Picked {
    Element(usize),
    Root,
    None,
}

/// A signal as the model emits it: name, trigger, emitting element.
type Expected = (String, usize, usize);

#[derive(Default)]
struct PointerModel {
    /// Elements whose enter hooks have fired and whose leave hooks will fire
    /// when uncovered.
    entered: Vec<usize>,
    /// The node a press marked (persists across releases, like
    /// bevy_picking's `state.pressing`).
    pressed: Option<Picked>,
    position: Option<Vec2>,
}

struct Model<'a> {
    spec: &'a Spec,
    /// Who counts as pressed/ignored: an element is hit-testable only if it
    /// and all ancestors allow pointer events.
    ignored: Vec<bool>,
}

impl<'a> Model<'a> {
    fn new(spec: &'a Spec) -> Self {
        let mut ignored = vec![false; spec.ids.len()];
        for i in 0..spec.ids.len() {
            ignored[i] = spec.none[i] || spec.parents[i].is_some_and(|p| ignored[p]);
        }
        Model { spec, ignored }
    }

    /// The deepest picked node at `p`, or `None` before any pointer move
    /// (Bevy has no pointer location, so nothing is hovered).
    fn picked(&self, p: Option<Vec2>, rects: &[Rect], root: Rect) -> Picked {
        let Some(p) = p else { return Picked::None };
        if !root.contains(p) {
            return Picked::None;
        }
        let mut best: Option<usize> = None;
        for (i, r) in rects.iter().enumerate() {
            if self.ignored[i] || !r.contains(p) {
                continue;
            }
            if best.is_none_or(|b| r.area() < rects[b].area()) {
                best = Some(i);
            }
        }
        best.map_or(Picked::Root, Picked::Element)
    }

    /// The element chain under `node`, deepest first. Chains of ignored
    /// elements are unreachable (a hit below an ignored ancestor would be
    /// ignored itself).
    fn chain(&self, node: Picked) -> Vec<usize> {
        let mut chain = Vec::new();
        let mut current = match node {
            Picked::Element(i) => Some(i),
            _ => None,
        };
        while let Some(i) = current {
            chain.push(i);
            current = self.spec.parents[i];
        }
        chain
    }

    /// The deepest chain element bound for `trigger`.
    fn emitter(&self, chain: &[usize], trigger: SignalTrigger) -> Option<usize> {
        chain.iter().copied().find(|&i| {
            self.spec.bindings[i]
                .iter()
                .any(|&t| TRIGGERS[t as usize] == trigger)
        })
    }

    fn move_to(
        &mut self,
        pointer: &mut PointerModel,
        p: Vec2,
        rects: &[Rect],
        root: Rect,
    ) -> Vec<Expected> {
        let picked = self.picked(Some(p), rects, root);
        let mut now: Vec<usize> = self
            .chain(picked)
            .into_iter()
            .filter(|&i| {
                self.spec.bindings[i].iter().any(|&t| {
                    TRIGGERS[t as usize] == SignalTrigger::Enter
                        || TRIGGERS[t as usize] == SignalTrigger::Leave
                })
            })
            .collect();
        now.sort_unstable();
        let mut out = Vec::new();
        for &i in &now {
            if pointer.entered.contains(&i) {
                continue;
            }
            if self.spec.bindings[i]
                .iter()
                .any(|&t| TRIGGERS[t as usize] == SignalTrigger::Enter)
            {
                out.push((self.spec.ids[i].clone(), TRIGGER_INDEX_ENTER, i));
            }
        }
        for &i in &pointer.entered {
            if now.contains(&i)
                || !self.spec.bindings[i]
                    .iter()
                    .any(|&t| TRIGGERS[t as usize] == SignalTrigger::Leave)
            {
                continue;
            }
            out.push((self.spec.ids[i].clone(), TRIGGER_INDEX_LEAVE, i));
        }
        pointer.entered = now;
        pointer.position = Some(p);
        sort_expected(out)
    }

    fn press(&mut self, pointer: &mut PointerModel, rects: &[Rect], root: Rect) -> Vec<Expected> {
        let picked = self.picked(pointer.position, rects, root);
        pointer.pressed = Some(picked);
        let chain = self.chain(picked);
        let emitter = self.emitter(&chain, SignalTrigger::Press);
        sort_expected(
            emitter
                .map(|i| vec![(self.spec.ids[i].clone(), TRIGGER_INDEX_PRESS, i)])
                .unwrap_or_default(),
        )
    }

    fn release(&mut self, pointer: &mut PointerModel, rects: &[Rect], root: Rect) -> Vec<Expected> {
        let picked = self.picked(pointer.position, rects, root);
        let mut out = Vec::new();
        let chain = self.chain(picked);
        // Click only when the hovered node is the pressed one; the press is
        // consumed — bevy_picking clears the pointer's press state at the
        // end of the release action, so a second release clicks nothing.
        if pointer.pressed == Some(picked)
            && let Some(i) = self.emitter(&chain, SignalTrigger::Click)
        {
            out.push((self.spec.ids[i].clone(), TRIGGER_INDEX_CLICK, i));
        }
        pointer.pressed = None;
        // The release hooks always run over the hovered chain.
        if let Some(i) = self.emitter(&chain, SignalTrigger::Release) {
            out.push((self.spec.ids[i].clone(), TRIGGER_INDEX_RELEASE, i));
        }
        sort_expected(out)
    }
}

const TRIGGER_INDEX_CLICK: usize = 0;
const TRIGGER_INDEX_PRESS: usize = 1;
const TRIGGER_INDEX_RELEASE: usize = 2;
const TRIGGER_INDEX_ENTER: usize = 3;
const TRIGGER_INDEX_LEAVE: usize = 4;

fn sort_expected(mut out: Vec<Expected>) -> Vec<Expected> {
    out.sort_unstable();
    out
}

// ------------------------------------------------------------ property ----

fn cases() -> BoxedStrategy<(Spec, Vec<Op>)> {
    (page_spec(), ops()).boxed()
}

#[proptest(cases = 24)]
fn pointer_op_sequences_match_the_signal_model(#[strategy(cases())] case: (Spec, Vec<Op>)) {
    let (spec, ops) = case;
    let (page, css) = render(&spec);
    let mut ui = TestUi::with_pointer(
        "signal-properties",
        &[("page.html", &page), ("style.css", &css)],
        VIEWPORT,
    )
    .stylesheet("style.css")
    .spawn(
        "page.html",
        TemplateContext::new(),
        Node {
            flex_direction: FlexDirection::Column,
            ..default()
        },
    );
    ui.settle();
    ui.update(1);
    let root = ui.root();
    let (entities, rects, root_rect) = {
        let world = ui.world_mut();
        let entities: Vec<Entity> = spec
            .ids
            .iter()
            .map(|id| {
                let mut elements = world.query::<(Entity, &HtmlElement)>();
                elements
                    .iter(world)
                    .find(|(_, element)| element.id.as_deref() == Some(id.as_str()))
                    .map(|(entity, _)| entity)
                    .unwrap_or_else(|| panic!("no #{id}"))
            })
            .collect();
        let rects: Vec<Rect> = entities
            .iter()
            .map(|&e| node_rect(world, e).expect("laid out"))
            .collect();
        let root_rect = node_rect(world, root).expect("laid out root");
        (entities, rects, root_rect)
    };
    let by_entity: std::collections::HashMap<Entity, usize> = entities
        .iter()
        .copied()
        .enumerate()
        .map(|(i, e)| (e, i))
        .collect();
    let mut model = Model::new(&spec);
    let mut pointer = PointerModel::default();

    for op in &ops {
        let expected = match op {
            Op::Move(choice) => {
                let p = resolve(*choice, &spec, &rects, root_rect);
                ui.move_pointer(p);
                model.move_to(&mut pointer, p, &rects, root_rect)
            }
            Op::Press => {
                ui.press_pointer();
                model.press(&mut pointer, &rects, root_rect)
            }
            Op::Release => {
                ui.release_pointer();
                model.release(&mut pointer, &rects, root_rect)
            }
        };
        let actual: Vec<Expected> = ui
            .take_signals()
            .into_iter()
            .map(|signal| {
                (
                    signal.name.to_string(),
                    TRIGGERS
                        .iter()
                        .position(|t| *t == signal.trigger)
                        .expect("known trigger"),
                    by_entity[&signal.target],
                )
            })
            .collect();
        let mut actual = actual;
        actual.sort_unstable();
        prop_assert_eq!(
            actual,
            expected,
            "after {op:?} of {ops:?}",
            op = op,
            ops = ops
        );
    }
}

/// Resolves a raw op choice to a point: element centers, points in an
/// element but below its child, and the area below the root.
fn resolve(choice: usize, spec: &Spec, rects: &[Rect], root: Rect) -> Vec2 {
    let n = spec.ids.len();
    if choice < n {
        rects[choice].center()
    } else if choice < 2 * n {
        let r = rects[choice - n];
        Vec2::new(r.center().x, r.max.y - 4.0)
    } else {
        Vec2::new(root.center().x, root.max.y + 40.0)
    }
}
