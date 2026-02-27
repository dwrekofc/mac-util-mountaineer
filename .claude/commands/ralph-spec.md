# ralph-spec: Create Specs from Requirements (Interactive)

You are a specification writer working collaboratively with the user. Your job is to read the active requirements document and convert it into Ralph-compatible spec files in `specs/`.

**This command is for initial spec creation and major restructuring.** After specs are created, run `./loop.sh spec` to autonomously refine and improve them over multiple iterations.

## How Ralph Uses Specs

Ralph's planning loop (`PROMPT_plan.md`) reads every file in `specs/*` and compares them against the codebase to generate a prioritized task list. Ralph's build loop (`PROMPT_build.md`) reads specs to know *what* to build and *how to verify* it's done. Specs are the **source of truth** — they must be clear enough that an AI agent with no prior context can read a spec and know exactly what code should exist.

## Your Process

### PHASE 1: Read Requirements & Resolve Questions

1. Find the OLDEST active requirements file in `.planning/`. List all `reqs-*.md` files in `.planning/` (NOT `.planning/archive/`), sort them, and read the one with the smallest increment number (e.g., `reqs-001.md` before `reqs-002.md`). Also read the matching `decisions-*.md` if it exists (same increment number). **Do NOT read from `.planning/archive/`.** Do NOT read `docs/` or any other location — the active reqs file is the single source of truth.

2. Identify **outstanding questions, ambiguities, or gaps** in the requirements. These might be:
   - Requirements that are vague or underspecified
   - Decisions marked as `undecided`, `exploring`, or `conflicts` in the decisions doc
   - Areas where you need to understand the user's intent before you can write a precise spec
   - Trade-offs that could go either way

3. Use `AskUserQuestion` to gather clarity on each outstanding question. Group related questions together. Help the user make decisions by presenting options with trade-offs where appropriate. Do not proceed until the user has answered.

4. **Update source documents.** After resolving questions with the user, update the source `reqs-XXX.md` and `decisions-XXX.md` files to reflect any clarifications, corrections, or new decisions made during this session. These updates are authoritative — downstream processes (`loop.sh spec`, `loop.sh plan`) rely on these files being current. Do NOT leave answered questions or changed requirements only in conversation history.

### PHASE 2: Draft Outline (propose topics, get approval)

5. Identify **Topics of Concern** — distinct areas of the system that can each be described in one sentence WITHOUT using the word "and." If you need "and," it's two topics. You MUST create at least one topic per JTBD in the requirements. If a JTBD is too large to implement in a single context window (~128k tokens), split it into multiple smaller topics. Prefer many small specs over few large specs.

6. Present a **draft outline** to the user in this exact format:

```
PROPOSED TOPICS & SPECS
=======================

1. topic-name — One sentence describing scope.
   Source: JTBD N
   Key reqs: bullet | bullet | bullet

2. topic-name — One sentence describing scope.
   Source: JTBD N, JTBD M
   Key reqs: bullet | bullet | bullet

...
```

Rules for the draft outline:
- Use short, lowercase-hyphenated topic names (these become filenames: `specs/topic-name.md`)
- One-sentence scope description — be specific, not vague
- Each topic MUST include a `Source:` line mapping it to the JTBD(s) it traces to
- "Key reqs" are 3-5 truncated bullet points (just enough to confirm scope, not full requirements)
- Order topics by dependency (foundational topics first)
- If the reqs have locked decisions, constraints, or design principles that apply to ALL topics, note them once under a header called `CROSS-CUTTING CONSTRAINTS` at the top

7. **Validate coverage before presenting.** Before showing the outline to the user, verify: (a) every JTBD in the reqs has at least one corresponding topic, (b) no single topic covers more than one JTBD unless it is genuinely atomic. If a JTBD has no topic, add one. If a topic maps to multiple JTBDs, evaluate whether it should be split.

8. **Stop and wait for user approval.** Do not write any spec files until the user confirms the topic list. The user may:
   - Approve as-is
   - Merge, split, rename, add, or remove topics
   - Adjust scope of individual topics
   - Ask questions

### PHASE 3: Write Specs (after approval)

For each approved topic, write `specs/<topic-name>.md` using this structure:

```markdown
# Topic Name

## Source
JTBD N: [title] | JTBD M: [title]

## Purpose
One paragraph: what this part of the system does and why it exists.

## Requirements
Bulleted list of concrete, testable requirements. Each bullet is one thing the code must do.
- Use imperative voice ("Support X", "Provide Y", "Reject Z")
- Be specific enough that an agent can search the codebase and confirm present/absent
- Include data shapes, APIs, and behaviors — not implementation details
- If a requirement references another topic, name it explicitly (e.g., "Use tokens from theme-engine")

## Constraints
Bulleted list of rules, boundaries, and locked decisions that govern HOW this topic is implemented.
- Where it lives (crate, module, directory)
- What it must NOT depend on
- Patterns to follow (builder pattern, trait-based, etc.)
- Provenance/attribution rules if adapting external code
- Performance, compatibility, or platform requirements

## Acceptance Criteria
Numbered list of concrete checks. Each one is a statement that is either true or false when looking at the codebase.
1. Specific behavior is observable
2. Specific files/modules exist
3. Integration points with other topics work

## References
Optional. Pointers to reference implementations, external codebases, or planning doc sections that informed this spec.
```

Rules for writing specs:
- **Include a `## Source` section** at the top of each spec (after the title, before Purpose) listing which JTBD(s) and user stories this spec traces to. This enables traceability back to requirements.
- **Extract, don't invent.** Every requirement should trace back to something in the reqs. If you infer something not explicitly stated, mark it with `[inferred]`.
- **Requirements describe WHAT, not HOW.** "Support light and dark themes" not "Create a HashMap<String, Theme>."
- **One spec per topic.** Don't split a topic across files or combine topics into one file.
- **Specs don't describe ordering or phases.** That's the implementation plan's job. Specs describe the end state.
- **Cross-cutting constraints** that apply to ALL specs should get their own spec file (e.g., `specs/design-principles.md`) rather than being repeated in every file.
- **Keep specs scannable.** Ralph reads them every loop iteration. Dense prose wastes context. Bullets > paragraphs.

After writing all spec files, print a summary:

```
SPECS WRITTEN
=============
specs/topic-a.md — N requirements, M acceptance criteria
specs/topic-b.md — N requirements, M acceptance criteria
...

Total: X specs, Y requirements, Z acceptance criteria

Next step: ./loop.sh spec  (autonomous refinement)
     then: ./loop.sh plan  (gap analysis against code)
```

## Important

- **Backflow is mandatory.** Any clarifications, corrections, or decisions made during this interactive session are AUTHORITATIVE. Before writing specs, you MUST propagate these changes back to the reqs and decisions files. The reqs doc must always reflect the latest agreed-upon state. Downstream processes (`loop.sh spec`, `loop.sh plan`) read these files as the source of truth and will produce conflicts if they are stale.
- PHASE 2 output is a SHORT draft for quick human review. Do NOT write full specs in Phase 2.
- Do NOT create `specs/` files until the user explicitly approves the topic list.
- If requirements conflict with each other, note the conflict in the spec and ask the user to resolve it.
- This command is for **collaborative creation**. For autonomous refinement of existing specs, use `./loop.sh spec`.
