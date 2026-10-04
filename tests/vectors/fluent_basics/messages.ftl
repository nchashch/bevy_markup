-brand = Pocket Quest
title = Inventory
greeting = Welcome, <b>{ $name }</b>!
coins =
    { $n ->
        [one] <em>One</em> coin
       *[other] { $n } coins
    }
pet =
    { $kind ->
        [cat] A cat sleeps.
       *[other] Some { $kind } waits.
    }
brand-line = Welcome to { -brand }.
keys = Press <kbd>Ctrl</kbd>+<kbd>S</kbd> to save.
entities = Fish &amp; chips &lt;3
item = A { $what }
code =
    line one
      indented two
