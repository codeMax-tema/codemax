# S7 Automatic Validation And Repair

## Scope

S7 covers validation command detection, validation failure feedback, automatic repair rounds, and manual-intervention handoff after the configured repair limit.

## Mainstream Language Detection

The detector lives in `agent/app/validation/detector.py`. It only inspects lightweight project files and existing build metadata. It does not install dependencies, copy repositories, or scan large generated directories.

Detected ecosystems:

- JavaScript/TypeScript: npm, pnpm, yarn, bun, Deno
- Python: pytest, ruff, mypy, uv
- Rust, Go
- Java, Kotlin, Scala: Maven, Gradle, sbt
- C#/.NET
- PHP, Ruby, Swift, Dart/Flutter
- C/C++: existing CTest build, Make, Meson build
- Elixir, Erlang, Haskell, Clojure, OCaml, Lua, R, Perl, Zig, Nim, Julia

## Runtime Behavior

- Explicit `validationCommand` from the user always wins.
- Without an explicit command, the Agent detects the worktree first, then the repository path.
- Detected commands are stored as `validationCandidates` so the UI and proof pack can explain the source.
- The desktop command `run_agent_validation_cycle` advances the Agent, executes each `validationRequest` through the Rust command runner, stores stdout/stderr on disk, submits the log tail back to the Agent, and continues until validation passes, pauses, or reaches the repair limit.
- Failed validation creates a `repairPlan`, writes `.codemax/agent-repair-round-N.md`, clears the previous result, and requests validation again.
- Repair can apply structured, worktree-scoped directives emitted in validation logs:
  `CODEMAX_REPAIR {"path":"relative/file.py","find":"old text","replace":"new text"}`.
  Paths must be relative, stay inside the task worktree, and match existing file content before any write occurs.
- `CODEMAX_MAX_REPAIR_ROUNDS` defaults to `5`; after the limit, the Agent enters `needs_intervention`.

## Environment

- Rust storage now honors `CODEMAX_APP_DATA_DIR`, `CODEMAX_WORKTREE_ROOT`, `CODEMAX_ARTIFACT_ROOT`, and `CODEMAX_DATABASE_URL`.
- The Python Agent process receives app-data defaults for checkpoint and memory paths when the user has not configured them.
- The Tauri dialog plugin permission is enabled in the default capability so repository folder selection can run after plugin installation.
