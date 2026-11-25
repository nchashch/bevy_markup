svg-title = SVG in bevy_markup
svg-theme = Farbe: { $theme ->
    [0] Gold
    [1] Minze
   *[other] Rosé
}
svg-scale = Maßstab: { $scale }×
svg-language = Deutsch
svg-inline-title = Eingebettete Symbole
svg-inline-body = Eingebettetes &lt;svg&gt; in der Vorlage. Sein <b>currentColor</b> ist die CSS-Farbe: Farbschema und Hover färben es um.
svg-sizes-title = Eine Datei, jede Größe
svg-sizes-body = Dieselbe <b>star.svg</b> als &lt;img&gt; in vier CSS-Größen, jede in ihren angezeigten Pixeln neu gezeichnet.
svg-css-title = CSS-Bilder
svg-css-body = Der Hintergrund dieser Karte ist ein gekacheltes SVG, die anderen Karten rahmt ein SVG-border-image. Beide bleiben in jedem Maßstab scharf.
svg-badge-title = Verläufe und Beschneidung
svg-badge-body = SVGs zeichnen die Icon-Teilmenge: Formen, Pfade, Gruppen, use, Verläufe und Beschneidungspfade. Filter, Masken, Muster, Marker und &lt;text&gt; werden abgelehnt.
svg-refused-title = Abgelehnt
svg-refused-body = <b>refused.svg</b> ist ein &lt;use&gt;-Zyklus, der den SVG-Parser abstürzen ließe: Er wird vor dem Parsen abgelehnt, hier erscheint nichts, und die Seite bleibt heil.
svg-status = UiScale { $scale } (T / X Farbe, S / B Maßstab, L / Y Sprache)
