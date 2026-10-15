# 24. Indentation inside Tera blocks is not Markdown

| Field | Content |
|---|---|
| ADR | 0024 |
| Title | Indentation inside Tera blocks is not Markdown |
| Date | 2026-10-15 04:14 +0400 |
| Author | Claude Opus 5.5 (Anthropic), via Claude Code |
| Commit | `ce19a69 Add review_0004, fix CI failure` + uncommitted (review_0004's fix session and this change) |
| Status | Accepted |
| Related | 0014, 0020; review_0004 H11; bug_0108 (pulldown decides what is code); bug_0126 |

## Context

A `.md` template mixes two syntaxes. ADR 0020 converts its Markdown once at
load with Tera syntax intact. `md::separate_statements` puts each own-line
Tera statement on lines of its own, with blank lines around it, so a
statement can't merge with neighbouring Markdown. Every structural Tera
statement therefore ends up as its own paragraph-level item.

Authors indent Tera block bodies as they would in any template:

```
{% for unit in units %}
    {% if unit.hp > 0 %}
        - **{{ unit.name }}** is up
    {% endif %}
{% endfor %}
```

In CommonMark, a line indented four or more spaces after a blank line is an
indented code block. The blank line that separation inserts after
`{% if %}` turned every such body into code: `*text*` showed literally in a
`<pre>`. Probed in review_0004 H11:
`to_html_template("{% if a %}\n    Some *text*\n{% endif %}\n")` produced
`<pre class="markdown"><code>Some *text*…`. Without separation the same
line would have been a lazy paragraph continuation. Either way the author's
indentation decided the Markdown structure, which the author never meant.

The project owner's requirement (2026-10-15): authors must not be
constrained by parsing. They should be able to write `.md`, `.html` and Tera
templates in whatever way reads best, including indenting.

`.html` templates are unaffected: HTML collapses whitespace outside `<pre>`.

## Decision

1. **Tera block bodies are dedented before conversion**
   (`md::dedent_blocks`, run by `to_html_template` before
   `separate_statements`).
   - A block is an own-line `if`/`for`/`block`/`component`/`filter`/`raw`
     or block-form `set`/`set_global` statement (no `=`) with its matching
     `end…`.
   - Its `elif`/`else` lines split it into segments (`md::block_tag`), and
     each segment loses the indentation its non-blank lines share
     (`dedent_segment`).
   - The pass recurses outermost first (`dedent_level`), so nested bodies
     line up with their own statements.
   - A body's *relative* indentation stays Markdown's: an indented code
     block or a nested list inside a body keeps its extra indent.
2. **Code is content, not structure.** At each level, pulldown decides
   which lines are code (`code_ranges` over the level's current text, as in
   bug_0108). An own-line statement inside code, such as a Tera sample in a
   fence, is neither a block nor dedented. Inside a fence that a body
   contains, the inner lines don't count toward the shared indentation (a
   fence's content may sit at column 0). A `{% raw %}` body is dedented, but
   its statements are text, as Tera reads them.
3. **Only leading whitespace is removed, and line structure is kept.**
   Tabs count to the next multiple of four, as in CommonMark. `dedent_blocks`
   returns a per-line offset map (`LineMap`, `dedented_to_source`), and
   `template_code_ranges` maps code ranges back through both the dedent and
   the separation. The include scanner (`template::resolve_references`)
   therefore keeps agreeing with the conversion: an include on an indented
   body line is a reference.
4. **Bounded.** Each level re-parses its body, so the pass is O(size ×
   depth). `MAX_DEDENT_DEPTH` = 16 levels; deeper bodies keep their
   indentation.

## Alternatives considered

- **Keep CommonMark's reading and document "don't indent inside Tera
  blocks."** Rejected by the requirement above. It also leaves the same
  source meaning different things depending on whether a statement line
  happens to precede it.
- **Separate with a single newline when the next line is indented** (no
  blank line, so the body becomes a lazy paragraph continuation). Rejected:
  the statement would merge into that paragraph, which is exactly what
  separation exists to prevent (ADR 0020). A body starting with a list or
  heading would still not convert as one.
- **Dedent every line by the indentation of its enclosing statement** (the
  statement's column, not the body's shared indentation). Rejected: bodies
  indented by a different amount than one level (two spaces under a
  four-space statement) would keep part of their indentation. The shared
  indentation is what the author added for readability.
- **Find structure once, globally, with one parse.** Not possible: what is
  code depends on the indentation being removed (a fence indented inside a
  body reads as indented code before the dedent). A per-level parse is the
  simplest exact answer; the depth cap bounds it.

## Consequences

- Indented and flat templates build the same UI
  (`indented_markdown_templates_build_like_flat_ones`, tests/html_ui.rs).
  The shapes are pinned by `md::tests::indented_tera_bodies_are_not_code`;
  offset mapping by `dedent_maps_offsets_back`; robustness by
  `dedent_survives_token_soup`; the scanner case by
  `template::scanner_code_is_commonmark_code` ("indented block body").
  Each fails with the pass disabled.
- **Behaviour change:** a template that relied on an indented block body
  rendering as code gets Markdown instead. Code inside a block now needs a
  fence, or an indent beyond the body's own.
- **Cost, measured** with a `--release` test probe (`Instant` around
  `dedent_blocks`): 620 KB of 10 000 nested, increasingly indented `if`
  blocks took 176 ms at a depth cap of 64 and 48 ms at 16. In a debug
  build, 2.8 s at a cap of 64. Authored templates nest a few levels and pay
  one pulldown parse per block body. `scanners_stay_linear_on_pathological_input`
  includes the nested shape (4× input ≤ 8× time).
- **Still open:** indentation inside raw HTML blocks in a `.md`
  (`<div>\n\n    **text**\n\n</div>`) is still CommonMark's indented code.
  That is the same authoring problem for HTML containers, not Tera blocks.
  It is not decided here.
