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
demo-title = Демонстрация bevy_markup
demo-about =
    Всё на экране — HTML и CSS: панели, кнопки и их 9-срезные рамки. Ниже можно
    переключить тему и язык; обрамлённые панели прокручиваются колёсиком мыши.
    Клавиатура и геймпад тоже работают: ← → / крестовина / левый стик —
    переход между панелями и кнопками, ↑ ↓ прокручивают панель в фокусе
    (а также правый стик, PageUp / PageDown), Enter / A — нажатие, L / Y и
    T / X переключают язык и тему.
demo-language = Язык
demo-theme = Тема
demo-scrollbars = Полосы прокрутки
demo-scrollbars-on = Вкл.
demo-scrollbars-off = Выкл.
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
log-title = Журнал экспедиции
log-entry =
    <b>День { $day }</b> · { $place ->
        [cistern] Цистерна
        [chasm] Восточная пропасть
        [stair] Обрушенная лестница
       *[galleries] Нижние галереи
    } — { $event ->
        [rest] Отдохнули у сухой стены и снова пересчитали факелы.
        [found] Нашли в иле { $count ->
            [one] <em>{ $count }</em> монету
            [few] <em>{ $count }</em> монеты
           *[many] <em>{ $count }</em> монет
        } — и больше ничего, что стоило бы нести.
        [storm] Вода поднялась через решётки в полу; мы переждали на лестнице.
        [map] Исправили карту: проход, отмеченный открытым, завален.
        [camp] Разбили лагерь. Верёвку починили, пока остальные спали.
       *[march] Четыре часа шли по стоячей воде; фонарь выдержал.
    }
