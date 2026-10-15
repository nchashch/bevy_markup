//! The testbench's workloads: what each scenario generates (templates and
//! stylesheets written into the in-memory `testbench://` source, so they
//! go through the real loaders and their pre-scans), how its model churns
//! per frame, and the context each workload UI renders.

use std::path::Path;

use bevy::asset::io::memory::Dir;
use bevy::prelude::*;
use serde_json::{Value, json};

/// The scenario list, grouped as the shell shows it.
pub const GROUPS: [(&str, &[&str]); 3] = [
    (
        "perf",
        &[
            "big-document",
            "all-dirty",
            "restyle-storm",
            "markdown",
            "localized",
            "many-uis",
            "calc-layout",
        ],
    ),
    (
        "security",
        &[
            "nested-rules",
            "deep-dom",
            "var-bombs",
            "grid-bombs",
            "hostile-args",
            "args-injection",
            "remote-sources",
            "markdown-bombs",
        ],
    ),
    ("correctness", &["reuse-vs-fresh", "indent-vs-flat"]),
];

/// Every scenario id, in the shell's order.
pub fn all() -> Vec<&'static str> {
    GROUPS.iter().flat_map(|(_, ids)| ids.iter().copied()).collect()
}

/// The sizes the − / + controls step through (rows, cards or UIs).
pub const SIZES: [usize; 9] = [10, 50, 100, 250, 500, 1000, 2000, 5000, 10000];

/// One row of the row-based workloads' model.
#[derive(Clone, serde::Serialize)]
pub struct Row {
    pub id: u64,
    pub name: String,
    pub value: u64,
    pub hot: bool,
    pub pct: u32,
    pub px: u32,
}

/// The mutable state the workloads render.
#[derive(Default)]
pub struct Model {
    pub rows: Vec<Row>,
    pub tick: u64,
    pub next_id: u64,
    rng: u64,
}

impl Model {
    pub fn new(size: usize) -> Self {
        let mut model = Self {
            rng: 0x9e37_79b9_7f4a_7c15,
            ..default()
        };
        for _ in 0..size {
            let row = model.row();
            model.rows.push(row);
        }
        model
    }

    /// xorshift: deterministic, so a run is reproducible.
    pub fn random(&mut self, below: usize) -> usize {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % below.max(1) as u64) as usize
    }

    fn row(&mut self) -> Row {
        let id = self.next_id;
        self.next_id += 1;
        let pct = 10 + self.random(20) as u32;
        Row {
            id,
            name: format!("unit-{id}"),
            value: self.random(1000) as u64,
            hot: false,
            pct,
            px: self.random(12) as u32,
        }
    }

    /// The accent colour the restyle storm cycles through (a context
    /// `var()`).
    pub fn accent(&self) -> String {
        let hue = (self.tick * 7 % 360) as f32;
        let color = Color::hsl(hue, 0.7, 0.55).to_srgba();
        format!(
            "#{:02x}{:02x}{:02x}",
            (color.red * 255.0) as u8,
            (color.green * 255.0) as u8,
            (color.blue * 255.0) as u8
        )
    }
}

/// How a workload UI is laid out in the stage, and what it is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// The scenario's (first) UI; its context follows the model.
    Main,
    /// The second UI the checks compare with the main one.
    Twin,
    /// Card `index` of the many-UIs scenario.
    Card(usize),
    /// A fixed UI a check reads (`probe` names it).
    Probe(&'static str),
}

/// One workload UI to spawn.
pub struct Spec {
    pub template: String,
    pub stylesheet: Option<String>,
    pub role: Role,
    /// A fixed context (security scenarios); `None` = from the model.
    pub context: Option<Value>,
}

/// What a scenario needs: its UIs, the stage's layout for them, and the
/// checks the shell shows.
pub struct Plan {
    pub specs: Vec<Spec>,
    /// Lay the UIs out in a wrapping row (cards, side-by-side twins)
    /// instead of a column.
    pub row: bool,
    pub checks: &'static [&'static str],
}

/// The workloads' shared stylesheet.
const BENCH_CSS: &str = "\
html { color: #dfe6ee; font-size: 13px; font-family: sans-serif; flex-direction: column; gap: 2px; }
h1 { color: #f0b429; font-size: 18px; }
h2 { color: #9fb3c8; font-size: 15px; }
.row { flex-direction: row; gap: 8px; padding: 1px 4px; border-width: 1px; border-color: var(--accent, #2c3542); }
.row.hot { background-color: #3a2f12; }
.k { color: #c7d2de; }
.v { color: #7fdbff; font-family: monospace; }
.wrap { flex-direction: row; flex-wrap: wrap; gap: 4px; }
.card { padding: 4px; border-width: 1px; border-color: #2c3542; background-color: #1a2029; }
";

const ROWS_HTML: &str = "{% for row in rows %}<div class=\"row{% if row.hot %} hot{% endif %}\" id=\"row-{{ row.id }}\"><p class=\"k\">{{ row.name }}</p><p class=\"v\">{{ row.value }}</p></div>\n{% endfor %}";

const DIRTY_HTML: &str = "{% for row in rows %}<div class=\"row\" id=\"row-{{ row.id }}\"><p class=\"k\">{{ row.name }}</p><p class=\"v\">{{ row.value }} / {{ tick }}</p></div>\n{% endfor %}";

/// The Markdown document, indented the way authors write Tera (ADR 0024).
const DOC_INDENTED_MD: &str = "# Report {{ tick }}

{% for row in rows %}
    {% if row.hot %}
        ## {{ row.name }} (hot)
    {% else %}
        ## {{ row.name }}
    {% endif %}

    - value **{{ row.value }}**
    - id {{ row.id }}

{% endfor %}
";

/// The same document without indentation: must build the same UI.
const DOC_FLAT_MD: &str = "# Report {{ tick }}

{% for row in rows %}
{% if row.hot %}
## {{ row.name }} (hot)
{% else %}
## {{ row.name }}
{% endif %}

- value **{{ row.value }}**
- id {{ row.id }}

{% endfor %}
";

const LOCALIZED_HTML: &str = "{% for row in rows %}<p data-l10n-id=\"tb-w-line\" data-l10n-args='{{ {\"n\": row.value, \"who\": row.name} }}'>x</p>
<p data-l10n-id=\"tb-w-count\" data-l10n-args='{{ {\"n\": row.value} }}'>x</p>
{% endfor %}";

const CARD_HTML: &str = "<div class=\"card\"><p class=\"k\">UI {{ id }}</p><p class=\"v\">{{ value }}</p></div>";

const CALC_HTML: &str = "<div class=\"wrap\">{% for row in rows %}<div class=\"card\" style=\"width: calc({{ row.pct }}% - {{ row.px }}px)\"><p class=\"v\">{{ row.value }}</p></div>{% endfor %}</div>";

/// Writes `files` under `testbench://<prefix>/` and returns their asset
/// paths in order.
fn write(dir: &Dir, prefix: &str, files: &[(&str, String)]) -> Vec<String> {
    files
        .iter()
        .map(|(name, text)| {
            let path = format!("{prefix}/{name}");
            dir.insert_asset_text(Path::new(&path), text);
            format!("testbench://{path}")
        })
        .collect()
}

/// Generates `scenario`'s assets under a fresh `prefix` (a new path per
/// start, so every start loads anew) and returns its plan.
#[hotpath::measure]
pub fn plan(scenario: &str, dir: &Dir, prefix: &str) -> Plan {
    let main = |template: &str, sheet: &str, role: Role| Spec {
        template: template.to_owned(),
        stylesheet: Some(sheet.to_owned()),
        role,
        context: None,
    };
    let probe = |template: &str, sheet: &str, name: &'static str, context: Value| Spec {
        template: template.to_owned(),
        stylesheet: Some(sheet.to_owned()),
        role: Role::Probe(name),
        context: Some(context),
    };
    let column = |specs, checks| Plan {
        specs,
        row: false,
        checks,
    };
    match scenario {
        "big-document" | "restyle-storm" | "reuse-vs-fresh" => {
            let paths = write(dir, prefix, &[("rows.html", ROWS_HTML.into()), ("bench.css", BENCH_CSS.into())]);
            let checks: &'static [&'static str] = if scenario == "reuse-vs-fresh" {
                &["built", "twins", "budget"]
            } else {
                &["built", "budget"]
            };
            column(vec![main(&paths[0], &paths[1], Role::Main)], checks)
        }
        "all-dirty" => {
            let paths = write(dir, prefix, &[("rows.html", DIRTY_HTML.into()), ("bench.css", BENCH_CSS.into())]);
            column(vec![main(&paths[0], &paths[1], Role::Main)], &["built", "budget"])
        }
        "markdown" => {
            let paths = write(dir, prefix, &[("doc.md", DOC_INDENTED_MD.into()), ("bench.css", BENCH_CSS.into())]);
            column(vec![main(&paths[0], &paths[1], Role::Main)], &["built", "budget"])
        }
        "indent-vs-flat" => {
            let paths = write(
                dir,
                prefix,
                &[
                    ("indented.md", DOC_INDENTED_MD.into()),
                    ("flat.md", DOC_FLAT_MD.into()),
                    ("bench.css", BENCH_CSS.into()),
                ],
            );
            Plan {
                specs: vec![main(&paths[0], &paths[2], Role::Main), main(&paths[1], &paths[2], Role::Twin)],
                row: true,
                checks: &["built", "twins", "budget"],
            }
        }
        "localized" => {
            let paths = write(dir, prefix, &[("rows.html", LOCALIZED_HTML.into()), ("bench.css", BENCH_CSS.into())]);
            column(vec![main(&paths[0], &paths[1], Role::Main)], &["built", "budget"])
        }
        "many-uis" => {
            let paths = write(dir, prefix, &[("card.html", CARD_HTML.into()), ("bench.css", BENCH_CSS.into())]);
            // Card UIs are created by the caller from the model's rows.
            Plan {
                specs: vec![main(&paths[0], &paths[1], Role::Card(0))],
                row: true,
                checks: &["built", "budget"],
            }
        }
        "calc-layout" => {
            let paths = write(dir, prefix, &[("cards.html", CALC_HTML.into()), ("bench.css", BENCH_CSS.into())]);
            column(vec![main(&paths[0], &paths[1], Role::Main)], &["built", "budget"])
        }
        "nested-rules" => {
            let css = format!("html {{ color: #ff6b6b }}\np {{ {} }}\n", "a {".repeat(20_000));
            let paths = write(
                dir,
                prefix,
                &[
                    ("page.html", "<p>The sheet must fail its load: this text keeps the default colour.</p>".into()),
                    ("bomb.css", css),
                ],
            );
            column(vec![probe(&paths[0], &paths[1], "sheet", json!({}))], &["sheet-refused", "built", "budget"])
        }
        "deep-dom" => {
            let html = format!(
                "<p>Before the deep tree.</p>{}<p>deep</p>{}<p>After the deep tree.</p>",
                "<div>".repeat(5_000),
                "</div>".repeat(5_000)
            );
            let paths = write(dir, prefix, &[("page.html", html), ("bench.css", BENCH_CSS.into())]);
            column(vec![probe(&paths[0], &paths[1], "page", json!({}))], &["built", "budget"])
        }
        "var-bombs" => {
            let mut css = String::from("html { color: #dfe6ee; --c0: 1px; --d0: x; --f0: 1px; --x: var(--y, 1px); --y: var(--x, 2px); ");
            for i in 1..2000 {
                css.push_str(&format!("--c{i}: var(--c{}); ", i - 1));
            }
            for i in 1..40 {
                css.push_str(&format!("--d{i}: var(--d{0}, a) var(--d{0}, b); ", i - 1));
            }
            for i in 1..64 {
                css.push_str(&format!("--f{i}: {}var(--f{}){}; ", "calc(".repeat(20), i - 1, " + 1px)".repeat(20)));
            }
            css.push_str("}\n.chain { width: var(--c1999, 40px) }\n.double { padding-left: var(--d39, 3px) }\n.cycle { width: var(--x, 120px) }\n.fn { width: var(--f63, 80px) }\n");
            let html = "<p class=\"chain\">a 2 000-link chain</p><p class=\"double\">a doubling fallback chain</p><p class=\"cycle\">a cycle</p><p class=\"fn\">nested functions on every link</p>";
            let paths = write(dir, prefix, &[("page.html", html.into()), ("bomb.css", css)]);
            column(vec![probe(&paths[0], &paths[1], "page", json!({}))], &["built", "budget"])
        }
        "grid-bombs" => {
            let names: Vec<String> = (0..1000).map(|i| format!("a{i}")).collect();
            let areas = "\"a\" ".repeat(40_000);
            let css = format!(
                "html {{ color: #dfe6ee }}\n.g1 {{ display: grid; grid-template-columns: repeat(4000000000, 1px) }}\n.g2 {{ display: grid; grid-template-rows: repeat(1431655766, [a] 1px 1px 1px) }}\n.g3 {{ display: grid; grid-template-areas: {areas} }}\n.g4 {{ display: grid; grid-template-columns: repeat(1000, [{}] 1px) }}\n",
                names.join(" ")
            );
            let html = "<div class=\"g1\"><p>repeat(4 000 000 000)</p></div><div class=\"g2\"><p>a 32-bit wrap</p></div><div class=\"g3\"><p>40 000 area rows</p></div><div class=\"g4\"><p>1 000 names per repeat</p></div>";
            let paths = write(dir, prefix, &[("page.html", html.into()), ("bomb.css", css)]);
            column(vec![probe(&paths[0], &paths[1], "page", json!({}))], &["built", "budget"])
        }
        "hostile-args" => {
            let html = "{% for p in payloads %}<p data-l10n-id=\"tb-w-hostile\" data-l10n-args='{{ {\"n\": 2, \"who\": p} }}'>x</p>
<p data-l10n-id=\"tb-w-hostile-cached\" data-l10n-args='{{ {\"who\": p} }}'>x</p>
{% endfor %}";
            let paths = write(dir, prefix, &[("page.html", html.into()), ("bench.css", BENCH_CSS.into())]);
            let payloads = json!([
                "<a href=x data-on-click=boom>Continue</a>",
                "[Continue](https://evil.example) ![x](../../save.png)",
                "\u{1}bevy-markup\u{1}<b data-x=' class=\"\u{e9}'>x</b>",
                "*emphasis* **strong** `code` ~~strike~~ | a | b |",
                "line\n\n# Heading\n\n- item\n\n    indented",
                "{{ secret }} {% raw %} \u{f73c}b\u{f73e}",
                "Ada \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467} Jose\u{301}",
            ]);
            column(
                vec![probe(&paths[0], &paths[1], "page", json!({ "payloads": payloads }))],
                &["built", "no-forged", "no-fallback", "budget"],
            )
        }
        "args-injection" => {
            let html = "<p id=\"literal\" data-l10n-id=\"tb-w-greet\" data-l10n-args='{{ {\"name\": name} }}'>x</p>
<p id=\"splice\" data-l10n-id=\"tb-w-greet\" data-l10n-args='{\"name\": \"{{ name }}\"}'>x</p>";
            let paths = write(dir, prefix, &[("page.html", html.into()), ("bench.css", BENCH_CSS.into())]);
            column(
                vec![probe(&paths[0], &paths[1], "page", json!({ "name": "Ada\", \"extra\": \"INJECTED" }))],
                &["built", "map-literal", "budget"],
            )
        }
        "remote-sources" => {
            let paths = write(
                dir,
                prefix,
                &[
                    ("remote.html", "{% include \"https://evil.example/x.html\" %}".into()),
                    ("framed.html", "<p>The remote border-image was skipped; the sheet still applies.</p>".into()),
                    ("remote.css", "html { color: #7fdb8f; border-image: url(\"https://evil.example/f.png\") 4 stretch; border-width: 4px }".into()),
                    ("bench.css", BENCH_CSS.into()),
                ],
            );
            column(
                vec![
                    probe(&paths[0], &paths[3], "include", json!({})),
                    probe(&paths[1], &paths[2], "border", json!({})),
                ],
                &["include-refused", "border-skipped", "budget"],
            )
        }
        "markdown-bombs" => {
            let mut md = String::new();
            md.push_str(&format!("{}a\n\n", "*".repeat(20_000)));
            md.push_str(&format!("{}a\n\n", "[".repeat(20_000)));
            md.push_str(&format!("{}quote\n\n", "> ".repeat(2_000)));
            md.push_str("\u{1}bevy-markup\u{1}<b data-x=' class=\"\u{e9}'>x</b>\n\n");
            for depth in 0..30 {
                md.push_str(&format!("{}{{% if true %}}\n", "    ".repeat(depth)));
            }
            md.push_str(&format!("{}deep **text**\n", "    ".repeat(30)));
            for depth in (0..30).rev() {
                md.push_str(&format!("{}{{% endif %}}\n", "    ".repeat(depth)));
            }
            md.push_str(&"    an indented line\n".repeat(10_000));
            let paths = write(dir, prefix, &[("bomb.md", md), ("bench.css", BENCH_CSS.into())]);
            column(vec![probe(&paths[0], &paths[1], "page", json!({}))], &["survived", "budget"])
        }
        _ => column(Vec::new(), &[]),
    }
}

/// What a frame's churn changed.
pub enum Changed {
    None,
    All,
    Card(usize),
}

/// One frame of `scenario`'s churn on the model.
#[hotpath::measure]
pub fn churn(scenario: &str, model: &mut Model) -> Changed {
    model.tick += 1;
    let len = model.rows.len();
    if len == 0 {
        return Changed::None;
    }
    // About one row in a hundred.
    let sparse = (len / 100).max(1);
    match scenario {
        "big-document" | "localized" | "markdown" | "indent-vs-flat" => {
            for _ in 0..sparse {
                let index = model.random(len);
                model.rows[index].value = model.random(1000) as u64;
            }
            if scenario == "indent-vs-flat" {
                let index = model.random(len);
                model.rows[index].hot ^= true;
            }
            Changed::All
        }
        "all-dirty" => {
            for row in &mut model.rows {
                row.value += 1;
            }
            Changed::All
        }
        "restyle-storm" => {
            let tick = model.tick as usize;
            model.rows[(tick + len - 1) % len].hot = false;
            model.rows[tick % len].hot = true;
            Changed::All
        }
        "calc-layout" => {
            for _ in 0..sparse {
                let index = model.random(len);
                model.rows[index].pct = 10 + model.random(20) as u32;
                model.rows[index].px = model.random(12) as u32;
            }
            Changed::All
        }
        "many-uis" => {
            let index = model.random(len);
            model.rows[index].value += 1;
            Changed::Card(index)
        }
        "reuse-vs-fresh" => {
            // A random edit: change, insert, remove, move or highlight —
            // the shapes the keyed/positional pairing and the reuse cache
            // have to get right.
            match model.random(5) {
                0 => {
                    let index = model.random(len);
                    model.rows[index].value = model.random(1000) as u64;
                }
                1 if len < 4 * SIZES[SIZES.len() - 1] => {
                    let index = model.random(len + 1);
                    let row = model.row();
                    model.rows.insert(index, row);
                }
                2 if len > 1 => {
                    let index = model.random(len);
                    model.rows.remove(index);
                }
                3 if len > 1 => {
                    let index = model.random(len - 1);
                    model.rows.swap(index, index + 1);
                }
                _ => {
                    let index = model.random(len);
                    model.rows[index].hot ^= true;
                }
            }
            Changed::All
        }
        _ => Changed::None,
    }
}

/// The context of a model-driven workload UI. Only the restyle storm
/// carries the `--accent` var (a per-frame `var()` change restyles every
/// UI whose sheet reads it).
#[hotpath::measure]
pub fn context(scenario: &str, model: &Model, role: Role) -> Value {
    match role {
        Role::Card(index) => {
            let row = &model.rows[index.min(model.rows.len().saturating_sub(1))];
            json!({ "id": index, "value": row.value })
        }
        _ if scenario == "restyle-storm" => {
            json!({ "rows": model.rows, "tick": model.tick, "--accent": model.accent() })
        }
        _ => json!({ "rows": model.rows, "tick": model.tick }),
    }
}
