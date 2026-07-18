# Dependency licences

CrateVista ships under `MIT OR Apache-2.0`. This directory holds the reviewable,
reproducible dependency-licence reports for both the Rust and the web dependency
graphs, plus the accepted-licence policy. The reports are **generated** — do not
edit them by hand; regenerate and commit, or CI's drift check fails.

## Rust dependencies

- **Tool:** [`cargo-about`](https://github.com/EmbarkStudios/cargo-about), pinned to
  the exact version in `.github/workflows/ci.yml` (`CARGO_ABOUT_VERSION`).
- **Policy/config:** [`about.toml`](../../about.toml) (accepted SPDX allowlist; the
  four release targets; dev-dependencies excluded, build-dependencies included).
- **Template:** [`about.hbs`](../../about.hbs) — deterministic, no timestamps or
  local paths.
- **Report:** [`rust-dependencies.md`](rust-dependencies.md).
- **Regenerate:**

  ```bash
  cargo about generate --frozen --workspace about.hbs -o docs/licenses/rust-dependencies.md
  ```

  `--frozen` (offline + locked) keeps the output reproducible: no network, no
  clearlydefined.io lookups, no host-dependent variation. cargo-about exits non-zero
  if any crate resolves to a licence outside `about.toml`'s `accepted` list.

## Web dependencies

- **Authority:** the committed `web/package-lock.json` plus each installed package's
  `package.json` licence field. No large runtime dependency is added — a small
  repository-owned Node script does the work.
- **Generator:** [`web/scripts/license-report.mjs`](../../web/scripts/license-report.mjs).
- **Reports:** [`web-dependencies.md`](web-dependencies.md) and
  [`web-dependencies.json`](web-dependencies.json).
- **Regenerate / check:**

  ```bash
  cd web
  npm run license:web        # regenerate the reports + policy check
  npm run license:web:check  # fail on drift or a policy violation (CI)
  ```

  Licences are matched by SPDX id — never inferred from a package name — and an
  unparseable compound SPDX expression is a hard failure, never silently accepted.

## Accepted-licence policy

Permissive licences are accepted directly: MIT, MIT-0, Apache-2.0, BSD-2-Clause,
BSD-3-Clause, ISC, 0BSD, CC0-1.0, Zlib, Unlicense, Python-2.0, BlueOak-1.0.0,
Unicode-DFS-2016, Unicode-3.0, WTFPL.

### Recorded exceptions

These are not permissive but are accepted with the rationale below. Each is reviewed
when the dependency set changes:

| SPDX | where | scope | rationale |
| --- | --- | --- | --- |
| `EPL-2.0` | `elkjs` | runtime | Weak, file-level copyleft. `elkjs` (the ELK graph-layout library) is used **unmodified** as a dependency; the EPL's file-level reciprocity does not extend to CrateVista's own source. Chosen in PRD 07 for the layout engine. |
| `MPL-2.0` | `lightningcss`, `axe-core` (+ platform packages) | dev/build | Weak, file-level copyleft; **build/dev tooling only**, never shipped in the binary or the static site. |
| `CC-BY-4.0` | `caniuse-lite` | dev | A **data** licence (browser-support data), dev-only; no code is distributed under it. |

No dependency uses a strong-copyleft (GPL/AGPL/LGPL) licence. If one ever appears,
the policy fails closed and the addition must be reviewed and either removed or
explicitly justified here.
