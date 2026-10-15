# 25. Indentation inside raw HTML containers is not Markdown

| Field | Content |
|---|---|
| ADR | 0025 |
| Title | Indentation inside raw HTML containers is not Markdown |
| Date | 2026-10-15 04:26 +0400 |
| Author | Claude Opus 5.5 (Anthropic), via Claude Code |
| Commit | `ce19a69 Add review_0004, fix CI failure` + uncommitted (review_0004's fix session, ADR 0024, this change) |
| Status | Accepted |
| Related | 0024 (extends it; its "Still open"), 0014, 0020; review_0004 H11; bug_0126, bug_0127 |

## Context

ADR 0024 dedents Tera block bodies in `.md` templates so that authors can
indent for readability. Its Consequences left the same problem open for raw
HTML containers. A Markdown page embeds real UI as raw HTML (the `markdown`
example's tabs and buttons), and Markdown inside an HTML element needs blank
lines around it (CommonMark §4.6: an HTML block ends at a blank line):

```
<div class="card">

    Some **text**

</div>
```

The indented line after the blank line is an indented code block, so this
rendered `<pre><code>Some **text**</code></pre>`. Indenting a container's
body is how HTML is normally written. The project owner asked for the same
treatment as Tera blocks (2026-10-15), under the same rule: authors must not
be constrained by parsing.

## Decision

1. **Own-line HTML containers are blocks in `md::dedent_blocks`**, alongside
   Tera blocks.
   - A line holding exactly one opening tag (`<div class="card">`) opens one;
     a line holding its closing tag (`</div>`, matched case-insensitively)
     closes it (`md::html_tag`).
   - Their bodies lose their shared indentation exactly as Tera bodies do
     (`dedent_segment`, recursively; HTML and Tera blocks nest in any order
     on one stack, `BlockKey::{Tera, Html}`).
2. **Not containers:** void elements (`VOID_ELEMENTS`: `br`, `img`, `input`,
   …), self-closing tags (`<x />`), comments, and lines holding more than one
   tag (`<p>hi</p>`). These are content.
3. **Verbatim elements:** `<pre>`, `<textarea>`, `<script>` and `<style>`
   bodies are neither dedented nor searched (`BlockKey::body` → `None`),
   because their whitespace is content. As with `{% raw %}`, only their own
   closer means anything inside them.

## Alternatives considered

- **Only CommonMark's block-level tag list** (type-6 names such as `div`,
  `section`, `table`). Rejected: an own-line custom element or `<span>`
  body indents the same way, and the dedent is harmless wherever the body is
  raw HTML. A body without blank lines stays one HTML block either way.
- **Also insert blank lines inside containers**, so Markdown in a
  `<div>` works without them. Not done: it changes structure, not
  indentation. Raw-HTML lines inside a container would become paragraphs
  (`<p><span>…</span></p>`). That's a separate decision; see Consequences.

## Consequences

- The indented container builds what its flat twin builds
  (`indented_html_containers_in_markdown_build_like_flat_ones`,
  tests/html_ui.rs). `md::tests::indented_html_container_bodies_are_not_code`
  pins nesting with Tera, case-insensitive closers, `<pre>` verbatim, and the
  non-container lines. Both fail with `html_tag` disabled.
  `dedent_survives_token_soup` now includes HTML tokens.
- **Still CommonMark:** Markdown inside a container still needs the blank
  lines (`<div>`, `**tight**`, `</div>` keeps `**tight**` literal, inside
  one HTML block). A line indented four spaces outside any block is still an
  indented code block. Both are Markdown's own rules, not author
  indentation.
