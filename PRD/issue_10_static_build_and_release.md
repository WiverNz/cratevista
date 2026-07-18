# PRD — Static site build, packaging, documentation, and public release

## Status

**Approved — safe to implement** (2026-07-17). Finalized and approved against the
implemented repository (PRDs 01–09 Implemented / Verified). No open questions
remain; every decision below is locked, including the two accepted maintainer
defaults: the committed frontend bundle is relocated to
`crates/cratevista-server/embedded/`, and the release builds both macOS targets
(`aarch64-apple-darwin` and `x86_64-apple-darwin`). The implementation plan is
recorded in `docs/adr/0009-static-build-and-release.md` (**Proposed** — it becomes
Accepted only when PRD 10 is Implemented / Verified). This PRD still performs **no**
publication, release creation or announcement.

## Source issue

`ISSUES/issue_10_static_build_and_release.md`

## Summary

Deliver three things without overclaiming any of them:

1. **`cargo cratevista build`** — a self-contained static site (`index.html` +
   embedded assets + `document.json` + `generation.json` + `diagnostics.json`)
   that any static host serves with **no running Rust server**, hostable at a URL
   root or an arbitrary subpath.
2. **crates.io publishability** — the workspace can be packaged and installed as
   `cargo install cargo-cratevista` with **no Node.js and no workspace checkout**,
   exposing the `cargo cratevista` subcommand.
3. **Release documentation and a gated release workflow** — checksummed
   cross-platform release archives (SHA-256), complete README / privacy /
   licensing / hosting / launch-checklist docs, ADR-0009. Publication and
   announcement remain **manual, gated actions this PRD prepares but never
   performs**.

The release must keep making one distinction explicit: **the CrateVista CLI
builds, tests and lints on stable Rust (1.97.1); nightly is required only at
runtime to generate rustdoc JSON for a *target* project.**

## Problem statement

CrateVista must be installable, publishable and hostable (URL root, GitHub/GitLab
Pages, CI artifact subdirectories) as an open-source tool, and clearly
differentiated from rustdoc HTML, static dependency graphs and terminal
inspectors — while never claiming a capability that is not implemented and tested.

## Goals

- `build` emits a self-contained static site that works, unchanged, from a URL
  root and from an arbitrary subpath, with no server and no per-host rebuild.
- The workspace is crates.io-publishable and `cargo install`s from the packaged
  `.crate` files alone (no workspace, no Node) on Linux, macOS and Windows.
- A tagged release workflow produces **checksummed** (SHA-256) cross-platform
  archives from a **deterministic committed frontend bundle** and **reproducible
  package file sets**; a **separate, manual** job performs `cargo publish` in
  dependency order. Compiled binaries are **not** promised byte-identical across
  builds (see "Reproducibility claims").
- Complete, accurate README + SECURITY + hosting + licensing + launch-checklist
  docs; ADR-0009.

## Non-goals

- No new analysis features; no hosted/remote ingestion; no AI summaries.
- **This PRD does not publish to crates.io, create a GitHub release, announce
  publicly, register repository topics, or permanently verify name availability.**
  It prepares those and gates them behind the launch checklist and a final
  name-availability recheck.
- **Copying source *snippets* into a public static site is out of scope** and
  deferred to a follow-up issue (see Decision 6). Repository *links* are in scope.
- No provenance/artifact signing in the first release (explicitly deferred;
  SHA-256 sums only).

## Repository baseline at approval — 2026-07-17

This section records the repository **as it was when PRD 10 was approved**. It is a
historical baseline, not a statement of current implementation — several items below
(the missing `embedded_assets()` API, the missing core `run_build`, the build CLI
stub) have since been implemented. The **implementation ledgers** (Phase 1 / 2A /
2B / 2C / 3, below) are the authoritative record of landed work.

Verified against the code on 2026-07-17.

- **`build` is the issue-01 stub.** `crates/cargo-cratevista/src/commands/build.rs`
  calls `cratevista_core::usecase::run_build()`, which returns
  `CommandFailure::unimplemented("build")` (exit code `NOT_IMPLEMENTED` = 4, code
  `"unimplemented"`). The `Command::Build` clap variant carries **no arguments**
  (its help reads "Build a self-contained static site. (not implemented yet)").
  `--output` / `--base-path` / `--include-source-snippets` **do not exist yet.**
- **`Open` already carries `--watch`** (PRD 09) plus a flattened `GenerateArgs` and
  `ServerArgs`; `Serve` carries `ServerArgs` only. This is settled and PRD 10 does
  not touch it.
- **Orchestration lives in `cratevista-core`.** `run_generate(&GenerateOptions,
  &dyn Clock) -> CommandOutcome` writes `target/cratevista/{document,generation,
  diagnostics}.json` with **prepare-then-commit** semantics (three temp files,
  per-file atomic rename, `generation.json` last as the completion marker; a fatal
  failure replaces nothing and leaves no temp files). `run_serve` / `run_open` are
  the other use cases. `usecase::run_build` is the stub PRD 10 replaces.
- **`cratevista-server::assets` cannot currently materialize a site.** The
  rust-embed struct `Assets` (`#[folder = "../../web/dist"]`) is **private**, and
  the only public function is `serve_path(path) -> axum::Response`. `is_fingerprinted`
  is public; `embedded_names()` is `#[cfg(test)] pub(crate)`. **There is no public
  API to enumerate embedded assets or write them to a directory** — PRD 10 must add
  one (Decision 2).
- **The frontend loader (`web/src/api/load.ts`) hardcodes the API routes.**
  `attemptLoad` fetches `` `${base}/api/document` ``, `` `.../api/generation` ``,
  `` `.../api/diagnostics` `` (base defaults to `""`, so absolute `/api/*`). PRD 09
  added coherent three-attempt loading, the typed `incoherent-snapshot` outcome,
  and **acceptance of an all-three-absent `X-CrateVista-Snapshot` triple** — the
  static-export rule. `ServerArtifactSource` and `ArtifactLoader` exist; there is
  **no static (relative-URL) source yet.**
- **PRD-09 static compatibility already exists and is browser-tested.**
  `web/src/api/liveReload.ts` probes `/api/health` **fail-closed** (a 404 / non-2xx
  / non-JSON / non-boolean → watch disabled → **no `EventSource`**), and
  `web/e2e/tests/static-export.spec.ts` already drives the **real built bundle**
  with no `/api/health`, no `/api/events` and header-less artifacts, asserting it
  renders and opens no `EventSource`. PRD 10 builds directly on this.
- **The frontend already uses relative URLs and query-string routing.**
  `web/vite.config.ts` sets `base: "./"`, so `index.html` references `./assets/…`;
  the SPA serializes durable state into `window.location.search` (query string),
  never into path segments. Assets and a sibling `./document.json` therefore
  resolve correctly under **any** hosting subpath **with no rewriting** (Decision 3).
- **`web/dist` is committed and reproducibility-gated.** It lives at the workspace
  root (`web/dist`), **outside** `crates/cratevista-server/`. `npm run check:dist`
  proves it matches a fresh `vite build` byte-for-byte; `npm run check:embed-rebuild`
  proves the embedded server bytes track it; `cratevista-server/build.rs` declares
  `cargo::rerun-if-changed=../../web/dist`. **Because the bundle is outside the
  server crate, `cargo package -p cratevista-server` will not include it** — the
  central publishing blocker (Decision 8).
- **CI (`.github/workflows/ci.yml`) already covers three OSes.** Jobs: `lint`
  (fmt / clippy / `+1.97.1 check` on Ubuntu), `test` (build + test on
  ubuntu/macos/windows-latest, pinned stable), `explorer` (types → typecheck ×2 →
  lint → unit → `check:dist` → build → E2E → `check:embed-rebuild` → Rust gates),
  and `install` (`cargo install --path crates/cargo-cratevista --locked` + `cargo
  cratevista --help` on all three OSes). **The install job installs from the
  workspace path, so it does not prove a crates.io-style install.** No
  `release.yml` exists. `rust-toolchain.toml` pins `1.97.1`.
- **Package metadata is already present on every crate.** All nine crates set
  `description`, `version.workspace`, `edition.workspace`, `rust-version.workspace`,
  `license.workspace` (`MIT OR Apache-2.0`), `repository.workspace`,
  `homepage.workspace`, `authors.workspace`. `cargo-cratevista` additionally sets
  `keywords` (5), `categories` (3, valid) and `readme = "../../README.md"`. **No
  crate sets `publish = false`.** `Cargo.lock` is committed.
- **Internal dependencies are path-only.** `[workspace.dependencies]` declares
  each internal crate as `{ path = "…" }` with **no `version`** — crates.io rejects
  a dependency that has no version requirement (Decision 8).
- **The dependency tree to publish.** `cargo-cratevista → cratevista-core →
  {schema, metadata, rustdoc, graph, config, server, watch}`. `cargo-cratevista`
  itself depends only on `cratevista-core` (+ `clap`).
- **Artifacts are already path-safe.** `RepoRelativePath` (schema) rejects
  absolute, drive-qualified, UNC and traversing spellings, so every `SourceLocation`
  in `document.json` is validated repo-relative. `generation.json` carries only
  counts, durations, a timestamp, `generator`, `toolchain`, `rustdoc_format_version`
  and BLAKE3 hashes — **no argv, no paths, no usernames**. `DocumentDiagnostic`
  messages are produced path-free by PRDs 03–05 (cargo/rustdoc argv lives only in
  `RustdocError`, never in an artifact). `Project` carries `repository_url` and
  `default_branch`, so repository links are feasible from existing data.
- **Existing docs.** `LICENSE-MIT`, `LICENSE-APACHE`, `SECURITY.md`, `CHANGELOG.md`
  (Keep a Changelog, `[Unreleased]`), `README.md`, `docs/NAME_AND_POSITIONING.md`,
  `docs/server.md`, `docs/configuration.md`, `docs/accessibility.md` all exist.
  **`docs/hosting.md`, `docs/launch-checklist.md` and `docs/adr/0009-*.md` do not
  exist** — PRD 10 creates them. The README lacks several issue-10 sections
  (screenshot placeholder, first-run, supported inputs, static-build example,
  known limitations); it currently documents only `cargo install --path`.

### Stale assumptions in the previous draft, corrected

- ~~"`build` reuses `cratevista-server::assets` to enumerate/write embedded
  assets."~~ **No such public API exists**; `Assets` is private. PRD 10 adds one
  (Decision 2).
- ~~"base-path rewrite of `index.html`/asset refs."~~ Assets are **already
  relative** (`base: "./"`) and routing is query-string, so **no rewriting is
  required** for any hosting target (Decision 3).
- ~~"a build-time flag/env switches the loader."~~ A build-time switch would mean
  two bundles and break single-bundle reproducibility. Mode is selected **at
  runtime** from a CSP-safe marker (Decision 4).
- ~~"Config `[build] …` reserved."~~ The root config uses `deny_unknown_fields`
  and does not accept `[build]`; "reserved" is not implementable. PRD 10 is
  **CLI-only** (Decision 5).
- ~~"`--include-source-snippets`."~~ **Deferred** to a follow-up issue; "exclude
  secrets" has no safe deterministic rule for a first release (Decision 6).
- ~~"`cargo publish -p cargo-cratevista --dry-run`" as the packaging test.~~
  Insufficient: its dependencies are unpublished, so PRD 10 specifies a
  package-and-install-from-`.crate` verification (Decision 8).
- ~~"Release/packaging lives in CI + `xtask`."~~ There is **no `xtask`** and no
  root `scripts/`. Static-build orchestration lives in **`cratevista-core`**;
  packaging/release lives in **CI** (Decision 2, Decision 9).

## Terminology

**Static site**: a prebuilt directory served by any static HTTP host, with no
Rust server. **Base path**: the URL subpath a site is hosted under (e.g.
`/cratevista/` for a GitHub project page). **Server mode / static mode**: whether
the running SPA talks to a live `/api/*` server or to sibling JSON files.

---

## Decision 1 — Output contract (LOCKED)

`build` produces exactly this directory, and nothing else is promised:

```text
<output>/
  index.html
  assets/**                 # the exact embedded bundle (byte-identical to `serve`)
  document.json             # the generated explorer document (deterministic)
  generation.json           # runtime metadata (timestamp, toolchain, tuple, partial)
  diagnostics.json          # the full generation diagnostics, unchanged
```

- Default `<output>` is `target/cratevista/site/`; `--output <dir>` overrides.
- The three JSON files are the **same verified artifacts** `run_generate` writes to
  `target/cratevista/`, copied unchanged — not re-serialized, not summarized.
- No `source/` or `snippets/` directory (Decision 6). No server, no `/api/*`.

## Decision 2 — Ownership and control flow (LOCKED)

**`cratevista-core` owns the static-build use case.** `usecase::run_build()` is
replaced by `run_build(&BuildOptions, &dyn Clock) -> CommandOutcome`.
`cargo-cratevista::commands::build` only maps CLI flags to `BuildOptions` and
calls it — **no orchestration logic in the CLI crate**, consistent with the
standing rule (ADR-0001) that core owns application orchestration.

```rust
// cratevista-core
pub struct BuildOptions {
    pub generate: GenerateOptions,   // reuses the whole generate contract
    pub output: PathBuf,             // default target/cratevista/site
    pub base_path: Option<BasePath>, // Decision 3
}
```

**The embedded bundle is the single asset source.** PRD 10 adds one public API to
`cratevista-server` so the *same* bytes `serve` sends are what `build` writes —
there is no second copy of the UI and no dependence on `web/dist` existing on disk
(so it works from an installed binary):

```rust
// cratevista-server::assets
/// Every embedded asset as (relative path, bytes). Reuses the private rust-embed
/// `Assets`; adds no new dependency and exposes no rust-embed type.
pub fn embedded_assets() -> impl Iterator<Item = (String, std::borrow::Cow<'static, [u8]>)>;
```

**Generation and materialization are separate seams.** Production composes them;
tests drive materialization alone (no cargo, no nightly):

```rust
// cratevista-core (the landed Phase-2B API)
pub fn materialize_static_site(
    artifacts: &ArtifactPaths,          // an existing target/cratevista holding the three JSON
    assets: impl Iterator<Item = (String, Cow<'static, [u8]>)>,  // embedded_assets()
    options: &SiteOptions,              // output, base_path, generated_at
    protected_paths: &[PathBuf],        // the exact generation inputs to protect
) -> Result<PublishedSite, BuildError>;

// production:
//   run_build
//     -> execute the existing generation pipeline (run_generate's shared seam)
//     -> receive committed ArtifactPaths plus the exact protected input paths
//     -> materialize_static_site(artifacts, embedded_assets(), site_options, protected_paths)
```

`materialize_static_site` **never runs cargo or nightly** — it consumes an
already-written artifact directory — so the deterministic filesystem, rollback,
base-path, privacy and ownership tests exercise it directly with a committed
snapshot. Production always reaches it via `run_generate`; **a preseeded
`target/cratevista` does not, and is not claimed to, bypass `run_generate` in
`run_build`.** The single gated full-pipeline `run_build` test uses the pinned
nightly (`nightly-2026-07-01`).

### Output ownership marker (schema, kinds, write timing)

`build` **owns only directories it created**, anchored to an ownership marker
written **before any content**. **Marker A is the first authoritative file written
into staging. A crash before its atomic commit may leave only a strict-shape P0
shell. Once marker A is authoritative, every later staging/publication state is
marker-classifiable.** The one pre-marker window (the P0 shell) is recognized by a
strict fail-closed shape and safely reclaimed by recovery; it never blocks
identification or safe cleanup (see the P0 rules under recovery).

There are **exactly three marker states** (kind ∈ {`"staging"`, `"site"`};
`output_key` distinguishes A/B from the portable published C):

```json
// A — INCOMPLETE STAGING (written first, immediately after mkdir).
{ "format": "cratevista-static-site", "version": 1, "kind": "staging",
  "output_key": "<key>", "generated_at": "<rfc3339>" }

// B — COMPLETE, NOT YET FINALIZED for publication. Valid ONLY inside
//     .cratevista-<key>-staging-*  OR transiently at <output> immediately after
//     the staging -> output rename, before finalization.
{ "format": "cratevista-static-site", "version": 1, "kind": "site",
  "output_key": "<key>", "generated_at": "<rfc3339>" }

// C — FINAL PUBLISHED SITE. Path-free and key-free, so a finished site can be
//     copied or hosted anywhere without exposing or binding it to its build path.
{ "format": "cratevista-static-site", "version": 1, "kind": "site",
  "generated_at": "<rfc3339>" }
```

- **`kind` has exactly two values: `"staging"` or `"site"`.** There is **no**
  `"backup"` kind.
  - an **incomplete staging** directory → **marker A** (`kind: "staging"` + `output_key`);
  - a **complete-but-unfinalized** directory → **marker B** (`kind: "site"` +
    `output_key`), valid only in a keyed staging dir or transiently at `<output>`
    just after the rename;
  - a **final published output** → **marker C** (`kind: "site"`, **no `output_key`**
    — portable, no build-path binding);
  - a **backup** directory is identified by its keyed `.cratevista-<output_key>-backup-*`
    **directory name** plus a valid **marker C** (a backup *is* a former finalized
    site, so its marker is not rewritten) — restoring it yields a valid site.
- **`output_key` distinguishes B from C.** Only A/B carry it; the difference
  between "published but finalization interrupted" (B at `<output>`) and "stable
  published" (C at `<output>`) is exactly the presence of a matching `output_key`.
- **Fail closed**: a marker that is missing, malformed, an unknown `version`,
  **any `kind` other than `"staging"`/`"site"`**, wrong for the context, or a
  **`output_key` that does not match** the current build's key is treated as *not
  owned / unrelated* — nothing is deleted, renamed or overwritten.

### Output identity: scoped temporaries and a per-output lock

Temporaries and the lock are **bound to one exact output** so a build for
`parent/site-a` can never inspect, delete or restore anything belonging to
`parent/site-b` in the same parent directory.

**`output_key`** is a deterministic, filename-safe token derived from **one
resolved full output identity**. It must be **stable when filesystem existence
changes** — creating the intermediate parent directories of an output must *not*
change its key. The naïve "hash the canonical existing ancestor, then a separator,
then the remainder" is **not** stable: once a build creates more components, the
nearest existing ancestor moves deeper, the separator moves, and the key changes
for the same output. So the ancestor/remainder split is resolved away *before*
hashing and never appears in the hashed bytes.

**Algorithm (LOCKED):**

1. **Lexically normalize** the absolute output path (`.`/`..`/redundant-separator
   removal; no filesystem access).
2. **Find and canonicalize the nearest existing ancestor** (this also performs
   symlink resolution/rejection).
3. **Append the normalized non-existing remainder components** to that canonical
   ancestor, producing **one resolved full output identity** (a single sequence of
   path components — the ancestor components followed by the remainder components,
   with **no marker of where the split was**).
4. **Serialize that full identity losslessly, with domain separation** —
   independent of platform separators and of the ancestor/remainder boundary:

   ```text
   serialized = b"cratevista-output-key-v1"
              || for each component c (in order):
                   u32_le(byte_length_of(c)) || raw_bytes_of(c)
   ```

   - **Unix**: `raw_bytes_of(c)` is the component's `OsStr` **raw bytes** (non-UTF-8
     preserved).
   - **Windows**: `raw_bytes_of(c)` is the component's **UTF-16 code units** encoded
     little-endian (`OsStrExt::encode_wide`), a lossless Windows representation.
   - The length prefix (`u32_le`) frames each component so boundaries are
     unambiguous **without** a separator byte, and platform separators are already
     gone (components, not a joined string).
5. `output_key = hex(BLAKE3(serialized))[..16]` — the first **16 lowercase-hex**
   characters, always filename-safe.

Because the hashed input is the *resolved full identity* (ancestor + remainder
merged, length-framed) with the split discarded, **the same resolved output yields
the same key before and after additional parent components exist**. The key need
only be stable for one **filesystem identity on one platform** — it is **not** a
cross-platform artifact identifier. It is **local bookkeeping, not a security
token** — its sole job is to prevent cross-output collisions — and it **never
appears in the published site** (marker C omits it).

**Keyed sibling names** (all under `<output>`'s parent), where `<output_key>` is
exactly **16 lowercase-hex** characters and `<nonce>` is exactly **32 lowercase-hex**
characters:

```text
.cratevista-<output_key>-staging-<nonce>     # this build's staging directory
.cratevista-<output_key>-backup-<nonce>      # a saved previous <output> for this key
.cratevista-<output_key>.lock                # this output's advisory lock
.cratevista-static-site.json.tmp-<nonce>     # a crash-safe marker temp (inside a dir)
```

**Nonce syntax (locked).** One format is used for staging, backup and marker-temp
names:

```text
nonce = exactly 32 lowercase hexadecimal ASCII characters
regex:  [0-9a-f]{32}
```

Nonces are generated from a **collision-resistant source suitable for cross-process
temporary names** — an OS-random facility (not timestamp/PID alone). **Candidate
recognition requires the exact fixed-width format**: no prefix, no additional suffix,
no uppercase hex, no shortened value and no path separator is accepted. A name that
deviates in any way is **not** a CrateVista candidate and is left untouched.

`<nonce>` distinguishes concurrent/leftover temporaries **for the same key**. A
build inspects, recovers, deletes or restores **only** candidates whose directory
name carries its exact `output_key` **and** whose staging marker's `output_key`
matches; a keyed name whose marker carries a *different* key is malformed/unowned
and is **never touched**. Temporaries for any other `output_key` are unrelated and
invisible to this build. The staging/backup candidate itself must be a **real
directory** — a candidate-directory **symlink or platform reparse-point equivalent is
unrelated and is never traversed or removed**.

**Missing output-parent preparation, then the per-output lock.** The lock file
lives under `<output>`'s **parent**, which may not exist yet (e.g. `<output> =
<workspace>/dist/site` where `dist/` does not exist). Preparing that parent chain
is the **sole permitted pre-lock filesystem preparation**. The exact locked
sequence:

1. lexically normalize `output`;
2. resolve through the nearest existing ancestor;
3. run **symlink rejection** and **protected-path safety** checks;
4. derive the stable resolved full identity and `output_key`;
5. create **only the missing parent chain** of `<output>` — **do not create
   `<output>` itself**;
6. resolve `output` **again**;
7. re-run the symlink and safety checks;
8. assert the resolved full identity and `output_key` are **unchanged** (they must
   be — the key is existence-stable by construction);
9. acquire `<output-parent>/.cratevista-<output_key>.lock` (a **cross-platform
   exclusive advisory lock**);
10. **only after the lock is held** may recovery, output inspection, staging
    creation, predecessor move/removal, publication, rollback or cleanup begin, and
    the lock is held through all of them.

Parent creation (step 5) is **idempotent** and may run concurrently in two
processes; both then contend on the **same** `output_key` lock (step 9). If
re-resolution (steps 6–8) changes the identity/`output_key` or reveals a symlink,
`build` **fails before inspecting or mutating any output state** (a symlink →
`build_output_symlink`; a changed key is an internal invariant → the internal
filesystem error, never `build_output_busy` or an ownership code).

**Lock rules:**

- **held by another process** → `build_output_busy` (runtime 1). It performs **no**
  output, staging, backup, recovery or publication mutation, and **does not inspect
  or delete candidates**; a safely created **empty parent chain may remain**, and it
  **never creates `<output>` itself**. (It is not "literally zero filesystem
  effect": the missing parent may have been prepared before the lock file could
  exist. No ownership guarantee for a non-empty `<output>` is weakened.)
- **process termination** → the OS releases the advisory lock automatically (this
  is why an advisory lock is used, not a `create_new` lock file that would stay
  locked forever after a crash);
- an **existing, unlocked** `.lock` file is reusable and does **not** block a build;
  no stale-PID guessing is performed;
- builds for **different `output_key`s** (including different nested outputs) use
  **different** locks and never block or inspect one another.

**`output_key` ownership (no caller-supplied key is trusted).** `OutputSafety`
carries an `output_key`, but Phase 2B must not trust an arbitrary key that may not
correspond to `SiteOptions.output`. The invariant: the production materialization
entry point **derives `ResolvedOutput` and `output_key` itself** (or validates
`OutputSafety.output_key` against a freshly derived key **before** opening the lock
or scanning candidates). The recommended shape is a constructor
`OutputSafety::for_output(output, protected)` with **private** `output_key` /
`protected` fields, so a test can still supply protected paths without cargo but
**cannot bind one output path to another output's key**. A mismatch is an internal
invariant / filesystem error that causes **no candidate scan or mutation**, and is
**not** mapped to `build_output_busy` or an ownership error.

**Dependency note (Phase 2):** the workspace has **no** existing cross-platform
advisory-file-lock facility, so **Phase 2 is authorized to add one minimal
cross-platform dependency** for this (e.g. `fs4`/`fs2`-style advisory locking). It
must be an OS advisory lock released on process exit — **not** a `create_new`-only
lock file. Adding it to `cargo-cratevista`/`cratevista-core` does not affect the
`cratevista-server` dependency boundary.

Before touching `<output>`, `build` resolves it, acquires the lock, and applies
these rules exactly (all scoped to the current `output_key`):

| `<output>` state | action |
| --- | --- |
| does not exist | publish normally |
| exists, **empty** directory | **adopt** as an empty predecessor — removed only *after* staging is complete, and **recreated** if the publish rename fails (see the sequence); an empty directory never becomes a backup and never gets a false site marker |
| exists, non-empty, valid **marker C** (final, no `output_key`) | stable published site → replace |
| exists, non-empty, **marker B** with a **matching** `output_key` | a publish rename completed but finalization was interrupted → **finalize** (atomically replace B with C), then treat as the stable published site (recovery below) |
| exists, non-empty, **marker B** with **another** `output_key` | **fail `build_output_marker_invalid`, modify nothing** |
| exists, non-empty, **marker A** (`kind: "staging"`) | wrong kind for a published output → **fail `build_output_marker_invalid`, modify nothing** |
| exists, non-empty, **no** marker | **fail `build_output_not_owned`, modify nothing** |
| exists, marker malformed / unknown version / other `kind` | **fail `build_output_marker_invalid`, modify nothing** |
| `<output>` **is a symlink** (itself or a symlinked ancestor component) | **fail `build_output_symlink`, modify nothing** |

**Path-safety checks** reject only outputs whose **replacement would delete
something protected**. The rule is stated in the *replacement-danger* direction —
`<output>` is dangerous exactly when publishing it (which removes whatever is at
`<output>`) would remove a protected path:

> **Reject** (`build_output_forbidden`, modify nothing) iff, for any protected
> path `p`:  **`<output> == p`  ||  `<output>` is an ancestor of `p`.**

Being a **descendant** of a protected root is **not** dangerous and is **not**
rejected: replacing `<output>` removes only `<output>` and what is under it, never
an ancestor. So `<output>` may live anywhere below the workspace as long as it does
not itself *contain* an input.

**Protected paths** are the **real, explicit generation inputs** available to core
(passed in a small core-owned safety context — see below), not a "workspace subtree
minus `target/`". At minimum:

- the root `Cargo.toml` and every workspace-member `Cargo.toml`;
- `Cargo.lock`;
- every Rust source root / `src_path` a target contributes;
- `cratevista.toml` (when config is enabled);
- discovered flow files and override files;
- explicitly referenced docs / examples;
- the three artifact files (`document.json` / `generation.json` /
  `diagnostics.json`) **and** their artifact root `target/cratevista`;
- the workspace root itself.

Checks are applied to the *lexically normalized* `<output>` and, for the ancestor
tests, resolved through its **nearest existing ancestor** canonicalized (a
not-yet-created `<output>` is checked through the ancestor that does exist, so a
symlinked parent is still caught). This is **independent of the marker-ownership
guard**: a non-empty *unowned* directory is still never replaced, whatever the path
check says.

- **`cargo cratevista build --output dist`** (i.e. `<workspace>/dist`) is
  **accepted**: `dist` is a descendant of the workspace root, equals no input, and
  is an ancestor of none — replacing it removes only `dist/`.
- The default **`target/cratevista/site/`** is **accepted**: it is a descendant of
  `target/cratevista` and an ancestor of no input (the three artifacts sit in
  `target/cratevista/`, *beside* `site/`, not under it).
- `<output>` equal to the workspace root, or to `target/cratevista`, or to any
  input, or an **ancestor** of any of them, is **rejected**.

**Core-owned safety context.** `run_build` gathers only the **protected paths**
from the real inputs the generation pipeline already discovered (the workspace root
and manifests, member manifests and target source roots, the lockfile, the config
root / flow / override / referenced files, and the artifact root plus the three
artifacts) and passes them as `protected_paths: &[PathBuf]` to
`materialize_static_site`. **The public seam derives `ResolvedOutput`, `output_key`
and `OutputSafety` internally from `options.output`** — `run_build` never
constructs or supplies a caller-controlled `OutputSafety`, and no caller-supplied
key is trusted. `materialize_static_site` performs the path checks against the
protected list and scopes every temporary/recovery operation to the derived
`output_key`; its tests supply an explicit protected set and **never run cargo**.
**`run_build` does not acquire the publication lock itself** — parent preparation,
lock acquisition, recovery and publication all happen exactly once, inside
`materialize_static_site`.

### Interrupted-publication recovery (runs first, before any new staging)

On **every** `build`, after acquiring the `output_key` lock and before creating a
new staging directory, `build` inspects `<output>` and **only the siblings carrying
its exact `output_key`** — `.cratevista-<output_key>-staging-*` and
`.cratevista-<output_key>-backup-*`. Candidates for any other `output_key` are
**enumerated out and never touched**. A directory name identifies a candidate; a
valid marker of the right `kind` **with a matching `output_key`** is what authorizes
any deletion or rename.

A `.cratevista-<output_key>-staging-*` directory can carry **marker A, marker B, or
no authoritative marker yet (P0)** — the last because `mkdir` and the marker-A
commit are separate operations:

- keyed staging + **marker A** (`kind: "staging"`, matching key) → owned
  **incomplete** staging; safe to delete per the table.
- keyed staging + **marker B** (`kind: "site"` + matching `output_key`) → owned
  **completed-but-unpublished** candidate. It is **not** malformed and **not**
  unowned; it is **never auto-published** (the build regenerates instead); it is
  deleted only **after** a valid `<output>` is confirmed or a sole backup restored.
  (A matching-key marker B **at `<output>`** — not in a staging dir — is the
  opposite case: a *completed rename* that is finalized and kept; see below.)
- keyed staging with **no authoritative marker**, satisfying the strict **P0**
  shape below → a pre-marker crash shell with no content; safe to delete per the
  table.
- a **current-output** candidate whose marker is malformed, whose `output_key`
  does not match, or which is unmarked **but fails the P0 shape** → **preserved,
  never touched**.

**P0 — pre-marker staging shell.** A directory is P0 **iff all** of the following
hold (fail-closed on any doubt):

- its name **exactly** matches `.cratevista-<current-output-key>-staging-<valid-nonce>`
  (the current build's key; a foreign/malformed name is never P0);
- it has **no** authoritative `.cratevista-static-site.json`;
- its entries are **either** none, **or** only **regular, non-symlink** files whose
  names **exactly** match `.cratevista-static-site.json.tmp-<valid-nonce>`;
- it contains **no** subdirectory, symlink, asset, artifact or any other file.

P0 may be deleted because it holds no published or user content and its exact keyed
name binds it to the current output. **Fail closed:** an unmarked keyed staging
directory containing **any** other entry is **not** P0; a marker-temp entry that is
a symlink or non-regular file is **not** P0; a foreign/malformed name is **not** P0
— such directories are **preserved and never touched automatically**. P0 is a
filesystem crash state before marker A commits, **not** a fourth marker schema.

**Marker-temp cleanup** (the `.cratevista-static-site.json.tmp-*` files):

- **an authoritative marker exists** → leftover marker-temp files are **ignored**
  while reading the authoritative marker, and may be deleted **only** once their
  directory is otherwise recognized as owned for the current output. A half-written
  marker-temp is **never** treated as malformed *authoritative* state.
- **no authoritative marker** → an empty directory, or one containing **only** exact
  regular marker-temp files, **is P0** and may be deleted.
- **any unrelated content** → **preserve the entire directory**.

All rows below count **only this `output_key`'s** candidates: "a valid backup"
means a valid backup **for this exact key**, and "multiple backups" (ambiguity)
counts **only** valid backups for this key. Another output's temporaries are
invisible.

**First, resolve `<output>`'s own marker** (all scoped to this `output_key`):

- **marker C** (final, no `output_key`) → stable published site; proceed to rows below.
- **marker B, matching `output_key`** → the post-rename/pre-finalization crash
  window: the publication rename completed but marker finalization was interrupted.
  **Atomically finalize** (replace B with C), then treat `<output>` as the stable
  published site and clean **this key's** staging/backups. The newly published
  output is **preserved**, never discarded and never rolled back to an older backup.
- **marker B, another `output_key`** → `build_output_marker_invalid`, modify nothing.
- **marker A** (`kind: "staging"`) at `<output>` → wrong kind for a published output
  → `build_output_marker_invalid`, modify nothing.

Then, for the staging/backup temporaries of **this** key:

| # | state (this `output_key` only) | recovery action |
| --- | --- | --- |
| A | `<output>` is a stable published site (marker C, or B finalized above) | Delete this key's stale staging (marker A, marker B, **or a strict-shape P0 shell** — a completed candidate is discarded, not published); delete this key's stale **backups** only **after** confirming `<output>` is valid. |
| B | `<output>` **absent**, exactly **one** valid backup for this key | **restore first**: rename that backup → `<output>` (then finalize its marker C if needed); **then** delete this key's stale staging (marker A / B / **strict-shape P0** — the completed candidate is discarded, never auto-published); only then may a new build begin. |
| C | `<output>` **absent**, **multiple** valid backups for this key | **do not guess** → `build_recovery_ambiguous` (carrying safe *relative* identifiers); **preserve every** such directory (staging of any state included) for manual recovery. |
| D | `<output>` **absent**, **no** backup for this key, this key's stale staging exists (marker A / B / **strict-shape P0**) | delete this key's staging — **including a completed marker-B candidate, which is discarded rather than published** — and proceed as a **first build**. |
| E | any **unmarked-but-not-P0 / malformed-marker / other-`kind` / mismatched-`output_key`** staging or backup (incl. a keyed staging with **marker B but a mismatched key**, or an unmarked keyed staging that **fails the strict P0 shape**) | **never** deleted or renamed automatically (left for the operator); a mismatched key is treated as another output's / unrelated. |
| F | a restoration rename (B) **fails** | `build_publish_unrecoverable`; **preserve every recoverable staging/backup path** and report safe relative identifiers. |

A keyed **staging** marker-B candidate (in `.cratevista-<key>-staging-*`, not at
`<output>`) is **never** promoted automatically: publishing a candidate whose
intended `<output>` state is unknown could resurrect a stale document, so recovery
discards it and regenerates. The one exception is a marker-B **at `<output>`** with
a matching key — that is a *completed publication rename*, so it is finalized and
kept, not discarded. Only after recovery has run does a build proceed to publication.

### Transactional publication (not fully atomic)

Replacement is **transactional with rollback**, not a single atomic operation:

```text
0. pre-lock preparation (the locked ten-step sequence), then recovery:
     a. lexically normalize and resolve <output>
     b. run symlink rejection + protected-path safety checks
     c. derive the resolved full identity and output_key
     d. create ONLY the missing parent components of <output> (never <output> itself)
     e. re-resolve <output> and re-run the symlink + safety checks
     f. assert the resolved identity and output_key are UNCHANGED
        (a change, or a newly revealed symlink, fails BEFORE any lock/scan/mutation)
     g. acquire the exclusive advisory lock  .cratevista-<output_key>.lock
          - held by another process -> build_output_busy (no scan, no mutation;
            only a safely created empty parent chain may remain)
          - held until step 7 / rollback completes; released automatically on crash
     h. only THEN run recovery (below) and publication, scoped to this output_key
   (Lock acquisition happens AFTER a possibly missing parent is prepared — never
    before — because the lock file lives in <output>'s parent.)
1. mkdir staging:  <output>/../.cratevista-<output_key>-staging-<nonce>/   (sibling, same filesystem)
     - write marker A  { kind: "staging", output_key, ... }  via write-temp -> rename
       (marker A is the FIRST authoritative file; ownership before content)
     NOTE: mkdir and the marker-temp/rename are SEPARATE operations. A crash between
       them leaves a "P0" pre-marker shell (empty, or only exact keyed marker-temp
       files) — see recovery. `mkdir` + marker-A creation are NOT atomic together.
2. write all content:
     - embedded_assets() (index.html + assets/**)
     - if base_path: write <base href> into staging/index.html   (Decision 3)
     - inject the static-mode marker into staging/index.html     (Decision 4)
     - copy the three JSON artifacts
3. atomically replace marker A with marker B  { kind: "site", output_key, ... }
     (complete; output_key is KEPT — never dropped before the rename)
4. make room for the rename target:
     - if <output> is an owned SITE (non-empty, valid marker C):
         rename it to <output>/../.cratevista-<output_key>-backup-<nonce>/  (keeps its marker C)
     - if <output> is an EMPTY adoptable directory:
         remove the empty directory  (NOT a backup — an empty dir never gets a
                                       false site marker; it holds no document to save)
     - if <output> is absent: nothing to do
5. rename staging -> <output>          (<output> now transiently carries marker B)
6. atomically replace marker B at <output> with marker C  { kind: "site", ... }  (drops output_key -> portable)
7. on success: remove the backup (if any); release the lock

rollback:
  - step 5 (staging->output) fails -> restore predecessor and fail:
      backup taken   -> rename backup -> <output>; delete staging; build_publish_failed
      <output> empty -> recreate empty <output>; delete staging; build_publish_failed
                        (if recreate fails: keep staging; build_publish_unrecoverable)
      <output> absent-> delete staging; build_publish_failed
  - step 6 (marker B -> C at <output>) fails -> PRESERVE <output> (marker B) and the
      backup; do NOT touch them; build_publish_unrecoverable. The next run finalizes
      marker B -> C safely (recovery, below).
```

- **Every marker creation/replacement is crash-safe** (steps 1, 3, 6): write a
  complete temporary marker file (`.cratevista-static-site.json.tmp-<nonce>`),
  flush/close as the platform supports, then **rename it over**
  `.cratevista-static-site.json`. The authoritative marker is **never truncated or
  overwritten in place**, so an injected marker-write failure can leave a
  half-written *temporary* file but never a partially written authoritative marker.
- **`output_key` is kept through the rename** (marker B carries it), and dropped
  **only** at step 6 when `<output>` is finalized to the portable marker C. This is
  what lets recovery tell a just-published-but-unfinalized `<output>` (B, matching
  key) from a stale one.
- **Marker states, by location:** `A` only in a keyed staging dir; `B` in a keyed
  staging dir *or* transiently at `<output>` between steps 5 and 6; `C` at a
  published `<output>` or in a backup dir.
- **The one pre-marker window (P0).** **Marker A is the first authoritative file
  written into staging. A crash before its atomic commit may leave only a strict-shape
  P0 shell. Once marker A is authoritative, every later staging/publication state is
  marker-classifiable.** The `mkdir` and the marker-temp/rename that commits A are
  **separate** filesystem operations, so this is **not** a claim that a
  CrateVista-created staging directory is immediately marked. A P0 shell is
  **conservatively recognizable** and contains **no content** (see recovery), so it is
  safely reclaimable. P0 is a filesystem crash state *before* marker A commits —
  **not** a fourth marker schema; A/B/C remain the only three marker states.
- **No mixed old/new site is ever observable** — a reader sees the whole old site,
  then the whole new site. A **brief absence** may exist between steps 4 and 5
  (two renames, not one), so this is *transactional publication with rollback*,
  **not** fully atomic across every OS.
- **On a pre-publication failure (steps 1–3)**, `<output>` is untouched — an existing
  owned `<output>` stays **byte-identical** — and staging is cleaned up under a
  precise rule that never destroys unexpected content:
  - **before marker A commits**, cleanup may remove the directory **only** when it
    satisfies **strict P0 recognition** (exact keyed staging name, a real non-symlink
    directory, no authoritative marker, empty or only exact keyed marker-temp files);
  - **after marker A commits**, the directory is **owned staging** for this key and is
    deleted on a pre-publication failure;
  - if safe cleanup **cannot be proven** (the shape is neither strict P0 nor owned
    staging), the directory is **preserved** and the internal filesystem error is
    returned **without modifying output**.
- **If restoration itself fails** — a backup rename back to `<output>` fails, an
  empty `<output>` cannot be recreated, or the step-6 finalization is interrupted —
  the recoverable directories are **preserved** and their paths reported in
  `build_publish_unrecoverable`; nothing is deleted. **No new error code** is
  introduced: `build_publish_failed` (predecessor restored) vs
  `build_publish_unrecoverable` (predecessor/finalization not restored) covers it.
- Windows: renaming onto a non-existent target (steps 4, 5) avoids the
  replace-existing-directory restriction; the staging/backup siblings share
  `<output>`'s parent so all renames stay on one volume.

- **Missing embedded bundle** cannot occur from an installed binary (rust-embed
  fails the *compile* if absent); a defensive error covers an empty bundle.

### Error codes (authoritative)

Every `build`-specific diagnostic code, its exit class, and the condition it
signals. Tests and acceptance criteria reference these names verbatim.

Each code has **one non-overlapping meaning**:

| code | exit | condition |
| --- | --- | --- |
| `build_invalid_base_path` | usage (2) | `--base-path` fails Decision-3 validation |
| `build_output_busy` | runtime (1) | the current output's `.cratevista-<output_key>.lock` is held by another process; **no** output/staging/backup/recovery/publication mutation, and **no** candidate inspection or deletion, was performed. A safely-created empty parent chain may remain; `<output>` itself is never created. |
| `build_output_not_owned` | runtime (1) | a **non-empty** `<output>` has **no** ownership marker at all |
| `build_output_marker_invalid` | runtime (1) | a marker **is present** but malformed, an unsupported version, or the wrong `kind` where a `site` marker is required. An **absent** marker is *not* this code (it is `build_output_not_owned`). |
| `build_output_symlink` | runtime (1) | `<output>` (or a symlinked ancestor component) is a symlink |
| `build_output_forbidden` | runtime (1) | `<output>` equals, or is an **ancestor** of, a protected path (workspace root, `target/cratevista`, or a real generation input) — replacing it would delete that path. A *descendant* is allowed. |
| `build_recovery_ambiguous` | runtime (1) | `<output>` absent and **multiple** valid backups exist **for this `output_key`** (recovery case C) |
| `build_publish_failed` | runtime (1) | the `staging→output` publication failed **but the predecessor state was successfully restored** — an owned site restored from its backup, an empty directory recreated, or original absence preserved |
| `build_publish_unrecoverable` | runtime (1) | rollback could **not** restore the predecessor state; **every recoverable staging/backup path is preserved** and reported via safe relative identifiers (it does **not** claim that two directories necessarily exist) |

Generation failures continue to use the existing `run_generate` codes/exit classes
unchanged.

## Decision 3 — Base path via relative URLs (LOCKED)

**Relative URLs already satisfy every required target with no rewriting.** Because
`web/dist` is built with Vite `base: "./"` and the SPA routes through the query
string, `index.html` at `<host>/<any/subpath>/index.html` resolves `./assets/…`
and `./document.json` against its own directory. This works, unchanged, for:

- URL root (`https://host/`);
- GitHub **project** Pages (`https://user.github.io/cratevista/`);
- GitLab Pages;
- arbitrary CI artifact subdirectories;
- any static host — **no Rust server**.

`--base-path` is therefore **optional** and required for **none** of the above. It
exists only as a convenience for a host that needs an absolute `<base href>`. Its
sole materialization effect is writing `<base href="<normalized>/">` into the
copied `index.html` (a CSP-safe HTML edit — **never** string replacement inside
minified JavaScript, which this PRD forbids).

- **Accepted syntax / normalization** (`BasePath::parse`):
  - `""` (or flag absent) → no `<base>` element (pure relative).
  - `"/"` → `<base href="/">`.
  - `"repo"`, `"/repo"`, `"/repo/"` → all normalize to `<base href="/repo/">`.
  - Multi-segment `"a/b"` → `<base href="/a/b/">`.
- **Rejected** (diagnostic `build_invalid_base_path`, exit 2): a scheme
  (`http:`, `https:`, `//host`, any `…://…`), a query string (`?`), a fragment
  (`#`), path traversal (`..`), a backslash, control characters, or interior
  whitespace.
- **Asset URLs**: `./assets/…` (relative, unchanged).
- **Artifact URLs**: `./document.json`, `./generation.json`, `./diagnostics.json`
  (relative, unchanged; the loader gets these via Decision 4).
- **Browser refresh**: query-string routing means a refresh re-requests
  `index.html` at the same path and re-reads the query — state survives, no server.
- **`file://`**: **not supported.** Browsers block `fetch` of `./document.json`
  over `file://`, so the site requires an HTTP static host. This is stated in
  `docs/hosting.md` and **not** claimed anywhere; no test asserts `file://` works.

## Decision 4 — Server vs static mode selection (LOCKED)

**One bundle, runtime selection, zero failed requests.** The static build injects
a CSP-safe marker into the copied `index.html`:

```html
<meta name="cratevista-mode" content="static" />
```

- The app reads it **before any fetch**. Present → **static mode**; absent (the
  server's embedded `index.html` has no such tag) → **server mode**.
- A `<meta>` tag is CSP-safe (an inline `<script>` would violate `script-src
  'self'`); adding it is an HTML edit, not JS rewriting.

**Static mode reuses the one coherent loader — no second implementation.**
`loadArtifacts` is generalized to take the three artifact URLs instead of building
`/api/*` from a base:

```ts
// web/src/api/load.ts  (endpoints, not a hardcoded base)
interface ArtifactEndpoints { document: string; generation: string; diagnostics: string; }
// server mode:  { document: "/api/document",   generation: "/api/generation",   diagnostics: "/api/diagnostics" }
// static mode:  { document: "./document.json", generation: "./generation.json", diagnostics: "./diagnostics.json" }
```

- A new `StaticArtifactSource` passes the relative triple to the **same**
  `loadArtifacts` coherence engine. The **all-three-absent header rule (PRD 09)**
  makes header-less static files load coherently; nothing new is needed there.
- **In static mode the app constructs no `LiveReload` at all.** The mode is known
  from the meta tag, so the app never probes `/api/health`, never opens an
  `EventSource`, and never requests any `/api/*` route. **The static contract is
  zero requests to any `/api/**` route** — `/api/health`, `/api/events`,
  `/api/document`, `/api/generation`, `/api/diagnostics`, `/api/source`, and any
  other. The browser E2E asserts this by **failing on any request whose path
  matches `/api/`** (a match rule, not an allow/deny list), so a future accidental
  API call cannot slip past.
- **Repository links** (Decision 6) render from `document.json`'s existing
  `project.repository_url` + `default_branch` + each entity's validated
  `SourceLocation`; they are identical in both modes. A safe **repository-root** link
  is production-reachable (`repository_url` is derived from real generation, Phase
  4B); a **source deep link** requires `default_branch`, which the current generator
  cannot supply, so deep links are not yet production-reachable (see Decision 6).

## Decision 5 — Configuration ownership: CLI-only (LOCKED)

PRD 10 adds **no `[build]` config section.** The root config
(`cratevista-config`) uses `deny_unknown_fields`, so a `[build]` table would be a
hard parse error today, and calling it "reserved" is not implementable. Build
inputs are per-invocation and host-specific (an output directory, an optional base
path), so persistent project configuration adds little value for a first release.

The CLI surface is therefore:

```text
cargo cratevista build [--output <dir>] [--base-path <path>] <generate flags…>
```

`build` also accepts the existing `GenerateArgs` (it must generate). A `[build]`
amendment to PRD 08 can be added additively later if demand appears; it is
explicitly out of scope here.

## Decision 6 — Source behavior: links in, snippets deferred (LOCKED)

Four distinct concepts, kept separate:

| concept | PRD 10 |
| --- | --- |
| **Repository source *links*** (`repository_url` + `default_branch` + validated span) | **In scope**, static-safe |
| **Local `/api/source` retrieval** | Server-only; **absent** from static sites (no server) |
| **Source *paths* recorded in the document** | Already validated repo-relative (PRD 02–04); shipped unchanged |
| **Source *snippets* copied into the public site** | **Deferred** to a follow-up issue |

- **Repository links** are **provider-aware**, built with the URL API and
  per-segment `encodeURIComponent` — never blind string concatenation of untrusted
  fields. The forge is detected from the parsed host, and only HTTPS forms whose
  deep-link layout is known produce a file link:

  | parsed `repository_url` | file link (entity with a `SourceLocation`) |
  | --- | --- |
  | GitHub HTTPS (`https://github.com/<o>/<r>`) | `…/<o>/<r>/blob/<branch>/<path>#L<line>` |
  | GitLab HTTPS (`https://gitlab.com/<o>/<r>`, incl. subgroups) | `…/<o>/<r>/-/blob/<branch>/<path>#L<line>` |
  | other **valid HTTPS** host | **repository-root link only** — never a guessed file link |
  | `ssh:`, `git:`, `git@…`, `file:`, credential-bearing (`user:pass@`), malformed | **no link** (neither file nor root) |

  - A trailing `.git` and a trailing `/` on the repository URL are normalized off
    before the link is built.
  - `<branch>` is `project.default_branch`; `<path>` is the entity's validated
    `RepoRelativePath`; `<line>` is the `SourceLocation` span start.
  - **No file link** is rendered when `repository_url`, `default_branch` or the
    entity's `SourceLocation` is missing.
  - Uses only already-safe document data, is identical in server and static mode,
    and copies **no** source into the site. (Small PRD-07 inspector coordination.)
  - **Production-reachability (corrected, Phase 4B/5A).** A safe **repository-root**
    link **is** production-reachable: `Project.repository_url` is now derived from the
    real generated document (the unanimous member `Cargo.toml` `repository`; see the
    Phase-4B ledger). The GitHub/GitLab **source deep-link** helper exists and is
    unit/component tested, and emits a link **only** when both `Project.default_branch`
    **and** a `SourceLocation` are present. **The current production generator has no
    authoritative `default_branch` source** (no config field, no git inspection), so
    **source deep links are not currently production-reachable** — only root links
    are. **No `main`/`master`/current-branch fallback is permitted**; a deep link stays
    absent until an authoritative branch source is specified in a later phase.
- **`--include-source-snippets` is removed from PRD 10 and deferred to a new
  follow-up issue (issue 13).** Rationale, per the prompt: "exclude secrets" has no
  safe deterministic rule, and copying arbitrary source into a public artifact is a
  privacy footgun for a first release. **This PRD does not claim that arbitrary
  source snippets can be made safe automatically.** The follow-up issue, if pursued,
  must specify a strict allowlist policy (explicit directory + manifest format,
  `RepoRelativePath → output URL` mapping, per-file and total byte caps,
  UTF-8/binary handling, symlink rejection, duplicate collapse, disappear-during-
  build tolerance, and a hard deny-list for `.env`, credential and common secret
  files **even when referenced**) before any snippet is copied. Repository links
  remain available meanwhile.

## Decision 7 — Diagnostics and privacy: reuse the verified artifact (LOCKED)

The static site includes the **full existing `diagnostics.json`, unchanged.** No
sanitized/summarized variant is introduced: the artifact is already privacy-safe
by construction and inventing a transform would add an unverified surface.

**What PRDs 02–09 already guarantee** (audited against the code):

- Every `SourceLocation.path` is a `RepoRelativePath`, which **rejects** absolute,
  drive-qualified, UNC and traversing spellings — no absolute path or username can
  appear in `document.json`.
- `generation.json` carries only counts, durations, an RFC-3339 timestamp,
  `generator`, `toolchain`, `rustdoc_format_version` and BLAKE3 hashes — **no
  argv, no path, no environment value**.
- `DocumentDiagnostic` messages are produced path-free by metadata/rustdoc/graph;
  cargo/rustdoc argv lives only in `RustdocError` (a terminal error type), never in
  an artifact.

**What PRD 10 must additionally test**: that a *produced site* — all three JSON
files **and** `index.html` — contains no absolute path (`C:\Users`, `/home/`,
`/Users/`), no username, no argv (`--edition`, `CARGO_HOME`, `RUSTUP_HOME`) and no
registry credential. This is a produced-artifact assertion over the real build
output, backing the existing per-crate guarantees end to end.

## Decision 8 — crates.io publication topology (LOCKED)

**All nine crates are publishable** (none sets `publish = false`), and
`cargo-cratevista` depends transitively on all of them, so the whole tree must
publish. Three concrete blockers exist today and PRD 10 fixes each:

1. **Internal deps lack version requirements.** `[workspace.dependencies]`
   declares each internal crate as `{ path = "…" }` only; crates.io rejects a
   dependency with no version. **Fix:** add `version = "0.1.0"` alongside each
   internal `path` (path is used in-workspace, version is used when published).
2. **`cratevista-server` embeds a bundle outside its own package.** rust-embed
   reads `../../web/dist`; `cargo package -p cratevista-server` includes only files
   under `crates/cratevista-server/`, so the published `.crate` would contain no UI
   and fail to compile on install. **Fix (LOCKED — maintainer default accepted):**
   relocate the embedded bundle to live **inside** the server crate — point Vite
   `outDir` at `crates/cratevista-server/embedded/`, change rust-embed to `#[folder
   = "embedded"]`, update `build.rs` `rerun-if-changed`, and repoint
   `web/scripts/check-dist.mjs`, `check-embed-rebuild.mjs`, the `check-dist`
   comparisons and the E2E harness paths. The relocated `embedded/` directory is
   version-controlled (as `web/dist` is today) so `cargo package` includes it.

   **Package file-set (LOCKED — `include` is an allowlist, not an add-list).** Do
   **not** blindly add `include = ["embedded/**"]`: a Cargo `include` list is the
   **complete** allowlist for the packaged file set, so an `include` of only
   `embedded/**` would silently drop `src/**`, `build.rs`, licences and everything
   else, producing an unbuildable `.crate`. Instead:
   1. after relocating, run `cargo package -p cratevista-server --list` **without**
      adding any `include`;
   2. if Cargo's **default** package rules already pick up the tracked
      `embedded/**` files, add **no** `include` at all;
   3. add an explicit `include` **only if required**, and then it must be a
      **complete** allowlist of every build-required file, at minimum as applicable:
      `src/**`, `build.rs`, `embedded/**`, `README*`, `LICENSE*`, and any
      `tests/**` / fixtures needed by packaged verification or read by `build.rs` /
      compile-time macros;
   4. **never** ship an `include` containing only `embedded/**`;
   5. `cargo package --list` **and** a build of the **extracted** package are the
      authoritative checks (a listed file set that still fails to compile is a
      failure).

   Apply the **same** audit to `cargo-cratevista` if an `include` is added there: its
   crate-local `README.md`, `LICENSE-MIT` and `LICENSE-APACHE` must **not**
   accidentally exclude `src/**` or the binary target. *(This is the single largest
   implementation task; it is mechanical but touches the dist/embed/E2E path
   plumbing — called out in the sequence and the risks.)*
3. **`readme = "../../README.md"` is outside `cargo-cratevista`.** `cargo package`
   will not include a file outside the crate, so crates.io would render no readme.
   **Fix:** ship a crate-local `crates/cargo-cratevista/README.md` (the packaged
   readme, which may be a trimmed install-focused copy) and set `readme =
   "README.md"`; keep the rich root `README.md` for the repository.
4. **The licence files are outside `cargo-cratevista`.** The SPDX `license =
   "MIT OR Apache-2.0"` expression does **not** cause `cargo package` to include the
   root `LICENSE-*` files, and `cargo package` never includes files outside the
   crate root. **Fix (LOCKED):** create crate-local copies
   `crates/cargo-cratevista/LICENSE-MIT` and `crates/cargo-cratevista/LICENSE-APACHE`,
   ensure they are inside the package (they sit in the crate root, so `cargo package`
   includes them; add them to `include` if `include` is set), and add a
   **drift check** (CI, and a local script) asserting each crate-local copy is
   **byte-identical** to the corresponding root `LICENSE-*`. `cargo package --list
   -p cargo-cratevista` must assert **both** files are present. Nothing relies on a
   file outside the package root being included implicitly.

**Publication order** (each crate published only after its deps are live):

```text
1. cratevista-schema
2. cratevista-metadata   3. cratevista-rustdoc   (depend on schema)
4. cratevista-graph      (schema + metadata + rustdoc)
5. cratevista-config     6. cratevista-server     7. cratevista-watch
8. cratevista-core       (all of the above)
9. cargo-cratevista      (core)
```

**Verification (documentation-and-CI, never a real publish):**

- `cargo package -p <crate> --list` for each crate, asserting the file set:
  `cratevista-server`'s list **must** contain `embedded/index.html` and the
  hashed `embedded/assets/**`; `cargo-cratevista`'s list **must** contain
  `README.md`, `LICENSE-MIT`, `LICENSE-APACHE`.
- A **package-then-install test with one locked mechanism: a Cargo
  `local-registry`.** `cargo-local-registry` builds an **offline Cargo source** — it
  is **not** a registry publishing service and has no web API, so **no publish or
  publish-`--dry-run` step is part of automated verification**. No workspace paths
  and no repo-local web assets participate; only the produced package contents do.
  **Compilation and installation from the local registry are the authoritative
  proof that the packaged dependency graph is complete and buildable.** Exact steps
  (CI, all three OSes):
  1. **Package**: `cargo package --no-verify -p <crate>` for all nine crates in
     publication order → nine `.crate` files. **`--no-verify` is required** because
     upper crates depend on internal crate *versions* that are not on crates.io, so
     Cargo's default post-package verify build would fail to resolve them; the
     install in step 6 is the real build check.
  2. **List assertions**: `cargo package --list -p <crate>` for each crate, before
     or alongside step 1 — `cratevista-server`'s list **must** contain
     `embedded/index.html` and the hashed `embedded/assets/**`; `cargo-cratevista`'s
     **must** contain `README.md`, `LICENSE-MIT`, `LICENSE-APACHE`.
  3. **Add workspace packages**: add the nine produced `.crate` files to a
     `cargo-local-registry` at `<tmp>/registry/` (standard layout: `index/**` +
     `.crate` files + `config.json`), in publication order.
  4. **Add third-party packages**: populate the same registry with **every**
     `Cargo.lock`-pinned third-party crate (from the job's
     `CARGO_HOME/registry/cache/**/*.crate`, already pinned by the earlier build),
     so the graph resolves entirely offline.
  5. **Source replacement**: a **temporary `CARGO_HOME`** whose `config.toml` sets
     `[source.crates-io] replace-with = "local"` and
     `[source.local] local-registry = "<registry>"`, with no crates.io credentials
     and no network source.
  6. **Install from outside the repo**: from a clean temp directory (not the
     workspace), `cargo install cargo-cratevista --version 0.1.0 --locked --offline`.
     A network fetch would error rather than reach crates.io. **This compile+install
     is the authoritative completeness/buildability check.**
  7. **Run** (the stable metadata-only fixture, Decision 8a): `cargo cratevista
     --help` and `cargo cratevista build --output <tmp>/site` against the fixture;
     assert the site materializes and renders — proving the packaged binary carries
     the embedded frontend **and** every internal crate.
  8. **Cleanup**: remove the temporary `CARGO_HOME`, the registry and the temp
     install dir. **Windows**: absolute paths written with forward slashes in
     `config.toml` (Cargo accepts them); the job runs under `pwsh`.

**Decision 8 — Phase-5B registry-assembly correction (LOCKED after the empirical
preflight; supersedes steps 3–4 above).** Steps 3–4 assumed the local-registry
tool could ingest a produced `.crate` file directly. The pinned tool
(`cargo-local-registry 0.2.12`, chosen because it builds and runs on Rust 1.97.1
and exposes both `sync` and `add`) **cannot**: its `add <name> --version <v>`
builds a throwaway manifest requiring that crate, runs Cargo's resolver, and
copies the resolved `.crate` from Cargo's cache — and its `main()` **deliberately
strips `[source]` replacement** so it always resolves the requested crate **from
crates.io**. `add cratevista-schema --version 0.1.0` therefore fails with *"no
matching package … location searched: crates.io index"* (reproduced empirically
and in the tool's source). This is a **tool limitation**, distinct from the Part-1
stop condition (Cargo *rejecting* an unpublished package in a replaced source): the
mandatory preflight proves Cargo 1.97.1 **does** install an unpublished package
from a replaced `local-registry` source `--locked --offline`. Accordingly, and per
the standing rule that manual index writing is permitted only when the pinned tool
demonstrably cannot add the archives **and** the PRD is amended (this note is that
amendment):

- **Third-party base (step 4) uses the tool's `sync`.** `cargo local-registry sync
  <Cargo.lock> <registry>` vendors every registry-sourced lock dependency and, by
  design, **skips the nine internal path crates**. Assembly/`sync` MAY reach
  crates.io; installation and verification are strictly offline.
- **The nine internal crates (step 3) are added by writing their index entries in
  the tool's own format** — the `get_index_path` layout (`index/<a><b>/<c><d>/<name>`
  for names ≥ 4 chars) plus the `RegistryPackage`/`RegistryDependency` JSON shape
  mirrored from `cargo-local-registry 0.2.12`. **All dependency/index metadata is
  derived from the Cargo-normalized packaged `Cargo.toml` produced by the same
  fresh `cargo package` invocation** — extracted from the exact `.crate` archive
  (the archive whose bytes are also SHA-256-checksummed for the entry's `cksum`),
  bound to that one archive by package name + version. **Workspace `cargo metadata`
  is used only as an independent expected-package consistency check** (that exactly
  the nine expected members exist) and never supplies an index dependency row. The
  normalized manifest is authoritative for the dependency package/rename names,
  version requirement, kind (normal/dev/build), target condition, optional,
  default-features, enabled features, the package feature table, `links` and
  `rust-version`; the archive bytes are authoritative for the checksum. An internal
  dependency that arrives without the `0.1.0` registry version, or with any
  path/git source, is rejected before the entry is written. A `local-registry`
  source needs **no `config.json`** (the tool writes none). This is **not** a
  bespoke format: it is the pinned tool's exact index format, generated for the
  crates the tool refuses to fetch.
- **The compile+install (step 6) remains the sole authoritative proof** that the
  packaged dependency graph is complete and buildable; a wrong or missing index
  entry becomes a hard offline failure, never a silent pass. The unpublished-package
  preflight is **retained as a permanent CI test** and **gates the whole harness**
  (a future Cargo/tool change that breaks the locked mechanism fails before the
  full package build). Provenance is independently guarded: a discriminating unit
  test proves the rows are manifest-driven (a path-bearing/un-normalized manifest,
  a version-less internal dep, or a manifest/archive identity mismatch are all
  rejected), and offline negative controls prove a wrong checksum, a missing index
  row, a missing archive, or a broken internal requirement each fail the install.

- **No automated publish or publish-`--dry-run`.** Actual **crates.io-side
  validation is a manual launch-checklist step taken immediately before the
  separately gated real publication** (Decision 9 / `docs/launch-checklist.md`), not
  a CI acceptance gate.

`Cargo.lock` is committed, so `--locked --offline` installs are reproducible.

### Decision 8a — the packaged-build fixture (metadata-only, stable)

The local-registry matrix runs `cargo cratevista build` against a **metadata-only
fixture with no documentable rustdoc target** (a single package whose only target
is, e.g., a binary with no lib/proc-macro, or an explicitly empty documentable
set). This exercises the **metadata-only successful generation path** (PRD 05:
empty `RustdocPlan` → `rustdoc: None` → `partial: false`, exit 0), so:

- `cargo cratevista build` runs **on stable, with no Node and no nightly**;
- it produces the static site via the metadata-only path;
- it proves the packaged binary contains the embedded frontend and all internal
  crates;
- it does **not** pretend normal rustdoc generation works without nightly.

Real rustdoc generation is proven separately by the **gated full-pipeline
`run_build` test on `nightly-2026-07-01`** (Testing strategy).

## Decision 9 — Release workflow and artifacts (LOCKED)

A new `.github/workflows/release.yml`, separate from `ci.yml`:

- **Trigger**: a pushed tag matching `v[0-9]+.[0-9]+.[0-9]+` (e.g. `v0.1.0`).
- **Toolchain**: pinned stable `1.97.1` (no nightly anywhere in the release build).
- **Targets** (matching runners CI already builds on, so nothing is promised that
  CI cannot produce):
  - `x86_64-unknown-linux-gnu` (ubuntu-latest);
  - `aarch64-apple-darwin` (macos-latest, which is Apple-silicon) **and**
    `x86_64-apple-darwin` (macos-13 Intel runner);
  - `x86_64-pc-windows-msvc` (windows-latest).
- **Build (one strategy — the committed relocated bundle is authoritative)**: the
  frontend bundle lives at `crates/cratevista-server/embedded/` (Vite `outDir`) and
  is **committed**. The release job (a) runs the repository's frontend
  reproducibility checks — `check:dist` and `check:embed-rebuild`, or their
  post-relocation equivalents — to prove the committed `embedded/` matches a fresh
  build, then (b) runs `cargo build --release -p cargo-cratevista --locked`, which
  embeds the **exact committed, verified `embedded/` bytes**. The job does **not**
  choose dynamically between rebuilding a bundle and using the committed one: a
  temporary build produced by `check:dist` is used only for *comparison* and is
  never what the binary embeds.
- **Archives**: `.tar.gz` on Linux/macOS, `.zip` on Windows, named
  `cargo-cratevista-<version>-<target>.<ext>`.
- **Binary**: `cargo-cratevista` (`cargo-cratevista.exe` on Windows); because it is
  named `cargo-<subcommand>`, `cargo cratevista …` works once it is on `PATH`.
- **Archive contents**: the `cargo-cratevista` binary, `LICENSE-MIT`,
  `LICENSE-APACHE`, `README.md`, `CHANGELOG.md`. Shell completions and a sample
  `.cratevista/` config are **deferred** (noted, not shipped in the first release).
- **Checksums**: one `<archive>.sha256` per archive.
- **Permissions**: the release job requests `contents: write` (to upload to the
  GitHub Release) and nothing more.
- **Provenance / signing**: **explicitly deferred.** SHA-256 sums only; no cosign,
  no SLSA attestation in the first release. Recorded as a future item.
- **`cargo publish` is a separate, manual, protected job** (`workflow_dispatch`,
  gated by a protected `release` environment) that publishes crates **in the
  Decision-8 order**. It is **never** triggered by the tag and **PRD 10 never runs
  it** — the workflow is delivered, not executed. Any real **crates.io-side
  validation** (a genuine `cargo publish --dry-run` against crates.io, name
  availability) is a **manual launch-checklist step performed immediately before**
  this job — never part of automated CI verification.

### Reproducibility claims (precise)

PRD 10 claims only these, and each is backed by a test:

- **The committed frontend bundle is deterministic** — `check:dist` proves it
  matches a fresh `vite build` byte-for-byte.
- **Package file sets are reproducible** — `cargo package --list` for each crate is
  asserted (Decision 8), so the *contents* of every `.crate` are pinned.
- **`document.json` is deterministic for unchanged input** — two builds produce
  byte-identical document bytes (the generator is deterministic by default).
- **Release archives are checksummed** — a SHA-256 file per archive, recomputed and
  verified in CI.

PRD 10 does **not** claim, and does not test, **bit-reproducible compiled
binaries**: `generation.json` carries a wall-clock timestamp (so it varies between
runs), and the release binaries are **not** promised byte-identical across builds
or platforms. There is no binary-reproducibility test and no such acceptance
criterion.

## Decision 10 — Documentation and launch scope (LOCKED)

**A. Repository/release docs PRD 10 must land** (ownership):

| document | action |
| --- | --- |
| `README.md` (root) | Expand to the issue-10 sections: value prop, screenshot/GIF **placeholder** using CrateVista's own UI, install (`--path` **and** future crates.io), first run, commands, supported inputs, the stable-CLI-vs-nightly distinction + compatibility tuple, manual-flow example, **static-build example**, privacy statement, known limitations, contribution links |
| `crates/cargo-cratevista/README.md` | New crate-local packaged readme (Decision 8.3) |
| `CHANGELOG.md` | Move `[Unreleased]` → a dated `[0.1.0]` at release; Keep-a-Changelog + semver |
| `SECURITY.md` | Add the **static-site** privacy statement (what a published site contains: the three artifacts, repo links, **no** snippets, **no** absolute paths) |
| `LICENSE-MIT`, `LICENSE-APACHE` (root) | Present — unchanged; **crate-local copies** `crates/cargo-cratevista/LICENSE-{MIT,APACHE}` are added and drift-checked byte-identical (Decision 8.4) |
| `docs/hosting.md` | **New**: URL root, GitHub/GitLab Pages, CI artifacts, base-path use, the `file://` limitation |
| `docs/launch-checklist.md` | **New**: the manual, gated launch steps (below), including the final name recheck |
| dependency-licensing report/process | **New**: a documented `cargo about` (Rust) + a `web/` license report process; the generated report is a launch artifact, the process is committed |
| `docs/adr/0009-static-build-and-release.md` | **Created at approval, status Proposed** (per repo convention, as ADR-0008 was at PRD-09 approval); becomes **Accepted** only when PRD 10 is Implemented / Verified |
| `ISSUES/issue_13_static_source_snippets.md` | **New specification shell** for the deferred snippets follow-up (Decision 6) — records the constraints only; **not** a PRD, **not** implementation, **not** added to `PRD/INDEX.md` as an approved PRD |

**B. Draft-only announcement material** (prepared, not published): crates.io
description, GitHub About text, Rust users forum / r/rust / This Week in Rust /
awesome-rust drafts, a demo GIF placeholder. Kept in the launch checklist or a
`docs/launch/` drafts area; **no overclaiming** — every claim maps to a tested
feature.

**C. Manual, gated actions this PRD never performs**: `cargo publish`, creating a
GitHub Release, any public announcement, registering repository topics, and the
**final name-availability recheck**. Name availability is a **launch-checklist
action taken immediately before publication**, never a stable automated acceptance
claim.

---

## Module boundaries

- **`cratevista-core`**: `run_build(&BuildOptions, &dyn Clock)` — thin
  orchestration: run the existing generation pipeline (via `run_generate`'s shared
  execution seam) once, then, on a committed snapshot, call
  `materialize_static_site(artifacts, embedded_assets(), &SiteOptions,
  protected_paths)` **exactly once**. **The public materialization seam receives
  `protected_paths: &[PathBuf]` and derives `ResolvedOutput`, `output_key` and
  `OutputSafety` internally**; `run_build` never constructs or supplies a
  caller-controlled `OutputSafety`, and **does not acquire the publication lock,
  prepare the parent, run recovery, or publish itself** — those are owned by
  `materialize_static_site` and happen once, there. The seam plus `BasePath`
  parsing/validation, the **existence-stable `output_key` derivation** (resolved
  full identity, length-framed, domain-separated), the **missing-parent preparation
  → re-resolve → lock** sequence, the per-output advisory lock,
  `OutputSafety::for_output(output, protected)` (private key derived from the output
  — no caller-supplied key is trusted), and the output-ownership contract (the
  **three-state A/B/C keyed marker with atomic rename-over transitions**, the **P0
  pre-marker crash shell**, path-safety, key-scoped recovery incl. the
  **post-rename/pre-finalization** window, transactional publish-with-rollback) and
  the `index.html` `<base href>` + static-marker injection. The shared generation
  seam returns the committed `ArtifactPaths` and the exact protected inputs, so
  cargo metadata, rustdoc, config and flow discovery each run **once**. The advisory
  lock dependency (`fs4`, added in Phase 2A) lives in `cratevista-core` only and
  does not touch the `cratevista-server` boundary.
- **`cratevista-server::assets`**: new public `embedded_assets()` iterator (the only
  production API addition). Exposes no rust-embed type; adds no dependency.
- **`cargo-cratevista`**: `Command::Build` gains `--output`, `--base-path` and the
  flattened `GenerateArgs`; `commands/build.rs` maps them to `BuildOptions` and
  calls `core::run_build`. No orchestration in the CLI crate.
- **`web/`**: `load.ts` generalized to an endpoints triple; `StaticArtifactSource`;
  `App.tsx` mode selection from the `<meta>` tag (static → no `LiveReload`);
  repository-link rendering in the inspector. **No second loader.**
- **CI / manifests**: `release.yml` (+ manual publish job); internal dep `version`
  fields; the `web/dist → crates/cratevista-server/embedded/` relocation and its
  script/harness repointing; `crates/cargo-cratevista/README.md` and crate-local
  `LICENSE-MIT` / `LICENSE-APACHE` copies + their byte-identical drift check.
- **Docs**: as Decision 10.

## Implementation sequence (bounded phases)

1. ~~**Server asset export.** Add `cratevista_server::assets::embedded_assets()` +
   unit tests (round-trips the real bundle; names match `serve_path`). *No CLI
   yet.*~~ **LANDED 2026-07-17 — see the Phase-1 ledger below.**
2. ~~**Core static build.** `SiteOptions`, `PublishedSite`,
   `OutputSafety::for_output` (private key), the **missing-parent preparation →
   re-resolve → lock** sequence, the cargo-free `materialize_static_site` seam, the
   **three-state A/B/C keyed marker** with **atomic rename-over transitions**,
   key-scoped recovery incl. **P0** and the **post-rename/pre-finalization** window,
   transactional publish-with-rollback, `<base href>` + static-marker injection,
   `BuildOptions`, and core `run_build` orchestration (execute the shared generation
   pipeline → materialize once) with the shared generation-execution seam and exact
   protected-input collection.~~ **LANDED 2026-07-17/18 — see the Phase-2A, Phase-2B
   and Phase-2C ledgers below.** *(This replaces the core `run_build` stub with the
   new `run_build(&BuildOptions, &dyn Clock)`; the `usecase::run_build()` stub the
   CLI still calls is untouched until step 3.)*
3. ~~**CLI surface.** `Command::Build { output, base_path, generate }`;
   `commands/build.rs` maps CLI values to `BuildOptions`/`run_build`; `--help` and
   arg tests. `serve`/`open` unchanged.~~ **LANDED 2026-07-18 — see the Phase-3
   ledger below.**
4. ~~**Frontend static mode.** Endpoints-triple loader refactor;
   `StaticArtifactSource`; `<meta>` mode selection (no `LiveReload` in static
   mode); repository links. Unit + component tests; extend
   `static-export.spec.ts` to load a **real produced site** with relative URLs.~~
   **LANDED 2026-07-18 — see the Phase-4 ledger below.**
5. **Publish plumbing.** Relocate the embedded bundle into
   `crates/cratevista-server/embedded/` (outDir + rust-embed folder + build.rs +
   package-file-set audit — **no `include` list unless a complete allowlist is
   demonstrably required** — + repoint `check:dist` / `check:embed-rebuild` / E2E
   harness); add internal-dep `version`s; crate-local `README.md` and crate-local
   `LICENSE-MIT` / `LICENSE-APACHE` + their byte-identical drift check. `cargo
   package --list` assertions; local-registry package-then-install test (3 OSes).
   **(Phase 5A landed: relocation + package topology — see the Phase-5A ledger.
   Phase 5B implemented the offline local-registry package-then-install harness +
   the three-OS CI matrix — see the Phase-5B ledger — and is green on the current
   host, but **step 5 stays incomplete** until the real ubuntu/macOS/windows matrix
   legs pass on GitHub Actions.)**
6. ~~**Release workflow + docs.** `release.yml` (tag → matrix build → archives →
   SHA-256 → GitHub Release upload) + the manual publish job; README/SECURITY/
   CHANGELOG/hosting/launch-checklist/licensing docs; ADR-0009; and create
   **`ISSUES/issue_13_static_source_snippets.md`** as a *specification shell* (not a
   PRD, not implementation) recording the Decision-6 snippet constraints. It is
   **not** added to `PRD/INDEX.md` as an approved PRD.~~ **LANDED 2026-07-18 — see the
   Phase-6 ledger below.** The workflows are delivered but **not executed** (no tag
   pushed, no release created, no publish run).
7. **End-to-end verification.** Base-path browser E2E; static zero-`/api/**` E2E;
   produced-site privacy scan; determinism (identical `document.json`, with
   `generation.json` timestamps explicitly allowed to differ); the full-pipeline
   gated-nightly `run_build` test; the local-registry package-then-install matrix;
   README first-run script. **IMPLEMENTED 2026-07-18 — current-host verification green;
   hosted closure pending. See the Phase-7 ledger below. Step 7 is
   implementation-complete but stays open until the hosted Linux/macOS/Windows legs
   pass on the same final changeset (as does Step 5).**

Each phase is independently reviewable and leaves the tree green.

> **Phase 1 — server asset export — LANDED 2026-07-17. PRD 10 stays Approved.**
>
> **API.** `crates/cratevista-server/src/assets.rs` gains one public function:
>
> ```rust
> pub fn embedded_assets() -> impl Iterator<Item = (String, std::borrow::Cow<'static, [u8]>)>;
> ```
>
> reachable as `cratevista_server::assets::embedded_assets` (via the existing
> `pub mod assets`). It yields every embedded frontend asset as `(normalized
> relative path, exact bytes)`. **No `rust_embed` type, no `Assets` type, no axum
> response type crosses the boundary** — the item is `(String, Cow<'static,
> [u8]>)`, exactly the approved contract.
>
> **Ordering / determinism.** rust-embed's `iter()` order is unspecified, so the
> entries are **collected and sorted lexicographically by path** before return; the
> bundle is small and this is a materialization API, not a per-request path. Two
> calls return identical ordered paths and bytes.
>
> **Path contract / shared source.** Both `embedded_assets` and `serve_path` read
> the **same private `Assets`** (rust-embed `#[folder = "../../web/dist"]` — its
> location is unchanged this phase; relocation is Phase 5). A single private
> predicate `is_embeddable_name` states the one path policy (relative,
> `/`-separated, non-empty, no leading slash, no backslash, no `.`/`..` component);
> `embedded_assets` `debug_assert!`s it so a future embed regression fails loudly
> rather than leaking a malformed public path. `serve_path` behaviour is **entirely
> unchanged** — cache headers, MIME, SPA fallback, missing-asset handling and
> fingerprint detection are untouched; no synthetic fallback entry is enumerated.
>
> **Tests (10 new, all in `assets.rs`; all discriminating unless noted):**
> 1 index.html present; 2 every fingerprinted asset + the JS/CSS/ELK-worker present;
> 3 paths lexicographically sorted; 4 paths unique; 5 every path normalized /
> traversal-free; 6 the enumerated name set **equals** the private `embedded_names`
> seam; 7 `serve_path(name)` returns `200` with **exactly** the paired body bytes
> for every asset (full-body compare, not counts); 8 deterministic across calls;
> 9 no synthetic SPA-fallback entry (each pair is a genuine embedded file, index.html
> exactly once); 10 an unknown path is never enumerated (while `serve_path` still
> falls back). **Negative controls:** breaking `serve_path`'s source fails test 7;
> forcing wrong bytes fails test 7. *Honest note:* on this platform rust-embed's
> `iter()` already returns sorted order, so removing the sort does **not** fail
> test 3 here — test 3 asserts the sorted-output **guarantee** (which protects
> platforms/versions where `iter()` differs), and the production sort is retained
> unconditionally.
>
> **Gates.** `cargo fmt --all --check`, `cargo clippy -p cratevista-server
> --all-targets --all-features -D warnings`, `cargo test -p cratevista-server
> --all-features` (**89 lib + 3 boundary + 9 e2e-fixtures**), `cargo test --workspace
> --all-features` (**714 → 724**, 0 failed), `cargo +1.97.1 check` — all green.
> **No new dependency**; `cratevista-server` still depends only on
> `cratevista-schema` (dependency-boundary tests pass); `#![forbid(unsafe_code)]`
> intact; **no frontend file changed** and `web/dist` was not rebuilt.
>
> **Out of scope this phase (unstarted):** `run_build`, `materialize_static_site`,
> `BuildOptions`/`SiteOptions`/`BasePath`, CLI build args, static frontend mode,
> repository links, the `web/dist` relocation, version requirements, crate-local
> README/licences, `release.yml`, local-registry packaging. **No final PRD-10
> acceptance criterion is checked**: this API is a building block, not itself an
> acceptance criterion.

> **Phase 2A — static-build safety foundations — LANDED 2026-07-17. PRD 10 stays
> Approved; Phase 2 remains incomplete (only its reusable foundations landed).**
>
> **New module** `crates/cratevista-core/src/static_site/` (owned specifically by
> static build — *not* moved into `cargo-cratevista`/`cratevista-server`, and not a
> general shared-domain crate):
> `base_path.rs`, `output_identity.rs`, `safety.rs`, `marker.rs`, `lock.rs`,
> `error.rs`, `mod.rs`. Wired via `pub mod static_site;` in `lib.rs`.
>
> - **`BasePath::parse(&str) -> Result<BasePath, BuildError>`** — the exact
>   Decision-3 contract. `"" → ""`, `"/" → "/"`, `repo`/`/repo`/`/repo/` → `/repo/`,
>   `a/b → /a/b/`; rejects a scheme, `//host`, query, fragment, `..`, backslash,
>   control chars and interior whitespace. The typed value's normalized form cannot
>   violate the contract. **No HTML is edited** in 2A.
> - **`resolve_output(&Path)` / `resolve_output_key(&Path) -> Result<String,
>   BuildError>`** — the locked existence-stable algorithm: lexical normalize →
>   nearest existing ancestor (symlink-rejected, `build_output_symlink`) →
>   canonicalize + append the missing remainder into **one** resolved identity →
>   `BLAKE3(b"cratevista-output-key-v1" ‖ per-component u32-LE-length ‖ lossless
>   bytes)`, first 16 lowercase-hex. **Unix** uses `OsStr` raw bytes (non-UTF-8
>   supported); **Windows** uses `encode_wide` UTF-16 LE. The ancestor/remainder
>   split never enters the hash, so **the key is unchanged when intermediate parent
>   directories are later created** (proved by test + a reverted control that hashes
>   depth). Never exposed in the published marker.
> - **`OutputSafety { protected: Vec<PathBuf>, output_key: String }`** with a
>   **directional** check: reject iff `output == p` **or** `output` is an *ancestor*
>   of a protected `p` (`build_output_forbidden`); a descendant is allowed. Proven:
>   `<workspace>/dist` and `target/cratevista/site` allowed; equal-to-input,
>   ancestor-of-input, workspace root and its ancestor rejected; a missing output is
>   checked through its nearest canonical existing ancestor; a symlinked ancestor →
>   `build_output_symlink`. Tests supply an explicit protected set — **no cargo**.
> - **Three-state A/B/C marker** (`Marker::staging(key)` / `complete(key)` /
>   `published()`): A = `staging`+key, B = `site`+key, C = `site` **no key**
>   (portable — C serializes with no `output_key` and no path). Context validation
>   (`MarkerRole::{Published, Staging(key), Complete(key)}`) distinguishes wrong
>   kind, wrong key-presence and mismatched key; **a present-but-invalid marker →
>   `build_output_marker_invalid`; an absent marker is `Ok(None)`, not invalid**
>   (Phase 2B maps a non-empty unmarked output to `build_output_not_owned`).
> - **Crash-safe marker I/O** through an injectable `MarkerFs`/`MarkerFile` seam:
>   serialize → write a uniquely-named `.tmp-*` sibling → flush → **rename over**
>   `.cratevista-static-site.json`; **never truncates/rewrites in place**. Failure
>   injected at temp-create / write / flush / rename each leaves the previous
>   authoritative marker **byte-identical** and no partial authoritative marker
>   (proved + a reverted in-place-write control); a leftover `.tmp-*` is not read as
>   authoritative.
> - **Per-output advisory lock** (`OutputLock::acquire` / `with`) on
>   `.cratevista-<output_key>.lock` via **`fs4 1.1.0`** — a non-blocking OS advisory
>   lock (`flock`/`LockFileEx`), released on `Drop` **and** on process termination
>   (handle close), **not** a `create_new` lock file. Contention →
>   `build_output_busy` **before** any body/hook runs (proved + a reverted
>   always-acquire control). A leftover unlocked lock file is reused; different keys
>   don't block; no stale-PID guessing. **Subprocess crash-release is deferred to
>   Phase 2B** (not faked with two in-process handles).
> - **`BuildError`** — one authoritative enum + `code()`/`exit()`/`to_diagnostic()`/
>   `to_command_failure()`. Phase 2A implements & tests `build_invalid_base_path`
>   (usage/2), `build_output_busy`, `build_output_marker_invalid`,
>   `build_output_symlink`, `build_output_forbidden` (runtime/1), each with **one
>   non-overlapping meaning**; the four `build_publish_*`/`recovery`/`not_owned`
>   variants are **declared** (single enum) but **not** claimed tested until Phase
>   2B. An internal `build_filesystem_error` catch-all is deliberately distinct so
>   the PRD codes keep their one-meaning contract. **No diagnostic exposes an
>   absolute path, username or output identity** (safe static labels only; test).
>
> **Advisory-lock dependency:** `fs4 1.1.0` (**MIT OR Apache-2.0**), added to
> `[workspace.dependencies]` and **`cratevista-core` only** (not `cargo-cratevista`,
> not `cratevista-server`). It reuses the `rustix`/`windows-sys` already in the tree,
> so `Cargo.lock` gained only `fs4`. `cratevista-server` is unchanged and still
> depends on neither core nor `fs4`; `embedded_assets` is untouched.
>
> **Gates (Rust 1.97.1):** `fmt`, `clippy -p cratevista-core -D warnings`,
> `clippy --workspace -D warnings`, `cargo test -p cratevista-core` (**+44
> static_site tests**), `cargo test --workspace` (**724 → 768**, 0 failed),
> `cargo +1.97.1 check`, `cargo tree`, `git diff --check` — all green.
> `#![forbid(unsafe_code)]` intact (no unsafe added). **No frontend/`web/dist`
> change; INDEX unchanged; ADR-0009 stays Proposed; `run_build` stub unchanged.**
>
> **Phase 2 remains incomplete.** Not started: `materialize_static_site`, staging
> asset/artifact writes, recovery cases A–F and the crash-window recovery,
> predecessor backup/restore, transactional publication, `run_build` orchestration,
> the CLI build arguments, the frontend static mode, repository links, the bundle
> relocation, and all packaging/release work.

> **Phase 2B — cargo-free materialization, recovery and transactional publication —
> LANDED 2026-07-18. PRD 10 stays Approved; Phase 2 remains incomplete (Phase 2C =
> `BuildOptions` + core `run_build` orchestration, and Phase 3 = the
> `cargo-cratevista` CLI surface, remained at the time).**
>
> **New modules** under `crates/cratevista-core/src/static_site/`: `nonce.rs`,
> `html.rs`, `fs_seam.rs`, `materialize.rs` (declared in `mod.rs`; re-exports
> `materialize_static_site`, `SiteOptions`, `PublishedSite`, `SiteFs`, `RealSiteFs`).
> `safety.rs`, `marker.rs`, `output_identity.rs` amended (see below).
>
> - **`materialize_static_site(artifacts: &ArtifactPaths, assets: impl Iterator<Item
>   = (String, Cow<'static, [u8]>)>, options: &SiteOptions, protected_paths:
>   &[PathBuf]) -> Result<PublishedSite, BuildError>`** — the approved cargo-free
>   seam. It **derives** `ResolvedOutput`, `output_key` and `OutputSafety` from
>   `options.output` itself; **no caller-supplied key is accepted**. Internally it
>   collects the assets and calls the seam-parameterized `run_materialize(&dyn SiteFs,
>   …, &OutputSafety)`, which **validates** the safety key against a freshly derived
>   key before any lock/scan/mutation (a mismatch → internal `build_filesystem_error`,
>   never `build_output_busy`/ownership).
> - **`SiteOptions { output, base_path: Option<BasePath>, generated_at: String }`**
>   (`SiteOptions::new(output, base_path, &dyn Clock)` reads the build timestamp once;
>   every A/B/C marker of a build shares it). **`PublishedSite`** keeps the resolved
>   output for a **success** report only — private field, never serialized, never in a
>   `BuildError`.
> - **`OutputSafety::for_output(output, protected)`** is now the only production
>   constructor; **fields are private**; the derived `output_key` cannot be forged. A
>   `#[cfg(test)] from_parts` exists solely to prove a forged key is rejected.
> - **Nonces (`nonce.rs`)** — one locked format: `[0-9a-f]{32}` from `getrandom`
>   (OS CSPRNG, not timestamp/PID). Keyed names `.cratevista-<key16>-staging-<n32>` /
>   `-backup-<n32>` / `.cratevista-<key16>.lock` and marker-temp
>   `.cratevista-static-site.json.tmp-<n32>`. `classify_for_key` requires the **exact**
>   fixed-width format (no prefix/suffix/uppercase/short/separator); a candidate must
>   be a **real directory** — a candidate-name symlink is never traversed or removed.
> - **Ten-step pre-lock preparation** — normalize/resolve → symlink+safety checks →
>   derive identity+key → create **only** missing parents (never `<output>`) →
>   **re-resolve** + re-check → assert identity/key unchanged (else fail before
>   lock/scan) → acquire the keyed lock → recovery + publication. The **resolved
>   canonical parent** is used for the lock and all siblings. `build_output_busy`
>   performs no candidate inspection or output/staging/backup mutation; only a safely
>   created empty parent chain may remain.
> - **Recovery (`materialize.rs`)** — P0/A/B/C classification scoped to the current
>   key; the recovery table rows **A–F** (stable output cleans stale staging then
>   backups; absent+one-backup restores **first**; absent+multiple →
>   `build_recovery_ambiguous`; absent+none → first build; invalid/mismatched/non-P0
>   preserved; restore failure → `build_publish_unrecoverable`). A matching **marker B
>   at `<output>`** is finalized to C and kept; a mismatched B or a marker A at
>   `<output>` → `build_output_marker_invalid`. Ownership: absent / empty-adoptable /
>   marker-C-replaceable / non-empty-unowned (`build_output_not_owned`) /
>   malformed (`build_output_marker_invalid`) / symlink (`build_output_symlink`);
>   symlinks are never followed when deciding emptiness/ownership.
> - **Transactional publication** — mkdir staging → commit **A** (first authoritative
>   file) → assets (with `index.html` transformed) + three artifacts copied → commit
>   **B** → make room (marker-C site → keyed backup; empty → remove; absent → nothing)
>   → rename staging→`<output>` → finalize **B→C** → delete backup (**only after C**)
>   → release lock. Rollback restores the predecessor (backup / recreated-empty /
>   absence) with `build_publish_failed`; an unrestorable predecessor or a post-rename
>   **B→C** failure preserves every recoverable path with `build_publish_unrecoverable`
>   (the next run finalizes B→C). A failure before **B** leaves an existing owned
>   `<output>` **byte-identical**.
> - **HTML transformation (`html.rs`)** — treats `index.html` as UTF-8, inserts exactly
>   one `<meta name="cratevista-mode" content="static" />` and (for a non-empty
>   `BasePath`) exactly one `<base href>` **before** URL-bearing elements, anchored to
>   the single validated `<head>` boundary; rejects an already-transformed document or
>   a missing/duplicate/malformed `<head>`. No HTML-parser dependency; minified JS/CSS
>   and `./assets/**` refs untouched. Asset validation rejects an empty set, a missing/
>   duplicate `index.html`, absolute/traversing/backslash paths, duplicates and
>   reserved-name collisions (`document.json`/`generation.json`/`diagnostics.json`/the
>   marker) via the internal `build_filesystem_error` — **no new public code**.
> - **Filesystem seam (`fs_seam.rs`, `SiteFs`/`RealSiteFs`)** — the narrow, static-
>   build-specific injection points (parent creation, re-resolution, enumeration,
>   staging mkdir, marker A/B/C, asset write, artifact copy, output→backup,
>   staging→output, backup restore, empty recreate, cleanup); **not** a generic VFS.
>
> **New dependency:** `getrandom = "0.3"` (**MIT OR Apache-2.0**), `cratevista-core`
> only, for the OS-random nonce. Already resolved in the tree (via `ahash`), so
> `Cargo.lock` gained only the dependency **edge** — no new crates.
>
> **Tests:** **+53** `static_site` unit tests (materialization/recovery/publication/
> content/nonce/html; total 97 static_site), incl. a **subprocess** proof that the
> advisory lock is released on process kill (real child process, not two in-process
> handles), the five crash-timing/P0 cases, and **five reverted negative controls**
> (disabled key filtering; any-unmarked-as-P0; no restore-first ordering; backup
> deleted before C; accepted forged key) — each fails its target test only while
> reverted.
>
> **Gates (Rust 1.97.1):** `fmt`, `clippy -p cratevista-core -D warnings`,
> `clippy --workspace -D warnings`, `cargo test -p cratevista-core`,
> `cargo test --workspace` (**768 → 821**, 0 failed), `cargo +1.97.1 check`,
> `cargo tree`, `git diff --check` — all green. `#![forbid(unsafe_code)]` intact.
> **No `run_build`/CLI/frontend/`web/dist` change; INDEX unchanged; ADR-0009 stays
> Proposed.**

> **Phase 2C — core `run_build` orchestration — LANDED 2026-07-18. PRD 10 stays
> Approved. This completes Implementation sequence step 2; the CLI is the separate
> step 3 and remains unstarted.**
>
> **New module** `crates/cratevista-core/src/build.rs` (`pub mod build;` + crate-root
> re-export `pub use build::{BuildOptions, run_build}`). **New test**
> `crates/cratevista-core/tests/build_live.rs`. **Amended:** `generate.rs` (shared
> execution seam), `cratevista-config/src/lib.rs` (`load_config_with`),
> `static_site/materialize.rs` (`#[cfg(test)] PublishedSite::new_for_test`).
>
> - **`BuildOptions { generate: GenerateOptions, output: PathBuf, base_path:
>   Option<BasePath> }`** — core-owned, **no `Default`** (the default-output policy
>   belongs to the CLI adapter, which has the workspace context). **`pub fn
>   run_build(&BuildOptions, &dyn Clock) -> CommandOutcome`.**
> - **Shared generation-execution seam.** `run_generate` now delegates to a
>   `pub(crate) execute_generate(&GenerateOptions, &dyn Clock) -> GenerateExecution`
>   returning `Committed { generated: GeneratedArtifacts, outcome }` or `Failed {
>   outcome }`. `GeneratedArtifacts { artifacts: ArtifactPaths, protected_paths:
>   Vec<PathBuf>, partial: bool }`. **`run_generate`'s behavior, diagnostics and
>   committed bytes are unchanged** (a committed snapshot is proven structurally, not
>   by reading a numeric exit code). Cargo metadata, rustdoc, config **and flow
>   discovery each run once** — config discovery is done via `discover()` +
>   `load_config_with(...)` so `.cratevista/` is scanned a single time.
> - **Exact protected-input collection** (`collect_protected_paths`, from the same
>   metadata + config the document was built from, never a recursive walk): workspace
>   root, root `Cargo.toml`, `Cargo.lock` (only when present), member manifests and
>   member target **source roots**, `cratevista.toml`, discovered flow/override
>   files, every configuration-**referenced** file, the artifact root and the three
>   artifacts. Externals (registry sources) are excluded; the list is sorted +
>   deduplicated and non-UTF-8 `PathBuf`s are preserved; no symlink is followed while
>   collecting; the paths are **internal** (never in a diagnostic) and passed straight
>   to `materialize_static_site`.
> - **`run_build` flow.** Execute generation once → on a fatal failure return the
>   **unchanged** generation outcome (no `SiteOptions`, no `embedded_assets()`, no
>   materialize, no output inspection/lock/recovery; an existing owned output stays
>   byte-identical) → on a committed snapshot construct `SiteOptions::new(output,
>   base_path, clock)`, obtain `embedded_assets()`, and call `materialize_static_site`
>   **exactly once** with the committed `ArtifactPaths`, the assets, the options and
>   the protected paths → success returns `ExitCode::SUCCESS` (a materialization
>   success message that makes no static-mode/E2E/release/publish claim); a
>   `BuildError` maps through `to_command_failure()` with its exact runtime/usage
>   exit. `run_build` performs **no** `resolve_output`, key derivation, parent
>   creation, `OutputSafety` construction, lock, candidate scan, recovery, staging or
>   rollback — all owned by `materialize_static_site`.
> - **Tests (+10 core).** Orchestration via injected generate/asset/materialize
>   seams: fatal-failure (original outcome, materializer & assets untouched,
>   output byte-identical), committed success (materializer called **once** with the
>   exact artifacts/output/base/protected + the embedded asset source), BuildError
>   mapping (runtime `build_output_not_owned`, usage `build_invalid_base_path`),
>   real-materializer marker-C + content (cargo-free), partial committed snapshot
>   (bytes copied unchanged), one-call/no-preflight, and protected-input coverage per
>   category (+ deterministic dedup, external exclusion, absent-lockfile exclusion).
>   Existing `run_generate` integration tests pass unchanged (the refactor
>   regression). **One `#[ignore]` full-pipeline test** `build_live.rs` runs the real
>   `run_build → run_generate → materialize_static_site` on `nightly-2026-07-01` with
>   the real embedded bundle — asserts index.html, static meta once, the three
>   artifacts, marker C without a key, a fingerprinted asset, artifacts matching the
>   committed snapshot, and no `/api`/source files; cleans up its temp output. It was
>   **run and passed** on the pinned nightly during this phase:
>   `cargo +nightly-2026-07-01 test -p cratevista-core --test build_live -- --ignored
>   --exact build_live_materializes_a_static_site`.
>
> **No new dependency; `Cargo.lock` unchanged.** No `unsafe`.
>
> **Gates (Rust 1.97.1):** `fmt`, `clippy -p cratevista-core -D warnings`,
> `clippy --workspace -D warnings`, `cargo test -p cratevista-core` (**173 lib**),
> `cargo test --workspace` (**821 → 831**, 0 failed), the ignored nightly
> full-pipeline test, `cargo +1.97.1 check`, `cargo tree`, `git diff --check` — all
> green. At the time, **the `cargo-cratevista` CLI was unchanged** (`Command::Build`,
> `commands/build.rs`, `usecase::run_build()` stub, help/parser tests all untouched);
> **no `web/`/`web/dist` change; INDEX unchanged; ADR-0009 stays Proposed.** *(Phase 3
> below wires the CLI and removes the `usecase::run_build()` stub.)*

> **Phase 3 — `cargo-cratevista` build CLI surface — LANDED 2026-07-18. PRD 10 stays
> Approved. This makes `cargo cratevista build` user-accessible and completes
> Implementation sequence step 3. Core generation/materialization orchestration is
> unchanged except for adding workspace-root output resolution.**
>
> **Final CLI syntax:** `cargo cratevista build [--output <dir>] [--base-path <path>]
> <generate flags…>`. The Clap variant is `Command::Build { output: Option<PathBuf>,
> base_path: Option<String>, #[command(flatten)] generate: GenerateArgs }` — the
> **existing `GenerateArgs` is reused** (every `generate` flag is accepted; no
> `ServerArgs`/watch/server flag is flattened; there is no
> `--include-source-snippets`).
> - **Default-output semantics.** No `--output` → the CLI supplies the **relative**
>   default `target/cratevista/site`; core anchors it. A relative `--output` (`dist`,
>   `a/b/site`) is resolved against the **workspace root the generation just
>   discovered**, never the process CWD; an absolute `--output` is used unchanged.
>   Resolution happens **in core** (`build::anchor_output`) after a committed
>   generation, using the new `GeneratedArtifacts.workspace_root` (the only core
>   change) — so it works when `build` is started outside the workspace with the
>   generation args pointing into it, and runs cargo metadata **once**.
> - **Base-path handling.** The adapter calls `BasePath::parse(...)` and maps the
>   error through `to_command_failure()`, preserving `build_invalid_base_path` /
>   **exit 2**; validation is **not** duplicated in the CLI. Absent → `None`.
> - **Adapter boundary.** `commands/build.rs` only maps CLI values → `BuildOptions`
>   and calls `cratevista_core::run_build(&BuildOptions, &SystemClock)`. It runs no
>   cargo/metadata, enumerates no assets, resolves no `ArtifactPaths`, collects no
>   protected paths, prepares no parents, takes no lock, does no recovery/
>   materialization, and prints no second success message.
> - **Old stub removed.** `cratevista_core::usecase::run_build()` (the
>   `unimplemented`/exit-4 stub) and its test are deleted; `dispatch` routes
>   `Command::Build` to the new adapter. There is now **one** `run_build` API
>   (`cratevista_core::run_build`). `CommandFailure::unimplemented` remains as an
>   unrelated preserved export.
> - **Files changed.** `crates/cargo-cratevista/src/cli.rs` (Build variant + help),
>   `.../src/dispatch.rs` (wiring), `.../src/commands/build.rs` (adapter),
>   `.../tests/cli.rs` (build suite), `crates/cratevista-core/src/generate.rs`
>   (`GeneratedArtifacts.workspace_root`), `crates/cratevista-core/src/build.rs`
>   (`anchor_output` + tests), `crates/cratevista-core/src/usecase.rs` (stub removed).
>   **No `web/` change; no `npm`.**
> - **Tests (+17).** CLI parser/help (defaults, `--output dist`/`a/b/site`/absolute,
>   `--base-path /demo/`/`repo`, full `GenerateArgs`, rejects `--watch`/server-only/
>   `--include-source-snippets`, help no longer says unimplemented and documents the
>   default + workspace-root-relative rule); diagnostics (`build_invalid_base_path`
>   exit 2 incl. JSON, a preserved generation error exit 3, a
>   `build_output_forbidden` runtime exit 1 via `--output .`, and no
>   unimplemented/exit-4 path); output resolution (default under the workspace from
>   root **and** from an external cwd, `--output dist` from outside → `<ws>/dist`,
>   absolute exact, a generation failure creates nothing anywhere); and core
>   anchoring unit tests (relative→workspace-root, absolute unchanged, never CWD,
>   generation runs once). **One stable, non-ignored metadata-only real-CLI test**
>   (`build_cli_materializes_a_metadata_only_site`) runs the real subcommand on a
>   bin-only fixture (no nightly/Node): exit 0, index.html + three artifacts +
>   marker C without a key + a fingerprinted asset, no `/api` and no `source/`. It
>   proves CLI dispatch; documentable-Rust proof remains the ignored core nightly
>   test.
>
> **Gates (Rust 1.97.1):** `fmt`, `clippy -p cargo-cratevista -D warnings`,
> `clippy -p cratevista-core -D warnings`, `clippy --workspace -D warnings`,
> `cargo test -p cargo-cratevista`, `cargo test -p cratevista-core`,
> `cargo test --workspace` (**831 → 848**, 0 failed), the stable metadata-only
> real-CLI test, `cargo +1.97.1 check`, `git diff --check` — all green. Five reverted
> negative controls (route Build to the stub; anchor relative output to CWD; parse
> base path only via a generic Clap error; omit flattened `GenerateArgs`; call the
> materializer twice) each failed their target test only while reverted. No `unsafe`.
> **No new dependency. No frontend/packaging/release change; INDEX unchanged;
> ADR-0009 stays Proposed.**

> **Phase 4 — frontend static mode + safe repository links — LANDED 2026-07-18. PRD
> 10 stays Approved. This completes Implementation sequence step 4. The frontend
> source AND the committed `web/dist` bundle are updated; **the bundle is NOT
> relocated** — relocation stays Phase 5.**
>
> - **Runtime mode.** `web/src/api/runtimeMode.ts`: `type RuntimeMode = "server" |
>   "static"` and pure `detectRuntimeMode(document)`. It reads **only** the injected
>   `<meta name="cratevista-mode" content="static">` — exact match → static; absent /
>   unrelated meta / any other content → server. No env var, define, query param,
>   hostname, failed request or build flag is consulted. The mode is decided **once**
>   at the App composition root (`main.tsx` → `App`), in the first `useState`
>   initializer, before any source / source-client / live-reload is constructed.
> - **One coherent loader.** `load.ts`'s `loadArtifacts(endpoints: ArtifactEndpoints,
>   …)` now takes the exact three URLs (`SERVER_ARTIFACT_ENDPOINTS = /api/document|
>   generation|diagnostics`, `STATIC_ARTIFACT_ENDPOINTS = ./document|generation|
>   diagnostics.json`) — no base-string concatenation. The existing PRD-09
>   coherence/retry/cancellation/typed-incoherent/all-absent-header logic is
>   **unchanged and shared**; `StaticArtifactSource` and `ServerArtifactSource` both
>   delegate to it and add no fetching of their own. `StaticArtifactSource` uses the
>   relative URLs verbatim (browser resolution honours any `<base href>` / subpath),
>   inspects no `window.location`, and opens no `EventSource`.
> - **Bootstrap + no `/api` in static mode.** `App` selects `StaticArtifactSource` in
>   static mode and constructs **no `LiveReload`** (a `liveReloadFactory` seam proves
>   0 calls in static / exactly 1 in server), **no `EventSource`**, and performs **no
>   `/api/health` probe**. The source-content capability is represented explicitly:
>   `AppData.sourceClient: SourceClient | null` — `null` in static mode, so the
>   inspector renders no “Show source” action and selecting an entity issues no
>   `/api/source` request. Server mode keeps the existing loader, live-reload
>   lifecycle and opt-in source client unchanged.
> - **Repository links.** `web/src/api/repositoryLinks.ts`: pure
>   `repositoryLinks(project, location?) → RepositoryLinks | null`. Parses with the
>   URL API; only `https:` with **no** credentials, a non-empty host and a non-empty
>   repository path is eligible (ssh/git/git@/file/http/malformed/relative → none;
>   credentials → none). Normalizes one trailing slash and a single trailing `.git`.
>   Provider = exact `github.com`/`gitlab.com` (incl. GitLab subgroups) else `other`
>   (root link only — no guessed deep link). File links: GitHub `…/blob/<branch>/
>   <path>#L<line>`, GitLab `…/-/blob/…`; the branch is encoded as one path value and
>   **each** path component is encoded independently (no double-encoding); `#L` comes
>   only from a positive span start line. The inspector renders these at the existing
>   PRD-07 location (`Panels.tsx`), preferring the source deep link, with
>   `target="_blank" rel="noopener noreferrer"`, provider-identifying accessible
>   names, no disabled placeholder, and **identical output in both modes**.
> - **Tests (+41 web).** New: `runtime-mode` (5), `repository-links` (22),
>   `static-mode` (5: no-LiveReload / no-EventSource / no-health, mode-before-fetch,
>   no-`/api/source`-on-select, plus server-mode LiveReload-once + source-action
>   regression), `inspector-links` (6), and loader endpoint-triple/coherence-parity
>   (3). All **280** web unit/component tests pass; typecheck, `check:types` and lint
>   clean. **Real-produced-site E2E** (`static-export.spec.ts`, rewritten): builds a
>   site with the real `cargo cratevista build` on the metadata-only fixture, serves
>   it over HTTP at the non-root subpath `/cratevista/`, and asserts the injected
>   marker, `./*.json` fetches, a rendered explorer + node selection, **zero `/api`
>   requests** (guarded on the whole `/api` namespace), **zero `EventSource`**
>   constructions (window.EventSource replaced before app code), no `/api/health`, no
>   `/api/source` on selection, relative asset URLs under the subpath, subpath refresh
>   restoring query state, and zero page errors / CSP violations. Full Playwright
>   suite: **81 passed**. **Scope note (corrected in Phase 4B, below):** the
>   repository-link **renderer + pure helper** landed here, but at Phase 4 the
>   **generator dropped repository metadata** — `derive_project` hard-coded
>   `repository_url: None` and `default_branch: None`, so repository links were **not
>   reachable from a real generated document**. This was a generator defect, **not**
>   merely an E2E fixture limitation. Phase 4B makes `repository_url` reachable and
>   proves it from an unmodified produced document; `default_branch` remains
>   unresolved (see below), so source **deep** links are still not
>   production-reachable.
> - **Bundle.** `web/dist` rebuilt with the repository build (`index.*.js` +
>   `index.html` changed; CSS/worker unchanged); **`check:dist`** (committed dist ==
>   fresh build) and **`check:embed-rebuild`** (server embeds those exact bytes) both
>   pass, and a real `cargo cratevista build` now emits the static-capable bundle. A
>   `cratevista-server` test asserts the **embedded** index stays marker-free (the
>   marker is injected only into the copied output).
>   - **Terminology — “committed bundle”.** Throughout this PRD, a “committed”
>     `web/dist` (or `embedded/`) means **version-controlled content that must be
>     present in the final repository changeset**, i.e. the rebuilt bytes exist in the
>     working tree and are staged for the maintainer's commit. It does **not** mean an
>     implementation agent runs `git commit`: **no implementation agent should run
>     `git commit`.** The `check:dist` / `check:embed-rebuild` gates are what prove the
>     tree-present bundle is current; creating the actual Git commit is the
>     maintainer's step.
> - **Five/seven negative controls** (static source uses `/api`; static constructs
>   LiveReload; static builds the source client; mode detected after the first fetch;
>   repo links skip per-segment encoding; unknown host gets a guessed GitHub link;
>   src changed without rebuilding dist → `check:dist` STALE) each failed their target
>   test/gate only while reverted.
> - **Gates:** `check:types`, `typecheck`, lint, `vitest` (280), `build`,
>   `check:dist`, Playwright E2E (81), `check:embed-rebuild`; `cargo fmt`,
>   `clippy --workspace -D warnings`, `cargo test -p cratevista-server`
>   (embedded-index-marker-free), `-p cargo-cratevista`, `-p cratevista-core`,
>   `cargo test --workspace`, `cargo +1.97.1 check`, the stable metadata-only real-CLI
>   test, `git diff --check` — all green. **No new frontend dependency. Phase 5
>   (bundle relocation, packaging, release, hosting/launch docs) remains untouched;
>   INDEX unchanged; ADR-0009 stays Proposed.**

> **Phase 4B — repository-metadata reachability correction — LANDED 2026-07-18. PRD
> 10 stays Approved. Bounded pre-Phase-5 correction: makes `Project.repository_url`
> reachable through real generation and corrects the Phase-4 wording. No bundle
> relocation, manifests, local-registry CI, release or public docs.**
>
> - **`Project.repository_url` — was unreachable, now derived from real metadata.**
>   *Before:* `cratevista_graph::derive_project` hard-coded `repository_url: None`;
>   `cratevista-metadata` never read `cargo_metadata::Package.repository` (the field
>   was present but discarded). *Now:* `normalize.rs` captures each **member**
>   package's non-empty `repository` as a package-entity attribute, and
>   `derive_project` adopts the **unanimous** member value (equality compared
>   trailing-slash-insensitively; conflicting members → `None`; none → `None`;
>   externals never contribute). The accepted string is kept **verbatim** — the
>   frontend helper still decides whether it is safe to render. No network, no
>   diagnostic (matching the existing architecture), no document-format change.
> - **`Project.default_branch` — no authoritative source; deliberately unresolved.**
>   There is no config field, no git inspection in the pipeline, and no other
>   verified source. Per the correction's constraints, this task adds **no** guessed
>   default (`main`/`master`/current branch), **no** network call and **no** git
>   subprocess. `default_branch` stays `None`, so a repository **root** link is
>   production-reachable but a **source deep link is not**. *Unresolved decision for a
>   later phase:* choose an authoritative `default_branch` source (e.g. a
>   `cratevista.toml` project field, or a gated one-shot `git symbolic-ref
>   refs/remotes/origin/HEAD` at generation time) — out of scope here.
> - **Real-pipeline tests.** `cratevista-metadata`: `member_package_captures_declared_
>   repository` (new fixture `single_package_repo.metadata.json` with `repository =
>   "https://github.com/example/example"`) and `absent_repository_adds_no_attribute`.
>   `cratevista-graph`: five `derive_project` tests reading the **real** `Project` from
>   `build_document` — one member adopted; unanimous (ignoring trailing slash);
>   conflicting → `None`; none → `None`; an unsafe string preserved as data. **E2E:**
>   the produced-site fixture Cargo.toml now declares a real `repository`; the spec
>   asserts the **unmodified** `document.json` carries `repository_url =
>   https://github.com/example/example`, the inspector renders a safe root link
>   (`target=_blank rel="noopener noreferrer"`), clicking opens a popup (no
>   current-tab navigation), there is **no** source deep link (no `default_branch`),
>   and the zero-`/api` / zero-`EventSource` guarantees still hold. `document.json` is
>   **not** edited after generation and no synthetic `Project` is injected.
> - **Phase-4 status.** Phase 4 stays landed: static mode, the loader, LiveReload and
>   no-`/api` evidence are unchanged, and `repository_url` is now reachable from a
>   real generated document. Source **deep** links are **not** claimed
>   production-reachable (pending an authoritative `default_branch`).
> - **Files:** `crates/cratevista-metadata/src/normalize.rs`,
>   `crates/cratevista-metadata/fixtures/single_package_repo.metadata.json`,
>   `crates/cratevista-metadata/tests/normalize.rs`,
>   `crates/cratevista-graph/src/lib.rs`, `web/e2e/support/static-site.ts`,
>   `web/e2e/tests/static-export.spec.ts`. **No frontend `src` / `web/dist` change**
>   (the helper and renderer were already correct); dist is unchanged and still
>   passes `check:dist` / `check:embed-rebuild`. **Gates:** `cargo fmt`,
>   `clippy --workspace -D warnings`, `cargo test --workspace`, `cargo +1.97.1 check`,
>   the produced-site E2E (82 Playwright green), `git diff --check` — all green.

> **Phase 5A — embedded-bundle relocation and package topology — LANDED 2026-07-18.
> PRD 10 stays Approved. Implementation-sequence step 5 stays **incomplete**: the
> offline local-registry package-then-install matrix and the three-OS matrix are
> Phase 5B. No `release.yml`, no `cargo publish`, no release/hosting/launch docs.**
>
> - **Authoritative bundle relocated.** `web/dist` → **`crates/cratevista-server/
>   embedded/`** via `git mv` (byte-identical; `embedded/index.html` +
>   `embedded/assets/**`). **`web/dist` no longer exists**; it is the sole bundle.
>   Vite `outDir` now points at `../crates/cratevista-server/embedded` with an
>   explicit `emptyOutDir` (clears exactly that directory, never a parent), so a
>   normal `npm run build` removes stale fingerprinted files (proven: an injected
>   `index.STALE*.js` is gone after a build). `check:dist` still builds into a **temp**
>   dir and never mutates the authoritative bundle.
> - **`cratevista-server` repointed.** `#[folder = "embedded"]` (crate-local),
>   `build.rs` watches `embedded`. `serve_path`, `embedded_assets`, cache/MIME/SPA/
>   fingerprint behavior, the marker-free embedded index, and static-materialization
>   (one marker only in the output copy) are all unchanged — 90 server tests green,
>   including exact embedded name+byte comparisons.
> - **Scripts / E2E / CI repointed** (names preserved): `check:dist` and
>   `check-embed-rebuild.mjs` baseline the crate-local `embedded/`; the E2E
>   `watch-server` and `security.spec.ts` serve/compare the crate-local bundle; the
>   real produced-site E2E is unchanged; `.gitignore` note and CI comments updated.
>   The only surviving `web/dist` strings are **historical** (PRD/CHANGELOG/ADR/docs),
>   **intentional** (a test asserting `web/dist` is gone; the relocation comment), or
>   **unrelated test fixtures** (`/w/web/dist` classifier inputs in `cratevista-watch`).
> - **Internal dependency versions.** Every internal edge in `[workspace.dependencies]`
>   now carries **both** `path` and `version = "0.1.0"` (all crates inherit via
>   `{ workspace = true }`; there are no raw internal path edges outside the table).
>   All nine crates stay `0.1.0`. **`Cargo.lock` unchanged** (adding a version to a
>   path edge does not change resolution). Internal DAG: schema ← metadata, rustdoc;
>   graph ← schema, metadata, rustdoc; config ← schema, graph (+dev metadata);
>   server ← schema; watch ← (none); core ← schema, metadata, rustdoc, graph, config,
>   server, watch; cargo-cratevista ← core. Verified in the **normalized packaged
>   manifests**: internal deps appear as `version = "0.1.0"` **only** — no `path`, no
>   `..`, no absolute path.
> - **No `include` list needed.** `cargo package -p cratevista-server --list` shows
>   Cargo's **default** rules already include the tracked `embedded/index.html` +
>   `embedded/assets/**` alongside `src/**` and `build.rs`, with **no** `web/dist`,
>   `node_modules` or target output — so **no `include` was added** (a negative
>   control proved an `include = ["embedded/**"]` would wrongly drop `src/**` /
>   `build.rs`). `cargo-cratevista` needs none either.
> - **Crate-local README + licences.** New `crates/cargo-cratevista/README.md`
>   (install-focused: what CrateVista does, `cargo cratevista`, the static-site
>   command, the stable-CLI-vs-nightly-rustdoc distinction, an absolute repo link;
>   **no** claims of being published, an existing Release, always-available deep
>   links, `file://`, binary reproducibility or source snippets); `readme =
>   "README.md"`. Byte-identical `LICENSE-MIT` / `LICENSE-APACHE` copies; the SPDX
>   `license = "MIT OR Apache-2.0"` is untouched (no `license-file`).
> - **Reusable checks (one existing test crate; no xtask, no 10th crate).**
>   `crates/cargo-cratevista/tests/packaging.rs`: `licenses_match_root_byte_for_byte`
>   (drift; fails on absent copy, one byte or a line-ending change; run with `cargo
>   test -p cargo-cratevista --test packaging licenses`),
>   `every_internal_workspace_dependency_edge_declares_a_version`,
>   `cargo_cratevista_readme_is_crate_local`, the relocation guards, and an
>   `#[ignore]` `package_file_sets_are_correct` (runs `cargo package --list` for the
>   two key crates: server's packaged `embedded/**` **equals** the authoritative set
>   exactly; CLI has README + both licences; none contains `web/dist` / `node_modules`
>   / `target/cratevista/site` / temp comparison dirs). `std`+`cargo` only, cross-OS.
> - **All nine packaged.** `cargo package --list` succeeds for all nine, and `cargo
>   package --workspace --locked --no-verify --allow-dirty` produces all nine
>   `.crate` archives (publication order schema → metadata → rustdoc → graph → config
>   → server → watch → core → cargo-cratevista). *(Per-crate `cargo package -p <c>`
>   for a crate with unpublished internal deps errors on the crates.io lookup; the
>   `--workspace` form treats the workspace crates as mutually available. **No
>   `cargo publish` / `--dry-run` was run.**)* **Extracted-package buildability is
>   deliberately NOT claimed here** — that, plus the offline local registry and the
>   three-OS install matrix, is **Phase 5B**.
> - **Eight reverted negative controls** each failed their target only while reverted:
>   Vite writes `web/dist` → relocation test fails; rust-embed reads `../../web/dist`
>   → server build fails; stale embedded file → `check:dist` STALE; a dropped internal
>   `version` → topology test fails; README pointed outside the crate → readme-local
>   test fails (+ Cargo warning); one licence byte → drift test fails; excluded
>   `embedded/index.html` → package check fails; `include = ["embedded/**"]` only →
>   `src/**`/`build.rs` absent from the package.
> - **Gates:** `check:types`, `typecheck`, lint (0 errors), `vitest` (280), `build`,
>   `check:dist`, Playwright E2E (**82**), `check:embed-rebuild`; `cargo fmt`,
>   `clippy --workspace -D warnings`, `cargo test -p cratevista-server`,
>   `-p cargo-cratevista`, `cargo test --workspace`, `cargo +1.97.1 check`, the stable
>   metadata-only real-CLI build test, the licence-drift + package-file-set checks,
>   `cargo package --list` × 9, `cargo package --no-verify` × 9, `git diff --check` —
>   all green. **No new dependency; `Cargo.lock` unchanged.** No local registry or
>   install matrix started; INDEX unchanged; ADR-0009 stays Proposed.

> **Phase 5B — offline local-registry package-then-install harness + CI matrix —
> IMPLEMENTED + HARDENED (current host green) 2026-07-18. PRD 10 stays Approved.
> Implementation-sequence step 5 stays **incomplete**: the three-OS acceptance is
> not checked until the real ubuntu/macOS/windows matrix legs pass on GitHub
> Actions — which **cannot be triggered from the current environment** (no git
> remote is configured and `gh` is not installed). No `release.yml`, no `cargo
> publish`, no release/hosting/launch docs.**
>
> - **Pinned tool.** `cargo-local-registry 0.2.12` (`CARGO_LOCAL_REGISTRY_VERSION`
>   in `ci.yml`), installed `--locked`, builds and runs on Rust 1.97.1 and exposes
>   both `sync` and `add`. Recorded help: `sync <LOCK> [PATH]` (with `--host`,
>   `--git`, `--no-delete`); `add <CRATE_NAME> [PATH]` (with `--version`, `--host`).
> - **Load-bearing preflight (Part 1) PASSED and now GATES the harness.** An
>   unpublished `cratevista-local-registry-probe 0.0.1` (outside the workspace, no
>   third-party dep) is packaged, added to a `local-registry`, and installed from a
>   **fresh `CARGO_HOME`** with `--locked --offline` (source replaced crates-io →
>   local), then executed (`cratevista-probe-ok`). **Cargo 1.97.1 accepts an
>   unpublished package in a replaced source** — the Part-1 stop condition did **not**
>   trigger. `run_preflight` runs FIRST inside the full harness, so a broken locked
>   mechanism halts everything before any packaging/assembly (Part-4 control #7).
> - **`add` limitation (empirical) — mechanism decided, PRD amended.** `cargo
>   local-registry add` cannot inject an unpublished local `.crate`: its `main()`
>   strips `[source]` replacement and resolves the named crate from crates.io, so
>   `add cratevista-schema --version 0.1.0` fails *"no matching package … location
>   searched: crates.io index"* (reproduced empirically, in the tool source, **and**
>   as a permanent negative control). Per the maintainer decision + the **Decision-8
>   Phase-5B correction** above: the third-party base is built with the tool's `sync`,
>   and the nine internal index entries are written in the tool's own index format.
> - **Index rows are derived from the packaged, normalized manifest (hardening).**
>   Every internal index row is built from the Cargo-**normalized** `Cargo.toml`
>   **extracted from that exact `.crate` archive** (via `flate2` + `tar`, so the
>   read is deterministic on all three OSes and needs no ambient `tar`), bound to the
>   archive by package name + version, with the entry's `cksum` = the SHA-256 of the
>   same archive bytes. Package/rename names, version requirement, kind, target,
>   optional, default-features, features, the feature table, `links` and
>   `rust-version` all come from that manifest; **workspace `cargo metadata` is used
>   only as an independent nine-member cross-check**, never as an index-row source.
>   An internal dep lacking the `0.1.0` version, or carrying any path/git source, is
>   rejected before the row is written; the finished line is re-validated (compact,
>   lowercase-hex cksum, no path/git/absolute/`..`).
> - **Reusable harness (one adjacent integration-test file; no xtask, no 10th
>   crate).** `crates/cargo-cratevista/tests/local_registry.rs`; harness-only dev-deps
>   `serde_json`, `toml`, `sha2 0.11`, `flate2`, `tar` (`serde_json`/`toml` were
>   already in `Cargo.lock`; `sha2`/`flate2`/`tar` + their trees are pinned there
>   now) — **none enters the installed binary**. Tests: two `#[ignore]` (the
>   preflight and the full install harness, each runnable `-- --ignored --exact
>   --nocapture`) plus one fast, non-ignored provenance unit test
>   (`index_rows_are_derived_from_the_packaged_manifest`). The harness creates every
>   temp dir, uses forward-slash absolute paths in generated TOML (Windows safe),
>   prints every child command, captures stdout/stderr on failure, never touches the
>   developer's `CARGO_HOME`/cargo bin, and installs from a cwd outside the workspace.
>   `CRATEVISTA_PACKAGE_ALLOW_DIRTY=1` permits a dirty local tree (CI runs clean);
>   `CRATEVISTA_KEEP_REGISTRY_TMP=1` preserves the temp tree on failure.
> - **Package + assemble + install (Parts 3–9) PASSED on the current host
>   (Windows).** `cargo package --workspace --locked --no-verify` into a dedicated
>   `--target-dir` → exactly nine `0.1.0` archives (no stale/dupe possible in a fresh
>   dir; `--no-verify` leaves no unpacked manifest, so the normalized manifest is
>   read from each `.crate`). `cargo local-registry sync` vendored the third-party
>   base and skipped the nine internal path crates (asserted). The re-read validation
>   asserts the **third-party archive count equals the lock's registry-sourced
>   package count derived from `Cargo.lock`** (currently **270**; not hard-coded),
>   nine internal archives, one index row each with matching name/vers/cksum and
>   `yanked=false`, internal reqs pin `0.1.0`, `cargo-cratevista → cratevista-core
>   0.1.0`, core's full internal set present, and — **from the archives themselves**
>   — the server `.crate`'s `embedded/**` equals the authoritative embedded set
>   exactly and the CLI `.crate` carries `README.md` + both licences. A **fresh
>   `CARGO_HOME`** (source replaced) + `CARGO_NET_OFFLINE=true` + `--locked --offline`
>   installed `cargo-cratevista 0.1.0` from a clean external cwd; the installed `cargo
>   cratevista --help` lists `build` (not "unimplemented") and `cargo cratevista
>   build` against a **copied** metadata-only fixture produced a valid site (marker
>   **C**, no `output_key`; static-mode marker exactly once; fingerprinted
>   `assets/**.js`; no `source/`, `snippets/` or `/api`).
> - **No Node (Part 8) PROVEN by executable shims.** `node`/`npm`/`npx` poison shims
>   (each records its call and exits non-zero) are prepended to `PATH` for the install
>   and installed-binary steps; no marker file exists afterward. Node is not set up in
>   the CI job either.
> - **Negative controls — green.** Install-level (each with its **own fresh
>   `CARGO_HOME`** so the cache cannot mask a defect): missing `cargo-cratevista`
>   archive; missing `cratevista-core` archive (archive gone, index row remains);
>   missing a **required** third-party crate (`clap`) → fails offline **without
>   fetching**; a **wrong internal checksum**; a **missing internal index row**
>   (row gone, archive remains); a broken internal requirement — each fails the
>   install. Assembler/provenance-level (fast unit test): duplicate/overwrite guard,
>   a path-bearing/un-normalized manifest, a version-less internal dep, and a
>   manifest/archive identity mismatch are all rejected; the row tracks the manifest
>   bytes (a third-party version bumped only in the manifest changes the row).
>   Tool-level: `cargo local-registry add` on an unpublished internal crate fails and
>   writes no entry (robust with or without network). Plus the Node poison self-test,
>   and the preflight gate. Policy controls at the call site: fresh-`CARGO_HOME`
>   isolation (≠ the developer's), `--offline` always present, no `--path`,
>   outside-workspace cwd.
> - **CI matrix (Part 11).** `ci.yml`'s old `cargo install --path` job is replaced by
>   `package-install` on **ubuntu/macos/windows-latest**, pinned stable 1.97.1:
>   checkout → toolchain → `cargo fetch --locked` → install pinned
>   cargo-local-registry → package-file-set + licence-drift checks → the preflight →
>   the full ignored harness → upload the harness log on failure. **No Node setup.**
>   Each leg packages and installs its own set (no cross-OS artifacts).
> - **Truthfulness — hosted matrix NOT run.** The three-OS matrix is implemented but
>   has **not** executed on GitHub Actions, and **cannot be triggered from the
>   current environment** (no git remote; `gh` absent). Executed locally on
>   **Windows** (`x86_64-pc-windows-msvc`, Rust 1.97.1, cargo-local-registry 0.2.12):
>   the fast provenance unit test, the preflight, and the full hardened harness are
>   green (harness ~46s). Per the CI-truthfulness rule, Implementation-sequence step 5
>   stays **incomplete** and the three-OS / local-registry-install and package-file-set
>   acceptance boxes stay **unchecked** until the real ubuntu/macOS/windows legs pass
>   on the same final changeset. The maintainer must push the branch and run the
>   `package-install` workflow to complete Step 5.
> - **Gates (current host):** `cargo fmt --all -- --check`, `clippy --workspace
>   --all-targets --all-features -D warnings`, `cargo test --workspace
>   --all-features` (incl. the fast provenance test), `cargo +1.97.1 check --workspace
>   --all-features`, the licence-drift + package-file-set checks, the preflight, the
>   ignored install harness, the installed `--help` + metadata-only build,
>   `git diff --check` — all green. No `npm`/Playwright (frontend unchanged). PRD stays
>   Approved; ADR-0009 Proposed; INDEX unchanged. Phase 6 not started.

> **Phase 6 — release/publish workflows + public documentation + dependency
> licensing — LANDED 2026-07-18 (current host green; workflows delivered, NOT
> executed). PRD 10 stays Approved. This completes Implementation-sequence step 6.
> Step 5 (hosted package-install matrix) and Step 7 stay incomplete; ADR-0009 stays
> Proposed; INDEX PRD-10 stays Approved.**
>
> - **`release.yml` (delivered, not triggered).** Trigger is a pushed SemVer tag
>   `v[0-9]+.[0-9]+.[0-9]+` only — never a branch push or PR. Top-level
>   `permissions: {}`; the build/validate jobs get `contents: read`, only the upload
>   job gets `contents: write` (no `packages`/`id-token`/`actions`). Pinned stable
>   1.97.1; **no nightly**. A `validate` job asserts the tag version equals the
>   `[workspace.package]` version and all nine package versions (`--locked`). The
>   `build` matrix builds the **exact four** targets on native runners (no
>   cross-compile): `x86_64-unknown-linux-gnu` (ubuntu-latest), `aarch64-apple-darwin`
>   (macos-latest), `x86_64-apple-darwin` (macos-13), `x86_64-pc-windows-msvc`
>   (windows-latest). Each leg runs the frontend reproducibility gates (`npm ci`,
>   `check:dist`, `check:embed-rebuild` — comparison only) then `cargo build --release
>   --locked -p cargo-cratevista --target <t>`, embedding the committed `embedded/`
>   bytes. Archives + checksums come from the **shared** helper (below). The `release`
>   job downloads all archives, re-verifies every SHA-256, checks the complete
>   four-target set (no missing/extra), and creates the GitHub Release via `gh`. No
>   signing/SLSA/attestation (deferred).
> - **`publish.yml` (delivered, never executed).** `workflow_dispatch` **only** — no
>   tag/push trigger, so a release tag cannot start it. Runs in the protected
>   `release` environment, requires a `PUBLISH` confirmation input, validates the
>   requested version against the workspace, and `cargo publish`es the nine crates in
>   the Decision-8 DAG order (schema → metadata → rustdoc → graph → config → server →
>   watch → core → cargo-cratevista), stopping on first failure, token via the
>   `CARGO_REGISTRY_TOKEN` environment secret. `permissions: contents: read` only. No
>   automated `--dry-run`, no name-availability check, no retry/continue-on-error;
>   index propagation between dependent publishes is documented, not papered over.
> - **Shared, deterministic archive helper.** `crates/cargo-cratevista/tests/release_archive.rs`
>   (harness-only dev-dep `zip` added alongside `flate2`/`tar`/`sha2`) assembles
>   `.tar.gz` (Unix) / `.zip` (Windows) with exactly one top-level dir holding the
>   binary + `LICENSE-MIT` + `LICENSE-APACHE` + `README.md` + `CHANGELOG.md` (Unix exec
>   bit set), plus one `sha256sum`-format `<archive>.sha256`. Fixed mtimes/timestamps
>   → byte-deterministic given identical inputs (no binary-reproducibility claim). One
>   implementation is shared by `release.yml`, the CI smoke matrix, and the local
>   smoke — no per-leg shell. Fast tests: exact four targets + names, exact content
>   set + exec bit, tar.gz/zip shapes, checksum round-trip + corruption/wrong-digest,
>   missing-binary/licence assembly failure, release.yml declares exactly the four
>   targets (no stray). All green on the current host (Windows).
> - **Documentation.** Root `README.md` expanded (value prop + differentiation from
>   rustdoc HTML / static dep graphs / terminal inspectors; from-source/crates.io
>   (future)/binary install; first run; the six commands; supported inputs;
>   stable-vs-nightly with the exact tuple `nightly-2026-07-01`/format 60/rustdoc-types
>   0.60.0/adapter 1; manual-flow + static-build examples; hosting link; privacy;
>   known limitations; contributing/security/licence). Crate-local
>   `crates/cargo-cratevista/README.md` aligned (compact, absolute repo links, no
>   publication overclaim). `SECURITY.md` expanded with the static-site privacy/threat
>   model. New `docs/hosting.md` (root/subpath, `--base-path`, GitHub/GitLab Pages, CI
>   artifacts, `file://` unsupported, zero `/api`, no EventSource, cache guidance). New
>   `docs/launch-checklist.md` (automated evidence vs maintainer review vs manual gated
>   actions vs never-performed; name availability a final manual gate; the
>   `[Unreleased]` → `[0.1.0]` transform). New `docs/launch/announcement-drafts.md`
>   (draft-only, no overclaim). `CHANGELOG.md` records the PRD-10 work under
>   `[Unreleased]` (no release date fabricated). `docs/adr/0009` updated to the
>   implemented decisions (three-state marker, key-scoped recovery, fs4 lock, no
>   `include` list, the archive helper + release/publish workflows, licensing) —
>   **status stays Proposed**.
> - **Dependency licensing.** Rust: pinned `cargo-about` (`CARGO_ABOUT_VERSION` in CI),
>   `about.toml` (accepted SPDX allowlist, four targets, dev-deps excluded), `about.hbs`
>   (deterministic, no timestamps/paths), report at `docs/licenses/rust-dependencies.md`
>   (reproduces byte-identical under `--frozen`; 159 shipped crates, all permissive).
>   Web: `web/scripts/license-report.mjs` over `package-lock.json` + installed metadata
>   (no large dep added; SPDX-id matching, no name inference, unparseable compound →
>   fail), reports at `docs/licenses/web-dependencies.{json,md}` (439 packages) with a
>   `--check` drift/policy gate. Recorded exceptions (EPL-2.0 elkjs runtime; MPL-2.0
>   dev tools; CC-BY-4.0 caniuse data) documented in `docs/licenses/README.md`. Both
>   reports path-free. Negative control proven: removing an accepted id makes
>   `cargo about` exit 1 (`unicode-ident`'s `Unicode-3.0`), reverted.
> - **`ISSUES/issue_13_static_source_snippets.md`** created as a specification shell
>   (allowlist, manifest, path→URL mapping, per-file/total caps, UTF-8/binary,
>   symlink rejection, duplicate collapse, change-during-build, hard secret deny-list,
>   privacy, determinism, collisions, CSP/MIME, no reliance on auto secret detection;
>   states the current release writes no snippets). **Not** a PRD, **not** in
>   `PRD/INDEX.md`.
> - **Workflow-boundary + bookkeeping tests** (`crates/cargo-cratevista/tests/workflows.rs`):
>   release-tag-only trigger, `contents: write`-only + no signing tokens, pinned-stable
>   /no-nightly; publish is dispatch-only/not-tag-triggered, protected `release` env +
>   confirmation + secret token + no write perms, DAG publish order, no automated
>   dry-run; ADR-0009 Proposed, PRD-10 Approved (not Verified), INDEX PRD-10 Approved,
>   issue 13 a shell not an approved PRD. All green.
> - **Gates (current host, Windows/1.97.1):** `cargo fmt --all -- --check`, `clippy
>   --workspace --all-targets --all-features -D warnings`, `cargo test --workspace
>   --all-features`, `cargo +1.97.1 check --workspace --all-features`, the release_archive
>   fast tests + the ignored `release_archive_smoke`, the workflows tests, `cargo about
>   generate --frozen` + `npm run license:web:check`, `git diff --check` — all green.
>   **Truthfulness:** the release/publish workflows are **delivered but not executed** —
>   no tag pushed, no GitHub Release, no `cargo publish`, no `cargo publish --dry-run`,
>   no announcement, no remote configured. PRD stays Approved; ADR-0009 Proposed; INDEX
>   unchanged.

> **Phase 7 — end-to-end verification and final local closure — IMPLEMENTED
> 2026-07-18: current-host verification green; hosted closure pending. PRD 10 stays
> Approved. Implementation-sequence Step 7 is implementation-complete but stays open,
> and Step 5 stays incomplete, until the real Linux/macOS/Windows GitHub-hosted legs
> pass on the same final changeset. ADR-0009 stays Proposed; INDEX PRD-10 stays
> Approved; no hosted-dependent acceptance box is checked.**
>
> - **Real produced-site browser E2E (7A).** `web/e2e/tests/static-export.spec.ts`
>   drives a site from the **real `cargo cratevista build`** (not `embedded/` served
>   directly) at **both** a subpath (`/cratevista/`) and the **URL root**: the
>   explorer loads, a view renders, a node selects, query-string state survives a
>   refresh, relative JS/worker/artifact URLs resolve, and the unmodified
>   `document.json` yields a safe repository-root link (`target=_blank`,
>   `rel="noopener noreferrer"`, no current-tab navigation, no source deep link while
>   `default_branch` is absent). A pre-app guard fails on **any** path matching
>   `/api/` (not a fixed list) and on **any** `EventSource` construction; the header-less
>   artifact triple loads coherently. No synthetic repository metadata is injected.
> - **Produced-site privacy scan (7B).** `crates/cratevista-core/tests/build_static.rs`
>   builds a real site from a bin-only fixture in a temporary **user** path (stable, no
>   nightly) and scans `index.html` + the three JSON artifacts for absolute/UNC/drive
>   paths, `/home/`, `/Users/`, the real username, the workspace/fixture path,
>   `CARGO_HOME`/`RUSTUP_HOME`, rustdoc/argv fragments and credential/secret patterns —
>   and **proves the scan detects an injected absolute path + credential** before
>   reverting the control. The gated nightly test repeats the scan on a real
>   rustdoc-generated site.
> - **Determinism (7C).** Two builds from unchanged input yield byte-identical
>   `document.json` and `diagnostics.json`, an identical embedded-asset **set and
>   bytes**, and an identical `index.html` under the fixed `Clock` seam;
>   `generation.json` may differ only in `generated_at`. No binary-reproducibility
>   claim is introduced.
> - **Full-pipeline nightly (7D).** `build_live.rs` (gated, `nightly-2026-07-01`) runs
>   `run_build → real rustdoc JSON → graph → committed artifacts →
>   materialize_static_site` on a documentable fixture and asserts rustdoc
>   **format 60** (from `generation.json`; rustdoc-types 0.60.0 / adapter 1 are the
>   pinned `cratevista-rustdoc` constants exercised by producing a real document),
>   marker **C**, embedded assets, the three artifacts, **no** snippets, the privacy
>   scan, and output outside protected inputs. A dedicated gated **nightly CI job**
>   runs it (no Node).
> - **README first-run (7E).** `ci/readme-first-run.sh` verifies the documented
>   commands in a **fresh CARGO_HOME / install root / target / outside-repo fixture**:
>   the **stable** path installs from source (`--path`, `--locked`, **no Node, no
>   nightly**) and runs `--help`, `doctor` and a metadata-only `build`; the **nightly**
>   path runs a real `generate` under the pinned nightly. A drift guard asserts the
>   README still documents every command the script runs; the crates.io line is
>   documentation-only and never executed. A **no-Node** CI job runs the stable
>   portion; the nightly job runs the generation portion. **Stable portion passed on
>   the current host.**
> - **Release-artifact verification (7F).** `release_archive.rs::release_archive_smoke`
>   assembles an archive from the compiled binary via the shared helper, verifies its
>   checksum, extracts it, runs the extracted binary's `--help` (lists `build`), and
>   proves corruption fails. A non-publishing **`release-smoke` CI matrix** exercises
>   the same helper on all four release runners/targets (build release → assemble →
>   `sha256sum -c`) without creating a release. **Passed on the current host
>   (Windows/zip).**
> - **Licensing verification (7G).** Rust: pinned `cargo-about --frozen` (offline,
>   deterministic; committed report reproduces byte-identical) + a CI drift/policy gate
>   (`git diff --exit-code`); a disallowed licence makes `cargo about` exit 1 (proven
>   with `unicode-ident`, reverted). Web: `npm run license:web:check` (drift + policy,
>   no name inference, unparseable compound → fail). Reports are path/timestamp-free.
> - **Documentation verification (7H).** `crates/cargo-cratevista/tests/docs_integrity.rs`:
>   internal Markdown links resolve; no `gamesrv`/`docs/references/` residue; no stale
>   `web/dist` instruction; no publication/snippet/binary-reproducibility/`file://`
>   overclaim; correct stable-vs-nightly wording + tuple; `Aleksandr Skibin` authorship
>   inherited by all nine crates.
> - **CI organization (7J).** `ci.yml` keeps concerns separated: `lint` (+ Rust licence
>   drift/policy), `test` matrix (includes the new workflow/docs/archive/build_static
>   guards), `explorer` (frontend/browser + web licence check), `package-install`
>   matrix (Phase 5B), `release-smoke` matrix, `readme-first-run` (no Node),
>   `nightly-pipeline` (the only nightly job). No Node in package-install/readme-first-run;
>   no nightly in stable/release jobs; tool versions pinned.
> - **Negative controls (7K).** Discriminating tests/live demos for: one `/api/`
>   request or an `EventSource` fails the E2E (guards); an injected absolute
>   path/credential fails the privacy scan (self-test, reverted); two differing builds
>   fail determinism; a README stable-generation or crates.io-available claim fails the
>   doc checks; an omitted licence fails the archive-content test; a corrupted archive
>   or wrong digest fails checksum verification; an unexpected release target fails the
>   matrix assertion; a tag trigger on publish, or a lost protected environment, fails
>   the workflow-boundary tests; a disallowed dependency licence fails the policy
>   (demonstrated live with cargo-about, reverted); flipping ADR-0009 to Accepted or
>   PRD/INDEX to Implemented/Verified fails the bookkeeping tests.
> - **Current-host gates (Windows / 1.97.1 / cargo-about 0.7.1):** `cargo fmt --all --
>   --check`, `clippy --workspace --all-targets --all-features -D warnings`, `cargo test
>   --workspace --all-features`, `cargo +1.97.1 check`, the ignored `release_archive_smoke`,
>   `build_live` (nightly), `build_static`, the Phase-5B preflight + full offline install
>   harness, `cargo about generate --frozen` + `npm run license:web:check`, the full
>   frontend suite, `git diff --check`, `cargo metadata` author check, `git grep -ni
>   gamesrv` / `reference-repositories.local` (none) — results in the Phase-7 gate run.
> - **Hosted closure pending.** The GitHub-hosted matrices — Phase-5B package-install on
>   ubuntu/macOS/windows, the release-smoke matrix, and the README/nightly jobs — are
>   **implemented but not executed** (no remote configured, `gh` absent). Per the
>   CI-truthfulness rule, Step 5 and Step 7 stay open, no hosted-dependent acceptance box
>   is checked, PRD stays Approved, ADR-0009 Proposed, INDEX unchanged. No tag, release,
>   `cargo publish`, `cargo publish --dry-run`, announcement or remote was created.

## Testing strategy (acceptance → evidence)

### Unit tests

- **Base-path parse/normalize/reject** (`cratevista-core`): `""`, `"/"`, `"repo"`,
  `"/repo"`, `"/repo/"`, `"a/b"` normalize as Decision 3; schemes, `?`, `#`, `..`,
  backslash, whitespace, control chars are rejected with `build_invalid_base_path`.
- **`<base href>` + marker injection**: the injected `index.html` contains exactly
  one `<base>` (only when a base path is given) and exactly one `cratevista-mode`
  meta; no `<script>` is added (CSP-safe); asset refs stay `./assets/…`.
- **`embedded_assets()`** (`cratevista-server`): yields `index.html` and the hashed
  `assets/**`; the set equals what `serve_path` serves.
- **Frontend loader endpoints** (`web`): `loadArtifacts` fetches the given triple
  (server `/api/*` vs static `./*.json`); the all-absent-header coherence rule and
  cancellation semantics (PRD 09) are unchanged; `StaticArtifactSource` opens no
  `EventSource` and issues no `/api/*` request.

### Materialization tests (`cratevista-core`, real filesystem, **no cargo, no nightly**)

These drive **`materialize_static_site`** directly against a committed
`target/cratevista/` snapshot — they do **not** call `run_build`, so they never run
generation. (A preseeded snapshot does not bypass `run_generate` in `run_build`; it
only feeds `materialize_static_site`.)

- **Materialization**: a committed snapshot → `<output>` contains `index.html`,
  `assets/**`, the three JSON files and `.cratevista-static-site.json`;
  `document.json` is schema-valid.
- **Staged rollback**: a forced failure during staging leaves an existing owned
  `<output>` **byte-identical** and leaves no staging directory; a failed *first*
  build creates no `<output>`.
- **Publish rollback**: a forced rename failure at publish step 3 **restores** the
  backup so the old owned `<output>` is byte-identical; a forced restore failure
  preserves **both** backup and staging and reports `build_publish_unrecoverable`.
- **No mixed site**: rebuilding over an owned `<output>` yields only new files — no
  old/new mixture is ever observable.
- **Ownership**:
  - an existing non-empty directory **without** the marker → `build_output_not_owned`,
    the directory is **untouched**;
  - a **malformed / unknown-version** marker → `build_output_marker_invalid`,
    untouched;
  - an **unowned directory with a similar name** (e.g. a user's own `site/`) is
    never deleted;
  - `<output>` that is a **symlink** → `build_output_symlink`, untouched.
- **Empty-output adoption + rollback** (against a fixed protected set):
  - a successful build **adopts** an existing empty `<output>` and publishes into it;
  - a forced `staging → output` rename failure **recreates the empty `<output>`**
    and fails `build_publish_failed` (the empty predecessor is restored);
  - a forced failure to recreate the empty `<output>` fails
    `build_publish_unrecoverable` and **preserves staging**;
  - an empty `<output>` is **never** turned into a backup and never gets a
    `kind: "site"` marker.
- **Path safety** (against an explicit protected set — no cargo): rejected with
  `build_output_forbidden` when `<output>` **equals or is an ancestor of** a
  protected path, and **accepted** otherwise:
  - `<workspace>/dist` (the documented `--output dist`) → **accepted**;
  - `target/cratevista/site/` (the default) → **accepted**;
  - `<output>` **equal to** an input → **rejected**;
  - `<output>` an **ancestor of** an input → **rejected**;
  - the workspace root and `target/cratevista` (and any ancestor of either) →
    **rejected**;
  - an unrelated descendant directory inside the workspace (containing no input) is
    **not** rejected merely for being inside the workspace or a source/config root.
- **`output_key` existence-stability**: derive the key while only a **distant**
  ancestor exists; create the intermediate parent directories; derive again → the
  key is **identical**. Two distinct output paths produce **distinct** scoped names.
  Non-UTF-8 Unix paths (behind a `#[cfg(unix)]` test) and non-ASCII Windows paths
  (behind a `#[cfg(windows)]` test) are supported and stable.
- **Marker timing**: **marker A** (`kind: "staging"` + `output_key`) is the **first
  authoritative file** written into a fresh staging directory (before any
  asset/artifact); it is upgraded to **marker B** after content, then finalized to
  **marker C** at `<output>` last. Because `mkdir` and the marker-A write are
  **separate** filesystem operations, a process killed in the mkdir → marker-temp →
  rename window can leave a **P0 pre-marker shell**: an empty (or temp-only) keyed
  staging directory with no authoritative marker. The test asserts that once marker A
  is authoritative every later state is marker-classifiable, and that the P0 shell is
  conservatively recognizable and contains no content.
- **P0 pre-marker shell recognition** (strict, fail-closed): a directory is treated
  as a P0 shell **only** when its name is exactly `.cratevista-<current-output-key>-
  staging-<valid-nonce>`, it carries **no** authoritative marker, and its entries are
  **either none or only** regular non-symlink `.cratevista-static-site.json.tmp-<valid-
  nonce>` files. A subdirectory, symlink, asset, artifact, foreign temp, or any
  unrelated file makes it **not** a P0 shell; the whole directory is then preserved
  and never deleted. (P0 is a recovery classification, **not** a fourth marker
  schema — A/B/C remain the only three.)
- **Atomic marker transitions**: an **injected marker-write failure** at each of the
  three transition points (A create, A→B, B→C) **never leaves a partially written
  authoritative `.cratevista-static-site.json`** — only a discarded `.tmp-*` file —
  because every transition is write-temp → flush → **rename-over**.
- **Interrupted-publication recovery** — one deterministic test per row of the
  recovery table (A–F), plus the `<output>`-marker resolution:
  - A: stable published `<output>` (marker C) → this key's staging (marker A or B)
    and backups cleaned, output kept;
  - B: absent output + exactly one valid backup for this key → backup **restored
    first**, then staging cleaned;
  - C: absent output + multiple valid backups for this key → `build_recovery_ambiguous`,
    all directories preserved (identifiers are safe relative names);
  - D: absent output + this key's staging, no backup → staging removed, first build;
  - E: unmarked / malformed / **other-`kind`** / **mismatched-`output_key`** staging
    or backup → never touched;
  - F: forced restoration failure → `build_publish_unrecoverable`, all paths kept.
- **Crash-window recovery** — inject termination at each publication point and assert
  the deterministic recovery state:
  - **after `mkdir`, before any marker-temp** → an empty keyed staging directory is a
    **P0 shell**; recovery removes it (a valid `<output>` is kept) and never treats
    the absent marker as malformed authoritative state;
  - **after the marker-temp is created / partially written / flushed, before the
    rename-over** → the keyed staging directory holds only a `.tmp-*` file and no
    authoritative marker; it is a **P0 shell**, removed as empty content, and the
    half-written temp is **never** parsed as an authoritative marker;
  - **after marker A written** (incomplete staging) → discarded as an incomplete
    candidate; a valid `<output>` is kept;
  - **after marker B written, before `staging → output`** (completed candidate in a
    staging dir) → discarded, never auto-published; a sole backup is restored first;
    multiple backups → `build_recovery_ambiguous`;
  - **after `staging → output`, before marker C** (`<output>` carries marker B with a
    matching key) → the next build **finalizes marker C and preserves the newly
    published `<output>`**, rather than discarding it or restoring the older backup;
    a marker B at `<output>` with **another** key → `build_output_marker_invalid`;
  - **after marker C, before backup cleanup** → `<output>` is the stable site; the
    leftover keyed backup is cleaned.
- **Marker-write-failure typed result**: a forced marker-write failure **before** the
  publish rename → staging failure, predecessor untouched; **after `staging → output`
  but before marker C** → `<output>` (marker B) and the backup are **preserved**,
  `build_publish_unrecoverable`, so the next run finalizes marker B → C safely.
- **Cross-output isolation** (two outputs `site-a`, `site-b` under one parent):
  - `site-a` recovery **never** touches `site-b`'s keyed staging or backup;
  - `site-a` **cannot** restore `site-b`'s sole backup (different `output_key`);
  - a keyed directory whose marker's `output_key` **mismatches** its name is
    **preserved**, never touched;
  - two builds for **different** outputs in one parent run to completion isolated
    (neither blocks nor mutates the other's temporaries).
- **Per-output locking** (interprocess, deterministic — a real second process/handle,
  no sleeps):
  - two builds for the **same** output: one holds the lock, the other returns
    **`build_output_busy` before any scan/rename/delete/staging/output mutation**;
  - an **unlocked leftover** `.cratevista-<key>.lock` file does **not** block a build;
  - after the lock holder **terminates**, the OS releases the lock and the next
    build acquires it and performs normal recovery;
  - a lock holder's **active staging** is never deleted by another concurrent build
    (that build is `build_output_busy`, so it never scans);
  - builds for **different** `output_key`s use different locks and neither blocks.
- **Missing-output-parent preparation** (the locked prepare → re-resolve → lock
  sequence, Decision 2):
  - **existing parent, normal path**: `<output>`'s parent already exists → no
    directory is created before the lock; `--output dist/site` with `dist/` present
    prepares nothing and proceeds directly to lock acquisition;
  - **nested missing parents**: `--output dist/site` (or a deeper `a/b/c/site`) with
    the whole chain absent → only the **missing parent chain** is created (never
    `<output>` itself); re-resolving after creation yields the **same** resolved
    identity and **same** `output_key`, and the lock is then taken on the prepared
    parent;
  - **identity/key change after creation fails closed**: if re-resolution after
    parent creation changes the resolved identity or `output_key`, or reveals a
    symlinked component, the build **fails before** acquiring the lock or touching
    any output/staging/backup state;
  - **contention leaves only the parent chain**: two same-output processes both
    idempotently prepare the (identical) missing parent chain and then contend on the
    **same** lock; the loser returns **`build_output_busy`** having performed **no**
    output/staging/backup/recovery/publication mutation, and the only filesystem
    residue permitted is the safely created empty parent chain — `<output>` itself is
    never created;
  - **distinct nested outputs → distinct locks**: two different nested outputs under a
    shared grandparent each prepare their own parent and acquire **different** locks;
    neither blocks nor mutates the other.
- **`output_key` ownership (no forged key)**: `OutputSafety` is constructed from the
  output itself (`OutputSafety::for_output(output, protected)`, private key); a
  caller-supplied or mismatched `output_key` — one that does not equal a freshly
  derived key for `<output>` — is rejected as an **internal invariant / filesystem
  error before** the lock is opened or any candidate is scanned. The mismatch is
  **not** mapped to `build_output_busy` or an ownership error, and it triggers **no**
  candidate scan or mutation.
- **P0 cleanup safety**: a keyed directory that fails the strict P0 shape — because it
  holds a subdirectory, a symlink, an asset/artifact, a foreign temp, or any user
  file — is **never** deleted by recovery, even when its name matches the current
  key; only a directory satisfying the exact P0 shape (empty or temp-only, no
  authoritative marker, correctly keyed name, no symlink/subdirectory) is removed.
- **Determinism**: two materializations of unchanged input produce byte-identical
  `document.json` and an identical asset set; `generation.json` **may** differ
  (timestamp) and that difference is explicitly allowed.
- **Base path**: `--base-path /demo/` writes exactly one `<base href="/demo/">`;
  omitting it writes none; asset/artifact refs stay relative in both; the rejection
  set (schemes, `?`, `#`, `..`, backslash, whitespace) yields `build_invalid_base_path`.
- **Privacy scan**: the produced site's three JSON files and `index.html` contain
  no absolute path, username, argv or credential (Decision 7).

### Full-pipeline test (`cratevista-core`, **gated nightly**)

- One `#[ignore]`-gated `run_build` test on the pinned `nightly-2026-07-01` runs the
  real `run_generate → materialize_static_site` pipeline on a fixture workspace and
  asserts the produced site. This is the only static-build test that needs nightly;
  it is never run in the stable jobs.

### Repository-link tests (`web` unit/component + E2E)

- **GitHub HTTPS** → `…/blob/<branch>/<path>#L<line>`.
- **GitLab HTTPS** (incl. a subgroup) → `…/-/blob/<branch>/<path>#L<line>`.
- **`.git` and trailing-slash normalization** produce the same link.
- **Credential-bearing** (`https://user:pass@…`), **`ssh:` / `git:` / `git@…` /
  `file:` / malformed** → **no** link.
- **Unsupported valid HTTPS host** → repository-root link only, **no guessed file
  link**.
- **Missing** `repository_url` / `default_branch` / `SourceLocation` → no file link.
- Links are built with the URL API + per-segment encoding (no blind concatenation).

### Browser E2E (Playwright, real produced site)

- **Base-path hosting**: serve the produced `<output>` from an HTTP static server
  at a **subpath** (e.g. `/cratevista/`) → the explorer loads, a view renders, a
  node selects; **zero CSP violations, zero page errors, no full-page navigation.**
- **Static capability**: the produced site opens **no** `EventSource` and makes
  **zero requests to any `/api/**` route** — the test **fails on any request whose
  path matches `/api/`** (a match rule, not an allow/deny list). Extends the
  existing `static-export.spec.ts`, now pointed at a real `run_build` output.
- **Coherent triple with no headers**: the produced header-less JSON triple loads
  and renders (PRD-09 all-absent rule, end to end).
- **Repository links (corrected — Phase 4B/5A):** the real-produced-site E2E proves,
  from the **unmodified** `document.json`, that a present `repository_url` renders a
  safe **repository-root** link (`target=_blank rel="noopener noreferrer"`, no
  current-tab navigation), and that **no source deep link renders while
  `default_branch` is absent**. The exhaustive GitHub/GitLab forge-specific deep-link
  and unsafe-input matrix is **unit/component** evidence (`repository-links.test.ts`
  + `inspector-links.test.tsx`) until an authoritative `default_branch` source is
  specified; the produced-site E2E does **not** manufacture GitHub/GitLab deep links.

### Packaging / release tests (CI)

- `cargo package -p <crate> --list` assertions (Decision 8): server list includes
  `embedded/**`; `cargo-cratevista` list includes `README.md`, `LICENSE-MIT` **and**
  `LICENSE-APACHE`.
- **Licence drift check** (Decision 8.4): `crates/cargo-cratevista/LICENSE-MIT` and
  `LICENSE-APACHE` are asserted **byte-identical** to the root `LICENSE-*` files.
- **Local-registry install** (the Decision-8 locked mechanism), on
  Linux/macOS/Windows: `cargo package --no-verify` the nine crates, assemble a Cargo
  `local-registry` (workspace crates + Lock-pinned third-party `.crate`s), replace
  the source via a temporary `CARGO_HOME`, then `cargo install cargo-cratevista
  --version 0.1.0 --locked --offline` **from outside the repo** and run
  `cargo cratevista --help` and `cargo cratevista build` on the **metadata-only
  stable fixture (Decision 8a)**. The compile+install is the authoritative
  completeness/buildability proof. Uses only produced package contents — no
  workspace paths, no repo-local web assets, no network.
- **Release matrix**: `release.yml` builds the four targets, produces archives and
  `.sha256` files, and a verify step recomputes and matches each SHA-256.
- **README first-run**: a scripted CI job runs the README's documented install +
  first-run steps from a clean environment. It requires **no Node**. Two things are
  proven separately: (a) the CLI **builds and installs on stable Rust** with no
  nightly; (b) when a step actually runs *generation*, it installs and uses the
  documented **pinned nightly** (`nightly-2026-07-01`) — the script does not pretend
  generation works on stable.
- **License policy**: `cargo about` (Rust) and the `web/` license report run and
  their output is checked in / attached; a policy check fails on a disallowed
  license.
- **No automated publish or publish-`--dry-run`.** `cargo-local-registry` is an
  offline source, not a publishing service; the compile+install above is the
  authoritative gate. Actual crates.io-side validation is a **manual launch step**
  immediately before the separately gated real publication (`docs/launch-checklist.md`).

### Fixtures

Reuse a committed single-package snapshot (as `web/e2e/fixtures/normal`) plus a
`.cratevista/` sample; add a base-path (subpath) hosting scenario.

## Acceptance criteria

- [ ] `cargo cratevista build` produces the Decision-1 site; it renders from a URL
  root **and** from a subpath with no rewriting. *(core integration + base-path
  Playwright E2E)*
- [ ] **`build` owns only sites it created.** An existing non-empty `<output>`
  without a valid `.cratevista-static-site.json` marker is **never** modified
  (`build_output_not_owned`); a malformed marker and an output symlink are rejected
  without touching anything. *(ownership materialization tests)*
- [ ] **Path safety is replacement-danger-directional.** `<output>` equal to, or an
  **ancestor of**, a protected path (workspace root, `target/cratevista`, or a real
  input) is rejected (`build_output_forbidden`); the documented `--output dist`
  (`<workspace>/dist`) and the default `target/cratevista/site/` are **accepted**;
  a descendant that contains no input is not rejected merely for being inside the
  workspace. *(path-safety materialization tests, no cargo)*
- [ ] **Publication is transactional with rollback** (not fully atomic): no
  old/new mixture is ever observable; a failed staging leaves an existing owned
  `<output>` **byte-identical**; a forced publish-rename failure **restores** the
  predecessor — the backup for a site, or the **recreated empty directory** for an
  adopted empty `<output>`; a failed restore preserves staging for manual recovery.
  *(staged-rollback + publish-rollback + empty-output-rollback tests)*
- [ ] **`output_key` is stable when filesystem existence changes.** Deriving the key
  before and after the output's intermediate parent directories are created yields
  the **identical** key (the hashed input is the resolved full identity — the
  ancestor/remainder split never appears in it); distinct outputs get distinct scoped
  names; non-UTF-8 Unix and non-ASCII Windows paths are supported. *(output_key
  stability tests)*
- [ ] **The marker is a three-state A→B→C machine written first and finalized last.**
  Marker **A** (`kind:"staging"`+key) is written immediately after mkdir; upgraded to
  **B** (`kind:"site"`+key) after content; finalized to path-free **C** at `<output>`
  last. Every transition is a crash-safe **write-temp → rename-over**, so an injected
  marker-write failure never leaves a partially written authoritative marker.
  `output_key` is **never dropped before the `staging → output` rename**. *(marker-
  timing + atomic-transition tests)*
- [ ] **Interrupted-publication recovery** handles the recovery-table cases A–F **and
  the post-rename/pre-finalization window**, all **scoped to the current
  `output_key`**: a sole valid backup for this key is **restored** first; multiple
  valid backups yield `build_recovery_ambiguous`; a completed **marker-B staging**
  candidate is **discarded, never auto-published**; a **marker-B at `<output>` with a
  matching key** is **finalized to C and the newly published output preserved** (not
  discarded, not rolled back); marker B at `<output>` with another key, or marker A at
  `<output>`, → `build_output_marker_invalid`; **unmarked / malformed / other-`kind` /
  mismatched-`output_key`** directories are never touched; a failed restore or
  interrupted finalization yields `build_publish_unrecoverable`. *(recovery +
  crash-window tests at every publication point)*
- [ ] **Temporaries and recovery are output-scoped.** A build for `site-a` never
  inspects, deletes or restores `site-b`'s keyed staging/backup in the same parent,
  and cannot restore `site-b`'s sole backup; a mismatched-key directory is
  preserved. *(cross-output isolation tests)*
- [ ] **Per-output interprocess exclusion.** A second concurrent build of the **same**
  output returns **`build_output_busy` before any mutation** while the first holds
  the advisory lock; an unlocked leftover lock file does not block; after the holder
  terminates the OS releases the lock and the next build proceeds; builds for
  different `output_key`s never block one another. On contention `build_output_busy`
  performs **no** output/staging/backup/recovery/publication mutation, inspects or
  deletes **no** candidate, and **never** creates `<output>` — the only permitted
  residue is a safely created **empty parent chain**. *(interprocess locking tests)*
- [ ] **A missing output parent is prepared under the lock discipline.** Before any
  output-state access the build normalizes → resolves the nearest existing ancestor →
  runs symlink/safety checks → derives the resolved identity and `output_key` →
  creates **only** the missing parent chain (never `<output>`) → **re-resolves** and
  re-checks → asserts the identity and `output_key` are **unchanged** → **then**
  acquires `<output-parent>/.cratevista-<output_key>.lock`; only after the lock may
  recovery, inspection, staging or mutation begin. A re-resolution that changes the
  identity/key or reveals a symlink **fails before** mutating output state.
  Parent creation is idempotent and safe under concurrency; `--output dist/site`
  therefore has a deterministic parent-preparation path. *(missing-parent
  preparation tests)*
- [ ] **The ownership key is derived from the output, never trusted from the caller.**
  `OutputSafety` is constructed via `OutputSafety::for_output(output, protected)`
  (private key); a mismatched/forged `output_key` is rejected as an internal
  invariant / filesystem error **before** the lock is opened or any candidate is
  scanned, and is **not** mapped to `build_output_busy` or an ownership error.
  *(forged-key rejection test)*
- [ ] **The P0 pre-marker crash shell is recognized strictly and never destroys user
  data.** Because `mkdir` and the marker-A write are separate operations, a crash can
  leave an empty (or `.tmp-*`-only) keyed staging directory with no authoritative
  marker; recovery removes it **only** when it matches the exact P0 shape (correctly
  keyed name, no authoritative marker, entries none-or-temp-only, no
  symlink/subdirectory/asset/foreign file) and never parses a half-written marker
  temp as authoritative. Any keyed directory failing the strict shape is preserved.
  P0 is a recovery classification, **not** a fourth marker schema. *(P0 recognition +
  P0-cleanup-safety + crash-window tests)*
- [ ] The produced static site opens **no** `EventSource` and makes **zero
  requests to any `/api/**` route** (the E2E fails on any path matching `/api/`).
  *(static-capability E2E)*
- [ ] The header-less artifact triple loads coherently in the produced site.
  *(all-absent-header E2E + loader unit tests)*
- [ ] `--base-path` parsing/normalization/rejection behaves exactly as Decision 3.
  *(core unit tests)*
- [ ] Repository *links* are **provider-aware** (GitHub `/blob/`, GitLab `/-/blob/`,
  root-only for unsupported HTTPS, no link for ssh/git/file/credential/malformed or
  missing fields); **no** source snippets are written. *(repository-link unit +
  E2E; a test asserting no `source/` dir)*
- [ ] The produced site contains no absolute path, username, argv or credential.
  *(privacy-scan materialization test)*
- [ ] Every workspace crate packages with the correct file set; `cratevista-server`
  ships its committed `embedded/` bundle and `cargo-cratevista` ships `README.md`,
  `LICENSE-MIT` and `LICENSE-APACHE` (the crate-local copies), each byte-identical to
  the root licence files. *(`cargo package --list` assertions + licence drift check)*
- [ ] `cargo install` from the **local-registry package set** works with **no Node,
  no workspace and no network** (`--offline`) and exposes `cargo cratevista`, on
  Linux/macOS/Windows. *(local-registry install matrix)*
- [ ] The tagged release workflow builds the four targets, produces archives and
  matching SHA-256 sums, and uploads them; `cargo publish` is a **separate manual**
  job that this PRD never runs. *(release.yml jobs; publish job present but gated)*
- [ ] Reproducibility claims hold and no more are made: deterministic committed
  bundle (`check:dist`), reproducible package file sets (`--list`), deterministic
  `document.json`, checksummed archives — **binaries are not claimed byte-identical**.
  *(the named tests; no binary-reproducibility test exists)*
- [ ] README first-run instructions succeed from a clean environment. *(scripted CI
  job)*
- [ ] The public description distinguishes CrateVista from rustdoc HTML / static
  dep graphs / terminal inspectors. *(README differentiation section; review)*
- [ ] Rust- and frontend-dependency licensing is documented. *(cargo-about + web
  license report + policy check)*
- [ ] Security/privacy behavior — including the static-site contract — is
  documented. *(SECURITY.md + `docs/hosting.md`)*
- [ ] A launch checklist exists, and name availability is a **final manual gate**.
  *(`docs/launch-checklist.md`)*
- [ ] The stable CLI builds/tests/lints with no nightly; nightly is only for
  rustdoc JSON, and release docs explain the distinction. *(existing CI stable jobs
  green; README/docs section)*

Verification (documentation records the commands; none publishes):

```bash
cargo test -p cratevista-core --all-features      # materialize_static_site, ownership, rollback, base-path
cargo test -p cratevista-server --all-features    # embedded_assets
cargo test -p cargo-cratevista --all-features     # build CLI surface
cargo run -p cargo-cratevista -- cratevista build --output dist --base-path /demo/   # in a fixture
cd web && npm run test && npm run e2e             # loader + repo-link + produced-site E2E
cargo package -p cratevista-server --list         # must list embedded/**
cargo package -p cargo-cratevista --list          # must list README + licenses
# gated nightly: the one full-pipeline run_build test (nightly-2026-07-01)
# CI: local-registry install matrix (offline, no Node) + base-path E2E + release.yml + SHA-256 verify
```

## Non-goals (explicit)

- No new analysis, no remote/hosted ingestion, no AI summaries.
- **No source snippets** in the site (Decision 6 — deferred to issue 13).
- **No `[build]` config section** (Decision 5).
- **No `file://` support** (Decision 3).
- **No provenance/signing** in the first release (Decision 9 — SHA-256 only).
- **No publication, release creation, announcement, topic registration or
  permanent name-availability claim** performed by this PRD (Decision 10).

## Risks and mitigations

- **Embedded-bundle relocation touches the dist/embed/E2E plumbing** → phased
  (sequence step 5), each script/harness repoint verified by the existing
  `check:dist` / `check:embed-rebuild` gates before proceeding.
- **crates.io install fails on a hidden path-only or outside-crate file** →
  `cargo package --list` assertions + the offline local-registry compile+install
  matrix (the authoritative completeness/buildability proof); crates.io-side
  validation is a final manual launch step, not a CI gate.
- **Apple-silicon vs Intel Mac target confusion** → both `aarch64-apple-darwin`
  (macos-latest) and `x86_64-apple-darwin` (macos-13) built and checksum-verified.
- **Overclaiming in launch copy** → checklist item: every public claim maps to a
  tested, implemented feature; name availability is a final manual gate.
- **Base-path breakage on Pages** → relative URLs by default + subpath E2E; no
  minified-JS rewriting anywhere.
- **`build` destroying a user's directory** → two independent guards. The
  **ownership** guard: a non-empty *unowned* `<output>`, or an output symlink, is
  rejected untouched. The **path-safety** guard: `<output>` is rejected only when it
  **equals or is an ancestor of** a protected path (workspace root,
  `target/cratevista`, or a real input), because only then would replacing it delete
  something protected — so a plain descendant like `--output dist` is allowed while
  `--output .` (an ancestor of every input) is refused. Startup cleanup deletes only
  marker-owned staging/backup siblings **carrying this build's `output_key`**.
- **Over-strict path rule breaking the documented command** → the rule is stated in
  the replacement-danger direction (`output == p || output ancestor-of p`), so a
  descendant is never rejected for merely being inside the workspace; `--output dist`
  and `target/cratevista/site/` are covered by acceptance tests.
- **A killed build leaving a broken or unrecoverable site** → publish never
  overwrites in place; the three-state A→B→C marker (each transition a crash-safe
  rename-over) keeps every temporary classifiable and keyed **once marker A is
  authoritative**. Because `mkdir` and the marker-A write are separate operations, a
  crash in the mkdir → marker-temp → rename window can leave a **P0 pre-marker shell**
  (empty or `.tmp-*`-only, no authoritative marker); recovery recognizes it by a
  **strict fail-closed shape** and removes it as empty content, never parsing a
  half-written marker temp as authoritative and never deleting a keyed directory that
  holds any real content. A crash in a **staging** marker-B window leaves a candidate
  recovery **discards**; a crash in the **post-rename/pre-finalization** window
  (marker B at `<output>`, matching key) is recognized as a **completed publication**
  and **finalized to C**, preserving the new output. An adopted empty `<output>` is
  recreated on a failed swap; a failed restore or interrupted finalization preserves
  the recoverable directories and reports `build_publish_unrecoverable`.
- **`output_key` drifting when parents are created** → the key hashes the *resolved
  full identity* with the existing-ancestor/remainder split discarded, so creating
  intermediate parents does not change it; a build always finds its own temporaries
  across the crash/restart it may have caused. Tested directly.
- **One build clobbering another's temporaries under a shared parent** → temporaries
  and the lock are **keyed by `output_key`**, so a build sees only its own
  `.cratevista-<key>-*` siblings; a different key is invisible. Cross-output isolation
  is tested directly.
- **Two concurrent builds of the same output racing recovery/publish** → a
  **per-output OS advisory lock** held across recovery→publish→cleanup; the loser
  gets `build_output_busy` **before any mutation** (it inspects/deletes no candidate
  and never creates `<output>`; the only permitted residue is a safely created empty
  parent chain). The lock is released on process exit (no permanent lockout after a
  crash — deliberately not a `create_new` lock file), so a leftover unlocked `.lock`
  never blocks. `fs4` (added in Phase 2A) provides the cross-platform advisory lock.
- **The lock parent not existing yet, or the output identity shifting during parent
  creation** → a fixed **prepare → re-resolve → lock** sequence: resolve the nearest
  existing ancestor and run symlink/safety checks, create **only** the missing parent
  chain (never `<output>`), then **re-resolve** and assert the identity and
  `output_key` are unchanged **before** acquiring the lock; a changed identity/key or
  a newly revealed symlink fails closed before any output-state mutation. Parent
  creation is idempotent, so two racing processes converge on the same prepared
  parent and then contend on the same lock.
- **A caller passing a mismatched `output_key`** (recovery scanning/deleting the wrong
  key's siblings) → `OutputSafety::for_output(output, protected)` derives the key from
  the output with private fields; a mismatch against a freshly derived key is an
  internal invariant / filesystem error caught **before** the lock is opened or any
  candidate scanned — never mapped to `build_output_busy` or an ownership error.

## Alternatives considered

- **Build-time loader flag / two bundles**: rejected — breaks single-bundle
  reproducibility and the embedding gates. Runtime `<meta>` selection instead.
- **Base-path rewriting of minified JS**: rejected and forbidden — relative URLs
  already work; only a CSP-safe `<base href>` is ever written.
- **A sanitized diagnostics artifact**: rejected — the existing artifact is
  path-safe by construction; a transform would add an unverified surface.
- **Source snippets in the first release**: rejected/deferred — no safe
  deterministic secret-exclusion rule; repository links cover the need.
- **`web/dist` staying at the workspace root for publishing**: rejected — it is
  outside the server crate and would not package.
- **Single atomic `remove_dir_all` + `rename` publish**: rejected — a failed rename
  after the remove would destroy an existing site. Replaced by rename-to-backup →
  rename-staging → restore-on-failure, described as transactional (not fully atomic).
- **A directory source via `cargo vendor`**: rejected for the install test — it
  vendors from workspace *paths*, so it would not exercise the produced `.crate`
  packages. A Cargo `local-registry` of the produced `.crate`s is used instead.

## Documentation changes

Full README; `docs/hosting.md`; `docs/launch-checklist.md`; ADR-0009; the static-
site privacy statement in `SECURITY.md`; a dependency-licensing report/process
(`cargo about` + web license report). Draft announcement material is prepared, not
published.

## Rollout and migration

Prepares the first public release. Tagging `v0.1.0` runs `release.yml` (binaries +
sums, GitHub Release upload). `cargo publish` and all announcements are **manual,
gated** actions behind `docs/launch-checklist.md` and a final name recheck; **this
PRD performs none of them.**

## Traceability

Issue-10 acceptance boxes → the tests above. Consumes PRD 05 (the three artifacts +
`run_generate`), PRD 06 (embedded assets, now exported), PRD 07 (`web/dist` +
relative URLs), PRD 08 (manual-flow README example), PRD 09 (fail-closed health
probe, coherent loader, all-absent-header static rule, preserved last graph).
Finalizes the MSRV/toolchain policy from ADR-0004/ADR-0010. Snippets → follow-up
issue 13.
