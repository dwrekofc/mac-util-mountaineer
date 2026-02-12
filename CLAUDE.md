## Beads Rust (br) — Dependency-Aware Issue Tracking
This project uses br (beads rust) for issue tracking. br provides a lightweight, dependency-aware issue database and CLI for selecting "ready work," setting priorities, and tracking status.

### Quick Refernce Essential Commands
```bash
br ready              # Show issues ready to work (no blockers)
br list --status open # All open issues
br show <id>          # Full issue details with dependencies
br create --title "Fix bug" --type bug --priority 2 --description "Details here"
br update <id> --status in_progress
br close <id> --reason "Completed"
br sync               # Export to JSONL for git sync
```

### Key Concepts
- **Dependencies**: Issues can block other issues. `br ready` shows only unblocked work.
- **Priority**: P0=critical, P1=high, P2=medium, P3=low, P4=backlog
- **Types**: task, bug, feature, epic, question, docs
- **JSON output**: Always use `--json` or `--robot` when parsing programmatically


## 🚨 MANDATORY: Starting Work on a Task

**CRITICAL**: When the user says "start work on beads issue...", "start work on the next task...", or similar, you MUST follow this workflow exactly. No exceptions.

### Workflow Pattern and Rules
1. **Start**: Run `br ready` to find actionable work
  When user specifies a phase or track (e.g., "Phase 4 of UI refactor"):
    - Run `br ready` to see unblocked tasks
    - Filter to tasks matching the specified phase/track
    - Pick the most important/foundational one (lower ID typically = more foundational)
    - If unclear, ask user to clarify which task

  **ONE EPIC AT A TIME** - Pick the single most important unblocked task related to the epic the user specifies. Work through the entire epic then commit changes at the end of the epic. **DO NOT COMMIT CHANGES UNTIL THE END OF THE EPIC**

2. **Claim**: Use `br update <id> --status in_progress`
3. **Work**: Implement the task
4. **Build and Test**
  build the app (see 'Building and Installing' workflow below) and ensure tests from the tasks/epics pass AND that the app builds and the user can test the application themselves (notify the user if this is not possible yet due to other dependencies and provide a timeline as to when they can expect to be able to test the new feature in the app themselves. It's ok if user can't test every single task, but the app MUST always build and run before closing the task) **EPICS CANNOT BE MARKED COMPLETE UNLESS ALL TESTS PASS AND THE APP BUILDS AND RUNS CORECTLY**
5. **TASK COMPLETE = BUILD PASSES** - The task is NOT complete until:
   - All code changes are made
   - `cargo build --release` passes with no unexpected errors
   - Any new warnings are addressed

6. **BUILD AND INSTALL** - After successful build:
   ```bash
   ./scripts/bundle-macos.sh && cp -R target/release/Mountaineer.app ~/Applications/
   ```
7. **Complete**: Use `br close <id> --reason "Done"`
8. **REFLECT** - Run the `/reflect` command to capture learnings

9. **COMMIT** - Follow the "Landing the Plane" workflow below
10. **Sync**: Always run `br sync` at session end
11. **NOTIFY USER** - Tell the user:
   - The app is ready to test at `~/Applications/Mountaineer.app`
   - What was completed
   - Recommend the next task to work on

### Building and Installing (Testing and Verifying your work)

#### When user says "build the app"
Run the bundle script and install to ~/Applications:
```bash
./scripts/bundle-macos.sh && cp -R target/release/Mountaineer.app ~/Applications/
```

#### Build App Bundle with Icon
```bash
./scripts/bundle-macos.sh
```
This builds a release binary and creates `target/release/Mountaineer.app` with the app icon.

#### Install Location
Install working versions to: **`~/Applications/Mountaineer.app`**

```bash
# Just copy - this overwrites the existing app
cp -R target/release/Mountaineer.app ~/Applications/
```

**Important:** Don't try to `rm -rf` the existing app first - it will fail with "Permission denied" due to macOS extended attributes. Just use `cp -R` directly, which overwrites the existing app without issues.

**Note:** User is NOT in the admin group, so `/Applications/` is not writable. Use `~/Applications/` instead.

### Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until app build passes, tests pass, and work is commited to the main branch.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session



## 🚨 MANDATORY: Creating New Tasks

**CRITICAL**: When creating beads issues, you MUST follow these rules exactly. Poorly scoped tasks waste context and block progress.

### Rules

1. **SMALL EPICS, TINY TASKS** - Break down epics into self-contained units of work that can be completed in one context window. Treat each epic as one feature, not a whole phase or collection of features.

2. **PREFIX WITH PHASE/FEATURE** - Always prepend the phase or feature name:
   - `[UI Refactor] Migrate sidebar to design tokens`
   - `[Performance] Add index on users.email column`
   - `[Multi-DB] Handle database close during active query`

3. **ATOMIC AND SELF-CONTAINED** - Each task must:
   - Have ONE clear deliverable
   - Include all context needed to execute
   - Not depend on reading a planning doc or other issues
   - Be completable without asking clarifying questions

4. **ONE CONTEXT WINDOW** - Epics must be small enough to complete in a single session:
   - If a epic might take multiple sessions, split it
   - If you're unsure, err on the side of smaller epics and tasks
   - Large tasks or epics run the risk of being abandoned mid-way and lose context

5. **AGENT-READY DESCRIPTIONS** - Write descriptions so a fresh agent can execute immediately:
   ```
   BAD:  "Continue the refactor discussed earlier"
   GOOD: "Migrate src/features/sidebar/ to use design tokens from src/design/.
          Replace all rgb(0x...) calls with token functions (bg_primary,
          text_secondary, etc). Verify with cargo build --release."
   ```

### Epic Description Template

```
**Goal:** [One sentence - what feature this Epic accomplishes and implements]

**Files:** [List specific files to modify]

**Steps:**
1. [Concrete step]
2. [Concrete step]
3. [Concrete step]

**Done when:** [Explicit completion criteria]
```
### Task Description Template

```
**Goal:** [One sentence - what this task accomplishes]

**Files:** [List specific files to modify]

**Steps:**
1. [Concrete step]
2. [Concrete step]
3. [Concrete step]

**Done when:** [Explicit completion criteria]
```

---

## Quickstart: Error Learnings

When you encounter issues, check `progress.txt` first—it contains documented solutions to problems already solved in this codebase.

---
# GPUI Learnings and Patterns

**note if you are having problems you cannot solve** – see this file from another project for deeper gotchas, problems, and reusabls patterns if necessary. do not load it unless you are really stuck of the user asks you to: `/Users/I852000/projects/airsql/CLAUDE.md`

## Key Dependencies
- `gpui` - Zed's UI framework (git dependency from zed-industries/zed)
- `gpui-component` - UI component library (local path or git from longbridge/gpui-component)

**See also:** [GPUI-DESIGN.md](./GPUI-DESIGN.md) for the GPUI mental model and research playbook

## Reference Repositories
- GPUI components: `/Users/I852000/ai/refs/zed_gpui/gpui-component-main`
- Zed app patterns: `/Users/I852000/ai/refs/zed_gpui/zed-main`
- Project scaffolding: `/Users/I852000/ai/refs/zed_gpui/create-gpui-app-main`
- **GPUI Design Guide:** [GPUI-DESIGN.md](./GPUI-DESIGN.md) — mental model, research playbook, styling reference

## GPUI Theme System

### Initializing and Customizing Themes
```rust
// Must initialize theme BEFORE using any gpui-component widgets
gpui_component::theme::init(cx);

// Change theme mode (Light/Dark)
gpui_component::theme::Theme::change(ThemeMode::Dark, None, cx);

// Customize colors AFTER changing theme mode
let theme = gpui_component::theme::Theme::global_mut(cx);
theme.background = rgb(0x2E3235).into();  // rgb() returns Rgba, .into() converts to Hsla
```

### Table-Specific Theme Colors
| Property | Purpose |
|----------|---------|
| `table` | Main table background |
| `table_head` | Header row background |
| `table_head_foreground` | Header text color |
| `table_even` | Alternating row stripe color |
| `table_hover` | Row hover background |
| `table_row_border` | Border between rows |
| `table_active` | Selected cell/row background (alpha auto-capped at 0.2) |
| `table_active_border` | Selected cell/row border |

### Popover/Menu Theme Colors
| Property | Purpose |
|----------|---------|
| `popover` | Context menu background |
| `popover_foreground` | Context menu text color |
| `shadow` | Enable/disable shadows globally (bool) |

**Note:** The `shadow` property controls shadows on popovers, inputs, buttons etc. Set `theme.shadow = false` for a flat look. This required a modification to the local gpui-component dependency (`styled.rs:popover_style()`) to make it conditional.

**Menu Item Hover Styling:** To get full-width edge-to-edge hover highlights (like sidebar items), modify `menu_item.rs:RenderOnce::render()` to:
- Remove `.px()` and `.rounded()` from outer element
- Add `.w_full()` to outer element
- Wrap children in inner `h_flex().py_1().px_2()` for content padding
- Use hardcoded color (e.g., `rgb(0x4A4E51)`) instead of `cx.theme().accent` for consistent styling

### Color Conversion
```rust
use gpui::{rgb, Hsla};

// Convert hex to Hsla for theme colors
let color: Hsla = rgb(0xE9C062).into();

// Apply alpha/transparency
let transparent_gold = color.alpha(0.25);
```

## Action Dispatching

### IMPORTANT: Correct way to dispatch actions in click handlers
```rust
// WRONG - does not work
.on_click(|_event, _window, cx| {
    cx.dispatch_action(&MyAction);  // This doesn't trigger global handlers!
})

// CORRECT - use window.dispatch_action with Box::new
.on_click(|_event, window, cx| {
    window.dispatch_action(Box::new(MyAction), cx);
})
```

### Registering Global Action Handlers
```rust
cx.on_action(move |_: &MyAction, cx: &mut App| {
    // Handle action globally
});
```

## Async Spawn Pattern

### IMPORTANT: Correct async closure syntax for App::spawn
```rust
// WRONG - lifetime error with borrowed AsyncApp
cx.spawn(|cx: &mut AsyncApp| async move {
    cx.background_executor().timer(...).await;
})

// CORRECT - use async closure syntax (Rust 1.85+)
cx.spawn(async move |cx: &mut AsyncApp| {
    cx.background_executor().timer(...).await;
})
```

The spawn signature is `AsyncFnOnce(&mut AsyncApp) -> R`. Use `async move |cx: &mut AsyncApp| { ... }` (async closures), NOT `|cx| async move { ... }` (closure returning future).

## TableDelegate Pattern

### Customizing Table Appearance
Override these methods in your `TableDelegate` implementation:

```rust
impl TableDelegate for MyDelegate {
    // Required: define columns
    fn column(&self, col_ix: usize, _cx: &App) -> Column { ... }

    // Required: render data cells
    fn render_td(&mut self, row_ix: usize, col_ix: usize, ...) -> impl IntoElement { ... }

    // Optional: customize header cells (bold, colors, etc.)
    fn render_th(&mut self, col_ix: usize, ...) -> impl IntoElement {
        div()
            .size_full()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(0xDAE3E8))
            .child(self.column(col_ix, cx).name.clone())
    }

    // Optional: implement sorting
    fn perform_sort(&mut self, col_ix: usize, sort: ColumnSort, ...) { ... }
}
```
## Dark Theme Color Palette (NotePlan-inspired)
```
Background:      #2E3235 (dark charcoal)
Alt Background:  #353A3D (sidebar, stripes)
Text:            #DAE3E8 (light gray)
Muted Text:      #c5c5c0 (secondary)
Accent Gold:     #E9C062 (selection)
Accent Teal:     #73B3C0 (buttons, links)
Border:          #464A4D (subtle borders)
```
