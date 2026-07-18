# Issue 13 — Static source snippets (specification shell)

> **This is a specification shell, not a PRD and not an approved work item.** It
> records the minimum constraints any future "copy source snippets into a static
> site" feature must satisfy. It defines no implementation, adds no CLI flag, changes
> no current output, and is **not** listed in `PRD/INDEX.md` as an approved PRD. It
> exists so the deferral from PRD 10 (Decision 6) is captured with its safety
> requirements rather than lost.

## Context

PRD 10 ships repository **links** but deliberately writes **no source snippets** into
a produced static site. **The current release intentionally writes no snippets** —
the explorer shows repository-relative *locations* and links back to the forge, and a
static site contains only `index.html`, the app assets, and the three JSON artifacts.
Copying file *contents* into a public site was deferred because there is no safe,
deterministic secret-exclusion rule for a first release.

A future feature that copies snippets into a static export must be specified as its
own PRD and satisfy at least the constraints below before it can be approved.

## Required constraints for any future implementation

- **Explicit allowlist policy.** Snippets are opt-in per file/path via an explicit
  allowlist; nothing is copied by default. No "copy everything except a denylist".
- **Explicit directory/manifest format.** A committed, reviewable manifest declares
  exactly which files (and which line ranges, if any) may be included.
- **`RepoRelativePath` → output-URL mapping.** Every included path is a validated
  repository-relative path mapped to a deterministic output URL; no absolute, UNC,
  drive-qualified or traversing path is ever emitted.
- **Per-file byte cap** and a **total byte cap** on emitted snippet content, with a
  clear diagnostic when either is exceeded.
- **UTF-8 vs binary handling.** Non-UTF-8/binary files are rejected (not silently
  mangled); a diagnostic names the offending path.
- **Symlink / reparse-point rejection.** A symlink or platform reparse-point in the
  path is rejected and never traversed.
- **Duplicate collapse.** The same file referenced multiple times is emitted once.
- **Disappear/change-during-build handling.** A file that disappears or changes
  between selection and emission fails closed for that file rather than emitting torn
  or stale content.
- **Hard deny-list, even when referenced.** `.env`, `.env.*`, credential files,
  private keys (`*.pem`, `id_rsa`, `*.key`), `.npmrc`/`.netrc`, and common secret
  files are **never** emitted even if explicitly allowlisted — the deny-list wins.
- **Privacy review.** The emitted snippet set is covered by the same produced-site
  privacy scan (no absolute paths, usernames, argv, `CARGO_HOME`/`RUSTUP_HOME`,
  credentials), extended to snippet content.
- **Deterministic output.** Identical inputs produce byte-identical snippet output.
- **Collision handling.** Output URL collisions are detected and resolved
  deterministically, never by last-writer-wins overwrite.
- **CSP and MIME considerations.** Snippet resources are served/rendered under the
  same strict CSP; MIME/type handling must not enable script execution from snippet
  content.
- **No reliance on automatic secret detection.** Automatic secret scanning is treated
  as insufficient on its own; the allowlist + deny-list + caps are the primary
  controls, not a heuristic scanner.

## Non-goals for this shell

This document does not choose a manifest syntax, a CLI surface, or an output layout,
and does not authorize any implementation. Those belong to a future PRD.
