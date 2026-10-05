# Values are markup: inline elements (<b>, <i>, <em>, <strong>, <code>, <kbd>)
# are styled by the active CSS theme. Write a literal < as &lt; and & as &amp;.
# A literal { or } must be written as {"{"} / {"}"} (Fluent syntax).

inventory-title = Inventory
greeting = Welcome back, { $name }!
hp-status = Health: { $hp } of { $max }
styled-heading = A <i>styled</i> <b>heading</b> <em><strong>here</strong></em>
styles-demo = Plain, <b>bold</b>, <strong>strong</strong>, <i>italic</i>, <em>em</em>, and <b><i>both</i></b>.
shortcut-hint = Press <kbd>Ctrl</kbd>+<kbd>S</kbd> to call <code>save_game()</code>, or <b><code>bold code</code></b>.
# Fluent strips the common indentation, keeping the inner indent.
code-sample =
    fn main() {"{"}
        // whitespace    kept
        println!("&lt;hi&gt; {"{"}{"}"}", <b>42</b>);
    {"}"}
item-count =
    { $count ->
        [0] No { $item } left
        [one] One { $item }
       *[other] { $count } × { $item }
    }
field-notes-title = Field notes
demo-title = bevy_markup demo
demo-about =
    Everything on screen is HTML + CSS: the panels, the buttons and their 9-slice
    frames. Switch the theme and language below; scroll the framed panels with the
    mouse wheel.
demo-language = Language
demo-theme = Theme
field-notes-torches =
    The torches sputter in the damp air of the lower galleries. { $name } counts what
    is left and decides that <i>three</i> will have to be enough for the descent,
    though the map suggests otherwise.
field-notes-rope =
    A length of rope, frayed at one end, is all that remains of the last expedition.
    It still holds weight, which is more than can be said for the <b>bridge</b> over
    the eastern chasm.
field-notes-key =
    The key is gone. Somewhere between the cistern and the collapsed stair it slipped
    free, and with it any hope of opening the vault the <em>easy</em> way. There is
    always the hard way.
field-notes-health =
    Health holds at { $hp } of { $max }. Not great, not terrible. The next rest point
    is marked on the map with a small <code>★</code>, two levels down and an unknown
    distance east.
