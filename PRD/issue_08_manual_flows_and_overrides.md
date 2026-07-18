# PRD — Add manual architecture flows and presentation overrides

## Status

**Implemented / Verified.** Completed on 2026-07-16. All 13 acceptance criteria are met, amendments A–C and steps 0–7 have landed with per-step ledgers below, and the full workspace gates pass (376 tests). The three previously blocking decisions were **locked** before implementation (see "## Open questions") and their consequences are specified in "## Required amendments to PRDs 02 / 05 / 07":

- **A (PRD 02):** typed optional `View::docs`/`View::examples` + `ViewExample`; **`SchemaVersion` 1.0 → 1.1** (additive); example contents **embedded**, so they render without `/api/source`.
- **B (PRD 05):** `EntityOverride::docs`, appending to — never replacing — discovered documentation, and never touching `documented`/coverage.
- **C (PRD 07):** a small rendering increment for flow docs/examples/description.

No open questions remain. Implement in the order given by "## Implementation sequence": amendments A → B → C first, each green on its own, then the `cratevista-config` crate. **Amendments A/B/C do not change the Implemented / Verified status of PRDs 02/05/07**; each is an additive amendment recorded in its own PRD when it lands, following the PRD-06 CSP/build-correctness precedent.

See "## Manual-content model" for the flow/override/localization decisions.

## Source issue

`ISSUES/issue_08_manual_flows_and_overrides.md`

## Summary

Implement `cratevista-config`: load, validate, and merge project-local CrateVista configuration (TOML) that adds **manual entities** (clients, databases, brokers, external APIs, infra), **flows** (curated architecture/runtime views mixing discovered and manual entities), and **overrides** (presentation enrichment of discovered entities without changing their identity). Produces the **`cratevista_graph::GraphOverlay`** consumed by the graph builder (issue 05). Configuration errors report file + location + actionable detail and never crash the server.

## Problem statement

rustdoc/Cargo cannot infer business/runtime architecture (external systems, request flows, edge labels like HTTP/SQL/Redis). Maintainers need a reviewable, comment-friendly, localization-ready way to enrich the generated document — for example the pattern Clients → Gateway → Services → Infrastructure.

## Goals

- A documented config convention (format + locations) with complete examples.
- Manual entities, flows (stages, ordered steps, selected discovered entities by stable ref, manual entities, relations, edge labels, default focus, output examples, doc blocks), and overrides (label/category/tags/description/stage/hidden/promoted/extra docs/presentation hints).
- Validation with file/line/column, invalid-reference, duplicate-id, unsupported-kind, missing-field, type-mismatch reporting; broken references degrade gracefully.
- Deterministic loading; documented merge precedence; localization-ready labels.

## Non-goals

- UI for authoring flows (issue 07 only renders them).
- Full UI translation (localization-ready data only).
- Changing discovered semantic identity (overrides are presentation-only).

## Current repository state

Verified against the repository on 2026-07-16, with **PRDs 01–08 all Implemented / Verified**.

- **`cratevista-config` exists and this PRD is fully implemented — amendments A–C and steps 0–7 have all landed** (2026-07-16). It never had a placeholder; this PRD created it and registered it in the root `Cargo.toml` `[workspace] members`. Implemented: `error`, `model`, `discover`, `load`, `validate`, `overlay`, `docs`, plus the `load_config` entry point. **It is wired into generation**: `cargo tree --workspace --invert cratevista-config` shows `cratevista-config ← cratevista-core ← cargo-cratevista` — one consumer chain, with `cratevista-graph` absent from it. `cargo cratevista generate`/`open` apply configuration by default and accept `--no-config`. The three fixture sets are committed under `crates/cratevista-config/fixtures/`. **The user documentation shipped in step 7: `docs/configuration.md` + `docs/adr/0007-config-format-toml.md`.**
- **The seam is `cratevista_graph::GraphOverlay`, not `ConfigOverlay`** (that type does not exist anywhere). Its real shape (`crates/cratevista-graph/src/input.rs`):

  ```rust
  pub struct GraphOverlay {
      pub entities: Vec<Entity>,                          // manual additions
      pub relations: Vec<Relation>,                       // manual additions
      pub overrides: BTreeMap<EntityId, EntityOverride>,  // presentation-only
      pub views: Vec<View>,                               // manual flow views
  }
  pub struct EntityOverride {
      pub label: Option<LocalizedText>,        // replaces
      pub description: Option<LocalizedText>,  // replaces
      pub add_tags: Vec<String>,
      pub set_attributes: BTreeMap<String, AttrValue>,
      pub hidden: Option<bool>,
      pub docs: Option<DocBlock>,              // APPENDS — landed with Amendment B
  }
  ```

- **Issue-05 already performs the referential sanitation** this PRD must therefore *not* duplicate (`crates/cratevista-graph/src/{overlay,validate}.rs`, all non-fatal warnings on the document's `diagnostics[]`):
  - `overlay::apply_overlay` forces `Provenance::Manual` on manual entities/relations, applies overrides, and emits `overlay_target_missing` when an override targets an unknown entity;
  - `validate::drop_dangling_relations` drops relations (including manual ones) whose endpoints are missing, emitting `dangling_relation`;
  - `validate::sanitize_views` drops unknown `View::entity_ids` members and an unknown `default_focus`, emitting `invalid_view_reference`.
- **Kinds are open newtypes** — `EntityKind(String)` / `RelationKind(String)` (`crates/cratevista-schema/src/kind.rs`). There are **no** `ExternalSystem`/`Infrastructure` variants and no `RelationKind::manual` variant, and none are needed: `external_system`, `infrastructure`, `manual_block`, `manual` are plain strings. The PRD-07 UI renders unknown kinds through its generic fallback and includes them in the legend.
- **`cratevista-schema` provides:** `Provenance::{Discovered, Manual}`; `LocalizedText`; `View { id, title, description, entity_kinds, relation_kinds, entity_ids: Option<Vec<EntityId>>, stages, default_focus, presentation, docs: Option<DocBlock>, examples: Vec<ViewExample> }` — `docs`/`examples` landed with **Amendment A** (schema `1.1`); `ViewExample { id, title, language: Option<String>, content, description: Option<LocalizedText> }`; `Stage { id, title, order }`; `Relation { .., label: Option<LocalizedText>, attributes }`; `Entity { .., docs: Option<DocBlock>, tags, attributes, description }`; and `source::RepoRelativePath` (traversal-safe path validation).
- **`View` has no `provenance` field** — only `Entity`/`Relation` do.
- **`init` (issue 01)** already writes `cratevista.toml` whose comments reserve `[metadata]`/`[rustdoc]`/`[server]` as "bound in later releases" and point at `.cratevista/flows/*.toml` and `docs/configuration.md`.
- **`docs/configuration.md`** (CLAUDE.md designates it the user configuration reference) **was created by this PRD** (step 7), together with `docs/adr/0007-config-format-toml.md`. Every TOML example in it is verified against the shipped parser.
- **`--no-config`** **was added by this PRD** (step 6) to `generate` and `open`; it is absent from `serve`, which replays existing artifacts.
- **`toml`/`serde_spanned` added to `[workspace.dependencies]`** (landed with steps 0-2; resolved `toml` 1.1.3, `serde_spanned` 1.1.1). **`toml_edit` was deliberately not added** — it is a format-preserving *editor* and this crate only reads; see the steps 0-2 ledger.
- Stable ids (issue 02) let overrides reference discovered entities.

## Required amendments to PRDs 02 / 05 / 07

Three decisions were locked on 2026-07-16. Each requires a small, **additive**
amendment to an already **Implemented / Verified** PRD. They are listed first
because they must land **before** `cratevista-config` can produce a complete
overlay, and each is independently verifiable.

**None of these amendments changes existing behaviour.** Every new field is
optional; every existing document, fixture and snapshot stays valid.

### Amendment A — PRD 02: typed flow docs/examples on `View` (+ `SchemaVersion` 1.0 → 1.1)

> **LANDED 2026-07-16 — Implemented / Verified.** PRD 02 stays Implemented / Verified (see its "### Additive amendment (2026-07-16)"); PRD 08 stays **Approved**. Amendments B and C, and `cratevista-config`, are **not** started.
>
> **Shipped:** `View::docs: Option<DocBlock>` + `View::examples: Vec<ViewExample>` + the `ViewExample` type (`crates/cratevista-schema/src/view.rs`, re-exported from `lib.rs`); `SchemaVersion::CURRENT` `"1.0"` → `"1.1"`; the checked-in JSON Schema and `web/src/types/generated/explorer-document.ts` regenerated and committed; the four `View` literals in `crates/cratevista-graph/src/views.rs` and `crates/cratevista-schema/examples/gen_fixtures.rs` updated for the new fields.
>
> **Tests added (9):** `crates/cratevista-schema/tests/view_docs_examples.rs` (7) — `CURRENT` is 1.1 and still major 1; round-trip with docs+examples; `ViewExample` round-trip; absent fields are omitted from the JSON; **a 1.0 document still deserializes and validates**; canonical determinism (8 repeats + parse→re-serialize byte equality); example order is preserved, not sorted. `crates/cratevista-server/src/snapshot.rs` (1) — `older_matching_minor_version_snapshot_still_loads` (a 1.0/1.0 pair loads). `crates/cratevista-server/tests/e2e_fixtures.rs` (1) — the five **real committed 1.0 snapshots** still load through the real loader.
>
> **Behaviour confirmed unchanged:** major-version rejection (`2.x` → `schema_version_unsupported`) and mixed-version rejection (`schema_version_mismatch`). **No fixture regeneration was forced**; the committed schema fixtures and all five snapshots remain 1.0 and are now the backward-compatibility evidence. **`web/dist` is untouched** (`check:dist` green) — types are erased at compile time, so no Amendment C work leaked in.
>
> **Latent trap fixed:** four tests hard-coded the literal `"1.0"` (three in `snapshot.rs`, one in `router.rs`). The bump made the `snapshot.rs` rewrites silent no-ops; all four now anchor on `SchemaVersion::CURRENT`, and the rewrite helper asserts the marker is present so the next bump cannot void them silently.
>
> **Gates:** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features` (**248 passed**, up from 239), `cargo +1.97.0 check --workspace --all-features`; `npm run generate:types`, `check:types`, `typecheck`, `typecheck:compat`, `lint`, `test` (**176 passed**), `check:dist`; `npm run e2e` (**70 passed, 0 skipped**) against a binary rebuilt after the schema change. All exit 0.

`crates/cratevista-schema/src/view.rs`:

```rust
pub struct View {
    // … existing fields unchanged …
    /// Flow-level documentation (Markdown), rendered by the explorer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<DocBlock>,
    /// Worked examples, with their contents EMBEDDED in the document.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<ViewExample>,
}

/// A worked example attached to a view. Contents are embedded at generation
/// time, so the explorer renders them without `/api/source` and a static export
/// (issue 10) is self-contained.
pub struct ViewExample {
    /// Stable identifier, unique within the view.
    pub id: String,
    /// Display title (localization-ready).
    pub title: LocalizedText,
    /// Syntax hint for highlighting/formatting (e.g. `json`, `http`, `sql`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// The example content itself, embedded verbatim (UTF-8).
    pub content: String,
    /// Optional prose about the example.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<LocalizedText>,
}
```

**`SchemaVersion::CURRENT` becomes `"1.1"`** (`crates/cratevista-schema/src/version.rs`), plus its doc comment.

Why the bump is safe — verified against the code, not assumed:

- The **server** gates on the **major** only (`require_supported_major`), so `1.1` is accepted. It additionally requires `document.schema_version == diagnostics.schema_version` (exact string equality), and both derive from `SchemaVersion::CURRENT`, so a generator run always emits a matching pair.
- **Committed 1.0 snapshots keep loading**: major is 1 and their document/diagnostics versions are equal to each other. No fixture regeneration is forced.
- The **frontend** gates on the major only (`SUPPORTED_SCHEMA_MAJOR = 1` in `web/src/api/load.ts`), so `1.1` loads and an old `1.0` document still loads.
- The committed **JSON Schema** types `SchemaVersion` as a plain `{"type": "string"}` — no `const`, no `pattern` — so neither `1.1` nor existing `1.0` fixtures break ajv validation, and the new fields are optional.
- Per **ADR-0003**, an additive change is a **minor** bump.

Amendment-A work and tests:

- Regenerate the committed JSON Schema (`cargo run -p cratevista-schema --example gen_schema`); `crates/cratevista-schema/tests/jsonschema_drift.rs` proves it is not stale.
- Regenerate the frontend types (`npm run generate:types`) and commit them; `npm run check:types` proves they are not stale.
- Update `crates/cratevista-server/src/router.rs` (~line 209), whose health test asserts `body["schema_version"] == "1.0"` — assert `SchemaVersion::CURRENT` instead of a literal, so the next bump cannot silently rot it.
- New schema tests: `View` round-trips with and without `docs`/`examples`; a `1.0` document (no new fields) still deserializes and validates; `ViewExample` round-trips; canonical-serialization determinism holds.
- New server test: a **1.0** snapshot still loads (regression), alongside the existing 2.0-unsupported and mixed-version tests.

### Amendment B — PRD 05: `EntityOverride::docs` with append semantics

> **LANDED 2026-07-16 — Implemented / Verified.** PRD 05 stays Implemented / Verified (see its "### Additive amendment (2026-07-16)"); PRD 08 stays **Approved**. Amendment C and `cratevista-config` are **not** started.
>
> **Shipped:** `EntityOverride::docs: Option<DocBlock>` (`crates/cratevista-graph/src/input.rs`); `append_docs` + `join_markdown` in `crates/cratevista-graph/src/overlay.rs`, applied inside `apply_overlay` after the existing override fields.
>
> **Markdown boundary (exact):** only newline characters *immediately adjoining the junction* are normalized — the discovered text's trailing newlines and the manual text's leading newlines — then the two are joined with `\n\n`, giving exactly one blank line. `\r` is trimmed alongside `\n` at the junction so a CRLF-terminated discovered block cannot leave a stray carriage return mid-document. **Nothing else is touched:** indentation, trailing spaces, blank lines *inside* either side, and the manual text's **own trailing newline** all survive byte-for-byte. An empty/newline-only manual block is a **no-op** — the discovered docs are not even rewritten to an identical value.
>
> **Coverage safety:** `documented` is never written by an override, so `compute_coverage` (which reads `docs.map(|d| d.documented).unwrap_or(false)`) is invariant **by construction** — for a `None → Some` transition the value goes from `unwrap_or(false)` to a stored `false`, which is the same input.
>
> **Tests added (10):** `overlay.rs` (9) — appended after discovered with the discovered `summary` preserved; exactly one blank line across 8 adjoining-newline combinations incl. CRLF, with no stray `\r`; internal content never rewritten; manual docs on an undocumented entity stay `documented: false` with `summary: None`; an override never flips `documented` (both directions); an empty manual block leaves discovered docs byte-identical (4 empty spellings); a docs override leaves label/description alone; label/description still **replace**; appending is deterministic (8 repeats). `coverage.rs` (1) — `docs_only_overrides_never_change_coverage` runs the real `apply_overlay` → `compute_coverage` order against a dishonest `documented: true` manual block on both a documented and an undocumented item, with the baseline **pinned at `Some(50)`** so it cannot pass vacuously (two `None`s would compare equal), and asserts the prose *did* land.
>
> **Gates:** `cargo test -p cratevista-graph` (**41 passed**, 1 pre-existing ignored); `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features` (**258 passed**, up from 248 — the 10 new tests), `cargo +1.97.0 check --workspace --all-features`. All exit 0.

`crates/cratevista-graph/src/input.rs`:

```rust
pub struct EntityOverride {
    // … existing fields unchanged …
    /// Manual documentation appended to whatever was discovered.
    pub docs: Option<DocBlock>,
}
```

`crates/cratevista-graph/src/overlay.rs` — **deterministic merge that preserves discovered documentation**:

| Case | Result |
| --- | --- |
| discovered `docs = Some(d)`, override `docs = Some(m)` | `markdown = format!("{}\n\n{}", d.markdown, m.markdown)` — **discovered first, manual appended**; `summary` keeps `d.summary`; `documented` keeps `d.documented` |
| discovered `docs = None`, override `docs = Some(m)` | `Some(DocBlock { markdown: m.markdown, summary: None, documented: false })` |
| override `docs = None` | discovered `docs` untouched |

Two rules that must not be "tidied away" by an implementer:

- **`documented` is never changed by an override.** It feeds
  `coverage::compute_coverage`, which measures *Rust* documentation coverage.
  Letting configuration set it would let a project fake its coverage number.
  Manual enrichment of an undocumented item therefore leaves it counted as
  undocumented.
- **`docs` appends, while `label`/`description` replace.** That asymmetry is
  deliberate and locked: documentation is additive enrichment; a label is a
  substitution. The separator is exactly one blank line, so output is
  byte-deterministic.

Ordering is already deterministic: `GraphOverlay::overrides` is a
`BTreeMap<EntityId, _>`, iterated in sorted id order, with at most one override
per id (config resolves duplicates first — FR6).

Amendment-B tests (`crates/cratevista-graph`): append preserves discovered
markdown and prepends it; `summary`/`documented` survive; the `None` +
override case yields `documented: false`; coverage is unchanged by a
docs-only override; determinism across repeated builds.

### Amendment C — PRD 07: render flow docs/examples (a small increment)

> **LANDED 2026-07-16 — Implemented / Verified.** PRD 07 stays Implemented / Verified (see its "### Additive increment (2026-07-16)"); PRD 08 stays **Approved**. **All three amendments (A, B, C) are now landed**; `cratevista-config` itself is **not** started.
>
> **Shipped:** `web/src/components/ViewDocs.tsx`, mounted in the inspector column of `web/src/App.tsx`; styles in `web/src/styles.css`. Renders the active view's `description`, its `docs` through the existing `SafeMarkdown` pipeline (no new dependency), and its `examples` as native `<details>`/`<summary>` disclosures with title, language-as-text, optional description and embedded `<pre><code>` content. Returns `null` when the view has none of these, so the eight generated views are visually unchanged. **No `/api/source` request** is involved — contents are embedded.
>
> **The `flow` fixture.** The browser test needs a served document carrying schema-1.1 docs/examples. When Amendment C landed, their producer (`cratevista-config`) did not exist yet and `generate` could not emit them — steps 0–6 have since shipped it, and `generate` now emits exactly this shape from `.cratevista/flows/*.toml`. `crates/cratevista-core/examples/gen_flow_fixture.rs` therefore **synthesizes** `web/e2e/fixtures/flow/` (4 manual entities, 3 labelled relations, one flow view with docs + 2 examples, 6 KB) — but **not** as hand-written JSON: it is committed through the **production writer** (`commit_artifacts`), so it is schema-validated with correct BLAKE3 hashes like every other snapshot, and it models what a `.cratevista/flows/*.toml` will produce. It is the **only committed 1.1 artifact**; the other six remain 1.0 and are the back-compat evidence.
>
> **Tests added (13):** 9 component (`web/tests/view-docs.test.tsx`) — description; docs as sanitized Markdown; examples with title/language/description/content; disclosure + summary focus; the empty and whitespace-only cases render nothing; **3 hostile-content tests** (scripts/`onerror`/`javascript:`/`iframe` stripped and nothing executed; hostile example content rendered as text with no elements created; example content not parsed as Markdown). 3 real-browser (`web/e2e/tests/view-docs.spec.ts`) — renders under the real server + CSP with **zero `/api/source` requests**, zero CSP violations and zero page errors; **keyboard-only disclosure** (Enter opens and closes); the generated views render no panel. 1 Rust (`e2e_fixtures.rs`) — the `flow` fixture is 1.1 and carries docs/examples/membership/focus.
>
> **jsdom gap, handled honestly:** jsdom implements `<details>` toggling on **click** but not **Enter-on-summary**, so keyboard activation is asserted in the real browser rather than faked in the component test — which is exactly what the real-browser requirement is for.
>
> **Gates:** frontend `generate:types`, `check:types`, `typecheck`, `typecheck:compat`, `lint`, `test` (**185 passed**, up from 176), `check:dist`, `build`; `npm run e2e` (**73 passed, 0 skipped**, up from 70) against a binary rebuilt after the dist; `check:embed-rebuild` (4/4 PASS); `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features` (**259 passed**, up from 258), `cargo +1.97.0 check --workspace --all-features`. All exit 0. **`web/dist` rebuilt and committed.**

The completed UI renders neither `View::description` nor view-level docs, so the
schema addition needs a renderer or it is dead data.

- Render `View::docs` for the active view through the **existing**
  `SafeMarkdown` pipeline (`react-markdown` + `remark-gfm` + `rehype-sanitize`) —
  no new Markdown dependency, no `rehype-raw`, no `dangerouslySetInnerHTML`.
- Render `View::examples` as titled, collapsible blocks with the embedded
  `content` in a `<pre><code>`, using `language` only as a display hint. **No
  `/api/source` request** is involved.
- Also surface `View::description` (currently unrendered) in the same panel.

Amendment-C work and tests: component tests for docs rendering, example
rendering, sanitization (a hostile `content`/`docs` string must not inject HTML),
and the empty case (no docs/examples ⇒ nothing rendered, so the eight generated
views are visually unchanged); one real-browser assertion that a flow's docs and
example are visible and that no CSP violation occurs. Because `web/src` changes,
the increment must rebuild and commit `web/dist` and pass `check:dist` and
`check:embed-rebuild`.

## Terminology

Per CONTEXT: **Manual entity**, **Flow**, **Override**, **Stage**. **Stable reference**: a discovered entity's stable id (issue 02) used to point at it from config.

## User-visible behavior

Config discovered automatically at workspace root. Canonical MVP locations:

```
cratevista.toml                 # tool config ([metadata]/[rustdoc]/[server]/[watch]/[cache])
.cratevista/
  flows/*.toml                  # one flow per file; a flow file may also declare the manual [[entity]]s it uses
  overrides/*.toml              # presentation overrides of discovered entities
  docs/*.md                     # manual documentation blocks referenced by id
```

- Present config → its overlay is merged into the generated document; manual+discovered entities coexist in views/flows.
- Invalid config → generation still succeeds for the valid parts; diagnostics list precise errors; server keeps serving the last valid document.

## Functional requirements

1. **Format decision:** TOML for MVP, in the three canonical locations above (`cratevista.toml`, `.cratevista/flows/*.toml`, `.cratevista/overrides/*.toml`). **Dependency honesty:** TOML is not zero-cost — it requires adding the `toml` crate for parsing and `serde_spanned` for source spans (line/column diagnostics). *(Landed: `toml_edit` proved unnecessary — see the steps 0-2 ledger.)* This is a deliberate dependency choice: TOML is the Cargo-ecosystem-native configuration format (consistent with `Cargo.toml`), has first-class comment and reviewable-diff support, and its Rust tooling (`toml`, `serde_spanned`) gives reliable span information for diagnostics. The alternative, YAML, would instead require a YAML crate (e.g. `serde_yaml`, which is unmaintained) or another dependency, with weaker ecosystem fit; so the trade is "add `toml`/`serde_spanned`" vs "add a YAML crate", and TOML wins on ecosystem fit and maintenance. (Issue examples show YAML; this PRD selects TOML.)
2. Discovery: load `cratevista.toml` + `.cratevista/**` deterministically (sorted file order); absence = empty overlay.
3. **Manual entities** (locked decision 3): declared **inside `.cratevista/flows/*.toml`** via `[[entity]]` for the MVP — a dedicated `.cratevista/entities/*.toml` location can be added additively later without breaking these files. Fields: id, kind (`external_system`/`infrastructure`/`manual_block`/… — any string, kinds are open), label (localized), description (localized), tags, attributes, source (optional, `RepoRelativePath`-validated).
   - **Ids are globally unique across the complete config set**, not per file. Two `[[entity]]` blocks sharing an id — in the same file or different files — are a **duplicate-id error** naming both locations, and neither is silently dropped.
   - A config-local `id = "redis"` becomes the entity id **`manual:redis`**.
   - **A manual entity may be referenced from any flow file**, not just the one declaring it. The validator therefore builds the global manual-id set across all flow files (in sorted order) *before* checking references, so resolution never depends on file order.
   - **References always use the full entity id** — `manual:redis` for manual entities, the discovered stable id (e.g. `item:struct:cvcore::model::Widget`) for discovered ones. One namespace, no bare-id resolution, so a member/endpoint can never be ambiguous or change meaning as the config set grows.
4. **Flows** (`.cratevista/flows/*.toml`) → schema `View`: id, localized title/description, ordered `stages`, membership via **`View::entity_ids`** (discovered entity refs by stable id + manual entity ids), manual relations, `default_focus`. **`View` has no `provenance` field**, so a flow view is not marked manual on the view itself; its manual character is carried by its member entities' `Provenance::Manual` and, if a marker is wanted, a `presentation` key. Refs to unknown ids → `invalid_view_reference` warning and the member is dropped, flow still built — **already implemented by issue-05 `sanitize_views`; this PRD must not reimplement it.**
   - **Edge labels** use the first-class **`Relation::label: Option<LocalizedText>`** (HTTP/WebSocket/SQL/Redis/…), **not** attributes: the PRD-07 adapter renders `relation.label` (falling back to the kind) and ignores label-ish attributes. `Relation::attributes` remains available for non-label metadata.
   - **`role` is part of a relation's identity, not decoration.** `RelationId` is `kind + from + to` (+ an optional role), so two edges between the same pair — a request and a response, or HTTP and WebSocket — need distinct `role`s or they derive one id and the graph's `merge_relation` keeps only the first (reporting `conflicting_relation_evidence`, which would not tell the author what to fix). `[[flow.relation]]` therefore accepts `role`, mapped via `RelationId::with_role`, and config diagnoses a duplicate derived id itself (`config_duplicate_relation`) with actionable guidance. The canonical `manual_flow.document.json` fixture already uses this shape (`role = "http"`, `label = "HTTP + WS"`).
   - **Flow doc blocks** → **`View::docs: Option<DocBlock>`** (Amendment A), authored inline or by referencing `.cratevista/docs/*.md`; config **reads the file and embeds its Markdown** into the document.
   - **Output-data examples** → **`View::examples: Vec<ViewExample>`** (Amendment A). Config resolves each referenced path via `RepoRelativePath`, **reads the file and embeds its contents verbatim** into the document. The explorer therefore renders examples **without `/api/source`**, and a static export (issue 10) is self-contained. `language` is a display hint only.
     - **Bounded:** an example larger than **64 KiB** is dropped with an `example_too_large` diagnostic; non-UTF-8 content is dropped with `example_not_utf8`; a missing file yields `missing_doc_file`. The cap is deliberately far below `/api/source`'s 1 MiB, because embedded content is shipped on **every** `/api/document` fetch rather than on demand.
5. **Overrides** (`.cratevista/overrides/*.toml`, keyed by discovered stable id) map onto the **real `EntityOverride`** fields:

   | Config key | Maps to | Notes |
   | --- | --- | --- |
   | `label` | `EntityOverride::label` | localized |
   | `description` | `EntityOverride::description` | localized |
   | `tags` | `EntityOverride::add_tags` | unioned, sorted, deduped (additive only — no tag removal) |
   | `hidden` | `EntityOverride::hidden` | issue-05 writes it to `attributes["hidden"]` |
   | `category`, `stage`, `promoted`, `presentation` | `EntityOverride::set_attributes` | **`stage` is genuinely an attribute**: the PRD-07 UI reads entity `attributes["stage"]` and only shows stage lanes when the active view defines `stages` |
   | `docs` (extra documentation) | `EntityOverride::docs` (**Amendment B**) | **Appends** to discovered docs, never replaces; never changes `documented` (coverage stays honest). Authored inline or by referencing `.cratevista/docs/*.md` |

   **Must not** change the entity's id/kind/qualified_name/parent/source (identity preserved) — enforced at the type level, since `EntityOverride` has no such fields. Override of unknown id → `overlay_target_missing` warning (ignored) — **already implemented by issue-05 `apply_overlay`.**
6. Merge precedence (documented): discovered base < overrides < explicit manual additions; conflicting overrides for the same id resolved by last-loaded-in-sorted-order with a warning. Result is deterministic.
7. Validation is structural + referential; errors carry a workspace-relative file path and (via `toml::de::Error::span()` for parse errors and `serde_spanned::Spanned<T>` for semantic ones) line/column where possible, degrading to file-level when no span exists.
8. Produce `cratevista_graph::GraphOverlay` for issue 05; never panic on bad input.

## Technical design

### Module boundaries

`cratevista-config` modules: `discover` (locate files), `load` (parse TOML with spans via `toml`/`serde_spanned`), `model` (raw config structs), `validate` (**config-internal only** — see below), `overlay` (raw→`cratevista_graph::GraphOverlay`: manual entities/relations, flows→`View`s, overrides→`EntityOverride`s), `docs` (load referenced markdown via `cratevista_schema::source::RepoRelativePath`), `error` (diagnostics with spans).

**Dependencies.** New: `toml`, `serde_spanned` (added to `[workspace.dependencies]`). **Not** `toml_edit`: it is a format-preserving editor and nothing here rewrites TOML — `toml::de::Error::span()` plus `Spanned<T>` already give every span a diagnostic needs. Crate deps: `cratevista-schema` (types + `RepoRelativePath`) and `cratevista-graph` (for the `GraphOverlay`/`EntityOverride` types only).

**Dependency direction (must hold, and must be proven by `cargo tree -i` like PRDs 04/05):**

```text
cratevista-config → cratevista-graph → cratevista-{schema,metadata,rustdoc}
cratevista-core   → cratevista-config          (core wires config into the pipeline)
```

`cratevista-graph` **must not** gain a dependency on `cratevista-config` — it stays pure (issue 05), and the empty overlay remains its normal default input. `cratevista-config` must not depend on `cratevista-core`, `cargo-cratevista`, `cratevista-server`, `cratevista-metadata` or `cratevista-rustdoc`.

**Validation split (corrected).** The earlier draft had `validate` take a `&ResolutionIndex` — **no such type exists** in `cratevista-graph`, and referential validation against discovered ids is **already implemented downstream** (see "## Current repository state"). Therefore:

- **`cratevista-config::validate` is config-internal and pure**: syntax/structure (missing field, type mismatch, unsupported/empty kind), duplicate ids **within the configuration**, internal reference consistency (a flow member or relation endpoint naming a manual entity declared in the same config set), stage-order sanity, and missing referenced doc/example files. It needs **no** discovered ids and so needs no graph input.
- **Cross-referencing config ids against *discovered* ids stays in issue 05**, which already drops and diagnoses them (`invalid_view_reference`, `dangling_relation`, `overlay_target_missing`). Config **must not** duplicate, pre-empt, or contradict those codes.

This keeps `cratevista-config` a pure, independently testable transform (files → `GraphOverlay` + diagnostics) with no dependency on analysis order.

### Data model (raw config)

```
# .cratevista/flows/<name>.toml
[[entity]]  id, kind, label, label_translations?, description?, tags?, attributes?
[[flow]]    id, title, description, default_focus?, [[flow.stage]] id,title,order,
            members = [entity-ids…], [[flow.relation]] from,to,kind?,role?,label?,attributes?,
            [[flow.example]] id,title,path,language?,description?, docs=[path…]

# .cratevista/overrides/<name>.toml
[[override]] target = "<stable-id>", label?, category?, tags?, description?, stage?, hidden?, promoted?, docs?, presentation?
```

### Control flow

`cratevista-core::run_generate` → `config::discover` + `config::load` (independent of analysis; may run first) → `config::validate(&raw)` (config-internal only) → `config::overlay::build(raw) -> GraphOverlay` → passed as `GraphInput { metadata, rustdoc, overlay }` → `cratevista_graph::build_document`, which applies the overlay and performs the referential sanitation against discovered ids. Config diagnostics are merged into the same `diagnostics[]` the pipeline already emits.

### Error handling

`ConfigError`/diagnostics: ParseError{file,line,col,msg}, DuplicateId, UnknownReference{target}, UnsupportedKind, MissingField, TypeMismatch, MissingDocFile. All non-fatal to generation (valid parts still produce a document); a fully invalid config yields the discovered-only document + diagnostics.

### Compatibility

TOML schema versioned alongside the document schema (a `version` key in `cratevista.toml`, optional and unused for MVP). Config additions are additive.

**`ExplorerDocument` gains exactly one additive change** — `View::docs` and `View::examples` (Amendment A) — bumping **`SchemaVersion` 1.0 → 1.1**. Manual entities, flows, overrides, stages, edge labels and membership all already have schema homes and need nothing.

Compatibility, verified against the code:

| Scenario | Outcome |
| --- | --- |
| New generator (1.1) → current server | **Loads.** Server gates on major; document/diagnostics both emit 1.1, so the equality check passes. |
| New generator (1.1) → current frontend | **Loads.** Frontend gates on `SUPPORTED_SCHEMA_MAJOR = 1`. |
| Existing committed 1.0 snapshots (E2E + benchmark fixtures) | **Still load.** Major 1, and their document/diagnostics versions equal each other. **No fixture regeneration is required** by this PRD. |
| 1.0 document + ajv JSON-Schema validation | **Still valid.** `SchemaVersion` is an unconstrained string; the new fields are optional. |
| Old generator (1.0) → new frontend | **Loads**; no docs/examples render. |
| A 2.x document | Still rejected (`schema_version_unsupported`), unchanged. |

### Frontend compatibility (PRD 07 is Implemented / Verified)

Checked against the shipped UI, not assumed. **A manual flow needs no frontend change:**

- **Explicit membership works.** `web/src/adapter/adapter.ts::viewEntityIds` honours `View::entity_ids` when present (and defensively skips ids absent from the document), falling back to the kind filters otherwise. Manual flows therefore project exactly their declared members.
- **Manual views appear automatically.** View tabs are data-driven from `document.views`, so a flow shows up as another tab with no UI work.
- **Unknown kinds render.** `external_system`/`infrastructure`/`manual_block` hit the adapter's generic unknown-kind fallback (marked "(unknown)") and appear in the dynamic legend.
- **Edge labels render** from `Relation::label` via `localized(...)`, falling back to the relation kind.
- **Stages render** when the view defines `stages`; entity membership comes from `attributes["stage"]`.
- **`default_focus`** is applied on load (`applyDefaultFocus`).
- **Diagnostics** surface through `/api/diagnostics` and the diagnostics panel.

**Gaps in the shipped UI, and how they are closed:**

- `View::description` is **not rendered** today; **Amendment C** renders it.
- There is **no renderer for flow-level doc blocks or output examples** today;
  **Amendment C** adds one, reading the **embedded** `View::docs`/`View::examples`
  (Amendment A) — so no `/api/source` request, no new dependency, and the
  existing `SafeMarkdown` sanitization applies.
- Reduced mode uses **visible** node count, so a flow view is unaffected unless
  it declares >1,500 members (`docs/benchmarks/prd-07-large-graph.md`).

Amendment C is the **only** frontend change this PRD requires; it is scoped,
listed in "## Required amendments", and must leave the eight generated views
visually unchanged (they carry no docs/examples).

### Security and privacy

Referenced docs/examples are loaded **only** via `cratevista_schema::source::RepoRelativePath`, which rejects absolute paths, drive letters, UNC paths and any `..`, and normalizes `\`→`/`. The same validation applies to any optional `source` on a manual entity. No escaping the workspace. Config diagnostics must not embed absolute paths (consistent with the rest of the tool) — report workspace-relative paths plus line/column.

### Embedded example contents — an explicit privacy consequence

Amendment A embeds doc/example **contents** into `document.json`. This is a
deliberate consequence of the locked decision ("work without `/api/source`"), and
it must be documented rather than discovered:

- Embedded contents are served by **`/api/document` to every client**, and are
  included in a **static export** (issue 10), **regardless of `--source`**.
  `--source` gates the on-demand `/api/source` endpoint; it does **not** gate
  document contents.
- This does **not** weaken the project's source-privacy rule ("source snippets
  must be opt-in or constrained to explicit source locations"): nothing is
  embedded unless a maintainer **explicitly names that file** in committed
  configuration. It is opt-in by authorship, per file, and reviewable in a diff.
- The consequence is nonetheless sharp: **do not reference files containing
  secrets, credentials or private data from `[[flow]].examples`.**
  `docs/configuration.md` must state this next to the `examples` key, not in a
  footnote, and issue 10 must state whether a published export includes them.
- The **64 KiB per-example cap** bounds accidental bulk inclusion (e.g. pointing
  at a large log or dump) and keeps `/api/document` small; oversize examples are
  dropped with a diagnostic rather than silently truncated.
- Only the **listed** file is read. Config never globs a directory into examples.

### Interaction with `ISSUES/issue_11_source_path_duplication.md`

Issue 11 is a **`cratevista-rustdoc` defect**: `map_span` joins a workspace-relative rustdoc filename onto `package_root`, so rustdoc-derived `SourceLocation`s duplicate their package prefix (`crates/cvapp/crates/cvapp/src/lib.rs` instead of `crates/cvapp/src/lib.rs`). Metadata-derived paths are correct.

It is **orthogonal to this PRD and not a blocker**, but two rules follow:

- **Config paths are unaffected.** Author-written doc/example paths never pass through `map_span`; they are validated directly by `RepoRelativePath`. Do not copy, imitate, or "compensate for" the `map_span` join.
- **Overrides must not be used to patch it.** `EntityOverride` cannot set `Entity::source` (identity is preserved by construction), and this PRD must not add that ability as a workaround — doing so would push a tool bug into users' committed configuration and entrench it. Issue 11 is fixed in `cratevista-rustdoc`, under its own PRD.

If issue 11 is fixed before this PRD lands, no change here is required.

## CLI/API/configuration changes

Defines the `cratevista.toml` + `.cratevista/` schema (documented in the canonical `docs/configuration.md`). `generate` (and therefore `open`) automatically applies config; **`--no-config`** disables it, producing pure discovered output. `--no-config` is added to `GenerateArgs`, so `open` inherits it.

**Scope correction — tool-setting binding is deferred.** The earlier draft bound the reserved `[metadata]`/`[rustdoc]`/`[server]` sections here. That is **not** required by issue 08, whose capabilities are manual entities, flows and overrides only, and it would change how the **already-implemented** PRD-03/04/06 options resolve — introducing a CLI-vs-file precedence contract, per-option merge semantics and new failure modes that belong with those PRDs, not with manual flows. The `init` template already describes these sections as "bound in later releases". **This PRD reserves them and binds none of them**; a `version` key in `cratevista.toml` stays optional and unused for the MVP. See Open questions.

## Files and modules to create or modify

- `crates/cratevista-config/{Cargo.toml,src/{lib,discover,load,model,validate,overlay,docs,error}.rs}` — a **new** crate (no placeholder exists), plus its entry in the root `Cargo.toml` `[workspace] members` and `toml`/`serde_spanned` in `[workspace.dependencies]`.
- `crates/cratevista-config/tests/{load,validate_refs,overrides_identity,merge_precedence,flow_build,bad_input}.rs`
- Fixtures: `crates/cratevista-config/fixtures/{clients_gateway_services_infra/,invalid_refs/,duplicate_ids/}` each with `.cratevista/flows/*.toml` and `.cratevista/overrides/*.toml`.
- `docs/configuration.md` (complete reference + examples); `docs/adr/0007-config-format-toml.md`.
- Wire `--no-config` + overlay application into `generate` (issue 05 pipeline).

## Testing strategy

### Unit tests

- Parse valid config; spans captured.
- Override preserves id/kind/qualified_name (identity), changes only presentation.
- Duplicate id / unknown ref / missing field / type mismatch each produce the right diagnostic with location.

### Integration tests

- The Clients → Gateway → Services → Infrastructure sample flow: manual + discovered entities coexist in one view; edge labels present; default focus set.
- Merge precedence: discovered < override < manual; conflicting overrides resolved deterministically with warning.
- Bad input: broken references do not crash; document still generated; diagnostics present.
- Determinism: same config → identical overlay/document.

### End-to-end tests

- `cargo cratevista generate` in a fixture workspace with `.cratevista/` produces a document containing the manual flow view; `--no-config` omits it.

### Fixtures

A realistic sample under `fixtures/clients_gateway_services_infra/` used both for tests and as the documented example in `docs/configuration.md`.

## Performance considerations

Config is small; parsing negligible. Loading is pure and cache-friendly (issue 09 keys include config files).

## Observability and diagnostics

Config diagnostics flow into the document `diagnostics[]` and the CLI summary; server `/api/diagnostics` surfaces them; each carries file + location.

## Documentation changes

`docs/configuration.md` (primary), ADR-0007 (TOML choice), README manual-flow example (issue 10).

## Rollout and migration

New crate + convention. `init` template (issue 01) expands to show override/flow examples (commented).

## Risks and mitigations

- **YAML-vs-TOML expectation mismatch** → ADR-0007 documents the decision; TOML chosen for ecosystem fit/comments/spans (dependency trade explained in FR1).
- **Overrides mutating identity** → type-level separation (override cannot set id/kind); identity test.
- **Crashes on malformed input** → all parsing fallible + graceful-degradation test; server keeps last valid document.
- **Line/col fidelity** → use `toml::de::Error::span()` + `serde_spanned::Spanned<T>`; degrade to file-level if spans are unavailable. Columns count characters, not bytes, so a multi-byte identifier does not skew them.

## Alternatives considered

- YAML (as in the issue examples): rejected — requires a YAML crate (e.g. unmaintained `serde_yaml`), weaker Cargo-ecosystem fit; TOML matches `Cargo.toml` conventions with good comment/diff/span support at the cost of the `toml`/`serde_spanned` dependencies (an accepted, justified trade — no format is dependency-free). Hand-authored JSON was likewise not adopted, because JSON lacks comments and is less diff-friendly for human-authored config.
- Single monolithic config file: allowed but the `.cratevista/` split (flows vs overrides) scales better for many flows and keeps override-vs-flow concerns separate; the directory split is canonical for MVP.

## Implementation sequence

Each step is independently verifiable. Steps A–C are the locked amendments to
completed PRDs; they land **first**, each green on its own, so a regression is
attributable to one amendment rather than to the new crate.

**A. PRD-02 amendment** — add `View::docs`/`View::examples` + `ViewExample`; bump `SchemaVersion::CURRENT` to `"1.1"`; regenerate the committed JSON Schema and the frontend types; de-literalize the `router.rs` health assertion; add the round-trip/back-compat/determinism tests and the 1.0-snapshot regression test. Gate: `cargo test --workspace --all-features`, `npm run check:types`.
**B. PRD-05 amendment** — add `EntityOverride::docs` and the append semantics in `apply_overlay`; tests per "Amendment B". Gate: `cargo test -p cratevista-graph`.
**C. PRD-07 increment** — render `View::docs`/`View::examples`/`description` via `SafeMarkdown`; component + sanitization + empty-case tests; one browser assertion. Rebuild and **commit `web/dist`**. Gate: `npm run test`, `npm run check:dist`, `npm run e2e`, `npm run check:embed-rebuild`.

Then the new crate (steps 1–6 need no changes outside it until step 7):

> **Steps 0–2 LANDED 2026-07-16.** *(Recorded when steps 3–7 were not yet started and config was not wired into `cratevista-core`. Steps 3–7 have since landed — see their ledgers below; the pipeline now reads configuration and `--no-config` exists.)*
>
> **Shipped:** `crates/cratevista-config` (`#![forbid(unsafe_code)]`), registered in `[workspace] members`, with modules `error` (stable codes + byte-offset → line/column), `model` (raw TOML model), `discover` (deterministic), `load` (span-preserving, per-file non-fatal) and `validate` (config-internal only).
>
> **Dependency deviation — `toml_edit` was NOT added.** The PRD's original list named `toml`, `toml_edit` and `serde_spanned`. `toml_edit` is a **format-preserving editor**; this crate only *reads*, and `toml` + `serde_spanned` already supply everything diagnostics need (`toml::de::Error::span()` for parse errors, `Spanned<T>` for semantic ones). Adding it would have violated the project rule against taking a dependency merely because a document mentions it. Resolved: `toml` **1.1.3**, `serde_spanned` **1.1.1**; `toml_edit` is not even a transitive dependency (toml 1.x no longer parses through it). A test pins this.
>
> **Diagnostics:** stable codes (`config_parse_error`, `config_invalid_structure`, `config_duplicate_entity_id`, `config_duplicate_flow_id`, `config_invalid_stage`, `config_unknown_manual_reference`, `config_invalid_id`, `config_read_failed`), each with a **workspace-relative** path and, where a span exists, a **1-based line/column counted in characters** (so a multi-byte identifier does not skew the column). A missing/misaligned span degrades to file-level rather than inventing a position. No diagnostic carries an absolute path — a test asserts it.
>
> **Determinism:** discovery sorts by file name (never `read_dir` order, which differs by OS and by run), and load order follows it. Load order decides which duplicate wins, so it must be a property of the names, not the disk.
>
> **Validation boundary, as specified:** the **complete global manual-id set is built in pass 1 across every flow file**, before any reference is resolved in pass 2 — so a flow may reference an entity declared in *another* file, even one that loads *later*, and the outcome never depends on file order. Discovered entity ids and override targets are **passed through untouched**: PRD 05 already diagnoses them (`invalid_view_reference`, `dangling_relation`, `overlay_target_missing`), and duplicating that here would create a second source of truth.
>
> **Tests: 45** (`cargo test -p cratevista-config`) — 4 `error` (1-based positions; character columns; out-of-range/misaligned degradation; display), 7 `discover` (absence is normal; sorted-not-filesystem-order; repeatable; only `*.toml`, directories ignored; missing dir; relative `/`-normalized labels), 11 `load` (full flow file incl. translations; comments; a syntax error is located and costs only its own file; missing field; **unknown field rejected, not silently ignored**; wrong type; reserved root sections tolerated-but-ignored; sorted load; overrides with every documented key; no absolute path leaks), 18 `validate` (valid config is silent; global duplicate entity ids across *and* within files naming both locations; **cross-file references**; **forward references to a later-loading file**; unknown manual refs in members/focus/relations; **discovered ids never validated**; **override targets not resolved**; duplicate flow ids; duplicate stage ids and orders; empty ids/kinds; an invalid kind still leaves the entity referenceable; the `manual:` prefix rejected with guidance; duplicate example ids; determinism across 5 runs), and 5 **dependency-boundary** tests.
>
> **Dependency-boundary evidence.** `cargo tree -i -p cratevista-config` lists only the crate itself — **nothing depends on it**, so `cratevista-graph` is provably independent. `cargo tree -p cratevista-config --depth 1` shows exactly `cratevista-schema`, `serde`, `serde_spanned`, `toml` (+ dev `tempfile`). `tests/dependency_boundary.rs` enforces this in `cargo test` rather than in a command someone must remember: it reads the manifests' dependency sections (ignoring comments) and fails if the graph — or schema/metadata/rustdoc/server — ever names config, if config reaches for core/CLI/server/metadata/rustdoc, if `toml_edit` appears, or if the crate leaves the workspace. **Verified by negative control:** injecting `cratevista-config` into `cratevista-graph`'s manifest makes the guard fail with a precise message; removing it makes it pass again.
>
> **Gates:** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features` (**304 passed**, up from 259 — the 45 new tests), `cargo +1.97.0 check --workspace --all-features`. All exit 0.

0. **Create the crate** and add `crates/cratevista-config` to the root `Cargo.toml` `[workspace] members`; add `toml`/`serde_spanned` to `[workspace.dependencies]` (**not** `toml_edit` — see the deviation note above). Verify the dependency direction with `cargo tree -i` (no path from `cratevista-graph` to `cratevista-config`).
1. `model` + `load` (spans) + `discover` (deterministic sorted order).
2. `validate` — **config-internal only** (structure, duplicate ids within config, internal refs, missing doc/example files). No discovered ids.
3. `overlay` (manual entities/flows/overrides → `cratevista_graph::GraphOverlay`).

> **Step 3 LANDED 2026-07-16 — PRD 08 stays Approved.** Steps 4–7 are not started: no `docs.rs`, no filesystem-backed docs/examples, no core/CLI wiring, no `--no-config`, no fixtures, no user documentation.
>
> **The legitimate one-way edge landed:** `cratevista-config → cratevista-graph` (for `GraphOverlay`/`EntityOverride`), plus `serde_json` (because `AttrValue` *is* `serde_json::Value`). **`cratevista-graph` remains pure** — `cargo tree -p cratevista-graph | grep -c cratevista-config` = 0. Notably, the reverse edge is now **impossible rather than merely forbidden**: adding it makes Cargo fail with *"cyclic package dependency"* before any test runs, which is a stronger guarantee than the boundary assertion. The boundary test was updated to allow `cratevista-graph` in this direction only and to record why.
>
> **API:** `build_overlay(&RawConfig, &Validation) -> OverlayOutcome { overlay, diagnostics }`. It takes the `Validation` so the authoritative accepted-manual-id set decides what reaches the graph — an entity `validate` already rejected (empty id, `manual:`-prefixed id, duplicate) never appears and is **not diagnosed twice**.
>
> **Mapping.** Manual entity → `Entity` with id `manual:<id>`, `Provenance::Manual`, and `qualified_name` = the config-local id (so it is searchable by the name its author gave it — matching the canonical `manual_flow.document.json` fixture). Label/description localized (`default` + translations), tags sorted/deduped, attributes converted TOML→`AttrValue`. Flow → `View` with **explicit `entity_ids` membership** (kind filters empty), localized title/description, `default_focus`, and stages as `stage:<id>`. Relation → `Relation` with `Provenance::Manual`, default kind `manual`, `Relation::label` (**not** an attribute — the PRD-07 adapter renders `label`), and `RelationId::with_role` when a role is given. Override → the real `EntityOverride` fields: `label`/`description`/`add_tags`/`hidden` directly, and `category`/`stage`/`promoted`/`presentation` into `set_attributes` (`stage` genuinely *is* an attribute — the UI reads `attributes["stage"]`).
>
> **Discovered ids are passed through verbatim** — members, relation endpoints, `default_focus` and override targets are never resolved or validated here. PRD 05 owns that (`invalid_view_reference`, `dangling_relation`, `overlay_target_missing`). Tests pin it.
>
> **No filesystem reads.** `[[flow]].docs`, `[[flow.example]].path` and `[[override]].docs` are left unresolved for step 4: views come out `docs: None` / `examples: []`, and `EntityOverride::docs` is `None`. A test points a flow at a **non-existent** file and asserts no diagnostic — proving nothing was opened. A manual entity's `source` **path** *is* mapped, because `RepoRelativePath` validation is pure string work that opens nothing; a traversing path is diagnosed (`config_invalid_source_path`) and omitted while the entity survives.
>
> **Determinism.** Identical input → identical overlay (asserted over 5 repeats). Author order is preserved where it *is* the narrative (flow members, relations, examples); stages are ordered by their explicit `order` field, because that — not declaration position — is where a flow's narrative lives.
>
> **Duplicate-override precedence:** field-level **last-loaded-wins** (files load in sorted order), with a `config_duplicate_override` diagnostic naming the later file. Field-level rather than wholesale replacement so two overrides setting *different* fields compose, and because `add_tags` is additive by nature; only a field both set actually conflicts.
>
> **New diagnostic codes:** `config_invalid_source_path`, `config_duplicate_relation`, `config_duplicate_override`.
>
> **Tests: 18 new (63 total in the crate; workspace 304 → 322).** Entity mapping (prefix/provenance/qualified_name/localization/tags/attributes/source); traversing source path; flow → view with explicit membership + focus + empty kind filters; stages ordered by `order`, not declaration; relations with role/label/distinct ids and author order; a role-less relation's basic id and default kind; duplicate relations diagnosed rather than collapsed; overrides onto the real fields; override targets passed through unresolved; duplicate-override field-level merge; cross-file manual references; validation-rejected entities excluded and not re-diagnosed; duplicate entity → one entity, first wins; duplicate flow → one view; empty config → empty overlay; determinism over 5 repeats; TOML→AttrValue across every type; and **no file-backed resolution**.
>
> **Gates:** `cargo test -p cratevista-config` (**63 passed**), `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features` (**322 passed**, up from 304), `cargo +1.97.0 check --workspace --all-features`. All exit 0.
4. `docs` loading + `RepoRelativePath` validation.

> **Step 4 LANDED 2026-07-16 — PRD 08 stays Approved.** Steps 5–7 are not started: no fixtures, no core/CLI wiring, no `--no-config`, no ADR, no user documentation.
>
> **Shipped:** `crates/cratevista-config/src/docs.rs` — the crate's **only** filesystem module. API: `WorkspaceFiles::{new, read_text}`, `FileError`, `MAX_EXAMPLE_BYTES`, and `embed_files(workspace_root, &RawConfig, &Validation, &mut GraphOverlay) -> Vec<ConfigDiagnostic>`, called **after** `build_overlay` to fill in what step 3 deliberately left empty.
>
> **Path safety is layered, because one layer is not enough:**
> 1. `RepoRelativePath` rejects absolute paths, drive letters, UNC and any `..` — pure string validation;
> 2. the path is resolved with `canonicalize()`, which **follows symlinks**;
> 3. the **resolved** file must still sit inside the **canonical** workspace root. This is the symlink guard: a link's path text is perfectly clean, so only resolution reveals the escape. The root is canonicalized too, which is what makes this correct on macOS where `/tmp` is itself a symlink;
> 4. it must be a regular file (a directory is not content);
> 5. when the root cannot be canonicalized there is nothing to prove containment against, so every read is **refused** — failing closed.
>
> **Only explicitly named files are read.** No globbing, no directory walking, no implicit discovery.
>
> **Embedding.** Flow docs → `View::docs` in **declaration order**, joined with exactly one blank line using the same junction rule as Amendment B (only adjoining newlines normalized; interior content byte-identical), `summary: None` (inventing one would put words in the author's mouth) and `documented: true` (inert for views — coverage reads only `Entity::docs`). Flow examples → `View::examples` in **narrative order** (a sequence is a story: request, then response). Override docs → `EntityOverride::docs` with **`summary: None`, `documented: false`** exactly as specified; note that `cratevista_graph::overlay::append_docs` (Amendment B) ignores that field entirely and preserves the *discovered* `documented`, so configuration cannot move coverage by any route.
>
> **The 64 KiB per-example cap** is checked against the file's real size **before reading**, so an enormous file is never pulled into memory to be rejected, and an oversize example is **dropped whole — never truncated** (a partial example would be a silent lie about the file). The diagnostic carries the real byte count. UTF-8 decoding is **strict**: no lossy replacement, which would silently corrupt embedded content.
>
> **Failures are local.** A bad doc or example drops only itself; the rest of the flow, the other docs, and the override's other fields all survive. Diagnostics carry only the author's own repo-relative spelling — a test asserts no absolute path leaks.
>
> **New diagnostic codes:** `config_invalid_file_path`, `config_missing_file`, `config_not_a_file`, `config_path_escapes_workspace`, `config_not_utf8`, `config_example_not_utf8`, `config_example_too_large`.
>
> **Refactor:** `accepted_flows` was extracted in `overlay.rs` and is now the **single** definition of "which flows become views", shared by steps 3 and 4 — step 4 correlates its reads by **view id**, not index, and two copies of that rule would have drifted.
>
> **Tests: 22 new (85 in the crate; workspace 322 → 344).** Valid embedding (contents byte-identical after decoding); multiple docs joined in declaration order; missing doc and missing example each dropping only themselves; narrative order preserved; traversal (`../…`) refused; absolute path refused *even when the file exists inside the workspace*; directory refused; non-UTF-8 doc and example refused with distinct codes; example **exactly at** the limit embedded; **one byte over** dropped whole and not truncated; docs exempt from the cap; override docs with `summary: None`/`documented: false`; an override doc failure leaving the override's other fields intact; no absolute path in any diagnostic; determinism over 5 repeats; a flow with no docs/examples untouched; the containment predicate directly; fail-closed on an uncanonicalizable root; **symlink escape refused** and **internal symlink allowed**; and the Markdown join rule.
>
> **Gates:** `cargo test -p cratevista-config` (**85 passed**), `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features` (**344 passed**, up from 322), `cargo +1.97.0 check --workspace --all-features`. All exit 0.
>
> **Two honest limitations, recorded rather than hidden:**
> - **The symlink tests skip on Windows**, where creating a symlink needs Developer Mode or admin — they print why rather than passing silently, and they *do* run on CI (`ubuntu-latest`). The containment predicate they exercise is unit-tested on every platform, so the logic is never uncovered.
> - **Docs are uncapped; only examples are.** That follows the PRD as written, but the same reasoning that justifies the example cap (embedded content ships on *every* `/api/document` fetch) applies to a large Markdown file too. Flagged for step 7 (`docs/configuration.md`): either document that docs are unbounded, or add a cap and say so.
5. Fixtures + unit/integration tests for 1–4.

> **Step 5 LANDED 2026-07-16 — PRD 08 stays Approved.** Steps 6–7 are not started: config is still **not** wired into `cratevista-core`, there is no `--no-config`, no ADR-0007 and no `docs/configuration.md`.
>
> **The three approved fixture sets** (`crates/cratevista-config/fixtures/`, 14 files):
> - **`clients_gateway_services_infra/`** — the issue-08 reference pattern, and the same sample `docs/configuration.md` will document in step 7. Four manual entities (`external_system`/`manual_block`/`infrastructure`), a flow whose **explicit membership mixes manual ids with the discovered `package:demo`**, four stages **declared out of order on purpose** (4, 1, 3, 2), five labelled relations — including **two between the same pair kept distinct by `role`** (`http`/`ws`) and edges in both directions across the manual/discovered boundary — a `default_focus`, two doc files and two examples, plus a presentation override of the discovered package with its own doc file.
> - **`invalid_refs/`** — an unparseable file *plus* a healthy one, unknown manual references in a member/focus/relation, discovered ids that do not exist, an override aimed at a missing entity, and missing doc/example files.
> - **`duplicate_ids/`** — the same entity id and flow id in two sorted files, and the same override target in two sorted files.
>
> **`crates/cratevista-config/tests/pipeline.rs` (19 tests) runs the real pipeline** — `discover → load → validate → build_overlay → embed_files` — and then hands the overlay to the **real `cratevista_graph::build_document`**. That last step is the point: it proves the *seam* works rather than that this crate's types agree with themselves.
> - **Manual and discovered entities coexist in a schema-valid document** (`document.validate()` asserted explicitly), the manual flow view sits **alongside** the eight generated views rather than replacing them, and no relation is dropped as dangling.
> - **The override enriches without touching identity**: label/tags/attributes change while kind/qualified_name/provenance do not, and — via Amendment B — its manual docs **append after** the discovered rustdoc while the discovered `summary` and `documented` survive.
> - **PRD 05 owns unknown discovered references**: config emits *nothing* about `item:struct:nope::Gone` or the missing override target, and the graph emits `invalid_view_reference`, `overlay_target_missing` and `dangling_relation` — with the document still valid. This is the boundary, asserted from both sides.
> - **Local degradation**: the malformed file produces one located `config_parse_error` while its healthy neighbour still yields its entity and flow; missing docs/examples drop only themselves.
> - **Precedence**: duplicate entity/flow ids are reported against the *second* file, name the first, and first-wins (exactly one entity, one view); duplicate overrides merge **last-loaded-wins per field** across sorted files, with fields only the first set surviving and tags unioned.
> - **Determinism**: the canonical `document.json` bytes and the rendered diagnostics are identical across 5 repeats.
>
> **Pinned, not endorsed:** `markdown_docs_are_currently_uncapped` fixes today's behaviour in a test. The example cap exists because embedded content ships on **every** `/api/document` fetch — which is equally true of a large doc. Step 7 must either document docs as unbounded or cap them; that test is the one to update, deliberately.
>
> **Symlink containment now runs where it can.** The step-4 unit tests and the new end-to-end test are `#[cfg(unix)]` with a hard `expect` on symlink creation, replacing "attempt and skip" — so on the platform CI runs they **cannot** silently become no-ops. Windows lacks the privilege by default; there the containment predicate is still unit-tested directly, so the logic is never uncovered. The `cfg(unix)` code was type-checked against `x86_64-unknown-linux-gnu` rather than left for CI to discover.
>
> **Dependency note:** `cratevista-metadata` was added as a **dev-dependency only** — `build_document` takes a `GraphInput` carrying a `MetadataIngest`, which the tests must construct to have any discovered entities at all. `tests/dependency_boundary.rs` now distinguishes runtime from dev sections and pins that metadata never becomes a runtime dependency (a dev-dependency does not ship, so it cannot violate the architecture).
>
> **Gates:** `cargo test -p cratevista-config` (**103** on Windows: 78 unit + 6 boundary + 19 pipeline; **106** on Unix, where the three symlink tests compile), `cargo test -p cratevista-graph` (**41**), `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features` (**362**, up from 344), `cargo +1.97.0 check --workspace --all-features`, and `cargo check --target x86_64-unknown-linux-gnu -p cratevista-config --tests`. All exit 0.
6. Wire into `cratevista-core::run_generate` + add `--no-config` to `GenerateArgs`; E2E through `generate`.

> **Step 6 LANDED 2026-07-16 — PRD 08 stays Approved.** Only step 7 remains (`docs/configuration.md` + ADR-0007). No README change, no watch support, no frontend change.
>
> **Orchestration API:** `cratevista_config::load_config(&Path) -> ConfigOutcome { overlay, diagnostics }` runs `discover → load → validate → build_overlay → embed_files`. **Absence returns an empty overlay with no diagnostics** via a cheap early exit — nothing is validated, converted or read — which is byte-identically equivalent to `GraphOverlay::default()`.
>
> **Dependency:** `cratevista-core → cratevista-config` added. `cargo tree --workspace --invert cratevista-config` → `config ← core ← cargo-cratevista`; `cratevista-graph` has **zero** path to config, and config still has zero path to core/CLI/server.
>
> **Pipeline:** config is loaded **before** `build_document` (the overlay is an input to it) and passed through `GraphInput.overlay`, replacing the hard-coded `GraphOverlay::default()`. A `config_ms` duration is recorded alongside `metadata_ms`/`rustdoc_ms`/`graph_ms`.
>
> **Diagnostics:** `crates/cratevista-core/src/config_diagnostics.rs` converts `ConfigDiagnostic → DocumentDiagnostic`. They join the graph's (which already carry metadata's and rustdoc's) in the existing `sort()` + `dedup()`, so the merge is **order-independent and deterministic**, and `generation.counts.diagnostics` picks them up automatically because it is derived from that vector.
>
> **Recoverable by construction:** every configuration problem is a `Severity::Warning`, `run_generate` returns `ExitCode::SUCCESS` regardless of diagnostic severity, and `partial` comes **only** from `result.partial` (rustdoc's `--keep-going`), so config can never set it. A broken file costs its own contents; the discovered document is still committed.
>
> **`--no-config`** was added to `GenerateArgs` — the **shared** generate/open option path — so `open` inherits it. Verified on the real CLI: `generate --help` and `open --help` each list it; **`serve --help` does not**, because `serve` only replays existing artifacts and has no generation to configure.
>
> **Where the location went, and why.** `DocumentDiagnostic` is `{severity, code, message, entities, relations}` — it has **no location field**. Rather than invent one, the location is prefixed onto the **message** in the conventional `file:line:column: message` form (e.g. `.cratevista/flows/broken.toml:1:6: key with no value…`). The **`code` stays a first-class field**, so machine consumers match on it exactly as before. A structured `location` field would be an additive **PRD-02 amendment** (`SchemaVersion` 1.1 → 1.2) plus a PRD-07 renderer — neither authorized by step 6, so it is recorded here as a candidate rather than done quietly.
>
> **Tests: 14 new (workspace 362 → 376).** 5 unit (`config_diagnostics.rs`: code stays first-class; location preserved with and without a position; every problem is a warning; batch order). 9 integration (`crates/cratevista-core/tests/generate_config.rs`, **no nightly, no network** — a bin-only workspace makes the run metadata-only, so the whole config path is exercised on every platform): valid config adds the flow/docs/examples/overrides and is silent; artifacts pass **schema validation, BLAKE3 hash verification and the real `cratevista_server::load_snapshot`**; malformed config still commits discovered output with a located warning, exit 0 and `partial` untouched; `--no-config` ignores even a deliberately malformed file and emits nothing; `--no-config` is **byte-identical** to a genuinely unconfigured project; absent config behaves like an empty overlay; unknown discovered references are diagnosed **only** by PRD 05 (`invalid_view_reference`/`overlay_target_missing`/`dangling_relation`) while config stays silent about them; fixed-clock output is deterministic across repeats; and no artifact leaks an absolute path.
>
> **Real CLI E2E**, same workspace, both ways: with config → 4 entities, **9 views** (8 generated + the flow), docs embedded, one located `config_parse_error`, exit 0. With `--no-config` → 3 entities, **8 views**, no manual content, **0 config diagnostics**, exit 0.
>
> **Gates:** `cargo test -p cratevista-config` (**103**), `-p cratevista-core` (**43**), `-p cargo-cratevista` (**11**), `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features` (**376**), `cargo +1.97.0 check --workspace --all-features`. All exit 0.
7. `docs/configuration.md` + ADR-0007 (`0007` is free: `docs/adr/` holds 0001–0006 and 0010).

> **Step 7 LANDED 2026-07-16 — PRD 08 is now Implemented / Verified.** Documentation only: **no production or test code changed** (`git diff --stat` touches `docs/` and `PRD/` exclusively).
>
> **Shipped:** `docs/configuration.md` (the user configuration reference CLAUDE.md designates) and `docs/adr/0007-config-format-toml.md`. The ADR records why TOML (it is already the language of the project; YAML's whitespace/coercion hazards and JSON's lack of comments disqualify them), why content is split per file in sorted name order, why Markdown/examples are referenced by path rather than inlined, and the consequences: unknown keys are errors, every problem is a warning, ids are global, validation is split between config and PRD 05, contents are embedded, an override cannot change identity.
>
> **Covered:** locations, manual entities, flows, stages (`order` authoritative; duplicate `order` rejected), relation **roles as identity** (the two-edges-one-pair case, and why `label` alone will not disambiguate), overrides, precedence, localization, all **18 diagnostic codes**, and `--no-config`.
>
> **Embedding stated plainly:** `docs`/`[[flow.example]].path` contents are copied into `document.json` and therefore published by `/api/document` **and static exports, regardless of `--source`** — with the reason (`--source` gates on-demand `/api/source`; embedding is what makes an export self-contained) and the boundary (nothing is read unless committed configuration names that file explicitly — opt-in, per file, visible in review).
>
> **The docs cap is recorded as a limitation, not a guarantee:** examples are capped at 64 KiB and an oversize one is **dropped whole, never truncated**; **Markdown `docs` are currently uncapped**, stated verbatim as *"a known limitation, not a guarantee"*, with the note that a future release may cap them and that callers must not rely on embedding an arbitrarily large document.
>
> **Safety documented as the five layers the code actually implements:** repo-relative check (textual, never touches disk) → resolve following symlinks → resolved path must still be inside the workspace (a symlink's *path text* is innocent; only resolving reveals an escape) → must be a regular file → must be strict UTF-8 (refused, never lossily decoded). An unresolvable workspace root refuses every read.
>
> **Stale statements fixed (3).** Two in this PRD: `docs/configuration.md` "does not exist yet" and `--no-config` "does not exist" — both now describe what shipped. The third, in **PRD 05**, was a **real contradiction, not just stale prose**: it reserved `StageId` as `stage:{view-slug}:{n}`, but the canonical `manual_flow.document.json` fixture uses `stage:client`/`stage:gateway` and `cratevista-config` emits `stage:{id}`. The speculative form was never implemented and disagreed with the committed fixture; PRD 05 now documents the shipped `stage:{id}` and why per-flow uniqueness suffices. *(The Amendment-C `flow`-fixture note, which said config "does not exist yet", was also re-dated rather than deleted — it explains why that fixture is synthesized.)*
>
> **Every TOML example verified against the shipped parser, not by eye.** The doc's `cratevista.toml`, entity, flow, stage, relation, example and override blocks were written **verbatim** into a temp workspace whose crate is named `demo` (so `package:demo` genuinely resolves) and run through the **real `cargo cratevista generate` binary**: **exit 0, zero `config_*` diagnostics** (the only diagnostic is `no_documentable_rustdoc_targets`, expected for a bin-only crate). The emitted `document.json` confirms each claim the prose makes: `schema_version` **1.1**; `view:clients-to-infra` with all 4 members in declaration order (TOML key `members` → JSON field **`entity_ids`**), `default_focus`, stages ordered by `order`; the two doc files joined with exactly one blank line; both examples embedded in order with `language` — **and `--source` was never passed**, which is the direct evidence for the embedding claim; the override applied (`label` "Demo service", `tags` `["core"]`, its docs appended); and the two same-pair relations kept distinct as `…:http` / `…:ws`. The three parser behaviors the prose asserts but the fixtures do not show were read from the source rather than assumed: `merge_overrides` (field-by-field, last-wins, `tags` unioned), duplicate stage `order` → `config_invalid_stage`, and `RawRootConfig`'s parse-and-ignore reserved sections.
>
> **Gates:** `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace --all-features` (**376**, unchanged — documentation only), `cargo +1.97.0 check --workspace --all-features`, `npm run check:dist`. All exit 0.

## Required additive amendment for PRD 09 (approved 2026-07-16)

**PRD 08 remains Implemented / Verified.** This amendment is **additive and
read-only**. It adds no parsing, no validation, no diagnostic code and no
filesystem access — it *returns what the crate already resolves internally*.

> **B3 LANDED 2026-07-17** as part of PRD-09's prerequisite phase — **PRD 08 stays
> Implemented / Verified**.
>
> **Shipped:** `crates/cratevista-config/src/referenced.rs` — `ReferencedFileKind`
> (`FlowDoc | FlowExample | OverrideDoc`, with `as_str()`),
> `ReferencedConfigFile { path: RepoRelativePath, kind }`, and `collect(&RawConfig)
> -> Vec<ReferencedConfigFile>`. `ConfigOutcome` gained `referenced_files`,
> populated in `load_config` **after** `embed_files`. Re-exported from the crate
> root.
>
> **No new filesystem reads**, as required: `collect` is pure string validation
> through `RepoRelativePath::new` over the already-parsed `RawConfig`. That is
> precisely what makes "missing / oversized / non-UTF-8 / directory" list
> naturally — the module never asks the disk anything, so it cannot care what the
> answer would be.
>
> **Symlink escapes are deliberately not filtered here** (a deviation worth
> naming): detecting one requires resolving the path on disk, which this module
> must not do. `embed_files` still refuses to *read* through an escaping symlink
> (`config_path_escapes_workspace`); the worst case here is watching a link that
> lives inside the workspace, which is what watching a workspace means.
>
> **Every parsed flow counts**, including one whose id turns out to be a
> duplicate: it is still a declared reference. Watching a file that turns out not
> to matter costs one extra regeneration; missing one produces a stale document.
>
> **Tests: +18 (config 90 lib + 25 pipeline).** 12 unit: all three kinds; across
> files; missing file listed; **genuinely oversized (> `MAX_EXAMPLE_BYTES`) and
> genuinely invalid-UTF-8 (`0xFF 0xFE`) files listed**; a directory listed;
> absolute / `..` / drive-letter / UNC / empty spellings excluded; repeats
> deduplicated; one path under two kinds keeps **both** entries; order
> deterministic across loads and sorted by path not declaration; no absolute path
> exposed; an unparseable file contributes nothing; absent config yields nothing.
> 6 integration through the **real `load_config`**: the reference fixture lists
> exactly its five references with correct kinds; the committed `invalid_refs`
> fixture **lists both missing files while still emitting both
> `config_missing_file` diagnostics**; no absolute/traversing leakage;
> deterministic across loads; **the overlay and diagnostics are identical to the
> hand-assembled pipeline** (same codes, same messages, same order, same entity
> and view ids, same embedded example count); and absent config yields an empty
> list and still `is_empty()`.
>
> **Negative control:** making `collect` fall back to a placeholder instead of
> dropping invalid spellings fails the exclusion test.
>
> **Dependencies: none added** — `cratevista-config` already depended on
> `cratevista-schema` for `RepoRelativePath`.

### `ConfigOutcome.referenced_files: Vec<RepoRelativePath>`

**Why PRD 09 needs it:** watch mode must regenerate when "manual documentation
included by configuration" changes — an explicit issue-09 requirement. Those
paths are resolved inside `docs::embed_files` and **thrown away**;
`ConfigOutcome { overlay, diagnostics }` never returns them, so today they cannot
be watched. Re-deriving them in `cratevista-watch` would mean a second TOML
parser and a second answer to "which files are inputs" — exactly the duplication
this PRD exists to prevent.

**Shape:** the existing **`cratevista_schema::RepoRelativePath`** — the same type
`docs.rs` and `overlay.rs` already validate through. No new path type is
invented, and the value is typed, not a `String`, so a consumer cannot
accidentally hand an absolute path to a watcher.

**Contents:** every path declared by `[[flow]].docs`, `[[flow.example]].path`, and
`[[override]].docs`, **sorted and deduplicated** (deterministic output; two flows
may reference one shared `.md`).

**Inclusion rule — the subtle half:**

- **Include a valid declared reference even when the file is missing, oversized
  or non-UTF-8.** These are exactly the paths a user is about to *fix*: creating
  a missing file, shrinking an oversized example, or re-encoding a bad one must
  trigger regeneration. A file that produced `config_missing_file` today is still
  a declared input — arguably the *most* important one to watch, since the next
  write makes the config valid.
- **Exclude invalid or traversing spellings** — anything `RepoRelativePath::new`
  rejects (absolute, drive-letter, UNC, any `..`) or that resolves outside the
  workspace. Those are not inputs; they are errors already reported as
  `config_invalid_file_path` / `config_path_escapes_workspace`, and handing a
  traversing path to a filesystem watcher would register a watch **outside the
  workspace** — turning a rejected path into a real observation of the user's
  disk. The exclusion is a security boundary, not tidiness.

Note the asymmetry is deliberate and it is the whole design: **the file's
*content* being unusable does not disqualify the path; the *path* being illegal
does.**

**Compatibility:** adding a public field to `ConfigOutcome` breaks struct-literal
construction, but the only constructor is `cratevista-config` itself and the only
consumer is `cratevista-core` — both in-workspace, both updated in the same
change. `#[non_exhaustive]` is not proposed: this is a pre-1.0 internal crate and
the field should be visible in every match.

**Tests PRD 09 must add here:** a fixture whose flow references a missing file, an
oversized example and a `..`-traversing path asserts that the first two appear in
`referenced_files` and the third does not; a shared `.md` referenced by two flows
appears once; output is sorted and stable across runs.

## Acceptance criteria

- [x] A sample flow reproduces Clients → Gateway → Services → Infrastructure. *(integration test + docs example)*
- [x] Manual and discovered entities coexist in one view. *(flow_build test)*
- [x] Overrides preserve discovered stable IDs. *(overrides_identity test)*
- [x] Invalid references produce actionable diagnostics. *(validate_refs test asserts file+location+target)*
- [x] Configuration supports comments and reviewable diffs. *(TOML; documented; parse test with comments)*
- [x] The schema is documented with complete examples. *(docs/configuration.md + fixture; includes the `examples`-are-embedded privacy note)*
- [x] Flow doc blocks and output-data examples are authored in config, **embedded** in the document, and rendered **without `/api/source`**. *(Amendment A schema round-trip + config embed test + Amendment C component/browser tests; `example_too_large` / `example_not_utf8` / `missing_doc_file` diagnostics tested)*
- [x] Overrides attach extra documentation that **appends to, and never replaces,** discovered docs, leaving `documented` (and therefore coverage) untouched. *(Amendment B merge tests)*
- [x] Manual entity ids are globally unique across the config set and referenceable from any flow file. *(duplicate_ids test naming both locations; cross-file reference test)*
- [x] The `SchemaVersion` 1.0 → 1.1 bump is additive and backward-compatible. *(schema round-trip of a 1.0 document; server regression test loading a committed 1.0 snapshot; `jsonschema_drift`; `check:types`)*
- [x] Localization-ready labels supported without full UI translation. *(label_translations parsed into LocalizedText; test)*
- [x] Configuration loading is deterministic. *(determinism test)*
- [x] Tests cover merge precedence and conflict behavior. *(merge_precedence test)*

Verification:

```bash
# Amendments A/B (schema + graph), then the config crate itself
cargo test -p cratevista-schema --all-features        # View docs/examples, 1.0 back-compat, drift
cargo test -p cratevista-graph  --all-features        # EntityOverride::docs append semantics
cargo test -p cratevista-config --all-features
cargo test --workspace --all-features                 # incl. the 1.0-snapshot server regression

# Amendment C (frontend rendering) — web/dist is committed, so order matters
cd web && npm run check:types && npm run test && npm run check:dist \
       && npm run build && cd .. && cargo build -p cargo-cratevista \
       && cd web && npm run e2e && npm run check:embed-rebuild && cd ..

# End to end
cargo run -p cargo-cratevista -- cratevista generate            # with .cratevista/ fixture
cargo run -p cargo-cratevista -- cratevista generate --no-config
```

## Manual-content model

CrateVista's TOML config must express **manual** content — manual entities, flows,
stages, typed labeled relations, overrides, localization, and examples — and merge
it with the **generated** Rust structure. The decisions below define that model.

1. **Config concepts**
   - **Manual entities + flows + stages + typed labeled relations + examples + doc blocks**, all referenced by **stable id**, expressed in TOML and mapped to schema `Entity`/`View`/`Stage`/`Relation`.
   - **Flows reference discovered items by stable id** — the discovered entity ids from issue 02.
   - Edge labels like HTTP/WebSocket/SQL/Redis are carried as relation labels/attributes.

2. **Localization**
   - Localization is **by stable id**, inline via `LocalizedText`/`label_translations` — not a duplicated parallel data file.

3. **Validation**
   - Config validation covers referenced files, example ids, edge endpoints, and dangling references (the `validate` path).

4. **Config maps to schema as follows**
   | config | schema |
   |---|---|
   | `[[entity]]` | `Entity{provenance=Manual}` |
   | `[[flow]]` + `[[flow.stage]]` | `View` (explicit `entity_ids` membership) + `Stage`s; `View` has no `provenance` field |
   | flow membership (by id) | references to discovered/manual stable ids |
   | `[[flow.relation]]` label/attributes | `Relation` |
   | `[[override]]` category/tags/presentation | presentation enrichment of a discovered entity |
   | `[[override]]` label/description | localized edits with identity preserved |
   | `label_translations` | `LocalizedText` |
   | override docs / flow examples | validated repo-relative paths |

5. **Visual/interaction acceptance criteria**
   - A configured flow reproduces a Clients → Gateway → Services → Infrastructure lane pattern, mixing manual (external systems) and discovered (crates/modules) entities in one view with labeled edges and a default focus.
   - Overridden labels/categories change presentation only; the entity stays selectable by its stable id and keeps its discovered kind/qualified name.

6. **Screens/states the explorer must support**
   - A manual flow appears as a selectable view tab with an ordered stage timeline whose steps highlight the referenced discovered + manual entities.

7. **CrateVista visualizes generated Rust data**
   - Config **enriches and augments** a generated document; it does not author the whole graph. Overrides target **generated** stable ids and must not alter identity.
   - Discovered entities are generated; config only enriches or adds. Manual entity/relation kinds use the open schema (with a `manual` kind for authored edges), not a fixed subtype enum.
   - Localization is inline (`LocalizedText`) rather than a duplicated data file.

## Open questions

**Resolved**

- *Format/locations* — TOML for MVP, in `cratevista.toml` + `.cratevista/flows/*.toml` + `.cratevista/overrides/*.toml`; dependency trade documented in FR1; matches the `init` template already shipped.
- *Where referential validation lives* — **resolved by the repository, not by choice.** Issue 05 **already** validates config references against discovered ids (`sanitize_views`, `drop_dangling_relations`, `apply_overlay`) with warnings on `diagnostics[]`. `cratevista-config` therefore validates only config-internal concerns and does **not** take an id set. (The draft's `&ResolutionIndex` type does not exist.)
- *Tool-setting binding* — **deferred out of this PRD** (see "## CLI/API/configuration changes"): not required by issue 08, and it would alter already-implemented PRD-03/04/06 behaviour.

**Locked on 2026-07-16 — no blocking questions remain**

1. **Flow docs/examples** — **locked (option b):** typed optional `View::docs`/`View::examples` are added to the schema as an **additive PRD-02 amendment**, bumping `SchemaVersion` **1.0 → 1.1**, and example **contents are embedded** in the document so they render **without `/api/source`**. A small **PRD-07 rendering increment** (Amendment C) is required so the data is not dead. See "## Required amendments to PRDs 02 / 05 / 07".
2. **Override extra documentation** — **locked:** `EntityOverride::docs: Option<DocBlock>` is added as an additive **PRD-05 amendment**, with **deterministic append semantics that preserve discovered documentation** (discovered markdown first, manual appended after one blank line; `summary` and `documented` preserved — configuration can never inflate coverage).
3. **Manual entity declaration site** — **locked:** `[[entity]]` stays **inside flow TOML files** for the MVP. Ids are **globally unique across the complete config set** and **may be referenced from any flow file**; references always use the full entity id (`manual:<id>`).

**Non-blocking, deferred by design** (recorded so they are not rediscovered as gaps):

- Binding the reserved `[metadata]`/`[rustdoc]`/`[server]` sections — deferred; not an issue-08 capability (see "## CLI/API/configuration changes").
- A dedicated `.cratevista/entities/*.toml` location — can be added additively later without breaking MVP files.
- The `version` key in `cratevista.toml` — parsed-and-ignored for the MVP.

## Traceability

Issue-08 checkboxes → tests above. Produces `cratevista_graph::GraphOverlay` for issue 05 (whose `apply_overlay`/`sanitize_views`/`drop_dangling_relations` already handle referential sanitation); manual flows rendered by the **completed** issue-07 UI with no frontend change (see "### Frontend compatibility"); diagnostics served by issue 06 via `/api/diagnostics`; config files watched by issue 09; example documented for issue 10 README.

## Review record

- Reviewed at: 2026-07-16 (against PRDs 01–07 Implemented / Verified, and the real repository)
- **Finalized at: 2026-07-16** — the three blocking decisions were locked by the maintainer and written up as Amendments A/B/C.
- Result: **Approved — safe to implement.** (First pass: *Changes required*; the repository-state errors below were corrected in place and the blockers are now resolved.)
- Finalization record:
  - **Locked decision 1** → Amendment A: typed `View::docs`/`View::examples` + `ViewExample`, `SchemaVersion` **1.0 → 1.1**, contents **embedded** (no `/api/source`), plus the PRD-07 rendering increment (Amendment C). Back-compatibility was **verified against the code**: the server gates on major and requires document/diagnostics version equality (both come from `CURRENT`, so a 1.1 run matches, and committed 1.0/1.0 fixtures still load); the frontend gates on `SUPPORTED_SCHEMA_MAJOR = 1`; the committed JSON Schema types `SchemaVersion` as an unconstrained string, so 1.1 and existing 1.0 fixtures both validate. **No fixture regeneration is forced.** One latent trap found and scheduled: `router.rs` asserts the literal `"1.0"` and must assert `SchemaVersion::CURRENT`.
  - **Locked decision 2** → Amendment B: `EntityOverride::docs` with append semantics (discovered markdown first, manual after one blank line; `summary` preserved). Added a rule the decision implies but did not state: **`documented` is never changed by an override**, because it feeds `coverage::compute_coverage` and configuration must not be able to inflate a project's documentation-coverage number.
  - **Locked decision 3** → FR3: `[[entity]]` stays in flow files; ids are globally unique across the config set and referenceable from any flow file. Specified the consequence: the validator builds the global manual-id set across all flow files **before** checking references (so resolution never depends on file order), and **references always use the full entity id** (`manual:<id>`), keeping one unambiguous namespace.
  - **New consequence documented:** embedding example contents means they are served by `/api/document` and included in static exports **regardless of `--source`**. This remains opt-in (a maintainer must name each file in committed config), but it is called out explicitly, capped at **64 KiB per example** with `example_too_large` / `example_not_utf8` diagnostics, and must be stated beside the `examples` key in `docs/configuration.md`.
- Major findings from the first pass (all corrected in place):
  - **"Current repository state" was substantially wrong.** `cratevista-config` has **no placeholder** and is not a workspace member (the crate must be created and registered). The seam is **`cratevista_graph::GraphOverlay`**, not `ConfigOverlay` — a type that exists nowhere — and its real fields are `entities`/`relations`/`overrides`/`views`, not `manual_entities`/`manual_relations`/`flows`. Kinds are **open newtypes** (`EntityKind(String)`/`RelationKind(String)`), so the claimed manual/external kind variants and `RelationKind::manual` do not exist and are not needed.
  - **Referential validation was already implemented by PRD 05** and would have been duplicated. `apply_overlay` (`overlay_target_missing`), `drop_dangling_relations` (`dangling_relation`) and `sanitize_views` (`invalid_view_reference`) already drop and diagnose config references to unknown discovered entities. `cratevista-config::validate` is re-scoped to **config-internal** checks only. The draft's `&ResolutionIndex` parameter referenced a **non-existent type**.
  - **`View` has no `provenance` field**, so "flows → `View` with `provenance=Manual`" was impossible; manual character is carried by member entities' `Provenance::Manual`.
  - **Edge labels are `Relation::label`, not attributes.** The shipped PRD-07 adapter renders `relation.label` and ignores label-ish attributes, so the draft's "labels as attributes" would have produced unlabelled edges.
  - **Override capability list exceeded `EntityOverride`.** Real fields are `label`/`description`/`add_tags`/`set_attributes`/`hidden`; `category`/`stage`/`promoted`/`presentation` map onto `set_attributes` (`stage` genuinely *is* an attribute — the UI reads `attributes["stage"]`), but **"extra documentation" has no path at all** → blocking decision 2 (proposed: additive `EntityOverride::docs`, a small PRD-05 amendment).
  - **Flow doc blocks / output-data examples have no schema home and no renderer** — `View` lacks docs/examples fields and the completed PRD-07 UI renders neither view descriptions nor view docs → blocking decision 1.
  - **Tool-setting binding was scope leak.** Binding `[metadata]`/`[rustdoc]`/`[server]` is not an issue-08 capability and would change already-implemented PRD-03/04/06 behaviour; deferred, with the sections reserved (matching the shipped `init` template).
  - **Frontend compatibility verified against the shipped UI, not assumed:** explicit `View::entity_ids` membership, data-driven view tabs, unknown-kind fallback + legend, `Relation::label` edge labels, stage lanes via `attributes["stage"]`, and `default_focus` all work with **no frontend change**. `View::description` and flow docs/examples are **not** rendered.
  - **No `ExplorerDocument` schema change is required** for the core capability — manual entities, flows, overrides, stages, edge labels and membership all already have schema homes. *(Superseded in part: Open question 1 was later answered **(b)**, so **Amendment A** did add `View::docs`/`View::examples` and bumped **`SchemaVersion` 1.0 → 1.1** — additively, and now landed. The core capability still needs no schema change; only flow docs/examples did.)*
  - **`ISSUES/issue_11_source_path_duplication.md` is orthogonal and not a blocker.** Config paths never pass through the defective `map_span`; the PRD now forbids using overrides to patch that defect (which would entrench a tool bug in user config) and forbids imitating the faulty package-root join.
  - Verified as accurate: TOML/ADR-0007 choice (`0007` is free), the canonical file locations (they match the shipped `init` template), `RepoRelativePath` for doc/example paths, `--no-config` being genuinely absent, `docs/configuration.md` being genuinely absent, and both verification command forms (`-- generate` and `-- cratevista generate`) working.
