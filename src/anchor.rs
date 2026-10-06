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
//!   `auto`. `Above`/`Left` anchor by `bottom`/`right`, which needs the
//!   viewport height/width; without a camera they anchor by `top`/`left`
//!   using the overlay's measured size instead. Its `position_type` is the
//!   stylesheet's or the app's.
//! - Once the overlay has a size it's kept inside the element's viewport.
//! - It renders on the element's UI camera (`UiTargetCamera`), so it works
//!   for UIs on render-to-texture cameras too.
//! - It's despawned with its element.
//!
//! [`HtmlWorldAnchor`] is the same for a point in the 3D world (an entity
//! plus an offset), projected through a camera.

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

/// Keeps a UI root over a point in the 3D world: `target`'s position plus
/// `offset`, projected through a camera every frame — nameplates, markers,
/// damage numbers.
///
/// - The root's `left`/`top` put its `pivot` (a fraction of its own size:
///   `(0.5, 1.0)` = bottom center, the default) on the projected point; make
///   it `position: absolute` (its `<html class>` rule).
/// - The camera is `camera`, else the root's `UiTargetCamera`, else the
///   default UI camera (the `IsDefaultUiCamera` one, else the window
///   camera Bevy UI renders with).
/// - It owns the root's `Visibility`: hidden while the point is behind the
///   camera or outside the viewport, or `target` is invisible
///   (`InheritedVisibility`); hide it for app reasons with CSS instead
///   (a root class with `display: none`).
/// - [`HtmlWorldAnchorView`] reports the distance to the camera and whether
///   the point is on screen, e.g. for a distance fade (`opacity`).
/// - It's despawned with `target`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
#[reflect(Component)]
#[require(HtmlWorldAnchorView)]
pub struct HtmlWorldAnchor {
    pub target: Entity,
    /// Added to the target's translation, in world units.
    pub offset: Vec3,
    /// The point of the root placed on the projection, as a fraction of its
    /// size.
    pub pivot: Vec2,
    /// The camera to project with (see the type docs for the default).
    pub camera: Option<Entity>,
}

impl HtmlWorldAnchor {
    pub fn new(target: Entity) -> Self {
        Self {
            target,
            offset: Vec3::ZERO,
            pivot: Vec2::new(0.5, 1.0),
            camera: None,
        }
    }

    pub fn with_offset(mut self, offset: Vec3) -> Self {
        self.offset = offset;
        self
    }

    pub fn with_pivot(mut self, pivot: Vec2) -> Self {
        self.pivot = pivot;
        self
    }

    pub fn with_camera(mut self, camera: Entity) -> Self {
        self.camera = Some(camera);
        self
    }
}

/// What [`HtmlWorldAnchor`] measured this frame.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component)]
pub struct HtmlWorldAnchorView {
    /// From the camera to the anchored point, in world units.
    pub distance: f32,
    /// The point projects into the camera's viewport (not behind it).
    pub on_screen: bool,
}

/// A world-anchored root: its anchor, measurements, insets, visibility,
/// measured size and UI camera.
type WorldOverlay = (
    Entity,
    &'static HtmlWorldAnchor,
    &'static mut HtmlWorldAnchorView,
    &'static mut Node,
    &'static mut Visibility,
    Option<&'static ComputedNode>,
    Option<&'static UiTargetCamera>,
);

/// Places every [`HtmlWorldAnchor`]ed root over its target, or despawns it
/// with its target.
pub(crate) fn place_world_anchored(
    mut overlays: Query<WorldOverlay>,
    targets: Query<(&GlobalTransform, Option<&InheritedVisibility>)>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    default_camera: DefaultUiCamera,
    mut commands: Commands,
) {
    for (overlay, anchor, mut view, mut node, mut visibility, own, ui_camera) in &mut overlays {
        let Ok((target, target_visibility)) = targets.get(anchor.target) else {
            commands.entity(overlay).try_despawn();
            continue;
        };
        let camera = anchor
            .camera
            .or(ui_camera.map(UiTargetCamera::entity))
            .or_else(|| default_camera.get());
        let Some((camera, camera_transform)) = camera.and_then(|camera| cameras.get(camera).ok())
        else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let point = target.translation() + anchor.offset;
        let projected = camera
            .world_to_viewport(camera_transform, point)
            .ok()
            .filter(|position| {
                camera.logical_viewport_size().is_some_and(|size| {
                    position.cmpge(Vec2::ZERO).all() && position.cmple(size).all()
                })
            });
        let measured = HtmlWorldAnchorView {
            distance: camera_transform.translation().distance(point),
            on_screen: projected.is_some(),
        };
        if *view != measured {
            *view = measured;
        }
        let shown = target_visibility.is_none_or(|visible| visible.get());
        let Some(position) = projected.filter(|_| shown) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let size = own.map_or(Vec2::ZERO, |own| own.size() * own.inverse_scale_factor);
        let top_left = position - size * anchor.pivot;
        let (left, top) = (Val::Px(top_left.x), Val::Px(top_left.y));
        if node.left != left || node.top != top {
            node.left = left;
            node.top = top;
        }
        visibility.set_if_neq(Visibility::Inherited);
    }
}
