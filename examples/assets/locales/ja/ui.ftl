# Values are markup (see en-US/ui.ftl for the rules).

# Item names arrive as English data values (`$item`, from the Tera context).
# Referenced from item-count: message references share the caller's
# variables (term arguments only accept literals, so a term can't do this).
item-name =
    { $item ->
        [torch] 松明
        [rope] ロープ
        [key] 鍵
       *[other] { $item }
    }

inventory-title = 持ち物
greeting = おかえりなさい、{ $name }さん！
hp-status = 体力：{ $hp } / { $max }
styled-heading = <i>装飾された</i><b>見出し</b><em><strong>ここ</strong></em>
styles-demo = 標準、<b>太字</b>、<strong>強調</strong>、<i>斜体</i>、<em>強勢</em>、そして<b><i>両方</i></b>。
shortcut-hint = <kbd>Ctrl</kbd>+<kbd>S</kbd>を押すと<code>save_game()</code>が呼ばれます。<b><code>太字のコード</code></b>もあります。
code-sample =
    fn main() {"{"}
        // 空白は    保持される
        println!("&lt;こんにちは&gt; {"{"}{"}"}", <b>42</b>);
    {"}"}
item-count =
    { $count ->
        [0] { item-name }：なし
       *[other] { item-name } × { $count }
    }
field-notes-title = 探索メモ
# One line each, like the field notes: a wrapped line would collapse to a
# stray space between Japanese characters.
demo-title = bevy_markup デモ
demo-about = 画面はすべてHTMLとCSSです。パネルもボタンも9スライスの枠も。下でテーマと言語を切り替えられます。枠付きパネルはマウスホイールでスクロールします。
demo-language = 言語
demo-theme = テーマ
# One line each: a wrapped line would collapse to a stray space between
# Japanese characters.
field-notes-torches = 下層回廊の湿った空気の中で松明がはぜる。{ $name }は残りを数え、地図がそうは言っていなくても、下降には<i>三本</i>で足りるはずだと決める。
field-notes-rope = 片端がほつれたロープ一本が、前回の探検隊の残したすべてだ。まだ重さには耐えられる。東の裂け目にかかる<b>橋</b>よりはずっとましだ。
field-notes-key = 鍵がない。貯水槽と崩れた階段のあいだのどこかで抜け落ち、宝物庫を<em>楽な</em>方法で開ける望みも一緒に消えた。難しい方法ならいつでもある。
field-notes-health = 体力は{ $max }中{ $hp }。良くはないが、最悪でもない。次の休憩地点は地図に小さな<code>★</code>で記されている。二階層下、東へどれほどかは分からない。
