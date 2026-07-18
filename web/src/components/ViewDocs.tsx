// Flow-level documentation and worked examples for the active view
// (schema 1.1: `View.docs` / `View.examples`, PRD-08 Amendment A).
//
// Everything rendered here is **embedded in the document**: examples carry their
// own `content`, so this component never calls `/api/source` and works whether or
// not the server was started with `--source`.
//
// The eight generated views carry none of these fields, so this renders nothing
// at all for them — the existing appearance is unchanged.
import { useApp, useUi } from "../app/AppContext.tsx";
import { SafeMarkdown } from "../markdown/SafeMarkdown.tsx";
import { localized } from "../types/index.ts";
import type { ViewExample } from "../types/generated/explorer-document.ts";

/**
 * One example, as a native `<details>` disclosure.
 *
 * `<details>/<summary>` is keyboard-operable and announced as a disclosure with
 * no JavaScript and no ARIA of our own, so it cannot drift out of sync with its
 * open state the way a hand-rolled toggle can.
 */
function Example({ example, lang }: { example: ViewExample; lang: string }) {
  const description = example.description ? localized(example.description, lang) : null;
  return (
    <details className="cv-example">
      <summary>
        {localized(example.title, lang)}
        {example.language && (
          // The language is stated as text, not conveyed by syntax colour alone.
          <span className="cv-example-lang"> ({example.language})</span>
        )}
      </summary>
      {description && <p className="cv-muted">{description}</p>}
      {/*
        `content` is embedded document data and is rendered as a TEXT child, so
        React escapes it. It is never parsed as Markdown or HTML: an example may
        legitimately contain `<script>` or `</code></pre>` as sample payload, and
        it must show up as characters on the page, never as markup.
        `language` is a display hint only — nothing here interprets it.
      */}
      <pre className="cv-code">
        <code>{example.content}</code>
      </pre>
    </details>
  );
}

/** The active view's description, documentation and examples. */
export function ViewDocs() {
  const { model } = useApp();
  const activeViewId = useUi((s) => s.activeViewId);
  const lang = useUi((s) => s.language);

  const view = activeViewId ? model.viewById.get(activeViewId) : undefined;
  if (!view) return null;

  const description = view.description ? localized(view.description, lang) : null;
  const markdown = view.docs?.markdown?.trim() ? view.docs.markdown : null;
  const examples = view.examples ?? [];
  // Render nothing rather than an empty shell: a view with none of these must
  // look exactly as it did before schema 1.1.
  if (!description && !markdown && examples.length === 0) return null;

  return (
    <section className="cv-panel cv-viewdocs" aria-label="View documentation">
      <h2 className="cv-panel-title">{localized(view.title, lang)}</h2>
      {description && <p className="cv-viewdocs-description">{description}</p>}
      {markdown && (
        // Sanitized: react-markdown + rehype-sanitize, no rehype-raw and no
        // dangerouslySetInnerHTML — the same pipeline as entity docs.
        <div className="cv-markdown">
          <SafeMarkdown>{markdown}</SafeMarkdown>
        </div>
      )}
      {examples.length > 0 && (
        <div className="cv-examples" aria-label="Examples">
          <h3 className="cv-panel-title">Examples</h3>
          {examples.map((example) => (
            <Example key={example.id} example={example} lang={lang} />
          ))}
        </div>
      )}
    </section>
  );
}
