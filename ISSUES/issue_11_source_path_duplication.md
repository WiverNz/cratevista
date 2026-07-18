# Issue 11 — rustdoc span source paths are duplicated

**Type:** defect
**Found:** 2026-07-16, during PRD-07 browser verification
**Status:** backlog (needs a PRD before implementation)
**Not a PRD-07 blocker** — see "Impact".

## Summary

Every `SourceLocation` derived from a **rustdoc span** carries a repo-relative
path whose package prefix appears **twice**. Paths derived from **cargo
metadata** are correct, which localises the defect precisely.

Observed (in `web/e2e/fixtures/normal/document.json`, generated from
`web/fixtures/sample-workspace`):

```text
crates/cvapp/crates/cvapp/src/lib.rs      <- wrong (rustdoc span)
crates/cvcore/crates/cvcore/src/lib.rs    <- wrong (rustdoc span)
crates/cvcore/crates/cvcore/src/model.rs  <- wrong (rustdoc span)

crates/cvapp/src/lib.rs                   <- correct (cargo metadata target)
crates/cvapp/Cargo.toml                   <- correct (cargo metadata)
Cargo.toml                                <- correct (cargo metadata)
```

Expected repo-relative form:

```text
crates/cvapp/src/lib.rs
```

Note that the same file is currently represented **both** correctly (via
metadata) and incorrectly (via rustdoc) in one document.

## Affected entities

All 27 source-bearing entities in the sample-workspace fixture are affected
wherever their location comes from a rustdoc span — impls, methods and
module/type items. Examples:

- `impl:cvapp:inherent:Service:b6af7b10e4834f23bd7ef7be08746792` → `crates/cvapp/crates/cvapp/src/lib.rs`
- `impl:cvapp:inherent:Service:…::describe` (method) → same duplicated path
- `impl:cvcore:Render:Widget:e2ef231a246948181ff40bf10ee1434a::render` → `crates/cvcore/crates/cvcore/src/lib.rs`

## Likely ownership and root cause

`crates/cratevista-rustdoc/src/spans.rs::map_span`.

rustdoc's `span.filename` may be absolute or relative. When it is **relative**,
`map_span` joins it onto `package_root`:

```rust
let full = if is_absolute_str(&normalized) {
    normalized
} else {
    let package = normalize_sep(&context.package_root.to_string_lossy());
    format!("{}/{}", package.trim_end_matches('/'), normalized)   // <-- here
};
```

But rustdoc emits relative filenames relative to the **cargo invocation's
working directory**, which for our generation is the **workspace root** — not
the package root. So for `cvapp`:

```text
package_root = <ws>/crates/cvapp
filename     = crates/cvapp/src/lib.rs        (already workspace-relative)
full         = <ws>/crates/cvapp/crates/cvapp/src/lib.rs
strip(<ws>)  = crates/cvapp/crates/cvapp/src/lib.rs
```

which reproduces the observed value exactly.

`cratevista-metadata::source::map_source` is **not** affected: it strips the
workspace root from an absolute path and never joins a package prefix.

A fix must not simply swap `package_root` for `workspace_root` without evidence:
the correct base depends on the cwd rustdoc was invoked with, so the PRD should
establish that contract explicitly (and cover both absolute and relative
filenames, plus the nested-package case where the two roots differ).

## Impact

**Low, and not a PRD-07 blocker.** The explorer operates normally:

- `/api/source` is **disabled by default**, so the path is displayed but never
  resolved;
- with `--source` enabled a duplicated path simply fails the server's guarded
  path validation, producing a stable, non-fatal `source_path_invalid` /
  `source_not_file` outcome that the UI already degrades on gracefully;
- no absolute path escapes, and no traversal is possible — the value is still a
  validated `RepoRelativePath`, merely a wrong one.

The user-visible symptom is a wrong path label in the inspector, and source
contents that cannot be shown for rustdoc-derived items when source is enabled.

## Out of scope / constraints

- **Do not add a frontend workaround.** The frontend must not "repair" paths; it
  displays the repo-relative path the document gives it.
- Do not weaken `RepoRelativePath` validation.
- The committed E2E fixtures encode the current (wrong) paths. Fixing this
  requires a gated fixture refresh (`npm run refresh:e2e-snapshots`), which
  recomputes `artifact_hashes`; the PRD must account for that.
- Add regression tests for a relative rustdoc filename, an absolute one, and a
  package whose root differs from the workspace root.
