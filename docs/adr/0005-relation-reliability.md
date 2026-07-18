# ADR 0005 — Relation reliability and cross-crate resolution

- Status: Accepted
- Date: 2026-07-14
- Related: PRD `PRD/issue_05_graph_builder.md`; ADR-0003 (schema versioning);
  ADR-0004 (rustdoc toolchain).

## Context

`cratevista-graph` merges Cargo metadata and normalized rustdoc data into one
`ExplorerDocument`. Relations vary in how reliably their endpoints are known.
Emitting an edge whose target is only *guessed* would mislead readers, so the
graph must classify relations as **reliable**, **approximate (deferred)**, or
**intentionally excluded**, and must never invent an edge.

## Decision

### Reliable relations (emitted)

Emitted only when both endpoints resolve to a known entity id:

- `contains` — workspace→package, package→Cargo target (metadata); target→rustdoc
  root module (cross-source link); module→item, struct/enum/union→impl,
  impl→method/assoc, enum→variant, struct/variant→field (rustdoc).
- `depends_on` — from Cargo metadata (per `normal`/`dev`/`build` role, with a
  BLAKE3 `target_cfg` discriminator; never collapsed).
- `implements` / `implemented_for` — impl→trait / impl→self type (rustdoc,
  intra-crate; cross-crate via structured resolution below).
- `has_field_type` / `accepts_type` / `returns_type` / `error_type` — struct/union
  field or fn param/return/`Result` error → the referenced type, when the target
  resolves **within an analyzed crate**.
- `re_exports` / `imports` — re-export/import site → canonical entity (rustdoc).

### Cross-crate resolution (structured, deterministic)

`cratevista-rustdoc` preserves each unresolved reference as a structured
`UnresolvedTypeRef { from, role: TypeReferenceRole, crate_name, canonical_path,
item_kind, display }` (PRD-04 bridge amendment). The graph resolves them across
the analyzed workspace crates using **only** that structured evidence — never by
re-parsing `display`, never by fuzzy or suffix matching:

1. Require `crate_name` + `canonical_path`; the crate must be an analyzed crate.
2. Look up `(crate, crate-relative path[, item_kind])` in an index built from the
   emitted entities.
3. Outcomes: **exactly one** candidate → emit the role-specific reliable relation;
   **zero** → `unresolved_cross_crate_reference` diagnostic, no edge; **more than
   one** → `ambiguous_cross_crate_reference` diagnostic, no edge.

Role → relation kind: `Field→has_field_type`, `Parameter→accepts_type`,
`Return→returns_type`, `Error→error_type`, `ImplFor→implemented_for`,
`ImplTrait→implements`. `AssociatedType` is **reserved** (no approved relation
kind; produces no edge).

### Approximate relations (deferred)

`references_type` (dyn-trait mentions, generic-argument references, and other
non-nominal textual type mentions) is **deferred** and is **never** emitted by
the graph. Such references remain diagnostics, not edges.

### Excluded relations

- Cross-crate references to **non-analyzed** external crates (std/third-party):
  preserved as `unresolved_cross_crate_reference` diagnostics; no edge.
- Macro-expanded relations and any runtime/call-graph edges: out of scope.

### Invariants

- **No invented edges.** A relation is emitted only when both endpoints exist;
  any relation that would dangle after merging/linking is dropped with a
  `dangling_relation` diagnostic.
- **Determinism.** Resolution keys on stable ids/paths and produces
  order-independent output; equal inputs yield a byte-identical `document.json`.
- Diagnostics are serialized only into `diagnostics.json`, never embedded in the
  document.

## Consequences

- Coverage is traded for correctness: workspace-internal cross-crate references
  resolve exactly; genuinely external ones stay diagnostics.
- Adding `references_type` later is additive and can reuse the same structured
  evidence and index.
