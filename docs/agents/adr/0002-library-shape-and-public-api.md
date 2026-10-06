# 2. A minimal-dependency Bevy library with a component/resource API

| Field | Content |
|---|---|
| ADR | 0002 |
| Title | A minimal-dependency Bevy library with a component/resource API |
| Date | 2026-10-06 06:28 +0400 |
| Author | Claude Opus 5.5 (Anthropic), via omp — retrospective record; the decisions are the commit author's (Nikita Chashchinskii) |
| Commit | `d2c9b63` Implement more features |
| Span | `280d442`, `c4bc132`, `e4fec30`, `5df7544`, `e3e99a6`, `668e810`, `be64be4`, `0072d5f`, `955e6ea` (2026-10-04 – 05) |
| Status | Accepted |
| Related | 0001; UPSTREAM U1, U2; `bug_0005`, `bug_0015` |

## Context

After 0001 the pipeline lived inside a demo binary. To be reused (its first
user became prototype_19), it had to be a library whose dependencies don't
impose choices on the app: Bevy apps choose their renderer, windowing,
picking and image formats.

## Decision

1. **Library + examples** (`280d442`): the crate is a library; demos are
   `examples/`. The library spawns no cameras and sets no `ClearColor`.
2. **Minimal Bevy features** (`c4bc132`): `bevy` with `default-features =
   false` and only what the code uses; examples get full Bevy through
   `[dev-dependencies]`. `cargo check --lib` catches a missing feature. Font
   fallback to system fonts (CJK) is an opt-in `system_fonts` feature.
3. **Ergonomic ECS API** (`e4fec30`, table in `AGENTS.md` "Public API"):
   - one plugin; pipeline stages as `HtmlUiSystems::{Render, Localize,
     Build}` in `PostUpdate`;
   - an `HtmlUi(Handle<HtmlTemplate>)` component whose required components
     (`Node`, `TemplateContext`, `RenderedHtml`, `LocalizedText`) can be
     overridden in the same bundle;
   - global configuration as resources (`DefaultStylesheet`, `ActiveLocale`,
     `FontFamilies`), per-entity overrides as components (`HtmlStylesheet`);
   - events on the root for app wiring (`HtmlUiBuilt`, later
     `HtmlUiRestyled`).
4. **Name and licensing**: renamed `p23` → `bevy_markup` (`5df7544`),
   MIT OR Apache-2.0 (`e3e99a6`), crates.io metadata (`668e810`), plugin
   renamed `HtmlUiPlugin` → `BevyMarkupPlugin` with release 0.2.0 (`0072d5f`).
5. **Patched dependency via git, not vendoring**: the fuzzers found two
   `fluent-syntax` crashes (bug_0005, bug_0015; UPSTREAM U1, U2); the fix was
   first vendored (`132f30d`), then moved to a git fork wired with
   `[patch.crates-io]` (`be64be4`, −5,252 lines).
6. **System fonts in the examples** (`955e6ea`): example assets moved to
   `examples/assets/` and fonts come from the OS (`FontSource::Serif`, …)
   instead of bundled files.

## Alternatives considered

- **Full default Bevy features**: forces rendering/windowing choices on
  every user and slows builds [INFERENCE from the `c4bc132` AGENTS.md text:
  "rendering, windowing and platform features are the app's choice"].
- **A builder/function API instead of components**: not recorded. The
  component shape matches Bevy's own UI widgets [INFERENCE].
- **Keeping the vendored `fluent-syntax`**: replaced by the git fork to drop
  5,000 vendored lines (`be64be4`).

## Consequences

- Users of `bevy_markup` (prototype_19 since its ADR 0015) depend on a small
  feature set; the examples double as integration tests of that claim.
- The `[patch.crates-io]` for `fluent-syntax` must be repeated by every
  workspace using bevy_markup until upstream merges the fixes (prototype_19's
  root `Cargo.toml` carries it).
- `HtmlUiBuilt` was documented as "children are replaced every time, so wire
  behaviour here" — true until ADR 0009 changed the update model, which made
  that advice harmful (bug_0023).
