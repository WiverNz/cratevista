// Deterministic web dependency-licence report (PRD 10, Phase 6.K).
//
// The committed `web/package-lock.json` is the dependency authority; each package's
// installed `package.json` (under `web/node_modules`) supplies its declared licence.
// No large runtime dependency is added — Node's built-ins plus the lockfile and
// installed metadata provide everything. Output is deterministic (sorted, no
// timestamps, no absolute paths).
//
// Usage:
//   node scripts/license-report.mjs            # write docs/licenses/web-dependencies.{json,md} + policy check
//   node scripts/license-report.mjs --check    # regenerate in memory, fail on drift + policy
//   node scripts/license-report.mjs --discover  # print the licence histogram only (no policy failure)
//
// Policy: fail on any missing, unknown, unparseable or disallowed SPDX licence.
// Licences are matched by SPDX id — never inferred from a package name — and an
// unparseable compound expression is a hard failure, never silently accepted.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const webRoot = resolve(here, "..");
const repoRoot = resolve(webRoot, "..");
const licensesDir = join(repoRoot, "docs", "licenses");

// Accepted SPDX identifiers. Permissive licences plus a small set of recorded
// exceptions (weak file-level copyleft and data licences), each justified in
// docs/licenses/README.md with the exact package and rationale. Extend only with
// review — never to paper over an unexpected licence.
const ACCEPTED = new Set([
  // Permissive.
  "MIT",
  "MIT-0",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "0BSD",
  "CC0-1.0",
  "Zlib",
  "Unlicense",
  "Python-2.0",
  "BlueOak-1.0.0",
  "Unicode-DFS-2016",
  "Unicode-3.0",
  "WTFPL",
  // Recorded exceptions (see docs/licenses/README.md):
  "MPL-2.0", //  weak file-level copyleft; dev/build tools (lightningcss, axe-core).
  "EPL-2.0", //  weak file-level copyleft; runtime dep elkjs, used unmodified.
  "CC-BY-4.0", // data (caniuse-lite); dev only.
]);

/** Reads a JSON file. */
function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

/** Normalizes a package.json `license`/`licenses` field into an SPDX string. */
function licenseOf(pkgJson) {
  if (typeof pkgJson.license === "string") return pkgJson.license;
  if (pkgJson.license && typeof pkgJson.license === "object" && pkgJson.license.type) {
    return String(pkgJson.license.type);
  }
  if (Array.isArray(pkgJson.licenses)) {
    const ids = pkgJson.licenses.map((l) => (typeof l === "string" ? l : l?.type)).filter(Boolean);
    if (ids.length > 0) return `(${ids.join(" OR ")})`;
  }
  return "UNKNOWN";
}

/**
 * Evaluates a (small) SPDX expression against ACCEPTED. Returns "accepted",
 * "disallowed", or "unparseable". Supports OR/AND and parentheses and `WITH`.
 */
function evaluate(expr) {
  if (!expr || expr === "UNKNOWN") return "unparseable";
  // Strip surrounding parens and normalize spaces.
  let e = expr.trim();
  while (e.startsWith("(") && e.endsWith(")")) e = e.slice(1, -1).trim();

  // A bare id (optionally `id WITH exception`).
  const bare = e.split(/\s+WITH\s+/i)[0].trim();
  if (!/[()]/.test(e) && !/\s+(OR|AND)\s+/i.test(e)) {
    return ACCEPTED.has(bare) ? "accepted" : "disallowed";
  }
  // OR: accepted if ANY operand accepted. AND: accepted only if ALL accepted.
  // Parentheses are not nested in practice for these deps; a nested/unparseable
  // shape returns "unparseable" rather than being silently accepted.
  if (/[()]/.test(e)) return "unparseable";
  if (/\s+OR\s+/i.test(e)) {
    const parts = e.split(/\s+OR\s+/i).map((p) => evaluate(p));
    if (parts.includes("unparseable")) return "unparseable";
    return parts.includes("accepted") ? "accepted" : "disallowed";
  }
  if (/\s+AND\s+/i.test(e)) {
    const parts = e.split(/\s+AND\s+/i).map((p) => evaluate(p));
    if (parts.includes("unparseable")) return "unparseable";
    return parts.every((p) => p === "accepted") ? "accepted" : "disallowed";
  }
  return "unparseable";
}

/** Builds the sorted package list from the lockfile + installed metadata. */
function collect() {
  const lock = readJson(join(webRoot, "package-lock.json"));
  const packages = [];
  for (const [path, entry] of Object.entries(lock.packages ?? {})) {
    if (path === "") continue; // the root project
    const idx = path.lastIndexOf("node_modules/");
    if (idx === -1) continue;
    const name = path.slice(idx + "node_modules/".length);
    let license = entry.license;
    if (!license) {
      try {
        license = licenseOf(readJson(join(webRoot, path, "package.json")));
      } catch {
        license = "UNKNOWN";
      }
    }
    packages.push({
      name,
      version: entry.version ?? "0.0.0",
      license: typeof license === "string" ? license : String(license),
      scope: entry.dev === true ? "dev" : "runtime",
    });
  }
  packages.sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
  return packages;
}

function build() {
  const packages = collect();
  const violations = [];
  const byLicense = {};
  for (const p of packages) {
    const verdict = evaluate(p.license);
    byLicense[p.license] = (byLicense[p.license] ?? 0) + 1;
    if (verdict !== "accepted") {
      violations.push({ ...p, verdict });
    }
  }
  const summary = Object.fromEntries(Object.entries(byLicense).sort(([a], [b]) => a.localeCompare(b)));
  const json = {
    policy: { accepted: [...ACCEPTED].sort() },
    counts: { total: packages.length },
    byLicense: summary,
    packages,
  };
  const md = renderMarkdown(json);
  return { json, md, violations };
}

function renderMarkdown({ counts, byLicense, packages }) {
  const lines = [];
  lines.push("# Web dependency licenses");
  lines.push("");
  lines.push("<!-- Generated by web/scripts/license-report.mjs (PRD 10, Phase 6.K).");
  lines.push("     Deterministic, path/timestamp-free. Do not edit by hand — run");
  lines.push("     `npm run license:web` and commit, or CI's drift check fails. -->");
  lines.push("");
  lines.push(`Total packages: **${counts.total}** (from the committed \`package-lock.json\`).`);
  lines.push("");
  lines.push("## Licences in use");
  lines.push("");
  lines.push("| SPDX | count |");
  lines.push("| --- | --- |");
  for (const [license, count] of Object.entries(byLicense)) {
    lines.push(`| ${license} | ${count} |`);
  }
  lines.push("");
  lines.push("## Packages");
  lines.push("");
  lines.push("| package | version | licence | scope |");
  lines.push("| --- | --- | --- | --- |");
  for (const p of packages) {
    lines.push(`| ${p.name} | ${p.version} | ${p.license} | ${p.scope} |`);
  }
  lines.push("");
  return lines.join("\n");
}

const mode = process.argv[2] ?? "write";
const { json, md, violations } = build();

if (mode === "--discover") {
  console.log(JSON.stringify(json.byLicense, null, 2));
  process.exit(0);
}

const jsonPath = join(licensesDir, "web-dependencies.json");
const mdPath = join(licensesDir, "web-dependencies.md");
const jsonText = `${JSON.stringify(json, null, 2)}\n`;

if (mode === "--check") {
  const currentJson = readFileSync(jsonPath, "utf8");
  const currentMd = readFileSync(mdPath, "utf8");
  let drift = false;
  if (currentJson !== jsonText) {
    console.error("license:web: docs/licenses/web-dependencies.json is STALE — run `npm run license:web`.");
    drift = true;
  }
  if (currentMd !== `${md}\n`) {
    console.error("license:web: docs/licenses/web-dependencies.md is STALE — run `npm run license:web`.");
    drift = true;
  }
  reportViolations(violations);
  process.exit(drift || violations.length > 0 ? 1 : 0);
}

// write mode
writeFileSync(jsonPath, jsonText);
writeFileSync(mdPath, `${md}\n`);
console.log(`license:web: wrote ${json.counts.total} packages to docs/licenses/web-dependencies.{json,md}`);
reportViolations(violations);
process.exit(violations.length > 0 ? 1 : 0);

function reportViolations(list) {
  if (list.length === 0) return;
  console.error(`license:web: ${list.length} licence policy violation(s):`);
  for (const v of list) {
    console.error(`  - ${v.name}@${v.version}: "${v.license}" (${v.verdict})`);
  }
}
