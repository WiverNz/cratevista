// Reusable, accessible inspector presentation primitives (Issue 15, Phase 5).
// Real buttons for actions, semantic lists, `aria-expanded` disclosures, clear
// empty states, bounded relation groups. No click-only <div>, no external asset,
// no backdrop-filter.
import { useState, type ReactNode } from "react";

import type { RelationGroup } from "../model/inspectorProjection.ts";
import { dashArrayFor } from "../adapter/relationStyle.ts";

/** A titled inspector section with a heading that forms the panel hierarchy. */
export function InspectorSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="cv-insp-section" aria-label={title}>
      <h3 className="cv-insp-heading">{title}</h3>
      {children}
    </section>
  );
}

/** A small labelled chip (kind / role / provenance …). Non-colour text always. */
export function Chip({ children, variant, title }: { children: ReactNode; variant?: string; title?: string }) {
  return (
    <span className={`cv-insp-chip${variant ? ` cv-insp-chip--${variant}` : ""}`} title={title}>
      {children}
    </span>
  );
}

/** A key → value identity row. Long values wrap/scroll inside their own cell. */
export function IdentityRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="cv-insp-row">
      <span className="cv-insp-row-key">{label}</span>
      <span className="cv-insp-row-val">{children}</span>
    </div>
  );
}

/** An explicit empty state, announced to assistive tech. */
export function EmptyState({ children }: { children: ReactNode }) {
  return (
    <p className="cv-insp-empty cv-muted" role="note">
      {children}
    </p>
  );
}

/** The default number of relation rows shown before "Show more". */
export const RELATION_PREVIEW_LIMIT = 6;

/**
 * A direction-labelled set of relation groups (outgoing OR incoming — never
 * merged). Each group shows a bounded preview and an accessible "Show more/less"
 * disclosure that expands ONLY that group. Row activation selects the other
 * endpoint (entity) or the relation itself.
 */
export function RelationGroups({
  groups,
  total,
  direction,
  onSelectEntity,
  onSelectRelation,
  limit = RELATION_PREVIEW_LIMIT,
}: {
  groups: RelationGroup[];
  total: number;
  direction: "outgoing" | "incoming";
  onSelectEntity: (id: string) => void;
  onSelectRelation: (id: string) => void;
  limit?: number;
}) {
  const title = direction === "outgoing" ? "Outgoing" : "Incoming";
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set());
  const toggle = (kind: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(kind)) next.delete(kind);
      else next.add(kind);
      return next;
    });

  return (
    <InspectorSection title={`${title} relations`}>
      {total === 0 ? (
        <EmptyState>No {direction} relations.</EmptyState>
      ) : (
        <ul className="cv-insp-rel-groups">
          {groups.map((g) => {
            const open = expanded.has(g.kind);
            const shown = open ? g.rows : g.rows.slice(0, limit);
            const hidden = g.rows.length - shown.length;
            const pattern = dashArrayFor(g.style.pattern, 2);
            return (
              <li key={g.kind} className="cv-insp-rel-group">
                <div className="cv-insp-rel-grouphead">
                  <span
                    className={`cv-insp-rel-kind${g.known ? "" : " cv-generic"}`}
                    data-pattern={g.style.pattern}
                    data-marker={g.style.marker}
                    aria-hidden="true"
                  >
                    <svg width="20" height="8" viewBox="0 0 20 8" className="cv-insp-rel-swatch">
                      <line x1="1" y1="4" x2={g.style.marker === "none" ? 19 : 14} y2="4" style={{ stroke: `var(${g.style.strokeToken})` }} strokeWidth="2" strokeDasharray={pattern} />
                      {g.style.marker !== "none" && (
                        <polygon points="14,1 20,4 14,7" style={{ fill: `var(${g.style.strokeToken})` }} />
                      )}
                    </svg>
                  </span>
                  <span className="cv-insp-rel-label">
                    {g.label}
                    {!g.known && " (unknown)"} · {g.rows.length}
                  </span>
                </div>
                <ul className="cv-insp-rel-rows">
                  {shown.map((r) => (
                    <li key={r.relationId} className="cv-insp-rel-row">
                      <button
                        type="button"
                        className="cv-insp-link"
                        onClick={() => onSelectEntity(r.otherId)}
                        title={r.otherLabel}
                      >
                        {r.otherLabel}
                      </button>
                      <button
                        type="button"
                        className="cv-insp-rel-open"
                        aria-label={`Inspect the ${g.label} relation to ${r.otherLabel}`}
                        onClick={() => onSelectRelation(r.relationId)}
                      >
                        ↔
                      </button>
                    </li>
                  ))}
                </ul>
                {(hidden > 0 || open) && g.rows.length > limit && (
                  <button
                    type="button"
                    className="cv-insp-showmore"
                    aria-expanded={open}
                    onClick={() => toggle(g.kind)}
                  >
                    {open ? "Show less" : `Show ${hidden} more`}
                  </button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </InspectorSection>
  );
}
