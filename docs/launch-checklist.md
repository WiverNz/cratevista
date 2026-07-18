# Launch checklist

The gated, mostly-manual steps that turn a verified CrateVista tree into a published
release. This checklist is organized by **who acts** and **how reversible** each step
is. Nothing here is performed automatically by PRD 10 — the release and publish
workflows are delivered but never triggered by that work.

Legend:

- 🤖 **Automated evidence** — a CI job or test proves it; a human only reads the
  result.
- 👀 **Maintainer review** — a human inspects and decides.
- 🚦 **Manual gated action** — a human deliberately triggers an irreversible or
  outward-facing step.
- ⛔ **Never performed by the implementation task** — reserved for the maintainer at
  launch.

---

## 1. Pre-release verification (mostly 🤖, reviewed 👀)

All of these must be green **on the same final commit** before anything is published:

- [ ] 👀 Clean working tree at the commit to be released (maintainer's choice of
      commit).
- [ ] 🤖 All stable CI jobs green (lint, test matrix, explorer/browser).
- [ ] 🤖 **Phase-5B package-install matrix green on Linux, macOS and Windows**
      (`package-install` workflow / job) — offline local-registry install, no Node.
- [ ] 🤖 **Phase-7 nightly full-pipeline test green** (`build_live`, run on
      `nightly-2026-07-01`).
- [ ] 🤖 **Release-artifact smoke matrix green** (the non-publishing archive/checksum
      smoke on all four runners).
- [ ] 🤖 **README first-run verification green** (stable install/CLI path + the
      pinned-nightly generation path).
- [ ] 🤖 Dependency-licence policy green — `cargo about generate --frozen` and
      `npm run license:web:check`, with reports in `docs/licenses/` current.
- [ ] 🤖 Package file-set checks green (`cargo package --list` assertions; server
      ships `embedded/**`; `cargo-cratevista` ships README + both licences).
- [ ] 🤖 Frontend bundle drift checks green (`check:dist`, `check:embed-rebuild`).
- [ ] 🤖 Produced-site privacy scan green (no absolute paths / usernames / argv /
      credentials).
- [ ] 👀 Version consistency: the tag version equals `[workspace.package] version`
      and every workspace package version.
- [ ] 👀 Changelog ready (see §5).
- [ ] 👀 ADR/PRD final bookkeeping prepared (see §6) — **staged, not yet applied**.

## 2. Manual crates.io gates (🚦)

Performed by the maintainer immediately before running `publish.yml`:

- [ ] 🚦 **Final crate-name availability recheck** on crates.io for all nine crate
      names. This is a **point-in-time** check at the moment of publication — never a
      permanent claim recorded in the repository.
- [ ] 🚦 **Real `cargo publish --dry-run` in dependency order** against crates.io
      (schema → metadata → rustdoc → graph → config → server → watch → core →
      cargo-cratevista). This is a manual launch gate; it is intentionally **not** an
      automated CI step.
- [ ] 👀 Verify each crate's rendered metadata, README and licences.
- [ ] 👀 Verify the crates.io token is configured as the `CARGO_REGISTRY_TOKEN`
      **environment secret** of the protected `release` environment, and that the
      environment's required reviewers are set.
- [ ] 👀 Account for **registry index propagation** between dependent crates: each
      crate must be live in the sparse index before the next resolves it. `publish.yml`
      publishes sequentially and stops on the first failure; if a dependent publish
      races the index, re-run from the failed crate (earlier crates are already live).
- [ ] 🚦 Explicit approval before starting `publish.yml` (the `workflow_dispatch`
      confirmation input plus the protected-environment approval).

## 3. Manual GitHub release gates (🚦)

- [ ] 🚦 Create and push the intended tag `vX.Y.Z` — **only after** §1 is green. This
      triggers `release.yml`.
- [ ] 👀 Verify all **four** archive names:
      - `cargo-cratevista-<version>-x86_64-unknown-linux-gnu.tar.gz`
      - `cargo-cratevista-<version>-aarch64-apple-darwin.tar.gz`
      - `cargo-cratevista-<version>-x86_64-apple-darwin.tar.gz`
      - `cargo-cratevista-<version>-x86_64-pc-windows-msvc.zip`
- [ ] 👀 Verify all four `.sha256` files and that each matches its archive.
- [ ] 👀 Inspect archive contents (binary + `LICENSE-MIT` + `LICENSE-APACHE` +
      `README.md` + `CHANGELOG.md`, and nothing else).
- [ ] 👀 Verify the release notes.
- [ ] 🚦 Install the binary from an archive on at least one target and run
      `cargo cratevista --help`.

## 4. Final public gates (🚦 / ⛔)

- [ ] 🚦 Verify crates.io installation: `cargo install cargo-cratevista` from a clean
      environment.
- [ ] 🚦 Verify README first-run from the **published** artifacts.
- [ ] 👀 Attach or retain the dependency-licence reports (`docs/licenses/`) as release
      assets or references.
- [ ] 👀 Final GitHub *About* text and repository **topics**.
- [ ] 👀 Announcement review (drafts in `docs/launch/announcement-drafts.md`).
- [ ] ⛔ **Announcement publication** — only after explicit maintainer action. Never
      performed by the implementation task.

## 5. CHANGELOG transformation

The changelog keeps the completed 0.1.0 work under **`[Unreleased]`** until launch.
At the moment of release, the maintainer transforms the heading:

```text
## [Unreleased]        →        ## [0.1.0] - YYYY-MM-DD
```

with the real release date, and opens a fresh empty `[Unreleased]` section above it.
Do **not** date `0.1.0` before it is actually published.

## 6. Final bookkeeping-only change (⛔ until hosted closure)

After **all** hosted legs in §1 pass on the same final commit, the maintainer applies
one bookkeeping-only change (no code):

- Implementation-sequence **Step 5 complete**; Phase 5B **LANDED / VERIFIED**.
- Implementation-sequence **Step 7 complete**.
- PRD 10 status → **Implemented / Verified**.
- ADR-0009 status → **Accepted**.
- `PRD/INDEX.md` PRD-10 row → **Implemented / Verified**.
- The hosted-dependent acceptance boxes checked.

This transition is **not** made until the real Linux/macOS/Windows evidence exists.
