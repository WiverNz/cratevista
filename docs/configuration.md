# Configuration

CrateVista works with **zero configuration**. Configuration is how you add what
`cargo metadata` and rustdoc cannot know: the external systems your code talks
to, and the runtime flows that connect them.

Everything here is optional. An unconfigured project generates exactly the same
document it always did.

## Where it lives

```text
cratevista.toml                 # tool settings (reserved; see "Reserved sections")
.cratevista/
  flows/*.toml                  # manual entities + flows
  overrides/*.toml              # presentation overrides of discovered items
  docs/*.md                     # Markdown referenced by flows/overrides
  examples/*                    # example files referenced by flows
```

Files are discovered at the **workspace root** — the directory holding the
workspace `Cargo.toml` that `cargo locate-project --workspace` reports — and
loaded in **sorted file-name order**, never in
filesystem order — so which file wins a conflict depends on its name, not on
your disk. Only `*.toml` files directly inside `flows/` and `overrides/` are
read; nested directories are ignored.

`docs/` and `examples/` are conventions, not requirements: a `path` or `docs`
entry may point anywhere inside the workspace.

Everything is TOML, so comments and reviewable diffs come for free. Why TOML and
not YAML is recorded in [ADR-0007](adr/0007-config-format-toml.md).

## Manual entities

Declared with `[[entity]]` inside any flow file.

```toml
[[entity]]
id = "web-client"
kind = "external_system"
label = { default = "Web client", de = "Web-Client" }
description = "The browser SPA."
tags = ["client"]

[[entity]]
id = "api-gateway"
kind = "manual_block"
label = "API gateway"
description = "Authenticates and fans out."

  [entity.attributes]
  tier = "edge"
  replicas = 3
```

| Key | Required | Notes |
| --- | --- | --- |
| `id` | yes | Config-local. Becomes the entity id **`manual:<id>`** |
| `kind` | yes | Any string — kinds are open (see below) |
| `label` | yes | Plain string or a translation table |
| `description` | no | Plain string or a translation table |
| `tags` | no | Sorted and de-duplicated |
| `attributes` | no | Free-form presentation data |
| `source` | no | Repo-relative path to where this lives, if anywhere |

**Ids are globally unique across the whole configuration**, not per file — and a
manual entity declared in one flow file may be referenced from **any other**.
Declaring the same id twice is an error naming both locations; the first wins.

Write `id = "redis"`, not `id = "manual:redis"` — the prefix is added for you.
**References always use the full id** (`manual:redis`), which keeps manual and
discovered ids in one unambiguous namespace.

**Kinds are open strings.** `external_system`, `infrastructure` and
`manual_block` are conventions, not an enum — any string works, and the explorer
renders an unfamiliar kind through a generic style and lists it in the legend.

## Flows

A flow is a curated view that mixes manual and discovered entities.

```toml
[[flow]]
id = "clients-to-infra"
title = "Clients → Gateway → Services → Infrastructure"
description = "How a checkout request reaches storage."
members = [
  "manual:web-client",
  "manual:api-gateway",
  "package:demo",          # a DISCOVERED entity, by its stable id
  "manual:postgres",
]
default_focus = "manual:api-gateway"
docs = [".cratevista/docs/checkout.md", ".cratevista/docs/scaling.md"]
```

| Key | Required | Notes |
| --- | --- | --- |
| `id` | yes | Becomes the view id **`view:<id>`** |
| `title` | yes | Plain string or a translation table |
| `description` | no | Shown above the graph |
| `members` | no | Full entity ids, in the order you want them |
| `default_focus` | no | The entity the explorer focuses on load |
| `docs` | no | Repo-relative Markdown paths, joined in order |

Membership is **explicit**: a flow shows exactly the entities you list, unlike
the eight generated views, which select by kind. Member order is preserved.

To find a discovered entity's stable id, look in
`target/cratevista/document.json` after a `cargo cratevista generate`, or click
the entity in the explorer — the id is in the URL.

### Stages

Stages are the ordered lanes of a flow.

```toml
  [[flow.stage]]
  id = "clients"
  title = "Clients"
  order = 1

  [[flow.stage]]
  id = "gateway"
  title = "Gateway"
  order = 2
```

`order` is authoritative — declare stages in any order you like. Both `id` and
`order` must be unique within the flow: a repeated `order` would make lane
placement depend on declaration position, which is exactly what `order` exists
to avoid. A stage id becomes **`stage:<id>`**.

Assign an entity to a stage with the `stage` key on an
[override](#overrides), or an entity `attributes.stage`.

### Relations

```toml
  [[flow.relation]]
  from = "manual:web-client"
  to = "manual:api-gateway"
  role = "http"
  label = "HTTPS"

  [[flow.relation]]
  from = "manual:web-client"
  to = "manual:api-gateway"
  role = "ws"
  label = { default = "WebSocket" }
```

| Key | Required | Notes |
| --- | --- | --- |
| `from`, `to` | yes | Full entity ids; either may be manual or discovered |
| `role` | no | **Part of the relation's identity** — see below |
| `kind` | no | Defaults to `manual` |
| `label` | no | What the edge shows (`HTTPS`, `SQL`, …) |
| `attributes` | no | Free-form, non-label metadata |

**`role` is not decoration.** A relation's identity is `kind + from + to` plus an
optional role, so **two edges between the same pair need distinct roles** or they
collapse into one. The example above is exactly that case: one HTTPS edge and one
WebSocket edge between the same two entities, kept apart by `role`. Omit the
roles and you get a `config_duplicate_relation` diagnostic telling you to add
one — the second edge is dropped rather than silently merged.

Use `label` for what a reader sees and `role` for what makes the edge distinct.
They are often related (`role = "http"`, `label = "HTTPS"`) but they are not the
same field, and a label alone will not disambiguate two edges.

#### Active-flow animation

A manual relation may opt into an **active-flow** presentation, in which the
explorer animates the edge's dashes travelling from source to target. This is a
navigation cue for an ordered or in-progress path through a manual flow — it does
**not** imply runtime traffic, and it never changes the relation's meaning or
identity.

Opt in with a single presentation attribute:

```toml
  [[flow.relation]]
  from = "manual:web-client"
  to = "manual:api-gateway"
  role = "http"
  label = "request"

  [flow.relation.attributes]
  flow = "active"
```

- The exact contract is `attributes.flow = "active"` (the string `"active"`); any
  other value, type, or a missing attribute leaves the relation **static**.
- Only **manual** relations are eligible. A discovered relation carrying the same
  attribute stays static.
- Absent this attribute, every relation renders exactly as before — existing
  documents are unchanged.
- Motion respects `prefers-reduced-motion` (it becomes a static, distinctly-dashed
  edge), and is automatically suppressed for a view with a very large number of
  active-flow relations; in both cases direction stays clear from the arrow, the
  distinct dash pattern, and the label.

### Examples

```toml
  [[flow.example]]
  id = "request"
  title = "Example request"
  path = ".cratevista/examples/request.http"
  language = "http"
  description = "What the web client sends."

  [[flow.example]]
  id = "response"
  title = "Example response"
  path = ".cratevista/examples/response.json"
  language = "json"
```

| Key | Required | Notes |
| --- | --- | --- |
| `id` | yes | Unique within the flow |
| `title` | yes | The disclosure's heading |
| `path` | yes | Repo-relative path; its **contents are embedded** |
| `language` | no | A display hint only — never interpreted or executed |
| `description` | no | Prose shown above the content |

Examples render in declaration order, because a sequence is usually a story
(request, then response). Each is a collapsible section in the explorer.

> **Read [Embedded content](#embedded-content-and-privacy) before pointing
> `path` at anything.** Example contents are copied into the generated document.

## Overrides

Overrides enrich a **discovered** entity's presentation. They live in
`.cratevista/overrides/*.toml`.

```toml
[[override]]
target = "package:demo"
label = "Demo service"
description = "The Rust service behind the gateway."
tags = ["core"]
category = "service"
stage = "stage:services"
promoted = true
docs = [".cratevista/docs/demo-notes.md"]

  [override.presentation]
  accent = "orange"
```

| Key | Effect |
| --- | --- |
| `target` | The discovered entity's stable id |
| `label`, `description` | **Replace** |
| `tags` | **Added** to the discovered tags (never removed) |
| `docs` | **Appended after** the discovered documentation |
| `hidden` | Hides the entity in default views |
| `category`, `stage`, `promoted` | Set as presentation attributes |
| `presentation` | Free-form attributes |

**An override can never change what something *is*.** There is no key for an
id, kind, parent or source — not by policy but by construction: the type has no
such fields. A rename is a label, never an identity change.

**`docs` appends; `label` replaces.** That asymmetry is deliberate: documentation
is additive enrichment, a label is a substitution. Your prose lands *after* the
item's rustdoc, separated by a blank line.

**An override never marks an item as documented.** Documentation coverage
measures *Rust* documentation, so adding prose in a TOML file cannot move your
coverage number — an undocumented item with manual notes stays counted as
undocumented. This is intentional: coverage you can edit is coverage you cannot
trust.

Overriding an id that does not exist is reported (`overlay_target_missing`) and
ignored; it never fails the run.

## Precedence

- **Files load in sorted name order.** `a_first.toml` before `z_last.toml`.
- **Duplicate entity or flow id** → an error naming both locations; the **first**
  declaration wins.
- **Two overrides of the same target** → merged **field by field, last loaded
  wins**. Fields only the earlier one set survive; `tags` from both are unioned.
  A `config_duplicate_override` diagnostic tells you it happened.
- **Merge order overall**: discovered data < overrides < manual additions.

Identical input always produces an identical document, byte for byte.

## Localization

Any label, title or description takes either a plain string or a translation
table:

```toml
label = "Redis"
# or
label = { default = "Redis", de = "Redis-Cache", ru = "Редис" }
```

`default` is the source language and the fallback. Other keys are language codes.
The data is localization-**ready**: the explorer resolves the active language and
falls back to `default`. Translating the UI itself is not part of this release.

## Diagnostics

Configuration problems are **warnings, never failures**. A broken file costs its
own contents; everything else still loads, the document is still generated, and
the exit code is still `0`. They appear in `target/cratevista/diagnostics.json`,
in the CLI summary, and in the explorer's diagnostics panel.

Each carries a workspace-relative location, with line and column where the parser
can give one:

```text
.cratevista/flows/broken.toml:1:6: key with no value, expected `=`
```

| Code | Meaning |
| --- | --- |
| `config_parse_error` | The file is not valid TOML |
| `config_invalid_structure` | Missing/unknown key, or a wrong type |
| `config_invalid_id` | An empty or malformed id |
| `config_duplicate_entity_id` | The same `[[entity]]` id twice |
| `config_duplicate_flow_id` | The same `[[flow]]` id twice |
| `config_invalid_stage` | Duplicate stage id, or duplicate `order` |
| `config_unknown_manual_reference` | A `manual:` id nothing declares |
| `config_duplicate_relation` | Two edges deriving one id — add a `role` |
| `config_duplicate_override` | The same target overridden twice |
| `config_invalid_source_path` | An entity `source` is not repo-relative |
| `config_invalid_file_path` | A `docs`/`path` entry is not repo-relative |
| `config_missing_file` | A referenced file does not exist |
| `config_not_a_file` | A referenced path is a directory |
| `config_path_escapes_workspace` | It resolves outside the workspace |
| `config_not_utf8` | A doc file is not valid UTF-8 |
| `config_example_not_utf8` | An example file is not valid UTF-8 |
| `config_example_too_large` | An example is over the size limit |
| `config_read_failed` | A file could not be read |

An **unknown key is an error, not a hint** — `titel = "…"` is reported rather
than silently ignored, so a typo cannot quietly cost you a title.

**Ids of discovered entities are not checked by the config loader.** A member,
endpoint, focus or target naming something that does not exist is reported by the
graph builder instead, as `invalid_view_reference`, `dangling_relation` or
`overlay_target_missing`. Same outcome, different reporter — there is only one
place that knows what actually exists.

## Embedded content and privacy

**Contents of `docs` and `[[flow.example]].path` are copied into
`document.json`.** That means they are:

- served by **`/api/document` to every client of the local server**;
- included in a **static export**;
- **regardless of `--source`.**

`--source` gates the on-demand `/api/source` endpoint. It does **not** gate
document contents, because embedding is what lets the explorer show an example
with no server round-trip and lets an exported site work on its own.

This does not weaken CrateVista's source-privacy rule: nothing is embedded unless
you **name that file explicitly** in committed configuration. It is opt-in, per
file, and visible in a diff.

> **Do not reference files containing secrets, credentials, tokens or private
> data from `docs` or `[[flow.example]].path`.** Only the file you name is read —
> nothing is globbed and no directory is walked — but the file you name *is* read
> in full and published.

### Size limits

- **Examples are capped at 64 KiB each.** An oversize example is **dropped
  whole, never truncated** — a half-shown example would misrepresent the file —
  and reported as `config_example_too_large` with the real size. The cap is far
  below `/api/source`'s 1 MiB because embedded content ships on *every*
  `/api/document` fetch, not on demand.
- **Markdown `docs` are currently uncapped.** *This is a known limitation, not a
  guarantee.* The same reasoning behind the example cap applies to a large
  Markdown file, so a future release may cap docs too. Do not rely on being able
  to embed an arbitrarily large document; keep flow docs to prose.

## Path, symlink and UTF-8 safety

Every referenced path is checked in layers, because no single layer is enough:

1. **It must be repo-relative.** Absolute paths, drive letters, UNC paths and any
   `..` are rejected outright (`config_invalid_file_path`). This is textual — it
   never touches the disk.
2. **It is then resolved, following symlinks.**
3. **The resolved file must still be inside the workspace.** A symlink's *path
   text* is perfectly innocent, so only resolving it reveals an escape; one that
   points out of the project is refused (`config_path_escapes_workspace`). A
   symlink that stays inside is fine.
4. **It must be a regular file** — a directory is not content.
5. **It must be valid UTF-8.** Decoding is strict: a file with invalid bytes is
   refused rather than lossily mangled into replacement characters.

If the workspace root itself cannot be resolved, every read is refused rather
than assumed safe.

## `--no-config`

```bash
cargo cratevista generate --no-config
cargo cratevista open --no-config
```

Ignores configuration entirely: nothing is discovered, **no file under
`.cratevista/` is opened**, and the output is pure discovered content — no manual
entities, flows or overrides, and no configuration diagnostics even if a file is
malformed. It is byte-identical to what an unconfigured project generates.

Useful for isolating whether something you see comes from your code or your
configuration.

The flag is on `generate` and `open` — the commands that generate. It is not on
`serve`, which replays artifacts that already exist.

## Reserved sections

`cratevista.toml` currently accepts, parses and **ignores** these:

```toml
version = "1"      # reserved; unused

[metadata]
include_external_deps = false

[rustdoc]
document_private_items = false

[server]
port = 7420
```

They are reserved so writing one is not an error today, and so a later release
can bind them without breaking your file. **They have no effect yet** — use the
CLI flags. Manual flows and overrides do *not* live here; they live under
`.cratevista/`.

## A complete example

The reference configuration lives in the repository and is exercised by the test
suite on every commit, so it cannot drift from the parser:

```text
crates/cratevista-config/fixtures/clients_gateway_services_infra/
```

Two more fixtures show how failures behave: `invalid_refs/` (broken references
and a malformed file) and `duplicate_ids/` (precedence).
