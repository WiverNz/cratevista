# ADR-0009: Static site build, packaging, and public release

- **Status**: Proposed — the implementation plan for PRD 10
  (`PRD/issue_10_static_build_and_release.md`, Approved — safe to implement,
  2026-07-17). **No code implements it yet.** This ADR becomes **Accepted** only
  when PRD 10 is Implemented / Verified.
- **Date**: 2026-07-17
- **Issue**: 10 — static site build, packaging, documentation, and public release
- **Supersedes**: none
- **Related**: [ADR-0004 (rustdoc/toolchain policy)](0004-rustdoc-toolchain-policy.md),
  [ADR-0006 (server and security)](0006-server-and-security.md),
  [ADR-0008 (watch and live reload)](0008-watch-and-live-reload.md),
  [ADR-0010 (Rust version policy)](0010-rust-version-policy.md),
  `ISSUES/issue_13_static_source_snippets.md` (deferred snippets follow-up)

## Context

CrateVista must ship: a `cargo cratevista build` that emits a self-contained static
site hostable with no Rust server, a crates.io-installable package that needs no
Node.js and no workspace checkout, and a gated release process — without claiming
any capability that is not implemented and tested. The pieces this builds on are
Implemented / Verified: `run_generate` and the three artifacts (PRD 05), the
embedded server assets (PRD 06), the relative-URL frontend and reproducibility
gates (PRD 07), and the fail-closed health probe, coherent loader and all-absent
snapshot-header rule (PRD 09).

The audit behind PRD 10 found real blockers the naive plan missed: the embedded
`web/dist` and the readme/licence files live **outside** their crates (so
`cargo package` would omit them), internal dependencies carry no version
requirements (so crates.io rejects them), and a destructive remove-then-rename
publish could delete a user's directory. This ADR records the decisions that
resolve those safely.

## Decision

The locked architecture (full detail in PRD 10):

1. **Core-owned static build.** `cratevista-core::run_build(&BuildOptions, &Clock)`
   composes `run_generate → materialize_static_site`; `materialize_static_site` is
   a cargo-free seam (deterministic, testable without nightly). `cargo-cratevista`
   only adapts CLI arguments — no orchestration in the CLI crate (ADR-0001).
   `cratevista-server` gains one public API, `assets::embedded_assets()`, so the
   *same* embedded bytes `serve` sends are what `build` writes.

2. **Transactional, marker-owned publication.** `build` owns only directories it
   created, proven by a `.cratevista-static-site.json` marker written **first**
   (`kind: "staging"`, with an `output_key`) and finalized to a path-free
   `kind: "site"` marker **last** — the only two marker kinds, in a three-state
   A→B→C machine whose every transition is a crash-safe write-temp→rename-over.
   Every temporary and the advisory lock are scoped to a stable, existence-invariant
   `output_key` (`.cratevista-<output_key>-staging-<nonce>`,
   `.cratevista-<output_key>-backup-<nonce>`, `.cratevista-<output_key>.lock`), so a
   build for one output never inspects, deletes or restores another's siblings.
   Publication is *transactional with rollback* (stage → move an owned predecessor to
   a keyed backup → rename staging in → finalize → restore on failure), **not** fully
   atomic across OSes. A non-empty unowned output, an output symlink, or a path that
   equals or is an **ancestor** of a protected input is rejected, modifying nothing;
   a per-output cross-platform advisory lock (`fs4`, in `cratevista-core` only)
   serializes concurrent builds of the same output. Key-scoped recovery restores a
   sole valid backup, refuses to guess among several (`build_recovery_ambiguous`),
   finalizes a completed-but-interrupted publication, discards an unpublished staging
   candidate, and never touches unmarked or foreign-keyed directories (the P0
   pre-marker shell is recognized strictly).

3. **One runtime-selected frontend bundle.** A single committed bundle serves both
   modes. The static build injects a CSP-safe `<meta name="cratevista-mode"
   content="static">`; the app selects static mode from it, fetches relative
   `./document.json` (reusing the one coherent loader via an endpoints triple), and
   mounts **no** live-reload — a static site makes **zero requests to any `/api/**`
   route**.

4. **Committed relocated embedded assets.** The bundle is relocated to
   `crates/cratevista-server/embedded/` (Vite `outDir`, `#[folder = "embedded"]`),
   committed and reproducibility-gated. No `include` list is added: Cargo's default
   packaging already includes the tracked `embedded/**`, and `cargo package --list`
   assertions prove the server `.crate` ships the whole bundle. The release binary
   embeds those exact committed, verified bytes — the job never chooses dynamically
   between rebuilding and using the committed bundle (a fresh `check:dist` build is
   comparison-only).

5. **CLI-only build settings.** `build [--output] [--base-path] <generate flags>`;
   no `[build]` config section (the root config uses `deny_unknown_fields`).
   Relative URLs alone support root, GitHub/GitLab Pages and CI subdirectories;
   `--base-path` is an optional CSP-safe `<base href>` (no minified-JS rewriting).
   `file://` is unsupported.

6. **Provider-aware source links; snippets deferred.** Repository links are built
   with the URL API per forge (GitHub `/blob/`, GitLab `/-/blob/`, root-only for
   unsupported HTTPS, no link for ssh/git/file/credential/malformed/missing).
   Copying source *snippets* into a public site is **deferred** to
   `ISSUES/issue_13_static_source_snippets.md` — "exclude secrets" has no safe
   deterministic rule for a first release. Diagnostics ship as the existing,
   already path-safe `diagnostics.json` unchanged.

7. **Offline local-registry package verification.** Nine crates are published in
   dependency order after adding internal `version` requirements and crate-local
   `README.md` + byte-identical `LICENSE-MIT`/`LICENSE-APACHE` copies. Verification
   is `cargo package --no-verify` → assemble a Cargo `local-registry` (workspace +
   Lock-pinned third-party `.crate`s) → source-replacement in a temporary
   `CARGO_HOME` → `cargo install --locked --offline` from outside the repo. The
   compile+install is authoritative; **no publish or publish-`--dry-run`** is part
   of automated verification.

8. **Four release targets, publication manual.** A tag-triggered `release.yml`
   (trigger `v[0-9]+.[0-9]+.[0-9]+`, pinned stable 1.97.1, `contents: write` only)
   validates the tag against the workspace/package versions, runs the frontend
   reproducibility gates, and builds `x86_64-unknown-linux-gnu`,
   `aarch64-apple-darwin`, `x86_64-apple-darwin` and `x86_64-pc-windows-msvc` — one
   native runner each, no cross-compile. A **single deterministic cross-platform
   archive helper** (shared by the release job, a non-publishing CI smoke matrix, and
   a local smoke test — no per-leg shell) assembles `.tar.gz`/`.zip` archives
   (binary + `LICENSE-MIT` + `LICENSE-APACHE` + `README.md` + `CHANGELOG.md`) with one
   `.sha256` each, recomputed before upload. A **separate, manual, protected**
   `publish.yml` (`workflow_dispatch` + `release` environment + confirmation input,
   token via environment secret) runs `cargo publish` in dependency order; it is
   never tag-triggered and PRD 10 never runs it. No signing/SLSA/attestation in the
   first release (deferred; SHA-256 only). Dependency licences are reported
   deterministically for both the Rust graph (pinned `cargo-about`) and the web graph,
   with CI drift/policy checks. Only precise reproducibility is claimed (deterministic
   bundle, reproducible package file sets, deterministic `document.json`, checksummed
   archives) — **not** bit-reproducible compiled binaries.

## Consequences

- The published `cargo-cratevista` installs with no Node and no workspace, embeds
  the UI, and exposes `cargo cratevista`, verified offline on three OSes.
- A static site is safe to host publicly: no server, no `/api/**` traffic, no
  source snippets, no absolute paths, provider-correct links.
- `build` cannot destroy a directory it does not own, and a killed build is
  recoverable rather than leaving a half-written or unidentifiable site.
- Relocating the bundle into `cratevista-server` is the largest mechanical task and
  touches the dist/embed/E2E plumbing; the existing reproducibility gates catch any
  repoint error.
- Snippets and binary signing are explicitly deferred, keeping the first release's
  safety surface small.

## Alternatives considered

- **Orchestration in the CLI crate** — rejected; violates the core-owns-orchestration
  rule (ADR-0001).
- **Two bundles / a build-time loader flag** — rejected; breaks single-bundle
  reproducibility. Runtime `<meta>` selection instead.
- **Single atomic `remove_dir_all` + `rename`** — rejected; a failed rename after
  the remove would destroy an existing site. Transactional rollback instead.
- **`cargo vendor` directory source for the install test** — rejected; it exercises
  workspace paths, not the produced packages. A `local-registry` of the produced
  `.crate`s instead.
- **Source snippets in the first release** — rejected/deferred; no safe deterministic
  secret-exclusion rule. Repository links cover the need.

This ADR is **Proposed**; it becomes **Accepted** when PRD 10 lands and is verified.
