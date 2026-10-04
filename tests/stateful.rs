//! Stateful property tests (`proptest_stateful`): random *sequences* of
//! runtime mutations applied to one long-lived `HtmlUi`, each checked against
//! a reference model right after it settles.
//!
//! The modeled state — default theme, the entity's own `HtmlStylesheet`,
//! locale, context value, outline marker, registered font faces — is exactly
//! the state that decides the dump. The interesting invariants are the ones
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
use common::{MONO, SERIF, TestUi};

use async_trait::async_trait;
use bevy::prelude::*;
use p23::bevy_fluent::BundleAsset;
use p23::prelude::*;
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

/// The modeled state of the `HtmlUi`. `Default` doubles as the state the
/// harness initializes (default theme framed, no own sheet, no locale,
/// `n = 0`, `Spectral` = the serif faces).
#[derive(Clone, Debug, Default)]
struct UiModel {
    theme: Theme,
    /// The entity's `HtmlStylesheet`, if any.
    own: Option<Theme>,
    locale: Locale,
    n: i64,
    outline: bool,
    /// `Spectral` re-registered with the mono face.
    fonts_swapped: bool,
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
            Theme::Framed => "html-ui border=16,16,16,16 padding=10,10,10,10 slice=frame.png 4,4,4,4 stretch",
            Theme::Plain => "html-ui padding=4,4,4,4",
            // Unstyled: the app's own 3px padding is restored.
            Theme::Broken => "html-ui padding=3,3,3,3",
        };
        let text = match self.locale {
            Locale::Raw => self.n.to_string(),
            Locale::En => format!("{} items", self.n),
            Locale::De => format!("{} Dinge", self.n),
        };
        // Both real themes ask for `Spectral`.
        let face = if self.fonts_swapped { "mono" } else { "serif" };
        let style = match self.effective() {
            Theme::Framed => format!("{face} 20px #ffffff"),
            Theme::Plain => format!("{face} 12px #00ff00"),
            Theme::Broken => "default 16px #ffffff".to_owned(),
        };
        let run = (!self.outline).then(|| format!("{text:?} {style}"));
        (root.to_owned(), run)
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
        ]
    }

    /// Only state-*changing* ops are valid: a same-value re-insert (e.g.
    /// `ActiveLocale(None)` while already unlocalized) legitimately triggers
    /// no rebuild, and `settle()` demands observable activity per op.
    fn preconditions_met(&self, op: &Op) -> bool {
        match *op {
            Op::SetN(n) => n != self.n,
            Op::SetTheme(theme) => theme != self.theme,
            Op::SetOwnSheet(theme) => self.own != Some(theme),
            Op::RemoveOwnSheet => self.own.is_some(),
            Op::SetLocale(locale) => locale != self.locale,
            Op::ToggleOutline | Op::SwapFonts => true,
        }
    }

    fn next_state(&mut self, op: &Op) {
        match *op {
            Op::SetN(n) => self.n = n,
            Op::SetTheme(theme) => self.theme = theme,
            Op::SetOwnSheet(theme) => self.own = Some(theme),
            Op::RemoveOwnSheet => self.own = None,
            Op::SetLocale(locale) => self.locale = locale,
            Op::ToggleOutline => self.outline = !self.outline,
            Op::SwapFonts => self.fonts_swapped = !self.fonts_swapped,
        }
    }

    async fn init_test_run(&self) -> TestUi {
        let files = [
            ("page.html", r#"<p data-l10n-id="count" data-l10n-args='{"n": {{ n }}}'>{{ n }}</p>"#),
            ("framed.css", r#"html { color: #ffffff; font-family: Spectral; font-size: 20px; border-image: url("frame.png") 4 fill stretch; border-width: 16px; padding: 10px }"#),
            ("plain.css", "html { color: #00ff00; font-family: Spectral; font-size: 12px; padding: 4px }"),
            ("broken.css", "{ not css"),
            ("locales/en-US/main.ftl.ron", r#"(locale: "en-US", resources: ["ui.ftl"])"#),
            ("locales/en-US/ui.ftl", "count = { $n } items"),
            ("locales/de/main.ftl.ron", r#"(locale: "de", resources: ["ui.ftl"])"#),
            ("locales/de/ui.ftl", "count = { $n } Dinge"),
        ];
        let mut ui = TestUi::new("stateful-ui", &files)
            .stylesheet("framed.css")
            .spawn(
                "page.html",
                TemplateContext::new().with("n", &self.n),
                // The app's own padding, restored whenever CSS drops it.
                Node { padding: UiRect::all(Val::Px(3.0)), ..default() },
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
                ctxt.world_mut().insert_resource(DefaultStylesheet::new(handle));
            }
            Op::SetOwnSheet(theme) => {
                let handle: Handle<Stylesheet> = ctxt.load(theme.path());
                ctxt.world_mut().entity_mut(root).insert(HtmlStylesheet(handle));
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
                    ctxt.world_mut().entity_mut(root).remove::<HtmlDebugOutline>();
                } else {
                    ctxt.world_mut().entity_mut(root).insert(HtmlDebugOutline);
                }
            }
            Op::SwapFonts => {
                // `self` is the state before the op.
                let faces = if self.fonts_swapped {
                    FontFaces::new(SERIF[0].clone())
                        .with_bold(SERIF[1].clone())
                        .with_italic(SERIF[2].clone())
                        .with_bold_italic(SERIF[3].clone())
                } else {
                    FontFaces::new(MONO.clone())
                };
                ctxt.world_mut().resource_mut::<FontFamilies>().insert("Spectral", faces);
            }
        }
        if self.rebuilds(op) {
            ctxt.settle();
        } else {
            ctxt.settle_quiet();
        }
    }

    async fn check_postconditions(&self, ctxt: &mut TestUi) {
        let dump = ctxt.dump();
        let (root, run) = self.expected();
        let first = dump.lines().next().unwrap_or_default();
        assert_eq!(first, root, "root box mismatch (model {self:?})\n--- dump ---\n{dump}");
        if let Some(run) = run {
            assert!(dump.contains(&run), "missing run {run:?} (model {self:?})\n--- dump ---\n{dump}");
        } else {
            // The outline shows the localized DOM instead of styled runs. The
            // outline text is one Debug-escaped run, so quotes appear as \".
            let marker = match self.locale {
                Locale::Raw => "data-l10n-id=\\\"count\\\"".to_owned(),
                Locale::En => format!("l10n \\\"{} items\\\"", self.n),
                Locale::De => format!("l10n \\\"{} Dinge\\\"", self.n),
            };
            assert!(dump.contains(&marker), "outline missing {marker:?} (model {self:?})\n--- dump ---\n{dump}");
        }
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
