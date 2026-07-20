// Deterministic layout cache key + a small bounded cache.
//
// The key includes everything that changes geometry, and deliberately EXCLUDES
// selection, hover, inspector expansion, and diagnostics-panel state, so those
// interactions never invalidate the cache / trigger relayout.
import type { LayoutResult } from "./types.ts";

export interface LayoutCacheKeyParts {
  /** Stable document identity (model.identity). */
  identity: string;
  viewId: string;
  /** Active entity-kind filter (order-independent). */
  kinds: readonly string[];
  /** Edge visibility mode — it changes which edges are laid out. */
  edgeMode: string;
  /** Expanded neighborhoods (order-independent). */
  expanded: readonly string[];
  stage: string | null;
  /** Visible node ids (order-independent). */
  nodeIds: readonly string[];
  /** Visible edge ids (order-independent). */
  edgeIds: readonly string[];
}

/**
 * The key includes only what changes the laid-out geometry. Focus is intentionally
 * NOT a direct input: hide-focus changes the geometry solely by *reducing* the
 * node/edge set (captured here by `nodeIds`/`edgeIds`), and dim-focus keeps the
 * full set unchanged — so the same key is reused and dim never relayouts. Any focus
 * effect that does not change the node/edge set therefore never invalidates the
 * cache.
 */
export function layoutCacheKey(parts: LayoutCacheKeyParts): string {
  const norm = {
    identity: parts.identity,
    viewId: parts.viewId,
    kinds: [...parts.kinds].sort(),
    edgeMode: parts.edgeMode,
    expanded: [...parts.expanded].sort(),
    stage: parts.stage,
    nodeIds: [...parts.nodeIds].sort(),
    edgeIds: [...parts.edgeIds].sort(),
  };
  return JSON.stringify(norm);
}

export class LayoutCache {
  private readonly map = new Map<string, LayoutResult>();

  constructor(private readonly max = 24) {}

  get(key: string): LayoutResult | undefined {
    const hit = this.map.get(key);
    if (hit) {
      // Bump recency.
      this.map.delete(key);
      this.map.set(key, hit);
    }
    return hit;
  }

  set(key: string, value: LayoutResult): void {
    if (this.map.has(key)) this.map.delete(key);
    this.map.set(key, value);
    while (this.map.size > this.max) {
      const oldest = this.map.keys().next().value;
      if (oldest === undefined) break;
      this.map.delete(oldest);
    }
  }

  has(key: string): boolean {
    return this.map.has(key);
  }

  clear(): void {
    this.map.clear();
  }
}
