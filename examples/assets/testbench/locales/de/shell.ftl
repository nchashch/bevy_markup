tb-title = Prüfstand
tb-group-perf = Leistung
tb-group-security = Sicherheit
tb-group-correctness = Korrektheit

tb-sc-big-document = Großes Dokument
tb-sc-all-dirty = Alles geändert
tb-sc-restyle-storm = Restyle-Sturm
tb-sc-markdown = Eingerücktes Markdown
tb-sc-localized = Lokalisierte Zeilen
tb-sc-many-uis = Viele UIs
tb-sc-calc-layout = calc()-Layout
tb-sc-nested-rules = Verschachtelte CSS-Regeln
tb-sc-deep-dom = Tiefes DOM
tb-sc-var-bombs = var()-Bomben
tb-sc-grid-bombs = Grid-Bomben
tb-sc-hostile-args = Feindliche Fluent-Argumente
tb-sc-args-injection = Argument-JSON-Injektion
tb-sc-remote-sources = Entfernte Quellen
tb-sc-markdown-bombs = Markdown-Bomben
tb-sc-reuse-vs-fresh = Wiederverwendung gegen Neubau
tb-sc-indent-vs-flat = Eingerückt gegen flach

tb-desc-big-document = Ein Dokument aus N Zeilen; pro Frame ändert sich etwa eine von hundert, unveränderte Zeilen verwenden den vorigen Aufbau wieder.
tb-desc-all-dirty = Jede Zeile ändert sich in jedem Frame: Rendern, Parsen, Kaskade und Abgleich vollständig.
tb-desc-restyle-storm = Eine Kontext-var() färbt jede Zeile neu, und eine Hervorhebung wandert pro Frame: Neugestaltung an Ort und Stelle.
tb-desc-markdown = Eine Markdown-Vorlage mit eingerückten Tera-Blöcken, in jedem Frame neu gerendert.
tb-desc-localized = N Zeilen Fluent-Nachrichten in einem Markdown-Bündel, gecachter und Pro-Wert-Pfad, Argumente ändern sich pro Frame.
tb-desc-many-uis = N getrennte kleine UIs; pro Frame wird eine davon aktualisiert.
tb-desc-calc-layout = N Karten mit calc() aus gemischten Einheiten, pro Frame gegen die Bühne aufgelöst.
tb-desc-nested-rules = Ein Stylesheet aus 20 000 verschachtelten Regeln: das Laden muss sauber scheitern, statt die App abzubrechen.
tb-desc-deep-dom = 5 000 verschachtelte Elemente: Inhalt jenseits von 100 Ebenen entfällt, der Rest wird aufgebaut.
tb-desc-var-bombs = var()-Ketten, sich verdoppelnde Ersatzwerte, Zyklen und verschachtelte Funktionen: die Ersetzung bleibt begrenzt.
tb-desc-grid-bombs = Grid-Spurzahlen, die früher überliefen: die Deklarationen entfallen, nichts stürzt ab.
tb-desc-hostile-args = Fluent-Textargumente mit HTML, Markdown-Links und -Bildern und präparierten Markern: als Text angezeigt, nichts gefälscht.
tb-desc-args-injection = Argumente aus einem feindlichen Namen auf zwei Arten gebaut: das Tera-Map-Literal behält ihn ganz, das handgeschriebene JSON ist injizierbar.
tb-desc-remote-sources = Ein https://-Include und border-image: das Include scheitert beim Laden, das Bild wird übersprungen.
tb-desc-markdown-bombs = Pathologisches Markdown: tiefe Hervorhebung, Klammern und Zitate, präparierte Klassenmarker, verschachtelte eingerückte Tera-Blöcke.
tb-desc-reuse-vs-fresh = Eine UI, die in jedem Frame zufällig verändert wird, wird alle paar Frames mit einem frischen Aufbau desselben Zustands verglichen.
tb-desc-indent-vs-flat = Eine eingerückte und eine flache Markdown-Vorlage desselben Dokuments müssen dieselbe UI ergeben.

tb-size = Größe: { $size }
tb-churn =
    { $on ->
        [true] Änderungen: an
       *[other] Änderungen: aus
    }
tb-restart = Neustart
tb-language = Sprache

tb-stat-frame = Frame { $avg } ms, max. { $max } ms, { $fps } fps
tb-stat-updates = Aufbauten { $builds }/s, Neugestaltungen { $restyles }/s
tb-stat-size = UIs { $uis }, Elemente { $elements }

tb-check-budget =
    { $state ->
        [pass] OK
        [fail] FEHLER
       *[other] WARTE
    } Frame-Budget: schlechtester { $detail } ms
tb-check-built =
    { $state ->
        [pass] OK
        [fail] FEHLER
       *[other] WARTE
    } aufgebaut { $detail }
tb-check-survived =
    { $state ->
        [pass] OK
        [fail] FEHLER
       *[other] WARTE
    } überlebt { $detail }
tb-check-sheet-refused =
    { $state ->
        [pass] OK
        [fail] FEHLER
       *[other] WARTE
    } Stylesheet abgelehnt { $detail }
tb-check-no-forged =
    { $state ->
        [pass] OK
        [fail] FEHLER
       *[other] WARTE
    } nichts gefälscht { $detail }
tb-check-no-fallback =
    { $state ->
        [pass] OK
        [fail] FEHLER
       *[other] WARTE
    } jede Nutzlast lokalisiert { $detail }
tb-check-map-literal =
    { $state ->
        [pass] OK
        [fail] FEHLER
       *[other] WARTE
    } Map-Literal behielt den Wert { $detail }
tb-check-include-refused =
    { $state ->
        [pass] OK
        [fail] FEHLER
       *[other] WARTE
    } entferntes Include abgelehnt { $detail }
tb-check-border-skipped =
    { $state ->
        [pass] OK
        [fail] FEHLER
       *[other] WARTE
    } entferntes Bild übersprungen { $detail }
tb-check-twins =
    { $state ->
        [pass] OK
        [fail] FEHLER
       *[other] WARTE
    } Zwillinge gleich { $detail }

tb-hint = N / P oder LB / RB: Szenario. − / + oder [ / ] oder LT / RT: Größe. C / X: Änderungen. R / Select: Neustart. L / Y: Sprache. Mit --features hotpath und `hotpath console` live profilieren.
