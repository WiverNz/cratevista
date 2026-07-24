// Presentational node card with progressive disclosure.
//
// All data is precomputed in `card` (see model/nodeCards.ts). This component
// only picks a density level (compact / normal / detailed) from zoom + selection
// and a single dominant visual state, then renders the already-computed content.
// It performs no aggregation.

import { type CSSProperties } from "react";

import {
  cardLevel,
  nodeVisualState,
  type CardLevel,
  type CardMetric,
  type NodeCard,
} from "../model/nodeCards.ts";

const SEVERITY_SYMBOL = { error: "✕", warning: "⚠", info: "ℹ" } as const;

/** Whether a metric is shown at the given level. */
function metricVisible(metric: CardMetric, level: CardLevel): boolean {
  if (level === "compact") return false;
  if (level === "normal") return metric.minLevel === "normal";
  return true; // detailed
}

/** Accessible, non-color, single-string description of the whole card. */
function accessibleLabel(card: NodeCard): string {
  const parts: string[] = [`${card.kindLabel}: ${card.fullTitle}`];
  if (!card.known) parts.push("unknown kind");
  // Authored architectural role, when present. For an unknown role the label IS the
  // complete authored value, so the full value is in the accessible name.
  if (card.role) parts.push(`architectural role: ${card.role.label}`);
  if (card.context) parts.push(`in ${card.context}`);
  if (card.visibility) parts.push(card.visibility);
  if (card.documented !== undefined) parts.push(card.documented ? "documented" : "undocumented");
  if (card.diagnostic) parts.push(card.diagnostic.label);
  return parts.join(", ");
}

export function NodeCardView({
  card,
  zoom,
  selected,
  related,
  searchMatch,
  dimmed = false,
}: {
  card: NodeCard;
  zoom: number;
  selected: boolean;
  related: boolean;
  searchMatch: boolean;
  /** Dim-focus de-emphasis. Orthogonal to `state`: it never suppresses the
   *  selected/search/diagnostic emphasis (those still win) — it only quiets an
   *  ordinary unrelated card. Purely presentational: no dimension/content change,
   *  never `aria-hidden`, always keyboard-reachable and clickable. */
  dimmed?: boolean;
}) {
  const level = cardLevel({ zoom, selected });
  const state = nodeVisualState({
    selected,
    searchMatch,
    related,
    diagnosticSeverity: card.diagnostic?.severity,
  });
  // Dim is applied only to a card the dominant state does not already spotlight,
  // so selection/search/diagnostic emphasis is never dimmed away.
  const showDim = dimmed && state !== "selected" && state !== "search" && state !== "diagnostic-error" && state !== "diagnostic-warning";

  const showBody = level !== "compact";
  const metrics = card.metrics.filter((m) => metricVisible(m, level));
  const showContext = showBody && !!card.context;
  // The one supporting description line: shown at normal+ when the entity genuinely
  // has a description. Absent → nothing renders (no empty block).
  const showDescription = showBody && !!card.description;
  const showIndicators =
    level === "detailed" &&
    (card.visibility !== undefined || card.documented !== undefined || card.hasSource);

  return (
    <div
      className={`cv-node cv-node--${card.category} cv-node--state-${state} cv-node--${level}${showDim ? " cv-node--dimmed" : ""}`}
      // A FIXED box — exactly the deterministic `cardSize()` ELK received — so
      // density/zoom/state never change dimensions; content flows within it. Role
      // is an additive layer: it sets a colour token the badge/cue read, but never
      // touches width/height/padding/handles.
      style={
        card.role
          ? ({ width: card.width, height: card.height, "--role-color": `var(${card.role.token})` } as CSSProperties)
          : { width: card.width, height: card.height }
      }
      role="group"
      aria-label={accessibleLabel(card)}
      data-kind={card.kind}
      data-role={card.role ? card.role.known ? card.role.authoredValue : "unknown" : undefined}
      data-level={level}
      data-state={state}
      data-dimmed={showDim ? "true" : undefined}
    >
      {/* Decorative, non-colour role cue: an absolutely-positioned child that never
          intercepts pointer events, never changes the box, and is one redundant
          channel alongside the role badge text and colour. */}
      {card.role && (
        <span className="cv-node-role-cue" data-cue={card.role.cue} aria-hidden="true" />
      )}
      <div className="cv-node-head">
        <span className="cv-node-badge">{card.kindLabel}</span>
        {card.role && (
          <span
            className={`cv-node-role${card.role.known ? "" : " cv-node-role--unknown"}`}
            title={card.role.authoredValue}
          >
            {card.role.label}
          </span>
        )}
        {!card.known && <span className="cv-node-unknown"> (unknown)</span>}
        {card.diagnostic && (
          <span
            className={`cv-node-diag cv-node-diag--${card.diagnostic.severity}`}
            role="img"
            aria-label={card.diagnostic.label}
            title={card.diagnostic.label}
          >
            <span aria-hidden="true">{SEVERITY_SYMBOL[card.diagnostic.severity]}</span>
            {level !== "compact" && (
              <span className="cv-node-diag-count" aria-hidden="true">
                {" "}
                {card.diagnostic.occurrences}
              </span>
            )}
          </span>
        )}
      </div>

      {/* The title truncates with CSS; the untruncated name stays available via
          `title` (tooltip) and the card's aria-label. */}
      <div className="cv-node-title" title={card.fullTitle}>
        {card.title}
      </div>

      {showContext && (
        <div className="cv-node-context" title={card.context}>
          {card.context}
        </div>
      )}

      {showDescription && (
        <div className="cv-node-desc" title={card.description}>
          {card.description}
        </div>
      )}

      {metrics.length > 0 && (
        <div className="cv-node-metrics">
          {metrics.map((m) => (
            <span key={m.key} className="cv-node-metric">
              <span className="cv-node-metric-label">{m.label}</span>
              <span className="cv-node-metric-value"> {m.value}</span>
            </span>
          ))}
        </div>
      )}

      {showIndicators && (
        <div className="cv-node-indicators">
          {card.visibility && <span className="cv-node-ind">{card.visibility}</span>}
          {card.documented !== undefined && (
            <span className="cv-node-ind">{card.documented ? "documented" : "undocumented"}</span>
          )}
          {card.hasSource && <span className="cv-node-ind">source</span>}
        </div>
      )}
    </div>
  );
}
