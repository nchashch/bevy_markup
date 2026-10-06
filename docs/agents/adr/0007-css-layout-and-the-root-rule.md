# 7. Layout through CSS — flex, grid, positioning — and the template styling its own root

| Field | Content |
|---|---|
| ADR | 0007 |
| Title | Layout through CSS — flex, grid, positioning — and the template styling its own root |
| Date | 2026-10-06 06:28 +0400 |
| Author | Claude Opus 5.5 (Anthropic), via omp — retrospective record; `9d21b17`/`5113f7c` are the commit author's (Nikita Chashchinskii) decisions; `decd505`, `ebfbf22` and the `overflow` part of `c8ab00f` were designed in an omp session with Claude Opus 5.5 for prototype_19 and committed by the author |
| Commit | `d2c9b63` Implement more features |
| Span | `025e685`, `9d21b17`, `5113f7c`, `decd505`, `ebfbf22`, `c8ab00f` (2026-10-04 – 06) |
| Status | Accepted |
| Related | 0001, 0004, 0009; `bug_0021`, `bug_0023`; prototype_19 ADR 0015, playtests 0024, 0028 |

## Context

At `30570ae` containers were fixed vertical columns; layout lived in Rust
(the `HtmlUi`'s `Node` and app code). Moving a real game UI (prototype_19)
onto bevy_markup showed what remained hand-written. Every overlay root's
placement, size, `GlobalZIndex` and `Pickable` lived in spawn code. Rounded
corners, borders with colours and unpickable layers were implemented with
app-side loops (prototype_19 playtest 0024).

## Decision

1. **Flex and sizing from CSS** (`9d21b17`, demo rewritten on CSS layout):
   `display`, `flex-*`, `justify-*`/`align-*`, sizes, margins and
   `box-sizing` map onto Bevy's `Node`. Defaults follow CSS's content-box,
   which the layout oracle caught (ADR 0004).
2. **Grid** (`5113f7c`): track lists, `repeat(auto-fill, minmax())`, line and
   span placement, `grid-auto-flow` onto Bevy's grid fields, laid out by
   taffy.
3. **Element-level properties** (`decd505`): `position` with insets,
   `z-index` → `ZIndex`, `border-radius`, `border-color`, and inherited
   `pointer-events: none` → `Pickable::IGNORE`. `CssOwned` records what CSS
   set, so a restyle without the declaration resets only that and leaves an
   app's own values alone.
4. **The root rule** (`ebfbf22`): the `HtmlUi` entity itself is styled by the
   `html` rule plus the template's own `<html id class>`. That covers
   layout, position and insets, `z-index`, `pointer-events`, background,
   border and `border-image`. `CssRoot` keeps the app's `Node` values: what
   CSS stops declaring goes back, and fields CSS never declared are never
   touched, so an app can still move the root every frame.
5. **`overflow`** (`c8ab00f`): `overflow`, `-x`, `-y`, where `auto` scrolls
   because Bevy has no scroll-if-needed. Added because an app writing
   `Node.overflow` itself was undone by every in-place update (bug_0023,
   ADR 0009).

## Alternatives considered

- **`GlobalZIndex` from CSS `z-index` on roots** — rejected: Bevy orders
  roots by `(GlobalZIndex, ZIndex)`, and plain `ZIndex` keeps a nested UI
  from escaping its parent's stacking (session decision; also why modal
  selection compares both, `focus.rs` `sync_navigation`).
- **Leaving the root to the app** — the previous state; the spawn code
  duplicated what the template already knew (prototype_19 removed five
  constants and nine spawn-site `Node`s, playtest 0028).

## Consequences

- prototype_19's `theme.css` "Roots" section holds every surface's placement
  and stacking (pause 100/101, popup 900, tooltip 1000).
- The root rule has no `:hover`/`:focus` state (always the default pseudo
  state); documented in `AGENTS.md` "Limits".
- Smoke-testing the root rule in prototype_19 exposed bug_0021 (a same-frame
  despawn panicked `update_pseudo_states`), fixed with `try_insert`.
- Still unsupported and documented: combinators, `position: fixed/sticky`,
  `gap` in `%`, font-relative lengths for layout.
