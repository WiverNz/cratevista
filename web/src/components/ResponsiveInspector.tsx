// Responsive inspector shell (Issue 15, Phase 2).
//
// Presentation only: it re-homes the EXISTING inspector content (ViewDocs,
// GraphList, entity/relation Inspector, DiagnosticsPanel) into one of three
// shells by viewport width, and owns nothing but local open/closed state:
//
//   - wide   (>= 1200px): a stable right-side complementary grid column;
//   - medium (768-1199px): a right-side modal drawer dialog;
//   - narrow (< 768px):    a full-viewport modal dialog.
//
// It never serializes its open state to the URL, never touches the graph
// projection / layout / focus / selection, and never requests an ELK layout.
// Switching selection updates the content; closing keeps the selection intact.
import { useCallback, useEffect, useRef, useState } from "react";

import { useUi, type Projection } from "../app/AppContext.tsx";
import { pushEscape } from "../app/escapeStack.ts";
import { GRAPH_PANEL_ID } from "./Chrome.tsx";
import { ViewDocs } from "./ViewDocs.tsx";
import { DiagnosticsPanel, GraphList, Inspector } from "./Panels.tsx";

/** Viewport class, matching the locked responsive-inspector breakpoints. */
export type ViewportClass = "wide" | "medium" | "narrow";

/** Reactively classifies the viewport. Without `matchMedia` (jsdom default) it
 *  reports `wide`, so the inspector is the stable grid column unless a test
 *  explicitly stubs a narrower width. */
export function useViewportClass(): ViewportClass {
  const compute = (): ViewportClass => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return "wide";
    if (window.matchMedia("(min-width: 1200px)").matches) return "wide";
    if (window.matchMedia("(min-width: 768px)").matches) return "medium";
    return "narrow";
  };
  const [cls, setCls] = useState<ViewportClass>(compute);
  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") return;
    const queries = [
      window.matchMedia("(min-width: 1200px)"),
      window.matchMedia("(min-width: 768px)"),
    ];
    const onChange = () => setCls(compute());
    onChange();
    queries.forEach((q) => q.addEventListener?.("change", onChange));
    return () => queries.forEach((q) => q.removeEventListener?.("change", onChange));
  }, []);
  return cls;
}

/** The inspector body — identical in every shell, built only from existing data.
 *  `modal` tells the Inspector whether it is inside the dialog (so it yields the
 *  Escape key to the dialog and never clears selection on close). */
function InspectorBody({ projection, modal }: { projection: Projection | null; modal: boolean }) {
  return (
    <>
      <ViewDocs />
      {projection && <GraphList projection={projection} />}
      <Inspector modal={modal} />
      <DiagnosticsPanel />
    </>
  );
}

const FOCUSABLE =
  'a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';

/** Elements behind a modal inspector that must not stay keyboard-reachable. */
const BACKGROUND_SELECTORS = [".cv-region-header", ".cv-region-tabs", `#${GRAPH_PANEL_ID}`];

export function ResponsiveInspector({ projection }: { projection: Projection | null }) {
  const cls = useViewportClass();
  const selection = useUi((s) => s.selection);
  const [open, setOpen] = useState(false);
  const opener = useRef<HTMLElement | null>(null);
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const restoreFocus = useRef(false);
  // The selection key we last auto-opened for. Keyed on selection so that an
  // explicit Close does NOT immediately reopen the drawer (the selection still
  // stands): only a *new* selection reopens it.
  const lastAutoOpened = useRef<string | null>(null);

  const selectionKey = selection.kind === "none" ? null : `${selection.kind}:${selection.id}`;
  const modal = cls !== "wide";

  // Selecting an entity/relation on a modal width opens the drawer. Changing the
  // selection while open leaves it open (the body just re-reads selection). Closing
  // is independent and must persist while the selection stands (reopen via the
  // trigger). This is genuine, non-derivable UI state, synced from selection.
  useEffect(() => {
    if (!modal) return;
    if (selectionKey == null) {
      lastAutoOpened.current = null;
      return;
    }
    if (selectionKey === lastAutoOpened.current) return; // already handled
    lastAutoOpened.current = selectionKey;
    if (typeof document !== "undefined") {
      opener.current = document.activeElement as HTMLElement | null;
    }
    setOpen(true);
  }, [modal, selectionKey]);

  const close = useCallback(() => {
    restoreFocus.current = true;
    setOpen(false);
  }, []);

  // Deterministic focus restoration AFTER the dialog unmounts and the trigger has
  // remounted: return focus to the control/entity that opened it when that is a
  // real, still-connected focusable control; otherwise to the persistent trigger.
  useEffect(() => {
    if (open || !restoreFocus.current) return;
    restoreFocus.current = false;
    const el = opener.current;
    const restorable =
      el != null &&
      el !== document.body &&
      document.contains(el) &&
      typeof (el as HTMLElement).focus === "function";
    (restorable ? (el as HTMLElement) : triggerRef.current)?.focus();
  }, [open]);

  const openFromTrigger = useCallback(() => {
    if (typeof document !== "undefined") opener.current = document.activeElement as HTMLElement | null;
    setOpen(true);
  }, []);

  const dialogOpen = modal && open;

  // Escape closes the modal inspector — via the shared escape stack, so a source
  // viewer opened INSIDE the drawer (pushed later) takes Escape first, and only
  // once it has popped does Escape close the drawer (keeping the selection).
  useEffect(() => {
    if (!dialogOpen) return;
    return pushEscape(close);
  }, [dialogOpen, close]);

  // Focus moves into the dialog on open; background regions are made inert so no
  // covered content stays keyboard-reachable. Active graph data is never
  // aria-hidden — only inert (and only while the modal is open).
  useEffect(() => {
    if (!dialogOpen) return;
    const dialog = dialogRef.current;
    const first = dialog?.querySelector<HTMLElement>(FOCUSABLE);
    (first ?? dialog)?.focus();
    const bg = BACKGROUND_SELECTORS.map((s) => document.querySelector(s)).filter(
      (el): el is Element => el != null,
    );
    bg.forEach((el) => el.setAttribute("inert", ""));
    return () => bg.forEach((el) => el.removeAttribute("inert"));
  }, [dialogOpen]);

  // Simple, dependency-free focus trap inside the dialog.
  const onDialogKeyDown = useCallback((e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "Tab") return;
    const dialog = dialogRef.current;
    if (!dialog) return;
    const focusables = Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE));
    if (focusables.length === 0) {
      e.preventDefault();
      return;
    }
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }, []);

  // Wide: the stable complementary column. No dialog, no open/close.
  if (!modal) {
    return (
      <aside className="cv-inspector cv-inspector--wide" aria-label="Details inspector">
        <InspectorBody projection={projection} modal={false} />
      </aside>
    );
  }

  // Medium / narrow: a modal dialog, plus a persistent trigger to (re)open it
  // while a selection stands. Both are fixed, so they occupy no grid column.
  return (
    <>
      {!open && (
        <button
          ref={triggerRef}
          type="button"
          className="cv-inspector-trigger"
          aria-haspopup="dialog"
          onClick={openFromTrigger}
        >
          Details
        </button>
      )}
      {open && (
        <div
          className={`cv-inspector-scrim cv-inspector-scrim--${cls}`}
          // The scrim closes on click, but is not a focusable/keyboard control:
          // Escape and the Close button are the keyboard paths.
          onClick={close}
        >
          <div
            ref={dialogRef}
            className={`cv-inspector-dialog cv-inspector-dialog--${cls}`}
            role="dialog"
            aria-modal="true"
            aria-label="Details inspector"
            tabIndex={-1}
            onClick={(e) => e.stopPropagation()}
            onKeyDown={onDialogKeyDown}
          >
            <div className="cv-inspector-dialog-head">
              <h2 className="cv-panel-title">Details</h2>
              <button
                type="button"
                className="cv-control cv-inspector-close"
                aria-label="Close details"
                onClick={close}
              >
                Close
              </button>
            </div>
            <div className="cv-inspector-dialog-body">
              <InspectorBody projection={projection} modal={true} />
            </div>
          </div>
        </div>
      )}
    </>
  );
}
