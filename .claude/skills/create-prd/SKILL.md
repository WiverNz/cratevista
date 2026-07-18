---
name: create-prd
description: Convert one CrateVista issue into an implementation-ready PRD without implementing it.
disable-model-invocation: true
argument-hint: <path-to-issue>
allowed-tools: Read, Glob, Grep, Write
---

Create an implementation-ready Product Requirements Document for the issue provided in `$ARGUMENTS`.

## Mandatory inputs

Read, in this order:

1. `CLAUDE.md`
2. `ISSUES/CONTEXT.md`
3. The issue file supplied in `$ARGUMENTS`
4. Existing files under `docs/adr/`
5. Existing related PRDs under `PRD/`
6. The current repository implementation relevant to the issue

If `$ARGUMENTS` is empty, stop and ask for an issue path.

## Repository exploration

Explore the repository before writing the PRD.

Determine:

- current workspace structure;
- existing modules and ownership boundaries;
- established terminology;
- existing commands, tests, fixtures, and CI;
- relevant design decisions;
- code that should be reused rather than duplicated;
- contradictions between the issue and current repository state.

Do not assume the repository is empty.

## Output path

Write the PRD to:

```text
PRD/<issue-file-name-without-directory>
```

For example:

```text
ISSUES/issue_03_cargo_metadata.md
→
PRD/issue_03_cargo_metadata.md
```

Create the `PRD/` directory if necessary.

## Required PRD structure

Use this structure:

```markdown
# PRD — <title>

## Status
## Source issue
## Summary
## Problem statement
## Goals
## Non-goals
## Current repository state
## Terminology
## User-visible behavior
## Functional requirements
## Technical design
### Module boundaries
### Data model
### Control flow
### Error handling
### Compatibility
### Security and privacy
## CLI/API/configuration changes
## Files and modules to create or modify
## Testing strategy
### Unit tests
### Integration tests
### End-to-end tests
### Fixtures
## Performance considerations
## Observability and diagnostics
## Documentation changes
## Rollout and migration
## Risks and mitigations
## Alternatives considered
## Implementation sequence
## Acceptance criteria
## Open questions
## Traceability
```

## PRD quality requirements

The PRD must:

- be specific enough for a fresh Claude Code session to implement;
- use repository paths and current symbol names where they already exist;
- clearly separate facts from proposals;
- identify assumptions;
- identify unresolved decisions rather than hiding them;
- describe stable module boundaries;
- avoid broad “update code as needed” instructions;
- map every issue acceptance criterion to implementation and verification;
- include exact commands that will verify completion;
- explain failure behavior and platform implications;
- preserve scope and non-goals from `ISSUES/CONTEXT.md`;
- call out any issue requirement that is obsolete or contradicted by the repository.

## Restrictions

- Do not implement production code.
- Do not modify the source issue.
- Do not mark uncertain decisions as settled without evidence.
- Do not remove acceptance criteria.
- Do not create a second unrelated plan file.

When finished, report only:

- PRD path;
- important decisions made;
- unresolved questions requiring user review.
