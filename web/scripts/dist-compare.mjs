// Recursive, byte-exact directory comparison used by check:dist.
// Pure + dependency-free so it can be unit-tested without running a build.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, posix, sep } from "node:path";

/** All file paths under `dir`, relative and POSIX-normalized, sorted. */
export function listFilesRecursive(dir, prefix = "") {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const abs = join(dir, entry.name);
    const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) out.push(...listFilesRecursive(abs, rel));
    else if (entry.isFile()) out.push(rel.split(sep).join(posix.sep));
  }
  return out.sort();
}

/**
 * Compares two directory trees by relative path AND exact bytes.
 * Returns `{ added, removed, changed }` (all sorted, relative paths).
 * `added`   — present in `candidate` but not `baseline`
 * `removed` — present in `baseline` but not `candidate`
 * `changed` — present in both but with differing bytes
 */
export function compareDirs(baseline, candidate) {
  const a = new Set(listFilesRecursive(baseline));
  const b = new Set(listFilesRecursive(candidate));
  const added = [...b].filter((p) => !a.has(p)).sort();
  const removed = [...a].filter((p) => !b.has(p)).sort();
  const changed = [];
  for (const rel of [...a].filter((p) => b.has(p)).sort()) {
    const left = readFileSync(join(baseline, ...rel.split("/")));
    const right = readFileSync(join(candidate, ...rel.split("/")));
    // Byte-exact (Buffer.equals), never text-normalized.
    if (!left.equals(right)) changed.push(rel);
  }
  return { added, removed, changed };
}

/** True when the trees are byte-identical. */
export function isIdentical(diff) {
  return diff.added.length === 0 && diff.removed.length === 0 && diff.changed.length === 0;
}

/** Human-readable drift report. */
export function formatDiff(diff) {
  const lines = [];
  for (const p of diff.removed) lines.push(`  - removed: ${p}`);
  for (const p of diff.added) lines.push(`  + added:   ${p}`);
  for (const p of diff.changed) lines.push(`  ~ changed: ${p}`);
  return lines.join("\n");
}

/** Exists + is a directory. */
export function isDirectory(path) {
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}
