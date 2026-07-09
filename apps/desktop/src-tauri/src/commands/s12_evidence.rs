use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;
use uuid::Uuid;

use crate::{
    core::error::{AppResult, CommandError},
    storage::{
        ApprovalRepository, ArtifactFileRecord, ArtifactRecord, ArtifactRepository,
        CommandRunRecord, CommandRunRepository, ManagedStorage, NewArtifactFile, StorageError,
        TaskRecord, TaskRepository,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreInput {
    pub validation_status: String,
    pub risk_level: String,
    pub diff_file_count: usize,
    pub approval_blocked: bool,
    pub has_delivery_summary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryScoreBreakdown {
    pub score: u8,
    pub test_score: u8,
    pub risk_score: u8,
    pub diff_score: u8,
    pub approval_score: u8,
    pub explanation: String,
    pub risk_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RiskFinding {
    pub kind: String,
    pub level: String,
    pub subject: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateTaskProofPackRequest {
    pub task_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordQualityGateRequest {
    pub task_id: String,
    pub gate_type: String,
    pub status: String,
    pub message: String,
    pub evidence_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverrideQualityGateRequest {
    pub task_id: String,
    pub gate_type: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDeliveryReviewStateRequest {
    pub task_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityGateRecord {
    pub id: String,
    pub task_id: String,
    pub gate_type: String,
    pub status: String,
    pub message: String,
    pub evidence_path: Option<String>,
    pub override_reason: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityGateOverrideResult {
    pub task_id: String,
    pub gate_type: String,
    pub overridden_count: usize,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityGateResultState {
    pub status: String,
    pub gates: Vec<TaskProofPackGate>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryScoreState {
    pub value: u8,
    pub grade: String,
    pub test_score: u8,
    pub risk_score: u8,
    pub diff_score: u8,
    pub approval_score: u8,
    pub explanation: String,
    pub risk_level: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCapsuleState {
    pub task_id: String,
    pub changed_files: Vec<String>,
    pub command_count: usize,
    pub diff_path: Option<String>,
    pub delivery_path: Option<String>,
    pub manifest_path: Option<String>,
    pub summary_path: Option<String>,
    pub capsule_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleHitState {
    pub id: String,
    pub rule: String,
    pub status: String,
    pub message: String,
    pub evidence_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookRunState {
    pub id: String,
    pub hook: String,
    pub status: String,
    pub message: String,
    pub evidence_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelArenaDecisionState {
    pub status: String,
    pub selected_model: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryReviewState {
    pub task_id: String,
    pub task_status: String,
    pub status: String,
    pub can_merge: bool,
    pub blockers: Vec<String>,
    pub validation_status: String,
    pub diff_file_count: usize,
    pub approval_blocked: bool,
    pub highest_risk_level: String,
    pub proof_pack_status: String,
    pub proof_pack_id: Option<String>,
    pub proof_pack_path: Option<String>,
    pub quality_gate_result: QualityGateResultState,
    pub delivery_score: DeliveryScoreState,
    pub risk_records: Vec<RiskFinding>,
    pub task_capsule: TaskCapsuleState,
    pub rule_hits: Vec<RuleHitState>,
    pub hook_runs: Vec<HookRunState>,
    pub model_arena_decision: ModelArenaDecisionState,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedTaskProofPack {
    pub task_id: String,
    pub artifact_id: String,
    pub generated_at: String,
    pub proof_pack_path: String,
    pub summary_key: String,
    pub delivery_score: TaskProofPackScore,
    pub proposals: Vec<TaskProofPackProposal>,
    pub screenshots: Vec<TaskProofPackScreenshot>,
    pub quality_gates: Vec<TaskProofPackGate>,
    pub risks: Vec<TaskProofPackRisk>,
    pub proof_pack_id: String,
    pub proof_dir: String,
    pub manifest_path: String,
    pub summary_path: String,
    pub capsule_path: String,
    pub delivery_score_breakdown: DeliveryScoreBreakdown,
    pub risk_findings: Vec<RiskFinding>,
    pub quality_gate_blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProofPackScore {
    pub value: u8,
    pub grade: String,
    pub summary_key: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProofPackProposal {
    pub id: String,
    pub title_key: String,
    pub summary_key: String,
    pub status: String,
    pub confidence: u8,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProofPackScreenshot {
    pub id: String,
    pub title_key: String,
    pub path: String,
    pub captured_at: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProofPackGate {
    pub id: String,
    pub title_key: String,
    pub summary_key: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskProofPackRisk {
    pub id: String,
    pub title_key: String,
    pub summary_key: String,
    pub level: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProofPackManifest {
    task_id: String,
    proof_pack_id: String,
    generated_at: String,
    changed_files: Vec<String>,
    commands: Vec<String>,
    diff_path: Option<String>,
    delivery_report_path: Option<String>,
    risk_findings: Vec<RiskFinding>,
    delivery_score: DeliveryScoreBreakdown,
    quality_gate_blockers: Vec<String>,
}

#[derive(Debug, Clone)]
struct QualityGateRow {
    id: String,
    gate_type: String,
    status: String,
    message: String,
    evidence_path: Option<String>,
    override_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct ProofPackRow {
    id: String,
    proof_dir: String,
    delivery_score: u8,
    risk_level: String,
    created_at: String,
}

#[derive(Debug, Clone)]
struct DeliveryScoreRow {
    score: u8,
    test_score: u8,
    risk_score: u8,
    diff_score: u8,
    approval_score: u8,
    explanation: String,
    created_at: String,
}

#[derive(Debug)]
struct DeliveryReviewSnapshot {
    task: TaskRecord,
    commands: Vec<CommandRunRecord>,
    artifacts: Vec<ArtifactRecord>,
    artifact_files: Vec<ArtifactFileRecord>,
    approvals_blocked: bool,
    quality_gates: Vec<QualityGateRow>,
    proof_pack: Option<ProofPackRow>,
    delivery_score: Option<DeliveryScoreRow>,
}

#[tauri::command]
pub fn generate_task_proof_pack(
    storage: State<'_, ManagedStorage>,
    request: GenerateTaskProofPackRequest,
) -> AppResult<GeneratedTaskProofPack> {
    generate_task_proof_pack_inner(&storage, request)
}

#[tauri::command]
pub fn record_quality_gate_result(
    storage: State<'_, ManagedStorage>,
    request: RecordQualityGateRequest,
) -> AppResult<QualityGateRecord> {
    record_quality_gate_result_inner(&storage, request)
}

#[tauri::command]
pub fn override_quality_gate(
    storage: State<'_, ManagedStorage>,
    request: OverrideQualityGateRequest,
) -> AppResult<QualityGateOverrideResult> {
    override_quality_gate_inner(&storage, request)
}

#[tauri::command]
pub fn get_delivery_review_state(
    storage: State<'_, ManagedStorage>,
    request: GetDeliveryReviewStateRequest,
) -> AppResult<DeliveryReviewState> {
    let task_id = required_text(
        request.task_id,
        "deliveryReview.taskIdRequired",
        "Task id is required.",
    )?;

    delivery_review_state_for_task(&storage, &task_id)
}

pub(crate) fn delivery_review_state_for_task(
    storage: &ManagedStorage,
    task_id: &str,
) -> AppResult<DeliveryReviewState> {
    let snapshot = load_delivery_review_snapshot(storage, task_id)?;
    Ok(build_delivery_review_state(snapshot))
}

pub(crate) fn delivery_review_blockers_for_task(
    storage: &ManagedStorage,
    task_id: &str,
) -> AppResult<Vec<String>> {
    delivery_review_state_for_task(storage, task_id).map(|state| state.blockers)
}

pub(crate) fn refresh_task_delivery_review_status(
    storage: &ManagedStorage,
    task_id: &str,
) -> AppResult<DeliveryReviewState> {
    let mut state = delivery_review_state_for_task(storage, task_id)?;
    let next_status = if state.can_merge {
        "readyToMerge"
    } else {
        "awaitingReview"
    };

    if should_update_review_status(&state.task_status) && state.task_status != next_status {
        let store = storage.store.lock().map_err(|_| storage_lock_error())?;
        TaskRepository::new(store.connection())
            .update_status(task_id, next_status, None)
            .map_err(storage_error)?;
        state.task_status = next_status.to_string();
    }

    Ok(state)
}

pub(crate) fn generate_task_proof_pack_inner(
    storage: &ManagedStorage,
    request: GenerateTaskProofPackRequest,
) -> AppResult<GeneratedTaskProofPack> {
    let task_id = request.task_id.trim().to_string();
    if task_id.is_empty() {
        return Err(CommandError::new(
            "proofPack.taskIdRequired",
            "Task id is required to generate a Proof Pack.",
        ));
    }

    let (
        task_title,
        commands,
        changed_files,
        screenshot_paths,
        diff_path,
        report_path,
        approvals_blocked,
    ) = load_proof_inputs(storage, &task_id)?;
    let risk_findings = scan_risks(&commands, &changed_files);
    let validation_status = validation_status_for_commands(storage, &task_id)?;
    let validation_status_for_gate = validation_status.clone();
    let risk_level = highest_risk_level(&risk_findings);
    let delivery_score = calculate_delivery_score(ScoreInput {
        validation_status,
        risk_level,
        diff_file_count: changed_files.len(),
        approval_blocked: approvals_blocked,
        has_delivery_summary: report_path.is_some(),
    });
    let quality_gate_blockers = quality_gate_blockers_for_task(storage, &task_id)?;
    let proof_pack_id = format!("proof-{task_id}-{}", Uuid::new_v4());
    let paths = storage
        .roots
        .ensure_task_artifact_dirs(&task_id)
        .map_err(storage_error)?;
    let proof_dir = paths.artifacts_dir.join("proof-pack");
    fs::create_dir_all(&proof_dir).map_err(storage_error)?;
    let manifest_path = proof_dir.join("manifest.json");
    let summary_path = proof_dir.join("summary.md");
    let capsule_path = proof_dir.join("task-capsule.json");
    let generated_at = now_text();
    let manifest = ProofPackManifest {
        task_id: task_id.clone(),
        proof_pack_id: proof_pack_id.clone(),
        generated_at: generated_at.clone(),
        changed_files: changed_files.clone(),
        commands: commands.clone(),
        diff_path: diff_path.clone(),
        delivery_report_path: report_path.clone(),
        risk_findings: risk_findings.clone(),
        delivery_score: delivery_score.clone(),
        quality_gate_blockers: quality_gate_blockers.clone(),
    };

    write_json(&manifest_path, &manifest)?;
    fs::write(&summary_path, proof_summary(&task_title, &manifest)).map_err(storage_error)?;
    write_json(&capsule_path, &manifest)?;
    persist_proof_pack(
        storage,
        &task_id,
        &proof_pack_id,
        &proof_dir,
        &summary_path,
        &manifest_path,
        &capsule_path,
        &delivery_score,
    )?;
    let _review_state = refresh_task_delivery_review_status(storage, &task_id)?;

    Ok(GeneratedTaskProofPack {
        task_id: task_id.clone(),
        artifact_id: proof_pack_id.clone(),
        generated_at: generated_at.clone(),
        proof_pack_path: proof_dir.to_string_lossy().to_string(),
        summary_key: "tasks.s12.summary".to_string(),
        delivery_score: build_frontend_score(&delivery_score),
        proposals: build_proof_pack_proposals(&risk_findings, &quality_gate_blockers),
        screenshots: build_proof_pack_screenshots(&screenshot_paths, &generated_at),
        quality_gates: build_quality_gates(
            &validation_status_for_gate,
            !changed_files.is_empty(),
            approvals_blocked,
            &quality_gate_blockers,
        ),
        risks: build_frontend_risks(&risk_findings),
        proof_pack_id,
        proof_dir: proof_dir.to_string_lossy().to_string(),
        manifest_path: manifest_path.to_string_lossy().to_string(),
        summary_path: summary_path.to_string_lossy().to_string(),
        capsule_path: capsule_path.to_string_lossy().to_string(),
        delivery_score_breakdown: delivery_score,
        risk_findings,
        quality_gate_blockers,
    })
}

pub(crate) fn record_quality_gate_result_inner(
    storage: &ManagedStorage,
    request: RecordQualityGateRequest,
) -> AppResult<QualityGateRecord> {
    let task_id = required_text(
        request.task_id,
        "qualityGate.taskIdRequired",
        "Task id is required.",
    )?;
    let gate_type = required_text(
        request.gate_type,
        "qualityGate.typeRequired",
        "Quality gate type is required.",
    )?;
    let status = required_text(
        request.status,
        "qualityGate.statusRequired",
        "Quality gate status is required.",
    )?;
    let message = required_text(
        request.message,
        "qualityGate.messageRequired",
        "Quality gate message is required.",
    )?;
    let id = format!("gate-{task_id}-{gate_type}-{}", Uuid::new_v4());
    let created_at = now_text();

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    TaskRepository::new(connection)
        .get_required(&task_id)
        .map_err(storage_error)?;
    connection
        .execute(
            "INSERT INTO quality_gate_results
             (id, task_id, gate_type, status, message, evidence_path, override_reason, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)",
            params![
                id,
                task_id,
                gate_type,
                status,
                message,
                request.evidence_path,
                created_at,
            ],
        )
        .map_err(storage_error)?;
    drop(store);
    let _review_state = refresh_task_delivery_review_status(storage, &task_id)?;

    Ok(QualityGateRecord {
        id,
        task_id,
        gate_type,
        status,
        message,
        evidence_path: request.evidence_path,
        override_reason: None,
        created_at,
    })
}

pub(crate) fn override_quality_gate_inner(
    storage: &ManagedStorage,
    request: OverrideQualityGateRequest,
) -> AppResult<QualityGateOverrideResult> {
    let task_id = required_text(
        request.task_id,
        "qualityGate.taskIdRequired",
        "Task id is required.",
    )?;
    let gate_type = required_text(
        request.gate_type,
        "qualityGate.typeRequired",
        "Quality gate type is required.",
    )?;
    let reason = required_text(
        request.reason,
        "qualityGate.overrideReasonRequired",
        "Quality gate override reason is required.",
    )?;

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    TaskRepository::new(connection)
        .get_required(&task_id)
        .map_err(storage_error)?;
    let updated = connection
        .execute(
            "UPDATE quality_gate_results
             SET override_reason = ?3
             WHERE task_id = ?1
               AND gate_type = ?2
               AND status != 'passed'
               AND override_reason IS NULL",
            params![task_id, gate_type, reason],
        )
        .map_err(storage_error)?;
    drop(store);
    let _review_state = refresh_task_delivery_review_status(storage, &task_id)?;

    Ok(QualityGateOverrideResult {
        task_id,
        gate_type,
        overridden_count: updated,
        reason,
    })
}

pub(crate) fn calculate_delivery_score(input: ScoreInput) -> DeliveryScoreBreakdown {
    let test_score = match input.validation_status.as_str() {
        "passed" => 35,
        "failed" => 8,
        _ => 14,
    };
    let risk_score = match input.risk_level.as_str() {
        "high" => 5,
        "medium" => 16,
        _ => 25,
    };
    let diff_score = if input.diff_file_count == 0 {
        0
    } else if input.diff_file_count <= 8 {
        20
    } else {
        12
    };
    let approval_score = if input.approval_blocked { 0 } else { 10 };
    let summary_score = if input.has_delivery_summary { 10 } else { 0 };
    let score = test_score + risk_score + diff_score + approval_score + summary_score;
    DeliveryScoreBreakdown {
        score,
        test_score,
        risk_score,
        diff_score,
        approval_score,
        explanation: format!(
            "validation={}, risk={}, files={}, approvalBlocked={}, summary={}",
            input.validation_status,
            input.risk_level,
            input.diff_file_count,
            input.approval_blocked,
            input.has_delivery_summary
        ),
        risk_level: input.risk_level,
    }
}

pub(crate) fn scan_risks(commands: &[String], changed_files: &[String]) -> Vec<RiskFinding> {
    let mut findings = Vec::new();
    for command in commands {
        let lower = command.to_lowercase();
        if lower.contains("rm -rf")
            || lower.contains("del /")
            || lower.contains("format ")
            || lower.contains("sudo ")
            || lower.contains("chmod -r")
        {
            findings.push(RiskFinding {
                kind: "dangerousCommand".to_string(),
                level: "high".to_string(),
                subject: command.clone(),
                reason: "Command can delete, elevate, or recursively change local files."
                    .to_string(),
            });
        }
    }

    for file in changed_files {
        let lower = file.to_lowercase();
        if lower.ends_with(".env")
            || lower.contains("secret")
            || lower.contains("credential")
            || lower.contains("id_rsa")
        {
            findings.push(RiskFinding {
                kind: "sensitiveFile".to_string(),
                level: "high".to_string(),
                subject: file.clone(),
                reason: "Changed file path looks sensitive and needs human review.".to_string(),
            });
        }
        if lower.contains("package.json")
            || lower.contains("cargo.toml")
            || lower.contains("pyproject.toml")
            || lower.contains("pom.xml")
        {
            findings.push(RiskFinding {
                kind: "dependencyChange".to_string(),
                level: "medium".to_string(),
                subject: file.clone(),
                reason: "Dependency or build configuration changed.".to_string(),
            });
        }
        if lower.contains("migration") || lower.contains("schema") {
            findings.push(RiskFinding {
                kind: "schemaChange".to_string(),
                level: "medium".to_string(),
                subject: file.clone(),
                reason: "Database schema or migration path changed.".to_string(),
            });
        }
    }
    findings
}

fn load_delivery_review_snapshot(
    storage: &ManagedStorage,
    task_id: &str,
) -> AppResult<DeliveryReviewSnapshot> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    let task = TaskRepository::new(connection)
        .get_required(task_id)
        .map_err(storage_error)?;
    let commands = CommandRunRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?;
    let artifacts = ArtifactRepository::new(connection)
        .artifacts_for_task(task_id)
        .map_err(storage_error)?;
    let artifact_files = ArtifactRepository::new(connection)
        .files_for_task(task_id)
        .map_err(storage_error)?;
    let approvals_blocked = ApprovalRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?
        .into_iter()
        .any(|approval| approval.decision.as_deref() != Some("approved"));
    let quality_gates = load_quality_gate_rows(connection, task_id)?;
    let proof_pack = load_latest_proof_pack(connection, task_id)?;
    let delivery_score = load_latest_delivery_score(connection, task_id)?;

    Ok(DeliveryReviewSnapshot {
        task,
        commands,
        artifacts,
        artifact_files,
        approvals_blocked,
        quality_gates,
        proof_pack,
        delivery_score,
    })
}

fn build_delivery_review_state(snapshot: DeliveryReviewSnapshot) -> DeliveryReviewState {
    let task_id = snapshot.task.id.clone();
    let latest_artifact = latest_delivery_artifact(&snapshot.artifacts);
    let changed_files = latest_artifact
        .map(|artifact| changed_files_from_json(&artifact.changed_files))
        .unwrap_or_default();
    let commands = snapshot
        .commands
        .iter()
        .map(|run| run.command.clone())
        .collect::<Vec<_>>();
    let validation_status = validation_status_from_runs(&snapshot.commands);
    let diff_path = latest_artifact.and_then(|artifact| artifact.diff_path.clone());
    let delivery_path = latest_artifact.and_then(|artifact| artifact.test_report_path.clone());
    let risk_records = scan_risks(&commands, &changed_files);
    let highest_risk_level = highest_risk_level(&risk_records);
    let manual_gate_blockers = quality_gate_blockers_from_rows(&snapshot.quality_gates);
    let proof_pack_path = snapshot
        .proof_pack
        .as_ref()
        .map(|proof_pack| proof_pack.proof_dir.clone());
    let proof_pack_status = if proof_pack_path.is_some() {
        "generated"
    } else {
        "missing"
    };
    let manifest_path = artifact_file_path(&snapshot.artifact_files, "proof_manifest");
    let summary_path = artifact_file_path(&snapshot.artifact_files, "proof_summary");
    let capsule_path = artifact_file_path(&snapshot.artifact_files, "task_capsule");
    let mut blockers = Vec::new();

    if validation_status != "passed" {
        blockers.push(format!("validation status is {validation_status}"));
    }
    if changed_files.is_empty() {
        blockers.push("final diff is empty".to_string());
    }
    if proof_pack_path.is_none() {
        blockers.push("proof pack has not been generated".to_string());
    }
    if snapshot.approvals_blocked {
        blockers.push("approval is not approved".to_string());
    }
    if risk_records.iter().any(|finding| finding.level == "high") {
        blockers.push("high risk finding requires review".to_string());
    }
    blockers.extend(manual_gate_blockers.clone());

    let status = if blockers.is_empty() {
        "passed"
    } else if validation_status == "passed" && proof_pack_path.is_some() {
        "warning"
    } else {
        "blocked"
    };
    let quality_gate_result = QualityGateResultState {
        status: status.to_string(),
        gates: build_quality_gates(
            &validation_status,
            !changed_files.is_empty() && proof_pack_path.is_some(),
            snapshot.approvals_blocked,
            &manual_gate_blockers,
        ),
        blockers: manual_gate_blockers,
    };
    let delivery_score = delivery_score_state(
        snapshot.delivery_score.as_ref(),
        snapshot.proof_pack.as_ref(),
        &validation_status,
        &highest_risk_level,
        changed_files.len(),
        snapshot.approvals_blocked,
        delivery_path.is_some(),
    );
    let rule_hits = build_rule_hits(
        &validation_status,
        !changed_files.is_empty(),
        proof_pack_path.as_deref(),
        &risk_records,
        &snapshot.quality_gates,
    );
    let hook_runs = build_hook_runs(
        &snapshot.commands,
        &validation_status,
        proof_pack_path.as_deref(),
    );
    let model_arena_decision = ModelArenaDecisionState {
        status: "placeholder".to_string(),
        selected_model: snapshot.task.model_id.clone(),
        reason: "Model Arena decision is reserved for D integration and exposed as stable state."
            .to_string(),
    };

    DeliveryReviewState {
        task_id: task_id.clone(),
        task_status: snapshot.task.status,
        status: status.to_string(),
        can_merge: blockers.is_empty(),
        blockers,
        validation_status,
        diff_file_count: changed_files.len(),
        approval_blocked: snapshot.approvals_blocked,
        highest_risk_level,
        proof_pack_status: proof_pack_status.to_string(),
        proof_pack_id: snapshot
            .proof_pack
            .as_ref()
            .map(|proof_pack| proof_pack.id.clone()),
        proof_pack_path,
        quality_gate_result,
        delivery_score,
        risk_records,
        task_capsule: TaskCapsuleState {
            task_id,
            changed_files,
            command_count: snapshot.commands.len(),
            diff_path,
            delivery_path,
            manifest_path,
            summary_path,
            capsule_path,
        },
        rule_hits,
        hook_runs,
        model_arena_decision,
        updated_at: now_text(),
    }
}

fn load_quality_gate_rows(
    connection: &Connection,
    task_id: &str,
) -> AppResult<Vec<QualityGateRow>> {
    let mut statement = connection
        .prepare(
            "SELECT id, gate_type, status, message, evidence_path, override_reason
             FROM quality_gate_results
             WHERE task_id = ?1
             ORDER BY created_at ASC, id ASC",
        )
        .map_err(storage_error)?;
    let rows = statement
        .query_map(params![task_id], |row| {
            Ok(QualityGateRow {
                id: row.get(0)?,
                gate_type: row.get(1)?,
                status: row.get(2)?,
                message: row.get(3)?,
                evidence_path: row.get(4)?,
                override_reason: row.get(5)?,
            })
        })
        .map_err(storage_error)?;
    let mut gates = Vec::new();
    for row in rows {
        gates.push(row.map_err(storage_error)?);
    }
    Ok(gates)
}

fn load_latest_proof_pack(
    connection: &Connection,
    task_id: &str,
) -> AppResult<Option<ProofPackRow>> {
    connection
        .query_row(
            "SELECT id, proof_dir, delivery_score, risk_level, created_at
             FROM proof_packs
             WHERE task_id = ?1
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
            params![task_id],
            |row| {
                let delivery_score: i64 = row.get(2)?;
                Ok(ProofPackRow {
                    id: row.get(0)?,
                    proof_dir: row.get(1)?,
                    delivery_score: clamp_score(delivery_score),
                    risk_level: row.get(3)?,
                    created_at: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(storage_error)
}

fn load_latest_delivery_score(
    connection: &Connection,
    task_id: &str,
) -> AppResult<Option<DeliveryScoreRow>> {
    connection
        .query_row(
            "SELECT score, test_score, risk_score, diff_score, approval_score, explanation, created_at
             FROM delivery_scores
             WHERE task_id = ?1
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
            params![task_id],
            |row| {
                let score: i64 = row.get(0)?;
                let test_score: i64 = row.get(1)?;
                let risk_score: i64 = row.get(2)?;
                let diff_score: i64 = row.get(3)?;
                let approval_score: i64 = row.get(4)?;
                Ok(DeliveryScoreRow {
                    score: clamp_score(score),
                    test_score: clamp_score(test_score),
                    risk_score: clamp_score(risk_score),
                    diff_score: clamp_score(diff_score),
                    approval_score: clamp_score(approval_score),
                    explanation: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(storage_error)
}

fn quality_gate_blockers_from_rows(rows: &[QualityGateRow]) -> Vec<String> {
    rows.iter()
        .filter(|row| row.status != "passed" && row.override_reason.is_none())
        .map(|row| {
            format!(
                "quality gate {} is {}: {}",
                row.gate_type, row.status, row.message
            )
        })
        .collect()
}

fn delivery_score_state(
    persisted: Option<&DeliveryScoreRow>,
    proof_pack: Option<&ProofPackRow>,
    validation_status: &str,
    risk_level: &str,
    diff_file_count: usize,
    approval_blocked: bool,
    has_delivery_summary: bool,
) -> DeliveryScoreState {
    if let Some(score) = persisted {
        return DeliveryScoreState {
            value: score.score,
            grade: delivery_grade(score.score).to_string(),
            test_score: score.test_score,
            risk_score: score.risk_score,
            diff_score: score.diff_score,
            approval_score: score.approval_score,
            explanation: score.explanation.clone(),
            risk_level: proof_pack
                .map(|proof_pack| proof_pack.risk_level.clone())
                .unwrap_or_else(|| risk_level.to_string()),
            created_at: Some(score.created_at.clone()),
        };
    }

    if let Some(proof_pack) = proof_pack {
        return DeliveryScoreState {
            value: proof_pack.delivery_score,
            grade: delivery_grade(proof_pack.delivery_score).to_string(),
            test_score: 0,
            risk_score: 0,
            diff_score: 0,
            approval_score: 0,
            explanation: "Score was loaded from the latest proof pack index.".to_string(),
            risk_level: proof_pack.risk_level.clone(),
            created_at: Some(proof_pack.created_at.clone()),
        };
    }

    let calculated = calculate_delivery_score(ScoreInput {
        validation_status: validation_status.to_string(),
        risk_level: risk_level.to_string(),
        diff_file_count,
        approval_blocked,
        has_delivery_summary,
    });
    DeliveryScoreState {
        value: calculated.score,
        grade: delivery_grade(calculated.score).to_string(),
        test_score: calculated.test_score,
        risk_score: calculated.risk_score,
        diff_score: calculated.diff_score,
        approval_score: calculated.approval_score,
        explanation: calculated.explanation,
        risk_level: calculated.risk_level,
        created_at: None,
    }
}

fn build_rule_hits(
    validation_status: &str,
    has_diff: bool,
    proof_pack_path: Option<&str>,
    risks: &[RiskFinding],
    quality_gates: &[QualityGateRow],
) -> Vec<RuleHitState> {
    let mut hits = vec![
        RuleHitState {
            id: "rule-validation".to_string(),
            rule: "validation_must_pass".to_string(),
            status: if validation_status == "passed" {
                "passed"
            } else {
                "blocked"
            }
            .to_string(),
            message: format!("validation status is {validation_status}"),
            evidence_path: None,
        },
        RuleHitState {
            id: "rule-diff".to_string(),
            rule: "final_diff_required".to_string(),
            status: if has_diff { "passed" } else { "blocked" }.to_string(),
            message: if has_diff {
                "final diff is recorded".to_string()
            } else {
                "final diff is empty".to_string()
            },
            evidence_path: None,
        },
        RuleHitState {
            id: "rule-proof-pack".to_string(),
            rule: "proof_pack_required".to_string(),
            status: if proof_pack_path.is_some() {
                "passed"
            } else {
                "blocked"
            }
            .to_string(),
            message: proof_pack_path
                .map(|path| format!("proof pack generated at {path}"))
                .unwrap_or_else(|| "proof pack has not been generated".to_string()),
            evidence_path: proof_pack_path.map(ToOwned::to_owned),
        },
        RuleHitState {
            id: "rule-risk".to_string(),
            rule: "high_risk_requires_review".to_string(),
            status: if risks.iter().any(|risk| risk.level == "high") {
                "blocked"
            } else if risks.iter().any(|risk| risk.level == "medium") {
                "warning"
            } else {
                "passed"
            }
            .to_string(),
            message: format!("{} risk finding(s) detected", risks.len()),
            evidence_path: None,
        },
    ];

    hits.extend(quality_gates.iter().map(|gate| {
        RuleHitState {
            id: format!("rule-quality-{}", gate.id),
            rule: format!("manual_quality_gate:{}", gate.gate_type),
            status: if gate.override_reason.is_some() {
                "warning".to_string()
            } else {
                gate.status.clone()
            },
            message: gate
                .override_reason
                .as_ref()
                .map(|reason| format!("{} (override: {reason})", gate.message))
                .unwrap_or_else(|| gate.message.clone()),
            evidence_path: gate.evidence_path.clone(),
        }
    }));
    hits
}

fn build_hook_runs(
    commands: &[CommandRunRecord],
    validation_status: &str,
    proof_pack_path: Option<&str>,
) -> Vec<HookRunState> {
    let validation_count = commands
        .iter()
        .filter(|run| run.purpose == "validation")
        .count();
    vec![
        HookRunState {
            id: "hook-validation-cycle".to_string(),
            hook: "validation_cycle".to_string(),
            status: validation_status.to_string(),
            message: format!("{validation_count} validation command(s) recorded"),
            evidence_path: commands
                .iter()
                .rev()
                .find(|run| run.purpose == "validation")
                .and_then(|run| run.stdout_path.clone()),
        },
        HookRunState {
            id: "hook-proof-pack".to_string(),
            hook: "proof_pack_generator".to_string(),
            status: if proof_pack_path.is_some() {
                "passed"
            } else {
                "notRun"
            }
            .to_string(),
            message: proof_pack_path
                .map(|path| format!("Proof Pack is indexed at {path}"))
                .unwrap_or_else(|| "Proof Pack generator has not produced evidence".to_string()),
            evidence_path: proof_pack_path.map(ToOwned::to_owned),
        },
    ]
}

fn latest_delivery_artifact(artifacts: &[ArtifactRecord]) -> Option<&ArtifactRecord> {
    artifacts
        .iter()
        .rev()
        .find(|artifact| artifact.diff_path.is_some() || artifact.test_report_path.is_some())
}

fn artifact_file_path(files: &[ArtifactFileRecord], file_type: &str) -> Option<String> {
    files
        .iter()
        .rev()
        .find(|file| file.file_type == file_type)
        .map(|file| file.path.clone())
}

fn validation_status_from_runs(runs: &[CommandRunRecord]) -> String {
    let mut latest_by_command = BTreeMap::new();
    for run in runs.iter().filter(|run| run.purpose == "validation") {
        latest_by_command.insert(run.command.clone(), run);
    }
    if latest_by_command.is_empty() {
        return "notRun".to_string();
    }
    if latest_by_command
        .values()
        .all(|run| run.status == "passed" && run.exit_code.unwrap_or(0) == 0)
    {
        "passed".to_string()
    } else {
        "failed".to_string()
    }
}

fn should_update_review_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "awaitingReview" | "readyToMerge" | "validating" | "repairing"
    )
}

fn clamp_score(value: i64) -> u8 {
    value.clamp(0, 100) as u8
}

pub(crate) fn quality_gate_blockers_for_task(
    storage: &ManagedStorage,
    task_id: &str,
) -> AppResult<Vec<String>> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let mut statement = store
        .connection()
        .prepare(
            "SELECT gate_type, status, message FROM quality_gate_results
             WHERE task_id = ?1 AND status != 'passed' AND override_reason IS NULL
             ORDER BY created_at ASC",
        )
        .map_err(storage_error)?;
    let rows = statement
        .query_map(params![task_id], |row| {
            let gate_type: String = row.get(0)?;
            let status: String = row.get(1)?;
            let message: String = row.get(2)?;
            Ok(format!("quality gate {gate_type} is {status}: {message}"))
        })
        .map_err(storage_error)?;

    let mut blockers = Vec::new();
    for row in rows {
        blockers.push(row.map_err(storage_error)?);
    }
    Ok(blockers)
}

fn load_proof_inputs(
    storage: &ManagedStorage,
    task_id: &str,
) -> AppResult<(
    String,
    Vec<String>,
    Vec<String>,
    Vec<String>,
    Option<String>,
    Option<String>,
    bool,
)> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    let task = TaskRepository::new(connection)
        .get_required(task_id)
        .map_err(storage_error)?;
    let commands = CommandRunRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?
        .into_iter()
        .map(|run| run.command)
        .collect::<Vec<_>>();
    let artifacts = ArtifactRepository::new(connection)
        .artifacts_for_task(task_id)
        .map_err(storage_error)?;
    let latest_artifact = artifacts
        .iter()
        .rev()
        .find(|artifact| artifact.diff_path.is_some() || artifact.test_report_path.is_some());
    let changed_files = latest_artifact
        .map(|artifact| changed_files_from_json(&artifact.changed_files))
        .unwrap_or_default();
    let screenshot_paths = latest_artifact
        .map(|artifact| changed_files_from_json(&artifact.screenshots))
        .unwrap_or_default();
    let diff_path = latest_artifact.and_then(|artifact| artifact.diff_path.clone());
    let report_path = latest_artifact.and_then(|artifact| artifact.test_report_path.clone());
    let approvals_blocked = ApprovalRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?
        .into_iter()
        .any(|approval| approval.decision.as_deref() != Some("approved"));

    Ok((
        task.title,
        commands,
        changed_files,
        screenshot_paths,
        diff_path,
        report_path,
        approvals_blocked,
    ))
}

fn validation_status_for_commands(storage: &ManagedStorage, task_id: &str) -> AppResult<String> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let runs = CommandRunRepository::new(store.connection())
        .list_for_task(task_id)
        .map_err(storage_error)?
        .into_iter()
        .filter(|run| run.purpose == "validation")
        .collect::<Vec<_>>();
    if runs.is_empty() {
        return Ok("notRun".to_string());
    }
    if runs
        .iter()
        .all(|run| run.status == "passed" && run.exit_code.unwrap_or(0) == 0)
    {
        Ok("passed".to_string())
    } else {
        Ok("failed".to_string())
    }
}

fn changed_files_from_json(raw: &str) -> Vec<String> {
    let mut files = BTreeSet::new();
    if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(raw) {
        for item in items {
            if let Some(path) = item
                .get("path")
                .and_then(Value::as_str)
                .or_else(|| item.as_str())
            {
                files.insert(path.to_string());
            }
        }
    }
    files.into_iter().collect()
}

fn highest_risk_level(findings: &[RiskFinding]) -> String {
    if findings.iter().any(|finding| finding.level == "high") {
        "high".to_string()
    } else if findings.iter().any(|finding| finding.level == "medium") {
        "medium".to_string()
    } else {
        "low".to_string()
    }
}

fn required_text(value: String, code: &str, message: &str) -> AppResult<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(CommandError::new(code, message));
    }
    Ok(value)
}

fn build_frontend_score(score: &DeliveryScoreBreakdown) -> TaskProofPackScore {
    TaskProofPackScore {
        value: score.score,
        grade: delivery_grade(score.score).to_string(),
        summary_key: "tasks.s12.deliveryScore.summary".to_string(),
    }
}

fn delivery_grade(score: u8) -> &'static str {
    match score {
        90..=100 => "A",
        80..=89 => "B",
        70..=79 => "C",
        60..=69 => "D",
        _ => "E",
    }
}

fn build_proof_pack_proposals(
    risk_findings: &[RiskFinding],
    quality_gate_blockers: &[String],
) -> Vec<TaskProofPackProposal> {
    let strict_status = if quality_gate_blockers.is_empty() {
        "passed"
    } else {
        "blocked"
    };
    let minimal_status = if risk_findings.iter().any(|finding| finding.level == "high") {
        "warning"
    } else {
        "passed"
    };

    vec![
        TaskProofPackProposal {
            id: "proposal-minimal".to_string(),
            title_key: "tasks.s12.proposals.minimal.title".to_string(),
            summary_key: "tasks.s12.proposals.minimal.summary".to_string(),
            status: minimal_status.to_string(),
            confidence: if minimal_status == "passed" { 90 } else { 74 },
        },
        TaskProofPackProposal {
            id: "proposal-hardened".to_string(),
            title_key: "tasks.s12.proposals.hardened.title".to_string(),
            summary_key: "tasks.s12.proposals.hardened.summary".to_string(),
            status: strict_status.to_string(),
            confidence: if strict_status == "passed" { 86 } else { 68 },
        },
    ]
}

fn build_proof_pack_screenshots(
    screenshot_paths: &[String],
    generated_at: &str,
) -> Vec<TaskProofPackScreenshot> {
    if screenshot_paths.is_empty() {
        return vec![TaskProofPackScreenshot {
            id: "screenshot-overview".to_string(),
            title_key: "tasks.s12.screenshots.overview".to_string(),
            path: "No screenshot artifact recorded for this task.".to_string(),
            captured_at: generated_at.to_string(),
            status: "warning".to_string(),
        }];
    }

    screenshot_paths
        .iter()
        .enumerate()
        .map(|(index, path)| TaskProofPackScreenshot {
            id: format!("screenshot-{}", index + 1),
            title_key: if index == 0 {
                "tasks.s12.screenshots.overview".to_string()
            } else {
                "tasks.s12.screenshots.mobile".to_string()
            },
            path: path.clone(),
            captured_at: generated_at.to_string(),
            status: "passed".to_string(),
        })
        .collect()
}

fn build_quality_gates(
    validation_status: &str,
    has_diff: bool,
    approvals_blocked: bool,
    quality_gate_blockers: &[String],
) -> Vec<TaskProofPackGate> {
    let validation_gate_status = if validation_status == "passed" {
        "passed"
    } else {
        "blocked"
    };
    let proof_gate_status = if has_diff && quality_gate_blockers.is_empty() {
        "passed"
    } else if quality_gate_blockers.is_empty() {
        "warning"
    } else {
        "blocked"
    };
    let approval_gate_status = if approvals_blocked {
        "warning"
    } else {
        "passed"
    };

    vec![
        TaskProofPackGate {
            id: "gate-tests".to_string(),
            title_key: "tasks.s12.qualityGate.tests.title".to_string(),
            summary_key: "tasks.s12.qualityGate.tests.summary".to_string(),
            status: validation_gate_status.to_string(),
        },
        TaskProofPackGate {
            id: "gate-proof".to_string(),
            title_key: "tasks.s12.qualityGate.proof.title".to_string(),
            summary_key: "tasks.s12.qualityGate.proof.summary".to_string(),
            status: proof_gate_status.to_string(),
        },
        TaskProofPackGate {
            id: "gate-approval".to_string(),
            title_key: "tasks.s12.qualityGate.approval.title".to_string(),
            summary_key: "tasks.s12.qualityGate.approval.summary".to_string(),
            status: approval_gate_status.to_string(),
        },
    ]
}

fn build_frontend_risks(findings: &[RiskFinding]) -> Vec<TaskProofPackRisk> {
    if findings.is_empty() {
        return vec![TaskProofPackRisk {
            id: "risk-storage".to_string(),
            title_key: "tasks.s12.riskRadar.storage.title".to_string(),
            summary_key: "tasks.s12.riskRadar.storage.summary".to_string(),
            level: "low".to_string(),
        }];
    }

    findings
        .iter()
        .enumerate()
        .map(|(index, finding)| TaskProofPackRisk {
            id: format!("risk-{}", index + 1),
            title_key: if finding.kind == "dependencyChange" || finding.kind == "schemaChange" {
                "tasks.s12.riskRadar.storage.title".to_string()
            } else {
                "tasks.s12.riskRadar.backend.title".to_string()
            },
            summary_key: if finding.kind == "dependencyChange" || finding.kind == "schemaChange" {
                "tasks.s12.riskRadar.storage.summary".to_string()
            } else {
                "tasks.s12.riskRadar.backend.summary".to_string()
            },
            level: finding.level.clone(),
        })
        .collect()
}

fn proof_summary(task_title: &str, manifest: &ProofPackManifest) -> String {
    format!(
        "# Proof Pack\n\nTask: {task_title}\n\nScore: {}\n\nRisk: {}\n\nChanged files: {}\n\nQuality blockers: {}\n",
        manifest.delivery_score.score,
        manifest.delivery_score.risk_level,
        manifest.changed_files.len(),
        manifest.quality_gate_blockers.len()
    )
}

fn persist_proof_pack(
    storage: &ManagedStorage,
    task_id: &str,
    proof_pack_id: &str,
    proof_dir: &Path,
    summary_path: &Path,
    manifest_path: &Path,
    capsule_path: &Path,
    delivery_score: &DeliveryScoreBreakdown,
) -> AppResult<()> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    let now = now_text();
    connection
        .execute(
            "INSERT OR REPLACE INTO proof_packs
             (id, task_id, summary, proof_dir, export_path, delivery_score, risk_level, created_at)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7)",
            params![
                proof_pack_id,
                task_id,
                "S12 Proof Pack generated.",
                proof_dir.to_string_lossy(),
                i64::from(delivery_score.score),
                delivery_score.risk_level,
                now,
            ],
        )
        .map_err(storage_error)?;
    connection
        .execute(
            "INSERT OR REPLACE INTO delivery_scores
             (id, task_id, score, test_score, risk_score, diff_score, approval_score, explanation, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                format!("score-{proof_pack_id}"),
                task_id,
                i64::from(delivery_score.score),
                i64::from(delivery_score.test_score),
                i64::from(delivery_score.risk_score),
                i64::from(delivery_score.diff_score),
                i64::from(delivery_score.approval_score),
                delivery_score.explanation,
                now,
            ],
        )
        .map_err(storage_error)?;
    let artifacts = ArtifactRepository::new(connection);
    for (path, file_type) in [
        (summary_path, "proof_summary"),
        (manifest_path, "proof_manifest"),
        (capsule_path, "task_capsule"),
    ] {
        let path_text = path.to_string_lossy().to_string();
        artifacts
            .record_file(NewArtifactFile {
                id: &format!("file-{}-{}", proof_pack_id, file_type),
                task_id,
                artifact_id: None,
                file_type,
                path: &path_text,
                size_bytes: file_size(path).map_err(storage_error)? as i64,
                compressed: false,
                retention_class: "permanent",
                expires_at: None,
            })
            .map_err(storage_error)?;
    }
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> AppResult<()> {
    let json = serde_json::to_string_pretty(value).map_err(json_error)?;
    fs::write(path, json).map_err(storage_error)
}

fn file_size(path: &Path) -> std::io::Result<u64> {
    Ok(fs::metadata(path)?.len())
}

fn now_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

fn storage_lock_error() -> CommandError {
    CommandError::new(
        "storage.lockUnavailable",
        "Local storage is temporarily unavailable.",
    )
}

fn storage_error(error: impl Into<StorageError>) -> CommandError {
    match error.into() {
        StorageError::NotFound(message) => CommandError::new("task.notFound", message),
        StorageError::UnsafeCleanup { task_id, reasons } => CommandError::new(
            "storage.unsafeCleanup",
            format!(
                "Task {task_id} is not safe to clean: {}",
                reasons.join("; ")
            ),
        ),
        StorageError::Sqlite(error) => CommandError::new(
            "storage.sqliteError",
            format!("Local database error: {error}"),
        ),
        StorageError::Io(error) => CommandError::new(
            "storage.filesystemError",
            format!("Filesystem error: {error}"),
        ),
    }
}

fn json_error(error: serde_json::Error) -> CommandError {
    CommandError::new(
        "proofPack.invalidJson",
        format!("Unable to encode Proof Pack JSON: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{
        ArtifactRepository, CommandRunRepository, ManagedStorage, NewArtifact, NewCommandRun,
        NewTask, SqliteStore, StorageRoots, TaskRepository,
    };
    use std::{path::PathBuf, sync::Mutex};

    #[test]
    fn delivery_score_penalizes_failed_validation_and_high_risk() {
        let score = calculate_delivery_score(ScoreInput {
            validation_status: "failed".to_string(),
            risk_level: "high".to_string(),
            diff_file_count: 4,
            approval_blocked: true,
            has_delivery_summary: true,
        });

        assert!(score.score < 70);
        assert_eq!(score.risk_level, "high");
    }

    #[test]
    fn risk_radar_detects_dangerous_command_and_sensitive_file() {
        let findings = scan_risks(
            &["rm -rf target".to_string()],
            &[".env".to_string(), "src/main.rs".to_string()],
        );

        assert!(findings
            .iter()
            .any(|finding| finding.kind == "dangerousCommand"));
        assert!(findings
            .iter()
            .any(|finding| finding.kind == "sensitiveFile"));
    }

    #[test]
    fn proof_pack_writes_manifest_capsule_and_database_indexes() {
        let (storage, root) = proof_pack_storage();
        seed_proof_task(&storage);

        let result = generate_task_proof_pack_inner(
            &storage,
            GenerateTaskProofPackRequest {
                task_id: "task-s12-proof".to_string(),
            },
        )
        .expect("generate proof pack");

        assert!(Path::new(&result.manifest_path).is_file());
        assert!(Path::new(&result.summary_path).is_file());
        assert!(Path::new(&result.capsule_path).is_file());
        assert_eq!(proof_pack_count(&storage), 1);
        assert_eq!(delivery_score_count(&storage), 1);

        if root.exists() {
            fs::remove_dir_all(root).expect("clean proof test root");
        }
    }

    #[test]
    fn delivery_review_blocks_ready_to_merge_until_proof_pack_exists() {
        let (storage, root) = proof_pack_storage();
        seed_proof_task(&storage);

        let blocked = refresh_task_delivery_review_status(&storage, "task-s12-proof")
            .expect("refresh delivery review before proof pack");
        assert!(!blocked.can_merge);
        assert_eq!(blocked.proof_pack_status, "missing");
        assert!(blocked
            .blockers
            .iter()
            .any(|blocker| blocker.contains("proof pack")));
        assert_eq!(task_status(&storage, "task-s12-proof"), "awaitingReview");

        let _proof_pack = generate_task_proof_pack_inner(
            &storage,
            GenerateTaskProofPackRequest {
                task_id: "task-s12-proof".to_string(),
            },
        )
        .expect("generate proof pack");

        let passed = delivery_review_state_for_task(&storage, "task-s12-proof")
            .expect("read delivery review after proof pack");
        assert!(passed.can_merge);
        assert_eq!(passed.proof_pack_status, "generated");
        assert!(passed.blockers.is_empty());
        assert_eq!(task_status(&storage, "task-s12-proof"), "readyToMerge");

        if root.exists() {
            fs::remove_dir_all(root).expect("clean proof test root");
        }
    }

    #[test]
    fn failed_quality_gate_blocks_until_override_reason_is_recorded() {
        let (storage, root) = proof_pack_storage();
        seed_proof_task(&storage);

        record_quality_gate_result_inner(
            &storage,
            RecordQualityGateRequest {
                task_id: "task-s12-proof".to_string(),
                gate_type: "build".to_string(),
                status: "failed".to_string(),
                message: "npm run build failed".to_string(),
                evidence_path: Some(
                    "D:/codemax/app-data/tasks/task-s12-proof/report.json".to_string(),
                ),
            },
        )
        .expect("record failed gate");

        let blocked = quality_gate_blockers_for_task(&storage, "task-s12-proof")
            .expect("read quality blockers");
        assert_eq!(blocked.len(), 1);

        let override_result = override_quality_gate_inner(
            &storage,
            OverrideQualityGateRequest {
                task_id: "task-s12-proof".to_string(),
                gate_type: "build".to_string(),
                reason: "User accepted temporary build warning after manual review.".to_string(),
            },
        )
        .expect("override failed gate");

        assert_eq!(override_result.overridden_count, 1);
        let unblocked = quality_gate_blockers_for_task(&storage, "task-s12-proof")
            .expect("read quality blockers after override");
        assert!(unblocked.is_empty());

        if root.exists() {
            fs::remove_dir_all(root).expect("clean proof test root");
        }
    }

    fn proof_pack_storage() -> (ManagedStorage, PathBuf) {
        let root = std::env::temp_dir().join(format!("codemax-s12-proof-{}", Uuid::new_v4()));
        let store = SqliteStore::open_in_memory().expect("open sqlite");
        store.migrate().expect("migrate sqlite");
        (
            ManagedStorage {
                roots: StorageRoots::from_app_data_dir(&root),
                store: Mutex::new(store),
            },
            root,
        )
    }

    fn seed_proof_task(storage: &ManagedStorage) {
        let store = storage.store.lock().expect("lock storage");
        let connection = store.connection();
        TaskRepository::new(connection)
            .create(NewTask {
                id: "task-s12-proof",
                title: "S12 proof task",
                description: "Generate proof pack",
                task_type: "custom",
                status: "completed",
                repository_path: "D:/codemax",
                worktree_path: Some("D:/codemax/.worktrees/task-s12-proof"),
                branch_name: Some("codex/task-s12-proof"),
                model_id: None,
            })
            .expect("create task");
        CommandRunRepository::new(connection)
            .record(NewCommandRun {
                id: "run-s12-proof",
                task_id: "task-s12-proof",
                purpose: "validation",
                command: "npm run check",
                cwd: "D:/codemax",
                status: "passed",
                stdout_path: None,
                stderr_path: None,
                exit_code: Some(0),
                duration_ms: Some(1200),
            })
            .expect("record command");
        ArtifactRepository::new(connection)
            .record_artifact(NewArtifact {
                id: "artifact-s12-proof",
                task_id: "task-s12-proof",
                changed_files: r#"[{"path":"apps/desktop/src/main.tsx"}]"#,
                diff_path: Some("D:/codemax/app-data/tasks/task-s12-proof/diff.patch"),
                test_report_path: Some("D:/codemax/app-data/tasks/task-s12-proof/report.json"),
                screenshots: "[]",
                summary: "Delivery summary",
                commit_message: "feat: proof task",
            })
            .expect("record artifact");
    }

    fn proof_pack_count(storage: &ManagedStorage) -> i64 {
        let store = storage.store.lock().expect("lock storage");
        store
            .connection()
            .query_row("SELECT COUNT(*) FROM proof_packs", [], |row| row.get(0))
            .expect("count proof packs")
    }

    fn delivery_score_count(storage: &ManagedStorage) -> i64 {
        let store = storage.store.lock().expect("lock storage");
        store
            .connection()
            .query_row("SELECT COUNT(*) FROM delivery_scores", [], |row| row.get(0))
            .expect("count delivery scores")
    }

    fn task_status(storage: &ManagedStorage, task_id: &str) -> String {
        let store = storage.store.lock().expect("lock storage");
        TaskRepository::new(store.connection())
            .get_required(task_id)
            .expect("read task")
            .status
    }
}
