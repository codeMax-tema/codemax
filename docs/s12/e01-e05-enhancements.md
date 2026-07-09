# S12-E01 to S12-E05 Enhancements

## Coverage

This stage adds the first enhanced capabilities after the S11 MVP loop:

- Multi-task scheduling with bounded concurrency.
- Mainstream-language parser summaries and bounded context retrieval.
- Screenshot artifact metadata for local UI validation.
- Proposal generation, selection, and regeneration models.
- Proof Pack, quality gates, delivery score, risk radar, and task capsule artifacts.

## Storage Boundaries

SQLite stores structured indexes, scores, gate rows, and paths. Large files stay on disk:

- screenshots: `app-data/tasks/<task-id>/screenshots/`
- proof manifests and task capsules: `app-data/tasks/<task-id>/artifacts/proof-pack/`
- diff and report files: existing S8/S10 artifact paths

This keeps the local database small and makes storage usage transparent.

## Parser Coverage

The Agent context registry covers TypeScript, JavaScript, Python, Java, Go, Rust, C, C++, C#, PHP, Ruby, Kotlin, Swift, Dart, Scala, Shell, SQL, HTML, CSS, Vue, Svelte, YAML, TOML, JSON, and Markdown.

`CodeParserService` can use Tree-sitter when the runtime has grammars installed. If not, it returns deterministic fallback summaries for imports, functions, classes, and symbols. The fallback mode is explicit in `parserMode`, so the Agent does not pretend it had full AST precision.

## Context Retrieval

`ContextRetriever` is intentionally bounded. It skips heavy generated directories such as `node_modules`, `target`, `.git`, `.worktrees`, and `__pycache__`, ignores very large files, scores relevant files by task query and parsed symbols, and returns at most the configured `max_files`.

The acceptance rule is that Agent context retrieval must not default to reading the whole repository.

## Screenshot Validation

`ScreenshotService` records task-local screenshot artifact metadata. When Playwright is unavailable, it returns `browserUnavailable` instead of hiding the failure. This lets the UI and Proof Pack explain why a screenshot is missing without storing browser output in SQLite.

## Proposal Selector

`ProposalService` generates two to three deterministic proposal cards. Each proposal includes advantages, drawbacks, risks, impact, effort, a recommendation flag, and rationale. Regeneration incorporates user feedback into the rationale and changes proposal ids so the UI can distinguish new options.

## Proof Pack And Merge Gate

`generate_task_proof_pack` writes:

- `manifest.json`
- `summary.md`
- `task-capsule.json`

The command persists rows in `proof_packs`, `delivery_scores`, and `artifact_files`. Risk radar scans command text and changed file paths for dangerous commands, sensitive files, dependency changes, and schema changes.

Merge preparation now also checks failed quality gates from `quality_gate_results`. Failed gates with no override reason become merge blockers. Existing tasks without gate rows keep the previous S10 behavior.

## C-Line Delivery Review Closure

The 2026-07-09 C-line closure adds durable rule hits, hook runs, hook escalation approvals, and model-arena decisions. These records are stored as SQLite indexes, exposed through IPC, rendered in the delivery review page, included in Proof Pack manifests, and used by merge blockers when a rule or hook remains unresolved.
