# Values are markup (see en-US/ui.ftl for the rules).

# Item names arrive as English data values (`$item`, from the Tera context).
# Referenced from item-count: message references share the caller's
# variables (term arguments only accept literals, so a term can't do this).
item-name =
    { $item ->
        [torch] Fackel
        [rope] Seil
        [key] Schlüssel
       *[other] { $item }
    }

inventory-title = Inventar
greeting = Willkommen zurück, { $name }!
hp-status = Gesundheit: { $hp } von { $max }
styled-heading = Eine <i>gestaltete</i> <b>Überschrift</b> <em><strong>hier</strong></em>
styles-demo = Normal, <b>fett</b>, <strong>stark</strong>, <i>kursiv</i>, <em>betont</em> und <b><i>beides</i></b>.
shortcut-hint = Drücke <kbd>Strg</kbd>+<kbd>S</kbd>, um <code>save_game()</code> aufzurufen, oder <b><code>fetter Code</code></b>.
code-sample =
    fn main() {"{"}
        // Leerzeichen    bleiben
        println!("&lt;hallo&gt; {"{"}{"}"}", <b>42</b>);
    {"}"}
item-count =
    { $count ->
        [0] { item-name }: keine mehr
       *[other] { item-name } × { $count }
    }
field-notes-title = Feldnotizen
demo-title = bevy_markup-Demo
demo-about =
    Alles auf dem Bildschirm ist HTML und CSS: die Paneele, die Schaltflächen und
    ihre 9-Slice-Rahmen. Unten lassen sich Thema und Sprache wechseln; die
    gerahmten Paneele scrollen mit dem Mausrad.
demo-language = Sprache
demo-theme = Thema
field-notes-torches =
    Die Fackeln knistern in der feuchten Luft der unteren Galerien. { $name } zählt,
    was übrig ist, und beschließt, dass <i>drei</i> für den Abstieg reichen müssen,
    auch wenn die Karte etwas anderes vermuten lässt.
field-notes-rope =
    Ein Seil, an einem Ende ausgefranst, ist alles, was von der letzten Expedition
    übrig ist. Es hält noch Gewicht – mehr, als man von der <b>Brücke</b> über die
    östliche Schlucht sagen kann.
field-notes-key =
    Der Schlüssel ist weg. Irgendwo zwischen Zisterne und eingestürzter Treppe ist er
    herausgerutscht und mit ihm jede Hoffnung, das Gewölbe auf die <em>einfache</em>
    Art zu öffnen. Es gibt immer den schweren Weg.
field-notes-health =
    Die Gesundheit hält bei { $hp } von { $max }. Nicht gut, nicht schlecht. Der
    nächste Rastplatz ist auf der Karte mit einem kleinen <code>★</code> markiert,
    zwei Ebenen tiefer und unbekannt weit östlich.
