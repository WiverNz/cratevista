---
name: create-all-prds
description: Create or refresh implementation-ready PRDs for every CrateVista issue in dependency order.
disable-model-invocation: true
allowed-tools: Read, Glob, Grep, Write
---

Generate implementation-ready PRDs for all files matching:

```text
ISSUES/issue_*.md
```

## Process

1. Read `CLAUDE.md`.
2. Read `ISSUES/CONTEXT.md`.
3. List all issue files and sort them by numeric prefix.
4. Explore the current repository once to understand its state.
5. Read all issues before writing the first PRD so cross-issue boundaries remain consistent.
6. For each issue, apply the full requirements of the `create-prd` skill.
7. Write one PRD per issue under `PRD/` using the same filename.
8. Create or update `PRD/INDEX.md`.

## `PRD/INDEX.md` requirements

Include:

- product summary;
- issue/PRD table;
- dependencies;
- recommended implementation order;
- major cross-cutting decisions;
- open questions;
- status column with one of:
  - Draft
  - Ready for review
  - Approved
  - In progress
  - Implemented
  - Verified

Do not mark a PRD `Approved` unless the user has explicitly approved it.

## Cross-PRD consistency

Ensure:

- crate/module names are consistent;
- schema concepts are consistent;
- CLI flags do not conflict;
- output paths are consistent;
- the same responsibility is not independently implemented in multiple issues;
- later PRDs reuse earlier boundaries;
- dependencies and prerequisites are explicit.

## Restrictions

- Do not implement production code.
- Do not skip an issue because its implementation does not exist yet.
- Do not combine all issues into one giant PRD.
- Do not overwrite explicit user decisions in an existing PRD without calling out the change.

When finished, report:

- generated/updated PRD paths;
- dependency order;
- decisions that require user approval before implementation.
