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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(ron: &str) -> Result<NineSliceDescriptor, ron::error::SpannedError> {
        ron::de::from_str(ron)
    }

    /// A frame gets its style's sliced image drawn over the border box (like
    /// CSS `border-image`), whether it had no `ImageNode` yet or an existing
    /// one (whose image, mode and box are replaced).
    #[test]
    fn frames_get_the_sliced_image_over_the_border_box() {
        use bevy::ecs::message::Messages;
        use bevy::ecs::system::RunSystemOnce;

        let mut world = World::new();
        world.init_resource::<Messages<AssetEvent<NineSlice>>>();
        let image = Handle::<Image>::default();
        let slicer = TextureSlicer {
            border: BorderRect {
                min_inset: Vec2::new(4.0, 3.0),
                max_inset: Vec2::new(2.0, 1.0),
            },
            ..default()
        };
        let mut slices = Assets::<NineSlice>::default();
        let style = slices.add(NineSlice {
            image: image.clone(),
            slicer: slicer.clone(),
        });
        world.insert_resource(slices);
        let fresh = world.spawn(NineSliceFrame(style.clone())).id();
        let existing = world
            .spawn((NineSliceFrame(style), ImageNode::new(Handle::default())))
            .id();

        world.run_system_once(apply_nine_slices).unwrap();

        for entity in [fresh, existing] {
            let node = world
                .get::<ImageNode>(entity)
                .expect("frame got an ImageNode");
            assert!(
                matches!(node.visual_box, VisualBox::BorderBox),
                "{entity}: {:?}",
                node.visual_box
            );
            assert_eq!(node.image, image);
            let NodeImageMode::Sliced(applied) = &node.image_mode else {
                panic!("{entity}: not sliced: {:?}", node.image_mode);
            };
            assert_eq!(applied.border.min_inset, slicer.border.min_inset);
            assert_eq!(applied.border.max_inset, slicer.border.max_inset);
        }
    }

    /// An unchanged frame whose style asset reloads (hot reload) takes the
    /// new slices; one whose style didn't change is left alone.
    #[test]
    fn style_reload_reapplies_to_unchanged_frames() {
        use bevy::ecs::message::Messages;

        let mut world = World::new();
        world.init_resource::<Messages<AssetEvent<NineSlice>>>();
        let slicer = |inset: f32| TextureSlicer {
            border: BorderRect::all(inset),
            ..default()
        };
        let mut slices = Assets::<NineSlice>::default();
        let reloaded = slices.add(NineSlice {
            image: Handle::default(),
            slicer: slicer(4.0),
        });
        let untouched = slices.add(NineSlice {
            image: Handle::default(),
            slicer: slicer(4.0),
        });
        world.insert_resource(slices);
        let frame = world.spawn(NineSliceFrame(reloaded.clone())).id();
        let other = world.spawn(NineSliceFrame(untouched.clone())).id();
        // A registered system keeps its change ticks between runs.
        let system = world.register_system(apply_nine_slices);
        world.run_system(system).unwrap();

        let mut slices = world.resource_mut::<Assets<NineSlice>>();
        slices.get_mut_untracked(&reloaded).unwrap().slicer = slicer(9.0);
        slices.get_mut_untracked(&untouched).unwrap().slicer = slicer(7.0);
        world.write_message(AssetEvent::<NineSlice>::Modified { id: reloaded.id() });
        world.run_system(system).unwrap();

        let inset = |entity| match &world.get::<ImageNode>(entity).unwrap().image_mode {
            NodeImageMode::Sliced(slicer) => slicer.border.min_inset.x,
            mode => panic!("not sliced: {mode:?}"),
        };
        assert_eq!(inset(frame), 9.0, "reloaded style re-applied");
        assert_eq!(inset(other), 4.0, "unchanged frame and style left alone");
    }

    /// The module docs' example parses, with each side read from its own
    /// field and `Tile(x)` carrying its stretch value.
    #[test]
    fn documented_example_parses() {
        let desc = parse(
            r#"(
                image: "frame.png",
                border: (left: 1, right: 2, top: 3, bottom: 4),
                sides: Tile(1.5),
                center: Stretch,
                max_corner_scale: 2.0,
            )"#,
        )
        .unwrap();
        assert_eq!(desc.image, "frame.png");
        let b = desc.border;
        assert_eq!([b.left, b.right, b.top, b.bottom], [1.0, 2.0, 3.0, 4.0]);
        assert!(
            matches!(SliceScaleMode::from(desc.sides), SliceScaleMode::Tile { stretch_value } if stretch_value == 1.5)
        );
        assert!(matches!(
            SliceScaleMode::from(desc.center),
            SliceScaleMode::Stretch
        ));
        assert_eq!(desc.max_corner_scale, 2.0);
    }

    /// Optional fields default to stretch sides/center and a corner scale
    /// of 1 (corners at image size). Catches a `0.0` default (corners
    /// collapse to nothing) or a `Tile` default.
    #[test]
    fn optional_fields_default() {
        let desc = parse(r#"(image: "f.png", border: (left: 16, right: 16, top: 16, bottom: 16))"#)
            .unwrap();
        assert!(matches!(
            SliceScaleMode::from(desc.sides),
            SliceScaleMode::Stretch
        ));
        assert!(matches!(
            SliceScaleMode::from(desc.center),
            SliceScaleMode::Stretch
        ));
        assert_eq!(desc.max_corner_scale, 1.0);
    }

    /// Missing required fields, missing border sides and unknown scale
    /// modes fail the load instead of silently drawing a wrong frame.
    #[test]
    fn invalid_manifests_are_errors() {
        for ron in [
            r#"(border: (left: 1, right: 1, top: 1, bottom: 1))"#,
            r#"(image: "f.png")"#,
            r#"(image: "f.png", border: (left: 1, right: 1, top: 1))"#,
            r#"(image: "f.png", border: (left: 1, right: 1, top: 1, bottom: 1), sides: Repeat)"#,
            r#"(image: "f.png", border: (left: 1, right: 1, top: 1, bottom: 1), sides: Tile)"#,
            "",
        ] {
            assert!(parse(ron).is_err(), "{ron}");
        }
    }
}
