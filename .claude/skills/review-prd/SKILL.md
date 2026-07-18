---
name: review-prd
description: Review one CrateVista PRD for completeness, repository accuracy, scope, and implementability.
disable-model-invocation: true
argument-hint: <path-to-prd>
allowed-tools: Read, Glob, Grep, Write
---

Review the PRD at `$ARGUMENTS`.

Read:

1. `CLAUDE.md`
2. `ISSUES/CONTEXT.md`
3. the PRD;
4. its source issue;
5. relevant ADRs;
6. relevant repository code and tests.

Check:

- every source issue criterion is preserved;
- proposed modules match the repository;
- responsibilities do not overlap incorrectly;
- command and configuration contracts are unambiguous;
- error handling is specified;
- cross-platform implications are addressed;
- security and privacy concerns are addressed;
- tests can prove the acceptance criteria;
- out-of-scope work has not leaked in;
- unresolved questions are genuinely unresolved;
- implementation steps are ordered and independently verifiable.

Update the PRD in place with necessary corrections.

Append or update:

```markdown
## Review record

- Reviewed at: <date>
- Result: Ready for review | Changes required
- Major findings:
  - ...
```

Do not implement production code.

At the end, report:

- result;
- changes made;
- blocking decisions still needed.
