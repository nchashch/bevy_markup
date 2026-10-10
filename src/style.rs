//! `.css` stylesheets and how they're assigned.
//!
//! Every [`HtmlUi`](crate::html::HtmlUi) uses its own [`HtmlStylesheet`] if it
//! has one, else the [`DefaultStylesheet`] resource, else no styles (white
//! text, Bevy's default font, 16px). Swapping either handle restyles at
//! runtime (themes); the UI isn't built while its stylesheet is still loading,
//! so it never flashes unstyled.
//!
//! Supported CSS (parsed with lightningcss; a syntax error fails the load):
//! - selectors: compound selectors — a type (or `*`) plus any `.class` /
//!   `#id` parts: `p`, `.note`, `#title`, `p.note`, `.a.b`, `h1#x.big` — incl.
//!   comma lists, plus the pseudo-classes `:hover`, `:active`, `:focus` and
//!   `:focus-visible` (see [`crate::focus`]) (each adds class-level
//!   specificity; the rule applies while the element's
//!   [`PseudoState`](crate::signals::PseudoState) has the bit set, which the
//!   library maintains from picking — a change restyles in place). Matched
//!   against each element's tag, `id` and `class` attributes. Combinators,
//!   attribute selectors and other pseudo-classes are skipped (logged at
//!   `debug`).
//! - inherited: `color`, `font-family`, `font-size`, `font-weight`,
//!   `font-style`, `pointer-events` (`none` → `Pickable::IGNORE` on the
//!   element and, by inheritance, its descendants; `auto` turns it back on)
//! - box properties on blocks, containers and the root rule (the `HtmlUi`
//!   node itself, see below): `border-image` (+ `-source`, `-slice`, `-repeat`),
//!   `border-width`, `padding` (absolute lengths); `background-color` on
//!   blocks and containers; `gap` / `row-gap` / `column-gap` on containers
//! - on blocks and containers: `outline` (+ `-style`, `-width`, `-color`;
//!   `outline-offset` in px; drawn only with a visible style, `currentColor`
//!   = the text color) → Bevy's `Outline`; `border-color` (+ `-top`/… sides; Bevy's
//!   `BorderColor`, undeclared sides transparent — needs a `border-width`),
//!   `border-radius` (+ the four corner longhands; the CSS `h / v` slash
//!   syntax gives elliptical corners, `%` per-axis against the node's
//!   dimensions), `z-index` (an integer →
//!   `ZIndex` among siblings; `auto` = 0). An app's own `BorderColor` /
//!   `ZIndex` / `Pickable` is left alone unless a rule sets it.
//! - inline elements: `background-color` paints the run (→ Bevy's
//!   `TextBackgroundColor`, no box); `text-decoration` (+ `-line` with
//!   `underline`/`line-through`, `-color`; `overline`/`blink` and
//!   `-thickness`/`-style` skipped) → `Underline`/`Strikethrough`; the
//!   color defaults to the text's (`currentColor`). Other box properties on
//!   inline elements stay ignored.
//! - `<img src="…" width height>`: an image flowing inline with the text
//!   (`InlineBox`/`InlineImage`; the CSS `background`/`border` boxes don't
//!   apply to it). `src` is an asset path relative to the template (like
//!   `{% include %}`; `/` from the asset root; URLs and data URIs
//!   unsupported); `width`/`height` are px attributes, aspect ratio kept
//!   when one is unset; the image's load restyles the UI, and its reload
//!   (hot reload) re-sizes the box.
//! - `opacity` (a number or `%`): CSS group opacity, approximated by
//!   multiplying the alpha of every color in the element's subtree (text,
//!   backgrounds, borders, outlines, `border-image` tint) by the product of
//!   the element's and its ancestors' opacities. App-attached images aren't
//!   faded.
//! - layout on blocks, containers and the root rule: `display` (`none`, `block`, `flex`,
//!   `grid`), `flex-direction`, `flex-wrap`, `flex-flow`, `justify-content`,
//!   `align-items`, `align-content`, `align-self`, `justify-items`,
//!   `justify-self`, `flex-grow`, `flex-shrink`, `flex-basis`, `flex`;
//!   `width`, `height`, `min-*`, `max-*` (px, `%`, `vw`/`vh`/`vmin`/`vmax`,
//!   `auto`/`none`); `margin` (+ sides; px, `%`, `auto`); `box-sizing`;
//!   `overflow` (+ `-x`/`-y`: `visible`, `hidden`, `clip`, `scroll`; `auto`
//!   scrolls — Bevy has no scroll-if-needed). As
//!   in CSS, sizes default to the content box. `position` (`static`,
//!   `relative`, `absolute`, `fixed`; `absolute` is placed in its parent's
//!   padding box, Bevy's rule, i.e. as if every parent were positioned;
//!   `fixed` positions against the viewport — Bevy's `FixedNode` — and does
//!   not inherit the parent's clipping or transform) with `top` /
//!   `right` / `bottom` / `left` / `inset` (px, `%`, viewport units, `auto`),
//!   which apply only to `relative`/`absolute`/`fixed`, as in CSS;
//!   `sticky` is skipped.
//!   Containers are flex columns unless `flex-direction` (or `display:
//!   block`) says otherwise — `display: flex` alone keeps the column — and
//!   neither containers nor blocks shrink by default (`flex-shrink: 0`).
//!   Font-relative lengths (`em`, `rem`) and `calc()` are skipped here.
//! - grid: `grid-template-rows`/`-columns` (`none` or track lists: lengths,
//!   `%`, viewport units, `fr`, `auto`, `min-content`, `max-content`,
//!   `minmax()`, `fit-content()`, `repeat()` with a count, `auto-fill` or
//!   `auto-fit`), `grid-template`, `grid-auto-rows`/`-columns`,
//!   `grid-auto-flow`, `grid`; placement by line number and `span`:
//!   `grid-row`/`-column` (+ `-start`/`-end`), `grid-area`. Named lines
//!   (names in track lists are ignored, placements by name skipped) and
//!   `grid-template-areas` are unsupported. Bare `grid-auto-flow: dense`
//!   doesn't parse (lightningcss); write `row dense`.
//! - cascade: `!important` beats normal declarations, then higher specificity
//!   (ids, classes, type) wins, then the later rule
//! - the `style` attribute (any element, the `<html>` root included): its
//!   declarations beat every normal rule and lose to `!important` rules
//!   unless `!important` themselves, as in CSS. Rendered by Tera like any
//!   attribute, so templated values (`style="width: {{ hp }}%"`) update the
//!   element in place.
//! - the root rule styles the `HtmlUi` node itself: the `html` rule, plus
//!   the document's own top-level `<html id="…" class="…">` if it has one
//!   (so each template can style its root: `<html class="hud">` with
//!   `.hud { position: absolute; top: 0; right: 0 }`). It takes everything a
//!   container does — layout, sizes, margins, `position` and insets,
//!   `border-radius`, box properties, `gap`, `background-color`,
//!   `border-color`, `z-index` (`ZIndex`; Bevy orders root nodes by
//!   `GlobalZIndex`, then `ZIndex`), `pointer-events: none` — and its
//!   inherited properties start the cascade, also for fragments. Only what
//!   it declares: the app's other `Node` fields and components stay, and
//!   whatever a later stylesheet or document stops declaring goes back to
//!   the app's value.
//!
//! `font-family` uses the first name registered in
//! [`FontFamilies`](crate::fonts::FontFamilies) (generic keywords via
//! [`FontFamilies::set_generic`](crate::fonts::FontFamilies::set_generic)); a
//! list with no registered name means Bevy's default font (like a browser's
//! default font), not the inherited family. `font-size`:
//! `px`, `em`/`%` (of the inherited size), `rem` (of the root size), keywords
//! (`medium` = 16px), `smaller`/`larger`. `font-weight`: bold above 500 (CSS
//! font matching with regular/bold faces).
//! `font-style`: `italic`/`oblique` vs `normal`.
//!
//! ## 9-slice frames with `border-image`
//!
//! ```css
//! html {
//!   border-image: url("frame.png") 16 fill / 16px stretch;
//!   border-width: 16px;
//!   padding: 12px 20px;
//! }
//! ```
//!
//! The image (relative to the `.css` file; its format must be enabled in the
//! app's Bevy features) is drawn 9-sliced over the node's border box:
//! `border-image-slice` gives the insets in image pixels (or `%` of the image
//! size), `border-image-repeat: stretch` stretches sides and center,
//! `repeat`/`round`/`space` tile them. `border-width` and `padding` become the
//! node's `border`/`padding`, insetting the content. Bevy differences: the
//! center is always drawn (`fill` or not), corners keep their image size
//! (`border-image-width`/`-outset` are ignored), and one repeat mode applies to
//! all sides. Box properties from the root rule (see above) are applied to
//! the `HtmlUi` node and restored when a later stylesheet drops them.

use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;
use lightningcss::stylesheet::{ParserOptions, StyleSheet};
use lightningcss::traits::IntoOwned;

/// A parsed `.css` file, plus the images its `border-image-source: url(...)`
/// declarations reference (resolved relative to the `.css` file, loaded as
/// dependencies).
#[derive(Asset, TypePath)]
pub struct Stylesheet {
    pub(crate) sheet: StyleSheet<'static>,
    /// URLs as written, parallel to `images`.
    pub(crate) image_urls: Vec<String>,
    #[dependency]
    pub(crate) images: Vec<Handle<Image>>,
}

impl Stylesheet {
    /// The lightningcss stylesheet tree.
    pub fn sheet(&self) -> &StyleSheet<'static> {
        &self.sheet
    }

    /// The image a `url(...)` in this sheet refers to.
    pub(crate) fn image(&self, url: &str) -> Option<&Handle<Image>> {
        let index = self.image_urls.iter().position(|u| u == url)?;
        self.images.get(index)
    }

    pub(crate) fn images(&self) -> &[Handle<Image>] {
        &self.images
    }
}

/// Stylesheet for every `HtmlUi` without an [`HtmlStylesheet`]. Swap the
/// handle to switch themes.
#[derive(Resource, Default, Clone, Debug, Reflect)]
#[reflect(Resource, Default)]
pub struct DefaultStylesheet(pub Option<Handle<Stylesheet>>);

impl DefaultStylesheet {
    pub fn new(stylesheet: Handle<Stylesheet>) -> Self {
        Self(Some(stylesheet))
    }
}

/// Per-entity stylesheet, overriding [`DefaultStylesheet`] for this `HtmlUi`.
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component)]
pub struct HtmlStylesheet(pub Handle<Stylesheet>);

#[derive(Default, TypePath)]
pub(crate) struct StylesheetLoader;

impl AssetLoader for StylesheetLoader {
    type Asset = Stylesheet;
    type Settings = ();
    type Error = BevyError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<Stylesheet, BevyError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source = String::from_utf8(bytes)?;

        let options = ParserOptions {
            filename: load_context.path().to_string(),
            ..ParserOptions::default()
        };
        // The parse error borrows `source`; render it before `source` drops.
        let sheet = StyleSheet::parse(&source, options).map_err(|err| err.to_string())?;
        let image_urls = crate::cascade::image_urls(&sheet);
        let mut images = Vec::with_capacity(image_urls.len());
        for url in &image_urls {
            let path = load_context.path().resolve_embed_str(url)?;
            images.push(load_context.load(path));
        }
        Ok(Stylesheet {
            sheet: sheet.into_owned(),
            image_urls,
            images,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["css"]
    }
}
