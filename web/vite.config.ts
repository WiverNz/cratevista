import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Relative base so the bundle works both embedded by the server (served at `/`)
// and under a static-hosting subpath (issue 10). Deterministic asset names help
// the committed-dist drift check.
export default defineConfig({
  base: "./",
  plugins: [react()],
  build: {
    // The single authoritative bundle lives INSIDE the server crate so
    // `cargo package -p cratevista-server` includes it (issue 10, Phase 5A). The
    // path is outside the web project root, so `emptyOutDir` is set explicitly:
    // Vite then clears exactly this directory (never a parent) before writing, so
    // a normal build cannot leave stale fingerprinted `index-*.js` beside the new
    // set. The reproducibility check (`check:dist`) builds into a temp dir instead
    // and never mutates this authoritative directory.
    outDir: "../crates/cratevista-server/embedded",
    emptyOutDir: true,
    sourcemap: false,
    // Stable, content-hashed asset names. `hashCharacters: "hex"` matters: the
    // server's `is_fingerprinted` rule (PRD 06) only recognises a hex hash
    // segment, and only provably-fingerprinted assets get immutable caching.
    // Base64url hashes (Vite's default) would silently fall back to `no-cache`.
    rollupOptions: {
      output: {
        hashCharacters: "hex",
        entryFileNames: "assets/[name].[hash].js",
        chunkFileNames: "assets/[name].[hash].js",
        assetFileNames: "assets/[name].[hash][extname]",
      },
    },
  },
  // The ELK layout worker is bundled separately by Vite; mirror the hex-hash
  // naming so the server's fingerprint rule recognises it too, and force an ES
  // module worker (same-origin, never a blob: worker — see the CSP amendment).
  worker: {
    format: "es",
    rollupOptions: {
      output: {
        hashCharacters: "hex",
        entryFileNames: "assets/[name].[hash].js",
        chunkFileNames: "assets/[name].[hash].js",
        assetFileNames: "assets/[name].[hash][extname]",
      },
    },
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./vitest.setup.ts"],
    include: ["tests/**/*.test.{ts,tsx}", "src/**/*.test.{ts,tsx}"],
    css: false,
  },
});
