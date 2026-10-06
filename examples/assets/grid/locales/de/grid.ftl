grid-title = Grid-Layout
grid-hint = Pfeile, Steuerkreuz oder linker Stick: durch das Raster · <kbd>Enter</kbd> / A: drücken · <kbd>Q</kbd> <kbd>E</kbd> / LB RB: Reiter · <kbd>Leertaste</kbd> / X: Layout · <kbd>L</kbd> / Y: Sprache; ändere die Fenstergröße, um die Felder neu anzuordnen.
grid-layout = Layout
grid-language = Sprache: Deutsch
grid-tab-items = Gegenstände
grid-tab-gear = Ausrüstung
grid-tab-quests = Aufträge
grid-details = Details
grid-stat-items = Arten
grid-stat-weight = Gewicht
grid-stat-gold = Gold
grid-status =
    { $layout ->
        [wide] Breites Layout: drei Spalten, Banner und Statusleiste überspannen alle.
       *[narrow] Schmales Layout: eine Spalte, die Bereiche stapeln sich in Quelltextreihenfolge.
    }
grid-item-map = Karte
grid-item-sword = Schwert
grid-item-shield = Schild
grid-item-potion = Trank
grid-item-herb = Kraut
grid-item-lantern = Laterne
grid-item-rope = Seil
grid-item-gem = Edelstein
grid-item-key = Schlüssel
grid-screen =
    { $tab ->
        [gear] Ausrüstung: ein festes 3 × 4-Raster, jedes Feld über Liniennummern platziert, die Figur überspannt zwei Zeilen.
        [quests] Aufträge: drei gleiche Spalten aus verschachtelten Kartenrastern unter einem Hauptauftrag, der alle drei überspannt.
       *[items] Gegenstände: so viele Spalten wie passen (auto-fill), ein 2 × 2-Feld, dichte Packung.
    }
grid-gear-hero = Ada
grid-gear-figure = die Tapfere
grid-gear-neck = Hals
grid-gear-head = Kopf
grid-gear-back = Rücken
grid-gear-main-hand = Haupthand
grid-gear-off-hand = Nebenhand
grid-gear-ring = Ring
grid-gear-trinket = Talisman
grid-gear-feet = Füße
grid-gear-empty = — leer —
grid-item-helm = Helm
grid-item-amulet = Amulett
grid-item-signet = Siegelring
grid-item-boots = Stiefel
grid-quests-active = Aktiv
grid-quests-done = Erledigt
grid-quests-failed = Gescheitert
grid-quest-descend = Das tiefste Gewölbe erreichen
grid-quest-torches = Die Fackeln einteilen
grid-quest-map = Die Karte berichtigen
grid-quest-key = Den verlorenen Schlüssel finden
grid-quest-rope = Das Seil bergen
grid-quest-rats = Die Zisterne säubern
grid-quest-bridge = Die östliche Schlucht überqueren
grid-quest-reward = { $gold } Gold
grid-quest-progress =
    { $status ->
        [done] Abgeschlossen
        [failed] Gescheitert bei { $progress } %
       *[other] { $progress } % erledigt
    }
