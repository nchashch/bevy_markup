//! # bevy_markup — HTML + CSS + Fluent + Tera as Bevy UI
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
//! use bevy_markup::prelude::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins((DefaultPlugins, BevyMarkupPlugin))
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
//! Mutating [`TemplateContext`](html::TemplateContext) re-renders — and
//! updates the UI only if the rendered HTML changed, in place: elements still
//! in the document (matched by `id`, else by position) keep their entities,
//! interaction state and app components, so templated per-frame values
//! (`style="width: {{ hp }}%"`) are fine; swapping
//! [`ActiveLocale`](l10n::ActiveLocale) re-localizes; swapping
//! [`DefaultStylesheet`](style::DefaultStylesheet) restyles — all at runtime.
//! After every content update an [`HtmlUiBuilt`](html::HtmlUiBuilt) event
//! fires on the `HtmlUi` entity, and [`HtmlElements`](html::HtmlElements)
//! finds elements by `id`/`class`. Kept elements keep what was attached to
//! them, so declare behaviour in the template rather than attaching it per
//! build: `data-on-click` signals for interactions, and
//! [custom elements](custom_elements) for components and observers (run
//! once per element). Style-only changes
//! (stylesheets, fonts) restyle the existing children in place and fire
//! [`HtmlUiRestyled`](html::HtmlUiRestyled) instead, keeping what you attached.
//!
//! ## Supported subset
//!
//! - **HTML** ([`html`]): blocks `h1`–`h6`, `p`, `li`, `pre`, loose text;
//!   containers `div`, `section`, `ul`, … as nested column nodes; other
//!   elements are inline text (inside a block) or walked through. Each block
//!   becomes a `Text` node with a `TextSpan` per styled run; blocks and
//!   containers carry an [`HtmlElement`](html::HtmlElement) (tag, id, classes).
//! - **CSS** ([`style`]): type, `.class`, `#id` and compound selectors with
//!   specificity, plus the interaction pseudo-classes `:hover`/`:active`
//!   (state comes from picking; a change restyles in place); `color`,
//!   `font-family`, `font-size`,
//!   `font-weight`, `font-style`, `pointer-events` (inherited); `border-image` (9-slice),
//!   `border-width`, `padding`, `background-color`, `gap`, flex and grid
//!   layout, sizes, margins, `box-sizing`, `position` with insets,
//!   `z-index`, `border-radius`, `border-color` and `opacity` (fades the
//!   subtree) on blocks, containers and
//!   the `HtmlUi` node itself (the `html` rule, or the template's own
//!   `<html class="…">`; what it stops declaring goes back to the app's
//!   values).
//! - **Fluent** ([`l10n`]): `data-l10n-id` / `data-l10n-args` /
//!   `data-l10n-name` (fluent-dom convention) on any element; translations
//!   may contain inline markup.
//! - **Tera** ([`template`](mod@template)): full Tera 2 syntax in `.html` files, rendered with
//!   the entity's [`TemplateContext`](html::TemplateContext). Templates
//!   compose: `{% extends %}` and `{% include %}` take paths relative to the
//!   template's file, and an included file's `{% component %}`s are usable
//!   in the whole template (`{{ <ui.button … /> }}`) — include a component
//!   library to share widgets.
//! - **Interaction signals** ([`signals`]): elements declare hooks with
//!   `data-on-click`/`-auxclick`/`-press`/`-release`/`-enter`/`-leave`
//!   naming an app-side signal (`click` is the primary button, `auxclick`
//!   the others, as in browsers), and `data-with` carries JSON data rendered
//!   with the template's context. Each signal carries its
//!   [`SignalSource`](signals::SignalSource): pointer and button (mouse,
//!   touch, custom pointers such as VR lasers) with its position, the
//!   hovering pointer, or the input that activated the focused element. Interactions arrive as one buffered
//!   [`ElementSignal`](signals::ElementSignal) message. Route them by name
//!   to systems with
//!   [`on_html_click`](signals::HtmlSignalsExt::on_html_click) /
//!   [`on_html_signal`](signals::HtmlSignalsExt::on_html_signal), or drain
//!   the message with `MessageReader`:
//!
//!   ```no_run
//!   # use bevy::prelude::*;
//!   # use bevy_markup::prelude::*;
//!   fn buy(mut signals: MessageReader<ElementSignal>) {
//!       for signal in signals.read() {
//!           if signal.name == "buy" {
//!               info!("buying {:?}", signal.payload["item"]);
//!           }
//!       }
//!   }
//!   ```
//!
//!   Nested hooks: the deepest bound element under the pointer wins.
//! - **Custom elements** ([`custom_elements`]): `is="<name>"` on a block or
//!   container runs the system the app defined with
//!   [`define_html_element`](custom_elements::HtmlCustomElementsExt::define_html_element)
//!   each time the element is spawned, with its `data-*` attributes — app
//!   components (materials, images, markers) declared in the template.
//! - **Anchored overlays** ([`anchor`]): [`HtmlAnchor`](anchor::HtmlAnchor)
//!   keeps a tooltip or popover root beside an element (right, left, above,
//!   below), inside the viewport, on the element's UI camera, and despawns
//!   it with the element; [`HtmlWorldAnchor`](anchor::HtmlWorldAnchor)
//!   keeps one over a 3D entity (nameplates, markers).
//! - **Tooltips** ([`tooltips`]): `data-tooltip="key"` (plus optional
//!   `data-tooltip-args` / `data-tooltip-placement`) shows the app's
//!   [`HtmlTooltips`](tooltips::HtmlTooltips) template beside the hovered
//!   element, like a browser's `title`.
//! - **Focus and navigation** ([`focus`]): `data-on-click` and
//!   `tabindex="0"` elements are focusable (`tabindex="-1"` opts out),
//!   `autofocus` takes the initial focus, focus survives rebuilds by `id`,
//!   `:focus` / `:focus-visible` and `outline` style it, and an
//!   [`HtmlModal`](focus::HtmlModal) root confines it. Focus is Bevy's
//!   `InputFocus`; bind your own gamepad/keyboard input and call
//!   [`HtmlFocus::navigate`](focus::HtmlFocus::navigate) /
//!   [`activate`](focus::HtmlFocus::activate) with the input you saw
//!   (activation emits the same `ElementSignal` as a click, its source that
//!   input — a key, a gamepad button, or `Synthetic` for harnesses).
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

#[cfg(feature = "fuzzing")]
#[cfg_attr(docsrs, doc(cfg(feature = "fuzzing")))]
#[doc(hidden)]
pub mod fuzz;

pub mod anchor;
mod build;
mod cascade;
pub mod custom_elements;
pub mod focus;
pub mod fonts;
pub mod html;
pub mod l10n;
pub mod nine_slice;
mod rebuild;
pub mod signals;
pub mod style;
pub mod template;
pub mod tooltips;

/// Dependencies whose types appear in this crate's API.
pub use {bevy_fluent, lightningcss, tera, tl};

/// Everything needed to build HTML UIs: `use bevy_markup::prelude::*;`.
pub mod prelude {
    pub use crate::anchor::{AnchorPlacement, HtmlAnchor, HtmlWorldAnchor, HtmlWorldAnchorView};
    pub use crate::custom_elements::{ElementConnected, HtmlCustomElementsExt};
    pub use crate::focus::{
        ActivateElement, FocusEdge, Focusable, HtmlFocus, HtmlModal, HtmlNoFocus,
    };
    pub use crate::fonts::{FontFaces, FontFamilies, GenericFamily};
    pub use crate::html::{
        HtmlDebugOutline, HtmlElement, HtmlElements, HtmlUi, HtmlUiBuilt, HtmlUiRestyled,
        RenderedHtml, TemplateContext,
    };
    pub use crate::l10n::ActiveLocale;
    pub use crate::nine_slice::{NineSlice, NineSliceFrame};
    pub use crate::signals::{
        ActivationInput, ElementSignal, ElementSignals, HtmlSignalsExt, PseudoState, SignalBinding,
        SignalSource, SignalTrigger,
    };
    pub use crate::style::{DefaultStylesheet, HtmlStylesheet, Stylesheet};
    pub use crate::template::HtmlTemplate;
    pub use crate::tooltips::{HtmlTooltip, HtmlTooltips};
    pub use crate::{BevyMarkupPlugin, HtmlUiSystems};
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
pub struct BevyMarkupPlugin;

/// Pipeline stages, in `PostUpdate` before Bevy UI layout (chained in this
/// order). Order your systems against these to see a stage's output the same
/// frame.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HtmlUiSystems {
    /// Tera renders templates and `tl` parses them into [`html::RenderedHtml`].
    Render,
    /// Fluent resolves `data-l10n-id` elements against [`l10n::ActiveLocale`].
    Localize,
    /// The DOM is styled with CSS and spawned as Bevy UI children (then
    /// [`html::HtmlUiBuilt`] fires), or existing children are restyled in
    /// place ([`html::HtmlUiRestyled`]).
    Build,
}

impl Plugin for BevyMarkupPlugin {
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
            .init_resource::<custom_elements::CustomElements>()
            .add_message::<signals::ElementSignal>()
            .add_systems(
                Update,
                (
                    signals::hover_signals,
                    signals::update_pseudo_states,
                    tooltips::show_tooltips,
                ),
            )
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
                    signals::dispatch_signals.before(HtmlUiSystems::Render),
                    anchor::place_world_anchored.before(UiSystems::Prepare),
                    anchor::place_anchored
                        .after(HtmlUiSystems::Build)
                        .before(UiSystems::Prepare),
                ),
            );
        focus::plugin(app);
    }
}
