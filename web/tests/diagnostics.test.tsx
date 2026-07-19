import { describe, it, expect } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { DiagnosticsExplorer } from "../src/components/DiagnosticsExplorer.tsx";
import { summarize, filterGroups } from "../src/diagnostics/grouping.ts";
import type { DocumentDiagnostic } from "../src/types/index.ts";

type Sev = "error" | "warning" | "info";
function diag(severity: Sev, code: string, message: string, occurrence_count = 1): DocumentDiagnostic {
  return { severity, code, message, occurrence_count };
}

/** A FlightTrace-shaped set: 19 external + 6 source info records, summing to 2119. */
function flighttraceLike(): DocumentDiagnostic[] {
  const out: DocumentDiagnostic[] = [];
  const external = [1449, 152, 85, 62, 31, 31, 19, 18, 18, 16, 10, 10, 5, 4, 4, 4, 2, 2, 2]; // 19 → 1924
  external.forEach((n, i) => out.push(diag("info", "external_crate_reference", `crate ${i}: ${n}`, n)));
  const source = [115, 29, 25, 18, 7, 1]; // 6 → 195
  source.forEach((n, i) => out.push(diag("info", "source_outside_workspace", `crate ${i}: ${n}`, n)));
  return out;
}

describe("diagnostics grouping model", () => {
  it("an ordinary diagnostic represents one occurrence", () => {
    const s = summarize([diag("warning", "c", "m")]);
    expect(s.recordCount).toBe(1);
    expect(s.occurrenceCount).toBe(1);
  });

  it("an aggregated record contributes its exact occurrence count", () => {
    const s = summarize([diag("info", "external_crate_reference", "serde", 1924)]);
    expect(s.recordCount).toBe(1);
    expect(s.occurrenceCount).toBe(1924);
    expect(s.groups[0].occurrenceCount).toBe(1924);
    expect(s.groups[0].recordCount).toBe(1);
  });

  it("25 records represent 2119 occurrences", () => {
    const s = summarize(flighttraceLike());
    expect(s.recordCount).toBe(25);
    expect(s.occurrenceCount).toBe(2119);
    const ext = s.groups.find((g) => g.code === "external_crate_reference")!;
    const src = s.groups.find((g) => g.code === "source_outside_workspace")!;
    expect([ext.recordCount, ext.occurrenceCount]).toEqual([19, 1924]);
    expect([src.recordCount, src.occurrenceCount]).toEqual([6, 195]);
  });

  it("severity totals count occurrences, not records (negative control 1)", () => {
    const s = summarize(flighttraceLike());
    // 25 records, but 2119 info OCCURRENCES.
    expect(s.bySeverity).toEqual({ error: 0, warning: 0, info: 2119 });
    expect(s.bySeverity.info).not.toBe(s.recordCount);
  });

  it("uses the structured count, never the message text (negative control 2)", () => {
    // Message claims a different number than the structured field.
    const s = summarize([diag("info", "c", "represents 5 things", 1924)]);
    expect(s.occurrenceCount).toBe(1924);
  });

  it("groups deterministically: error, warning, info; then code lexicographically", () => {
    const shuffled = [
      diag("info", "zeta", "z"),
      diag("error", "beta", "b"),
      diag("warning", "alpha", "a"),
      diag("info", "alpha", "a2"),
      diag("error", "alpha", "a3"),
    ];
    const s = summarize(shuffled);
    expect(s.groups.map((g) => `${g.severity}:${g.code}`)).toEqual([
      "error:alpha",
      "error:beta",
      "warning:alpha",
      "info:alpha",
      "info:zeta",
    ]);
  });

  it("filters by severity and by code/text", () => {
    const s = summarize([
      diag("error", "aaa", "hello"),
      diag("warning", "bbb", "world"),
      diag("info", "ccc", "external thing"),
    ]);
    expect(filterGroups(s, new Set(["error"]), "").map((g) => g.code)).toEqual(["aaa"]);
    expect(filterGroups(s, new Set(["error", "warning", "info"]), "bbb").map((g) => g.code)).toEqual(["bbb"]);
    expect(filterGroups(s, new Set(["error", "warning", "info"]), "external").map((g) => g.code)).toEqual(["ccc"]);
  });
});

describe("DiagnosticsExplorer component", () => {
  it("shows the records-vs-occurrences summary and starts with groups collapsed", () => {
    const { container } = render(<DiagnosticsExplorer diagnostics={flighttraceLike()} />);
    // Summary distinguishes records from occurrences.
    expect(screen.getByText(/25 records/)).toBeTruthy();
    expect(screen.getByText(/2119 occurrences/)).toBeTruthy();
    // Collapsed: no entry rows rendered initially.
    expect(container.querySelectorAll(".cv-diag-entry").length).toBe(0);
    // Group headers are present and carry both counts.
    const ext = screen.getByRole("button", { name: /Info external_crate_reference, 19 records, 1924 occurrences/ });
    expect(ext.getAttribute("aria-expanded")).toBe("false");
  });

  it("expands and collapses a group, revealing bounded representative entries", () => {
    render(<DiagnosticsExplorer diagnostics={flighttraceLike()} />);
    const ext = screen.getByRole("button", { name: /external_crate_reference, 19 records/ });
    fireEvent.click(ext);
    expect(ext.getAttribute("aria-expanded")).toBe("true");
    // 19 records < REPRESENTATIVE(20), so all show and there is no "Show all".
    expect(screen.queryByText(/Show all/)).toBeNull();
    fireEvent.click(ext);
    expect(ext.getAttribute("aria-expanded")).toBe("false");
  });

  it("offers an explicit Show all for large groups", () => {
    const many = Array.from({ length: 50 }, (_, i) => diag("warning", "big_code", `item ${i}`));
    const { container } = render(<DiagnosticsExplorer diagnostics={many} />);
    const head = screen.getByRole("button", { name: /Warning big_code, 50 records/ });
    fireEvent.click(head);
    // Representative bound: 20 entries + a "Show all" affordance.
    expect(container.querySelectorAll(".cv-diag-entry").length).toBe(20);
    const showAll = screen.getByRole("button", { name: /Show all 50 records/ });
    fireEvent.click(showAll);
    expect(container.querySelectorAll(".cv-diag-entry").length).toBe(50);
  });

  it("filters by severity and shows an empty-filter state", () => {
    render(<DiagnosticsExplorer diagnostics={[diag("error", "e_code", "boom"), diag("info", "i_code", "note")]} />);
    // Turn off info → only the error group remains. The severity-filter button's
    // accessible name is exactly "Info" (its symbol is aria-hidden).
    fireEvent.click(screen.getByRole("button", { name: "Info" }));
    expect(screen.queryByRole("button", { name: /i_code/ })).toBeNull();
    expect(screen.getByRole("button", { name: /e_code/ })).toBeTruthy();
    // Search with no match → empty state.
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "zzz-nomatch" } });
    expect(screen.getByText(/No diagnostics match/)).toBeTruthy();
  });

  it("keeps initial DOM bounded for 2000+ diagnostics", () => {
    // 2000 diagnostics across several codes and severities.
    const codes = ["alpha", "beta", "gamma", "delta", "epsilon"];
    const sevs: Sev[] = ["error", "warning", "info"];
    const many: DocumentDiagnostic[] = [];
    for (let i = 0; i < 2000; i++) {
      many.push(diag(sevs[i % 3], codes[i % 5], `message ${i}`));
    }
    const { container } = render(<DiagnosticsExplorer diagnostics={many} />);
    // No entry rows rendered initially — only the summary and group headers.
    expect(container.querySelectorAll(".cv-diag-entry").length).toBe(0);
    // Bounded number of group headers: at most severities × codes = 15.
    const headers = container.querySelectorAll(".cv-diag-grouphead");
    expect(headers.length).toBeLessThanOrEqual(15);
    // Expanding ONE group stays bounded (representative only).
    fireEvent.click(headers[0] as HTMLElement);
    expect(container.querySelectorAll(".cv-diag-entry").length).toBeLessThanOrEqual(20);
  });

  it("is keyboard operable with aria-expanded and accessible names", async () => {
    const user = userEvent.setup();
    render(<DiagnosticsExplorer diagnostics={[diag("info", "external_crate_reference", "serde", 1924)]} />);
    const head = screen.getByRole("button", { name: /Info external_crate_reference, 1 record, 1924 occurrences/ });
    expect(head.getAttribute("aria-expanded")).toBe("false");
    head.focus();
    expect(document.activeElement).toBe(head);
    await user.keyboard("{Enter}");
    expect(head.getAttribute("aria-expanded")).toBe("true");
    // The revealed entry is reachable.
    const region = document.getElementById(head.getAttribute("aria-controls")!)!;
    expect(within(region).getByText(/serde/)).toBeTruthy();
  });
});
