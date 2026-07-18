// Guarded source-content client for PRD-06's opt-in `GET /api/source`.
//
// The server exposes contents only (no line/column API), so we send just the
// URL-encoded repo-relative path. `source_disabled` (403) is a normal capability
// absence, not an error. No response ever yields an absolute path — we display
// only the repo-relative path we already have from the document.
import { apiErrorCode } from "../types/runtime.ts";
import type { FetchFn } from "./load.ts";

/** Stable non-fatal source errors from PRD 06. */
export type SourceErrorCode =
  | "source_path_invalid"
  | "source_outside_root"
  | "source_not_file"
  | "source_too_large"
  | "source_not_utf8";

const STABLE_CODES: readonly string[] = [
  "source_path_invalid",
  "source_outside_root",
  "source_not_file",
  "source_too_large",
  "source_not_utf8",
];

/** User-facing text per stable code — never echoes server paths or raw text. */
export const SOURCE_ERROR_MESSAGES: Record<SourceErrorCode, string> = {
  source_path_invalid: "That source path is not valid.",
  source_outside_root: "That file resolves outside the project root.",
  source_not_file: "That path is not a regular file.",
  source_too_large: "That file is too large to display here.",
  source_not_utf8: "That file is not valid UTF-8 text.",
};

export type SourceOutcome =
  | { status: "ok"; text: string }
  /** Server has source serving turned off — a normal capability absence. */
  | { status: "disabled" }
  /** A stable, non-fatal source error. */
  | { status: "error"; code: SourceErrorCode; message: string }
  /** Network or malformed response — generic + retryable. */
  | { status: "failed"; message: string };

export interface SourceClient {
  /** Fetches contents for a repo-relative path. Rejects with `AbortError` when
   *  the signal aborts; callers ignore that. */
  fetchSource(path: string, signal?: AbortSignal): Promise<SourceOutcome>;
}

export class HttpSourceClient implements SourceClient {
  constructor(
    private readonly base = "",
    // Wrapped, not a bare `fetch` reference. This is invoked below as
    // `this.fetchFn(...)`, which would call `fetch` with the client instance as
    // its receiver; the browser requires a Window receiver and throws
    // "Illegal invocation", which the catch below would report as an
    // unreachable server. Tests inject a mock, so only a real browser shows it.
    private readonly fetchFn: FetchFn = (input, init) => fetch(input, init),
  ) {}

  async fetchSource(path: string, signal?: AbortSignal): Promise<SourceOutcome> {
    const url = `${this.base}/api/source?path=${encodeURIComponent(path)}`;
    let response: Response;
    try {
      response = await this.fetchFn(url, signal ? { signal } : undefined);
    } catch (error) {
      // Let aborts propagate so the caller can ignore them.
      if (error instanceof Error && error.name === "AbortError") throw error;
      return { status: "failed", message: "Could not reach the server." };
    }

    if (response.ok) {
      try {
        return { status: "ok", text: await response.text() };
      } catch {
        return { status: "failed", message: "Could not read the response." };
      }
    }

    let body: unknown;
    try {
      body = await response.json();
    } catch {
      return { status: "failed", message: "The server returned an unreadable response." };
    }
    const code = apiErrorCode(body);
    if (response.status === 403 && code === "source_disabled") {
      return { status: "disabled" };
    }
    if (code && STABLE_CODES.includes(code)) {
      const stable = code as SourceErrorCode;
      return { status: "error", code: stable, message: SOURCE_ERROR_MESSAGES[stable] };
    }
    return { status: "failed", message: "The server could not return that source file." };
  }
}
