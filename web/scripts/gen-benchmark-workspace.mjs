// Deterministically generates a realistic Rust workspace for the PRD-07
// large-graph benchmark.
//
// This is NOT a pile of isolated synthetic nodes. Each generated crate carries
// the structure a real Rust workspace produces, so the benchmark exercises the
// adapter, the projection and ELK on a representative shape:
//
//   * multiple crates, each depending on the previous one (crate dependencies)
//   * nested public AND private modules (containment)
//   * structs with fields, enums with variants
//   * traits with methods, trait implementations, inherent implementations
//   * free functions and methods
//   * type references across modules and across crates (accepts/returns)
//   * a deliberate mix of documented and undocumented public items
//
// Fully deterministic: no randomness, no clock, no environment input. The same
// arguments always produce byte-identical sources.
//
// Usage: node scripts/gen-benchmark-workspace.mjs <out-dir> <crates> <mods-per-crate> <types-per-mod>
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [outDir, cratesArg, modsArg, typesArg] = process.argv.slice(2);
if (!outDir) {
  console.error(
    "usage: gen-benchmark-workspace.mjs <out-dir> [crates] [mods-per-crate] [types-per-mod]",
  );
  process.exit(1);
}
const CRATES = Number(cratesArg ?? 8);
const MODS = Number(modsArg ?? 4);
const TYPES = Number(typesArg ?? 4);

const crateName = (index) => `bench_c${index}`;
const write = (rel, text) => {
  const path = join(outDir, rel);
  mkdirSync(join(path, ".."), { recursive: true });
  writeFileSync(path, text);
};

/** One module: structs, enums, a trait, impls, methods, and type references. */
function moduleSource(crateIndex, modIndex) {
  const lines = [];
  const isPrivate = modIndex % 3 === 2;
  lines.push(
    isPrivate
      ? `//! Internal module ${modIndex} of crate ${crateIndex}.`
      : `//! Public module ${modIndex} of crate ${crateIndex}.`,
  );
  lines.push("");
  // A trait per module, implemented by each struct below (trait impls).
  lines.push(`/// Behaviour shared by the types in module ${modIndex}.`);
  lines.push(`pub trait Shape${modIndex} {`);
  lines.push("    /// Returns a stable identifier.");
  lines.push("    fn id(&self) -> u32;");
  lines.push("    /// Describes the value.");
  lines.push("    fn describe(&self) -> String {");
  lines.push(`        format!("shape {}", self.id())`);
  lines.push("    }");
  lines.push("}");
  lines.push("");

  for (let t = 0; t < TYPES; t++) {
    const name = `Item${modIndex}_${t}`;
    // Every third public item is deliberately left undocumented, so the
    // documentation-coverage view has something real to measure.
    const documented = t % 3 !== 1;
    if (documented) lines.push(`/// A documented item (${name}).`);
    lines.push(`pub struct ${name} {`);
    lines.push("    /// The numeric identifier.");
    lines.push("    pub id: u32,");
    lines.push("    /// A human-readable label.");
    lines.push("    pub label: String,");
    lines.push("}");
    lines.push("");

    // An enum per type: variants + a type reference to the struct above.
    if (documented) lines.push(`/// The state of a [\`${name}\`].`);
    lines.push(`pub enum State${modIndex}_${t} {`);
    lines.push("    /// Not yet started.");
    lines.push("    Idle,");
    lines.push("    /// Currently running.");
    lines.push("    Running(u32),");
    lines.push("    /// Finished.");
    lines.push("    Done,");
    lines.push("}");
    lines.push("");

    // An inherent impl with methods, including type-reference relations.
    lines.push(`impl ${name} {`);
    lines.push("    /// Creates a new value.");
    lines.push(`    pub fn new(id: u32) -> ${name} {`);
    lines.push(`        ${name} { id, label: String::new() }`);
    lines.push("    }");
    lines.push("    /// Returns the current state.");
    lines.push(`    pub fn state(&self) -> State${modIndex}_${t} {`);
    lines.push(`        State${modIndex}_${t}::Running(self.id)`);
    lines.push("    }");
    lines.push("    /// Accepts another item and merges it.");
    lines.push(`    pub fn merge(&mut self, other: &${name}) {`);
    lines.push("        self.id = self.id.wrapping_add(other.id);");
    lines.push("    }");
    lines.push("}");
    lines.push("");

    // A trait impl.
    lines.push(`impl Shape${modIndex} for ${name} {`);
    lines.push("    fn id(&self) -> u32 {");
    lines.push("        self.id");
    lines.push("    }");
    lines.push("}");
    lines.push("");
  }

  // A free function referencing this module's first type (returns_type).
  lines.push(`/// Builds the first item of module ${modIndex}.`);
  lines.push(`pub fn build_${modIndex}() -> Item${modIndex}_0 {`);
  lines.push(`    Item${modIndex}_0::new(${modIndex})`);
  lines.push("}");
  return lines.join("\n") + "\n";
}

/** The crate root: declares modules and re-uses the previous crate (dep edge). */
function libSource(crateIndex) {
  const lines = [`//! Benchmark crate ${crateIndex} for the CrateVista large-graph benchmark.`, ""];
  for (let m = 0; m < MODS; m++) {
    const isPrivate = m % 3 === 2;
    lines.push(isPrivate ? `mod m${m};` : `pub mod m${m};`);
  }
  lines.push("");
  if (crateIndex > 0) {
    const dep = crateName(crateIndex - 1);
    lines.push(`/// Bridges to the previous crate, creating a cross-crate type reference.`);
    lines.push(`pub fn bridge() -> ${dep}::m0::Item0_0 {`);
    lines.push(`    ${dep}::m0::build_0()`);
    lines.push("}");
    lines.push("");
  }
  lines.push("/// The crate's entry type.");
  lines.push("pub struct Root {");
  lines.push("    /// How many modules this crate exposes.");
  lines.push("    pub modules: u32,");
  lines.push("}");
  lines.push("");
  lines.push("impl Root {");
  lines.push("    /// Creates the root.");
  lines.push("    pub fn new() -> Root {");
  lines.push(`        Root { modules: ${MODS} }`);
  lines.push("    }");
  lines.push("}");
  lines.push("");
  lines.push("impl Default for Root {");
  lines.push("    fn default() -> Root {");
  lines.push("        Root::new()");
  lines.push("    }");
  lines.push("}");
  return lines.join("\n") + "\n";
}

rmSync(outDir, { recursive: true, force: true });

const members = [];
for (let c = 0; c < CRATES; c++) {
  const name = crateName(c);
  members.push(`crates/${name}`);
  const deps = c > 0 ? `${crateName(c - 1)} = { path = "../${crateName(c - 1)}" }\n` : "";
  write(
    `crates/${name}/Cargo.toml`,
    `[package]
name = "${name}"
version.workspace = true
edition.workspace = true
license.workspace = true

[lib]
path = "src/lib.rs"

[dependencies]
${deps}`,
  );
  write(`crates/${name}/src/lib.rs`, libSource(c));
  for (let m = 0; m < MODS; m++) {
    write(`crates/${name}/src/m${m}.rs`, moduleSource(c, m));
  }
}

write(
  "Cargo.toml",
  `# GENERATED by web/scripts/gen-benchmark-workspace.mjs — do not edit by hand.
#
# A self-contained workspace used ONLY to generate the PRD-07 large-graph
# benchmark fixture. It has its own [workspace] table so it is independent of the
# top-level CrateVista workspace, and is never built or tested by the project.
[workspace]
resolver = "2"
members = [${members.map((m) => `"${m}"`).join(", ")}]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
`,
);

console.log(
  `gen-benchmark-workspace — wrote ${CRATES} crates × ${MODS} modules × ${TYPES} types to ${outDir}`,
);
