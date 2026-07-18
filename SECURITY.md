# Security Policy

## Reporting a vulnerability

Please report security issues **privately** rather than opening a public issue. Use
GitHub's **private vulnerability reporting** for this repository
(the repository's *Security → Report a vulnerability* advisory channel). We will
acknowledge your report and work on a fix. If private reporting is unavailable to
you, contact the maintainer through the repository's listed contact channels.

Please do not disclose the issue publicly until a fix is available.

## Security & privacy model

CrateVista is designed to be local-first and source-private.

### Local by default

- The default workflow is fully local. CrateVista does not upload your source code or
  generated project data, and performs no remote/hosted analysis.
- `cargo cratevista doctor` and `cargo cratevista init` are read-only with respect to
  your environment: they never install toolchains or modify machine configuration.
  `init` only writes a `cratevista.toml` in the current directory, and never
  overwrites an existing one without `--force`.
- CrateVista never executes your project's binaries or tests as part of
  visualization.

### The live server (`serve` / `open`)

- The server binds to `127.0.0.1` by default. A non-loopback `--host` exposes it on
  your network and prints a warning.
- It does **not** expose arbitrary local files over HTTP. Serving source-file
  **contents** is opt-in (`--source`), and even then every requested path is
  validated server-side against the workspace; a path that escapes is rejected.
- Responses carry a strict Content-Security-Policy (no `unsafe-eval`, no remote
  origins), `nosniff`, `DENY` framing and same-origin referrer policy, and no
  permissive CORS.

### The static site (`build`)

A produced static site is safe to host publicly:

- **Contents.** Exactly `index.html`, the fingerprinted app assets, and the three
  JSON artifacts (`document.json`, `generation.json`, `diagnostics.json`). Nothing
  else — no `source/` or `snippets/` directory, no server.
- **Repository links, not snippets.** When the analyzed workspace declares a
  `repository` and the metadata is safe, the explorer renders a link back to it
  (repository root always; per-file deep links only when an authoritative
  `default_branch` is present). It **never** copies source-file contents into the
  site.
- **Repository-relative paths only.** Every `SourceLocation` is a validated
  repository-relative path (`RepoRelativePath` rejects absolute, drive-qualified, UNC
  and traversing spellings). Absolute paths do not enter the artifacts.
- **Tested for leakage.** A privacy scan asserts the produced `index.html` and the
  three JSON artifacts contain no absolute filesystem path, no username, no
  Cargo/rustdoc argv fragments, no `CARGO_HOME`/`RUSTUP_HOME` path, and no
  credential-bearing URL or common secret pattern. The scan is proven to *detect* an
  intentionally injected absolute path or credential before that control is reverted.
- **No server chatter.** In static mode the site opens **no** `EventSource` and makes
  **zero** requests to any `/api/**` route — proven by a real-browser test against a
  produced site. There is no `/api/health`, `/api/events` or `/api/source` in a
  static export.

### Output safety (`build`)

`build` owns only directories it created. It writes an ownership marker first and
finalizes it last, publishes transactionally with rollback, and refuses to modify a
non-empty directory it does not own, an output symlink, or a path that equals or is
an ancestor of a protected input (the workspace root, the artifact directory, or any
generation input). A killed build is recoverable rather than leaving a half-written
or unidentifiable site.

### What is and is not sanitized

CrateVista removes environment variables, secrets, build outputs and ignored files
from generated documents, and keeps argv/paths out of artifacts. It does **not**
attempt to scrub secrets from **content you author yourself** — a `repository` URL,
a manual flow label, or documentation text you write is reproduced as given. Do not
put credentials in a `repository` URL or in manual configuration; a credential-bearing
`repository` URL produces **no** link, but CrateVista does not otherwise inspect
author-supplied text for secrets. Copying source *snippets* into a static site is
deliberately out of scope for the current release (see
`ISSUES/issue_13_static_source_snippets.md`), precisely because a safe, deterministic
secret-exclusion rule does not yet exist.

### Paths from user input

Paths derived from user input are validated, and non-UTF-8 paths are rejected with a
clear diagnostic.
