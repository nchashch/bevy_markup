//! Stateful property tests (`proptest_stateful`): random *sequences* of
//! runtime mutations applied to one long-lived `HtmlUi`, each checked against
//! a reference model right after it settles.
//!
//! The modeled state — theme, locale, context value, outline marker — is
//! exactly the state that decides the dump. The interesting invariants are
//! the ones pairwise tests can't reach: the `html` rule's box properties
//! must survive *arbitrary* theme chains (framed → plain → broken → …) with
//! the app's own `Node` restored whenever no CSS provides them, and a broken
//! stylesheet must never wedge the pipeline (the `FailedSheets` latch).
//!
//! `TestUi` (see `common`) is the harness; each test case runs its own app.

mod common;
use common::TestUi;

use async_trait::async_trait;
use bevy::prelude::*;
use p23::bevy_fluent::BundleAsset;
use p23::prelude::*;
use proptest::prelude::*;
use proptest_stateful::{ModelState, ProptestStatefulConfig};
use std::time::Duration;

/// Which stylesheet the `DefaultStylesheet` resource points at.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Theme {
    #[default]
    Framed,
    Plain,
    /// Fails to parse: the UI must render unstyled, not blank.
    Broken,
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
/// harness initializes (theme framed, no locale, `n = 0`).
#[derive(Clone, Debug, Default)]
struct UiModel {
    theme: Theme,
    locale: Locale,
    n: i64,
    outline: bool,
}

#[derive(Clone, Debug)]
enum Op {
    SetN(i64),
    SetTheme(Theme),
    SetLocale(Locale),
    ToggleOutline,
}

impl UiModel {
    /// The dump we expect right now: the root line (theme decides the `html`
    /// rule's box) and the text run (locale decides the text, theme the
    /// styling), the latter only when the outline hasn't replaced content.
    fn expected(&self) -> (String, Option<String>) {
        let root = match self.theme {
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
        let style = match self.theme {
            Theme::Framed => "default 20px #ffffff",
            Theme::Plain => "default 12px #00ff00",
            Theme::Broken => "default 16px #ffffff",
        };
        let run = (!self.outline).then(|| format!("{text:?} {style}"));
        (root.to_owned(), run)
    }
}

#[async_trait(?Send)]
impl ModelState for UiModel {
    type Operation = Op;
    type RunContext = TestUi;
    type OperationStrategy = BoxedStrategy<Op>;

    fn op_generators(&self) -> Vec<BoxedStrategy<Op>> {
        vec![
            (0i64..1000).prop_map(Op::SetN).boxed(),
            Just(Theme::Framed).prop_map(Op::SetTheme).boxed(),
            Just(Theme::Plain).prop_map(Op::SetTheme).boxed(),
            Just(Theme::Broken).prop_map(Op::SetTheme).boxed(),
            Just(Locale::En).prop_map(Op::SetLocale).boxed(),
            Just(Locale::De).prop_map(Op::SetLocale).boxed(),
            Just(Locale::Raw).prop_map(Op::SetLocale).boxed(),
            Just(()).prop_map(|_| Op::ToggleOutline).boxed(),
        ]
    }

    /// Only state-*changing* ops are valid: a same-value re-insert (e.g.
    /// `ActiveLocale(None)` while already unlocalized) legitimately triggers
    /// no rebuild, and `settle()` demands observable activity per op.
    fn preconditions_met(&self, op: &Op) -> bool {
        match *op {
            Op::SetN(n) => n != self.n,
            Op::SetTheme(theme) => theme != self.theme,
            Op::SetLocale(locale) => locale != self.locale,
            Op::ToggleOutline => true,
        }
    }

    fn next_state(&mut self, op: &Op) {
        match *op {
            Op::SetN(n) => self.n = n,
            Op::SetTheme(theme) => self.theme = theme,
            Op::SetLocale(locale) => self.locale = locale,
            Op::ToggleOutline => self.outline = !self.outline,
        }
    }

    async fn init_test_run(&self) -> TestUi {
        let files = [
            ("page.html", r#"<p data-l10n-id="count" data-l10n-args='{"n": {{ n }}}'>{{ n }}</p>"#),
            ("framed.css", r#"html { color: #ffffff; font-size: 20px; border-image: url("frame.png") 4 fill stretch; border-width: 16px; padding: 10px }"#),
            ("plain.css", "html { color: #00ff00; font-size: 12px; padding: 4px }"),
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
        match *op {
            Op::SetN(n) => {
                let root = ctxt.root();
                ctxt.world_mut()
                    .get_mut::<TemplateContext>(root)
                    .unwrap()
                    .insert("n", &n);
            }
            Op::SetTheme(theme) => {
                let path = match theme {
                    Theme::Framed => "framed.css",
                    Theme::Plain => "plain.css",
                    Theme::Broken => "broken.css",
                };
                let handle: Handle<Stylesheet> = ctxt.load(path);
                ctxt.world_mut().insert_resource(DefaultStylesheet::new(handle));
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
                let root = ctxt.root();
                if self.outline {
                    ctxt.world_mut().entity_mut(root).remove::<HtmlDebugOutline>();
                } else {
                    ctxt.world_mut().entity_mut(root).insert(HtmlDebugOutline);
                }
            }
        }
        ctxt.settle();
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
        max_ops: 6,
        test_case_timeout: Duration::from_secs(60),
        proptest_config: ProptestConfig {
            cases: 24,
            ..ProptestConfig::default()
        },
    });
}
