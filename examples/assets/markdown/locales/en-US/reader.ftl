reader-tab-release = Release notes
reader-tab-tutorial = Tutorial
reader-language = Language: English
reader-hint = Click, or <kbd>↑</kbd> <kbd>↓</kbd> / D-pad and <kbd>Enter</kbd> / A. <kbd>Q</kbd> <kbd>E</kbd> / bumpers switch pages, <kbd>Space</kbd> / Y switches the language.
release-body = ## New in 0.5.2

  This release is about *reading* and *writing*: Markdown in templates and
  translations, inline styling on runs, and images **inside** your text.

  - Inline `background-color` paints behind a run — like a marker pen.
  - `text-decoration` underlines or strikes runs, with its own color.
  - **`<img>`** flows an image into the text, aspect ratio kept.
  - `border-radius: h / v` draws elliptical corners — this panel has them.
  - `position: fixed` pins a ribbon to the viewport, parents be damned.
  - Text **inside** a run, never starting a line: FTL reads a line-initial
    `*` as a variant marker.

  > All of it is plain Bevy UI: no web view, no textures of text.

  Done reading? ~~Neither were we.~~
tutorial-body = ## Writing a page

  A page is a `.md` file the app loads as a template:

  1. Tera fills {"{"}{"{"} braces {"}"}{"}"} from the context.
  2. The result converts to HTML (this list is a Markdown list).
  3. `data-l10n-id` replaces this article with *this very text* — the
     prose you are reading is a Fluent value in Markdown.

  - [x] Headings, emphasis, lists
  - [x] Block quotes and code:

        ```
        cargo run --example markdown
        ```
  - [ ] Your first page

  Missing: tables and links have no UI mapping yet — they render as text.
