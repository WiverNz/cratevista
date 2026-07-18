// Layout client: manages the ELK worker, request tokens, stale-response
// rejection, caching, and recoverable error handling. The worker is injected
// (WorkerFactory) so this is unit-testable without a real Worker/browser.
import { LayoutCache } from "./cache.ts";
import {
  DEFAULT_LAYOUT_OPTIONS,
  type LayoutRequest,
  type LayoutResponse,
  type LayoutResult,
} from "./types.ts";

/** Minimal structural subset of a Web Worker used by the client. */
export interface WorkerLike {
  postMessage(message: LayoutRequest): void;
  onmessage: ((event: { data: LayoutResponse }) => void) | null;
  onerror: ((event: unknown) => void) | null;
  terminate(): void;
}

export type WorkerFactory = () => WorkerLike;

export type LayoutOutcome =
  | { status: "ok"; result: LayoutResult }
  | { status: "error"; error: string }
  | { status: "stale" };

/** The layout capability the UI depends on (implemented by `LayoutClient`;
 *  faked in tests). */
export interface LayoutEngine {
  layout(input: LayoutInput): Promise<LayoutOutcome>;
  terminate(): void;
}

export interface LayoutInput {
  key: string;
  request: Omit<LayoutRequest, "token" | "options"> &
    Partial<Pick<LayoutRequest, "options">>;
}

/** The real browser worker factory (same-origin ES module worker; no blob). */
export function defaultWorkerFactory(): WorkerLike {
  return new Worker(new URL("./elk.worker.ts", import.meta.url), {
    type: "module",
  }) as unknown as WorkerLike;
}

export class LayoutClient {
  private token = 0;
  private worker: WorkerLike | null = null;
  private readonly cache = new LayoutCache();
  private pending: {
    token: number;
    key: string;
    resolve: (o: LayoutOutcome) => void;
  } | null = null;

  constructor(private readonly factory: WorkerFactory = defaultWorkerFactory) {}

  private ensureWorker(): WorkerLike {
    if (this.worker) return this.worker;
    const worker = this.factory();
    worker.onmessage = (event) => this.handleResponse(event.data);
    worker.onerror = () => this.failPending("layout worker crashed");
    this.worker = worker;
    return worker;
  }

  private handleResponse(response: LayoutResponse): void {
    // Discard stale responses (a newer request superseded this one).
    if (!this.pending || response.token !== this.pending.token) return;
    const { resolve, key } = this.pending;
    this.pending = null;
    if (response.ok) {
      this.cache.set(key, response.result);
      resolve({ status: "ok", result: response.result });
    } else {
      resolve({ status: "error", error: response.error });
    }
  }

  private failPending(error: string): void {
    if (!this.pending) return;
    const { resolve } = this.pending;
    this.pending = null;
    resolve({ status: "error", error });
  }

  /** Requests a layout. Returns cached geometry immediately when the key hits;
   *  otherwise posts to the worker. A superseded request resolves `stale`. */
  layout(input: LayoutInput): Promise<LayoutOutcome> {
    const cached = this.cache.get(input.key);
    if (cached) return Promise.resolve({ status: "ok", result: cached });

    const worker = this.ensureWorker();
    const token = ++this.token;

    // Supersede any in-flight request: mark it stale.
    if (this.pending) {
      const superseded = this.pending;
      this.pending = null;
      superseded.resolve({ status: "stale" });
    }

    return new Promise<LayoutOutcome>((resolve) => {
      this.pending = { token, key: input.key, resolve };
      const request: LayoutRequest = {
        token,
        nodes: input.request.nodes,
        edges: input.request.edges,
        stages: input.request.stages,
        nodeStage: input.request.nodeStage,
        options: input.request.options ?? DEFAULT_LAYOUT_OPTIONS,
      };
      worker.postMessage(request);
    });
  }

  terminate(): void {
    this.failPending("layout client terminated");
    this.worker?.terminate();
    this.worker = null;
  }
}
