# CodeMax Built-in Agent System Prompt Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite the CodeMax built-in system prompt so the product behaves like a Codex-style autonomous coding agent while retaining CodeMax privacy, control, memory, and auditable-delivery advantages.

**Architecture:** Keep one stable product-level base prompt in `codemax-programming-assistant.md`. The prompt defines autonomous tool use and engineering behavior, while dynamic task context and enforceable safety controls remain Runtime responsibilities.

**Tech Stack:** Markdown, CodeMax Agent Runtime contracts, task Worktrees, structured tool calls.

---

### Task 1: Replace the product-level system prompt

**Files:**
- Modify: `codemax-programming-assistant.md`
- Reference: `最终计划.md`
- Reference: `docs/architecture/runtime-boundaries.md`
- Reference: `docs/superpowers/specs/2026-07-12-codemax-built-in-agent-system-prompt-design.md`

- [ ] **Step 1: Replace the current mixed prompt**

Rewrite the file around these stable sections:

```text
Document Purpose
System Prompt
Identity and Mission
Instruction Priority
Operating Contract
Core Engineering Principles
Autonomous Tool Loop
Task Modes
Repository and Worktree Safety
Editing and Validation Discipline
Failure Recovery
Privacy, Memory, and User Control
Auditability and Delivery
Multi-Task Coordination
Communication
Dynamic Runtime Context
Runtime Enforcement Boundary
Integration Notes
```

The prompt must explicitly state that CodeMax can repeatedly invoke Runtime-provided tools to read, search, edit, execute, validate, and repair. It must not require every model response to be a one-shot `EditingPlan`.

- [ ] **Step 2: Preserve CodeMax-specific product behavior**

Include concise behavioral rules for:

```text
Privacy Ledger
Run Contract
Memory Cockpit
Preference Distiller
Personal Profile
Token and Context Budget
Quality Gate
Delivery Score
Proof Pack
Task Capsule
```

The model should cooperate with these systems, while the Runtime remains responsible for enforcement and persistence.

- [ ] **Step 3: Remove conflicting or brittle rules**

Remove:

```text
fixed drive-letter examples
fixed five-request browsing limit
unobservable remaining-token percentages
permission to skip required validation when budget is low
absolute whole-repository quality requirements for every narrow task
agent-owned merge ordering
automatic permanent retention of user corrections
large language/framework capability lists
repetitive behavioral examples
```

### Task 2: Validate the rewritten prompt

**Files:**
- Test: `codemax-programming-assistant.md`

- [ ] **Step 1: Scan for forbidden legacy wording**

Run:

```powershell
rg -n "You do NOT execute commands|valid JSON only|No more than 5 internet|remaining < 20%|Skip non-critical validation|E:\\codemax|D:\\codemax" codemax-programming-assistant.md
```

Expected: no matches.

- [ ] **Step 2: Confirm required autonomous-agent concepts**

Run:

```powershell
rg -n "tool|worktree|validation|repair|Privacy Ledger|Run Contract|Proof Pack|Quality Gate|Runtime" codemax-programming-assistant.md
```

Expected: every required concept is present in an applicable behavioral section.

- [ ] **Step 3: Check document size and structure**

Run:

```powershell
$text = Get-Content -LiteralPath .\codemax-programming-assistant.md -Raw -Encoding UTF8
[pscustomobject]@{
  Characters = $text.Length
  Lines = ($text -split "`n").Count
  HasSystemPrompt = $text.Contains("# System Prompt")
  HasToolLoop = $text.Contains("Autonomous Tool Loop")
  HasRuntimeBoundary = $text.Contains("Runtime Enforcement Boundary")
}
```

Expected:

```text
Characters: less than the original 31,095-byte document
HasSystemPrompt: True
HasToolLoop: True
HasRuntimeBoundary: True
```

- [ ] **Step 4: Review the final diff**

Run:

```powershell
git diff -- codemax-programming-assistant.md
```

Expected: one focused rewrite with no unrelated repository modifications.

