world-on-screen = Plates on screen: { $count } of { $total }
world-hint = K: hit the nearest unit · R: everyone back · Space: plates on/off · L: language

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
