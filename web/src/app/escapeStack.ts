// A single, ordered Escape-handler stack for nested dismissables (Issue 15,
// Phase 5). One document-level capture listener owns the Escape key; whatever was
// registered LAST (the topmost dismissable — e.g. a source viewer opened inside a
// modal inspector drawer) handles Escape first and stops the event, so it never
// also reaches the layer beneath (the drawer) or the global selection-clear
// handler. Registering through here avoids competing document-level Escape
// listeners with undefined precedence.

type EscapeHandler = () => void;

const stack: EscapeHandler[] = [];
let installed = false;

function onKeyDown(event: KeyboardEvent): void {
  if (event.key !== "Escape" || stack.length === 0) return;
  event.preventDefault();
  event.stopPropagation();
  // Pop THEN call: the stack is self-draining, and the handler's own effect-cleanup
  // disposer (which also removes by identity) becomes an idempotent no-op. Only the
  // topmost dismissable handles this Escape; layers beneath are untouched.
  const top = stack.pop();
  top?.();
}

function ensureInstalled(): void {
  if (installed || typeof window === "undefined") return;
  // On `window` in the capture phase: it is the single top-level authority, fires
  // before any bubble-phase handler, and is reachable by both real key events and
  // test-dispatched `keydown` on `window`.
  window.addEventListener("keydown", onKeyDown, true);
  installed = true;
}

/**
 * Pushes an Escape handler and returns a disposer that removes exactly this one
 * (by identity, wherever it currently sits — safe if inner layers already popped).
 * While anything is on the stack, Escape is consumed by the topmost handler only.
 */
export function pushEscape(handler: EscapeHandler): () => void {
  ensureInstalled();
  stack.push(handler);
  return () => {
    const index = stack.lastIndexOf(handler);
    if (index !== -1) stack.splice(index, 1);
  };
}

/** Current stack depth — for tests and for handlers that must defer to an inner
 *  dismissable (e.g. the wide-mode selection-clear yields while a source viewer is
 *  open). */
export function escapeDepth(): number {
  return stack.length;
}
