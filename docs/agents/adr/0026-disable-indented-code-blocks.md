# 26. Disable indented code blocks in Markdown (fenced code only)

| Field | Content |
|---|---|
| ADR | 0026 |
| Title | Disable indented code blocks in Markdown (fenced code only) |
| Date | 2026-10-15 04:38 +0400 |
| Author | Claude Opus 5.5 (Anthropic), via Claude Code |
| Commit | `4e908e5 Review 4` |
| Status | Proposed (decision deferred by the project owner, 2026-10-15) |
| Related | 0014, 0020, 0024, 0025; review_0004 H11; the project rule "authors must not be constrained by parsing" |

## Context

CommonMark has two kinds of code block, and both become `<pre><code>`:

- **fenced:** lines between ` ``` ` or `~~~` markers;
- **indented:** a line indented four or more spaces that can't continue a
  paragraph (after a blank line, at the start of a list item's content, …).
  Indentation alone makes it code; this is original Markdown's syntax.

Indented code is the reason readable indentation breaks Markdown templates.
ADRs 0024 (Tera block bodies) and 0025 (raw HTML container bodies) remove the
indentation authors add *inside blocks*. Indentation anywhere else still
turns into code: a line indented for alignment at the top level after a blank
line, or an indented line inside a list or blockquote beyond its container's
column. The project owner asked whether indented code can be switched off so
that only fenced blocks are code.

`pulldown-cmark` 0.13 has no option for it: its `Options` only add
extensions (tables, footnotes, …), and indented code is core CommonMark.
MDX (Markdown for JSX) is the precedent: it disables indented code so that
authors can indent freely, and fenced blocks are its only code syntax.
[INFERENCE] from MDX's documented "indented code is not supported"; not
re-checked against a current MDX release.

## Proposed decision

1. Before every Markdown conversion (`md::convert`: `.md` templates *and*
   Fluent values with `markdown: true`, so the same text means the same
   thing in both), find the blocks pulldown would read as indented code
   (`Event::Start(Tag::CodeBlock(CodeBlockKind::Indented))` with their
   offset ranges).
2. From each of their lines, remove the code indentation: four columns plus
   the block's shared extra indent, measured after any container prefix
   (`>` markers, a list item's content column). The block then parses as a
   paragraph of its container: top level, list item, or quote. Re-parse
   until no indented code is left (bounded; usually one pass).
3. Do the same in the scanners' view (`template_code_ranges`, applied after
   `separate_statements`, mapped back like ADR 0024's dedent), so that the
   include scanner and statement separation keep agreeing with the
   conversion: an include on a formerly "indented code" line is a reference.

Fenced blocks and inline code spans stay the only code. ADRs 0024–0025 stay
in place: their dedent still decides structure (an indented `- item` inside
a block lines up as a top-level list rather than a nested one).

## Alternatives considered

- **Keep CommonMark (status quo).** Authors must know that four spaces mean
  code; ADRs 0024/0025 cover the block bodies, the common case. This is the
  fallback if the decision is "no".
- **Strip all leading whitespace before conversion.** Rejected: list nesting
  and list-item continuation depend on indentation relative to the marker
  (`- a` / `  - b`).
- **Patch or fork pulldown-cmark to skip the indented-code rule.** Exact, but
  a fork to maintain (cf. the fluent-syntax fork, UPSTREAM.md U1/U2), and an
  upstream option is unlikely to be accepted for a CommonMark core rule
  [INFERENCE]. The re-parse approach in 2. needs no fork.

## Consequences (if accepted)

- **Breaking:** a 4-space-indented code sample in a `.md` template or a
  Markdown Fluent value becomes a paragraph; it needs a fence. CHANGELOG
  entry marked **Breaking**.
- **Cost:** one extra pulldown parse per conversion when the text holds an
  indented code block, and none otherwise (the check is the first parse's
  events). Template conversion runs once at load (ADR 0020); per-value
  Fluent conversion runs per frame for uncacheable messages.
- **Tests to write:** indented samples at the top level, in list items and
  in quotes convert as paragraphs of their container; fenced blocks are
  untouched; an indented ` ``` ` line doesn't open a fence (it is paragraph
  text, as in a parser without indented code); the scanners agree (an
  include on such a line is a reference); token-soup robustness.
- **Related open question** (not part of this ADR): Markdown inside a raw
  HTML element needs blank lines around it (CommonMark §4.6). Inserting them
  automatically is a separate structural change (raw-HTML lines inside a
  container would become paragraphs); see ADR 0025's Consequences.
