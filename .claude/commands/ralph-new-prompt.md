# ralph-new-prompt: Create a Custom Loop Prompt

> **Usage:** Run as a Claude Code slash command: `/ralph-new-prompt`

You are a prompt engineer helping the user create a new Ralph loop prompt. Your job is to guide them through designing a well-structured `PROMPT_<name>.md` that works with `./loop.sh <name>`.

---

## Step 1: Detect Context

Determine where you are running:

- **If `templates/shared/` directory exists in the project root:** You are in the **ralph source project**. New prompts will be created at `templates/shared/PROMPT_<name>.md` so they ship with `ralph-init`.
- **Otherwise:** You are in a **ralph-init'd project**. New prompts will be created at `./PROMPT_<name>.md` for local use only.

Tell the user which context you detected and where the new prompt file will be created.

---

## Step 2: Read Existing Prompts as Examples

Read these files to understand Ralph's prompt conventions (adjust paths based on context detected above):

- `PROMPT_plan.md` — A read-only loop prompt (studies code, generates a plan, does NOT modify code)
- `PROMPT_build.md` — A read-write loop prompt (implements code, runs tests, commits)

Study their structure carefully. These are your reference examples.

---

## Step 3: Gather Requirements

Use `AskUserQuestion` for each of the following. Do NOT skip any step.

### 3a. Purpose

Ask: "What is this loop mode for? What task should it perform each iteration?"

Offer examples to spark ideas:
- **Migration** — Read old API, read new API spec, migrate one module per iteration
- **Documentation** — Read code, write or update docs for one module per iteration
- **Testing** — Read specs + code, write missing tests for one component per iteration
- **Refactoring** — Read code smells list, refactor one area per iteration
- **Audit** — Review one component against specs, document findings per iteration
- **Code Review** — Read diffs or PRs, provide structured feedback per iteration

### 3b. Mode Name

Ask: "What should the mode be called? This becomes the filename (`PROMPT_<name>.md`) and the loop argument (`./loop.sh <name>`)."

Rules for the name:
- Lowercase letters, numbers, and hyphens only
- No spaces or underscores
- Examples: `audit`, `docs`, `migrate`, `test`, `code-review`

### 3c. Orientation Sources (Phase 0)

Ask: "What files/directories should the agent study before starting each iteration? These become the Phase 0 orientation steps."

Common choices:
- `specs/*` — application specifications
- `IMPLEMENTATION_PLAN.md` — current plan and status
- `src/*` or `crates/*` — source code
- `src/lib/*` — shared utilities
- `docs/*` — existing documentation
- `tests/*` — existing test files
- Other project-specific directories

### 3d. Core Task

Ask: "Describe the core task the agent should perform each iteration. Be specific — what's the input, what's the action, what's the output?"

### 3e. File Modification

Ask: "Does this mode modify files in the project? (This determines whether guardrails and git commit steps are needed.)"

Options:
- **Yes, it modifies/creates files** — Will include guardrails section and git commit step (like `PROMPT_build.md`)
- **No, it's read-only analysis** — Will skip guardrails and commit steps (like `PROMPT_plan.md`)

### 3f. Exit Conditions

Ask: "What should the agent do at the end of each iteration to signal completion?"

Common patterns:
- Update `IMPLEMENTATION_PLAN.md` with findings
- Commit changes with a descriptive message and push
- Write findings to a log file (e.g., `AUDIT_LOG.md`)
- Update a tracking document

---

## Step 4: Generate the Prompt

Using the gathered information, generate a `PROMPT_<name>.md` file following this anatomy:

### Ralph Prompt Anatomy

```
Phase 0 — Orientation (lines numbered 0a, 0b, 0c, 0d)
  Load context using parallel subagents. Read specs, plans, source code.
  Use "up to N parallel Sonnet subagents" for bulk reads.
  This is the "eyes open" phase — orient before acting.

Phases 1-N — Core Task Instructions (numbered sequentially)
  The actual work of one iteration. Be specific about:
  - What to do
  - How to verify it's correct
  - What to produce/output
  - When to use subagents and what tier (Sonnet for reads, Opus for reasoning)

Guardrails — Standing Orders (only for file-modifying prompts)
  Numbered with escalating 9s: 99999, 999999, 9999999, etc.
  These are rules that apply EVERY iteration regardless of the specific task.
  Higher number = more critical rule.
  Examples from PROMPT_build.md:
  - Keep IMPLEMENTATION_PLAN.md current
  - Update AGENTS.md when learning new commands
  - Implement completely, no placeholders
  - Keep AGENTS.md operational only (no status updates)
```

**Additional conventions:**
- **First line must be a description comment:** `<!-- description: One-line summary of what this mode does -->` — This is displayed by `./loop.sh help` alongside the mode name. Keep it under 60 characters.
- Use `[square-bracket placeholders]` for values that should be customized per-project (e.g., `[project-specific goal]`)
- Reference `@IMPLEMENTATION_PLAN.md` and `@AGENTS.md` with `@` prefix when they should be loaded as context
- Specify subagent counts and tiers: "up to 500 Sonnet subagents for searches/reads", "1 Opus subagent for complex reasoning"
- Keep prompts concise — the agent reads this every iteration, so density matters

### Present for Review

Show the generated prompt to the user. Use `AskUserQuestion` to ask:
"Here's the generated prompt. Want me to write it as-is, or would you like to make changes?"

Options:
- Write it as-is
- I want to make changes (then iterate based on feedback)

---

## Step 5: Write the File

Write the prompt to the appropriate location based on context detected in Step 1:

- **Ralph source project:** `templates/shared/PROMPT_<name>.md`
- **Ralph-init'd project:** `./PROMPT_<name>.md`

After writing, confirm:

```
Created: PROMPT_<name>.md

To use this mode:
  ./loop.sh <name>              # Unlimited iterations
  ./loop.sh <name> 10           # Max 10 iterations
  ./loop.sh help                # See all available modes
```

If in the ralph source project, also note: "This prompt will be included in all future `ralph-init` scaffolds. To use it in existing projects, copy it manually."
