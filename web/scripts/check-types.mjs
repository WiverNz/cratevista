// Fails when the committed generated types are stale. Regenerates to a temp file
// and byte-compares. Run: `npm run check:types`.
import { compileFromFile } from "json-schema-to-typescript";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const schemaPath = resolve(
  here,
  "../../crates/cratevista-schema/schema/cratevista-document.schema.json",
);
const committedPath = resolve(
  here,
  "../src/types/generated/explorer-document.ts",
);

const banner = `/**
 * GENERATED FILE — DO NOT EDIT.
 *
 * Produced by \`npm run generate:types\` from
 * crates/cratevista-schema/schema/cratevista-document.schema.json (PRD 02).
 * Regenerate after any schema change; \`npm run check:types\` fails when stale.
 */
/* eslint-disable */`;

const fresh = await compileFromFile(schemaPath, {
  bannerComment: banner,
  additionalProperties: true,
  style: { singleQuote: false, semi: true },
  declareExternallyReferenced: true,
});

let committed = "";
try {
  committed = await readFile(committedPath, "utf8");
} catch {
  console.error(
    `check:types: ${committedPath} is missing. Run \`npm run generate:types\`.`,
  );
  process.exit(1);
}

if (fresh !== committed) {
  console.error(
    "check:types: generated ExplorerDocument types are stale. Run `npm run generate:types` and commit.",
  );
  process.exit(1);
}
console.log("check:types: generated types are current.");
