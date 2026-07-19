#!/usr/bin/env bash
# Open a CrateVista interactive explorer for a local Rust project.
#
#   ./scripts/open-project.sh /path/to/my-rust-project
#   ./scripts/open-project.sh /path/to/my-rust-project/Cargo.toml
#   ./scripts/open-project.sh /path/to/my-rust-project --static
#
# Default (live): generate + serve + open the browser, with local source access and
# watch mode, bound to loopback, with CrateVista choosing an available port. The
# server keeps running in the foreground; Ctrl+C stops it.
#
# This script never modifies the target project or any Git state; CrateVista's only
# writes are under the project's own `target/cratevista/`.
set -euo pipefail

NIGHTLY="nightly-2026-07-01"

usage() {
  cat >&2 <<EOF
Usage: $(basename "$0") <project-dir-or-Cargo.toml> [--static]

  <project-dir-or-Cargo.toml>  A Rust workspace directory or a path to its Cargo.toml.
  --static                     Build a self-contained static snapshot instead of the
                               live explorer (see the note printed by --static).
EOF
}

STATIC=0
PROJECT=""
for arg in "$@"; do
  case "$arg" in
    --static) STATIC=1 ;;
    -h|--help) usage; exit 0 ;;
    *://*) echo "error: expected a filesystem path, not a URL: $arg" >&2; exit 2 ;;
    -*) echo "error: unknown option: $arg" >&2; usage; exit 2 ;;
    *)
      if [ -z "$PROJECT" ]; then PROJECT="$arg"
      else echo "error: unexpected extra argument: $arg" >&2; exit 2; fi ;;
  esac
done

if [ -z "$PROJECT" ]; then usage; exit 2; fi
if [ ! -e "$PROJECT" ]; then echo "error: path does not exist: $PROJECT" >&2; exit 2; fi

# Resolve the manifest from either a directory or a direct Cargo.toml path.
if [ -d "$PROJECT" ]; then
  MANIFEST="$PROJECT/Cargo.toml"
else
  MANIFEST="$PROJECT"
fi
if [ ! -f "$MANIFEST" ] || [ "$(basename "$MANIFEST")" != "Cargo.toml" ]; then
  echo "error: no Cargo.toml found (looked at: $MANIFEST)." >&2
  echo "       Pass a workspace directory or a path to its Cargo.toml." >&2
  exit 2
fi
# Absolute path (handles spaces).
MANIFEST="$(cd "$(dirname "$MANIFEST")" && pwd)/Cargo.toml"
PROJECT_ROOT="$(dirname "$MANIFEST")"

# --- Command resolution -----------------------------------------------------
# 1) Prefer the installed `cargo cratevista` subcommand.
# 2) Otherwise, if run from the CrateVista repository, fall back to `cargo run`.
# 3) Otherwise fail with a clear instruction.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if cargo cratevista --help >/dev/null 2>&1; then
  CMD=(cargo cratevista)
elif [ -f "$REPO_ROOT/crates/cargo-cratevista/Cargo.toml" ]; then
  echo "note: 'cargo cratevista' is not installed; using the repository build." >&2
  CMD=(cargo run --quiet --manifest-path "$REPO_ROOT/Cargo.toml" -p cargo-cratevista -- cratevista)
else
  echo "error: CrateVista is not available." >&2
  echo "       Install it:  cargo install --path crates/cargo-cratevista   (from the CrateVista repo)" >&2
  echo "       or run this script from inside the CrateVista repository." >&2
  exit 127
fi

# --- Pinned nightly (reported, never installed automatically) ---------------
if command -v rustup >/dev/null 2>&1; then
  if ! rustup toolchain list 2>/dev/null | grep -q "$NIGHTLY"; then
    echo "warning: the pinned nightly '$NIGHTLY' is not installed." >&2
    echo "         Generating rustdoc JSON needs it. Install it yourself with:" >&2
    echo "           rustup toolchain install $NIGHTLY" >&2
    echo "         (A metadata-only workspace still works without it.)" >&2
  fi
fi

if [ "$STATIC" -eq 1 ]; then
  # Static snapshot: build the self-contained site under the project's own target.
  echo "Building a static snapshot for: $PROJECT_ROOT"
  "${CMD[@]}" build --manifest-path "$MANIFEST" --toolchain "$NIGHTLY"
  status=$?
  SITE="$PROJECT_ROOT/target/cratevista/site"
  echo ""
  echo "Static snapshot built at:"
  echo "  $SITE"
  echo ""
  echo "This is a generated SNAPSHOT:"
  echo "  - no source-content API (/api/**);"
  echo "  - no live reload — rebuild after changes;"
  echo "  - it must be served over HTTP (file:// is unsupported)."
  echo ""
  echo "Automatic serving is not supported by this launcher: CrateVista has no"
  echo "built-in static-directory server, and this script will not pull in Python,"
  echo "Node or another dependency just to serve files. Serve the directory above"
  echo "with any static HTTP host (it uses relative URLs, so a subpath is fine)."
  exit $status
fi

# Live explorer: exec so the child's exit code is preserved and Ctrl+C stops it.
echo "Launching the live CrateVista explorer for: $PROJECT_ROOT"
echo "  (source access + watch mode, loopback only; Ctrl+C to stop)"
exec "${CMD[@]}" open \
  --manifest-path "$MANIFEST" \
  --toolchain "$NIGHTLY" \
  --source \
  --watch
