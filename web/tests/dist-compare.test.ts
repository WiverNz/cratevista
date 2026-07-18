import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  compareDirs,
  formatDiff,
  isDirectory,
  isIdentical,
  listFilesRecursive,
} from "../scripts/dist-compare.mjs";

// The check:dist drift guard rests entirely on this comparison being recursive
// and byte-exact, so it is tested directly against real temp directories.
describe("dist-compare", () => {
  let root: string;
  const write = (dir: string, rel: string, bytes: string | Uint8Array) => {
    const abs = join(dir, ...rel.split("/"));
    mkdirSync(dirname(abs), { recursive: true });
    writeFileSync(abs, bytes);
  };

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), "dist-compare-test-"));
  });
  afterEach(() => {
    rmSync(root, { recursive: true, force: true });
  });

  const pair = () => {
    const a = join(root, "baseline");
    const b = join(root, "candidate");
    mkdirSync(a, { recursive: true });
    mkdirSync(b, { recursive: true });
    return [a, b] as const;
  };

  it("lists nested files as sorted relative POSIX paths", () => {
    const [a] = pair();
    write(a, "index.html", "x");
    write(a, "assets/deep/nested.js", "y");
    write(a, "assets/index.css", "z");
    expect(listFilesRecursive(a)).toEqual([
      "assets/deep/nested.js",
      "assets/index.css",
      "index.html",
    ]);
  });

  it("reports identical trees as identical", () => {
    const [a, b] = pair();
    for (const dir of [a, b]) {
      write(dir, "index.html", "same");
      write(dir, "assets/app.abcdef12.js", "same");
    }
    const diff = compareDirs(a, b);
    expect(diff).toEqual({ added: [], removed: [], changed: [] });
    expect(isIdentical(diff)).toBe(true);
  });

  it("detects added, removed and changed files recursively", () => {
    const [a, b] = pair();
    write(a, "index.html", "one");
    write(b, "index.html", "two"); // changed
    write(a, "assets/gone.js", "x"); // removed
    write(b, "assets/new.js", "y"); // added
    write(a, "assets/keep.css", "k");
    write(b, "assets/keep.css", "k"); // unchanged

    const diff = compareDirs(a, b);
    expect(diff.added).toEqual(["assets/new.js"]);
    expect(diff.removed).toEqual(["assets/gone.js"]);
    expect(diff.changed).toEqual(["index.html"]);
    expect(isIdentical(diff)).toBe(false);
    expect(formatDiff(diff)).toContain("assets/gone.js");
    expect(formatDiff(diff)).toContain("assets/new.js");
    expect(formatDiff(diff)).toContain("index.html");
  });

  it("compares bytes, not text: a one-byte and line-ending difference counts", () => {
    const [a, b] = pair();
    write(a, "a.js", new Uint8Array([0x00, 0x01, 0x02]));
    write(b, "a.js", new Uint8Array([0x00, 0x01, 0x03]));
    write(a, "b.js", "line\n");
    write(b, "b.js", "line\r\n");
    expect(compareDirs(a, b).changed).toEqual(["a.js", "b.js"]);
  });

  it("detects same-size but different content (not a length-only check)", () => {
    const [a, b] = pair();
    write(a, "a.js", "abcd");
    write(b, "a.js", "abce");
    expect(compareDirs(a, b).changed).toEqual(["a.js"]);
  });

  it("isDirectory distinguishes directories, files and missing paths", () => {
    const [a] = pair();
    write(a, "f.txt", "x");
    expect(isDirectory(a)).toBe(true);
    expect(isDirectory(join(a, "f.txt"))).toBe(false);
    expect(isDirectory(join(root, "nope"))).toBe(false);
  });
});
