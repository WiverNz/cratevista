import { describe, it, expect, beforeEach, vi } from "vitest";

vi.mock("@xyflow/react", () => import("./support/xyflow.tsx"));

import { render, screen, fireEvent, within } from "@testing-library/react";
import { App } from "../src/App.tsx";
import type { ArtifactSource, LoadOutcome } from "../src/api/load.ts";
import type { ExplorerDocument } from "../src/types/index.ts";
import {
  fakeLayout,
  fakeSource,
  okOutcome,
  sampleDocument,
  watchDisabledLiveReload,
  STRUCT,
  ENUM,
} from "./support/harness.tsx";

/** The sample document with a project repository + branch. STRUCT keeps its
 *  `src/app.rs` location (lines 10-20); ENUM has no source. */
function withRepository(
  repository_url: string | null,
  default_branch: string | null = "main",
): ExplorerDocument {
  const doc = sampleDocument();
  return {
    ...doc,
    project: { ...doc.project, repository_url, default_branch },
  } as ExplorerDocument;
}

function renderWith(document: ExplorerDocument, mode: "server" | "static" = "static") {
  const outcome: LoadOutcome = okOutcome({ document });
  const source: ArtifactSource = fakeSource(outcome);
  return render(
    <App
      mode={mode}
      source={source}
      layout={fakeLayout().engine}
      liveReload={watchDisabledLiveReload}
    />,
  );
}

async function selectNode(id: string) {
  await screen.findByRole("tablist", { name: "Views" });
  fireEvent.click(screen.getByTestId(`node-${id}`));
  return screen.findByRole("region", { name: "Entity inspector" });
}

beforeEach(() => {
  window.history.pushState(null, "", "/");
});

describe("inspector repository/source links", () => {
  it("renders a source deep link and a repository root link with safe hrefs", async () => {
    renderWith(withRepository("https://github.com/owner/repo"));
    await selectNode(STRUCT);
    const section = screen.getByLabelText("Repository links");

    const source = within(section).getByRole("link", {
      name: /Open this source file on GitHub/i,
    });
    expect(source).toHaveAttribute("href", "https://github.com/owner/repo/blob/main/src/app.rs#L10");

    const root = within(section).getByRole("link", { name: /Open the repository on GitHub/i });
    expect(root).toHaveAttribute("href", "https://github.com/owner/repo");
  });

  it("sets external-link attributes and an accessible provider name", async () => {
    renderWith(withRepository("https://gitlab.com/owner/repo"));
    await selectNode(STRUCT);
    const section = screen.getByLabelText("Repository links");
    for (const link of within(section).getAllByRole("link")) {
      expect(link).toHaveAttribute("target", "_blank");
      expect(link).toHaveAttribute("rel", "noopener noreferrer");
      expect(link.getAttribute("aria-label")).toMatch(/GitLab/);
      expect(link.getAttribute("aria-label")).toMatch(/new tab/i);
    }
    // GitLab uses the /-/blob layout.
    expect(
      within(section).getByRole("link", { name: /source file/i }),
    ).toHaveAttribute("href", "https://gitlab.com/owner/repo/-/blob/main/src/app.rs#L10");
  });

  it("shows only a root link for an entity with no source location", async () => {
    renderWith(withRepository("https://github.com/owner/repo"));
    await selectNode(ENUM); // ENUM has no `source`
    const section = screen.getByLabelText("Repository links");
    expect(within(section).queryByRole("link", { name: /source file/i })).not.toBeInTheDocument();
    expect(within(section).getByRole("link", { name: /Open the repository/i })).toBeInTheDocument();
  });

  it("renders no repository section when repository_url is unsafe or missing", async () => {
    for (const url of ["ssh://git@github.com/o/r.git", "https://user:pass@github.com/o/r", null]) {
      const { unmount } = renderWith(withRepository(url));
      await selectNode(STRUCT);
      expect(screen.queryByLabelText("Repository links")).not.toBeInTheDocument();
      unmount();
    }
  });

  it("shows only a root link for an unsupported HTTPS host, even with a location", async () => {
    renderWith(withRepository("https://git.example.com/o/r"));
    await selectNode(STRUCT);
    const section = screen.getByLabelText("Repository links");
    expect(within(section).queryByRole("link", { name: /source file/i })).not.toBeInTheDocument();
    const root = within(section).getByRole("link", { name: /Open the repository/i });
    expect(root).toHaveAttribute("href", "https://git.example.com/o/r");
  });

  it("renders identically in server and static mode for the same document", async () => {
    const doc = withRepository("https://github.com/owner/repo");
    const staticView = renderWith(doc, "static");
    await selectNode(STRUCT);
    const staticHtml = screen.getByLabelText("Repository links").innerHTML;
    staticView.unmount();

    const serverView = renderWith(doc, "server");
    await selectNode(STRUCT);
    const serverHtml = screen.getByLabelText("Repository links").innerHTML;
    serverView.unmount();

    expect(staticHtml).toBe(serverHtml);
  });
});
