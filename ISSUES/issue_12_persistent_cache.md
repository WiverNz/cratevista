# Issue 12 — Add a persistent generation cache

## Status

**Specification shell.** Split out of issue 09 on 2026-07-16 by an approved
maintainer decision (PRD-09 blocker B4): watch mode ships without persistent
caching, and caching is specified separately here rather than rushed alongside it.

This is a shell, not a plan. It records why the split happened and what any PRD
must answer. Follow the normal workflow — do not implement from this file.

## Goal

Make repeated generations fast by reusing work whose inputs have not changed,
without ever showing a stale architecture map.

## Why this is not part of issue 09

Watch-mode correctness does not depend on a cache: debounce plus single-flight
already remove redundant runs, and cargo caches builds underneath. Caching is a
performance feature with an independent, and much harsher, correctness budget.

Three concrete gaps made a safe specification impossible inside issue 09:

1. **The key's main input does not exist.** `cratevista_rustdoc::cache_key(...)`
   takes a caller-supplied `input_digest`, and **nothing computes one**. There is
   no per-target source-file enumeration (`RustdocPlan` carries `package_root`,
   not file lists), and `GenerationReport.input_hashes` is written as an **empty
   `BTreeMap`**.
2. **A digest needs its own ignore policy**, which could disagree with the
   watcher's answer to "which files are inputs" — two sources of truth for one
   question is how stale-cache bugs are born.
3. **A stale cache is this product's worst failure.** The output is an
   architecture map people trust; a wrong-but-fast map is worse than a slow
   correct one, and cache bugs are the class that survives review.

## Existing constraints (verified 2026-07-16)

- **`cratevista_rustdoc::cache_key(&RustdocTarget, &RustdocOptions,
  &CompatibilityTuple, input_digest: &str) -> String` already exists**, is
  tested, and is domain-framed (`cratevista-rustdoc-cache:v1:`, BLAKE3, truncated
  to 32 hex chars). It already covers target identity, features,
  `include_private`, nightly, rustdoc format version, `rustdoc_types`, and adapter
  version.
- **Do not invent a second rustdoc cache-key format.** Any PRD reuses this
  function and supplies `input_digest`. A second key format means two answers to
  "is this entry valid".
- **No cache store exists anywhere.** `--no-cache` does not exist and was
  deliberately **not** added by PRD 09: a flag that disables a nonexistent cache
  is a lie in `--help` that must then be supported forever.
- `GenerationReport.input_hashes` exists in the schema and is currently empty —
  it is the natural home for the digest, and populating it is an artifact-visible
  change to weigh.

## Required behavior

Cache, with safe boundaries defined per stage:

- cargo metadata
- raw rustdoc JSON (per target)
- normalized rustdoc data (per crate)
- the final explorer document
- frontend layout state, if any

Invalidation must favor correctness over cleverness: if a keyed input changes, the
stage and **everything downstream** is invalid.

## Questions a PRD must answer

- **Ownership**: a new crate, or `cratevista-core`? Which crate may touch the
  cache directory?
- **Directory**: `target/cratevista/cache/`? It must be inside `target/` so the
  PRD-09 watcher already ignores it, and safe to delete at any time.
- **`input_digest`**: which files, discovered how, hashed how? Content hash, or
  mtime+size with hash confirmation? How does it stay consistent with PRD 09's
  watched input set — ideally one shared enumeration, not two?
- **Key granularity** per stage, and what each key must include (at minimum:
  CrateVista version, `SchemaVersion`, config file hashes, everything `cache_key`
  already covers).
- **Corruption recovery**: a truncated or hand-edited entry must be treated as a
  miss, never a failure — the same "never publish a bad candidate" rule the
  snapshot loader already follows.
- **Size and cleanup**: bound, eviction policy, and what stops raw rustdoc JSON
  from growing without limit.
- **Concurrency**: two `open --watch` processes on one workspace; two `generate`
  runs; a cache write racing a read.
- **`--no-cache`**: exact semantics (bypass reads *and* writes), and whether it is
  global or per-command.
- **Proof of correctness**: how does a test show a cache hit produces **byte-
  identical** artifacts to a cold run? That equivalence is the feature's real
  acceptance criterion.

## Acceptance criteria (inherited from issue 09)

- [ ] Cache keys include all inputs that affect output.
- [ ] `--no-cache` behavior is defined and tested.
- [ ] A cache hit produces byte-identical artifacts to a cold run.
- [ ] A corrupted cache entry is a miss, never a failure.
- [ ] Deleting the cache directory at any time is safe.

## PRD requirement

Do not implement this issue directly. First create:

```text
PRD/issue_12_persistent_cache.md
```

The PRD must answer every question above and map each acceptance criterion to
concrete modules, tests, and verification commands.

## Related

- `ISSUES/issue_09_watch_and_live_reload.md` — the source of this split.
- `PRD/issue_09_watch_and_live_reload.md` — "## Caching: deferred, with reasons".
- `docs/adr/0008-watch-and-live-reload.md` — records the deferral.
- `crates/cratevista-rustdoc/src/cache.rs` — the existing key. Its doc comment
  points here for `input_digest`.
