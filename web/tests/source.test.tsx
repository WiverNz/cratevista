import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { screen, fireEvent, waitFor, within } from "@testing-library/react";
import { HttpSourceClient, type SourceClient, type SourceOutcome } from "../src/api/source.ts";
import type { FetchFn } from "../src/api/load.ts";
import { renderApp, STRUCT, ENUM } from "./support/harness.tsx";

function textResponse(body: string, ok = true, status = 200): Response {
  return { ok, status, text: async () => body, json: async () => ({}) } as unknown as Response;
}
function jsonErr(code: string, status: number): Response {
  return {
    ok: false,
    status,
    json: async () => ({ error: { code, message: "server text" } }),
    text: async () => "",
  } as unknown as Response;
}

/** A source client we can drive from tests. */
function fakeClient(outcome: SourceOutcome | (() => Promise<SourceOutcome>)) {
  const calls: string[] = [];
  const client: SourceClient = {
    fetchSource(path, signal) {
      calls.push(path);
      if (typeof outcome === "function") return outcome();
      // Respect abort so abortion tests can observe it.
      return new Promise<SourceOutcome>((resolve, reject) => {
        if (signal?.aborted) {
          reject(Object.assign(new Error("aborted"), { name: "AbortError" }));
          return;
        }
        signal?.addEventListener("abort", () =>
          reject(Object.assign(new Error("aborted"), { name: "AbortError" })),
        );
        resolve(outcome);
      });
    },
  };
  return { client, calls };
}

async function selectStruct() {
  await screen.findByRole("tablist", { name: "Views" });
  fireEvent.click(screen.getByTestId(`node-${STRUCT}`));
  return screen.findByRole("region", { name: "Entity inspector" });
}

beforeEach(() => {
  window.history.pushState(null, "", "/");
});

describe("HttpSourceClient", () => {
  it("URL-encodes the repo-relative path and sends no line/col params", async () => {
    let seen = "";
    const fetchFn: FetchFn = async (url) => {
      seen = url;
      return textResponse("fn main() {}");
    };
    const out = await new HttpSourceClient("", fetchFn).fetchSource("src/a b/lib.rs");
    expect(seen).toBe("/api/source?path=src%2Fa%20b%2Flib.rs");
    expect(seen).not.toMatch(/line|col/);
    expect(out).toEqual({ status: "ok", text: "fn main() {}" });
  });

  it("maps 403 source_disabled to a capability absence", async () => {
    const fetchFn: FetchFn = async () => jsonErr("source_disabled", 403);
    expect(await new HttpSourceClient("", fetchFn).fetchSource("a.rs")).toEqual({
      status: "disabled",
    });
  });

  it.each([
    "source_path_invalid",
    "source_outside_root",
    "source_not_file",
    "source_too_large",
    "source_not_utf8",
  ])("maps stable error %s without echoing server text", async (code) => {
    const fetchFn: FetchFn = async () => jsonErr(code, 400);
    const out = await new HttpSourceClient("", fetchFn).fetchSource("a.rs");
    expect(out.status).toBe("error");
    if (out.status === "error") {
      expect(out.code).toBe(code);
      expect(out.message).not.toContain("server text");
    }
  });

  it("maps a malformed error body to a generic retryable failure", async () => {
    const fetchFn: FetchFn = async () =>
      ({ ok: false, status: 500, json: async () => { throw new Error("bad json"); } }) as unknown as Response;
    const out = await new HttpSourceClient("", fetchFn).fetchSource("a.rs");
    expect(out.status).toBe("failed");
  });

  it("maps a network failure to a generic retryable failure", async () => {
    const fetchFn: FetchFn = async () => {
      throw new Error("offline");
    };
    expect((await new HttpSourceClient("", fetchFn).fetchSource("a.rs")).status).toBe("failed");
  });

  it("propagates AbortError so callers can ignore it", async () => {
    const fetchFn: FetchFn = async () => {
      throw Object.assign(new Error("aborted"), { name: "AbortError" });
    };
    await expect(new HttpSourceClient("", fetchFn).fetchSource("a.rs")).rejects.toThrow("aborted");
  });
});

describe("source action in the inspector", () => {
  it("does not request source before explicit activation", async () => {
    const { client, calls } = fakeClient({ status: "ok", text: "code" });
    renderApp({ sourceClient: client });
    await selectStruct();
    // Location visible, but no request yet.
    expect(screen.getByLabelText("Source contents")).toBeInTheDocument();
    expect(calls).toEqual([]);
    expect(screen.getByRole("button", { name: "Show source" })).toBeInTheDocument();
  });

  it("shows source contents on activation and keeps the repo-relative path", async () => {
    const { client, calls } = fakeClient({ status: "ok", text: "pub struct Thing;" });
    renderApp({ sourceClient: client });
    await selectStruct();
    fireEvent.click(screen.getByRole("button", { name: "Show source" }));
    await waitFor(() => expect(screen.getByText("pub struct Thing;")).toBeInTheDocument());
    expect(calls).toEqual(["src/app.rs"]);
    const section = screen.getByLabelText("Source contents");
    expect(within(section).getByText("src/app.rs")).toBeInTheDocument();
    // The span from the document remains visible in the fields list.
    expect(screen.getByText(/:10-20/)).toBeInTheDocument();
  });

  it("never displays an absolute path", async () => {
    const { client } = fakeClient({ status: "ok", text: "code here" });
    renderApp({ sourceClient: client });
    await selectStruct();
    fireEvent.click(screen.getByRole("button", { name: "Show source" }));
    await waitFor(() => expect(screen.getByText("code here")).toBeInTheDocument());
    const html = screen.getByLabelText("Source contents").innerHTML;
    expect(html).not.toMatch(/[A-Za-z]:\\/);
    expect(html).not.toContain("/home/");
  });

  it("source_disabled degrades to location-only without a global error", async () => {
    const { client } = fakeClient({ status: "disabled" });
    renderApp({ sourceClient: client });
    await selectStruct();
    fireEvent.click(screen.getByRole("button", { name: "Show source" }));
    await waitFor(() =>
      expect(screen.getByText(/Source contents are disabled/i)).toBeInTheDocument(),
    );
    // Location still visible; inspector still usable; no global error state.
    expect(within(screen.getByLabelText("Source contents")).getByText("src/app.rs")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Entity inspector" })).toBeInTheDocument();
    expect(screen.queryByText(/Could not load the explorer/i)).not.toBeInTheDocument();
  });

  it("shows a specific inline message for a stable error", async () => {
    const { client } = fakeClient({
      status: "error",
      code: "source_too_large",
      message: "That file is too large to display here.",
    });
    renderApp({ sourceClient: client });
    await selectStruct();
    fireEvent.click(screen.getByRole("button", { name: "Show source" }));
    await waitFor(() => expect(screen.getByText(/too large/i)).toBeInTheDocument());
    expect(screen.getByRole("region", { name: "Entity inspector" })).toBeInTheDocument();
  });

  it("offers retry after a generic failure", async () => {
    let attempt = 0;
    const client: SourceClient = {
      fetchSource: async () => {
        attempt += 1;
        return attempt === 1
          ? { status: "failed", message: "Could not reach the server." }
          : { status: "ok", text: "recovered" };
      },
    };
    renderApp({ sourceClient: client });
    await selectStruct();
    fireEvent.click(screen.getByRole("button", { name: "Show source" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(screen.getByText("recovered")).toBeInTheDocument());
  });

  it("aborts the in-flight request when the selection changes", async () => {
    let aborted = false;
    const client: SourceClient = {
      fetchSource: (_path, signal) =>
        new Promise((_resolve, reject) => {
          signal?.addEventListener("abort", () => {
            aborted = true;
            reject(Object.assign(new Error("aborted"), { name: "AbortError" }));
          });
        }),
    };
    renderApp({ sourceClient: client });
    await selectStruct();
    fireEvent.click(screen.getByRole("button", { name: "Show source" }));
    // Change selection → SourceSection unmounts (keyed by entity id) → abort.
    fireEvent.click(screen.getByTestId(`node-${ENUM}`));
    await waitFor(() => expect(aborted).toBe(true));
    expect(screen.getByRole("region", { name: "Entity inspector" })).toBeInTheDocument();
  });

  it("aborts on unmount / when the inspector closes", async () => {
    let aborted = false;
    const client: SourceClient = {
      fetchSource: (_path, signal) =>
        new Promise((_resolve, reject) => {
          signal?.addEventListener("abort", () => {
            aborted = true;
            reject(Object.assign(new Error("aborted"), { name: "AbortError" }));
          });
        }),
    };
    renderApp({ sourceClient: client });
    await selectStruct();
    fireEvent.click(screen.getByRole("button", { name: "Show source" }));
    // Escape clears the selection → inspector closes → section unmounts.
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(aborted).toBe(true));
  });

  it("ignores a late response after the section unmounts (stale)", async () => {
    let resolveLate: ((o: SourceOutcome) => void) | null = null;
    const client: SourceClient = {
      fetchSource: () => new Promise<SourceOutcome>((r) => (resolveLate = r)),
    };
    renderApp({ sourceClient: client });
    await selectStruct();
    fireEvent.click(screen.getByRole("button", { name: "Show source" }));
    await waitFor(() => expect(screen.getByText(/Loading source/i)).toBeInTheDocument());
    // Clear selection → the keyed SourceSection unmounts.
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() =>
      expect(screen.queryByLabelText("Source contents")).not.toBeInTheDocument(),
    );
    // A late resolution must not render or throw.
    resolveLate!({ status: "ok", text: "LATE-STALE" });
    await new Promise((r) => setTimeout(r, 10));
    expect(screen.queryByText("LATE-STALE")).not.toBeInTheDocument();
  });
});
