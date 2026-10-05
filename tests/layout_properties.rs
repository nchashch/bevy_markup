//! Layout properties: invariants the CSS flexbox spec guarantees for *any*
//! sizes, checked on Bevy UI's real headless layout (`TestUi::with_layout`)
//! of generated stylesheets. Complements the layout oracle (fixed vectors
//! against Chromium) with generated inputs; tolerances are 1px because Bevy
//! rounds layout to whole pixels.

mod common;
use common::{TestUi, node_rect};

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_markup::prelude::*;
use proptest::collection::vec;
use proptest::prop_assert;
use test_strategy::proptest;

const VIEWPORT: UVec2 = UVec2::new(640, 480);
const TOLERANCE: f32 = 1.0;

/// Lays out `page` + `css` under a full-width column root; each element's
/// rect by its first class.
fn layout(name: &str, page: &str, css: &str) -> HashMap<String, Rect> {
    let root = Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Column,
        ..default()
    };
    let mut ui = TestUi::with_layout(name, &[("page.html", page), ("style.css", css)], VIEWPORT)
        .stylesheet("style.css")
        .spawn("page.html", TemplateContext::new(), root);
    ui.settle();
    let world = ui.world_mut();
    let elements: Vec<(Entity, String)> = world
        .query::<(Entity, &HtmlElement)>()
        .iter(world)
        .filter_map(|(entity, element)| Some((entity, element.classes.first()?.clone())))
        .collect();
    elements
        .into_iter()
        .map(|(entity, class)| (class, node_rect(world, entity).expect("laid out")))
        .collect()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= TOLERANCE
}

/// Items `.i0`…, each a fixed-width block, in a container `.c`.
fn items(widths: &[u16]) -> (String, String) {
    let page: String = widths
        .iter()
        .enumerate()
        .map(|(i, _)| format!(r#"<p class="i{i}">x</p>"#))
        .collect();
    let css: String = widths
        .iter()
        .enumerate()
        .map(|(i, width)| format!(".i{i} {{ width: {width}px }}\n"))
        .collect();
    (format!(r#"<div class="c">{page}</div>"#), css)
}

/// `row-reverse` places each item at the mirror image of its `row` position
/// within the container (catches direction/justify mapping mix-ups).
#[proptest(cases = 12)]
fn row_reverse_mirrors_row(#[strategy(vec(10u16..100, 1..5))] widths: Vec<u16>) {
    let (page, css) = items(&widths);
    let row = layout(
        "prop-row",
        &page,
        &format!("html {{ font-size: 20px }} .c {{ flex-direction: row; width: 600px }}\n{css}"),
    );
    let reverse = layout(
        "prop-row-reverse",
        &page,
        &format!(
            "html {{ font-size: 20px }} .c {{ flex-direction: row-reverse; width: 600px }}\n{css}"
        ),
    );
    let container = row["c"];
    for i in 0..widths.len() {
        let (a, b) = (row[&format!("i{i}")], reverse[&format!("i{i}")]);
        let mirrored = container.min.x + container.max.x - a.max.x;
        prop_assert!(
            close(b.min.x, mirrored),
            "item {i}: row {a:?}, reverse {b:?}, expected x {mirrored}"
        );
        prop_assert!(close(a.width(), b.width()));
    }
}

/// `margin: 0 auto` centers a fixed-width block: equal free space left and
/// right, whatever the width.
#[proptest(cases = 12)]
fn auto_margins_center(#[strategy(40u16..600)] width: u16) {
    let rects = layout(
        "prop-auto-margin",
        r#"<div class="c"><p class="b">x</p></div>"#,
        &format!("html {{ font-size: 20px }} .b {{ width: {width}px; margin: 0 auto }}"),
    );
    let (c, b) = (rects["c"], rects["b"]);
    prop_assert!(close(b.width(), f32::from(width)));
    prop_assert!(
        close(b.min.x - c.min.x, c.max.x - b.max.x),
        "container {c:?}, block {b:?}"
    );
}

/// With zero bases, `flex-grow` splits the container's width in proportion
/// to the factors.
#[proptest(cases = 12)]
fn flex_grow_shares_free_space(
    #[strategy(1u8..6)] g1: u8,
    #[strategy(1u8..6)] g2: u8,
    #[strategy(200u16..600)] width: u16,
) {
    let rects = layout(
        "prop-grow",
        r#"<div class="c"><p class="a">x</p><p class="b">x</p></div>"#,
        &format!(
            "html {{ font-size: 20px }} .c {{ flex-direction: row; width: {width}px }} \
             .a {{ flex: {g1} 1 0px }} .b {{ flex: {g2} 1 0px }}"
        ),
    );
    let (a, b) = (rects["a"], rects["b"]);
    let expected = f32::from(width) * f32::from(g1) / f32::from(g1 + g2);
    prop_assert!(
        close(a.width(), expected),
        "a {a:?}, expected width {expected}"
    );
    prop_assert!(close(a.width() + b.width(), f32::from(width)));
    prop_assert!(close(b.min.x, a.max.x));
}

/// `justify-content: space-between`: the first item touches the start, the
/// last the end, and the gaps between neighbours are equal.
#[proptest(cases = 12)]
fn space_between_spreads_items(#[strategy(vec(10u16..100, 2..5))] widths: Vec<u16>) {
    let (page, css) = items(&widths);
    let rects = layout(
        "prop-space-between",
        &page,
        &format!(
            "html {{ font-size: 20px }} .c {{ flex-direction: row; width: 600px; \
             justify-content: space-between }}\n{css}"
        ),
    );
    let container = rects["c"];
    let item = |i: usize| rects[&format!("i{i}")];
    let last = widths.len() - 1;
    prop_assert!(close(item(0).min.x, container.min.x));
    prop_assert!(close(item(last).max.x, container.max.x));
    let gaps: Vec<f32> = (0..last)
        .map(|i| item(i + 1).min.x - item(i).max.x)
        .collect();
    let (min, max) = gaps.iter().fold((f32::MAX, f32::MIN), |(lo, hi), gap| {
        (lo.min(*gap), hi.max(*gap))
    });
    prop_assert!(max - min <= TOLERANCE, "unequal gaps {gaps:?}");
}

/// `flex-wrap: wrap` keeps every item inside the container and never
/// overlaps two items, whatever the widths (each narrower than a line).
#[proptest(cases = 12)]
fn wrapped_items_stay_inside_without_overlap(#[strategy(vec(20u16..200, 2..8))] widths: Vec<u16>) {
    let (page, css) = items(&widths);
    let rects = layout(
        "prop-wrap",
        &page,
        &format!(
            "html {{ font-size: 20px }} .c {{ flex-direction: row; flex-wrap: wrap; width: 300px; \
             column-gap: 5px; row-gap: 5px }}\n{css}"
        ),
    );
    let container = rects["c"];
    let item = |i: usize| rects[&format!("i{i}")];
    for i in 0..widths.len() {
        let a = item(i);
        prop_assert!(
            a.min.x >= container.min.x - TOLERANCE && a.max.x <= container.max.x + TOLERANCE
        );
        for j in i + 1..widths.len() {
            let overlap = a.intersect(item(j));
            prop_assert!(
                overlap.width() <= TOLERANCE || overlap.height() <= TOLERANCE,
                "{i} and {j} overlap"
            );
        }
    }
}
