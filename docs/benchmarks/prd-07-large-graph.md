# PRD-07 large-graph benchmark and node-budget decision

**Date:** 2026-07-16
**Raw data:** [`prd-07-large-graph.json`](prd-07-large-graph.json) (written by
`npm run benchmark`)

## Decision

**`DEFAULT_LARGE_GRAPH_BUDGET` stays at 1,500.** The value is **no longer
provisional**; it is retained on the evidence below.

It is a **configurable frontend engineering default** — not a schema limit, not a
server limit, and not a promise that every machine behaves identically. It is the
point beyond which this frontend prefers a focused neighbourhood to a full
render. Documents may contain any number of entities; the budget only decides
what is drawn at once.

## What was measured

The real embedded production bundle, served by the real `cargo-cratevista serve`
binary over loopback, driven by the pinned Playwright Chromium. Timings come from
the app's own in-memory instrumentation (`web/src/app/perf.ts`, User Timing) and
from the browser — nothing simulated, nothing estimated.

All cases use the `traits-and-impls` view, the widest projection the generated
documents produce. Each case is one **cold** run (fresh context, empty HTTP
cache) plus **three warm** runs (primed context, reloaded); the two are reported
separately and never averaged together.

### Environment

| | |
| --- | --- |
| OS | Windows 10.0.22631 (x64) |
| CPU | 13th Gen Intel Core i9-13900K |
| Logical cores | 32 |
| System memory | 31.7 GB |
| Node / npm | v24.13.0 / 11.17.0 |
| Playwright | 1.61.1 |
| Chromium | 149.0.7827.55 |
| Rust | cargo 1.97.0 (c980f4866 2026-06-30) |

**This is fast desktop hardware.** Every number below is a best case; a laptop on
battery will be materially slower. That asymmetry is the main reason the budget is
not raised.

### Fixtures

Deterministically generated realistic Rust workspaces (`npm run
gen:benchmark-workspaces`) — crates with real dependencies, nested public and
private modules, structs with fields, enums with variants, traits, trait **and**
inherent impls, functions and methods, cross-crate type references, and a
deliberate documented/undocumented mix. Not isolated synthetic nodes.

| Fixture | Document entities | Relations | `traits-and-impls` projects |
| --- | --- | --- | --- |
| `normal` (sample) | 101 | 166 | 64 |
| `bench-near` | 1,656 | 2,946 | 1,212 |
| `bench-at` | 2,208 | 3,929 | 1,616 |
| `bench-large` | 4,416 | 7,861 | 3,232 |

## Results

Milliseconds, `median [min–max]` across three warm runs.

| Case | Projected | Visible | Reduced | Model | Adapter | Reduce | ELK worker | First usable (warm) | First usable (cold) | Selection → inspector | Pan/zoom | JS heap |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| sample-101 | 64 | 64 | no | 0.2 | 0.0 | — | 133 [133–134] | **165** [165–170] | 233 | 16 [15–20] | 186 | 28 MB |
| near-1212 | 1,212 | 1,212 | no | 1.7 | 0.6 | — | 859 [856–871] | **1,019** [1,006–1,028] | 1,116 | 206 [194–214] | 264 | 202 MB |
| at-1616 | 1,616 | 69 | yes | 1.7 | 0.5 | 0.4 | 211 [169–218] | **336** [285–346] | 371 | 225 [174–237] | 177 | 51 MB |
| large-3232 | 3,232 | 69 | yes | 3.2 | 0.9 | 0.7 | 283 [232–304] | **505** [475–523] | 548 | 802 [800–1,081] | 174 | 28 MB |
| large-3232-full | 3,232 | 3,232 | no (forced) | 4.3 | 0.7 | 0.7 | 1,595 [1,584–1,639] | **3,662** [3,546–3,789] | 3,566 | 3,414 [3,212–3,613] | 660 | 468 MB |

Artifact fetch + validation was 8.6–31.7 ms across all cases (loopback, so this
measures parsing and hash-verified loading far more than transfer).

**Memory caveat:** `jsHeapBytes` comes from Chromium's non-standard, quantised
`performance.memory`. It is recorded as reported and is coarse — treat it as an
order of magnitude, not a precise figure. Where the API is unavailable the field
is `null`; no value is ever estimated. The `large-3232` reduced-mode heap (28 MB)
is lower than `near-1212` (202 MB) precisely because reduced mode renders 69
nodes rather than 1,212 — the document is bigger, the render is far smaller.

## Rationale

### Why full rendering below the threshold is acceptable

At **1,212 visible nodes** (below the budget, rendered in full) the graph is
usable: **~1.0 s to first usable graph** warm, **1.1 s** cold, and **206 ms**
from click to inspector. Pan/zoom stays at 264 ms for a wheel-zoom plus an
eight-step drag. Nothing here crosses into "broken", though 1 s is already past
the point where a user notices the wait.

The pure computation is negligible at every size: model/index construction is
**≤ 4.3 ms** and adapter projection **≤ 1 ms** even for the 4,416-entity
document. **The cost is layout and rendering, not our data structures** — which
is exactly why the budget governs *visible nodes*, not document size.

### Worker/layout behaviour near the threshold

ELK dominates first-usable time and grows steeply with visible nodes:
133 ms at 64 → **859 ms at 1,212** → 1,595 ms at 3,232. Between 1,212 and 3,232
the node count grows 2.7× while worker time grows 1.9× and *first usable* grows
3.6× — layout is not the only cost above ~1,200; React's commit and paint of
thousands of nodes increasingly dominate.

Interpolating to the threshold, a full render at ~1,500 visible nodes should land
near **1.0–1.3 s** first usable, with selection still in the low hundreds of ms.
That is the boundary of acceptable, which is where a threshold belongs.

### Interaction responsiveness

This is what decides the value. **Selection → inspector**:

- 64 nodes → **16 ms** (imperceptible)
- 1,212 nodes → **206 ms** (responsive)
- 3,232 nodes rendered in full → **3,414 ms** (unusable — every click stalls for
  over three seconds)

Selection latency degrades far worse than first-paint does. A user tolerates a
one-second load once; a three-second delay on *every click* makes the tool feel
broken.

In reduced mode at 3,232 projected, selection is **802 ms** — slower than the
1,212 full render, because selecting recentres the reduced neighbourhood and
therefore reprojects and relayouts. That is intended behaviour, and it is still
4× better than the full render.

### Full-render behaviour above the threshold

Forcing a full render of 3,232 nodes costs **3.7 s to first usable**, **3.4 s per
selection**, and **468 MB** of JS heap on a 32-core i9. It works — nothing hangs
or crashes — but it is not something to enter by default. This is why *Render full
graph* stays available and explicit: it is a deliberate choice with a visible
warning, not the default path.

### Why reduced mode is preferable beyond 1,500

Reduced mode makes first-usable time **independent of document size**: 336 ms at
1,616 projected and 505 ms at 3,232 projected, versus 3,662 ms for the same
document rendered fully. A 7× improvement, and it degrades gracefully as
documents grow.

The cost is honest and visible: the banner states how many of how many nodes are
shown, the neighbourhood can be expanded, the full graph is one click away, and
the complete entity list (including hidden entities) stays searchable and
keyboard-navigable. Nothing is silently dropped.

### Why not a different number?

- **Higher (e.g. 3,000):** the 3,232-node full render measures 3.4 s per
  selection and 468 MB — on the fastest hardware available here. That is not a
  default worth shipping.
- **Lower (e.g. 1,000):** the 1,212-node full render is comfortably usable
  (1.0 s / 206 ms). Lowering the budget would push graphs into reduced mode that
  do not need it, and reduced mode's value is precisely that it is *not* used
  when it is not needed.

**Honest limits of this evidence.** The exact knee was not bisected: the measured
points are 1,212 (good) and 3,232 (poor), and 1,500 sits between them without a
direct full-render measurement at that size. The `at-1616` case exercises the
boundary from above — it enters reduced mode, as intended — rather than measuring
a 1,616-node full render. All measurements come from one machine and one browser;
the ranking of the options is robust, the absolute milliseconds are not
transferable.

## Reproducing

```bash
cd web
npm run build && cargo build -p cargo-cratevista   # binary must embed this dist
npm run benchmark                                   # writes prd-07-large-graph.json
```

Set `CRATEVISTA_BENCH_LOG=/abs/path.log` for live progress (Playwright buffers
console output until a test ends).

Regenerating the fixtures needs the pinned nightly and is never part of a normal
run or CI:

```bash
npm run gen:benchmark-workspaces
npm run refresh:e2e-snapshots
```

## Changing the budget

`DEFAULT_LARGE_GRAPH_BUDGET` in `web/src/app/AppContext.tsx`. `<App budget={…} />`
overrides it, and the reduced-mode tests use that seam. Changing it affects only
what the frontend draws at once — never what `generate` produces or what the
server serves.
