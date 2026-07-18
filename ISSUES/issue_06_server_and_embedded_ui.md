# Issue 06 — Serve the document with an embedded web application

## Goal

Provide a local HTTP server that serves the generated CrateVista document and a prebuilt frontend embedded in the installed Cargo subcommand.

End users must not need Node.js.

## Commands

```bash
cargo cratevista serve
cargo cratevista open
```

Expected defaults:

- bind to `127.0.0.1`;
- choose a documented default port;
- fail clearly or select another port according to an explicit policy;
- `open` launches the default browser after the server is ready.

## Required server responsibilities

- Serve the embedded SPA assets.
- Serve the explorer document through a stable endpoint.
- Serve diagnostics.
- Provide health/readiness information.
- Support SPA fallback routes.
- Set correct content types.
- Avoid directory traversal.
- Avoid exposing arbitrary repository files.
- Support graceful shutdown.
- Log the local URL.
- Make browser opening optional and testable.

## Build responsibilities

- Frontend assets are built during CrateVista development/release.
- Release artifacts embed those assets.
- Rust-only contributor workflows should have a documented fallback when rebuilding the frontend is not necessary.
- Packaging must not depend on absolute build-machine paths.

## Acceptance criteria

- [ ] A locally installed binary serves the UI with no Node.js installation.
- [ ] `/api/document` returns a valid explorer document.
- [ ] Static assets have correct content types.
- [ ] Unknown SPA routes return the application shell where appropriate.
- [ ] Source-file endpoints cannot read paths outside the configured workspace.
- [ ] The default server is not publicly reachable from another machine.
- [ ] Server tests cover readiness, missing document, malformed document, and path traversal attempts.
- [ ] `open` waits for readiness before opening a browser.
- [ ] Ctrl-C performs a clean shutdown.

## PRD requirement

Do not implement this issue directly.

First create:

```text
PRD/issue_06_server_and_embedded_ui.md
```

The PRD must map every acceptance criterion to concrete modules, tests, and verification commands.
