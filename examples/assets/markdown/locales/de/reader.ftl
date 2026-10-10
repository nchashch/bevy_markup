reader-tab-release = Versionshinweise
reader-tab-tutorial = Kurzanleitung
reader-language = Sprache: Deutsch
reader-hint = Klicken, oder <kbd>↑</kbd> <kbd>↓</kbd> / Steuerkreuz und <kbd>Enter</kbd> / A. <kbd>Q</kbd> <kbd>E</kbd> / Schultertasten wechseln die Seite, <kbd>Leertaste</kbd> / Y die Sprache.
release-body = ## Neu in 0.5.2

  Dieses Release dreht sich um *Lesen* und *Schreiben*: Markdown in
  Templates und Übersetzungen, Inline-Stil auf Textläufen und Bilder
  mitten im Text.

  - Inline-`background-color` zeichnet hinter einem Lauf — wie ein Textmarker.
  - `text-decoration` unterstreicht oder durchstreicht Läufe, in eigener Farbe.
  - **`<img>`** bringt ein Bild in den Text, Seitenverhältnis bleibt.
  - `border-radius: h / v` zeichnet elliptische Ecken — dieses Panel hat sie.
  - `position: fixed` nagt ein Band an den Viewport, Eltern sind egal.
  - Text **innerhalb** eines Laufs, nie am Zeilenanfang: FTL liest ein
    zeilenanfängliches `*` als Variantenmarker.

  > Alles davon ist normales Bevy-UI: kein Web-View, keine Text-Texturen.

  Fertig gelesen? ~~Wir auch nicht.~~
tutorial-body = ## Eine Seite schreiben

  Eine Seite ist eine `.md`-Datei, die die App als Template lädt:

  1. Tera füllt {"{"}{"{"} braces {"}"}{"}"} aus dem Kontext.
  2. Das Ergebnis wird nach HTML konvertiert (diese Liste ist eine
     Markdown-Liste).
  3. `data-l10n-id` ersetzt diesen Artikel durch *genau diesen Text* —
     der Prosa, den du liest, ist ein Fluent-Wert in Markdown.

  - [x] Überschriften, Betonung, Listen
  - [x] Zitate und Code:

        ```
        cargo run --example markdown
        ```
  - [ ] Deine erste Seite

  Fehlt: Tabellen und Links haben noch kein UI-Mapping — sie erscheinen
  als Text.
