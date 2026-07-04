# S1 Desktop Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete S1 by turning the existing scaffold into a buildable Tauri v2 desktop foundation with React, Tailwind, shadcn-style primitives, IPC, event push, frontend API wrappers, and unified error notifications.

**Architecture:** `apps/desktop/src` owns the React UI shell and frontend API layer. `apps/desktop/src-tauri` owns Rust commands, event emission, and local backend boundaries. The S1 scope stops at shell, IPC, and basic UI primitives; database and task behavior remain for S2 and later.

**Tech Stack:** Tauri v2, Rust, React 18, TypeScript, Vite, Tailwind CSS, Radix UI primitives, Zustand, TanStack Query.

---

### Task 1: S1 Contract

**Files:**
- Modify: `tests/architecture/verify-architecture.mjs`

- [x] **Step 1: Add S1 contract requirements**

Require UI primitives, IPC event layer, Rust event module, and ping command.

- [x] **Step 2: Run contract before implementation**

Run: `npm run check:architecture`

Expected before implementation: fails with missing S1 files and content.

### Task 2: Frontend Foundation

**Files:**
- Modify: `apps/desktop/package.json`
- Modify: `apps/desktop/vite.config.ts`
- Modify: `apps/desktop/tailwind.config.ts`
- Modify: `apps/desktop/src/styles/global.css`
- Create: `apps/desktop/src/lib/utils.ts`
- Create: `apps/desktop/src/components/ui/button.tsx`
- Create: `apps/desktop/src/components/ui/dialog.tsx`
- Create: `apps/desktop/src/components/ui/tabs.tsx`
- Create: `apps/desktop/src/components/ui/table.tsx`
- Create: `apps/desktop/src/components/ui/toast.tsx`
- Create: `apps/desktop/src/components/ui/toaster.tsx`
- Create: `apps/desktop/src/state/notificationStore.ts`
- Create: `apps/desktop/src/api/errors.ts`
- Create: `apps/desktop/src/api/events.ts`
- Modify: `apps/desktop/src/api/tauriClient.ts`
- Modify: `apps/desktop/src/app/providers.tsx`
- Modify: `apps/desktop/src/app/App.tsx`

- [x] **Step 1: Add dependencies and alias support**

Add Radix primitives, class variance helpers, and Tailwind merge support. Configure Vite alias for `@`.

- [x] **Step 2: Add shadcn-style primitives**

Add Button, Dialog, Tabs, Table, Toast, and Toaster primitives.

- [x] **Step 3: Add frontend IPC and event wrappers**

Add `pingDesktop`, `emitAppReady`, normalized IPC errors, and `listenAppReady`.

### Task 3: Rust/Tauri Foundation

**Files:**
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/src/commands/app.rs`
- Create: `apps/desktop/src-tauri/src/events.rs`
- Create: `apps/desktop/src-tauri/icons/icon.ico`

- [x] **Step 1: Add test command**

Expose `ping` alongside `health`.

- [x] **Step 2: Add frontend event push**

Expose `emit_app_ready` and emit `codemax://app-ready`.

- [x] **Step 3: Add Windows icon placeholder**

Add a minimal `icon.ico` so Tauri Windows resource generation can run.

### Task 4: Verification

**Files:**
- Read: `package.json`
- Read: `apps/desktop/package.json`
- Read: `apps/desktop/src-tauri/Cargo.toml`

- [x] **Step 1: Install dependencies**

Run: `npm install`

Expected: dependencies installed and lockfile generated.

- [x] **Step 2: Run architecture contract**

Run: `npm run check`

Expected: `Architecture contract passed with 49 required files.`

- [x] **Step 3: Build frontend**

Run: `npm run build:desktop`

Expected: TypeScript and Vite build pass.

- [x] **Step 4: Check Rust backend**

Run: `npm run check:tauri`

Expected: Cargo check passes.

- [x] **Step 5: Build Tauri shell without bundling**

Run: `npm run build:tauri:debug`

Expected: debug executable builds without creating an installer bundle.

