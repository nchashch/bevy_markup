# 13. Own the Fluent bundle asset instead of bevy_fluent

| Field | Content |
|---|---|
| ADR | 0013 |
| Title | Own the Fluent bundle asset instead of bevy_fluent |
| Date | 2026-10-08 11:35 +0400 |
| Author | GLM-5.3-Flash (Z.ai), via omp, at the project owner's request |
| Commit | `461f051` Release version 0.3.0 + uncommitted drop bevy_fluent (LocaleBundle asset + loader) |
| Status | Accepted |
| Related | 0001, 0002; UPSTREAM U1, U2; `bug_0005`, `bug_0015` |

## Context

Since 0001, bevy_markup's Fluent path is: `data-l10n-id` attributes resolve
against one locale bundle pointed at by the `ActiveLocale` resource. The
bundle itself was bevy_fluent 0.15's `BundleAsset` — a `*.ftl.ron`/`*.ftl.yaml`
manifest (`locale`, `resources`) whose loader fetches each `.ftl` as a
`ResourceAsset` and builds a `FluentBundle` behind an `Arc`. bevy_markup
registered `FluentPlugin` (`if !app.is_plugin_added::<FluentPlugin>()`), put
`Handle<BundleAsset>` in `ActiveLocale`, and re-exported both the crate
(`pub use bevy_fluent`) and `BundleAsset` in the prelude.

An inventory of that coupling: every Fluent call in the library
(`get_message`, `pattern`, `format_pattern`, `FluentArgs` parsing) goes
through the `fluent` crate directly, which was already a dependency; of
bevy_fluent only `BundleAsset` and `FluentPlugin`'s two loaders are used.
`Locale`, `Localization`, `LocalizationBuilder`, `fluent-langneg` and
`fluent_content` are not.

The cost: bevy_fluent is Bevy-version-coupled (0.15 tracks Bevy 0.19), so
every Bevy bump needs a matching bevy_fluent release — the README
compatibility table carries a `bevy_fluent` column and `Cargo.toml` carried
the comment "`fluent` must match bevy_fluent's version". And its dependency
graph drags crates bevy_markup never uses: `serde_yaml` (unmaintained),
`fluent-langneg`, `fluent_content`, plus `uuid`, `indexmap`, `futures-lite`.

## Decision

1. **Own the bundle asset** (`src/l10n.rs`): `LocaleBundle` wraps
   `Arc<fluent::concurrent::FluentBundle<Arc<FluentResource>>>` behind a
   `Deref` — the same shape as bevy_fluent's `BundleAsset`. The concurrent
   memoizer is required for the asset to be `Send + Sync`; `translate`/`resolve_all`
   stay generic over `R: Borrow<FluentResource>, M: MemoizerKind`, so the
   plain `FluentBundle<FluentResource>` the unit tests build works unchanged.
2. **Own the loader**: `LocaleBundleLoader` (`*.ftl.ron` only) reads the same
   manifest schema — `(locale: "en-US", resources: ["ui.ftl"])` — so existing
   bundle files need no edits. Each resource is read with
   `LoadContext::read_asset_bytes` (the template loader's pattern), making the
   `.ftl` files dependencies: the file watcher reloads bundles that use an
   edited file, and `LoadedWithDependencies`/`Modified` drive re-localization
   exactly as before (`l10n::localize` is untouched apart from the type). FTL
   parse errors are logged and the resource still loads (Fluent keeps the
   valid messages); `add_resource` overrides (duplicate ids across
   resources) are logged — the earlier message wins, as bevy_fluent's
   behavior.
3. **Cutover**: `ActiveLocale(Option<Handle<LocaleBundle>>)`, prelude
   exports `LocaleBundle`, `pub use fluent` replaces `pub use bevy_fluent`,
   `BevyMarkupPlugin` registers the loader unconditionally (the
   `is_plugin_added` dance is gone), `unic-langid` becomes a required
   dependency (the manifest's `locale` field; it was already in the tree via
   fluent-bundle and a direct dependency of the `fuzzing` feature). The
   internal `unic-langid` cargo feature is removed, and `.ftl.yaml`/`.ftl.yml`
   (nothing in the repo uses them) is not carried over.

## Alternatives considered

- **Keep bevy_fluent**: keeps the version coupling (its Bevy support is
  single-maintainer; a Bevy bump waits on a bevy_fluent release) and the
  unused dependency weight. Its distinctive features don't fit anyway:
  locale negotiation lives in the `Localization` component (entity-scoped),
  while bevy_markup's model is one resource with one handle per app.
- **Depend on bevy_fluent without re-exporting it**: still compiles
  `serde_yaml` and friends and keeps the version sync; only the API
  readability changes.
- **Keep bevy_fluent behind an opt-in feature**: a feature can't hide the
  type rename apps compile against; the coupling would remain in the default
  build.

## Consequences

- Breaking rename for users: `Handle<bevy_fluent::BundleAsset>` →
  `Handle<bevy_markup::LocaleBundle>` (mechanical, ~15 call sites in this
  repo's tests and examples; asset paths and manifests unchanged). Recorded
  in CHANGELOG under [Unreleased] as breaking.
- The dependency tree drops bevy_fluent and with it `serde_yaml` and
  `fluent_content` (`fluent-langneg` stays: fluent-bundle 0.15 depends on
  it) — Cargo.lock after the change. The README compatibility table loses
  its `bevy_fluent` column.
- The fluent-syntax patch story (U1/U2, `[patch.crates-io]`) is unchanged in
  substance — 0.11 is what fluent 0.16's fluent-bundle requires — and no
  longer phrased against bevy_fluent's version; downstream still needs the
  same `[patch]` entry (README).
- ~150 lines of loader + manifest code and its semantics are ours (fourth
  asset after template/stylesheet/nine-slice, same house pattern); the
  behavior contract with `rebuild`/`localize` is unchanged, covered by the
  existing vectors (`html_ui` Fluent tests, `stateful`, `golden`).
- Still open, unchanged: no app-facing configuration of
  `FluentBundle::set_formatter` (locale-grouped number formatting); with the
  loader owned, adding it is now one loader option away.
