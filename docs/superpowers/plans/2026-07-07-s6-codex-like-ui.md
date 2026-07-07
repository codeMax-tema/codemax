# S6 Codex-Like UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the S6 desktop UI shell with a Codex-like task setup dialog and a dedicated settings page for models, modes, permissions, storage, memory, appearance, and language.

**Architecture:** Keep UI state in the existing Zustand store, use route ids instead of adding a router dependency, and keep page components inside their feature folders. The run setup dialog owns draft task configuration locally because this S6 slice is a UI contract, not a persisted backend flow yet.

**Tech Stack:** React 18, TypeScript, Vite, Zustand, Radix Dialog, lucide-react, Tailwind/global CSS, static Node verification script.

---

### Task 1: Frontend Acceptance Contract

**Files:**
- Create: `tests/frontend/verify-s6-ui.mjs`
- Modify: `package.json`

- [ ] Add a static verification script that checks for Codex-like dialog controls, dedicated settings layout, and bilingual i18n keys.
- [ ] Add the script to `npm run check`.
- [ ] Run `npm run check` and confirm the new script fails before UI implementation.

### Task 2: Shell And State

**Files:**
- Modify: `apps/desktop/src/state/appStore.ts`
- Modify: `apps/desktop/src/app/App.tsx`
- Modify: `apps/desktop/src/styles/global.css`

- [ ] Extend app state with current route, dialog open state, compact/high-contrast toggles, and selected task id.
- [ ] Replace the single repository page render with a desktop shell, sidebar navigation, and routed content.
- [ ] Add base shell styling with stable sidebar, top region, content panels, focus states, and responsive constraints.

### Task 3: Codex-Like Task Dialog

**Files:**
- Create: `apps/desktop/src/features/tasks/NewTaskDialog.tsx`
- Create: `apps/desktop/src/features/tasks/TaskOverviewPage.tsx`
- Create: `apps/desktop/src/features/tasks/taskFixtures.ts`

- [ ] Build a wide run setup dialog with a prompt composer on the left and run contract controls on the right.
- [ ] Add mode selection for AGENT, PLAN, ASK, and REVIEW.
- [ ] Add model selection, model strength, permissions, storage summary, and a link to full settings.
- [ ] Keep validation accessible with inline errors.

### Task 4: Dedicated Settings Page

**Files:**
- Create: `apps/desktop/src/features/settings/SettingsPage.tsx`
- Modify: `apps/desktop/src/i18n/locales/zh-CN.json`
- Modify: `apps/desktop/src/i18n/locales/en-US.json`

- [ ] Build a two-pane settings page with left category rail and right details.
- [ ] Cover models, permissions, modes, storage, memory, appearance, and language.
- [ ] Keep all visible strings in i18n dictionaries.

### Task 5: Verification

**Files:**
- Modify as needed from prior tasks.

- [ ] Run `npm run check`.
- [ ] Run `npm run build:desktop`.
- [ ] Start the Vite dev server and capture or inspect the UI if the environment allows.
- [ ] Report any verification gaps honestly.
