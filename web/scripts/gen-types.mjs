// Generates committed TypeScript types for `ExplorerDocument` from the checked-in
// JSON Schema. Run: `npm run generate:types`. The output is committed and MUST
// NOT be edited by hand. Only `ExplorerDocument` is schematized (PRD 02);
// GenerationReport/DiagnosticsReport are hand-written under src/types/.
import { compileFromFile } from "json-schema-to-typescript";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const schemaPath = resolve(
  here,
  "../../crates/cratevista-schema/schema/cratevista-document.schema.json",
);
const outPath = resolve(here, "../src/types/generated/explorer-document.ts");

const banner = `/**
 * GENERATED FILE — DO NOT EDIT.
 *
 * Produced by \`npm run generate:types\` from
 * crates/cratevista-schema/schema/cratevista-document.schema.json (PRD 02).
 * Regenerate after any schema change; \`npm run check:types\` fails when stale.
 */
/* eslint-disable */`;

const body = await compileFromFile(schemaPath, {
  bannerComment: banner,
  additionalProperties: true, // forward-compatible: unknown fields tolerated
  style: { singleQuote: false, semi: true },
  declareExternallyReferenced: true,
});

await mkdir(dirname(outPath), { recursive: true });
await writeFile(outPath, body, "utf8");
console.log(`wrote ${outPath}`);
