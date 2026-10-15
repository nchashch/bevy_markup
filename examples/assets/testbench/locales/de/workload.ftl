# Nachrichten der erzeugten Arbeitslasten (lokalisierte Zeilen, feindliche
# Argumente, Argument-Injektion). Selektoren machen eine Nachricht
# unzwischenspeicherbar: der Pro-Wert-Markdown-Pfad.
tb-w-line = Zeile **{ $n }** für { $who }
tb-w-count =
    { $n ->
        [one] ein Gegenstand in Fach { $n }
       *[other] { $n } Gegenstände in Fach { $n }
    }
tb-w-greet = Hallo { $name }!
tb-w-hostile =
    { $n ->
        [one] { $who } fand einen
       *[other] { $who } fand { $n }
    }
tb-w-hostile-cached = Hallo { $who }!
