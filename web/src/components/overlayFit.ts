// Overlay-safe fit: a pure, centralized model that converts the visible overlay
// rectangles into per-side fit padding, so `fitView` keeps content (including a
// just-selected node) clear of the corner overlays and the wide inspector column.
//
// It NEVER touches node coordinates or the layout result: it only produces the
// asymmetric `{top,right,bottom,left}` padding React Flow 12 accepts on
// `fitView({ padding })`. Everything here is pure and unit-testable; the caller
// reads the DOM rects on demand (at fit time), never in a loop.

/** The subset of a `DOMRect` this model needs (all in viewport pixels). */
export interface InsetRect {
  readonly left: number;
  readonly top: number;
  readonly right: number;
  readonly bottom: number;
}

/** Per-side fit padding, in pixels. */
export interface FitPadding {
  readonly top: number;
  readonly right: number;
  readonly bottom: number;
  readonly left: number;
}

/** The overlay rectangles that occupy the graph viewport, by anchor. Any may be
 *  absent (not mounted / not visible). `inspector` is the wide inspector column
 *  or the medium drawer when it overlaps the graph. */
export interface OverlayRects {
  readonly topLeft?: InsetRect | null;
  readonly topRight?: InsetRect | null;
  readonly bottomLeft?: InsetRect | null;
  readonly inspector?: InsetRect | null;
}

/** Base gap kept on every side even with no overlay. */
export const OVERLAY_FIT_BASE = 24;
/** Extra clearance added beyond an overlay's own extent. */
export const OVERLAY_FIT_GAP = 12;

/** How far a panel reaches from a container edge; 0 when it does not overlap. */
function reach(panel: InsetRect | null | undefined, extentPx: number): number {
  if (!panel) return 0;
  return extentPx > 0 ? extentPx + OVERLAY_FIT_GAP : 0;
}

/**
 * Computes asymmetric fit padding from the container and the overlay rects.
 *
 * Each side's padding is the base gap plus the deepest overlay reach into that
 * side. Left-anchored panels (top-left controls, bottom-left legend) push the
 * left inset; right-anchored panels (top-right controls, inspector) push the
 * right inset; and so on. A panel that does not overlap a side contributes
 * nothing to it, so a wide inspector rendered as a separate column (to the right
 * of, not over, the graph) adds no right inset.
 */
export function overlayFitPadding(container: InsetRect, overlays: OverlayRects): FitPadding {
  const left =
    OVERLAY_FIT_BASE +
    Math.max(
      reach(overlays.topLeft, (overlays.topLeft?.right ?? 0) - container.left),
      reach(overlays.bottomLeft, (overlays.bottomLeft?.right ?? 0) - container.left),
    );
  const right =
    OVERLAY_FIT_BASE +
    Math.max(
      reach(overlays.topRight, container.right - (overlays.topRight?.left ?? 0)),
      reach(overlays.inspector, container.right - (overlays.inspector?.left ?? 0)),
    );
  const top =
    OVERLAY_FIT_BASE +
    Math.max(
      reach(overlays.topLeft, (overlays.topLeft?.bottom ?? 0) - container.top),
      reach(overlays.topRight, (overlays.topRight?.bottom ?? 0) - container.top),
    );
  const bottom =
    OVERLAY_FIT_BASE + reach(overlays.bottomLeft, container.bottom - (overlays.bottomLeft?.top ?? 0));
  return { top, right, bottom, left };
}

/** Narrows a `DOMRect`-like value to the fields this model reads (or null). */
export function toInsetRect(rect: DOMRect | null | undefined): InsetRect | null {
  if (!rect) return null;
  return { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom };
}
