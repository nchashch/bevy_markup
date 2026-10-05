//! Stateful property tests (`proptest_stateful`): random *sequences* of
//! runtime mutations applied to one long-lived `HtmlUi`, each checked against
//! a reference model right after it settles.
//!
//! The modeled state — default theme, the entity's own `HtmlStylesheet`,
//! locale, context value, outline marker, registered font faces, and the
//! pointer (position, hovered/entered button, held press) — is exactly the
//! state that decides the dump and the `ElementSignal`s. Pointer ops run the
//! real picking stack (`TestUi::with_pointer`, real `WindowEvent` input) and
//! the model predicts what bevy_picking + bevy_markup emit: enter/leave for
//! the hovered chain (including a leave from the enter snapshot when a
//! rebuild or restyle drops the button out from under a stationary
//! pointer), press/click/release with the press consumed per release. The
//! interesting invariants are the ones
//! pairwise tests can't reach: the `html` rule's box properties must survive
//! *arbitrary* theme chains (framed → plain → broken → …) with the app's own
//! `Node` restored whenever no CSS provides them; a broken stylesheet must
//! never wedge the pipeline (the `FailedSheets` latch); and a per-entity
//! sheet must win while it's set and loads, fall back to the default when it
//! fails (following later default swaps), and hand back to the default when
//! removed.
//!
//! `TestUi` (see `common`) is the harness; each test case runs its own app.

mod common;
use common::{SERIF, TestUi};

use async_trait::async_trait;
use bevy::prelude::*;
use bevy_markup::bevy_fluent::BundleAsset;
use bevy_markup::prelude::*;
use proptest::prelude::*;
use proptest_stateful::{ModelState, ProptestStatefulConfig};
use std::time::Duration;

/// A stylesheet `DefaultStylesheet` or the entity's `HtmlStylesheet` can
/// point at.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Theme {
    #[default]
    Framed,
    Plain,
    /// Fails to parse: the UI must render unstyled, not blank.
    Broken,
}

impl Theme {
    fn path(self) -> &'static str {
        match self {
            Theme::Framed => "framed.css",
            Theme::Plain => "plain.css",
            Theme::Broken => "broken.css",
        }
    }
}

/// Which Fluent bundle `ActiveLocale` points at (`Raw` = `None`: the
/// element's own content shows).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Locale {
    #[default]
    Raw,
    En,
    De,
}

const VIEWPORT: UVec2 = UVec2::new(640, 480);

/// What a press is holding: nothing, the button, or some other (replaced or
/// unbound) node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Pressed {
    #[default]
    None,
    Btn,
    Other,
}

/// An expected signal: the hook's name and its trigger, as a sortable key.
type Expected = (String, usize);

const CLICK: usize = 0;
const PRESS: usize = 1;
const RELEASE: usize = 2;
const ENTER: usize = 3;
const LEAVE: usize = 4;

/// The modeled state of the `HtmlUi`. `Default` doubles as the state the
/// harness initializes (default theme framed, no own sheet, no locale,
/// `n = 0`, pointer untouched).
#[derive(Clone, Debug, Default)]
struct UiModel {
    theme: Theme,
    /// The entity's `HtmlStylesheet`, if any.
    own: Option<Theme>,
    locale: Locale,
    n: i64,
    outline: bool,
    /// `Spectral` registered with the mono face.
    fonts_swapped: bool,
    /// The pointer's absolute position, once it has moved.
    pointer: Option<Vec2>,
    /// The button's enter has fired; its leave is armed.
    entered: bool,
    pressed: Pressed,
    /// The signals the applied op must emit (drained and compared after it).
    pending: Vec<Expected>,
}

#[derive(Clone, Debug)]
enum Op {
    SetN(i64),
    SetTheme(Theme),
    SetOwnSheet(Theme),
    RemoveOwnSheet,
    SetLocale(Locale),
    ToggleOutline,
    SwapFonts,
    MoveOver,
    MoveOutside,
    Press,
    Release,
}

impl UiModel {
    /// The sheet that styles the UI: the own sheet unless it failed (then
    /// the default, as when there is none). `Broken` = unstyled.
    fn effective(&self) -> Theme {
        match self.own {
            Some(Theme::Broken) | None => self.theme,
            Some(own) => own,
        }
    }

    /// The dump we expect right now: the root line (the effective sheet
    /// decides the `html` rule's box) and the text run (locale decides the
    /// text, the sheet and fonts the styling), the latter only when the
    /// outline hasn't replaced content.
    fn expected(&self) -> (String, Option<String>) {
        let root = match self.effective() {
            Theme::Framed => {
                "html-ui border=16,16,16,16 padding=10,10,10,10 slice=frame.png 4,4,4,4 stretch"
            }
            Theme::Plain => "html-ui padding=4,4,4,4",
            // Unstyled: the app's own 3px padding is restored.
            Theme::Broken => "html-ui padding=3,3,3,3",
        };
        let text = match self.locale {
            Locale::Raw => self.n.to_string(),
            Locale::En => format!("{} items", self.n),
            Locale::De => format!("{} Dinge", self.n),
        };
        // Real glyphs: `Spectral` is unregistered in the layout harness until
        // `SwapFonts` points it at the mono face (the fake serif faces can't
        // measure), so the cascade falls back to Bevy's default font.
        let face = if self.fonts_swapped {
            "serif"
        } else {
            "default"
        };
        let style = match self.effective() {
            Theme::Framed => format!("{face} 20px #ffffff"),
            Theme::Plain => format!("{face} 12px #00ff00"),
            Theme::Broken => "default 16px #ffffff".to_owned(),
        };
        let run = (!self.outline).then(|| format!("{text:?} {style}"));
        (root.to_owned(), run)
    }

    /// Whether the button is hit-testable: the outline replaces it with text,
    /// and the unstyled (broken-sheet) button has no size, which picking
    /// skips.
    fn button_live(&self) -> bool {
        !self.outline && self.effective() != Theme::Broken
    }

    /// The button's border box, derived from the effective theme: the root's
    /// border + padding shifts the content, the `p` line above it is one
    /// line of text (Bevy ceils text nodes to whole pixels), and the button
    /// is 100×20 at the content's left edge.
    fn button_rect(&self) -> Option<Rect> {
        let (offset, font): ((f32, f32), f32) = match self.effective() {
            Theme::Framed => ((26.0, 26.0), 20.0), // border 16 + padding 10
            Theme::Plain => ((4.0, 4.0), 12.0),    // padding 4
            Theme::Broken => ((3.0, 3.0), 16.0),   // the app's own padding
        };
        if !self.button_live() {
            return None;
        }
        let line = (font * 1.2).ceil();
        Some(Rect::from_center_size(
            vec2(offset.0 + 50.0, offset.1 + line + 10.0),
            vec2(100.0, 20.0),
        ))
    }

    /// Whether the stationary pointer is over the live button.
    fn over_button(&self) -> bool {
        self.button_rect()
            .is_some_and(|rect| self.pointer.is_some_and(|p| rect.contains(p)))
    }

    /// Whether `op` must rebuild. Only a default-sheet swap under a working
    /// own sheet legitimately rebuilds nothing.
    fn rebuilds(&self, op: &Op) -> bool {
        !matches!(op, Op::SetTheme(_)) || matches!(self.own, None | Some(Theme::Broken))
    }
}

#[async_trait(?Send)]
impl ModelState for UiModel {
    type Operation = Op;
    type RunContext = TestUi;
    type OperationStrategy = BoxedStrategy<Op>;

    fn op_generators(&self) -> Vec<BoxedStrategy<Op>> {
        let themes = || prop_oneof![Just(Theme::Framed), Just(Theme::Plain), Just(Theme::Broken)];
        vec![
            (0i64..1000).prop_map(Op::SetN).boxed(),
            themes().prop_map(Op::SetTheme).boxed(),
            themes().prop_map(Op::SetOwnSheet).boxed(),
            Just(Op::RemoveOwnSheet).boxed(),
            prop_oneof![Just(Locale::En), Just(Locale::De), Just(Locale::Raw)]
                .prop_map(Op::SetLocale)
                .boxed(),
            Just(Op::ToggleOutline).boxed(),
            Just(Op::SwapFonts).boxed(),
            Just(Op::MoveOver).boxed(),
            Just(Op::MoveOutside).boxed(),
            Just(Op::Press).boxed(),
            Just(Op::Release).boxed(),
        ]
    }

    /// Only state-*changing* ops are valid: a same-value re-insert (e.g.
    /// `ActiveLocale(None)` while already unlocalized) legitimately triggers
    /// no rebuild, and `settle()` demands observable activity per op.
    /// Pointer ops always run (they need no rebuild; idempotent moves emit
    /// nothing, which the model predicts).
    fn preconditions_met(&self, op: &Op) -> bool {
        match *op {
            Op::SetN(n) => n != self.n,
            Op::SetTheme(theme) => theme != self.theme,
            Op::SetOwnSheet(theme) => self.own != Some(theme),
            Op::RemoveOwnSheet => self.own.is_some(),
            Op::SetLocale(locale) => locale != self.locale,
            Op::ToggleOutline => true,
            Op::SwapFonts => !self.fonts_swapped,
            Op::MoveOver | Op::MoveOutside | Op::Press | Op::Release => true,
        }
    }

    fn next_state(&mut self, op: &Op) {
        let was_live = self.button_live();
        match *op {
            Op::SetN(n) => self.n = n,
            Op::SetTheme(theme) => self.theme = theme,
            Op::SetOwnSheet(theme) => self.own = Some(theme),
            Op::RemoveOwnSheet => self.own = None,
            Op::SetLocale(locale) => self.locale = locale,
            Op::ToggleOutline => self.outline = !self.outline,
            Op::SwapFonts => self.fonts_swapped = true,
            Op::MoveOver => {
                self.pointer = Some(match self.button_rect() {
                    Some(rect) => rect.center(),
                    None => vec2(320.0, 240.0),
                });
            }
            Op::MoveOutside => self.pointer = Some(vec2(4000.0, 4000.0)),
            Op::Press | Op::Release => {}
        }
        // What the op must emit. Content updates that keep the button (new
        // `n`, another locale) update it in place: hover and presses survive,
        // nothing is emitted. The outline toggle replaces the content, so a
        // hovered button leaves (from the enter snapshot), the one coming
        // back enters afresh, and a held press now points at a despawned
        // entity. Style changes keep the button too, but the root box moves
        // it, so a stationary pointer can drop out of it (or into it). Releases emit
        // their hooks over the hovered chain, click only while it is the
        // pressed node, and consume the press.
        self.pending = match *op {
            Op::MoveOver => {
                let mut out = vec![];
                if self.button_live() && !self.entered {
                    self.entered = true;
                    out.push(("enter".to_owned(), ENTER));
                }
                out
            }
            Op::MoveOutside => {
                let mut out = vec![];
                if self.entered {
                    self.entered = false;
                    out.push(("leave".to_owned(), LEAVE));
                }
                out
            }
            Op::Press => {
                let mut out = vec![];
                if self.over_button() {
                    self.pressed = Pressed::Btn;
                    out.push(("press".to_owned(), PRESS));
                } else if self.pointer.is_some() {
                    // Over something unbound (or the outline text).
                    self.pressed = Pressed::Other;
                }
                out
            }
            Op::Release => {
                let mut out = vec![];
                if self.over_button() {
                    out.push(("release".to_owned(), RELEASE));
                    if self.pressed == Pressed::Btn {
                        out.push(("click".to_owned(), CLICK));
                    }
                }
                self.pressed = Pressed::None;
                out
            }
            Op::SetN(_) | Op::SetLocale(_) => vec![],
            Op::ToggleOutline => {
                let mut out = vec![];
                if self.entered {
                    self.entered = false;
                    out.push(("leave".to_owned(), LEAVE));
                }
                if self.over_button() {
                    self.entered = true;
                    out.push(("enter".to_owned(), ENTER));
                }
                if self.pressed != Pressed::None {
                    self.pressed = Pressed::Other;
                }
                out
            }
            Op::SetTheme(_) | Op::SetOwnSheet(_) | Op::RemoveOwnSheet | Op::SwapFonts => {
                let mut out = vec![];
                if was_live && !self.over_button() && self.entered {
                    self.entered = false;
                    out.push(("leave".to_owned(), LEAVE));
                }
                if !was_live && self.over_button() {
                    self.entered = true;
                    out.push(("enter".to_owned(), ENTER));
                }
                out
            }
        };
    }

    async fn init_test_run(&self) -> TestUi {
        let files = [
            (
                "page.html",
                r#"<p data-l10n-id="count" data-l10n-args='{"n": {{ n }}}'>{{ n }}</p>
<div id="btn" data-on-click="click" data-on-press="press" data-on-release="release"
     data-on-enter="enter" data-on-leave="leave"></div>"#,
            ),
            (
                "framed.css",
                r#"html { color: #ffffff; font-family: Spectral; font-size: 20px; border-image: url("frame.png") 4 fill stretch; border-width: 16px; padding: 10px }
#btn { width: 100px; height: 20px }"#,
            ),
            (
                "plain.css",
                "html { color: #00ff00; font-family: Spectral; font-size: 12px; padding: 4px }\n#btn { width: 100px; height: 20px }",
            ),
            ("broken.css", "{ not css"),
            (
                "locales/en-US/main.ftl.ron",
                r#"(locale: "en-US", resources: ["ui.ftl"])"#,
            ),
            ("locales/en-US/ui.ftl", "count = { $n } items"),
            (
                "locales/de/main.ftl.ron",
                r#"(locale: "de", resources: ["ui.ftl"])"#,
            ),
            ("locales/de/ui.ftl", "count = { $n } Dinge"),
        ];
        let mut ui = TestUi::with_pointer("stateful-ui", &files, VIEWPORT)
            .stylesheet("framed.css")
            .spawn(
                "page.html",
                TemplateContext::new().with("n", &self.n),
                // The app's own padding, restored whenever CSS drops it; the
                // column direction apps use (blocks stack vertically, so the
                // pointer geometry is text-width independent).
                Node {
                    padding: UiRect::all(Val::Px(3.0)),
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
            );
        // Preload everything an op may switch to.
        let _: Handle<Stylesheet> = ui.load("plain.css");
        let _: Handle<Stylesheet> = ui.load("broken.css");
        let _: Handle<BundleAsset> = ui.load("locales/en-US/main.ftl.ron");
        let _: Handle<BundleAsset> = ui.load("locales/de/main.ftl.ron");
        ui.settle();
        ui
    }

    async fn run_op(&self, op: &Op, ctxt: &mut TestUi) {
        let root = ctxt.root();
        match *op {
            Op::SetN(n) => {
                ctxt.world_mut()
                    .get_mut::<TemplateContext>(root)
                    .unwrap()
                    .insert("n", &n);
            }
            Op::SetTheme(theme) => {
                let handle: Handle<Stylesheet> = ctxt.load(theme.path());
                ctxt.world_mut()
                    .insert_resource(DefaultStylesheet::new(handle));
            }
            Op::SetOwnSheet(theme) => {
                let handle: Handle<Stylesheet> = ctxt.load(theme.path());
                ctxt.world_mut()
                    .entity_mut(root)
                    .insert(HtmlStylesheet(handle));
            }
            Op::RemoveOwnSheet => {
                ctxt.world_mut().entity_mut(root).remove::<HtmlStylesheet>();
            }
            Op::SetLocale(locale) => {
                let active = match locale {
                    Locale::Raw => ActiveLocale(None),
                    Locale::En => ActiveLocale::new(ctxt.load("locales/en-US/main.ftl.ron")),
                    Locale::De => ActiveLocale::new(ctxt.load("locales/de/main.ftl.ron")),
                };
                ctxt.world_mut().insert_resource(active);
            }
            Op::ToggleOutline => {
                if self.outline {
                    ctxt.world_mut()
                        .entity_mut(root)
                        .remove::<HtmlDebugOutline>();
                } else {
                    ctxt.world_mut().entity_mut(root).insert(HtmlDebugOutline);
                }
            }
            Op::SwapFonts => {
                // `self` is the state before the op: `Spectral` is
                // unregistered (the cascade falls back to the default font);
                // the swap points it at the serif face — a real, loadable
                // font (see the harness setup), so text keeps its metrics.
                let faces = FontFaces::new(SERIF[0].clone());
                ctxt.world_mut()
                    .resource_mut::<FontFamilies>()
                    .insert("Spectral", faces);
            }
            Op::MoveOver => {
                // The same point the model computes, so model and reality
                // can't drift: the live button's center, or anywhere when the
                // outline replaced it.
                let p = match self.button_rect() {
                    Some(rect) => rect.center(),
                    None => Vec2::new(320.0, 240.0),
                };
                ctxt.move_pointer(p);
                ctxt.update(1);
            }
            Op::MoveOutside => {
                // Beyond the viewport: the UI picking backend skips the
                // pointer, so nothing is hovered but the window itself.
                ctxt.move_pointer(Vec2::new(4000.0, 4000.0));
                ctxt.update(1);
            }
            Op::Press => {
                ctxt.press_pointer();
                ctxt.update(1);
            }
            Op::Release => {
                ctxt.release_pointer();
                ctxt.update(1);
            }
        }
        match op {
            Op::MoveOver | Op::MoveOutside | Op::Press | Op::Release => {
                // A frame each, no rebuild to wait for.
            }
            _ => {
                if self.rebuilds(op) {
                    ctxt.settle();
                } else {
                    ctxt.settle_quiet();
                }
            }
        }
    }

    async fn check_postconditions(&self, ctxt: &mut TestUi) {
        let dump = ctxt.dump();
        let (root, run) = self.expected();
        let first = dump.lines().next().unwrap_or_default();
        assert_eq!(
            first, root,
            "root box mismatch (model {self:?})\n--- dump ---\n{dump}"
        );
        if let Some(run) = run {
            assert!(
                dump.contains(&run),
                "missing run {run:?} (model {self:?})\n--- dump ---\n{dump}"
            );
        } else {
            // The outline shows the localized DOM instead of styled runs. The
            // outline text is one Debug-escaped run, so quotes appear as \".
            let marker = match self.locale {
                Locale::Raw => "data-l10n-id=\\\"count\\\"".to_owned(),
                Locale::En => format!("l10n \\\"{} items\\\"", self.n),
                Locale::De => format!("l10n \\\"{} Dinge\\\"", self.n),
            };
            assert!(
                dump.contains(&marker),
                "outline missing {marker:?} (model {self:?})\n--- dump ---\n{dump}"
            );
        }
        // The op's signals: everything recorded since the previous check is
        // exactly what the just-applied op was predicted to emit.
        let mut actual: Vec<Expected> = ctxt
            .drain_log()
            .into_iter()
            .map(|signal| {
                let trigger = match signal.trigger {
                    SignalTrigger::Click => CLICK,
                    // The model only presses the primary button.
                    SignalTrigger::AuxClick => unreachable!("no other buttons"),
                    SignalTrigger::Press => PRESS,
                    SignalTrigger::Release => RELEASE,
                    SignalTrigger::Enter => ENTER,
                    SignalTrigger::Leave => LEAVE,
                };
                (signal.name.to_string(), trigger)
            })
            .collect();
        actual.sort();
        let mut expected = self.pending.clone();
        expected.sort();
        assert_eq!(
            actual, expected,
            "signals mismatch (model {self:?})\n--- dump ---\n{dump}"
        );
    }

    async fn clean_up_test_run(&self, _ctxt: &mut TestUi) {
        // `TestUi`'s Drop removes the temp asset root.
    }
}

#[test]
fn ui_state_machine_matches_model() {
    proptest_stateful::test::<UiModel>(ProptestStatefulConfig {
        min_ops: 1,
        max_ops: 8,
        test_case_timeout: Duration::from_secs(60),
        proptest_config: ProptestConfig {
            cases: 32,
            ..ProptestConfig::default()
        },
    });
}
