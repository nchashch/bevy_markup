hr-title = Hot Reload
hr-palette = Palette: { $palette ->
    [0] Karmesin
    [1] Ozean
    [2] Wald
   *[other] Unbekannt
}
hr-cycle = Palette wechseln (H)
hr-add-coin = + Eine Münze
hr-coins =
    { $coins ->
        [0] Du hast <em>keine</em> Münzen.
        [one] Du hast <em>eine</em> Münze.
       *[other] Du hast <em>{ $coins }</em> Münzen.
    }
hr-language = Sprache: Deutsch
hr-hint = Drücke <kbd>H</kbd> (oder den Knopf), um das Stylesheet neu zu schreiben — die UI wird ohne Neustart umgestaltet. Auch das Bearbeiten der Dateien unter <kbd>examples/assets/hot-reload/</kbd> funktioniert.
