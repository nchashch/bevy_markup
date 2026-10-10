{# A Markdown template (ADR 0014): Tera fills the context, then the page
   converts to the HTML the pipeline parses. The prose comes from Fluent
   (`notes-body` is a Markdown value of the `markdown: true` bundle — the
   list and emphasis are Markdown); the seal is an `<img>` flowing inline;
   raw HTML (the h1, the article, the button) passes through unchanged. #}
<div class="notes">
  <h1 data-l10n-id="notes-title">Notes</h1>

  <article data-l10n-id="notes-body">fallback</article>

  ![seal](../ui/frame.png)

  <div class="action" data-on-click="switch-language" data-l10n-id="journal-language">Language</div>
</div>
