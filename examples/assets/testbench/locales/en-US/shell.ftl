tb-title = Testbench
tb-group-perf = Performance
tb-group-security = Security
tb-group-correctness = Correctness

tb-sc-big-document = Big document
tb-sc-all-dirty = All dirty
tb-sc-restyle-storm = Restyle storm
tb-sc-markdown = Indented Markdown
tb-sc-localized = Localized rows
tb-sc-many-uis = Many UIs
tb-sc-calc-layout = calc() layout
tb-sc-nested-rules = Nested CSS rules
tb-sc-deep-dom = Deep DOM
tb-sc-var-bombs = var() bombs
tb-sc-grid-bombs = Grid bombs
tb-sc-hostile-args = Hostile Fluent args
tb-sc-args-injection = Args JSON injection
tb-sc-remote-sources = Remote sources
tb-sc-markdown-bombs = Markdown bombs
tb-sc-reuse-vs-fresh = Reuse vs fresh build
tb-sc-indent-vs-flat = Indented vs flat

tb-desc-big-document = One document of N rows; churn changes about one row in a hundred per frame, so unchanged rows reuse the previous build.
tb-desc-all-dirty = Every row changes every frame: the full render, parse, cascade and diff.
tb-desc-restyle-storm = A context var() recolours every row and a highlight moves each frame: restyles in place.
tb-desc-markdown = A Markdown template with indented Tera blocks, re-rendered every frame.
tb-desc-localized = N rows of Fluent messages in a Markdown bundle, cached and per-value paths, arguments changing per frame.
tb-desc-many-uis = N separate small UIs; churn updates one of them per frame.
tb-desc-calc-layout = N cards sized by mixed-unit calc(), resolved per frame against the stage.
tb-desc-nested-rules = A stylesheet of 20 000 nested rules: its load must fail cleanly instead of aborting the app.
tb-desc-deep-dom = 5 000 nested elements: content past 100 levels is dropped and the rest builds.
tb-desc-var-bombs = var() chains, doubling fallbacks, cycles and nested functions: the substitution stays bounded.
tb-desc-grid-bombs = Grid track counts that used to overflow: the declarations drop, nothing panics.
tb-desc-hostile-args = Fluent string arguments carrying HTML, Markdown links and images and crafted markers: shown as text, nothing forged.
tb-desc-args-injection = Arguments built two ways from a hostile name: the Tera map literal keeps it whole, the hand-written JSON splice is injectable.
tb-desc-remote-sources = An https:// include and border-image: the include fails its load and the image is skipped.
tb-desc-markdown-bombs = Pathological Markdown: deep emphasis, brackets and quotes, crafted class markers, nested indented Tera blocks.
tb-desc-reuse-vs-fresh = A UI mutated at random every frame is compared with a fresh build of the same state every few frames.
tb-desc-indent-vs-flat = An indented and a flat Markdown template of the same document must build the same UI.

tb-size = Size: { $size }
tb-churn =
    { $on ->
        [true] Churn: on
       *[other] Churn: off
    }
tb-restart = Restart
tb-language = Language

tb-stat-frame = Frame { $avg } ms, max { $max } ms, { $fps } fps
tb-stat-updates = Builds { $builds }/s, restyles { $restyles }/s
tb-stat-size = UIs { $uis }, elements { $elements }

tb-check-budget =
    { $state ->
        [pass] PASS
        [fail] FAIL
       *[other] WAIT
    } frame budget: worst { $detail } ms
tb-check-built =
    { $state ->
        [pass] PASS
        [fail] FAIL
       *[other] WAIT
    } built { $detail }
tb-check-survived =
    { $state ->
        [pass] PASS
        [fail] FAIL
       *[other] WAIT
    } survived { $detail }
tb-check-sheet-refused =
    { $state ->
        [pass] PASS
        [fail] FAIL
       *[other] WAIT
    } stylesheet refused { $detail }
tb-check-no-forged =
    { $state ->
        [pass] PASS
        [fail] FAIL
       *[other] WAIT
    } nothing forged { $detail }
tb-check-no-fallback =
    { $state ->
        [pass] PASS
        [fail] FAIL
       *[other] WAIT
    } every payload localized { $detail }
tb-check-map-literal =
    { $state ->
        [pass] PASS
        [fail] FAIL
       *[other] WAIT
    } map literal kept the value { $detail }
tb-check-include-refused =
    { $state ->
        [pass] PASS
        [fail] FAIL
       *[other] WAIT
    } remote include refused { $detail }
tb-check-border-skipped =
    { $state ->
        [pass] PASS
        [fail] FAIL
       *[other] WAIT
    } remote image skipped { $detail }
tb-check-twins =
    { $state ->
        [pass] PASS
        [fail] FAIL
       *[other] WAIT
    } twins match { $detail }

tb-hint = N / P or LB / RB: scenario. − / + or [ / ] or LT / RT: size. C / X: churn. R / Select: restart. L / Y: language. Run with --features hotpath and `hotpath console` for live profiling.
