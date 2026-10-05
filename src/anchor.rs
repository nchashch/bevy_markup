//! Overlays anchored to an element: tooltips, popovers, dropdowns.
//!
//! Put [`HtmlAnchor`] on an absolutely positioned UI root (an `HtmlUi`, say,
//! styled `position: absolute` by its `<html class>`) and bevy_markup keeps
//! it beside the element every frame, like CSS anchor positioning:
//!
//! ```no_run
//! # use bevy::prelude::*;
//! # use bevy_markup::prelude::*;
//! # fn f(mut commands: Commands, assets: Res<AssetServer>, button: Entity) {
//! commands.spawn((
//!     HtmlUi::new(assets.load("tooltip.html")),
//!     HtmlAnchor::new(button, AnchorPlacement::Above).with_gap(8.0),
//! ));
//! # }
//! ```
//!
//! - The overlay's insets are set from the element's laid-out rect (the
//!   previous frame's layout): `Right`/`Below` set `left`/`top`, `Above`
//!   sets `left`/`bottom`, `Left` sets `right`/`top`; the other insets are
//!   `auto`. Its `position_type` is the stylesheet's or the app's.
//! - Once the overlay has a size it's kept inside the element's viewport.
//! - It renders on the element's UI camera (`UiTargetCamera`), so it works
//!   for UIs on render-to-texture cameras too.
//! - It's despawned with its element.

use bevy::camera::Camera;
use bevy::prelude::*;
use bevy::ui::{ComputedUiTargetCamera, UiGlobalTransform};

/// Keeps this UI node beside `element` (see the [module docs](self)).
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Component)]
pub struct HtmlAnchor {
    /// The element (any UI node) to stay beside.
    pub element: Entity,
    /// Which side of the element.
    pub placement: AnchorPlacement,
    /// Space between the element and the overlay, in logical px.
    pub gap: f32,
}

impl HtmlAnchor {
    pub fn new(element: Entity, placement: AnchorPlacement) -> Self {
        Self {
            element,
            placement,
            gap: 0.0,
        }
    }

    pub fn with_gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }
}

/// The side of the element an [`HtmlAnchor`]ed overlay sits on, aligned to
/// the element's start edge (top for `Right`/`Left`, left for
/// `Above`/`Below`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Reflect)]
pub enum AnchorPlacement {
    #[default]
    Right,
    Left,
    Above,
    Below,
}

/// An anchored overlay: its anchor, insets, measured size and camera.
type Overlay = (
    Entity,
    &'static HtmlAnchor,
    &'static mut Node,
    Option<&'static ComputedNode>,
    Option<&'static UiTargetCamera>,
);

/// Places every [`HtmlAnchor`]ed node beside its element, or despawns it
/// with its element.
pub(crate) fn place_anchored(
    mut overlays: Query<Overlay>,
    elements: Query<(
        &ComputedNode,
        &UiGlobalTransform,
        Option<&ComputedUiTargetCamera>,
    )>,
    cameras: Query<&Camera>,
    mut commands: Commands,
) {
    for (overlay, anchor, mut node, own, target) in &mut overlays {
        let Ok((element, transform, element_camera)) = elements.get(anchor.element) else {
            commands.entity(overlay).try_despawn();
            continue;
        };
        let camera = element_camera.and_then(ComputedUiTargetCamera::get);
        if let Some(camera) = camera
            && target.map(|target| target.entity()) != Some(camera)
        {
            commands.entity(overlay).try_insert(UiTargetCamera(camera));
        }

        // Physical px → logical UI px, the space `Node` insets use.
        let scale = element.inverse_scale_factor;
        let size = element.size() * scale;
        let min = transform.translation * scale - size / 2.0;
        let max = min + size;
        let viewport = camera
            .and_then(|camera| cameras.get(camera).ok())
            .and_then(Camera::physical_viewport_size)
            .map(|size| size.as_vec2() * scale);
        let own = own
            .map(|own| own.size() * own.inverse_scale_factor)
            .filter(|own| own.cmpgt(Vec2::ZERO).all());
        // Keeps `start..start + extent` inside `0..limit` once both are known.
        let clamp = |start: f32, extent: Option<f32>, limit: Option<f32>| match (extent, limit) {
            (Some(extent), Some(limit)) => start.min(limit - extent).max(0.0),
            _ => start,
        };
        let (width, height) = (own.map(|own| own.x), own.map(|own| own.y));
        let (view_w, view_h) = (viewport.map(|v| v.x), viewport.map(|v| v.y));
        let gap = anchor.gap;
        let [left, top, right, bottom] = match anchor.placement {
            AnchorPlacement::Right => [
                Some(clamp(max.x + gap, width, view_w)),
                Some(clamp(min.y, height, view_h)),
                None,
                None,
            ],
            AnchorPlacement::Below => [
                Some(clamp(min.x, width, view_w)),
                Some(clamp(max.y + gap, height, view_h)),
                None,
                None,
            ],
            // Bottom-anchored: right without knowing the overlay's height
            // (top-anchored by its measured height without a viewport).
            AnchorPlacement::Above => match view_h {
                Some(view_h) => [
                    Some(clamp(min.x, width, view_w)),
                    None,
                    None,
                    Some(view_h - min.y + gap),
                ],
                None => [
                    Some(min.x),
                    Some(min.y - gap - height.unwrap_or(0.0)),
                    None,
                    None,
                ],
            },
            AnchorPlacement::Left => match view_w {
                Some(view_w) => [
                    None,
                    Some(clamp(min.y, height, view_h)),
                    Some(view_w - min.x + gap),
                    None,
                ],
                None => [
                    Some(min.x - gap - width.unwrap_or(0.0)),
                    Some(min.y),
                    None,
                    None,
                ],
            },
        };
        let inset = |value: Option<f32>| value.map_or(Val::Auto, Val::Px);
        let (left, top, right, bottom) = (inset(left), inset(top), inset(right), inset(bottom));
        if (node.left, node.top, node.right, node.bottom) != (left, top, right, bottom) {
            node.left = left;
            node.top = top;
            node.right = right;
            node.bottom = bottom;
        }
    }
}
