<!-- description: Autonomous spec refinement — convert and refine specs from requirements -->
0a. Find the OLDEST active requirements file in `.planning/`. Use a glob to list all `reqs-*.md` files in `.planning/` (NOT `.planning/archive/`), sort them, and read the one with the smallest increment number (e.g., `reqs-001.md` before `reqs-002.md`). Do NOT read from `.planning/archive/`. Do NOT read from `docs/` or any other location. If no `reqs-*.md` file exists in `.planning/`, stop and commit a note to `IMPLEMENTATION_PLAN.md` explaining that no requirements were found.
0b. Check if a matching `decisions-*.md` file exists in `.planning/` (same increment number as the reqs file). If it exists, read it with a subagent. Do NOT read archived decisions.
0c. Read all existing `specs/*.md` files (if any) with up to 250 parallel Sonnet subagents to understand current spec coverage.
0d. If source code exists in `src/` or `crates/` or `apps/`, read it with up to 250 parallel Sonnet subagents to understand what has been implemented.

1. **Determine work mode and execute:**

   **A. If no specs exist yet** — create initial spec drafts from the requirements:
   - Identify **Topics of Concern** from the reqs — distinct areas of the system that can each be described in one sentence WITHOUT using the word "and." If you need "and," it's two topics.
   - For each topic, create `specs/<topic-name>.md` using the template below. Use short, lowercase-hyphenated topic names.
   - Extract requirements from the reqs document. Every requirement should trace back to the reqs. If you infer something not explicitly stated, mark it with `[inferred]`.
   - Cross-cutting constraints that apply to ALL specs should get their own spec file (e.g., `specs/design-principles.md`).

   **B. If specs already exist (no source code)** — refine them:
   - Compare specs against the reqs document. Fill gaps in requirements and acceptance criteria.
   - Resolve inconsistencies between specs (e.g., two specs claiming ownership of the same concern).
   - Improve specificity and testability — vague requirements become concrete, testable statements.
   - If the reqs contain decisions or constraints not yet reflected in specs, add them.

   **C. If specs exist AND source code exists** — compare specs against the implementation:
   - Where code implements something not in a spec, add the requirement to the appropriate spec (mark with `[observed from code]`).
   - Where specs describe something not yet implemented, leave the requirement as-is — that's a build task, not a spec problem.
   - Where code contradicts a spec, flag the mismatch in a comment at the bottom of the spec under `## Notes`.
   - Also perform all refinements from mode B above.

2. **Quality pass** — after completing step 1, review all specs for structural quality:
   - Verify every spec has all required sections: Purpose, Requirements, Constraints, Acceptance Criteria, References.
   - Check that requirements use imperative voice and are specific enough to verify against code.
   - Check for topic overlap — if two specs claim the same concern, consolidate into one.
   - Ensure cross-cutting constraints have their own spec file rather than being duplicated across specs.
   - Remove duplicate requirements that appear in multiple specs (keep in the most relevant spec).
   - Where something is ambiguous and cannot be resolved from the reqs or decisions docs, mark it with `[needs-clarification]` and move on — do NOT block.

3. After all spec changes are complete: `git add specs/` then `git commit` with a message summarizing what specs were created or refined, then `git push`.

---

## Spec File Template

Every spec file in `specs/` must use this structure:

```markdown
# Topic Name

## Purpose
One paragraph: what this part of the system does and why it exists.

## Requirements
Bulleted list of concrete, testable requirements. Each bullet is one thing the code must do.
- Use imperative voice ("Support X", "Provide Y", "Reject Z")
- Be specific enough that an agent can search the codebase and confirm present/absent
- Include data shapes, APIs, and behaviors — not implementation details
- If a requirement references another topic, name it explicitly

## Constraints
Bulleted list of rules, boundaries, and locked decisions.
- Where it lives (crate, module, directory)
- What it must NOT depend on
- Patterns to follow
- Performance, compatibility, or platform requirements

## Acceptance Criteria
Numbered list of concrete checks. Each is a true/false statement when examining the codebase.
1. Specific behavior is observable
2. Specific files/modules exist
3. Integration points with other topics work

## References
Optional. Pointers to reference implementations or planning doc sections.
```

---

99999. IMPORTANT: Only read `.planning/reqs-*.md` and `.planning/decisions-*.md` with the smallest increment number. Do NOT read `.planning/archive/`, `docs/`, or any other planning documents. The active reqs file is the single source of truth for this iteration.
999999. Use the spec template exactly as defined above (Purpose / Requirements / Constraints / Acceptance Criteria / References). All specs must be in `specs/` with lowercase-hyphenated filenames.
9999999. Extract, don't invent. Mark inferred requirements with `[inferred]`. Mark requirements observed from code with `[observed from code]`. Mark unresolvable ambiguities with `[needs-clarification]`.
99999999. One spec per topic. Don't combine topics or split a topic across files.
999999999. Specs describe WHAT not HOW. Requirements use imperative voice. Specs describe the end state, not ordering or phases.
9999999999. Keep specs scannable — bullets over paragraphs. Dense prose wastes context in future loop iterations.
99999999999. Do NOT delete or remove existing requirements from specs unless they directly contradict the reqs document. Refinement means adding clarity, not reducing scope.
999999999999. This prompt runs AUTONOMOUSLY in a headless loop. Do NOT use AskUserQuestion or any interactive tools. Do NOT stop and wait for user input. Complete the full cycle (orient → refine → quality pass → commit) every iteration.
9999999999999. Prefer conservative refinement over aggressive restructuring. Small improvements each iteration compound. Do not reorganize all specs in a single pass — focus on the highest-value refinements first.
