# E2E snapshots

Two **complete, real** CrateVista artifact snapshots, served by the actual
`cargo cratevista serve` binary during the Playwright suite. Each is a full
three-artifact snapshot — never a lone `document.json`, because the server
requires `generation.json` and `diagnostics.json` too.

| Snapshot  | Source workspace                | `partial` | Entities | Views |
| --------- | ------------------------------- | --------- | -------- | ----- |
| `normal`  | `web/fixtures/sample-workspace`  | `false`   | 101      | 8     |
| `partial` | `web/fixtures/partial-workspace` | `true`    | 73       | 8     |

Both were produced by real generation runs against real Rust workspaces using
the pinned nightly (`nightly-2026-07-01`, rustdoc format 60) — not hand-written.

## Integrity: do not hand-edit

`generation.json` embeds BLAKE3 digests over the **exact stored bytes** of
`document.json` and `diagnostics.json`:

```json
"artifact_hashes": {
  "document_blake3": "<64 lowercase hex>",
  "diagnostics_blake3": "<64 lowercase hex>"
}
```

Any edit to any artifact — even whitespace — invalidates the digests and the
server refuses to load the snapshot. The three files of a snapshot must always
be copied together, as a unit.

## The `partial` snapshot is genuinely partial

It is **not** a copy of `normal` with the flag flipped. `partial-workspace`
contains `cvbroken`, a crate that deliberately fails to compile. Generating with
`--keep-going` documents the healthy crate, records a real `target_failed`
diagnostic, and marks the result `partial: true`. That drives the partial banner
under exactly the conditions a user would hit.

## Refreshing (gated)

Running the E2E suite needs **no nightly toolchain** — the snapshots are
committed. Regenerating them does:

```bash
npm run refresh:e2e-snapshots
cargo test -p cratevista-server --test e2e_fixtures   # verify integrity
```

`crates/cratevista-server/tests/e2e_fixtures.rs` loads both snapshots through
the real server loader on every `cargo test`, so drift is caught there rather
than as a confusing browser-startup failure.

## Generated and deterministically path-normalized

The `target_failed` diagnostic quotes the rustdoc command verbatim, and cargo
resolves that command to an **absolute** manifest path — the filesystem layout
of whoever refreshed the fixture. That must not be committed.

The refresh pipeline therefore normalizes before committing:

1. generate the genuine failing workspace snapshot with `--keep-going`;
2. keep the real `partial: true` result and the real `target_failed` diagnostic;
3. rewrite only the known absolute fixture-workspace prefix to the stable token
   `<fixture-workspace>`;
4. re-commit through the **production writer**
   (`cratevista_core::artifacts::commit_artifacts`), which recomputes
   `artifact_hashes` over the exact normalized bytes it writes;
5. validate the result through the real snapshot loader.

So the digests are correct *by construction*, not by hand. This is why the
fixtures must never be hand-edited: editing bytes without re-hashing breaks
integrity, and hand-writing digests defeats the check they exist for. The
normalization is a fixture-pipeline concern only — the production diagnostics
pipeline is unchanged.

`crates/cratevista-server/tests/e2e_fixtures.rs` enforces all of it: `partial`
stays true, `target_failed` survives, both digests validate through the loader,
and no Windows drive path or Unix home path is present in any artifact.
