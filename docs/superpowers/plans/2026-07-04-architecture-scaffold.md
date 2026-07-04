# Architecture Scaffold Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the initial architecture scaffold with desktop frontend and Rust/Tauri backend together under `apps/desktop`, plus Agent, database, config, contracts, and architecture docs.

**Architecture:** The desktop package owns React UI and Rust local backend. The Python Agent stays as a separate local service. Shared behavior is documented through contracts, migration files, and runtime boundary docs.

**Tech Stack:** React 18, TypeScript, Vite, Tauri v2, Rust, SQLite, Python 3.11, FastAPI, LangGraph.

---

### Task 1: Architecture Contract

**Files:**
- Create: `tests/architecture/verify-architecture.mjs`
- Modify: `package.json`

- [x] **Step 1: Write the failing architecture contract**

Run: `node tests/architecture/verify-architecture.mjs`

Expected before scaffold: exit code 1 with missing architecture files.

- [x] **Step 2: Add root check script**

Add `check:architecture` to root `package.json`.

### Task 2: Desktop Package

**Files:**
- Create: `apps/desktop/package.json`
- Create: `apps/desktop/index.html`
- Create: `apps/desktop/src/**`
- Create: `apps/desktop/src-tauri/**`

- [x] **Step 1: Add React/Vite frontend skeleton**

Create `src/main.tsx`, app provider, i18n, store, API client, domain types, and feature folders.

- [x] **Step 2: Add Rust/Tauri backend skeleton**

Create `src-tauri/Cargo.toml`, `tauri.conf.json`, health command, and module boundaries for storage, Git, command execution, safety, and Agent service management.

### Task 3: Supporting Architecture

**Files:**
- Create: `agent/app/**`
- Create: `database/migrations/0001_initial.sql`
- Create: `contracts/ipc.schema.json`
- Create: `config/*.json`
- Create: `docs/architecture/*.md`

- [x] **Step 1: Add Python Agent skeleton**

Create FastAPI app factory, health router, settings, graph state, graph nodes, memory service, and OpenAI-compatible provider adapter.

- [x] **Step 2: Add database and contract scaffold**

Create initial SQLite schema and IPC contract.

- [x] **Step 3: Add runtime docs**

Document module ownership, call direction, Worktree boundary, and storage boundary.

### Task 4: Verification

**Files:**
- Read: `tests/architecture/verify-architecture.mjs`

- [x] **Step 1: Run architecture contract**

Run: `npm run check:architecture`

Expected after scaffold: exit code 0.

