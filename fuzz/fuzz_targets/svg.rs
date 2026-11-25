//! Fuzzes the `svg` loader (ADR 0038): roxmltree parse → recursion-depth
//! check → usvg conversion (refusing image resolver) → render-cost
//! estimate → resvg draw, within a 256 px side and a budget scaled to it.
//!
//! The body is `bevy_markup::fuzz::svg_target`, shared with the other
//! drivers' `svg` targets.
//!
//! Contract: never panics, overflows the stack or aborts; refusals are
//! fine. The call is timed: the budget allows a few milliseconds of
//! drawing, so a slow input means the estimate missed a cost (or a parse
//! step is superlinear) and fails here instead of passing as a mere
//! refusal.
//!
//! Every panic aborts, tiny-skia's far-off-canvas ones (U24, U27, U28)
//! included: they panic in release too, and the loader's extent check
//! refuses their inputs, so one reaching tiny-skia is a miss. (resvg's
//! turbulence overflow, U20, was excused until ADR 0051 put filters
//! outside the subset.)
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| bevy_markup::fuzz::svg_target(data));
