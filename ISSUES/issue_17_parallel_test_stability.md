# Issue 17 — Frontend suite parallel-execution stability

**Type:** test-infrastructure hardening
**Raised:** 2026-07-23
**Status:** draft — placeholder, not approved, not scheduled

Draft placeholder isolating a **pre-existing** test-infrastructure characteristic
surfaced (and partially mitigated) during Issue 15 Phase 3. It is not approved and
not to be implemented here.

## Provenance

Under the canonical command `npm run test` (Vitest, jsdom, default file-parallel
runner) the frontend suite is **deterministically green when run single-threaded**
(`vitest run --no-file-parallelism` → 606/606, repeatedly) but, under the default
parallel runner, **intermittently** fails a single already-async test per run
(roughly 1 run in 2–3), on a *different* test each time. Observed offenders (all
pre-existing, none introduced by Issue 15):

- `tests/history.test.tsx` — popstate restore / pushState / replaceState assertions
  (several already carry a 4000 ms contention allowance);
- `tests/app.test.tsx` — "restores view on popstate";
- `tests/dim-focus.test.tsx` — "entering dim never rewrites the edges mode",
  "Back/Forward restores dim after clearing";
- `tests/controls.test.tsx`, `tests/fixtures.test.tsx` — control/tab assertions.

## Mechanism (diagnosis)

These are **not** logic races and **not** shared-state leaks — single-threaded
execution proves every assertion is correct. The failures are **CPU starvation**
of oversubscribed parallel workers: an already-async assertion (`findBy*` /
`waitFor`, default 1000 ms; or a store-subscription side effect) occasionally does
not settle within its window while other workers saturate the CPU. The codebase
already acknowledges this with explicit 4000 ms "parallel-suite CPU contention"
timeouts in `history.test.tsx`.

Issue 15 Phases 1–3 fixed the **demonstrated, in-scope** causes (synchronous
assertions on asynchronous React-Flow / URL / layout updates were converted to
`waitFor`/`findBy`, matching the existing `controls.test` precedent) and one real
breakage (a `minHeight`→`height` card assertion). That materially lowered the flake
rate but did not eliminate the residual CPU-starvation class.

The dominant remaining pattern is a synchronous query that runs *before* the thing
it needs has committed: many tests do `await ready()` (which resolves once the
Views `tablist` exists) and then, on the very next line, `getByTestId(\`node-…\`)`
or read a control value synchronously. The graph nodes can commit a frame after the
tablist, so under worker starvation that synchronous query occasionally throws
"not found" or reads a pre-commit value. The robust fix is `findByTestId` /
`waitFor` for that first post-`ready()` query — applied uniformly across the suite,
which is the broad sweep deferred here.

## Why deferred (out of Phase-3 scope)

Eliminating the residual requires **broad, suite-wide** work that is not
node-card redesign: either (a) systematically applying the demonstrated-need
4000 ms contention allowance across the remaining already-async assertions, and/or
(b) a runner-level change (a worker cap matched to physical cores, or a
per-file-isolation/pool adjustment). The Phase-3 brief explicitly forbade blind
retries, arbitrary sleeps, global serialization "merely to hide races", and broad
timeout increases — so the correct action was to fix the demonstrated causes and
isolate the residual here rather than claim the parallel gate is deterministically
green.

## Proposed scope (when selected)

- Audit every remaining already-async assertion that flakes under contention and
  apply the 4000 ms allowance only where a need is demonstrated.
- Evaluate a Vitest `maxWorkers` / pool configuration matched to the host, measured
  against wall-clock cost, WITHOUT hiding any genuine race.
- Add a small "stability" CI job that runs the suite N consecutive times.

Acceptance would be: ≥5 consecutive clean canonical runs on the reference host,
with no assertion weakened and single-threaded execution still green.

## Status

Draft placeholder. Do not approve. Do not implement.
