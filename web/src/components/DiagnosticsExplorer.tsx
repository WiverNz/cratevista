// Scalable, grouped diagnostics explorer.
//
// Renders a compact summary and per-(severity, code) groups. Groups start COLLAPSED,
// so opening the panel for thousands of diagnostics renders only the summary and a
// bounded set of group headers — never thousands of rows. Expanding a group shows a
// bounded number of representative entries first; "Show all" is explicit. The panel
// never rewrites or discards diagnostics; it distinguishes emitted *records* from the
// underlying *occurrences* they represent.

import { useMemo, useState } from "react";

import type { DocumentDiagnostic } from "../types/index.ts";
import {
  type DiagnosticGroup,
  type Severity,
  explanationFor,
  filterGroups,
  summarize,
} from "../diagnostics/grouping.ts";

/** How many entries a group shows before "Show all". Keeps expansion bounded. */
const REPRESENTATIVE = 20;

const ALL_SEVERITIES: readonly Severity[] = ["error", "warning", "info"];

/** A short severity symbol + word (severity is never conveyed by colour alone). */
const SEVERITY_LABEL: Record<Severity, { symbol: string; word: string }> = {
  error: { symbol: "✕", word: "Error" },
  warning: { symbol: "⚠", word: "Warning" },
  info: { symbol: "ℹ", word: "Info" },
};

function plural(n: number, one: string): string {
  return `${n} ${one}${n === 1 ? "" : "s"}`;
}

export function DiagnosticsExplorer({
  diagnostics,
}: {
  diagnostics: readonly DocumentDiagnostic[];
}) {
  const summary = useMemo(() => summarize(diagnostics), [diagnostics]);
  const [severities, setSeverities] = useState<ReadonlySet<Severity>>(
    () => new Set(ALL_SEVERITIES),
  );
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set());
  const [showAll, setShowAll] = useState<ReadonlySet<string>>(() => new Set());

  const visibleGroups = useMemo(
    () => filterGroups(summary, severities, query),
    [summary, severities, query],
  );

  function toggleSeverity(sev: Severity) {
    setSeverities((prev) => {
      const next = new Set(prev);
      if (next.has(sev)) next.delete(sev);
      else next.add(sev);
      return next;
    });
  }
  function toggleGroup(key: string) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  const { recordCount, occurrenceCount, bySeverity } = summary;

  return (
    <div className="cv-panel cv-diagnostics" aria-label="Diagnostics">
      <h2 className="cv-panel-title">Diagnostics</h2>

      {/* Summary: records vs represented occurrences, kept distinct. */}
      <p className="cv-diag-summary" role="status">
        <strong>{plural(recordCount, "record")}</strong>
        <span aria-hidden="true"> · </span>
        <strong>{plural(occurrenceCount, "occurrence")}</strong>
      </p>
      <p className="cv-diag-summary cv-muted">
        {`${plural(bySeverity.error, "error")} · ${plural(bySeverity.warning, "warning")} · ${bySeverity.info} info`}
        <span className="cv-visually-hidden"> (counted as occurrences)</span>
      </p>

      {/* Filters. */}
      <div className="cv-diag-filters">
        <div className="cv-diag-sevfilter" role="group" aria-label="Filter by severity">
          {ALL_SEVERITIES.map((sev) => (
            <button
              key={sev}
              type="button"
              className={`cv-diag-sevbtn cv-diag-sev-${sev}`}
              aria-pressed={severities.has(sev)}
              onClick={() => toggleSeverity(sev)}
            >
              <span aria-hidden="true">{SEVERITY_LABEL[sev].symbol} </span>
              {SEVERITY_LABEL[sev].word}
            </button>
          ))}
        </div>
        <label className="cv-diag-search">
          <span className="cv-visually-hidden">Search diagnostics by code or text</span>
          <input
            type="search"
            placeholder="Filter by code or text…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
      </div>

      {/* Groups. */}
      {visibleGroups.length === 0 ? (
        <p className="cv-muted" role="status">
          {recordCount === 0 ? "No diagnostics." : "No diagnostics match the current filter."}
        </p>
      ) : (
        <ul className="cv-diag-groups">
          {visibleGroups.map((group) => {
            const key = `${group.severity} ${group.code}`;
            return (
              <DiagnosticGroupItem
                key={key}
                group={group}
                groupKey={key}
                open={expanded.has(key)}
                showingAll={showAll.has(key)}
                onToggle={() => toggleGroup(key)}
                onShowAll={() => setShowAll((prev) => new Set(prev).add(key))}
              />
            );
          })}
        </ul>
      )}
    </div>
  );
}

function DiagnosticGroupItem({
  group,
  groupKey,
  open,
  showingAll,
  onToggle,
  onShowAll,
}: {
  group: DiagnosticGroup;
  groupKey: string;
  open: boolean;
  showingAll: boolean;
  onToggle: () => void;
  onShowAll: () => void;
}) {
  const sev = SEVERITY_LABEL[group.severity];
  const explanation = explanationFor(group.code);
  const panelId = `cv-diag-group-${groupKey.replace(/[^a-z0-9]+/gi, "-")}`;
  const label = `${sev.word} ${group.code}, ${plural(group.recordCount, "record")}, ${plural(
    group.occurrenceCount,
    "occurrence",
  )}${group.affectedEntities > 0 ? `, ${plural(group.affectedEntities, "affected entity")}` : ""}`;

  const shown = showingAll ? group.records : group.records.slice(0, REPRESENTATIVE);
  const hidden = group.recordCount - shown.length;

  return (
    <li className={`cv-diag-group cv-diag-sev-${group.severity}`}>
      <button
        type="button"
        className="cv-diag-grouphead"
        aria-expanded={open}
        aria-controls={panelId}
        aria-label={label}
        onClick={onToggle}
      >
        <span className="cv-diag-sevtag" aria-hidden="true">
          {sev.symbol} {sev.word}
        </span>
        <code className="cv-diag-code">{group.code}</code>
        <span className="cv-diag-counts" aria-hidden="true">
          {plural(group.recordCount, "record")} · {plural(group.occurrenceCount, "occurrence")}
        </span>
      </button>
      {explanation !== "" && (
        <p className="cv-diag-explain cv-muted" aria-hidden="true">
          {explanation}
        </p>
      )}
      {open && (
        <div id={panelId} className="cv-diag-entries">
          <ul>
            {shown.map((record, i) => (
              <li key={i} className="cv-diag-entry">
                {record.message}
                {record.occurrence_count > 1 && (
                  <span className="cv-diag-occ"> · {plural(record.occurrence_count, "occurrence")}</span>
                )}
              </li>
            ))}
          </ul>
          {hidden > 0 && (
            <button type="button" className="cv-diag-showall" onClick={onShowAll}>
              Show all {group.recordCount} records
            </button>
          )}
        </div>
      )}
    </li>
  );
}
