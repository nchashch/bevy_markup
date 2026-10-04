//! # p23 — HTML + CSS + Fluent + Tera as Bevy UI
//!
//! Write UI as HTML templates; get Bevy UI nodes.
//!
//! ```text
//! .html (Tera template) ──render(TemplateContext)──▶ HTML ──tl──▶ DOM
//!   ──Fluent (data-l10n-id, ActiveLocale)──▶ localized DOM
//!   ──CSS (DefaultStylesheet / HtmlStylesheet, FontFamilies)──▶ Bevy UI nodes
//! ```
//!
//! ## Quick start
//!
//! ```no_run
//! use bevy::prelude::*;
//! use p23::prelude::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins((DefaultPlugins, HtmlUiPlugin))
//!         .add_systems(Startup, setup)
//!         .run();
//! }
//!
//! fn setup(
//!     mut commands: Commands,
//!     asset_server: Res<AssetServer>,
//!     mut fonts: ResMut<FontFamilies>,
//! ) {
//!     commands.spawn(Camera2d);
//!
//!     // CSS `font-family: Spectral` → these files.
//!     fonts.insert(
//!         "Spectral",
//!         FontFaces::new(asset_server.load("fonts/Spectral-Regular.ttf"))
//!             .with_bold(asset_server.load("fonts/Spectral-Bold.ttf")),
//!     );
//!     // Applies to every `HtmlUi` without its own `HtmlStylesheet`.
//!     commands.insert_resource(DefaultStylesheet::new(asset_server.load("ui/theme.css")));
//!     // `data-l10n-id` attributes resolve against this Fluent bundle.
//!     commands.insert_resource(ActiveLocale::new(
//!         asset_server.load("locales/en-US/main.ftl.ron"),
//!     ));
//!
//!     // One component: the template. `Node`, `TemplateContext`, … are required
//!     // components; override any of them in the same bundle.
//!     commands.spawn((
//!         HtmlUi::new(asset_server.load("ui/hud.html")),
//!         TemplateContext::new().with("player", "Ada").with("coins", &3),
//!         Node { flex_direction: FlexDirection::Column, ..default() },
//!     ));
//! }
//! ```
//!
//! Mutating [`TemplateContext`](html::TemplateContext) re-renders; swapping
//! [`ActiveLocale`](l10n::ActiveLocale) re-localizes; swapping
//! [`DefaultStylesheet`](style::DefaultStylesheet) restyles — all at runtime.
//! After every (re)build an [`HtmlUiBuilt`](html::HtmlUiBuilt) event fires on
//! the `HtmlUi` entity; use [`HtmlElements`](html::HtmlElements) to find
//! elements by `id`/`class` and attach behaviour.
//!
//! ## Supported subset
//!
//! - **HTML** ([`html`]): blocks `h1`–`h6`, `p`, `li`, `pre`, loose text; any
//!   other element is inline text (inside a block) or a container (outside).
//!   Each block becomes a `Text` node with a `TextSpan` per styled run and an
//!   [`HtmlElement`](html::HtmlElement) component (tag, id, classes).
//! - **CSS** ([`style`]): type, `.class`, `#id` and compound selectors with
//!   specificity; `color`, `font-family`, `font-size`,
//!   `font-weight`, `font-style` (inherited); `border-image` (9-slice),
//!   `border-width`, `padding` on blocks and on the `HtmlUi` node (`html`
//!   rule); `background-color` (blocks).
//! - **Fluent** ([`l10n`]): `data-l10n-id` / `data-l10n-args` (fluent-dom
//!   convention); translations may contain inline markup.
//! - **Tera** ([`template`](mod@template)): full Tera 2 syntax in `.html` files, rendered with
//!   the entity's [`TemplateContext`](html::TemplateContext).
//! - **9-slice frames**: in CSS via `border-image` (see [`style`]), or for
//!   nodes outside HTML via `*.slice.ron` assets and
//!   [`NineSliceFrame`](nine_slice::NineSliceFrame) ([`nine_slice`]).
//!
//! ## Cargo features
//!
//! - `system_fonts`: fall back to installed system fonts per script (e.g. CJK)
//!   when the chosen font lacks glyphs.

use bevy::prelude::*;
use bevy::ui::UiSystems;
use bevy_fluent::FluentPlugin;

mod build;
mod cascade;
pub mod fonts;
pub mod html;
pub mod l10n;
pub mod nine_slice;
pub mod style;
pub mod template;

/// Dependencies whose types appear in this crate's API.
pub use {bevy_fluent, lightningcss, tera, tl};

/// Everything needed to build HTML UIs: `use p23::prelude::*;`.
pub mod prelude {
    pub use crate::fonts::{FontFaces, FontFamilies, GenericFamily};
    pub use crate::html::{
        HtmlDebugOutline, HtmlElement, HtmlElements, HtmlUi, HtmlUiBuilt, RenderedHtml,
        TemplateContext,
    };
    pub use crate::l10n::ActiveLocale;
    pub use crate::nine_slice::{NineSlice, NineSliceFrame};
    pub use crate::style::{DefaultStylesheet, HtmlStylesheet, Stylesheet};
    pub use crate::template::HtmlTemplate;
    pub use crate::{HtmlUiPlugin, HtmlUiSystems};
    pub use bevy_fluent::BundleAsset;
}

/// Adds the asset loaders (`.html`, `.css`, `*.slice.ron`, Fluent's
/// `*.ftl.ron`), the [`DefaultStylesheet`](style::DefaultStylesheet),
/// [`ActiveLocale`](l10n::ActiveLocale) and
/// [`FontFamilies`](fonts::FontFamilies) resources, and the systems that turn
/// [`HtmlUi`](html::HtmlUi) entities into Bevy UI.
///
/// Adds bevy_fluent's `FluentPlugin` unless the app already did.
#[derive(Default)]
pub struct HtmlUiPlugin;

/// Pipeline stages, in `PostUpdate` before Bevy UI layout (chained in this
/// order). Order your systems against these to see a stage's output the same
/// frame.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HtmlUiSystems {
    /// Tera renders templates and `tl` parses them into [`html::RenderedHtml`].
    Render,
    /// Fluent resolves `data-l10n-id` elements against [`l10n::ActiveLocale`].
    Localize,
    /// The DOM is styled with CSS and spawned as Bevy UI children; then
    /// [`html::HtmlUiBuilt`] fires.
    Build,
}

impl Plugin for HtmlUiPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FluentPlugin>() {
            app.add_plugins(FluentPlugin);
        }
        app.init_asset::<template::HtmlTemplate>()
            .init_asset_loader::<template::HtmlTemplateLoader>()
            .init_asset::<style::Stylesheet>()
            .init_asset_loader::<style::StylesheetLoader>()
            .init_asset::<nine_slice::NineSlice>()
            .init_asset_loader::<nine_slice::NineSliceLoader>()
            .init_resource::<style::DefaultStylesheet>()
            .init_resource::<l10n::ActiveLocale>()
            .init_resource::<fonts::FontFamilies>()
            .configure_sets(
                PostUpdate,
                (
                    HtmlUiSystems::Render,
                    HtmlUiSystems::Localize,
                    HtmlUiSystems::Build,
                )
                    .chain()
                    .before(UiSystems::Prepare),
            )
            .add_systems(
                PostUpdate,
                (
                    html::render_templates.in_set(HtmlUiSystems::Render),
                    l10n::localize.in_set(HtmlUiSystems::Localize),
                    build::build_html_ui.in_set(HtmlUiSystems::Build),
                    nine_slice::apply_nine_slices.before(UiSystems::Prepare),
                ),
            );
    }
}
