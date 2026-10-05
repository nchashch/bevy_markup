hello-title = Hello, HTML!
hello-greeting = Welcome, <b>{ $name }</b>.
hello-coins =
    { $coins ->
        [0] You have <em>no</em> coins.
        [one] You have <em>one</em> coin.
       *[other] You have <em>{ $coins }</em> coins.
    }
hello-add-coin = + Add a coin
hello-hint = Press <kbd>Space</kbd> to switch language.
