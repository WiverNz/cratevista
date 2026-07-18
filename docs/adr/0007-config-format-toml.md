# ADR-0007: TOML for manual flows and overrides

- **Status**: Accepted
- **Date**: 2026-07-16
- **Issue**: 08 — manual flows and overrides
- **Supersedes**: none
- **Related**: [ADR-0003 (schema versioning)](0003-schema-versioning.md),
  [ADR-0005 (relation reliability)](0005-relation-reliability.md),
  [ADR-0006 (server and security)](0006-server-and-security.md)

## Context

CrateVista discovers what `cargo metadata` and rustdoc can prove. It cannot
discover a Postgres instance, a browser client, or that a request crosses three
crates in a particular order. Issue 08 adds a way for authors to state those
things by hand.

That input is *committed, reviewed, hand-written source*, sitting next to
`Cargo.toml` and living as long as the code does. The format is therefore a
long-term interface, not an implementation detail.

## Decision

**Configuration is TOML**: `cratevista.toml` at the workspace root for tool
settings, and `.cratevista/flows/*.toml` and `.cratevista/overrides/*.toml` for
manual content.

**Content is split across many files, not one.** Flows are the unit of
authorship and the unit of merge conflict; one file per flow keeps two people
adding two flows from touching the same lines. Files load in **sorted file-name
order** so that precedence is a property of the repository, not of the
filesystem's enumeration order.

**Markdown and examples are referenced by path, not inlined.** Prose belongs in
`.md` where an editor will render it and a reviewer will read it as prose;
examples belong in `.http`/`.json` files where they are syntax-highlighted and
can be linted. Neither belongs escaped inside a TOML string.

## Why TOML

**It is already the language of the project.** Every Rust developer editing
CrateVista's config has `Cargo.toml` open in another tab. That is not a
tie-breaker; it is the argument. A format you already know how to write, and
whose parser is already in the dependency tree via Cargo's own ecosystem, has no
adoption cost.

**Rejected: YAML.** Significant whitespace makes a deep structure easy to get
subtly wrong, and the type coercion rules are a genuine hazard — `no` and `on`
become booleans, and a version like `1.10` becomes a float. For a file where a
mistyped key silently changes an entity's identity, "the obvious reading is the
actual meaning" matters more than compactness.

**Rejected: JSON.** No comments. Configuration explaining *why* an entity exists
is most of the value; a format that cannot carry a rationale is disqualified.

**Rejected: RON, KDL, Dhall.** Each is a better fit in isolation for some part of
the problem, and each would require every contributor to learn a format they use
nowhere else, to save syntax we get for free.

**The cost we accept**: TOML's array-of-tables syntax (`[[flow.stage]]`) is
awkward for deeply nested structures, and our flow files are nested. We keep the
nesting shallow — entity, flow, stage, relation, example, override, and no
deeper — which the format handles cleanly.

## Consequences

**Unknown keys are errors** (`deny_unknown_fields`). A silently ignored `titel`
is a typo that costs a title and reports nothing; the whole point of a
hand-written format is that mistakes surface. The cost is that we cannot add a
key without a version story — accepted, because the reserved sections in
`cratevista.toml` already parse-and-ignore, which is where growth will happen.

**Every problem is a warning, never a failure.** A malformed config file costs
its own contents; the rest still loads and generation still succeeds with exit
`0`. Configuration is an *enrichment* of a document that is fine without it, so
letting a typo in a presentation file block a build would invert the priority.

**Diagnostics carry a file, line and column.** `serde_spanned` preserves source
spans through deserialization, which is the reason parse errors can point at a
line rather than name a file and shrug. `DocumentDiagnostic` has no location
field, so the location is prefixed onto the message in the standard
`file:line:col:` form rather than forcing a schema amendment.

**Ids are global across the config set, not per file.** A flow in one file may
reference a manual entity declared in another; that is the feature. It follows
that a duplicate id is a diagnostic naming both sites, and that splitting files
is an organizational choice, not a namespacing one.

**Validation is split, deliberately.** `cratevista-config` checks only what it
can see by itself: syntax, structure, ids, internal references. Whether
`package:demo` exists is unknowable to a TOML parser, so references to discovered
entities are sanitized by the graph builder (PRD 05), which is the only component
that knows what was discovered. This keeps the dependency arrow pointing one way
(`cratevista-config → cratevista-graph`) and keeps one owner per rule.

**Referenced file contents are embedded in the document** and therefore published
by `/api/document` and static exports regardless of `--source`. This is what lets
an exported site stand alone. It stays consistent with the source-privacy rule
because nothing is read unless a committed file names it explicitly — opt-in, per
file, visible in review. Examples are capped at 64 KiB; Markdown docs are
currently uncapped, which is a known limitation recorded in
[docs/configuration.md](../configuration.md), not a guarantee.

**An override cannot change identity.** `EntityOverride` has no id, kind, parent
or source field, so "rename" can only ever mean a label. Enforced by the type,
not by review.

**`docs` appends, everything else replaces.** Manual prose lands after discovered
rustdoc, and an override never sets `documented` — coverage measures Rust
documentation, and a number you can edit in a TOML file is a number nobody can
trust.

## Alternatives considered

**Configuration inside `Cargo.toml` under `[package.metadata.cratevista]`.** A
real convention with real precedent, and it avoids a new file. Rejected: flows
are workspace-level, not package-level, and `Cargo.toml` is a build manifest —
burying a dozen flows in it would harm the file people actually need to read.
`cratevista.toml` remains the place for tool settings that are genuinely global.

**A single `cratevista.toml` holding everything.** Simpler discovery, one file to
find. Rejected on collaboration: every flow addition would contend for the same
file.

**Attributes on Rust items (`#[cratevista::flow(...)]`).** Documentation next to
the code it describes, checked by the compiler. Rejected: it would make a
visualization tool a build dependency of the crates it visualizes, and most
manual entities (a database, a browser) have no Rust item to attach to.

**Deriving flows from code.** Out of scope by decision — CrateVista does not
infer call graphs, and a guessed flow presented as fact is worse than no flow.
