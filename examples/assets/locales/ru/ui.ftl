# Values are markup (see en-US/ui.ftl for the rules).

# Item names arrive as English data values (`$item`, from the Tera context).
# Referenced from item-count: message references share the caller's
# variables (term arguments only accept literals, so a term can't do this).
item-name =
    { $item ->
        [torch] Факел
        [rope] Верёвка
        [key] Ключ
       *[other] { $item }
    }

inventory-title = Инвентарь
greeting = С возвращением, { $name }!
hp-status = Здоровье: { $hp } из { $max }
styled-heading = <i>Стилизованный</i> <b>заголовок</b> <em><strong>здесь</strong></em>
styles-demo = Обычный, <b>жирный</b>, <strong>сильный</strong>, <i>курсив</i>, <em>акцент</em> и <b><i>всё сразу</i></b>.
shortcut-hint = Нажмите <kbd>Ctrl</kbd>+<kbd>S</kbd>, чтобы вызвать <code>save_game()</code>, или <b><code>жирный код</code></b>.
code-sample =
    fn main() {"{"}
        // пробелы    сохраняются
        println!("&lt;привет&gt; {"{"}{"}"}", <b>42</b>);
    {"}"}
item-count =
    { $count ->
        [0] { item-name }: больше нет
       *[other] { item-name } × { $count }
    }
field-notes-title = Полевые заметки
field-notes-torches =
    Факелы шипят в сыром воздухе нижних галерей. { $name } пересчитывает остатки и
    решает, что для спуска хватит и <i>трёх</i>, хотя карта говорит об обратном.
field-notes-rope =
    Моток верёвки, растрёпанный с одного конца, — всё, что осталось от прошлой
    экспедиции. Вес он ещё держит, чего не скажешь о <b>мосте</b> через восточную пропасть.
field-notes-key =
    Ключ потерян. Где-то между цистерной и обрушенной лестницей он выскользнул, а
    вместе с ним и надежда открыть хранилище <em>лёгким</em> путём. Всегда есть трудный путь.
field-notes-health =
    Здоровье держится на { $hp } из { $max }. Не отлично, но и не ужасно. Следующий
    привал отмечен на карте маленькой <code>★</code> — двумя уровнями ниже и неизвестно
    насколько восточнее.
