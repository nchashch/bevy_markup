hr-title = Hot reload
hr-palette = Palette: { $palette ->
    [0] Crimson
    [1] Ocean
    [2] Forest
   *[other] Unknown
}
hr-cycle = Cycle palette (H)
hr-add-coin = + Add a coin
hr-coins =
    { $coins ->
        [0] You have <em>no</em> coins.
        [one] You have <em>one</em> coin.
       *[other] You have <em>{ $coins }</em> coins.
    }
hr-language = Language: English
hr-hint = Press <kbd>H</kbd> (or the button) to rewrite the stylesheet — the UI restyles without a restart. Editing the files under <kbd>examples/assets/hot-reload/</kbd> works too.
