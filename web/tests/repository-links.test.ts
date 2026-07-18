import { describe, it, expect } from "vitest";
import {
  repositoryLinks,
  type ProjectLike,
  type SourceLocationLike,
} from "../src/api/repositoryLinks.ts";

const branch = "main";
const loc = (path: string, startLine = 12): SourceLocationLike => ({
  path,
  span: { start_line: startLine },
});
const project = (repository_url: string | null | undefined, default_branch = branch): ProjectLike => ({
  repository_url,
  default_branch,
});

describe("repositoryLinks — provider recognition and root", () => {
  it("recognizes GitHub and builds a source deep link", () => {
    const links = repositoryLinks(project("https://github.com/owner/repo"), loc("src/lib.rs", 42));
    expect(links).toEqual({
      provider: "github",
      repository: "https://github.com/owner/repo",
      source: "https://github.com/owner/repo/blob/main/src/lib.rs#L42",
    });
  });

  it("recognizes GitLab and uses its /-/blob layout", () => {
    const links = repositoryLinks(project("https://gitlab.com/owner/repo"), loc("src/lib.rs", 7));
    expect(links?.provider).toBe("gitlab");
    expect(links?.source).toBe("https://gitlab.com/owner/repo/-/blob/main/src/lib.rs#L7");
  });

  it("supports GitLab subgroup repository paths", () => {
    const links = repositoryLinks(
      project("https://gitlab.com/group/subgroup/repo"),
      loc("a/b.rs", 3),
    );
    expect(links?.repository).toBe("https://gitlab.com/group/subgroup/repo");
    expect(links?.source).toBe("https://gitlab.com/group/subgroup/repo/-/blob/main/a/b.rs#L3");
  });
});

describe("repositoryLinks — normalization", () => {
  it("strips a single trailing .git", () => {
    const links = repositoryLinks(project("https://github.com/owner/repo.git"), loc("x.rs"));
    expect(links?.repository).toBe("https://github.com/owner/repo");
    expect(links?.source).toContain("/owner/repo/blob/");
  });

  it("strips a single trailing slash", () => {
    const links = repositoryLinks(project("https://github.com/owner/repo/"), null);
    expect(links?.repository).toBe("https://github.com/owner/repo");
  });
});

describe("repositoryLinks — encoding", () => {
  it("encodes a branch containing a slash as one path value", () => {
    const links = repositoryLinks(
      project("https://github.com/o/r", "feature/x"),
      loc("src/lib.rs", 1),
    );
    expect(links?.source).toBe("https://github.com/o/r/blob/feature%2Fx/src/lib.rs#L1");
  });

  it("encodes each source path component independently (space, %, #, ?, unicode)", () => {
    const links = repositoryLinks(project("https://github.com/o/r"), loc("a b/c%d/e#f/g?h/café.rs", 5));
    // Every reserved char is encoded, and the separators between components remain.
    expect(links?.source).toBe(
      "https://github.com/o/r/blob/main/a%20b/c%25d/e%23f/g%3Fh/caf%C3%A9.rs#L5",
    );
  });

  it("does not double-encode", () => {
    const links = repositoryLinks(project("https://github.com/o/r"), loc("a%20b.rs", 2));
    // A literal `%20` in the path becomes `%2520`, never left as `%20`.
    expect(links?.source).toBe("https://github.com/o/r/blob/main/a%2520b.rs#L2");
  });
});

describe("repositoryLinks — unsupported hosts and unsafe URLs", () => {
  it("gives an unknown HTTPS host a root link only, even with branch + location", () => {
    const links = repositoryLinks(project("https://git.example.com/o/r"), loc("src/lib.rs", 3));
    expect(links).toEqual({ provider: "other", repository: "https://git.example.com/o/r" });
    expect(links?.source).toBeUndefined();
  });

  it("returns null for a credential-bearing URL", () => {
    expect(repositoryLinks(project("https://user:pass@github.com/o/r"), loc("x.rs"))).toBeNull();
    expect(repositoryLinks(project("https://user@github.com/o/r"), loc("x.rs"))).toBeNull();
  });

  it("returns null for ssh / git / git@ / file / http / malformed / relative inputs", () => {
    for (const raw of [
      "ssh://git@github.com/o/r.git",
      "git://github.com/o/r.git",
      "git@github.com:o/r.git",
      "file:///home/user/repo",
      "http://github.com/o/r",
      "not a url",
      "/relative/path",
      "github.com/o/r",
    ]) {
      expect(repositoryLinks(project(raw), loc("x.rs")), raw).toBeNull();
    }
  });

  it("returns null when the repository path is empty", () => {
    expect(repositoryLinks(project("https://github.com/"), loc("x.rs"))).toBeNull();
    expect(repositoryLinks(project("https://github.com"), loc("x.rs"))).toBeNull();
  });
});

describe("repositoryLinks — missing data", () => {
  it("returns null when repository_url is absent or empty", () => {
    expect(repositoryLinks(project(null), loc("x.rs"))).toBeNull();
    expect(repositoryLinks(project(undefined), loc("x.rs"))).toBeNull();
    expect(repositoryLinks(project(""), loc("x.rs"))).toBeNull();
  });

  it("root link only when the default branch is missing", () => {
    const links = repositoryLinks(
      { repository_url: "https://github.com/o/r", default_branch: null },
      loc("src/lib.rs", 3),
    );
    expect(links?.repository).toBe("https://github.com/o/r");
    expect(links?.source).toBeUndefined();
  });

  it("root link only when there is no source location", () => {
    const links = repositoryLinks(project("https://github.com/o/r"), null);
    expect(links?.source).toBeUndefined();
    expect(links?.repository).toBe("https://github.com/o/r");
  });

  it("omits the #L fragment when the span has no positive start line", () => {
    const noSpan = repositoryLinks(project("https://github.com/o/r"), { path: "x.rs" });
    expect(noSpan?.source).toBe("https://github.com/o/r/blob/main/x.rs");
    const zero = repositoryLinks(project("https://github.com/o/r"), { path: "x.rs", span: { start_line: 0 } });
    expect(zero?.source).toBe("https://github.com/o/r/blob/main/x.rs");
  });

  it("never emits a local filesystem URL or a source snippet", () => {
    const links = repositoryLinks(project("https://github.com/o/r"), loc("src/lib.rs", 1));
    expect(links?.source?.startsWith("https://")).toBe(true);
    expect(links?.source).not.toMatch(/file:|[A-Za-z]:\\|\/home\//);
  });
});
