# S6 Frontend Core Pages Design

## Goal

S6 builds the first usable desktop UI shell for the Agent programming task console. It turns the existing repository selector into a workspace-first command surface with task overview, task detail, approval, settings, storage, memory, language, and appearance entry points.

The design follows the user-approved reference file `D:\codemax\workspace-first-ui-preview.html`: calm engineering-console style, high information density, restrained visual treatment, visible storage and memory transparency, and default minimal UI with future style switching.

## User Approval Baseline

- Approved direction: Workspace-first three-column dispatch console.
- Visual reference: `workspace-first-ui-preview.html`.
- Default style: minimal, restrained, dense, long-term engineering use.
- Must preserve: i18n-first copy, multi-style extension points, storage transparency, and user-controlled memory.
- Out of scope for S6: full Diff viewer implementation, full Proof Pack workflow, complete Hooks Studio, complete multi-model arena, and production-grade agent execution orchestration beyond existing API surfaces.

## Design Principles

1. Put the user's local workspace first. The selected repository and current task should be visible before secondary product features.
2. Make Agent state legible. Users should quickly see what the Agent is doing, why it is blocked, and what evidence exists.
3. Keep risky actions explicit. Pause, cancel, approval, cleanup, and merge-review actions must be visually clear and never hidden behind vague controls.
4. Keep data ownership visible. Storage paths, worktree path, retained artifacts, and memory records must be inspectable.
5. Use compact but readable UI. The product is a desktop tool for repeated work, so layout should be dense without becoming cramped.
6. Use real UI text through i18n keys. No hard-coded user-facing copy in components where locale resources can cover it.

## Information Architecture

### App Shell

The app becomes a single desktop workspace with:

- Left sidebar: repository switcher, primary navigation, status filters, task list, storage summary.
- Center workspace: route content for repository, tasks, task detail, approvals, and settings.
- Right inspector: contextual review drawer for the current task or selected settings category.

The right inspector can collapse below narrower desktop widths. The app keeps a desktop minimum width consistent with the current project baseline and the preview file.

### Routes

S6 keeps the existing route constants and maps them to usable screens:

- `/`: repository selection and repository status.
- `/tasks`: task overview with filters, stats, and new task dialog.
- `/tasks/:taskId`: task detail workspace with Todo, logs, changes, reports, delivery summary, feedback input, memory summary.
- `/approvals`: approval queue and high-risk action review.
- `/settings`: settings hub for model, commands, safety, storage, memory, language, and UI style.

## Page Designs

### Repository Page

Purpose: let users choose a local Git repository and understand its safety state.

Main areas:

- Repository picker button with loading and error states.
- Repository summary: name, path, branch, dirty state.
- Recent repositories section with an empty state in S6 if no persisted recent-repository API is available yet.
- Storage location hint showing where app data and worktrees live.

Empty state should be practical: select a repository first, then create tasks. No marketing copy.

### Task Overview Page

Purpose: let users monitor all task work at a glance.

Main areas:

- Status strip: running, waiting approval, failed, completed counts.
- Filter segmented control: all, running, waiting approval, needs human input, completed, merged, failed, canceled.
- Task table/list: title, status, repository, created time, updated time, risk or gate status.
- New task button opens a dialog.

The task list follows the preview's thread list pattern: compact rows, status dot, title, secondary metadata, and a small status pill.

### New Task Dialog

Purpose: create a natural-language task without hiding important execution constraints.

Fields:

- Task description textarea.
- Task type segmented/list control: bug fix, tests, refactor, explain code, custom.
- Validation command section with editable command rows; if command detection is not available yet, show the default configured commands and mark them as user-editable.
- Model selector tied to settings/model configs; if no provider exists yet, show a disabled model row with a direct settings link.
- Run contract summary: repository, worktree isolation, approval policy, max repair rounds.

Submit behavior:

- Validate that a repository is selected and description is not empty.
- After creation, route to task detail.
- For S6, use existing API boundaries where available. If a later-stage backend read is unavailable, keep demo data in feature-level fixtures with names that make replacement obvious, such as `taskFixtures.ts`.

### Task Detail Page

Purpose: show the actual Agent work chain and evidence.

Center content:

- Header: title, status, repository, branch/worktree, model/round.
- Checkpoint strip: worktree, validation commands, risk status, memory use.
- Todo list: status, title, description, start/end time if available.
- Live log panel: command, cwd, stdout/stderr snippets, exit state.
- Changed files list: added/modified/deleted status, path, summary.
- Test report section: command, exit code, duration, pass/fail summary.
- Delivery summary section: Agent summary and commit message suggestion.
- Feedback composer: user can provide follow-up feedback.

Right inspector:

- Delivery score panel; if S8 scoring data is unavailable, show `Not scored yet` with the reason.
- Quality gate status.
- Approval status.
- Memory used by this task.
- Storage usage and artifact path.

High-risk or blocked states should be visible in both the center header and inspector.

### Approvals Page

Purpose: review high-risk actions before execution.

Main areas:

- Approval queue list: command/action, task, risk level, requested time.
- Detail panel: command, cwd, reason, impact scope, affected paths, Agent rationale.
- Actions: allow, reject, request changes.

If backend approvals are not fully wired yet, S6 should still render the queue, details, and empty states. Action buttons must be disabled with clear explanatory text instead of pretending that approval was completed.

### Settings Page

Purpose: expose configuration without turning S6 into a full admin system.

Settings sections:

- Model providers: provider, base URL, model name, and a test-connection control. If the backend test action is unavailable, keep the control disabled with a clear message.
- Validation commands: default test/lint/build commands.
- Safety: high-risk operation policy and max repair rounds.
- Storage: app data path, logs retention, screenshots retention, worktree cleanup policy.
- Memory: long-term memory list with source/scope/update time and edit/delete controls.
- Language and appearance: UI language, Agent output language, proof pack language, commit message language, UI style, compact mode, high contrast.

The language and appearance section must include a visible note that new UI styles require user confirmation before landing.

## Visual System

### Layout

- App container: desktop console shell, bounded by stable min width.
- Left sidebar: about 292-308 px.
- Right inspector: about 292-312 px.
- Center column: fluid, scrollable content.
- Border radius: 7-8 px, matching the preview.
- Avoid nested cards. Use cards only for repeated items, modals, and framed tools such as terminal/log/diff previews.

### Color

Use the preview's neutral-first palette as the guide:

- Background: soft gray surface.
- Surface: white or near-white.
- Text: near-black.
- Primary accent: near-black for primary actions, blue only for links/progress/active technical states.
- Status: green success, amber waiting, red destructive, gray completed.

Dark mode support should be token-driven and parallel to light mode, but S6 can start with functional light mode plus stored theme selection if full dark token polish is too large.

### Typography

- System sans stack, preserving Chinese readability.
- Compact desktop scale: 11-16 px for most operational UI.
- No viewport-scaled font sizes.
- Letter spacing remains 0 except small uppercase labels already present in the codebase.

### Controls

- Use lucide icons for navigation and icon buttons.
- Use shadcn-style Button, Dialog, Tabs/Table where appropriate.
- Use segmented controls for filters and task type.
- Use toggles/checkboxes for binary settings.
- Use explicit buttons for destructive or approval decisions.

## State Management

Extend Zustand state conservatively:

- current repository
- current route or selected task id if needed
- task status filter
- locale
- theme/style name
- compact mode
- inspector collapsed state

Use TanStack Query for task list/detail and settings reads when real async IPC/API calls exist. Keep temporary demo data isolated in feature-level fixtures so it can be replaced by real queries.

## Internationalization

S6 must expand `zh-CN` and `en-US` locale resources for all new screens.

Rules:

- Components call `t(key, locale)`.
- Use namespaces in key names, for example `tasks.overview.title`, `settings.storage.title`.
- Missing resources must not crash the UI.
- No visible user-facing strings should be left hard-coded in new route components.

## Testing And Verification

Implementation should use TDD where behavior changes are testable.

Minimum verification:

- Frontend build passes.
- Existing architecture check passes.
- Locale JSON parses.
- No user token appears in new files.
- If component tests are available or easy to add, cover filter state, task dialog validation, locale switching, and settings state updates.
- Visual verification with the desktop dev server should compare the implemented shell against `workspace-first-ui-preview.html`.

## Requirement Coverage

- S6-T01 to S6-T04: app shell, route mapping, Zustand UI state, and TanStack Query readiness.
- S6-T05 to S6-T07: repository selection, repository summary, and recent-repository section.
- S6-T08 to S6-T11: task overview list, status filter, statistics, and new task entry.
- S6-T12 to S6-T16: new task dialog, task type, validation command configuration, model selection, and submit flow.
- S6-T17 to S6-T26: task detail header, Todo list, logs, file changes, test report, delivery summary, task controls, feedback, memory summary, and memory list.
- S6-T27 to S6-T32: settings hub, retention settings, worktree cleanup policy, memory management, and task cleanup entry.

## Implementation Defaults And Boundaries

The current backend may not expose every later-stage API. S6 implementation should use these defaults:

- Use real repository IPC calls already present for repository selection and validation.
- Use real task or agent APIs only where they already exist and are stable.
- For task overview/detail data that is not yet readable through IPC, use clearly named feature fixtures, not inline component literals.
- For task creation, validate the form and route to a created draft view if the full Agent launch is unavailable; make the draft state visible to the user.
- For approval actions before S9, show the review UI and disable mutation buttons with an explanatory message.
- For cleanup before S2/S3 cleanup actions are exposed to the frontend, show the storage policy and disable destructive cleanup buttons.

The UI should still be structured around the real S6 requirements so later backend wiring does not require a page rewrite.
