---
name: implement-prd
description: Implement one approved CrateVista PRD, verify it, and synchronize its acceptance checklist.
disable-model-invocation: true
argument-hint: <path-to-approved-prd>
allowed-tools: Read, Glob, Grep, Edit, Write, Bash
---

Implement the PRD at `$ARGUMENTS`.

## Preconditions

Read:

1. `CLAUDE.md`
2. `ISSUES/CONTEXT.md`
3. the selected PRD;
4. the source issue;
5. relevant ADRs;
6. relevant implementation and tests.

Stop without implementing if:

- the PRD does not exist;
- the PRD is materially incomplete;
- the PRD contains a blocking open question;
- the selected work depends on an unimplemented prerequisite;
- the requested scope conflicts with current repository state and cannot be resolved safely.

Explain the blocker instead of guessing.

## Implementation process

1. Restate the implementation boundary internally.
2. Inspect existing code before creating new modules.
3. Implement the smallest coherent vertical slice that satisfies the PRD.
4. Add or update tests alongside production changes.
5. Keep public interfaces documented.
6. Run focused tests during development.
7. Run all PRD verification commands.
8. Run repository quality gates.
9. Update the PRD acceptance checklist:
   - mark only verified criteria complete;
   - include evidence or commands;
   - leave incomplete criteria unchecked.
10. Update `PRD/INDEX.md` status accurately.
11. Add an ADR only when the implementation introduces a durable architectural decision not already covered.

## Restrictions

- Do not silently expand scope.
- Do not weaken tests to make them pass.
- Do not delete acceptance criteria.
- Do not claim completion when verification failed.
- Do not install global software without explicit user approval.
- Do not publish packages or create releases unless explicitly requested.
- Do not commit secrets or machine-specific paths.

## Completion report

Report:

- implemented behavior;
- important files changed;
- tests and checks run;
- acceptance criteria still incomplete;
- follow-up work outside this PRD.
