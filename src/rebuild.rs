//! When an `HtmlUi` is rebuilt or restyled, and with which stylesheet: a pure
//! decision over one frame's observations, kept apart from the ECS so it can
//! be tested exhaustively. [`build_html_ui`](crate::build::build_html_ui)
//! gathers a [`Frame`] per entity and acts on the [`Decision`].
//!
//! Rules:
//! - The entity's own `HtmlStylesheet` wins when it's ready. A failed one
//!   falls back to the `DefaultStylesheet`; a failed (or no) default means
//!   unstyled. A sheet still loading that would be used defers the update:
//!   the UI never flashes unstyled. So does a document still rendering.
//! - Structure changes (template output, translations, outline) update the
//!   children for the new document; style changes (stylesheets, their
//!   images, fonts) restyle them. Both reconcile in place (`build.rs`); the
//!   decision picks the event and the stylesheet. An entity never built yet builds; when both
//!   kinds are pending, the rebuild wins.
//! - Any change updates, at once or — if it arrives while a needed sheet is
//!   loading — the frame that sheet resolves (ready *or* failed). Asset
//!   failures emit no event, so this deferral is what turns a failure into an
//!   update; and re-requesting an already failed sheet reads as loading for a
//!   frame, so a change seen only then must not be lost.
//! - Default-stylesheet changes count only while the default applies (no
//!   ready override).

use bevy::prelude::*;

/// A stylesheet's load state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Ready,
    Loading,
    Failed,
}

/// One frame's observations for one `HtmlUi`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Frame {
    /// The entity's `HtmlStylesheet`, if it has one.
    pub own: Option<Phase>,
    /// The `DefaultStylesheet`, if set.
    pub default: Option<Phase>,
    /// The template has rendered (successfully or not).
    pub document_ready: bool,
    /// The override was set, replaced, removed or reloaded, or its images
    /// loaded.
    pub own_changed: bool,
    /// The same for the default stylesheet.
    pub default_changed: bool,
    /// `FontFamilies` changed.
    pub fonts_changed: bool,
    /// An element's `:hover`/`:active` state changed: restyle (the styles
    /// are state-dependent, the structure isn't).
    pub state_changed: bool,
    /// The UI's context values for the sheet's `var()` names changed:
    /// restyle (the values style, they don't render).
    pub vars_changed: bool,
    /// An `<img>` inline image of this UI loaded (or reloaded): restyle so
    /// its inline box takes the image's size.
    pub images_changed: bool,
    /// Structure: template output, translations or the debug outline changed.
    pub content_changed: bool,
}

/// Which stylesheet an update uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Own,
    Default,
    Unstyled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    /// Update the children for new content, styled from this source.
    Build(Source),
    /// Restyle the existing children from this source.
    Restyle(Source),
    /// Nothing changed.
    Skip,
    /// A needed stylesheet, or the document, isn't ready.
    Wait,
}

/// The kind of update a change needs; a rebuild covers a restyle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Update {
    Restyle,
    Build,
}

/// Per-entity state: whether it was built, and which update is waiting.
#[derive(Component, Default, Debug)]
pub(crate) struct RebuildState {
    built: bool,
    pending: Option<Update>,
    /// A style-side change is outstanding: observed but not yet applied by
    /// an executed update. Unlike `pending` it survives being merged into
    /// a later `Build`, so the consumer can tell a content-only build from
    /// one that carries style changes (and must not reuse cached specs).
    style_pending: bool,
    /// The context's `--` values' hash as of the last update: `Frame`'s
    /// `vars_changed` compares against it. Only restyled builds record it
    /// (a `Wait` keeps the old hash, so the change outlives the wait).
    pub(crate) vars: u64,
}

impl RebuildState {
    /// Whether a style-side change is outstanding (see the field).
    pub(crate) fn style_pending(&self) -> bool {
        self.style_pending
    }

    /// The executed update applied the styles: cached specs may be reused
    /// again once a fresh render arrives.
    pub(crate) fn style_pending_applied(&mut self) {
        self.style_pending = false;
    }

    pub fn decide(&mut self, frame: Frame) -> Decision {
        let own_ready = frame.own == Some(Phase::Ready);
        let style = frame.own_changed
            || (!own_ready && frame.default_changed)
            || frame.fonts_changed
            || frame.state_changed
            || frame.images_changed
            || frame.vars_changed;
        // The bit is consumed by the consumer when the update runs, not
        // here: a style change seen during a `Wait` must still be visible
        // in the build that finally runs for it.
        self.style_pending |= style;
        let change = if frame.content_changed {
            Some(Update::Build)
        } else if style {
            Some(Update::Restyle)
        } else {
            None
        };
        let pending = self.pending.take().max(change);
        let source = match (frame.own, frame.default) {
            (Some(Phase::Ready), _) => Some(Source::Own),
            (Some(Phase::Loading), _) => None,
            (Some(Phase::Failed) | None, Some(Phase::Ready)) => Some(Source::Default),
            (Some(Phase::Failed) | None, Some(Phase::Loading)) => None,
            (Some(Phase::Failed) | None, Some(Phase::Failed) | None) => Some(Source::Unstyled),
        };
        let (Some(source), true) = (source, frame.document_ready) else {
            self.pending = pending;
            return Decision::Wait;
        };
        match pending {
            None => Decision::Skip,
            // Nothing to restyle yet.
            Some(Update::Build) | Some(Update::Restyle) if !self.built => {
                self.built = true;
                Decision::Build(source)
            }
            Some(Update::Build) => Decision::Build(source),
            Some(Update::Restyle) => Decision::Restyle(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const PHASES: [Option<Phase>; 4] = [
        None,
        Some(Phase::Ready),
        Some(Phase::Loading),
        Some(Phase::Failed),
    ];

    /// The stylesheet an update must use, or `None` while one it depends on
    /// loads (stated per CSS-like fallback, independently of `decide`).
    fn expected_source(own: Option<Phase>, default: Option<Phase>) -> Option<Source> {
        if own == Some(Phase::Ready) {
            return Some(Source::Own);
        }
        if own == Some(Phase::Loading) {
            return None;
        }
        // No override, or it failed: the default decides.
        match default {
            Some(Phase::Ready) => Some(Source::Default),
            Some(Phase::Loading) => None,
            _ => Some(Source::Unstyled),
        }
    }

    /// A state that has built once (so restyles are possible).
    fn built() -> RebuildState {
        let mut state = RebuildState::default();
        let first = state.decide(Frame {
            document_ready: true,
            content_changed: true,
            ..Frame::default()
        });
        assert_eq!(first, Decision::Build(Source::Unstyled));
        state
    }

    /// Every combination of load states, with a structure change: builds
    /// from the right source, or waits exactly when a needed sheet loads.
    #[test]
    fn source_follows_fallback_for_every_phase_combination() {
        for own in PHASES {
            for default in PHASES {
                let decision = built().decide(Frame {
                    own,
                    default,
                    document_ready: true,
                    content_changed: true,
                    ..Frame::default()
                });
                let expected = match expected_source(own, default) {
                    Some(source) => Decision::Build(source),
                    None => Decision::Wait,
                };
                assert_eq!(decision, expected, "own {own:?}, default {default:?}");
            }
        }
    }

    /// No change, nothing pending: never updates, whatever the load states.
    #[test]
    fn nothing_changed_never_updates() {
        for own in PHASES {
            for default in PHASES {
                let decision = built().decide(Frame {
                    own,
                    default,
                    document_ready: true,
                    ..Frame::default()
                });
                assert!(
                    matches!(decision, Decision::Skip | Decision::Wait),
                    "own {own:?}, default {default:?}: {decision:?}"
                );
            }
        }
    }

    /// A context `--` value change restyles; a `Wait` (a needed sheet
    /// loading) keeps the signal so the change outlives the wait.
    #[test]
    fn context_var_changes_restyle_and_survive_a_wait() {
        let mut state = built();
        let decision = state.decide(Frame {
            document_ready: true,
            vars_changed: true,
            ..Frame::default()
        });
        assert_eq!(decision, Decision::Restyle(Source::Unstyled));

        // While a needed sheet loads: wait, and the change is still pending.
        let mut state = built();
        let decision = state.decide(Frame {
            own: Some(Phase::Loading),
            document_ready: true,
            vars_changed: true,
            ..Frame::default()
        });
        assert_eq!(decision, Decision::Wait);
        let decision = state.decide(Frame {
            own: Some(Phase::Ready),
            document_ready: true,
            vars_changed: true,
            ..Frame::default()
        });
        // The wait kept the (style-kind) change: it applies as a restyle,
        // from the sheet that just became ready.
        assert_eq!(decision, Decision::Restyle(Source::Own));
    }

    /// A style change seen during a `Wait` marks the build that finally
    /// resolves it, even when that frame's own signals are content-only:
    /// the reuse gate must not serve cached specs styled by the previous
    /// sheet. The consumer clears the bit when the update runs.
    #[test]
    fn style_change_during_a_wait_marks_the_resolving_build() {
        let mut state = built();
        let wait = state.decide(Frame {
            own: Some(Phase::Loading),
            document_ready: true,
            own_changed: true,
            content_changed: true,
            ..Frame::default()
        });
        assert_eq!(wait, Decision::Wait);
        assert!(state.style_pending());

        // The sheet resolved; this frame only the content changed.
        let decision = state.decide(Frame {
            own: Some(Phase::Ready),
            document_ready: true,
            content_changed: true,
            ..Frame::default()
        });
        assert!(matches!(decision, Decision::Build(_)));
        assert!(state.style_pending());
        state.style_pending_applied();
        assert!(!state.style_pending());

        // Consumed: the next content-only build may reuse again.
        let decision = state.decide(Frame {
            document_ready: true,
            content_changed: true,
            ..Frame::default()
        });
        assert!(matches!(decision, Decision::Build(_)));
        assert!(!state.style_pending());
    }

    /// Style signals restyle a built UI; structure signals rebuild it; both
    /// at once rebuild.
    #[test]
    fn style_changes_restyle_and_structure_changes_rebuild() {
        let ready = Frame {
            default: Some(Phase::Ready),
            document_ready: true,
            ..Frame::default()
        };
        let cases = [
            (
                Frame {
                    default_changed: true,
                    ..ready
                },
                Decision::Restyle(Source::Default),
            ),
            (
                Frame {
                    fonts_changed: true,
                    ..ready
                },
                Decision::Restyle(Source::Default),
            ),
            (
                Frame {
                    own_changed: true,
                    ..ready
                },
                Decision::Restyle(Source::Default),
            ),
            (
                Frame {
                    content_changed: true,
                    ..ready
                },
                Decision::Build(Source::Default),
            ),
            (
                Frame {
                    content_changed: true,
                    fonts_changed: true,
                    ..ready
                },
                Decision::Build(Source::Default),
            ),
        ];
        for (frame, expected) in cases {
            assert_eq!(built().decide(frame), expected, "{frame:?}");
        }
    }

    /// The first update of an entity is a build, even if only style changed
    /// (there's nothing to restyle).
    #[test]
    fn first_update_builds() {
        let frame = Frame {
            default: Some(Phase::Ready),
            document_ready: true,
            default_changed: true,
            ..Frame::default()
        };
        assert_eq!(
            RebuildState::default().decide(frame),
            Decision::Build(Source::Default)
        );
    }

    /// A default-stylesheet change under a ready override changes nothing
    /// (the override wins); under a failed or missing override it restyles.
    #[test]
    fn default_changes_count_only_while_the_default_applies() {
        let frame = |own| Frame {
            own,
            default: Some(Phase::Ready),
            document_ready: true,
            default_changed: true,
            ..Frame::default()
        };
        assert_eq!(built().decide(frame(Some(Phase::Ready))), Decision::Skip);
        for own in [None, Some(Phase::Failed)] {
            assert_eq!(
                built().decide(frame(own)),
                Decision::Restyle(Source::Default),
                "own {own:?}"
            );
        }
    }

    /// bug_0001/0010 shape: a change while the sheet loads, then the sheet
    /// fails with no further signal — the failure itself must update
    /// (unstyled), exactly once. A restyle waiting together with a rebuild
    /// becomes the rebuild.
    #[test]
    fn deferred_changes_update_once_when_the_sheet_resolves() {
        let mut state = built();
        let loading = Frame {
            default: Some(Phase::Loading),
            document_ready: true,
            ..Frame::default()
        };
        assert_eq!(
            state.decide(Frame {
                default_changed: true,
                ..loading
            }),
            Decision::Wait
        );
        assert_eq!(
            state.decide(Frame {
                content_changed: true,
                ..loading
            }),
            Decision::Wait
        );
        assert_eq!(state.decide(loading), Decision::Wait);
        let failed = Frame {
            default: Some(Phase::Failed),
            document_ready: true,
            ..Frame::default()
        };
        assert_eq!(state.decide(failed), Decision::Build(Source::Unstyled));
        assert_eq!(state.decide(failed), Decision::Skip);
    }

    /// Nothing happens before the document has rendered; changes seen
    /// meanwhile update once it has.
    #[test]
    fn waits_for_the_document() {
        let mut state = RebuildState::default();
        let pending = Frame {
            content_changed: true,
            ..Frame::default()
        };
        assert_eq!(state.decide(pending), Decision::Wait);
        assert_eq!(state.decide(Frame::default()), Decision::Wait);
        let ready = Frame {
            document_ready: true,
            ..Frame::default()
        };
        assert_eq!(state.decide(ready), Decision::Build(Source::Unstyled));
    }

    fn phase() -> impl Strategy<Value = Option<Phase>> {
        prop_oneof![
            Just(None),
            Just(Some(Phase::Ready)),
            Just(Some(Phase::Loading)),
            Just(Some(Phase::Failed)),
        ]
    }

    fn frame() -> impl Strategy<Value = Frame> {
        (
            phase(),
            phase(),
            prop::bool::weighted(0.8),
            prop::array::uniform6(any::<bool>()),
        )
            .prop_map(
                |(
                    own,
                    default,
                    document_ready,
                    [
                        own_changed,
                        default_changed,
                        fonts_changed,
                        state_changed,
                        images_changed,
                        content_changed,
                    ],
                )| {
                    Frame {
                        own,
                        default,
                        document_ready,
                        own_changed,
                        default_changed,
                        fonts_changed,
                        state_changed,
                        // The vars signal is a restyle like the others;
                        // modeling it separately would need the hash
                        // bookkeeping, which build.rs owns.
                        vars_changed: images_changed,
                        images_changed,
                        content_changed,
                    }
                },
            )
    }

    proptest! {
        /// Over any frame sequence: updates use the fallback's source, never
        /// happen while a needed sheet loads or the document renders, and
        /// happen exactly on the first possible frame at or after each
        /// relevant change — no change is lost, none is invented. The kind is
        /// a rebuild if any waiting change was structural or nothing was
        /// built yet, else a restyle.
        #[test]
        fn no_change_is_lost_and_none_is_invented(frames in prop::collection::vec(frame(), 1..40)) {
            let mut state = RebuildState::default();
            let (mut waiting_structure, mut waiting_style, mut built) = (false, false, false);
            for (index, frame) in frames.iter().enumerate() {
                waiting_structure |= frame.content_changed;
                waiting_style |= frame.own_changed
                    || frame.fonts_changed
                    || frame.state_changed
                    || frame.images_changed
                    || (frame.own != Some(Phase::Ready) && frame.default_changed);
                let decision = state.decide(*frame);
                match expected_source(frame.own, frame.default) {
                    Some(source) if frame.document_ready => {
                        let expected = if waiting_structure || (waiting_style && !built) {
                            built = true;
                            Decision::Build(source)
                        } else if waiting_style {
                            Decision::Restyle(source)
                        } else {
                            Decision::Skip
                        };
                        prop_assert_eq!(decision, expected, "frame {}", index);
                        waiting_structure = false;
                        waiting_style = false;
                    }
                    _ => prop_assert_eq!(decision, Decision::Wait, "frame {}", index),
                }
            }
        }
    }
}
