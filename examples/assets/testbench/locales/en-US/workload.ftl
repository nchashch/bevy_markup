# Messages the generated workloads use (localized rows, hostile args,
# args injection). Selectors make a message uncacheable: the per-value
# Markdown path.
tb-w-line = Row **{ $n }** for { $who }
tb-w-count =
    { $n ->
        [one] one item in slot { $n }
       *[other] { $n } items in slot { $n }
    }
tb-w-greet = Hi { $name }!
tb-w-hostile =
    { $n ->
        [one] { $who } found one
       *[other] { $who } found { $n }
    }
tb-w-hostile-cached = Hello { $who }!
