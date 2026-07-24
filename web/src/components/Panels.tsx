// Legend, inspector (entity + relation), status panels, and blocking/empty states.
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { useApp, useUi, type Projection } from "../app/AppContext.tsx";
import { DiagnosticsExplorer } from "./DiagnosticsExplorer.tsx";
import { SafeMarkdown } from "../markdown/SafeMarkdown.tsx";
import { localized } from "../types/index.ts";
import type { LegendEntry, RelationLegendEntry } from "../state/selectors.ts";
import { dashArrayFor, flowDash, type RelationStyle } from "../adapter/relationStyle.ts";
import type { Entity, Relation, DocumentDiagnostic } from "../types/index.ts";
import type { DocumentModel } from "../model/model.ts";
import { entityInspection, relationInspection } from "../model/inspectorProjection.ts";
import {
  Chip,
  IdentityRow,
  InspectorSection,
  RelationGroups as RelationGroupsView,
} from "./InspectorSections.tsx";
import { pushEscape } from "../app/escapeStack.ts";
import {
  repositoryLinks,
  type RepositoryLinks as RepositoryLinkSet,
} from "../api/repositoryLinks.ts";
import type { SourceClient } from "../api/source.ts";

/** View-wide active-flow legend state, mirroring the graph's flow policy. */
export interface LegendFlow {
  /** Eligible active-flow relations exist in the view. */
  present: boolean;
  /** Continuous motion is currently running (policy allows it and reduced-motion
   *  is off). */
  motionEnabled: boolean;
  /** Eligible relations exceed the view threshold, so motion is suppressed. */
  suppressedByCount: boolean;
}

export function Legend({
  entries,
  relations = [],
  flow,
}: {
  entries: LegendEntry[];
  relations?: RelationLegendEntry[];
  flow?: LegendFlow;
}) {
  const showFlow = !!flow?.present;
  if (entries.length === 0 && relations.length === 0 && !showFlow) return null;
  return (
    <div className="cv-legend" aria-label="Legend">
      <h2 className="cv-panel-title">Legend</h2>
      {entries.length > 0 && (
        <ul className="cv-legend-kinds">
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
      )}
      {relations.length > 0 && <RelationLegend relations={relations} />}
      {showFlow && <ActiveFlowLegend flow={flow} />}
    </div>
  );
}

/**
 * The single active-flow legend sample, shown once when the view contains eligible
 * flow relations (never one row per edge). It reuses the same `cv-edge-flow`
 * classes and motion tokens as graph edges, so it animates only when motion is
 * actually running and renders statically under reduced motion or view-wide
 * suppression — where it adds a textual note that motion is off. Its accessible
 * name names the treatment and its source→target direction.
 */
function ActiveFlowLegend({ flow }: { flow: LegendFlow }) {
  const motionNote = flow.suppressedByCount
    ? " (motion off: many flows)"
    : !flow.motionEnabled
      ? " (motion off)"
      : "";
  const label = `Active flow: manual flow relation, ${
    flow.motionEnabled ? "animated dashes travel" : "arrow shows direction"
  } from source to target`;
  return (
    <div className="cv-flow-legend" role="group" aria-label="Active flow">
      <ul className="cv-rel-legend-list">
        <li className="cv-rel-legend-item">
          <span
            className="cv-rel-sample"
            role="img"
            tabIndex={0}
            aria-label={label}
            data-flow="active"
            data-motion={flow.motionEnabled ? "on" : "off"}
          >
            <ActiveFlowSample motion={flow.motionEnabled} />
          </span>
          <span className="cv-rel-legend-label">
            Active flow
            {motionNote && <span className="cv-muted">{motionNote}</span>}
          </span>
        </li>
      </ul>
    </div>
  );
}

/** Sample line width (px). The flow dash geometry scales from this via the same
 *  `flowDash` helper the graph edges use, so legend and graph never diverge. */
const FLOW_SAMPLE_WIDTH = 2;

/** Miniature active-flow sample: a manual-toned line carrying the shared,
 *  width-scaled flow dash (and, when enabled, the shared dash animation) plus a
 *  directional arrowhead. */
function ActiveFlowSample({ motion }: { motion: boolean }) {
  const stroke = "var(--rel-manual)";
  const fd = flowDash(FLOW_SAMPLE_WIDTH);
  return (
    <svg className="cv-rel-sample-svg" width="36" height="12" viewBox="0 0 36 12" aria-hidden="true">
      <line
        className={`cv-edge-flow${motion ? " cv-edge-flow--motion" : ""}`}
        x1="1"
        y1="6"
        x2="27"
        y2="6"
        style={
          {
            stroke,
            "--edge-flow-dash": fd.dashArray,
            "--edge-flow-dash-cycle": String(fd.cycle),
          } as CSSProperties
        }
        strokeWidth={FLOW_SAMPLE_WIDTH}
        strokeLinecap="round"
      />
      <polygon points="27,2 35,6 27,10" style={{ fill: stroke }} />
    </svg>
  );
}

/** The relation legend: one keyboard-focusable, screen-reader-labelled sample
 *  per relation kind present in the active view. Every sample is drawn from the
 *  central relation-style registry (`entry.style`); nothing is redefined here. */
function RelationLegend({ relations }: { relations: RelationLegendEntry[] }) {
  return (
    <div className="cv-rel-legend" role="group" aria-label="Relation types">
      <h3 className="cv-legend-subtitle">Relations</h3>
      <ul className="cv-rel-legend-list">
        {relations.map((entry) => (
          <li key={entry.kind} className="cv-rel-legend-item">
            <span
              className="cv-rel-sample"
              role="img"
              tabIndex={0}
              aria-label={describeRelation(entry)}
              data-kind={entry.kind}
              data-pattern={entry.style.pattern}
              data-width={entry.style.states.normal.width}
              data-marker={entry.style.marker}
              data-stroke-token={entry.style.strokeToken}
            >
              <RelationSample style={entry.style} />
            </span>
            <span className={`cv-rel-legend-label${entry.known ? "" : " cv-generic"}`}>
              {entry.label}
              {!entry.known && " (unknown)"}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** Miniature edge sample: line (stroke token, width, dash pattern) + arrowhead. */
function RelationSample({ style }: { style: RelationStyle }) {
  const width = style.states.normal.width;
  const hasMarker = style.marker !== "none";
  const lineEnd = hasMarker ? 27 : 35;
  const stroke = `var(${style.strokeToken})`;
  return (
    <svg className="cv-rel-sample-svg" width="36" height="12" viewBox="0 0 36 12" aria-hidden="true">
      <line
        x1="1"
        y1="6"
        x2={lineEnd}
        y2="6"
        style={{ stroke }}
        strokeWidth={width}
        strokeDasharray={dashArrayFor(style.pattern, width)}
        strokeLinecap="round"
      />
      {hasMarker && (
        <polygon
          points={
            style.marker === "arrow-closed"
              ? "27,2 35,6 27,10"
              : "27,2 35,6 27,10 29.5,6"
          }
          style={{ fill: stroke }}
        />
      )}
    </svg>
  );
}

/** Accessible description of a relation legend row, naming the relation and its
 *  direction so severity/direction is never conveyed by the sample alone. */
function describeRelation(entry: RelationLegendEntry): string {
  const direction =
    entry.style.marker === "none"
      ? "no direction marker"
      : "arrow shows direction from source to target";
  const known = entry.known ? "" : " (unknown relation)";
  return `${entry.label}${known}: ${entry.style.pattern} line, ${direction}`;
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
  return <DiagnosticsExplorer diagnostics={list} />;
}

/**
 * The details inspector. `modal` is true when it is inside the drawer/full-screen
 * dialog (medium/narrow): there, Escape is owned by the dialog (via the escape
 * stack) and closing keeps the selection, so the inspector does NOT register a
 * selection-clearing Escape. On wide it registers one — yielding to any inner
 * dismissable (e.g. an open source viewer) because it sits BENEATH it on the stack.
 */
export function Inspector({ modal = false }: { modal?: boolean }) {
  const { store, model } = useApp();
  const selection = useUi((s) => s.selection);

  useEffect(() => {
    if (modal || selection.kind === "none") return;
    return pushEscape(() => store.getState().clearSelection());
  }, [modal, selection.kind, store]);

  const selectEntity = useCallback((id: string) => store.getState().selectEntity(id), [store]);
  const selectRelation = useCallback((id: string) => store.getState().selectRelation(id), [store]);

  if (selection.kind === "none") {
    return (
      <div className="cv-inspector-empty cv-muted" role="note">
        Select a node or edge to inspect.
      </div>
    );
  }
  if (selection.kind === "entity") {
    const entity = model.entityById.get(selection.id);
    if (!entity) return <div className="cv-inspector-empty cv-muted">Entity not found.</div>;
    return (
      <EntityInspector
        entity={entity}
        model={model}
        onSelectEntity={selectEntity}
        onSelectRelation={selectRelation}
      />
    );
  }
  const relation = model.relationById.get(selection.id);
  if (!relation) return <div className="cv-inspector-empty cv-muted">Relation not found.</div>;
  return (
    <RelationInspector relation={relation} model={model} onSelectEntity={selectEntity} />
  );
}

/** The header shared by both inspector projections: title + kind/role/provenance
 *  chips. Role and kind stay separate concepts. */
function InspectorHeader({
  title,
  kindLabel,
  roleLabel,
  roleKnown,
  provenance,
}: {
  title: string;
  kindLabel: string;
  roleLabel?: string;
  roleKnown?: boolean;
  provenance: string;
}) {
  return (
    <header className="cv-insp-header">
      <h2 tabIndex={-1} className="cv-inspector-title" title={title}>
        {title}
      </h2>
      <div className="cv-insp-chips">
        <Chip variant="kind">{kindLabel}</Chip>
        {roleLabel !== undefined && (
          <Chip variant={roleKnown ? "role" : "role-unknown"} title={roleLabel}>
            {roleLabel}
          </Chip>
        )}
        <Chip variant="provenance">{provenance}</Chip>
      </div>
    </header>
  );
}

function EntityInspector({
  entity,
  model,
  onSelectEntity,
  onSelectRelation,
}: {
  entity: Entity;
  model: DocumentModel;
  onSelectEntity: (id: string) => void;
  onSelectRelation: (id: string) => void;
}) {
  const lang = useUi((s) => s.language);
  const insp = useMemo(() => entityInspection(model, entity, lang), [model, entity, lang]);
  const docs = entity.docs;
  const parent = typeof entity.parent === "string" ? entity.parent : undefined;
  const parentEntity = parent ? model.entityById.get(parent) : undefined;

  return (
    <section className="cv-inspector cv-insp" aria-label="Entity inspector">
      <InspectorHeader
        title={localized(entity.label, lang)}
        kindLabel={entity.kind}
        roleLabel={insp.roleLabel}
        roleKnown={insp.roleKnown}
        provenance={entity.provenance}
      />

      <InspectorSection title="Identity">
        <div className="cv-insp-rows">
          <IdentityRow label="Qualified name">
            <code className="cv-insp-code">{entity.qualified_name}</code>
          </IdentityRow>
          <IdentityRow label="Id">
            <code className="cv-insp-code">{entity.id}</code>
          </IdentityRow>
          {docs && (
            <IdentityRow label="Documentation">
              {docs.documented ? "documented" : "undocumented"}
            </IdentityRow>
          )}
          {entity.tags && entity.tags.length > 0 && (
            <IdentityRow label="Tags">{entity.tags.join(", ")}</IdentityRow>
          )}
          {entity.source && (
            <IdentityRow label="Source location">
              <code className="cv-insp-code">{entity.source.path}</code>
              {entity.source.span &&
                ` :${entity.source.span.start_line}-${entity.source.span.end_line}`}
            </IdentityRow>
          )}
        </div>
        {docs?.markdown && (
          <div className="cv-docs">
            <SafeMarkdown>{docs.markdown}</SafeMarkdown>
          </div>
        )}
      </InspectorSection>

      <ActionsSection entity={entity} model={model} />

      <InspectorSection title="Hierarchy">
        <div className="cv-insp-rows">
          <IdentityRow label="Parent">
            {parent ? (
              <button type="button" className="cv-insp-link" onClick={() => onSelectEntity(parent)}>
                {parentEntity ? localized(parentEntity.label, lang) : parent}
              </button>
            ) : (
              <span className="cv-muted">none</span>
            )}
          </IdentityRow>
        </div>
        <BoundedEntityList title="Children" items={insp.children} onSelect={onSelectEntity} />
      </InspectorSection>

      <RelationGroupsView
        groups={insp.outgoing}
        total={insp.outgoingTotal}
        direction="outgoing"
        onSelectEntity={onSelectEntity}
        onSelectRelation={onSelectRelation}
      />
      <RelationGroupsView
        groups={insp.incoming}
        total={insp.incomingTotal}
        direction="incoming"
        onSelectEntity={onSelectEntity}
        onSelectRelation={onSelectRelation}
      />

      {insp.diagnostics.length > 0 && (
        <InspectorSection title="Diagnostics">
          <DiagnosticsList diags={insp.diagnostics} />
        </InspectorSection>
      )}
    </section>
  );
}

/** A bounded, selectable entity list (children / endpoints) with a Show more. */
function BoundedEntityList({
  title,
  items,
  onSelect,
  limit = 6,
}: {
  title: string;
  items: { id: string; label: string }[];
  onSelect: (id: string) => void;
  limit?: number;
}) {
  const [open, setOpen] = useState(false);
  if (items.length === 0) {
    return (
      <div className="cv-insp-rows">
        <IdentityRow label={title}>
          <span className="cv-muted">none</span>
        </IdentityRow>
      </div>
    );
  }
  const shown = open ? items : items.slice(0, limit);
  const hidden = items.length - shown.length;
  return (
    <div className="cv-insp-sublist">
      <span className="cv-insp-row-key">
        {title} · {items.length}
      </span>
      <ul className="cv-insp-list">
        {shown.map((c) => (
          <li key={c.id}>
            <button type="button" className="cv-insp-link" onClick={() => onSelect(c.id)} title={c.label}>
              {c.label}
            </button>
          </li>
        ))}
      </ul>
      {items.length > limit && (
        <button type="button" className="cv-insp-showmore" aria-expanded={open} onClick={() => setOpen((v) => !v)}>
          {open ? "Show less" : `Show ${hidden} more`}
        </button>
      )}
    </div>
  );
}

/** Repository + local-source actions (both already privacy-safe). */
function ActionsSection({ entity, model }: { entity: Entity; model: DocumentModel }) {
  return (
    <InspectorSection title="Source & repository">
      <RepositoryLinksSection entity={entity} model={model} />
      <EntitySource entity={entity} />
    </InspectorSection>
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
      <h4 className="cv-insp-subheading">Repository</h4>
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
  const showBtnRef = useRef<HTMLButtonElement | null>(null);
  const preRef = useRef<HTMLPreElement | null>(null);
  const isOpen = state.k === "ok";
  const close = useCallback(() => setState({ k: "idle" }), []);

  useEffect(() => {
    const current = controller;
    return () => current.current?.abort();
  }, []);

  // When the source viewer is showing it is the TOPMOST dismissable: it takes
  // Escape first (via the shared stack, above any inspector drawer), moves focus
  // into itself, and on close returns focus to the "Show source" button. The
  // inspector drawer's own Escape only fires once this has popped.
  useEffect(() => {
    if (!isOpen) return;
    preRef.current?.focus();
    const dispose = pushEscape(close);
    return () => {
      dispose();
      showBtnRef.current?.focus();
    };
  }, [isOpen, close]);

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
    <div className="cv-source" aria-label="Source contents" data-source-open={isOpen ? "true" : undefined}>
      <h4 className="cv-insp-subheading">Source</h4>
      <p className="cv-muted">
        <code className="cv-insp-code">{path}</code>
      </p>
      {state.k === "idle" && (
        <button ref={showBtnRef} type="button" className="cv-control" onClick={load}>
          Show source
        </button>
      )}
      {state.k === "loading" && (
        <p role="status" className="cv-muted">
          Loading source…
        </p>
      )}
      {state.k === "ok" && (
        <div className="cv-source-view" role="group" aria-label="Source contents viewer">
          <button type="button" className="cv-control cv-source-close" aria-label="Close source" onClick={close}>
            Close source
          </button>
          <pre ref={preRef} tabIndex={-1} className="cv-code">
            <code>{state.text}</code>
          </pre>
        </div>
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

function RelationInspector({
  relation,
  model,
  onSelectEntity,
}: {
  relation: Relation;
  model: DocumentModel;
  onSelectEntity: (id: string) => void;
}) {
  const lang = useUi((s) => s.language);
  const insp = useMemo(() => relationInspection(model, relation, lang), [model, relation, lang]);

  return (
    <section className="cv-inspector cv-insp" aria-label="Relation inspector">
      <InspectorHeader
        title={insp.relLabel}
        kindLabel={relation.kind}
        provenance={relation.provenance}
      />

      <InspectorSection title="Direction">
        <div className="cv-insp-rows">
          <IdentityRow label="From">
            <button type="button" className="cv-insp-link" onClick={() => onSelectEntity(insp.fromId)} title={insp.fromLabel}>
              {insp.fromLabel}
            </button>
          </IdentityRow>
          <IdentityRow label="To">
            <button type="button" className="cv-insp-link" onClick={() => onSelectEntity(insp.toId)} title={insp.toLabel}>
              {insp.toLabel}
            </button>
          </IdentityRow>
          <IdentityRow label="Direction">
            <span className="cv-muted">from → to</span>
          </IdentityRow>
        </div>
      </InspectorSection>

      <InspectorSection title="Identity">
        <div className="cv-insp-rows">
          <IdentityRow label="Kind">
            {insp.known ? insp.style.label : `${relation.kind} (unknown)`}
          </IdentityRow>
          <IdentityRow label="Id">
            <code className="cv-insp-code">{relation.id}</code>
          </IdentityRow>
          {relation.role && <IdentityRow label="Role">{relation.role}</IdentityRow>}
          {relation.label && (
            <IdentityRow label="Label">{localized(relation.label, lang)}</IdentityRow>
          )}
        </div>
      </InspectorSection>

      {insp.diagnostics.length > 0 && (
        <InspectorSection title="Diagnostics">
          <DiagnosticsList diags={insp.diagnostics} />
        </InspectorSection>
      )}
    </section>
  );
}

/** Represented occurrences for a diagnostic (missing/invalid → 1), never lost. */
function occurrencesOf(d: DocumentDiagnostic): number {
  const n = (d as { occurrence_count?: unknown }).occurrence_count;
  return typeof n === "number" && Number.isInteger(n) && n >= 1 ? n : 1;
}

/** The inspector diagnostics list. Severity is text (not colour-only), and the
 *  represented occurrence count is always shown so no diagnostic count is lost.
 *  Rendered inside an `InspectorSection` "Diagnostics", so it carries no heading. */
function DiagnosticsList({ diags }: { diags: DocumentDiagnostic[] }) {
  return (
    <ul className="cv-insp-diags">
      {diags.map((d, i) => {
        const n = occurrencesOf(d);
        return (
          <li key={i} className={`cv-diag cv-diag-${d.severity}`}>
            <strong className="cv-diag-sevtag">{d.severity}</strong> <code>{d.code}</code>{" "}
            <span className="cv-diag-occ cv-muted">
              {n} occurrence{n === 1 ? "" : "s"}
            </span>{" "}
            {d.message}
          </li>
        );
      })}
    </ul>
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
