{# A localized Markdown reader: the tabs are raw HTML (real UI inside
   Markdown), the pages are Markdown included by Tera, and their prose
   comes from Fluent — the `markdown: true` bundle turns every message
   value's Markdown into the HTML the pipeline parses. #}
<div class="reader">

  <div class="tabs">
    <p class="tab{% if active_tab == "release" %} active{% endif %}" data-on-click="tab" data-tab="release" data-l10n-id="reader-tab-release">Release notes</p>
    <p class="tab{% if active_tab == "tutorial" %} active{% endif %}" data-on-click="tab" data-tab="tutorial" data-l10n-id="reader-tab-tutorial">Tutorial</p>
  </div>

  {% if active_tab == "release" -%}
  {% include "release.md" %}
  {%- else -%}
  {% include "tutorial.md" %}
  {%- endif %}

  <div class="foot">
    <p class="action" autofocus data-on-click="switch-language" data-l10n-id="reader-language">Language</p>
    <p class="hint" data-l10n-id="reader-hint">Click, or <kbd>↑</kbd> <kbd>↓</kbd> / D-pad and <kbd>Enter</kbd> / A. <kbd>Q</kbd> <kbd>E</kbd> / bumpers switch pages, <kbd>Space</kbd> / Y switches the language.</p>
  </div>

</div>
