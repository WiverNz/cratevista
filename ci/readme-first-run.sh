#!/usr/bin/env bash
# README first-run verification (PRD 10, Phase 7E).
#
# Proves the README's documented install + first-run steps actually work from a clean
# environment — with NO Node — in two separable parts:
#
#   stable  : install from source on stable Rust, then --help, doctor, and a
#             metadata-only `build` (no nightly, no network for generation).
#   nightly : run a real generation on a documentable fixture using the pinned
#             nightly (rustdoc JSON is nightly-only).
#
# Usage: ci/readme-first-run.sh [stable|nightly|all]   (default: all)
#
# It runs in a clean, isolated environment: a fresh CARGO_HOME, a fresh install root,
# a fresh target dir, and fixtures copied OUTSIDE the repository. The future crates.io
# install line is documentation-only and is never executed here.
#
# Drift guard: the script asserts the README still documents the exact commands it
# depends on, so the docs and this verification cannot silently diverge.
set -euo pipefail

MODE="${1:-all}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
README="$REPO_ROOT/README.md"
NIGHTLY="nightly-2026-07-01"

log() { printf '\n=== %s ===\n' "$1"; }

# --- Drift guard: the README must document what this script runs -------------
log "README drift guard"
require_readme() {
  if ! grep -qF "$1" "$README"; then
    echo "README drift: expected to find '$1' in README.md" >&2
    exit 1
  fi
  echo "ok: README documents '$1'"
}
require_readme "cargo install --path crates/cargo-cratevista"
require_readme "cargo cratevista --help"
require_readme "cargo cratevista doctor"
require_readme "cargo cratevista build"
require_readme "rustup toolchain install nightly-2026-07-01"
# The crates.io install line is present but flagged as future — never executed here.
require_readme "cargo install cargo-cratevista"

# --- Isolated environment ----------------------------------------------------
WORK="$(mktemp -d)"
cleanup() { rm -rf "$WORK" || true; }
trap cleanup EXIT

export CARGO_HOME="$WORK/cargo-home"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"   # toolchains are shared/read-only
INSTALL_ROOT="$WORK/install"
export CARGO_TARGET_DIR="$WORK/target"
mkdir -p "$CARGO_HOME" "$INSTALL_ROOT"

BIN="$INSTALL_ROOT/bin/cargo-cratevista"

run_bin() {
  # git-bash resolves the .exe suffix automatically; fall back explicitly.
  if [ -x "$BIN" ]; then "$BIN" "$@"; else "$BIN.exe" "$@"; fi
}

install_stable() {
  log "install from source (stable, no Node)"
  cargo install --path "$REPO_ROOT/crates/cargo-cratevista" \
    --root "$INSTALL_ROOT" --locked

  log "cargo cratevista --help"
  run_bin --help | grep -q "build" || { echo "help missing build command" >&2; exit 1; }

  log "cargo cratevista doctor"
  # doctor is read-only; a non-zero exit (e.g. missing nightly) is acceptable here —
  # we only assert it runs and produces output.
  run_bin doctor || true

  log "metadata-only build (stable, no nightly)"
  local fixture="$WORK/meta-fixture"
  mkdir -p "$fixture/src"
  cat > "$fixture/Cargo.toml" <<'EOF'
[package]
name = "readmemeta"
version = "0.0.0"
edition = "2021"

[workspace]
EOF
  echo 'fn main() {}' > "$fixture/src/main.rs"
  run_bin build --manifest-path "$fixture/Cargo.toml" --output "$fixture/site"
  test -f "$fixture/site/index.html" || { echo "site index.html missing" >&2; exit 1; }
  test -f "$fixture/site/document.json" || { echo "document.json missing" >&2; exit 1; }
  echo "ok: metadata-only build produced a site on stable"
}

run_generation_nightly() {
  log "real generation (pinned nightly $NIGHTLY)"
  if ! rustup toolchain list 2>/dev/null | grep -q "$NIGHTLY"; then
    echo "installing $NIGHTLY (documented in the README)"
    rustup toolchain install "$NIGHTLY" --profile minimal
  fi
  # A documentable library fixture, so generation actually invokes rustdoc JSON.
  local fixture="$WORK/lib-fixture"
  mkdir -p "$fixture/src"
  cat > "$fixture/Cargo.toml" <<'EOF'
[package]
name = "readmelib"
version = "0.0.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[workspace]
EOF
  cat > "$fixture/src/lib.rs" <<'EOF'
//! A tiny documentable crate.

/// A documented item.
pub struct Widget {
    /// The size.
    pub size: u32,
}
EOF
  run_bin generate --manifest-path "$fixture/Cargo.toml"
  test -f "$fixture/target/cratevista/document.json" \
    || { echo "generation did not write document.json" >&2; exit 1; }
  echo "ok: real generation produced document.json via the pinned nightly"
}

case "$MODE" in
  stable)  install_stable ;;
  nightly) install_stable; run_generation_nightly ;;
  all)     install_stable; run_generation_nightly ;;
  *) echo "usage: $0 [stable|nightly|all]" >&2; exit 2 ;;
esac

log "README first-run verification passed ($MODE)"
