world-on-screen = Plates on screen: { $count } of { $total }
world-hint = ← → / D-pad / left stick: choose · Enter / A: press · K / B: hit · R / X: everyone back · Space / Select: plates · L / Y: language
world-hit = Hit nearest
world-respawn = Everyone back
world-plates = Plates on/off
world-language = Language: English

# Nameplates. Translations are markup: <b>, <i> and <span class> are styled
# by world/style.css.
world-level = Lv { $level }
world-title = { $epithet ->
    [brave] the Brave
    [swift] the Swift
    [wise] the Wise
    [grim] the Grim
    [quiet] the Quiet
   *[lost] the Lost
} · <span class="{ $faction }">{ $faction ->
    [ally] ally
    [hostile] hostile
   *[neutral] neutral
}</span>
world-status = <b>{ $hp }</b> HP · { $hits ->
    [0] <i>unhurt</i>
    [one] hit <b>once</b>
   *[other] hit <b>{ $hits }</b> times
}
