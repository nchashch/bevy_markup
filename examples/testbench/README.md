# testbench

![The testbench: a scenario list grouped into Performance, Security and Correctness on the left (Hostile Fluent args selected); size, churn, restart and language controls on top; a line of live numbers (frame time, builds and restyles per second, UIs and elements); green PASS checks (built, nothing forged, every payload localized, frame budget); and the stage below showing hostile payloads — HTML tags, Markdown links, crafted markers, a family emoji — rendered as plain text](../screenshots/testbench.png)

An interactive torture bench for performance, security and correctness. Pick
a scenario, change its size, toggle the per-frame churn, and watch the frame
time, the update rate and the scenario's checks (**PASS** / **FAIL** /
**WAIT**). With `--features hotpath`, every pipeline system and the bench's
own work is timed per call, in the real Bevy frame loop.

```sh
# Interactive, with live profiling: run `hotpath console` in a second terminal
# (cargo install hotpath --features=tui), then:
cargo run --release --example testbench --features hotpath

# Scripted: one scenario, a fixed size, a bounded headless run; the checks'
# final states are logged ("testbench: final …") and the hotpath report
# printed on exit.
cargo run --release --example testbench --features hotpath -- \
    --scenario big-document --size 2000 --no-render --exit-after 900
```

## Scenarios

The workloads are generated at runtime into an in-memory asset source
(`testbench://`), so they go through the real loaders and their pre-scans,
and each start loads fresh paths. Size is the number of rows, cards or UIs
for the performance and correctness scenarios; the security inputs are
fixed.

| Group | Scenario | What it does | Checks |
|---|---|---|---|
| Performance | Big document | N rows; churn changes ~1 % of them per frame, so unchanged rows reuse the previous build (ADR 0021) | built, budget |
| | All dirty | every row changes every frame: full render, parse, cascade, diff | built, budget |
| | Restyle storm | a context `var()` recolours every row, a highlight moves | built, budget |
| | Indented Markdown | a `.md` template with indented Tera blocks (ADR 0024), re-rendered | built, budget |
| | Localized rows | Fluent rows in a `markdown: true` bundle, cached and per-value paths | built, budget |
| | Many UIs | N separate small `HtmlUi`s; one updates per frame | built, budget |
| | calc() layout | N cards sized by mixed-unit `calc()` | built, budget |
| Security | Nested CSS rules | 20 000 nested rules: the sheet's load fails cleanly (bug_0106) | sheet refused, built |
| | Deep DOM | 5 000 nested elements: content past 100 levels is dropped | built |
| | var() bombs | 2 000-link chains, doubling fallbacks, cycles, nested functions (bug_0092) | built |
| | Grid bombs | track counts that overflowed (bug_0081, bug_0121), 1 000 names (bug_0099) | built |
| | Hostile Fluent args | HTML, Markdown links and images, crafted markers, emoji in string args (bug_0088, bug_0107, bug_0128) | nothing forged, every payload localized |
| | Args JSON injection | a hostile name through a Tera map literal and through a hand-written JSON splice (bug_0119) | map literal kept the value |
| | Remote sources | an `https://` include and `border-image` (bug_0118) | include refused, image skipped |
| | Markdown bombs | 20 000 `*` and `[`, 2 000 nested quotes, crafted markers, 30 nested indented Tera blocks, 10 000 indented lines | survived |
| Correctness | Reuse vs fresh build | random edits (change, insert, remove, move, highlight) every frame; every 20 frames the churn holds and the UI is compared with a fresh build of the same state | twins match |
| | Indented vs flat | an indented and a flat Markdown template of the same document must build the same UI | twins match |

Every scenario also has a **frame budget** check: the worst frame after a
30-frame warm-up must stay under 250 ms.

## Controls

| Input | Action |
|---|---|
| Click a scenario, or focus it and Enter / A | Start it |
| N / P, gamepad RB / LB | Next / previous scenario |
| − / + buttons, [ / ], gamepad LT / RT, ← → on the focused size | Size (10 … 10 000) |
| C / gamepad X, or the Churn button | Toggle the per-frame churn |
| R / gamepad Select, or Restart | Restart the scenario |
| L / gamepad Y, or Language | English ↔ German |

Command-line flags: `--scenario <id>` (pins it; the scenario ids are the
ones in `scenarios.rs`, e.g. `reuse-vs-fresh`), `--size <n>` (the nearest
size step at or above), `--no-churn`, plus the examples' launcher flags
(`--no-render`, `--offscreen`, `--exit-after <frames>`).

## Reading the numbers

The shell shows the frame time over the last 120 frames, the workloads'
builds and restyles per second, and the element count. The hotpath report
splits the frame by function: bevy_markup's systems (`render_templates`,
`localize`, `build_html_ui` and their internals such as `pair_trees_into`,
`collect_items`, `update_children`, `HtmlStyles::get_with`) and the bench's
own model work (`testbench::churn`, `testbench::serialize_context`,
`scenarios::context`). Whatever the frame time holds beyond those is Bevy's
own layout, text and rendering. For example, a 2 000-row big document in
release spent about 4 ms per frame in `build_html_ui` against a ~65 ms frame,
which tells you where to optimise next.

## Files

| File | Contents |
|---|---|
| `main.rs` | The app: the in-memory asset source, the controls, starting scenarios, churn, the twin comparison, the checks, the shell's numbers |
| `scenarios.rs` | The scenarios: generated templates and stylesheets, the row model, the per-frame churn, the workload contexts |
| `../assets/testbench/shell.html` | The shell template (scenario list, controls, numbers, checks, the stage) |
| `../assets/testbench/style.css` | The shell's stylesheet |
| `../assets/testbench/locales/{en-US,de}/` | Fluent bundles (`markdown: true`): the shell's strings and the workloads' messages |
