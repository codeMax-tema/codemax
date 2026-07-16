use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use crate::{
    commands::{
        delivery::{generate_task_delivery_inner, GenerateTaskDeliveryRequest},
        diff::{generate_task_diff_inner, GenerateTaskDiffRequest},
        merge::{
            merge_task_inner, prepare_task_merge_inner, MergeTaskRequest, PrepareTaskMergeRequest,
        },
        s12_evidence::{generate_task_proof_pack_inner, GenerateTaskProofPackRequest},
    },
    exec::{
        CommandExecutor, CommandLogPaths, CommandOutputSink, CommandRequest, CommandRunRegistry,
    },
    git::{self, TaskMergeStatus},
    storage::{
        ApprovalRepository, ArtifactRepository, CleanupGuard, CommandRunRepository,
        MemoryRepository, NewApproval, NewArtifactFile, NewCommandRun, NewConversation,
        NewMemoryItem, NewMessage, NewRunContract, NewTask, RunContractRepository, SqliteStore,
        StorageRoots, TaskRepository,
    },
};

const TASK_ID: &str = "task-s11-e2e";

#[cfg(windows)]
fn validation_command() -> String {
    "powershell -NoProfile -ExecutionPolicy Bypass -File validate.ps1".to_string()
}

#[cfg(not(windows))]
fn validation_command() -> String {
    "sh validate.sh".to_string()
}

#[tokio::test]
async fn s11_mvp_demo_repo_runs_from_worktree_to_local_merge() {
    let repository = demo_repository("mvp-flow");
    let target_branch = git::current_branch(&repository).expect("read target branch");
    let storage_root = temp_path("storage");
    let storage = acceptance_storage(&storage_root);

    create_task_record(&storage, &repository);
    let worktree = git::create_task_worktree(&repository, &storage.roots.worktree_root, TASK_ID)
        .expect("create task worktree");
    persist_worktree(&storage, &worktree.worktree_path, &worktree.branch_name);
    persist_run_contract(&storage, &repository);

    let failing =
        run_validation(&storage, TASK_ID, &worktree.worktree_path, "run-s11-failed").await;
    assert_eq!(failing.status, "failed");
    assert_eq!(failing.exit_code, Some(1));

    let feature_path = PathBuf::from(&worktree.worktree_path)
        .join("src")
        .join("feature.py");
    fs::write(&feature_path, "def enabled():\n    return True\n")
        .expect("apply deterministic Agent repair");

    let passing =
        run_validation(&storage, TASK_ID, &worktree.worktree_path, "run-s11-passed").await;
    assert_eq!(passing.status, "passed");
    assert_eq!(passing.exit_code, Some(0));

    let diff = generate_task_diff_inner(
        &storage,
        GenerateTaskDiffRequest {
            task_id: TASK_ID.to_string(),
            base_ref: Some(target_branch.clone()),
        },
    )
    .expect("generate final diff");
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, "src/feature.py");
    assert!(PathBuf::from(&diff.diff_path).is_file());

    let delivery = generate_task_delivery_inner(
        &storage,
        GenerateTaskDeliveryRequest {
            task_id: TASK_ID.to_string(),
        },
    )
    .expect("generate delivery report");
    assert_eq!(delivery.report.overall_status, "passed");
    assert_eq!(delivery.report.command_count, 1);
    assert_eq!(delivery.report.passed_count, 1);
    assert_eq!(delivery.report.failed_count, 0);
    assert!(PathBuf::from(&delivery.report_path).is_file());
    assert!(PathBuf::from(&delivery.delivery_path).is_file());

    let proof_pack = generate_task_proof_pack_inner(
        &storage,
        GenerateTaskProofPackRequest {
            task_id: TASK_ID.to_string(),
        },
    )
    .expect("generate proof pack before merge");
    assert!(PathBuf::from(&proof_pack.proof_dir).is_dir());

    let prepared = prepare_task_merge_inner(
        &storage,
        PrepareTaskMergeRequest {
            task_id: TASK_ID.to_string(),
            target_branch: Some(target_branch.clone()),
        },
    )
    .expect("prepare merge");
    assert!(prepared.can_merge, "{:?}", prepared.blockers);

    let merge = merge_task_inner(
        &storage,
        MergeTaskRequest {
            task_id: TASK_ID.to_string(),
            target_branch: Some(target_branch.clone()),
            commit_message: "fix: enable demo feature".to_string(),
            preview_id: prepared.preview_id.clone(),
            confirmed: true,
        },
    )
    .expect("merge task");
    assert_eq!(merge.status, TaskMergeStatus::Merged);
    assert_eq!(merge.task_status, "merged");
    assert!(merge
        .merge_record_path
        .as_deref()
        .is_some_and(|path| Path::new(path).is_file()));
    assert!(
        fs::read_to_string(repository.join("src").join("feature.py"))
            .expect("read merged feature")
            .contains("return True")
    );

    record_temporary_context_file(&storage);
    let removed = {
        let store = storage.store.lock().expect("storage lock");
        CleanupGuard::new(store.connection())
            .remove_temporary_artifact_file_records(TASK_ID)
            .expect("cleanup temporary artifacts")
    };
    assert_eq!(removed, 1);
    let permanent_files = {
        let store = storage.store.lock().expect("storage lock");
        ArtifactRepository::new(store.connection())
            .files_for_task(TASK_ID)
            .expect("list artifact files")
    };
    assert!(permanent_files.iter().any(|file| file.file_type == "diff"));
    assert!(permanent_files
        .iter()
        .any(|file| file.file_type == "merge_record"));

    cleanup_paths([
        PathBuf::from(&worktree.worktree_path),
        storage_root,
        repository,
    ]);
}

#[test]
fn s11_acceptance_covers_repository_approval_and_memory_edges() {
    let non_git = temp_path("non-git");
    fs::create_dir_all(&non_git).expect("create non git dir");
    fs::write(non_git.join(".git"), "not-a-gitdir").expect("prevent parent repo discovery");
    assert!(git::validate_repository(&non_git).is_err());

    let repository = demo_repository("edges");
    assert!(git::validate_repository(&repository)
        .expect("validate demo repo")
        .path
        .contains("codemax-s11-edges"));

    let storage_root = temp_path("edge-storage");
    let storage = acceptance_storage(&storage_root);
    create_task_record(&storage, &repository);

    {
        let store = storage.store.lock().expect("storage lock");
        let approvals = ApprovalRepository::new(store.connection());
        for (id, decision) in [
            ("approval-s11-approved", "approved"),
            ("approval-s11-rejected", "rejected"),
            ("approval-s11-revise", "revise"),
        ] {
            approvals
                .create(NewApproval {
                    id,
                    task_id: TASK_ID,
                    approval_type: "command",
                    risk_level: "high",
                    content: "command: npm install left-pad",
                    reason: "Dependency install requires user control.",
                })
                .expect("create approval");
            approvals
                .decide(id, decision, Some("S11 acceptance decision"))
                .expect("decide approval");
        }
        let decisions = approvals
            .list_for_task(TASK_ID)
            .expect("list task approvals")
            .into_iter()
            .map(|approval| approval.decision.unwrap_or_default())
            .collect::<Vec<_>>();
        assert!(decisions.contains(&"approved".to_string()));
        assert!(decisions.contains(&"rejected".to_string()));
        assert!(decisions.contains(&"revise".to_string()));
    }

    {
        let store = storage.store.lock().expect("storage lock");
        let memory = MemoryRepository::new(store.connection());
        memory
            .create_conversation(NewConversation {
                id: "conversation-s11",
                task_id: Some(TASK_ID),
                repository_path: Some(repository.to_string_lossy().as_ref()),
                title: "S11 acceptance memory",
            })
            .expect("create conversation");
        for index in 0..55 {
            memory
                .add_message(NewMessage {
                    id: &format!("message-s11-{index:03}"),
                    conversation_id: "conversation-s11",
                    task_id: Some(TASK_ID),
                    role: "user",
                    content: &format!("visible acceptance message {index}"),
                    token_count: 4,
                    is_pinned: false,
                    retention_class: "recent",
                })
                .expect("add message");
        }
        let recent = memory
            .recent_messages("conversation-s11", 50)
            .expect("load recent messages");
        assert_eq!(recent.len(), 50);
        assert_eq!(recent[0].id, "message-s11-005");

        let saved = memory
            .upsert_memory_item(NewMemoryItem {
                id: "memory-s11",
                scope: "repository",
                scope_id: Some(repository.to_string_lossy().as_ref()),
                key: "defaultValidationCommand",
                value: &validation_command(),
                confidence: 0.9,
                source: "s11_acceptance",
                is_user_editable: true,
            })
            .expect("save memory");
        assert!(saved.is_user_editable);
        memory
            .delete_memory_item("memory-s11")
            .expect("delete memory");
        assert!(memory
            .memory_item("memory-s11")
            .expect("query deleted memory")
            .is_none());
    }

    cleanup_paths([storage_root, repository, non_git]);
}

async fn run_validation(
    storage: &crate::storage::ManagedStorage,
    task_id: &str,
    cwd: &str,
    run_id: &str,
) -> crate::exec::CommandExecutionResult {
    let paths = storage
        .roots
        .ensure_task_artifact_dirs(task_id)
        .expect("create artifact dirs");
    let result = CommandExecutor
        .run(
            CommandRequest {
                task_id: task_id.to_string(),
                run_id: Some(run_id.to_string()),
                command: validation_command(),
                cwd: cwd.to_string(),
                env: BTreeMap::new(),
                timeout_ms: Some(30_000),
                purpose: Some("validation".to_string()),
                approval_id: None,
            },
            CommandLogPaths {
                stdout_path: paths.logs_dir.join(format!("{run_id}.stdout.log")),
                stderr_path: paths.logs_dir.join(format!("{run_id}.stderr.log")),
            },
            CommandRunRegistry::default(),
            noop_output_sink(),
            Vec::new(),
        )
        .await
        .expect("run validation command");

    let store = storage.store.lock().expect("storage lock");
    CommandRunRepository::new(store.connection())
        .record(NewCommandRun {
            id: &result.run_id,
            task_id: &result.task_id,
            purpose: "validation",
            command: &result.command,
            cwd: &result.cwd,
            status: &result.status,
            stdout_path: Some(&result.stdout_path),
            stderr_path: Some(&result.stderr_path),
            exit_code: result.exit_code.map(i64::from),
            duration_ms: Some(result.duration_ms as i64),
        })
        .expect("record command run");

    result
}

fn acceptance_storage(root: &Path) -> crate::storage::ManagedStorage {
    let roots = StorageRoots::from_app_data_dir(root);
    roots.ensure_base_dirs().expect("create storage roots");
    let store = SqliteStore::open_in_memory().expect("open sqlite");
    store.migrate().expect("migrate sqlite");
    crate::storage::ManagedStorage {
        roots,
        store: Mutex::new(store),
    }
}

fn create_task_record(storage: &crate::storage::ManagedStorage, repository: &Path) {
    let target_branch = git::current_branch(repository).expect("read acceptance target branch");
    let store = storage.store.lock().expect("storage lock");
    TaskRepository::new(store.connection())
        .create(NewTask {
            id: TASK_ID,
            title: "S11 MVP acceptance",
            description: "Run the MVP flow against a deterministic demo repository.",
            task_type: "bugfix",
            status: "created",
            repository_path: repository.to_string_lossy().as_ref(),
            worktree_path: None,
            branch_name: None,
            target_branch: &target_branch,
            workspace_kind: "git_worktree",
            source_path: repository.to_string_lossy().as_ref(),
            original_write_authorized: false,
            workspace_estimated_bytes: 0,
            model_id: None,
        })
        .expect("create task");
}

fn persist_worktree(storage: &crate::storage::ManagedStorage, worktree_path: &str, branch: &str) {
    let store = storage.store.lock().expect("storage lock");
    TaskRepository::new(store.connection())
        .update_worktree_metadata(TASK_ID, worktree_path, branch)
        .expect("persist worktree metadata");
}

fn persist_run_contract(storage: &crate::storage::ManagedStorage, repository: &Path) {
    let repository_path = repository.to_string_lossy().to_string();
    let allowed_paths_json =
        serde_json::to_string(&vec![repository_path.clone()]).expect("encode allowed paths");
    let validation_command = validation_command();
    let allowed_commands_json =
        serde_json::to_string(&vec![validation_command.clone()]).expect("encode commands");
    let contract_json = serde_json::json!({
        "mode": "s11_acceptance",
        "allowedPaths": [repository_path],
        "allowedCommands": [validation_command],
        "validationCommand": validation_command,
        "tokenBudgetTotal": 4000,
        "tokenBudgetPerCall": 1200
    })
    .to_string();

    let store = storage.store.lock().expect("storage lock");
    RunContractRepository::new(store.connection())
        .upsert(NewRunContract {
            id: "contract-s11-acceptance",
            task_id: TASK_ID,
            profile_id: None,
            mode: "s11_acceptance",
            model_id: Some("gpt-5-codex"),
            reasoning_effort: "medium",
            permission_level: "workspace-write",
            network_policy: "disabled",
            allowed_paths_json: &allowed_paths_json,
            allowed_commands_json: &allowed_commands_json,
            validation_command: Some(validation_command.as_str()),
            token_budget_total: 4000,
            token_budget_per_call: 1200,
            output_language: "zh-CN",
            memory_scope: "task",
            budget_overflow_policy: "block",
            contract_json: &contract_json,
        })
        .expect("persist run contract");
}

fn record_temporary_context_file(storage: &crate::storage::ManagedStorage) {
    let paths = storage
        .roots
        .ensure_task_artifact_dirs(TASK_ID)
        .expect("create artifact dirs");
    let temp_path = paths.context_dir.join("temporary-context.txt");
    fs::write(&temp_path, "temporary context").expect("write temporary context");
    let store = storage.store.lock().expect("storage lock");
    ArtifactRepository::new(store.connection())
        .record_file(NewArtifactFile {
            id: "file-s11-temporary-context",
            task_id: TASK_ID,
            artifact_id: None,
            file_type: "context",
            path: temp_path.to_string_lossy().as_ref(),
            size_bytes: 17,
            compressed: false,
            retention_class: "temporary",
            expires_at: Some("1"),
        })
        .expect("record temporary context file");
}

fn demo_repository(label: &str) -> PathBuf {
    let repository = temp_path(label);
    fs::create_dir_all(repository.join("src")).expect("create demo repository");
    run_git(&repository, &["init"]);
    run_git(
        &repository,
        &["config", "user.email", "codemax@example.test"],
    );
    run_git(&repository, &["config", "user.name", "Codemax Test"]);
    fs::write(
        repository.join("src").join("feature.py"),
        "def enabled():\n    return False\n",
    )
    .expect("write feature fixture");
    fs::write(
        repository.join("validate.ps1"),
        "\
$text = Get-Content 'src/feature.py' -Raw
if ($text -match 'return True') {
    Write-Output 'feature enabled'
    exit 0
}
Write-Output 'feature disabled'
exit 1
",
    )
    .expect("write PowerShell validation fixture");
    fs::write(
        repository.join("validate.sh"),
        "#!/bin/sh\nif grep -q 'return True' src/feature.py; then\n  echo 'feature enabled'\n  exit 0\nfi\necho 'feature disabled'\nexit 1\n",
    )
    .expect("write shell validation fixture");
    run_git(&repository, &["add", "."]);
    run_git(&repository, &["commit", "-m", "initial demo fixture"]);

    repository
}

fn temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("codemax-s11-{label}-{}", uuid::Uuid::new_v4()))
}

fn run_git(path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .expect("run git command");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn noop_output_sink() -> CommandOutputSink {
    Arc::new(|_| {})
}

fn cleanup_paths(paths: impl IntoIterator<Item = PathBuf>) {
    for path in paths {
        if path.exists() {
            fs::remove_dir_all(path).expect("clean acceptance temp path");
        }
    }
}
