world-on-screen = Namensschilder im Bild: { $count } von { $total }
world-hint = ← → / Steuerkreuz / linker Stick: wählen · Enter / A: drücken · K / B: treffen · R / X: alle zurück · Leertaste / Select: Schilder · L / Y: Sprache
world-hit = Nächste treffen
world-respawn = Alle zurück
world-plates = Schilder an/aus
world-language = Sprache: Deutsch

# Namensschilder. Übersetzungen sind Markup: <b>, <i> und <span class> sind in
# world/style.css gestaltet.
world-level = St. { $level }
world-title = { $epithet ->
    [brave] Tapfer
    [swift] Flink
    [wise] Weise
    [grim] Grimmig
    [quiet] Still
   *[lost] Verloren
} · <span class="{ $faction }">{ $faction ->
    [ally] verbündet
    [hostile] feindlich
   *[neutral] neutral
}</span>
world-status = <b>{ $hp }</b> LP · { $hits ->
    [0] <i>unversehrt</i>
    [one] <b>einmal</b> getroffen
   *[other] <b>{ $hits }</b>-mal getroffen
}
