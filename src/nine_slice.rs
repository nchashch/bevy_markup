//! 9-slice styles as assets, for nodes outside HTML. Inside HTML, use CSS
//! `border-image` instead (see [`crate::style`]).
//!
//! A `*.slice.ron` file names an image (relative to the `.ron` file) and how to
//! slice it:
//!
//! ```ron
//! (
//!     image: "frame.png",
//!     border: (left: 16, right: 16, top: 16, bottom: 16),
//!     sides: Stretch,      // or Tile(1.0)
//!     center: Stretch,     // or Tile(1.0)
//!     max_corner_scale: 1.0,
//! )
//! ```
//!
//! Put `NineSliceFrame(asset_server.load("ui/frame.slice.ron"))` on a UI node to
//! draw it as the node's sliced background image, covering the border box —
//! use `Node::padding` to inset the content. The image format (e.g. `png`) must
//! be enabled in the app's Bevy features.

use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use serde::Deserialize;

/// A loaded 9-slice style: the source image and how to slice it.
#[derive(Asset, TypePath, Debug)]
pub struct NineSlice {
    #[dependency]
    pub image: Handle<Image>,
    pub slicer: TextureSlicer,
}

/// Draws a [`NineSlice`] as this UI node's background, covering the whole
/// border box so `Node::padding` insets the children, not the frame. Inserts or
/// updates the entity's `ImageNode` once the style has loaded (keeping any
/// existing tint).
#[derive(Component, Clone, Debug, Reflect)]
#[reflect(Component)]
#[require(Node)]
pub struct NineSliceFrame(pub Handle<NineSlice>);

#[derive(Deserialize)]
struct NineSliceDescriptor {
    image: String,
    border: BorderDescriptor,
    #[serde(default)]
    sides: ScaleModeDescriptor,
    #[serde(default)]
    center: ScaleModeDescriptor,
    #[serde(default = "default_max_corner_scale")]
    max_corner_scale: f32,
}

/// Slice line insets in image pixels.
#[derive(Deserialize)]
struct BorderDescriptor {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

#[derive(Deserialize, Default)]
enum ScaleModeDescriptor {
    #[default]
    Stretch,
    /// Repeat once the slice is drawn at more than this multiple of its size.
    Tile(f32),
}

fn default_max_corner_scale() -> f32 {
    1.0
}

impl From<ScaleModeDescriptor> for SliceScaleMode {
    fn from(mode: ScaleModeDescriptor) -> Self {
        match mode {
            ScaleModeDescriptor::Stretch => SliceScaleMode::Stretch,
            ScaleModeDescriptor::Tile(stretch_value) => SliceScaleMode::Tile { stretch_value },
        }
    }
}

#[derive(Default, TypePath)]
pub(crate) struct NineSliceLoader;

impl AssetLoader for NineSliceLoader {
    type Asset = NineSlice;
    type Settings = ();
    type Error = BevyError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<NineSlice, BevyError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let desc: NineSliceDescriptor = ron::de::from_bytes(&bytes)?;

        let image_path = load_context.path().resolve_embed_str(&desc.image)?;
        let border = desc.border;
        Ok(NineSlice {
            image: load_context.load(image_path),
            slicer: TextureSlicer {
                border: BorderRect {
                    min_inset: Vec2::new(border.left, border.top),
                    max_inset: Vec2::new(border.right, border.bottom),
                },
                center_scale_mode: desc.center.into(),
                sides_scale_mode: desc.sides.into(),
                max_corner_scale: desc.max_corner_scale,
            },
        })
    }

    fn extensions(&self) -> &[&str] {
        &["slice.ron"]
    }
}

/// Applies a style to frames that were just added/changed, or whose style just
/// (re)loaded.
pub(crate) fn apply_nine_slices(
    mut commands: Commands,
    mut events: MessageReader<AssetEvent<NineSlice>>,
    slices: Res<Assets<NineSlice>>,
    mut frames: Query<(Entity, Ref<NineSliceFrame>, Option<&mut ImageNode>)>,
) {
    let reloaded: HashSet<AssetId<NineSlice>> = events
        .read()
        .filter_map(|event| match event {
            AssetEvent::LoadedWithDependencies { id } | AssetEvent::Modified { id } => Some(*id),
            _ => None,
        })
        .collect();

    for (entity, frame, image_node) in &mut frames {
        if !frame.is_changed() && !reloaded.contains(&frame.0.id()) {
            continue;
        }
        let Some(slice) = slices.get(&frame.0) else {
            continue;
        };
        let mode = NodeImageMode::Sliced(slice.slicer.clone());
        match image_node {
            Some(mut node) => {
                node.image = slice.image.clone();
                node.image_mode = mode;
                node.visual_box = VisualBox::BorderBox;
            }
            None => {
                commands.entity(entity).insert(ImageNode {
                    visual_box: VisualBox::BorderBox,
                    ..ImageNode::new(slice.image.clone()).with_mode(mode)
                });
            }
        }
    }
}
