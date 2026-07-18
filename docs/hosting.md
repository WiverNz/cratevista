# Hosting a CrateVista static site

`cargo cratevista build` produces a **self-contained static site** — `index.html`,
fingerprinted app assets under `assets/`, and the three JSON artifacts
(`document.json`, `generation.json`, `diagnostics.json`). Any static HTTP host serves
it with **no running Rust server** and **no Node.js**.

```text
site/
  index.html
  assets/**          # fingerprinted JS/CSS
  document.json
  generation.json
  diagnostics.json
```

## Build it

```bash
# Default output: target/cratevista/site
cargo cratevista build

# A custom directory:
cargo cratevista build --output dist
```

## How it stays portable

- **Relative URLs.** The bundle is built with a relative base, so `index.html`
  references `./assets/…` and the app fetches `./document.json` (and the two sibling
  JSON files) **relative to its own directory**. The same output works unchanged at a
  URL root or under any subpath — no per-host rebuild, no rewriting.
- **Query-string routing.** All durable state (`view`, `entity`/`relation`, `q`,
  `kinds`, `focus`, `edges`, `stage`) lives in the URL query string, never in path
  segments. A refresh re-requests `index.html` at the same path and re-reads the
  query, so state survives with no server-side routing.
- **Static-mode marker.** `build` injects a CSP-safe
  `<meta name="cratevista-mode" content="static">` into `index.html`. The app reads
  it before any fetch and enters static mode: it loads the sibling JSON artifacts and
  constructs **no** `/api` capability at all.

## Hosting targets

### URL root

Serve the output directory as the site root:

```
https://example.com/           →  index.html, ./assets/…, ./document.json
```

Nothing special is required.

### Arbitrary subpath / `--base-path`

Relative URLs already work under any subpath, so you usually need **nothing**:

```
https://example.com/reports/cratevista/    →  works as-is
```

`--base-path` is **optional** — only for a host that needs an absolute `<base href>`
(for example when serving the same assets from a rewritten path). It writes exactly
one `<base href="/<path>/">` into `index.html` and changes nothing else:

```bash
cargo cratevista build --output dist --base-path /cratevista/
```

Accepted forms normalize to `/<path>/` (`repo`, `/repo`, `/repo/` → `/repo/`). A
scheme, query, fragment, `..`, backslash or interior whitespace is rejected.

### GitHub Pages (project page)

A project page is served under `https://<user>.github.io/<repo>/`. Because the site
uses relative URLs, build it and publish the output directory as the Pages artifact —
no `--base-path` is required (though `--base-path /<repo>/` is harmless if you prefer
an explicit base).

```bash
cargo cratevista build --output public
# publish `public/` as the Pages artifact
```

### GitLab Pages

Publish the output directory as the `public/` artifact of a Pages job. Relative URLs
work under the project's Pages subpath unchanged.

### Generic static hosting

Any host that serves files over HTTP works: S3 + CloudFront, Netlify, a plain nginx
`root`, `python -m http.server`, etc. Serve the directory; no configuration beyond
static file serving is needed.

### CI artifact download / hosting

The output directory is a normal folder — upload it as a CI artifact and download it,
or copy it into a hosting bucket. It carries no absolute paths, so it relocates
freely.

## Requirements and caveats

- **HTTP is required; `file://` is not supported.** Browsers block `fetch` of the
  sibling `./document.json` over `file://`, so opening `index.html` directly from
  disk will not load the document. Use any static HTTP host (even a local one).
- **Same-origin serving.** The site fetches its artifacts from its own origin;
  serving the JSON from a different origin is not part of the contract.
- **No server, no Node.** Hosting and using a produced site needs neither a running
  Rust server nor Node.js.
- **No `/api`, no EventSource.** A static site issues **zero** requests to any
  `/api/**` route and opens **no** `EventSource`. There is no live reload in static
  mode.
- **No source directory.** No `source/` or `snippets/` directory is produced; the
  explorer shows repository-relative *locations* and links, never copied file
  contents.

## Caching

The `assets/**` files are content-fingerprinted, so they are safe to cache
aggressively (immutable). The three JSON artifacts and `index.html` are **not**
fingerprinted — cache them with revalidation (or a short max-age) so a re-published
site is picked up. A typical policy:

```
/assets/*     Cache-Control: public, max-age=31536000, immutable
/*.json       Cache-Control: no-cache
/index.html   Cache-Control: no-cache
```
