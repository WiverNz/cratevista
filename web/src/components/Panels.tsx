// Legend, inspector (entity + relation), status panels, and blocking/empty states.
import { useCallback, useEffect, useRef, useState } from "react";
import { useApp, useUi, type Projection } from "../app/AppContext.tsx";
import { SafeMarkdown } from "../markdown/SafeMarkdown.tsx";
import { localized } from "../types/index.ts";
import type { LegendEntry } from "../state/selectors.ts";
import type { Entity, Relation, DocumentDiagnostic } from "../types/index.ts";
import type { DocumentModel } from "../model/model.ts";
import {
  repositoryLinks,
  type RepositoryLinks as RepositoryLinkSet,
} from "../api/repositoryLinks.ts";
import type { SourceClient } from "../api/source.ts";

export function Legend({ entries }: { entries: LegendEntry[] }) {
  if (entries.length === 0) return null;
  return (
    <div className="cv-legend" aria-label="Legend">
      <h2 className="cv-panel-title">Legend</h2>
      <ul>
        {entries.map((e) => (
          <li key={e.category}>
            <span className="cv-swatch" style={{ background: e.color }} aria-hidden="true" />
            <span className={e.known ? undefined : "cv-generic"}>
              {e.category}
              {!e.known && " (unknown)"}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export function PartialBanner() {
  return (
    <div className="cv-banner cv-partial" role="status">
      Partial generation — some targets were skipped; the document may be incomplete.
    </div>
  );
}

/**
 * A regeneration is running.
 *
 * `role="status"` and `aria-live="polite"`: this is progress, not a problem. It
 * must never interrupt a screen reader mid-sentence or move focus — the user is
 * reading the graph that is still on screen, and a background rebuild is not a
 * reason to take their place in it.
 */
export function RegeneratingIndicator() {
  return (
    <div className="cv-banner cv-regenerating" role="status" aria-live="polite">
      Regenerating…
    </div>
  );
}

/**
 * A regeneration failed.
 *
 * The code and message come from the server, which builds both to be safe here:
 * a stable code plus prose. Cargo's own output — absolute paths, `CARGO_HOME`,
 * whole command lines — stays in the terminal, and this deliberately does not
 * offer to show more.
 *
 * `role="alert"` because it is a failure and the existing `ErrorState` announces
 * the same way; nothing is blocked and the graph stays interactive.
 */
export function GenerationFailedBanner(props: { code: string; message: string; }) {
  return (
    <div className="cv-banner cv-error-banner" role="alert">
      <strong>Regeneration failed.</strong> {props.message}{" "}
      <span className="cv-muted">({props.code})</span>{" "}
      <span className="cv-muted">The document below is the last one that built.</span>
    </div>
  );
}

/** A reload could not be applied. The document on screen is still the last good
 *  one, which is why this is a banner rather than the blocking `ErrorState`. */
export function ReloadErrorBanner(props: { message: string }) {
  return (
    <div className="cv-banner cv-error-banner" role="alert">
      <strong>Could not refresh.</strong> {props.message}{" "}
      <span className="cv-muted">Showing the last document that loaded.</span>
    </div>
  );
}

export function GenerationStatus() {
  return (
    <div className="cv-banner cv-warn" role="status">
      Generation status unavailable.
    </div>
  );
}

export function DiagnosticsPanel() {
  const { diagnostics, diagnosticsAvailable } = useApp();
  if (!diagnosticsAvailable) {
    return (
      <div className="cv-panel" aria-label="Diagnostics">
        <h2 className="cv-panel-title">Diagnostics</h2>
        <p className="cv-muted" role="status">
          Diagnostics unavailable.
        </p>
      </div>
    );
  }
  const list = diagnostics?.diagnostics ?? [];
  return (
    <div className="cv-panel" aria-label="Diagnostics">
      <h2 className="cv-panel-title">Diagnostics ({list.length})</h2>
      {list.length === 0 ? (
        <p className="cv-muted">No diagnostics.</p>
      ) : (
        <ul>
          {list.map((d, i) => (
            <li key={i} className={`cv-diag cv-diag-${d.severity}`}>
              <strong>{d.severity}</strong> <code>{d.code}</code> {d.message}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function diagnosticsFor(
  model: DocumentModel,
  kind: "entity" | "relation",
  id: string,
): DocumentDiagnostic[] {
  const map = kind === "entity" ? model.diagnosticsByEntity : model.diagnosticsByRelation;
  return [...(map.get(id) ?? [])];
}

export function Inspector() {
  const { store, model } = useApp();
  const selection = useUi((s) => s.selection);

  useEffect(() => {
    if (selection.kind === "none") return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") store.getState().clearSelection();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selection.kind, store]);

  if (selection.kind === "none") {
    return (
      <div className="cv-inspector-empty cv-muted">Select a node or edge to inspect.</div>
    );
  }
  if (selection.kind === "entity") {
    const entity = model.entityById.get(selection.id);
    if (!entity) return <div className="cv-inspector-empty cv-muted">Entity not found.</div>;
    return <EntityInspector entity={entity} model={model} />;
  }
  const relation = model.relationById.get(selection.id);
  if (!relation) return <div className="cv-inspector-empty cv-muted">Relation not found.</div>;
  return <RelationInspector relation={relation} model={model} />;
}

function EntityInspector({ entity, model }: { entity: Entity; model: DocumentModel }) {
  const outgoing = model.outgoing.get(entity.id) ?? [];
  const incoming = model.incoming.get(entity.id) ?? [];
  const children = model.childrenByParent.get(entity.id) ?? [];
  const diags = diagnosticsFor(model, "entity", entity.id);
  const docs = entity.docs;
  return (
    <section className="cv-inspector" aria-label="Entity inspector">
      <h2 tabIndex={-1} className="cv-inspector-title">
        {localized(entity.label)}
      </h2>
      <dl className="cv-fields">
        <dt>Kind</dt>
        <dd>
          <span className="cv-kind-badge">{entity.kind}</span>
        </dd>
        <dt>Qualified name</dt>
        <dd>
          <code>{entity.qualified_name}</code>
        </dd>
        <dt>Id</dt>
        <dd>
          <code>{entity.id}</code>
        </dd>
        <dt>Provenance</dt>
        <dd>{entity.provenance}</dd>
        {entity.tags && entity.tags.length > 0 && (
          <>
            <dt>Tags</dt>
            <dd>{entity.tags.join(", ")}</dd>
          </>
        )}
        {docs && (
          <>
            <dt>Documentation</dt>
            <dd>{docs.documented ? "documented" : "undocumented"}</dd>
          </>
        )}
        {entity.parent && (
          <>
            <dt>Parent</dt>
            <dd>
              <code>{entity.parent}</code>
            </dd>
          </>
        )}
        {entity.source && (
          <>
            <dt>Source</dt>
            <dd>
              <code>{entity.source.path}</code>
              {entity.source.span &&
                ` :${entity.source.span.start_line}-${entity.source.span.end_line}`}
            </dd>
          </>
        )}
      </dl>

      {docs?.markdown && (
        <div className="cv-docs">
          <SafeMarkdown>{docs.markdown}</SafeMarkdown>
        </div>
      )}

      <RepositoryLinksSection entity={entity} model={model} />

      <EntitySource entity={entity} />

      {children.length > 0 && (
        <RelatedList title="Children" ids={children} model={model} />
      )}
      <RelationGroups title="Outgoing" relations={outgoing} model={model} outgoing />
      <RelationGroups title="Incoming" relations={incoming} model={model} outgoing={false} />

      {Object.keys(entity.attributes ?? {}).length > 0 && (
        <div className="cv-attrs">
          <h3 className="cv-panel-title">Attributes</h3>
          <ul>
            {Object.entries(entity.attributes ?? {}).map(([k, v]) => (
              <li key={k}>
                <code>{k}</code>: {JSON.stringify(v)}
              </li>
            ))}
          </ul>
        </div>
      )}

      {diags.length > 0 && <DiagnosticsList diags={diags} />}
    </section>
  );
}

/** Provider-aware repository / source links for the selected entity.
 *
 *  Renders nothing when the project has no safe repository URL. Prefers the file
 *  deep link (when provider + branch + location allow) and always offers the
 *  repository root. Never emits a disabled placeholder, and never exposes a raw
 *  unsafe `repository_url` as an href — the URL is normalized and validated first.
 *  Output is identical in server and static mode (it depends only on document
 *  data, never on the runtime mode). */
function RepositoryLinksSection({ entity, model }: { entity: Entity; model: DocumentModel }) {
  const location = entity.source
    ? { path: entity.source.path, span: entity.source.span }
    : null;
  const links: RepositoryLinkSet | null = repositoryLinks(model.document.project, location);
  if (!links) return null;

  const providerLabel =
    links.provider === "github" ? "GitHub" : links.provider === "gitlab" ? "GitLab" : "repository";

  return (
    <div className="cv-repo-links" aria-label="Repository links">
      <h3 className="cv-panel-title">Repository</h3>
      <ul className="cv-repo-link-list">
        {links.source && (
          <li>
            <a
              className="cv-repo-link"
              href={links.source}
              target="_blank"
              rel="noopener noreferrer"
              aria-label={`Open this source file on ${providerLabel} (opens in a new tab)`}
            >
              View source on {providerLabel}
            </a>
          </li>
        )}
        <li>
          <a
            className="cv-repo-link"
            href={links.repository}
            target="_blank"
            rel="noopener noreferrer"
            aria-label={`Open the repository on ${providerLabel} (opens in a new tab)`}
          >
            Open repository
          </a>
        </li>
      </ul>
    </div>
  );
}

/** The opt-in source-contents section, shown only when a source-content capability
 *  exists — i.e. in server mode with a client. Static mode has no `/api/source`, so
 *  no client, so this renders nothing and issues no request on selection. */
function EntitySource({ entity }: { entity: Entity }) {
  const { sourceClient } = useApp();
  if (!entity.source || !sourceClient) return null;
  return (
    <SourceSection
      key={entity.id}
      path={entity.source.path as unknown as string}
      client={sourceClient}
    />
  );
}

type SourceState =
  | { k: "idle" }
  | { k: "loading" }
  | { k: "ok"; text: string }
  | { k: "disabled" }
  | { k: "error"; message: string }
  | { k: "failed"; message: string };

/** Opt-in source-contents section. Never fetches until the user activates it;
 *  aborts on unmount (the parent keys this by entity id, so a selection change
 *  unmounts it); ignores stale responses. Only the repo-relative path is shown —
 *  never an absolute path. */
export function SourceSection({ path, client }: { path: string; client: SourceClient }) {
  const sourceClient = client;
  const [state, setState] = useState<SourceState>({ k: "idle" });
  const controller = useRef<AbortController | null>(null);
  const token = useRef(0);

  useEffect(() => {
    const current = controller;
    return () => current.current?.abort();
  }, []);

  const load = useCallback(() => {
    controller.current?.abort();
    const next = new AbortController();
    controller.current = next;
    const mine = ++token.current;
    setState({ k: "loading" });
    sourceClient
      .fetchSource(path, next.signal)
      .then((outcome) => {
        if (mine !== token.current) return; // stale response
        if (outcome.status === "ok") setState({ k: "ok", text: outcome.text });
        else if (outcome.status === "disabled") setState({ k: "disabled" });
        else if (outcome.status === "error") setState({ k: "error", message: outcome.message });
        else setState({ k: "failed", message: outcome.message });
      })
      .catch(() => {
        /* aborted — ignore */
      });
  }, [sourceClient, path]);

  return (
    <div className="cv-source" aria-label="Source contents">
      <h3 className="cv-panel-title">Source</h3>
      <p className="cv-muted">
        <code>{path}</code>
      </p>
      {state.k === "idle" && (
        <button type="button" onClick={load}>
          Show source
        </button>
      )}
      {state.k === "loading" && (
        <p role="status" className="cv-muted">
          Loading source…
        </p>
      )}
      {state.k === "ok" && (
        <pre className="cv-code">
          <code>{state.text}</code>
        </pre>
      )}
      {state.k === "disabled" && (
        <p role="status" className="cv-muted">
          Source contents are disabled on this server; showing the location only.
        </p>
      )}
      {state.k === "error" && (
        <p role="status" className="cv-inline-error">
          {state.message}
        </p>
      )}
      {state.k === "failed" && (
        <p role="status" className="cv-inline-error">
          {state.message}{" "}
          <button type="button" onClick={load}>
            Retry
          </button>
        </p>
      )}
    </div>
  );
}

function RelationGroups({
  title,
  relations,
  model,
  outgoing,
}: {
  title: string;
  relations: readonly Relation[];
  model: DocumentModel;
  outgoing: boolean;
}) {
  if (relations.length === 0) return null;
  const byKind = new Map<string, Relation[]>();
  for (const r of relations) {
    const list = byKind.get(r.kind);
    if (list) list.push(r);
    else byKind.set(r.kind, [r]);
  }
  return (
    <div className="cv-related">
      <h3 className="cv-panel-title">{title}</h3>
      {[...byKind.entries()].sort().map(([kind, rs]) => (
        <div key={kind} className="cv-related-group">
          <h4>{kind}</h4>
          <ul>
            {rs.map((r) => {
              const otherId = outgoing ? r.to : r.from;
              const other = model.entityById.get(otherId);
              return (
                <li key={r.id}>{other ? localized(other.label) : otherId}</li>
              );
            })}
          </ul>
        </div>
      ))}
    </div>
  );
}

function RelatedList({
  title,
  ids,
  model,
}: {
  title: string;
  ids: readonly string[];
  model: DocumentModel;
}) {
  return (
    <div className="cv-related">
      <h3 className="cv-panel-title">{title}</h3>
      <ul>
        {ids.map((id) => {
          const e = model.entityById.get(id);
          return <li key={id}>{e ? localized(e.label) : id}</li>;
        })}
      </ul>
    </div>
  );
}

function RelationInspector({ relation, model }: { relation: Relation; model: DocumentModel }) {
  const from = model.entityById.get(relation.from);
  const to = model.entityById.get(relation.to);
  const diags = diagnosticsFor(model, "relation", relation.id);
  return (
    <section className="cv-inspector" aria-label="Relation inspector">
      <h2 tabIndex={-1} className="cv-inspector-title">
        <span className="cv-kind-badge">{relation.kind}</span>
      </h2>
      <dl className="cv-fields">
        <dt>Id</dt>
        <dd>
          <code>{relation.id}</code>
        </dd>
        <dt>From</dt>
        <dd>{from ? localized(from.label) : relation.from}</dd>
        <dt>To</dt>
        <dd>{to ? localized(to.label) : relation.to}</dd>
        {relation.role && (
          <>
            <dt>Role</dt>
            <dd>{relation.role}</dd>
          </>
        )}
        {relation.label && (
          <>
            <dt>Label</dt>
            <dd>{localized(relation.label)}</dd>
          </>
        )}
        <dt>Provenance</dt>
        <dd>{relation.provenance}</dd>
      </dl>
      {Object.keys(relation.attributes ?? {}).length > 0 && (
        <div className="cv-attrs">
          <h3 className="cv-panel-title">Attributes</h3>
          <ul>
            {Object.entries(relation.attributes ?? {}).map(([k, v]) => (
              <li key={k}>
                <code>{k}</code>: {JSON.stringify(v)}
              </li>
            ))}
          </ul>
        </div>
      )}
      {diags.length > 0 && <DiagnosticsList diags={diags} />}
    </section>
  );
}

function DiagnosticsList({ diags }: { diags: DocumentDiagnostic[] }) {
  return (
    <div className="cv-related">
      <h3 className="cv-panel-title">Diagnostics</h3>
      <ul>
        {diags.map((d, i) => (
          <li key={i} className={`cv-diag cv-diag-${d.severity}`}>
            <strong>{d.severity}</strong> <code>{d.code}</code> {d.message}
          </li>
        ))}
      </ul>
    </div>
  );
}

export function LoadingState() {
  return (
    <div className="cv-state" role="status">
      Loading CrateVista…
    </div>
  );
}

export function ErrorState({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <div className="cv-state cv-error" role="alert">
      <p>Could not load the explorer document.</p>
      <p className="cv-muted">{message}</p>
      <button type="button" onClick={onRetry}>
        Retry
      </button>
    </div>
  );
}

export function IncompatibilityState({
  found,
  onRetry,
}: {
  found: string;
  onRetry: () => void;
}) {
  return (
    <div className="cv-state cv-error" role="alert">
      <p>Unsupported document schema version: {found}.</p>
      <p className="cv-muted">Regenerate with a matching CrateVista version.</p>
      <button type="button" onClick={onRetry}>
        Retry
      </button>
    </div>
  );
}

export function EmptyState() {
  return (
    <div className="cv-state cv-muted" role="status">
      This view has no entities to show.
    </div>
  );
}

export function ReducedModeBanner({ projection }: { projection: Projection }) {
  const { store } = useApp();
  const renderFull = useUi((s) => s.renderFullGraph);
  const selection = useUi((s) => s.selection);
  const overBudget = projection.fullCount > projection.visibleCount || renderFull;

  if (projection.reduced) {
    return (
      <div className="cv-banner cv-warn" role="status">
        Reduced view — showing <strong>{projection.visibleCount}</strong> of{" "}
        <strong>{projection.fullCount}</strong> nodes around a focus.{" "}
        {selection.kind === "entity" && (
          <button
            type="button"
            onClick={() => store.getState().expandNeighborhood((selection as { id: string }).id)}
          >
            Expand neighborhood
          </button>
        )}{" "}
        <button type="button" onClick={() => store.getState().setRenderFull(true)}>
          Render full graph
        </button>{" "}
        <span className="cv-muted">(rendering all nodes may be slow)</span>
      </div>
    );
  }
  if (renderFull && overBudget) {
    return (
      <div className="cv-banner cv-warn" role="status">
        Full graph — <strong>{projection.fullCount}</strong> nodes (may be slow).{" "}
        <button type="button" onClick={() => store.getState().setRenderFull(false)}>
          Return to reduced view
        </button>
      </div>
    );
  }
  return null;
}

/** Complete, searchable/keyboard list of every entity (incl. hidden ones in
 *  reduced mode). Selecting one recenters the reduced neighborhood on it. */
export function GraphList({ projection }: { projection: Projection }) {
  const { store } = useApp();
  const renderFull = useUi((s) => s.renderFullGraph);
  if (!projection.reduced && !renderFull) return null;
  const visible = new Set(projection.graph.nodes.map((n) => n.id));
  return (
    <div className="cv-panel cv-graphlist" aria-label="All entities">
      <h2 className="cv-panel-title">All entities ({projection.allNodes.length})</h2>
      <ul>
        {projection.allNodes.map((n) => (
          <li key={n.id}>
            <button
              type="button"
              aria-pressed={visible.has(n.id)}
              onClick={() => {
                store.getState().selectEntity(n.id);
                store.getState().expandNeighborhood(n.id);
              }}
            >
              {n.label} <span className="cv-muted">{n.kind}</span>
              {!visible.has(n.id) && <span className="cv-muted"> · hidden</span>}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
