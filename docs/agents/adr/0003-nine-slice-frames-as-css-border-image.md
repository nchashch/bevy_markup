# 3. 9-slice frames as CSS `border-image`, not a custom asset format

| Field | Content |
|---|---|
| ADR | 0003 |
| Title | 9-slice frames as CSS `border-image`, not a custom asset format |
| Date | 2026-10-06 06:28 +0400 |
| Author | Claude Opus 5.5 (Anthropic), via omp — retrospective record; the decisions are the commit author's (Nikita Chashchinskii) |
| Commit | `d2c9b63` Implement more features |
| Span | `69e7225`, `1ed06cc`, `23cc5cf` (2026-10-04) |
| Status | Accepted |
| Related | 0001; `bug_0018` |

## Context

Framed panels were part of the prototype from the start (`69e7225`): a
`*.slice.ron` asset (image + slice insets) loaded into a `NineSlice` and drawn
by a `NineSliceFrame` component. Frames were therefore configured in a
bevy_markup-specific format beside the CSS, not in it.

## Decision

Frames are CSS (`1ed06cc` "Drive 9-slice box styling via CSS instead of
custom .ron file", `23cc5cf` "Enable 9-slice styling inside HTML templates"):
`border-image` (shorthand and `-source`/`-slice`/`-repeat` longhands) with
`border-width` and `padding`, on blocks, containers and the `html` rule.
`url()` resolves relative to the stylesheet, and the stylesheet loader loads
the images as dependencies. The result is Bevy's sliced `ImageNode` with
`VisualBox::BorderBox`. A block with box properties gets a wrapper node,
because a node can't be both `Text` and `ImageNode`.
`NineSliceFrame` stays for non-HTML nodes.

## Alternatives considered

- **Keep `*.slice.ron`**: the commit message names it as what was replaced;
  a second styling channel next to CSS.

## Consequences

- Themes can change frames (the demo's parchment and terminal themes do).
- Bevy's `TextureSlicer` limits the mapping: the center is always drawn,
  `border-image-width`/`-outset` are ignored, one repeat mode for all sides
  (`AGENTS.md` "Limits").
- A `border-image` stylesheet waits for its image: under prototype_19's
  `--no-render` (no image loading) such a UI never builds, so p19 avoids
  `border-image` (its `AGENTS.md`).
- The pipeline-owned frame is marked `CssFrame`, so an app's own
  `ImageNode` on an element is app state (bug_0018).
