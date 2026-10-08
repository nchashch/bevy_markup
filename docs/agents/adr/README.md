# Architecture Decision Records

Short, dated writeups of significant decisions in bevy_markup: the
alternatives considered, why one was picked, and what it cost. They are
separate from [`AGENTS.md`](../../../AGENTS.md), which describes the code's
*current* state and is rewritten as it changes. An ADR stays as written; a
later one supersedes it. Format and rules:
[`docs/agents/skills/adr.md`](../skills/adr.md).

0001–0012 were written retrospectively on 2026-10-06 from the git history
(`59f8f32..d2c9b63`), the `AGENTS.md` text at each commit, the bug ledger,
and the omp session that designed 0007–0012 together with the project owner.
Each record says who made the decision and marks reasoning nobody recorded
as `[INFERENCE]`.

## Index

| # | Title | Status |
|---|-------|--------|
| [0001](./0001-html-tera-fluent-css-onto-bevy-ui.md) | UI as HTML templates (Tera) + Fluent + CSS, built into native Bevy UI entities | Accepted |
| [0002](./0002-library-shape-and-public-api.md) | A minimal-dependency Bevy library with a component/resource API | Accepted |
| [0003](./0003-nine-slice-frames-as-css-border-image.md) | 9-slice frames as CSS `border-image`, not a custom asset format | Accepted |
| [0004](./0004-verification-strategy.md) | Verify against reference implementations, then attack with every kind of generated input | Accepted |
| [0005](./0005-update-decisions-as-a-pure-state-machine.md) | When to rebuild or restyle: a pure state machine, with restyles in place | Accepted (content-rebuild part superseded by [0009](./0009-in-place-update-model.md)) |
| [0006](./0006-declarative-interaction-signals-and-pseudo-classes.md) | Declarative interaction: `data-on-*` signals as messages, `:hover`/`:active` as restyles | Accepted (extended by [0011](./0011-signal-sources-dataset-and-routing.md)) |
| [0007](./0007-css-layout-and-the-root-rule.md) | Layout through CSS — flex, grid, positioning — and the template styling its own root | Accepted |
| [0008](./0008-focus-and-navigation-in-the-library.md) | Browser-style focus and directional navigation in the library; input stays the app's | Accepted |
| [0009](./0009-in-place-update-model.md) | Update in place: skip identical renders, reconcile by key, write only what changed | Accepted |
| [0010](./0010-overlays-anchors-and-tooltips.md) | Overlays: anchored to elements and world points, with tooltips built in | Accepted |
| [0011](./0011-signal-sources-dataset-and-routing.md) | Signals: what produced them, browser button semantics, per-feature `data-*`, routing by name | Accepted |
| [0012](./0012-template-composition-and-examples.md) | Template composition: one Tera set per template, Tera 2 components as the widget library | Accepted |
| [0013](./0013-own-the-fluent-bundle-asset.md) | Own the Fluent bundle asset instead of bevy_fluent | Accepted |

Add new records to this index in the same commit that adds the file.
