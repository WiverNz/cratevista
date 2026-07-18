# Accessibility

CrateVista targets a **WCAG 2.1 AA application baseline, not complete WCAG
certification**. This page records exactly what has been verified, and by what
method, so the gaps are visible rather than implied.

## What is verified, and how

### Automated rule checks (axe-core, jsdom)

`axe-core` runs over the rendered application in the component suite. Scope and
limits:

- It covers the app shell, toolbar, view tabs, graph region, inspector and
  panels as rendered in **jsdom**, not in a real browser engine.
- Automated rules catch a minority of accessibility problems by construction.
  A clean axe run is a floor, never a certificate.
- Rules requiring real layout or paint (notably **colour contrast**) cannot be
  evaluated meaningfully in jsdom and are handled as manual checks below.

### Keyboard and semantics (component tests, Vitest)

Roles, names, focus order and keyboard handlers are asserted against the
rendered DOM: the view `tablist` and its roving tabindex, the search
`listbox`/`option` results, the inspector, the graph list, and Escape handling.

### Real Chromium (Playwright)

Evidence that only a real browser engine can give — `web/e2e/tests/a11y.spec.ts`
and `web/e2e/tests/reduced-mode.spec.ts`, run against the production bundle:

- **Visible focus indicator.** Focus is moved by a real `Tab` press (so
  `:focus-visible` genuinely matches) and the computed style is asserted to
  carry an outline or a box-shadow — not `none`.
- **View tabs.** `ArrowRight`/`ArrowLeft` (wrapping), `Home` and `End` move
  focus and activate the tab (activation follows focus), and exactly one tab is
  in the tab order at a time (roving tabindex).
- **Search results by keyboard alone.** Type, `Tab` into the results, `Enter` to
  select; the inspector opens. No pointer involved.
- **GraphList by keyboard alone** (see below).
- **Escape** clears the selection and the inspector closes.
- **Inspector focus.** The inspector title carries `tabIndex={-1}`, so the app
  can move focus to newly revealed content without inserting a heading into the
  tab order.
- **Reduced motion.** With `prefers-reduced-motion: reduce` emulated, the app
  stays fully usable and **no element** retains a transition or animation longer
  than 50 ms — nothing essential is conveyed by motion.
- **Not colour alone.** Every node states its kind as text, the legend names
  each kind as text, and reduced mode marks hidden entries with the word
  "hidden" — colour is always redundant.
- **No horizontal clipping** at 1280, 1440 and 1920 px wide: the page never
  scrolls sideways, and the graph and inspector occupy separate, non-overlapping
  regions.

### GraphList keyboard evidence

The GraphList is the keyboard-reachable equivalent of the canvas: in reduced
mode it lists **every** entity, including those the graph does not render. It is
verified in real Chromium against the large benchmark fixture, so reduced mode
is entered through the **normal production budget policy** rather than a
test-only override:

- the reduced-mode banner is visible and states both counts as text;
- the list renders and exposes more entities than the graph does;
- an entity the graph does **not** render is reachable and focusable;
- `Enter` and `Space` both activate an entry;
- the reduced neighbourhood recentres on it and the inspector opens with it;
- `Escape` clears the selection and focus returns to a GraphList item rather
  than falling back to `<body>`;
- **no pointer interaction is required** at any step.

## Manual checks

These are performed by a person, not asserted by a test. Record the date and the
result when they are repeated.

- **Colour contrast.** The palette is authored against WCAG AA (4.5:1 body text,
  3:1 large text and UI boundaries). Verified by inspection in the browser's
  devtools contrast checker on the dark theme; automated contrast checking is
  not meaningful in jsdom, and the real-browser suite does not assert ratios.
- **Zoom / reflow.** Spot-checked at 200 % browser zoom on a desktop viewport.

## Not claimed

Stated plainly, because absence of evidence is not evidence of accessibility:

- **No screen-reader or other assistive-technology behaviour is claimed.** No
  testing has been performed with NVDA, JAWS, VoiceOver, Orca or any AT. Roles
  and names are asserted structurally; how a given AT announces them has not
  been observed.
- **No WCAG certification, audit or conformance statement** is offered. This is
  an engineering baseline.
- **The graph canvas is inherently visual.** React Flow's canvas is not a
  substitute for the GraphList, which is why the complete, searchable,
  keyboard-navigable entity list exists.
- Mobile/touch and high-contrast-mode behaviour are untested; the supported
  target is a desktop viewport.
