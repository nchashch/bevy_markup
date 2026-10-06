# 10. Overlays: anchored to elements and world points, with tooltips built in

| Field | Content |
|---|---|
| ADR | 0010 |
| Title | Overlays: anchored to elements and world points, with tooltips built in |
| Date | 2026-10-06 06:28 +0400 |
| Author | Claude Opus 5.5 (Anthropic), via omp — designed and implemented in that omp session for the project owner; committed by the owner (Nikita Chashchinskii) |
| Commit | `d2c9b63` Implement more features |
| Span | `3c73cba`, `d2c9b63` (2026-10-06) |
| Status | Accepted |
| Related | 0007, 0011; prototype_19 playtests 0029, 0033, 0036 |

## Context

prototype_19 positioned three kinds of overlay itself:

- **Tooltips** and **selector popups**: each spawn computed the element's
  rect in physical pixels, converted it to UI pixels, placed the overlay and
  clamped it to the viewport, and copied the element's UI camera (needed on
  VR quad panels). Neither followed its element if it moved.
- **Tooltip wiring**: hover signals with a shared `data-with` blob drove a
  spawn/despawn system (~60 lines).
- **Nameplates**: each frame projected the target through the camera,
  mirrored its visibility by hand and cleaned up after it.

## Decision

1. **`HtmlAnchor { element, placement, gap }`** (`3c73cba`, `src/anchor.rs`):
   - placement is right, left, above or below; "above" is bottom-anchored,
     so it's correct before the overlay is measured;
   - updated every frame from the previous frame's layout, and clamped to
     the viewport once the overlay has a size;
   - drawn on the element's UI camera and despawned with the element.
2. **Built-in tooltips** (`d2c9b63`, `src/tooltips.rs`), like a browser's
   `title`:
   - the app inserts `HtmlTooltips { template, gap }`;
   - `data-tooltip="key"` plus optional `data-tooltip-args` (JSON) and
     `data-tooltip-placement` shows that template, anchored, while a pointer
     is over the element or a descendant (the nearest such element wins);
   - the template gets `key`, `args` and `placement`, is refreshed by
     updates, and is despawned on leave.
3. **`HtmlWorldAnchor { target, offset, pivot, camera }`** (`d2c9b63`):
   - keeps a root's pivot (default bottom center) on the projection of a 3D
     entity plus an offset;
   - uses the given camera, else the root's `UiTargetCamera`, else the
     default UI camera;
   - owns the root's `Visibility`: hidden behind the camera, off screen, or
     when the target is invisible;
   - is despawned with the target, and reports
     `HtmlWorldAnchorView { distance, on_screen }`.

## Alternatives considered

- **Tooltips as an app pattern on hover signals** — the previous state,
  repeated per project; also required packing tooltip keys into `data-with`
  (see 0011).
- **App-owned visibility for world anchors** — would race the anchor's own
  hiding; the app hides for its own reasons with CSS instead (a root class
  with `display: none`; prototype_19's `NameplatesVisible`).
- **Showing tooltips on focus too** — not done: pointer-only, like `title`.

## Consequences

- prototype_19 positions no overlay itself: tooltip and popup placement
  match the hand-computed values exactly (playtest 0029); its tooltip system
  is gone (playtest 0033).
- Nameplates are now centered over their target. Before, they hung from the
  projected point to the right; this changed with the anchor's default pivot
  (playtest 0036).
- `HtmlAnchor` and `HtmlWorldAnchor` are `Reflect`, so BRP tooling can see
  them (found missing during the prototype_19 smoke run, playtest 0029).
- Not exercised: viewport clamping in prototype_19, tooltips on render-to-
  texture panels, VR cameras.
