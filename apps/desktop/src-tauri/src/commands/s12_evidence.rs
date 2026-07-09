use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;
use uuid::Uuid;

use crate::{
    core::error::{AppResult, CommandError},
    storage::{
        ApprovalRecord, ApprovalRepository, ArtifactFileRecord, ArtifactRecord, ArtifactRepository,
        CommandRunRecord, CommandRunRepository, ContextSourceRecord, ContextSourceRepository,
        ContractBreachRecord, ContractBreachRepository, ManagedStorage, MergeRecord,
        MergeRecordRepository, NewArtifactFile, PrivacyLedgerEntryRecord, PrivacyLedgerRepository,
        RunContractRecord, RunContractRepository, StorageError, TaskRecord, TaskRepository,
        TodoRecord, TodoRepository, TokenBudgetRecord, TokenBudgetRepository,
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
pub struct RecordRuleHitRequest {
    pub task_id: String,
    pub rule: String,
    pub status: String,
    pub message: String,
    pub evidence_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordHookRunRequest {
    pub task_id: String,
    pub hook: String,
    pub lifecycle: String,
    pub status: String,
    pub message: String,
    pub command: Option<String>,
    pub evidence_path: Option<String>,
    pub approval_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestHookApprovalRequest {
    pub task_id: String,
    pub hook: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveHookApprovalRequest {
    pub task_id: String,
    pub approval_id: String,
    pub approved: bool,
    pub reviewer: Option<String>,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordModelArenaDecisionRequest {
    pub task_id: String,
    pub status: String,
    pub selected_model: Option<String>,
    pub selected_proposal_id: Option<String>,
    pub rationale: String,
    pub compared_models: Vec<String>,
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
pub struct RuleHitRecord {
    pub id: String,
    pub task_id: String,
    pub rule: String,
    pub status: String,
    pub message: String,
    pub evidence_path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookApprovalRecord {
    pub id: String,
    pub task_id: String,
    pub hook: String,
    pub request_reason: String,
    pub status: String,
    pub reviewer: Option<String>,
    pub resolved_reason: Option<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookRunRecord {
    pub id: String,
    pub task_id: String,
    pub hook: String,
    pub lifecycle: String,
    pub status: String,
    pub message: String,
    pub command: Option<String>,
    pub evidence_path: Option<String>,
    pub approval_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelArenaDecisionRecord {
    pub id: String,
    pub task_id: String,
    pub status: String,
    pub selected_model: Option<String>,
    pub selected_proposal_id: Option<String>,
    pub rationale: String,
    pub compared_models: Vec<String>,
    pub created_at: String,
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
pub struct ProofPackFileState {
    pub file_type: String,
    pub path: String,
    pub status: String,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyLedgerSummaryState {
    pub entry_count: usize,
    pub blocked_count: usize,
    pub redacted_count: usize,
    pub sensitive_count: usize,
    pub latest_entry: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunContractSummaryState {
    pub status: String,
    pub contract_id: Option<String>,
    pub mode: Option<String>,
    pub model_id: Option<String>,
    pub permission_level: Option<String>,
    pub network_policy: Option<String>,
    pub validation_command: Option<String>,
    pub token_budget_total: Option<i64>,
    pub token_budget_per_call: Option<i64>,
    pub breach_count: usize,
    pub unresolved_breach_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenBudgetSummaryState {
    pub record_count: usize,
    pub total_tokens_estimate: i64,
    pub budget_limit: i64,
    pub budget_remaining: i64,
    pub overflow_count: usize,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleHitState {
    pub id: String,
    pub rule: String,
    pub status: String,
    pub message: String,
    pub evidence_path: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookRunState {
    pub id: String,
    pub hook: String,
    pub lifecycle: String,
    pub status: String,
    pub message: String,
    pub command: Option<String>,
    pub evidence_path: Option<String>,
    pub approval_id: Option<String>,
    pub approval_status: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelArenaDecisionState {
    pub status: String,
    pub selected_model: Option<String>,
    pub selected_proposal_id: Option<String>,
    pub rationale: String,
    pub compared_models: Vec<String>,
    pub created_at: Option<String>,
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
    pub proof_pack_files: Vec<ProofPackFileState>,
    pub privacy_ledger_summary: PrivacyLedgerSummaryState,
    pub run_contract_summary: RunContractSummaryState,
    pub token_budget_summary: TokenBudgetSummaryState,
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
    pub proof_pack_files: Vec<ProofPackFileState>,
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
    proof_pack_files: Vec<ProofPackFileState>,
    privacy_ledger_summary: PrivacyLedgerSummaryState,
    run_contract_summary: RunContractSummaryState,
    token_budget_summary: TokenBudgetSummaryState,
    rule_hits: Vec<RuleHitState>,
    hook_runs: Vec<HookRunState>,
    model_arena_decision: ModelArenaDecisionState,
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

#[derive(Debug, Clone)]
struct RuleHitRow {
    id: String,
    rule_id: String,
    status: String,
    message: String,
    evidence_path: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone)]
struct HookApprovalRow {
    id: String,
    hook_id: String,
    status: String,
    request_reason: String,
    reviewer: Option<String>,
    resolved_reason: Option<String>,
    created_at: String,
    resolved_at: Option<String>,
}

#[derive(Debug, Clone)]
struct HookRunRow {
    id: String,
    hook_id: String,
    lifecycle: String,
    status: String,
    message: String,
    command: Option<String>,
    evidence_path: Option<String>,
    approval_id: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone)]
struct ModelArenaDecisionRow {
    status: String,
    selected_model: Option<String>,
    selected_proposal_id: Option<String>,
    rationale: String,
    compared_models: Vec<String>,
    created_at: String,
}

#[derive(Debug)]
struct DeliveryReviewSnapshot {
    task: TaskRecord,
    todos: Vec<TodoRecord>,
    commands: Vec<CommandRunRecord>,
    artifacts: Vec<ArtifactRecord>,
    artifact_files: Vec<ArtifactFileRecord>,
    approvals: Vec<ApprovalRecord>,
    approvals_blocked: bool,
    merge_records: Vec<MergeRecord>,
    run_contract: Option<RunContractRecord>,
    contract_breaches: Vec<ContractBreachRecord>,
    privacy_entries: Vec<PrivacyLedgerEntryRecord>,
    token_budget_records: Vec<TokenBudgetRecord>,
    context_sources: Vec<ContextSourceRecord>,
    quality_gates: Vec<QualityGateRow>,
    proof_pack: Option<ProofPackRow>,
    delivery_score: Option<DeliveryScoreRow>,
    rule_hits: Vec<RuleHitRow>,
    hook_runs: Vec<HookRunRow>,
    hook_approvals: Vec<HookApprovalRow>,
    model_arena_decision: Option<ModelArenaDecisionRow>,
}

#[derive(Debug, Clone)]
struct ProofPackFileWrite {
    file_type: &'static str,
    path: PathBuf,
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
pub fn record_rule_hit(
    storage: State<'_, ManagedStorage>,
    request: RecordRuleHitRequest,
) -> AppResult<RuleHitRecord> {
    record_rule_hit_inner(&storage, request)
}

#[tauri::command]
pub fn record_hook_run(
    storage: State<'_, ManagedStorage>,
    request: RecordHookRunRequest,
) -> AppResult<HookRunRecord> {
    record_hook_run_inner(&storage, request)
}

#[tauri::command]
pub fn request_hook_approval(
    storage: State<'_, ManagedStorage>,
    request: RequestHookApprovalRequest,
) -> AppResult<HookApprovalRecord> {
    request_hook_approval_inner(&storage, request)
}

#[tauri::command]
pub fn resolve_hook_approval(
    storage: State<'_, ManagedStorage>,
    request: ResolveHookApprovalRequest,
) -> AppResult<HookApprovalRecord> {
    resolve_hook_approval_inner(&storage, request)
}

#[tauri::command]
pub fn record_model_arena_decision(
    storage: State<'_, ManagedStorage>,
    request: RecordModelArenaDecisionRequest,
) -> AppResult<ModelArenaDecisionRecord> {
    record_model_arena_decision_inner(&storage, request)
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
    let mut quality_gate_blockers = quality_gate_blockers_for_task(storage, &task_id)?;
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
    let proof_pack_path_text = proof_dir.to_string_lossy().to_string();
    let c_line_snapshot = load_delivery_review_snapshot(storage, &task_id)?;
    let rule_hits = build_rule_hits(
        &validation_status_for_gate,
        !changed_files.is_empty(),
        Some(&proof_pack_path_text),
        &risk_findings,
        &c_line_snapshot.quality_gates,
        &c_line_snapshot.rule_hits,
    );
    let hook_runs = build_hook_runs(
        &c_line_snapshot.commands,
        &validation_status_for_gate,
        Some(&proof_pack_path_text),
        &c_line_snapshot.hook_runs,
        &c_line_snapshot.hook_approvals,
    );
    let model_arena_decision = model_arena_decision_state(
        c_line_snapshot.model_arena_decision.as_ref(),
        &c_line_snapshot.task,
    );
    let privacy_ledger_summary = privacy_ledger_summary_state(&c_line_snapshot.privacy_entries);
    let run_contract_summary = run_contract_summary_state(
        c_line_snapshot.run_contract.as_ref(),
        &c_line_snapshot.contract_breaches,
    );
    let token_budget_summary = token_budget_summary_state(&c_line_snapshot.token_budget_records);
    quality_gate_blockers.extend(contract_blockers_from_snapshot(&c_line_snapshot));
    quality_gate_blockers.extend(privacy_blockers_from_entries(
        &c_line_snapshot.privacy_entries,
    ));
    quality_gate_blockers.extend(token_budget_blockers_from_records(
        &c_line_snapshot.token_budget_records,
    ));
    quality_gate_blockers.extend(rule_hit_blockers_from_rows(&c_line_snapshot.rule_hits));
    quality_gate_blockers.extend(hook_blockers_from_rows(
        &c_line_snapshot.hook_runs,
        &c_line_snapshot.hook_approvals,
    ));
    quality_gate_blockers = dedupe_strings(quality_gate_blockers);
    let quality_gates = build_quality_gates(
        &validation_status_for_gate,
        !changed_files.is_empty(),
        approvals_blocked,
        &quality_gate_blockers,
    );
    let generated_at = now_text();
    let proof_pack_files = planned_proof_pack_files(&proof_dir);
    write_core_proof_pack_files(
        &proof_pack_files,
        &c_line_snapshot,
        &task_title,
        &proof_pack_id,
        &generated_at,
        &changed_files,
        &commands,
        &diff_path,
        &report_path,
        &risk_findings,
        &delivery_score,
        &quality_gate_blockers,
        &quality_gates,
        &rule_hits,
        &hook_runs,
        &model_arena_decision,
        &privacy_ledger_summary,
        &run_contract_summary,
        &token_budget_summary,
    )?;
    let mut manifest = ProofPackManifest {
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
        proof_pack_files: proof_pack_file_states(&proof_pack_files),
        privacy_ledger_summary,
        run_contract_summary,
        token_budget_summary,
        rule_hits: rule_hits.clone(),
        hook_runs: hook_runs.clone(),
        model_arena_decision: model_arena_decision.clone(),
    };

    write_json(&manifest_path, &manifest)?;
    fs::write(&summary_path, proof_summary(&task_title, &manifest)).map_err(storage_error)?;
    write_json(
        &capsule_path,
        &task_capsule_document(
            &c_line_snapshot,
            &manifest,
            &proof_pack_file_states(&proof_pack_files),
        ),
    )?;
    manifest.proof_pack_files = proof_pack_file_states(&proof_pack_files);
    write_json(&manifest_path, &manifest)?;
    persist_proof_pack(
        storage,
        &task_id,
        &proof_pack_id,
        &proof_dir,
        &proof_pack_files,
        &delivery_score,
    )?;
    let _review_state = refresh_task_delivery_review_status(storage, &task_id)?;

    Ok(GeneratedTaskProofPack {
        task_id: task_id.clone(),
        artifact_id: proof_pack_id.clone(),
        generated_at: generated_at.clone(),
        proof_pack_path: proof_pack_path_text,
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
        proof_pack_files: proof_pack_file_states(&proof_pack_files),
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

pub(crate) fn record_rule_hit_inner(
    storage: &ManagedStorage,
    request: RecordRuleHitRequest,
) -> AppResult<RuleHitRecord> {
    let task_id = required_text(
        request.task_id,
        "ruleHit.taskIdRequired",
        "Task id is required.",
    )?;
    let rule = required_text(request.rule, "ruleHit.ruleRequired", "Rule id is required.")?;
    let status = required_text(
        request.status,
        "ruleHit.statusRequired",
        "Rule hit status is required.",
    )?;
    let message = required_text(
        request.message,
        "ruleHit.messageRequired",
        "Rule hit message is required.",
    )?;
    let id = format!("rule-hit-{task_id}-{rule}-{}", Uuid::new_v4());
    let created_at = now_text();

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    TaskRepository::new(connection)
        .get_required(&task_id)
        .map_err(storage_error)?;
    ensure_rule_registered(connection, &rule)?;
    connection
        .execute(
            "INSERT INTO rule_hits
             (id, task_id, rule_id, status, message, evidence_path, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                task_id,
                rule,
                status,
                message,
                request.evidence_path,
                created_at,
            ],
        )
        .map_err(storage_error)?;
    drop(store);
    let _review_state = refresh_task_delivery_review_status(storage, &task_id)?;

    Ok(RuleHitRecord {
        id,
        task_id,
        rule,
        status,
        message,
        evidence_path: request.evidence_path,
        created_at,
    })
}

pub(crate) fn record_hook_run_inner(
    storage: &ManagedStorage,
    request: RecordHookRunRequest,
) -> AppResult<HookRunRecord> {
    let task_id = required_text(
        request.task_id,
        "hookRun.taskIdRequired",
        "Task id is required.",
    )?;
    let hook = required_text(request.hook, "hookRun.hookRequired", "Hook id is required.")?;
    let lifecycle = required_text(
        request.lifecycle,
        "hookRun.lifecycleRequired",
        "Hook lifecycle is required.",
    )?;
    let status = required_text(
        request.status,
        "hookRun.statusRequired",
        "Hook run status is required.",
    )?;
    let message = required_text(
        request.message,
        "hookRun.messageRequired",
        "Hook run message is required.",
    )?;
    let id = format!("hook-run-{task_id}-{hook}-{}", Uuid::new_v4());
    let created_at = now_text();

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    TaskRepository::new(connection)
        .get_required(&task_id)
        .map_err(storage_error)?;
    if let Some(approval_id) = request.approval_id.as_deref() {
        ensure_hook_approval_exists(connection, &task_id, approval_id)?;
    }
    connection
        .execute(
            "INSERT INTO hook_runs
             (id, task_id, hook_id, lifecycle, status, message, command, evidence_path, approval_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                task_id,
                hook,
                lifecycle,
                status,
                message,
                request.command,
                request.evidence_path,
                request.approval_id,
                created_at,
            ],
        )
        .map_err(storage_error)?;
    drop(store);
    let _review_state = refresh_task_delivery_review_status(storage, &task_id)?;

    Ok(HookRunRecord {
        id,
        task_id,
        hook,
        lifecycle,
        status,
        message,
        command: request.command,
        evidence_path: request.evidence_path,
        approval_id: request.approval_id,
        created_at,
    })
}

pub(crate) fn request_hook_approval_inner(
    storage: &ManagedStorage,
    request: RequestHookApprovalRequest,
) -> AppResult<HookApprovalRecord> {
    let task_id = required_text(
        request.task_id,
        "hookApproval.taskIdRequired",
        "Task id is required.",
    )?;
    let hook = required_text(
        request.hook,
        "hookApproval.hookRequired",
        "Hook id is required.",
    )?;
    let reason = required_text(
        request.reason,
        "hookApproval.reasonRequired",
        "Hook approval reason is required.",
    )?;
    let id = format!("hook-approval-{task_id}-{hook}-{}", Uuid::new_v4());
    let created_at = now_text();

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    TaskRepository::new(connection)
        .get_required(&task_id)
        .map_err(storage_error)?;
    connection
        .execute(
            "INSERT INTO hook_approvals
             (id, task_id, hook_id, request_reason, status, reviewer, resolved_reason, created_at, resolved_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', NULL, NULL, ?5, NULL)",
            params![id, task_id, hook, reason, created_at],
        )
        .map_err(storage_error)?;
    drop(store);
    let _review_state = refresh_task_delivery_review_status(storage, &task_id)?;

    Ok(HookApprovalRecord {
        id,
        task_id,
        hook,
        request_reason: reason,
        status: "pending".to_string(),
        reviewer: None,
        resolved_reason: None,
        created_at,
        resolved_at: None,
    })
}

pub(crate) fn resolve_hook_approval_inner(
    storage: &ManagedStorage,
    request: ResolveHookApprovalRequest,
) -> AppResult<HookApprovalRecord> {
    let task_id = required_text(
        request.task_id,
        "hookApproval.taskIdRequired",
        "Task id is required.",
    )?;
    let approval_id = required_text(
        request.approval_id,
        "hookApproval.idRequired",
        "Hook approval id is required.",
    )?;
    let reason = required_text(
        request.reason,
        "hookApproval.resolveReasonRequired",
        "Hook approval decision reason is required.",
    )?;
    let status = if request.approved {
        "approved"
    } else {
        "rejected"
    };
    let reviewer = request
        .reviewer
        .and_then(|value| {
            let value = value.trim().to_string();
            if value.is_empty() {
                None
            } else {
                Some(value)
            }
        })
        .unwrap_or_else(|| "local-reviewer".to_string());
    let resolved_at = now_text();

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    TaskRepository::new(connection)
        .get_required(&task_id)
        .map_err(storage_error)?;
    let updated = connection
        .execute(
            "UPDATE hook_approvals
             SET status = ?3, reviewer = ?4, resolved_reason = ?5, resolved_at = ?6
             WHERE task_id = ?1 AND id = ?2",
            params![task_id, approval_id, status, reviewer, reason, resolved_at],
        )
        .map_err(storage_error)?;
    if updated == 0 {
        return Err(CommandError::new(
            "hookApproval.notFound",
            format!("Hook approval {approval_id} was not found."),
        ));
    }
    let record = load_hook_approval_record(connection, &task_id, &approval_id)?;
    drop(store);
    let _review_state = refresh_task_delivery_review_status(storage, &task_id)?;

    Ok(record)
}

pub(crate) fn record_model_arena_decision_inner(
    storage: &ManagedStorage,
    request: RecordModelArenaDecisionRequest,
) -> AppResult<ModelArenaDecisionRecord> {
    let task_id = required_text(
        request.task_id,
        "modelArena.taskIdRequired",
        "Task id is required.",
    )?;
    let status = required_text(
        request.status,
        "modelArena.statusRequired",
        "Model Arena decision status is required.",
    )?;
    let rationale = required_text(
        request.rationale,
        "modelArena.rationaleRequired",
        "Model Arena rationale is required.",
    )?;
    let id = format!("model-arena-{task_id}-{}", Uuid::new_v4());
    let created_at = now_text();
    let compared_models_json =
        serde_json::to_string(&request.compared_models).map_err(json_error)?;

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    TaskRepository::new(connection)
        .get_required(&task_id)
        .map_err(storage_error)?;
    connection
        .execute(
            "INSERT INTO model_arena_decisions
             (id, task_id, status, selected_model, selected_proposal_id, rationale, compared_models_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                task_id,
                status,
                request.selected_model,
                request.selected_proposal_id,
                rationale,
                compared_models_json,
                created_at,
            ],
        )
        .map_err(storage_error)?;
    drop(store);
    let _review_state = refresh_task_delivery_review_status(storage, &task_id)?;

    Ok(ModelArenaDecisionRecord {
        id,
        task_id,
        status,
        selected_model: request.selected_model,
        selected_proposal_id: request.selected_proposal_id,
        rationale,
        compared_models: request.compared_models,
        created_at,
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
    let todos = TodoRepository::new(connection)
        .list_for_task(task_id)
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
    let approvals = ApprovalRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?;
    let approvals_blocked = approvals
        .iter()
        .any(|approval| approval.decision.as_deref() != Some("approved"));
    let merge_records = MergeRecordRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?;
    let run_contract = RunContractRepository::new(connection)
        .get_for_task(task_id)
        .map_err(storage_error)?;
    let contract_breaches = ContractBreachRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?;
    let privacy_entries = PrivacyLedgerRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?;
    let token_budget_records = TokenBudgetRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?;
    let context_sources = ContextSourceRepository::new(connection)
        .list_for_task(task_id)
        .map_err(storage_error)?;
    let quality_gates = load_quality_gate_rows(connection, task_id)?;
    let proof_pack = load_latest_proof_pack(connection, task_id)?;
    let delivery_score = load_latest_delivery_score(connection, task_id)?;
    let rule_hits = load_rule_hit_rows(connection, task_id)?;
    let hook_runs = load_hook_run_rows(connection, task_id)?;
    let hook_approvals = load_hook_approval_rows(connection, task_id)?;
    let model_arena_decision = load_latest_model_arena_decision(connection, task_id)?;

    Ok(DeliveryReviewSnapshot {
        task,
        todos,
        commands,
        artifacts,
        artifact_files,
        approvals,
        approvals_blocked,
        merge_records,
        run_contract,
        contract_breaches,
        privacy_entries,
        token_budget_records,
        context_sources,
        quality_gates,
        proof_pack,
        delivery_score,
        rule_hits,
        hook_runs,
        hook_approvals,
        model_arena_decision,
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
    blockers.extend(contract_blockers_from_snapshot(&snapshot));
    blockers.extend(privacy_blockers_from_entries(&snapshot.privacy_entries));
    blockers.extend(token_budget_blockers_from_records(
        &snapshot.token_budget_records,
    ));
    blockers.extend(rule_hit_blockers_from_rows(&snapshot.rule_hits));
    blockers.extend(hook_blockers_from_rows(
        &snapshot.hook_runs,
        &snapshot.hook_approvals,
    ));

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
    let proof_pack_files = build_proof_pack_files(&snapshot.artifact_files);
    let privacy_ledger_summary = privacy_ledger_summary_state(&snapshot.privacy_entries);
    let run_contract_summary =
        run_contract_summary_state(snapshot.run_contract.as_ref(), &snapshot.contract_breaches);
    let token_budget_summary = token_budget_summary_state(&snapshot.token_budget_records);
    let rule_hits = build_rule_hits(
        &validation_status,
        !changed_files.is_empty(),
        proof_pack_path.as_deref(),
        &risk_records,
        &snapshot.quality_gates,
        &snapshot.rule_hits,
    );
    let hook_runs = build_hook_runs(
        &snapshot.commands,
        &validation_status,
        proof_pack_path.as_deref(),
        &snapshot.hook_runs,
        &snapshot.hook_approvals,
    );
    let model_arena_decision =
        model_arena_decision_state(snapshot.model_arena_decision.as_ref(), &snapshot.task);

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
        proof_pack_files,
        privacy_ledger_summary,
        run_contract_summary,
        token_budget_summary,
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

fn load_rule_hit_rows(connection: &Connection, task_id: &str) -> AppResult<Vec<RuleHitRow>> {
    let mut statement = connection
        .prepare(
            "SELECT id, rule_id, status, message, evidence_path, created_at
             FROM rule_hits
             WHERE task_id = ?1
             ORDER BY created_at ASC, id ASC",
        )
        .map_err(storage_error)?;
    let rows = statement
        .query_map(params![task_id], |row| {
            Ok(RuleHitRow {
                id: row.get(0)?,
                rule_id: row.get(1)?,
                status: row.get(2)?,
                message: row.get(3)?,
                evidence_path: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(storage_error)?;
    let mut hits = Vec::new();
    for row in rows {
        hits.push(row.map_err(storage_error)?);
    }
    Ok(hits)
}

fn load_hook_approval_rows(
    connection: &Connection,
    task_id: &str,
) -> AppResult<Vec<HookApprovalRow>> {
    let mut statement = connection
        .prepare(
            "SELECT id, hook_id, status, request_reason, reviewer, resolved_reason, created_at, resolved_at
             FROM hook_approvals
             WHERE task_id = ?1
             ORDER BY created_at ASC, id ASC",
        )
        .map_err(storage_error)?;
    let rows = statement
        .query_map(params![task_id], |row| {
            Ok(HookApprovalRow {
                id: row.get(0)?,
                hook_id: row.get(1)?,
                status: row.get(2)?,
                request_reason: row.get(3)?,
                reviewer: row.get(4)?,
                resolved_reason: row.get(5)?,
                created_at: row.get(6)?,
                resolved_at: row.get(7)?,
            })
        })
        .map_err(storage_error)?;
    let mut approvals = Vec::new();
    for row in rows {
        approvals.push(row.map_err(storage_error)?);
    }
    Ok(approvals)
}

fn load_hook_run_rows(connection: &Connection, task_id: &str) -> AppResult<Vec<HookRunRow>> {
    let mut statement = connection
        .prepare(
            "SELECT id, hook_id, lifecycle, status, message, command, evidence_path, approval_id, created_at
             FROM hook_runs
             WHERE task_id = ?1
             ORDER BY created_at ASC, id ASC",
        )
        .map_err(storage_error)?;
    let rows = statement
        .query_map(params![task_id], |row| {
            Ok(HookRunRow {
                id: row.get(0)?,
                hook_id: row.get(1)?,
                lifecycle: row.get(2)?,
                status: row.get(3)?,
                message: row.get(4)?,
                command: row.get(5)?,
                evidence_path: row.get(6)?,
                approval_id: row.get(7)?,
                created_at: row.get(8)?,
            })
        })
        .map_err(storage_error)?;
    let mut runs = Vec::new();
    for row in rows {
        runs.push(row.map_err(storage_error)?);
    }
    Ok(runs)
}

fn load_latest_model_arena_decision(
    connection: &Connection,
    task_id: &str,
) -> AppResult<Option<ModelArenaDecisionRow>> {
    connection
        .query_row(
            "SELECT status, selected_model, selected_proposal_id, rationale, compared_models_json, created_at
             FROM model_arena_decisions
             WHERE task_id = ?1
             ORDER BY created_at DESC, id DESC
             LIMIT 1",
            params![task_id],
            |row| {
                let compared_models_json: String = row.get(4)?;
                Ok(ModelArenaDecisionRow {
                    status: row.get(0)?,
                    selected_model: row.get(1)?,
                    selected_proposal_id: row.get(2)?,
                    rationale: row.get(3)?,
                    compared_models: serde_json::from_str(&compared_models_json).unwrap_or_default(),
                    created_at: row.get(5)?,
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

fn build_proof_pack_files(files: &[ArtifactFileRecord]) -> Vec<ProofPackFileState> {
    files
        .iter()
        .filter(|file| {
            matches!(
                file.file_type.as_str(),
                "proof_manifest"
                    | "proof_summary"
                    | "task_capsule"
                    | "proof_task"
                    | "proof_run_contract"
                    | "proof_privacy_ledger"
                    | "proof_todos"
                    | "proof_commands"
                    | "proof_validation_report"
                    | "proof_diff_patch"
                    | "proof_quality_gate"
                    | "proof_delivery_score"
                    | "proof_approvals"
                    | "proof_risk_report"
                    | "proof_merge_record"
                    | "proof_context_sources"
                    | "proof_rules_hooks"
                    | "proof_model_arena"
            )
        })
        .map(|file| ProofPackFileState {
            file_type: file.file_type.clone(),
            path: file.path.clone(),
            status: if file.size_bytes > 0 {
                "generated".to_string()
            } else {
                "empty".to_string()
            },
            size_bytes: file.size_bytes,
        })
        .collect()
}

fn privacy_ledger_summary_state(entries: &[PrivacyLedgerEntryRecord]) -> PrivacyLedgerSummaryState {
    let blocked_count = entries.iter().filter(|entry| entry.blocked).count();
    let redacted_count = entries.iter().filter(|entry| entry.redacted).count();
    let sensitive_count = entries
        .iter()
        .filter(|entry| entry.sensitivity_level != "none")
        .count();
    PrivacyLedgerSummaryState {
        entry_count: entries.len(),
        blocked_count,
        redacted_count,
        sensitive_count,
        latest_entry: entries.last().map(|entry| {
            format!(
                "{}:{} -> {}",
                entry.action, entry.data_kind, entry.destination
            )
        }),
        status: if blocked_count > 0 {
            "blocked"
        } else if redacted_count > 0 || sensitive_count > 0 {
            "warning"
        } else if entries.is_empty() {
            "missing"
        } else {
            "passed"
        }
        .to_string(),
    }
}

fn run_contract_summary_state(
    contract: Option<&RunContractRecord>,
    breaches: &[ContractBreachRecord],
) -> RunContractSummaryState {
    let unresolved_breach_count = breaches
        .iter()
        .filter(|breach| !matches!(breach.status.as_str(), "approved" | "resolved"))
        .count();
    RunContractSummaryState {
        status: if contract.is_none() {
            "missing"
        } else if unresolved_breach_count > 0 {
            "blocked"
        } else if breaches.is_empty() {
            "passed"
        } else {
            "warning"
        }
        .to_string(),
        contract_id: contract.map(|contract| contract.id.clone()),
        mode: contract.map(|contract| contract.mode.clone()),
        model_id: contract.and_then(|contract| contract.model_id.clone()),
        permission_level: contract.map(|contract| contract.permission_level.clone()),
        network_policy: contract.map(|contract| contract.network_policy.clone()),
        validation_command: contract.and_then(|contract| contract.validation_command.clone()),
        token_budget_total: contract.map(|contract| contract.token_budget_total),
        token_budget_per_call: contract.map(|contract| contract.token_budget_per_call),
        breach_count: breaches.len(),
        unresolved_breach_count,
    }
}

fn token_budget_summary_state(records: &[TokenBudgetRecord]) -> TokenBudgetSummaryState {
    let total_tokens_estimate = records
        .iter()
        .map(|record| record.total_tokens_estimate)
        .sum::<i64>();
    let budget_limit = records
        .iter()
        .map(|record| record.budget_limit)
        .max()
        .unwrap_or_default();
    let budget_remaining = records
        .last()
        .map(|record| record.budget_remaining)
        .unwrap_or_default();
    let overflow_count = records
        .iter()
        .filter(|record| {
            record.budget_remaining < 0
                || (record.budget_limit > 0 && record.total_tokens_estimate > record.budget_limit)
        })
        .count();

    TokenBudgetSummaryState {
        record_count: records.len(),
        total_tokens_estimate,
        budget_limit,
        budget_remaining,
        overflow_count,
        status: if overflow_count > 0 {
            "blocked"
        } else if records.is_empty() {
            "missing"
        } else {
            "passed"
        }
        .to_string(),
    }
}

fn contract_blockers_from_snapshot(snapshot: &DeliveryReviewSnapshot) -> Vec<String> {
    let mut blockers = Vec::new();
    if snapshot.run_contract.is_none() {
        blockers.push("run contract has not been generated".to_string());
    }
    blockers.extend(
        snapshot
            .contract_breaches
            .iter()
            .filter(|breach| !matches!(breach.status.as_str(), "approved" | "resolved"))
            .map(|breach| {
                format!(
                    "contract breach {} is {}: {}",
                    breach.breach_type, breach.status, breach.reason
                )
            }),
    );
    blockers
}

fn privacy_blockers_from_entries(entries: &[PrivacyLedgerEntryRecord]) -> Vec<String> {
    entries
        .iter()
        .filter(|entry| entry.blocked)
        .map(|entry| {
            format!(
                "privacy ledger blocked {} from {}: {}",
                entry.data_kind, entry.source_ref, entry.reason
            )
        })
        .collect()
}

fn token_budget_blockers_from_records(records: &[TokenBudgetRecord]) -> Vec<String> {
    records
        .iter()
        .filter(|record| {
            record.budget_remaining < 0
                || (record.budget_limit > 0 && record.total_tokens_estimate > record.budget_limit)
        })
        .map(|record| {
            format!(
                "token budget exceeded in {}: {} / {}",
                record.phase, record.total_tokens_estimate, record.budget_limit
            )
        })
        .collect()
}

fn rule_hit_blockers_from_rows(rows: &[RuleHitRow]) -> Vec<String> {
    rows.iter()
        .filter(|row| matches!(row.status.as_str(), "blocked" | "failed"))
        .map(|row| format!("rule {} is {}: {}", row.rule_id, row.status, row.message))
        .collect()
}

fn hook_blockers_from_rows(runs: &[HookRunRow], approvals: &[HookApprovalRow]) -> Vec<String> {
    let mut blockers = Vec::new();
    for approval in approvals
        .iter()
        .filter(|approval| approval.status != "approved")
    {
        blockers.push(format!(
            "hook approval {} for {} is {}",
            approval.id, approval.hook_id, approval.status
        ));
    }
    for run in runs
        .iter()
        .filter(|run| matches!(run.status.as_str(), "blocked" | "approvalRequired"))
    {
        let approval_status = run
            .approval_id
            .as_ref()
            .and_then(|approval_id| {
                approvals
                    .iter()
                    .find(|approval| approval.id == *approval_id)
            })
            .map(|approval| approval.status.as_str())
            .unwrap_or("missing");
        if approval_status != "approved" {
            blockers.push(format!(
                "hook {} is {}: {}",
                run.hook_id, run.status, run.message
            ));
        }
    }
    blockers
}

fn model_arena_decision_state(
    decision: Option<&ModelArenaDecisionRow>,
    task: &TaskRecord,
) -> ModelArenaDecisionState {
    if let Some(decision) = decision {
        return ModelArenaDecisionState {
            status: decision.status.clone(),
            selected_model: decision.selected_model.clone(),
            selected_proposal_id: decision.selected_proposal_id.clone(),
            rationale: decision.rationale.clone(),
            compared_models: decision.compared_models.clone(),
            created_at: Some(decision.created_at.clone()),
        };
    }

    ModelArenaDecisionState {
        status: "notRequired".to_string(),
        selected_model: task.model_id.clone(),
        selected_proposal_id: None,
        rationale: "No Model Arena decision has been recorded for this task.".to_string(),
        compared_models: task
            .model_id
            .as_ref()
            .map(|model_id| vec![model_id.clone()])
            .unwrap_or_default(),
        created_at: None,
    }
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
    persisted_hits: &[RuleHitRow],
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
            created_at: None,
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
            created_at: None,
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
            created_at: None,
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
            created_at: None,
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
            created_at: None,
        }
    }));
    hits.extend(persisted_hits.iter().map(|hit| RuleHitState {
        id: hit.id.clone(),
        rule: hit.rule_id.clone(),
        status: hit.status.clone(),
        message: hit.message.clone(),
        evidence_path: hit.evidence_path.clone(),
        created_at: Some(hit.created_at.clone()),
    }));
    hits
}

fn build_hook_runs(
    commands: &[CommandRunRecord],
    validation_status: &str,
    proof_pack_path: Option<&str>,
    persisted_runs: &[HookRunRow],
    hook_approvals: &[HookApprovalRow],
) -> Vec<HookRunState> {
    let validation_count = commands
        .iter()
        .filter(|run| run.purpose == "validation")
        .count();
    let mut runs = vec![
        HookRunState {
            id: "hook-validation-cycle".to_string(),
            hook: "validation_cycle".to_string(),
            lifecycle: "after_validation".to_string(),
            status: validation_status.to_string(),
            message: format!("{validation_count} validation command(s) recorded"),
            command: None,
            evidence_path: commands
                .iter()
                .rev()
                .find(|run| run.purpose == "validation")
                .and_then(|run| run.stdout_path.clone()),
            approval_id: None,
            approval_status: None,
            created_at: None,
        },
        HookRunState {
            id: "hook-proof-pack".to_string(),
            hook: "proof_pack_generator".to_string(),
            lifecycle: "before_merge".to_string(),
            status: if proof_pack_path.is_some() {
                "passed"
            } else {
                "notRun"
            }
            .to_string(),
            message: proof_pack_path
                .map(|path| format!("Proof Pack is indexed at {path}"))
                .unwrap_or_else(|| "Proof Pack generator has not produced evidence".to_string()),
            command: None,
            evidence_path: proof_pack_path.map(ToOwned::to_owned),
            approval_id: None,
            approval_status: None,
            created_at: None,
        },
    ];

    runs.extend(persisted_runs.iter().map(|run| {
        let approval_status = run.approval_id.as_ref().and_then(|approval_id| {
            hook_approvals
                .iter()
                .find(|approval| approval.id == *approval_id)
                .map(|approval| approval.status.clone())
        });

        HookRunState {
            id: run.id.clone(),
            hook: run.hook_id.clone(),
            lifecycle: run.lifecycle.clone(),
            status: run.status.clone(),
            message: run.message.clone(),
            command: run.command.clone(),
            evidence_path: run.evidence_path.clone(),
            approval_id: run.approval_id.clone(),
            approval_status,
            created_at: Some(run.created_at.clone()),
        }
    }));
    runs
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

fn ensure_rule_registered(connection: &Connection, rule: &str) -> AppResult<()> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM rule_registry WHERE id = ?1",
            params![rule],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(storage_error)?
        .is_some();
    if exists {
        return Ok(());
    }

    connection
        .execute(
            "INSERT INTO rule_registry
             (id, name, category, severity, description, enabled, created_at)
             VALUES (?1, ?2, 'delivery', 'medium', ?3, 1, ?4)",
            params![
                rule,
                rule.replace('_', " "),
                format!("C-line delivery rule registered from task execution: {rule}"),
                now_text(),
            ],
        )
        .map_err(storage_error)?;
    Ok(())
}

fn ensure_hook_approval_exists(
    connection: &Connection,
    task_id: &str,
    approval_id: &str,
) -> AppResult<()> {
    let exists = connection
        .query_row(
            "SELECT 1 FROM hook_approvals WHERE task_id = ?1 AND id = ?2",
            params![task_id, approval_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(storage_error)?
        .is_some();
    if exists {
        Ok(())
    } else {
        Err(CommandError::new(
            "hookApproval.notFound",
            format!("Hook approval {approval_id} was not found."),
        ))
    }
}

fn load_hook_approval_record(
    connection: &Connection,
    task_id: &str,
    approval_id: &str,
) -> AppResult<HookApprovalRecord> {
    connection
        .query_row(
            "SELECT id, task_id, hook_id, request_reason, status, reviewer, resolved_reason, created_at, resolved_at
             FROM hook_approvals
             WHERE task_id = ?1 AND id = ?2",
            params![task_id, approval_id],
            |row| {
                Ok(HookApprovalRecord {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    hook: row.get(2)?,
                    request_reason: row.get(3)?,
                    status: row.get(4)?,
                    reviewer: row.get(5)?,
                    resolved_reason: row.get(6)?,
                    created_at: row.get(7)?,
                    resolved_at: row.get(8)?,
                })
            },
        )
        .map_err(storage_error)
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

fn planned_proof_pack_files(proof_dir: &Path) -> Vec<ProofPackFileWrite> {
    [
        ("proof_manifest", "manifest.json"),
        ("proof_task", "task.json"),
        ("proof_run_contract", "run-contract.json"),
        ("proof_privacy_ledger", "privacy-ledger.json"),
        ("proof_todos", "todos.json"),
        ("proof_commands", "commands.json"),
        ("proof_validation_report", "validation-report.json"),
        ("proof_diff_patch", "diff.patch"),
        ("proof_quality_gate", "quality-gate.json"),
        ("proof_delivery_score", "delivery-score.json"),
        ("proof_approvals", "approvals.json"),
        ("proof_risk_report", "risk-report.json"),
        ("proof_merge_record", "merge-record.json"),
        ("proof_summary", "summary.md"),
        ("task_capsule", "task-capsule.json"),
        ("proof_context_sources", "context-sources.json"),
        ("proof_rules_hooks", "rules-hooks.json"),
        ("proof_model_arena", "model-arena.json"),
    ]
    .into_iter()
    .map(|(file_type, file_name)| ProofPackFileWrite {
        file_type,
        path: proof_dir.join(file_name),
    })
    .collect()
}

#[allow(clippy::too_many_arguments)]
fn write_core_proof_pack_files(
    files: &[ProofPackFileWrite],
    snapshot: &DeliveryReviewSnapshot,
    task_title: &str,
    proof_pack_id: &str,
    generated_at: &str,
    changed_files: &[String],
    commands: &[String],
    diff_path: &Option<String>,
    report_path: &Option<String>,
    risk_findings: &[RiskFinding],
    delivery_score: &DeliveryScoreBreakdown,
    quality_gate_blockers: &[String],
    quality_gates: &[TaskProofPackGate],
    rule_hits: &[RuleHitState],
    hook_runs: &[HookRunState],
    model_arena_decision: &ModelArenaDecisionState,
    privacy_ledger_summary: &PrivacyLedgerSummaryState,
    run_contract_summary: &RunContractSummaryState,
    token_budget_summary: &TokenBudgetSummaryState,
) -> AppResult<()> {
    write_json(
        proof_file_path(files, "proof_task")?,
        &json!({
            "proofPackId": proof_pack_id,
            "generatedAt": generated_at,
            "taskTitle": task_title,
            "task": task_record_value(&snapshot.task),
            "changedFiles": changed_files,
            "diffPath": diff_path,
            "deliveryReportPath": report_path
        }),
    )?;
    write_json(
        proof_file_path(files, "proof_run_contract")?,
        &json!({
            "summary": run_contract_summary,
            "contract": snapshot.run_contract.as_ref().map(run_contract_value),
            "breaches": snapshot
                .contract_breaches
                .iter()
                .map(contract_breach_value)
                .collect::<Vec<_>>()
        }),
    )?;
    write_json(
        proof_file_path(files, "proof_privacy_ledger")?,
        &json!({
            "summary": privacy_ledger_summary,
            "entries": snapshot
                .privacy_entries
                .iter()
                .map(privacy_entry_value)
                .collect::<Vec<_>>()
        }),
    )?;
    write_json(
        proof_file_path(files, "proof_todos")?,
        &json!({
            "taskId": snapshot.task.id,
            "items": snapshot.todos.iter().map(todo_record_value).collect::<Vec<_>>()
        }),
    )?;
    write_json(
        proof_file_path(files, "proof_commands")?,
        &json!({
            "taskId": snapshot.task.id,
            "commandSummary": commands,
            "runs": snapshot
                .commands
                .iter()
                .map(command_run_value)
                .collect::<Vec<_>>()
        }),
    )?;
    write_json(
        proof_file_path(files, "proof_validation_report")?,
        &json!({
            "validationStatus": validation_status_from_runs(&snapshot.commands),
            "reportPath": report_path,
            "validationCommands": snapshot
                .commands
                .iter()
                .filter(|run| run.purpose == "validation")
                .map(command_run_value)
                .collect::<Vec<_>>(),
            "qualityGateBlockers": quality_gate_blockers
        }),
    )?;
    write_diff_patch(
        proof_file_path(files, "proof_diff_patch")?,
        diff_path.as_deref(),
        changed_files,
    )?;
    write_json(
        proof_file_path(files, "proof_quality_gate")?,
        &json!({
            "gates": quality_gates,
            "manualGates": snapshot
                .quality_gates
                .iter()
                .map(quality_gate_row_value)
                .collect::<Vec<_>>(),
            "blockers": quality_gate_blockers,
            "ruleHits": rule_hits,
            "hookRuns": hook_runs
        }),
    )?;
    write_json(
        proof_file_path(files, "proof_delivery_score")?,
        &json!({
            "score": delivery_score,
            "privacyLedgerSummary": privacy_ledger_summary,
            "runContractSummary": run_contract_summary,
            "tokenBudgetSummary": token_budget_summary
        }),
    )?;
    write_json(
        proof_file_path(files, "proof_approvals")?,
        &json!({
            "approvals": snapshot
                .approvals
                .iter()
                .map(approval_record_value)
                .collect::<Vec<_>>(),
            "hookApprovals": snapshot
                .hook_approvals
                .iter()
                .map(hook_approval_row_value)
                .collect::<Vec<_>>()
        }),
    )?;
    write_json(
        proof_file_path(files, "proof_risk_report")?,
        &json!({
            "riskFindings": risk_findings,
            "highestRiskLevel": highest_risk_level(risk_findings),
            "qualityGateBlockers": quality_gate_blockers,
            "privacyLedgerSummary": privacy_ledger_summary,
            "tokenBudgetSummary": token_budget_summary
        }),
    )?;
    write_json(
        proof_file_path(files, "proof_merge_record")?,
        &json!({
            "canMerge": quality_gate_blockers.is_empty()
                && validation_status_from_runs(&snapshot.commands) == "passed"
                && !changed_files.is_empty()
                && !snapshot.approvals_blocked,
            "blockers": quality_gate_blockers,
            "records": snapshot
                .merge_records
                .iter()
                .map(merge_record_value)
                .collect::<Vec<_>>()
        }),
    )?;
    write_json(
        proof_file_path(files, "proof_context_sources")?,
        &json!({
            "tokenBudgetSummary": token_budget_summary,
            "tokenBudgetRecords": snapshot
                .token_budget_records
                .iter()
                .map(token_budget_record_value)
                .collect::<Vec<_>>(),
            "contextSources": snapshot
                .context_sources
                .iter()
                .map(context_source_value)
                .collect::<Vec<_>>()
        }),
    )?;
    write_json(
        proof_file_path(files, "proof_rules_hooks")?,
        &json!({
            "ruleHits": rule_hits,
            "hookRuns": hook_runs,
            "hookApprovals": snapshot
                .hook_approvals
                .iter()
                .map(hook_approval_row_value)
                .collect::<Vec<_>>()
        }),
    )?;
    write_json(
        proof_file_path(files, "proof_model_arena")?,
        &json!({
            "decision": model_arena_decision,
            "taskModel": snapshot.task.model_id
        }),
    )?;
    Ok(())
}

fn task_capsule_document(
    snapshot: &DeliveryReviewSnapshot,
    manifest: &ProofPackManifest,
    files: &[ProofPackFileState],
) -> Value {
    json!({
        "task": task_record_value(&snapshot.task),
        "summary": {
            "proofPackId": manifest.proof_pack_id,
            "generatedAt": manifest.generated_at,
            "changedFileCount": manifest.changed_files.len(),
            "commandCount": manifest.commands.len(),
            "score": manifest.delivery_score.score,
            "riskLevel": manifest.delivery_score.risk_level,
            "qualityBlockerCount": manifest.quality_gate_blockers.len()
        },
        "keyDecisions": [
            {
                "kind": "qualityGate",
                "status": if manifest.quality_gate_blockers.is_empty() { "passed" } else { "blocked" },
                "reason": if manifest.quality_gate_blockers.is_empty() {
                    "Quality gate has no unresolved blockers.".to_string()
                } else {
                    manifest.quality_gate_blockers.join("; ")
                }
            },
            {
                "kind": "modelArena",
                "status": manifest.model_arena_decision.status,
                "selectedModel": manifest.model_arena_decision.selected_model,
                "selectedProposalId": manifest.model_arena_decision.selected_proposal_id,
                "rationale": manifest.model_arena_decision.rationale
            }
        ],
        "artifactIndex": files,
        "validationCommands": snapshot
            .commands
            .iter()
            .filter(|run| run.purpose == "validation")
            .map(command_run_value)
            .collect::<Vec<_>>(),
        "riskAndScore": {
            "deliveryScore": manifest.delivery_score,
            "riskFindings": manifest.risk_findings,
            "privacyLedgerSummary": manifest.privacy_ledger_summary,
            "runContractSummary": manifest.run_contract_summary,
            "tokenBudgetSummary": manifest.token_budget_summary
        }
    })
}

fn proof_file_path<'a>(files: &'a [ProofPackFileWrite], file_type: &str) -> AppResult<&'a Path> {
    files
        .iter()
        .find(|file| file.file_type == file_type)
        .map(|file| file.path.as_path())
        .ok_or_else(|| {
            CommandError::new(
                "proofPack.fileMissing",
                format!("Proof Pack file mapping {file_type} is missing."),
            )
        })
}

fn proof_pack_file_states(files: &[ProofPackFileWrite]) -> Vec<ProofPackFileState> {
    files
        .iter()
        .map(|file| match fs::metadata(&file.path) {
            Ok(metadata) => ProofPackFileState {
                file_type: file.file_type.to_string(),
                path: file.path.to_string_lossy().to_string(),
                status: if metadata.len() > 0 {
                    "generated".to_string()
                } else {
                    "empty".to_string()
                },
                size_bytes: metadata.len() as i64,
            },
            Err(_) => ProofPackFileState {
                file_type: file.file_type.to_string(),
                path: file.path.to_string_lossy().to_string(),
                status: "missing".to_string(),
                size_bytes: 0,
            },
        })
        .collect()
}

fn write_diff_patch(
    path: &Path,
    diff_path: Option<&str>,
    changed_files: &[String],
) -> AppResult<()> {
    let content = diff_path
        .and_then(|source| fs::read_to_string(source).ok())
        .map(|raw| sanitize_evidence_text(&raw))
        .unwrap_or_else(|| {
            let mut fallback = String::from(
                "# CodeMax sanitized diff reference\n# Original diff content was not available in local artifact storage.\n",
            );
            for file in changed_files {
                fallback.push_str(&format!("diff --codemax-placeholder {file}\n"));
            }
            fallback
        });
    fs::write(path, content).map_err(storage_error)
}

fn task_record_value(task: &TaskRecord) -> Value {
    json!({
        "id": task.id,
        "title": task.title,
        "description": task.description,
        "taskType": task.task_type,
        "status": task.status,
        "repositoryPath": task.repository_path,
        "worktreePath": task.worktree_path,
        "branchName": task.branch_name,
        "modelId": task.model_id,
        "createdAt": task.created_at,
        "updatedAt": task.updated_at,
        "completedAt": task.completed_at
    })
}

fn todo_record_value(todo: &TodoRecord) -> Value {
    json!({
        "id": todo.id,
        "taskId": todo.task_id,
        "title": todo.title,
        "description": todo.description,
        "status": todo.status,
        "startedAt": todo.started_at,
        "completedAt": todo.completed_at,
        "errorMessage": todo.error_message
    })
}

fn command_run_value(run: &CommandRunRecord) -> Value {
    json!({
        "id": run.id,
        "taskId": run.task_id,
        "purpose": run.purpose,
        "command": sanitize_evidence_text(&run.command),
        "cwd": run.cwd,
        "status": run.status,
        "stdoutPath": run.stdout_path,
        "stderrPath": run.stderr_path,
        "exitCode": run.exit_code,
        "durationMs": run.duration_ms,
        "createdAt": run.created_at
    })
}

fn approval_record_value(approval: &ApprovalRecord) -> Value {
    json!({
        "id": approval.id,
        "taskId": approval.task_id,
        "approvalType": approval.approval_type,
        "riskLevel": approval.risk_level,
        "content": sanitize_evidence_text(&approval.content),
        "reason": approval.reason,
        "decision": approval.decision,
        "comment": approval.comment,
        "createdAt": approval.created_at,
        "decidedAt": approval.decided_at
    })
}

fn hook_approval_row_value(approval: &HookApprovalRow) -> Value {
    json!({
        "id": approval.id,
        "hook": approval.hook_id,
        "status": approval.status,
        "requestReason": approval.request_reason,
        "reviewer": approval.reviewer,
        "resolvedReason": approval.resolved_reason,
        "createdAt": approval.created_at,
        "resolvedAt": approval.resolved_at
    })
}

fn merge_record_value(record: &MergeRecord) -> Value {
    json!({
        "id": record.id,
        "taskId": record.task_id,
        "status": record.status,
        "targetBranch": record.target_branch,
        "sourceBranch": record.source_branch,
        "commitSha": record.commit_sha,
        "commitMessage": record.commit_message,
        "conflictFiles": record.conflict_files,
        "errorReason": record.error_reason,
        "recordPath": record.record_path,
        "createdAt": record.created_at
    })
}

fn run_contract_value(contract: &RunContractRecord) -> Value {
    json!({
        "id": contract.id,
        "taskId": contract.task_id,
        "profileId": contract.profile_id,
        "mode": contract.mode,
        "modelId": contract.model_id,
        "reasoningEffort": contract.reasoning_effort,
        "permissionLevel": contract.permission_level,
        "networkPolicy": contract.network_policy,
        "allowedPathsJson": contract.allowed_paths_json,
        "allowedCommandsJson": contract.allowed_commands_json,
        "validationCommand": contract.validation_command,
        "tokenBudgetTotal": contract.token_budget_total,
        "tokenBudgetPerCall": contract.token_budget_per_call,
        "outputLanguage": contract.output_language,
        "memoryScope": contract.memory_scope,
        "budgetOverflowPolicy": contract.budget_overflow_policy,
        "contractJson": sanitize_evidence_text(&contract.contract_json),
        "createdAt": contract.created_at,
        "updatedAt": contract.updated_at
    })
}

fn contract_breach_value(breach: &ContractBreachRecord) -> Value {
    json!({
        "id": breach.id,
        "taskId": breach.task_id,
        "contractId": breach.contract_id,
        "breachType": breach.breach_type,
        "requestedValue": sanitize_evidence_text(&breach.requested_value),
        "policyValue": breach.policy_value,
        "status": breach.status,
        "approvalId": breach.approval_id,
        "reason": breach.reason,
        "createdAt": breach.created_at
    })
}

fn privacy_entry_value(entry: &PrivacyLedgerEntryRecord) -> Value {
    json!({
        "id": entry.id,
        "taskId": entry.task_id,
        "eventType": entry.event_type,
        "dataKind": entry.data_kind,
        "sourceType": entry.source_type,
        "sourceRef": entry.source_ref,
        "destination": entry.destination,
        "provider": entry.provider,
        "modelId": entry.model_id,
        "action": entry.action,
        "sensitivityLevel": entry.sensitivity_level,
        "findingCount": privacy_finding_count(&entry.findings_json),
        "redacted": entry.redacted,
        "blocked": entry.blocked,
        "reason": entry.reason,
        "sizeBytes": entry.size_bytes,
        "createdAt": entry.created_at
    })
}

fn token_budget_record_value(record: &TokenBudgetRecord) -> Value {
    json!({
        "id": record.id,
        "taskId": record.task_id,
        "runId": record.run_id,
        "callType": record.call_type,
        "provider": record.provider,
        "modelId": record.model_id,
        "phase": record.phase,
        "inputTokensEstimate": record.input_tokens_estimate,
        "outputTokensEstimate": record.output_tokens_estimate,
        "totalTokensEstimate": record.total_tokens_estimate,
        "budgetLimit": record.budget_limit,
        "budgetRemaining": record.budget_remaining,
        "overflowPolicy": record.overflow_policy,
        "qualityFallback": record.quality_fallback,
        "createdAt": record.created_at
    })
}

fn context_source_value(source: &ContextSourceRecord) -> Value {
    json!({
        "id": source.id,
        "taskId": source.task_id,
        "runId": source.run_id,
        "sourceType": source.source_type,
        "sourceRef": source.source_ref,
        "layer": source.layer,
        "included": source.included,
        "tokensEstimate": source.tokens_estimate,
        "sensitivityLevel": source.sensitivity_level,
        "redacted": source.redacted,
        "blocked": source.blocked,
        "reason": source.reason,
        "createdAt": source.created_at
    })
}

fn quality_gate_row_value(gate: &QualityGateRow) -> Value {
    json!({
        "id": gate.id,
        "gateType": gate.gate_type,
        "status": gate.status,
        "message": gate.message,
        "evidencePath": gate.evidence_path,
        "overrideReason": gate.override_reason
    })
}

fn privacy_finding_count(raw: &str) -> usize {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|value| value.as_array().map(Vec::len))
        .unwrap_or_default()
}

fn sanitize_evidence_text(value: &str) -> String {
    value
        .lines()
        .map(|line| {
            let lower = line.to_lowercase();
            if [
                "api_key",
                "apikey",
                "password",
                "secret",
                "authorization:",
                "bearer ",
                "token=",
                "token:",
            ]
            .iter()
            .any(|marker| lower.contains(marker))
            {
                "[REDACTED sensitive evidence line]".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn proof_summary(task_title: &str, manifest: &ProofPackManifest) -> String {
    format!(
        "# Proof Pack\n\nTask: {task_title}\n\nScore: {}\n\nRisk: {}\n\nChanged files: {}\n\nQuality blockers: {}\n\nPrivacy: {} entries, {} blocked\n\nRun contract: {}\n\nToken budget: {} / {}, remaining {}\n\nRules: {}\n\nHooks: {}\n\nModel arena: {}\n",
        manifest.delivery_score.score,
        manifest.delivery_score.risk_level,
        manifest.changed_files.len(),
        manifest.quality_gate_blockers.len(),
        manifest.privacy_ledger_summary.entry_count,
        manifest.privacy_ledger_summary.blocked_count,
        manifest.run_contract_summary.status,
        manifest.token_budget_summary.total_tokens_estimate,
        manifest.token_budget_summary.budget_limit,
        manifest.token_budget_summary.budget_remaining,
        manifest.rule_hits.len(),
        manifest.hook_runs.len(),
        manifest.model_arena_decision.status
    )
}

fn persist_proof_pack(
    storage: &ManagedStorage,
    task_id: &str,
    proof_pack_id: &str,
    proof_dir: &Path,
    files: &[ProofPackFileWrite],
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
    for file in files {
        let path = file.path.as_path();
        let file_type = file.file_type;
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
        ApprovalRepository, ArtifactRepository, CommandRunRepository, ContextSourceRepository,
        ManagedStorage, MergeRecordRepository, NewApproval, NewArtifact, NewCommandRun,
        NewContextSource, NewMergeRecord, NewPrivacyLedgerEntry, NewRunContract, NewTask, NewTodo,
        NewTokenBudgetRecord, PrivacyLedgerRepository, RunContractRepository, SqliteStore,
        StorageRoots, TaskRepository, TodoRepository, TokenBudgetRepository,
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
        for file_name in [
            "task.json",
            "run-contract.json",
            "privacy-ledger.json",
            "todos.json",
            "commands.json",
            "validation-report.json",
            "diff.patch",
            "quality-gate.json",
            "delivery-score.json",
            "approvals.json",
            "risk-report.json",
            "merge-record.json",
            "summary.md",
            "context-sources.json",
            "rules-hooks.json",
            "model-arena.json",
        ] {
            assert!(
                Path::new(&result.proof_dir).join(file_name).is_file(),
                "{file_name} should be generated"
            );
        }
        assert!(result
            .proof_pack_files
            .iter()
            .any(|file| file.file_type == "proof_privacy_ledger"));
        assert!(result
            .proof_pack_files
            .iter()
            .all(|file| file.status == "generated"));
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

    #[test]
    fn privacy_blocked_entry_blocks_delivery_review() {
        let (storage, root) = proof_pack_storage();
        seed_proof_task(&storage);

        {
            let store = storage.store.lock().expect("lock storage");
            PrivacyLedgerRepository::new(store.connection())
                .record(NewPrivacyLedgerEntry {
                    id: "privacy-blocked-secret",
                    task_id: "task-s12-proof",
                    event_type: "context.blocked",
                    data_kind: "secret",
                    source_type: "file",
                    source_ref: ".env",
                    destination: "model",
                    provider: Some("openai-compatible"),
                    model_id: Some("gpt-5-codex"),
                    action: "blocked",
                    sensitivity_level: "high",
                    findings_json: r#"[{"kind":"secret"}]"#,
                    redacted: true,
                    blocked: true,
                    reason: "Environment secret must not enter model context.",
                    size_bytes: 24,
                })
                .expect("record blocked privacy entry");
        }

        generate_task_proof_pack_inner(
            &storage,
            GenerateTaskProofPackRequest {
                task_id: "task-s12-proof".to_string(),
            },
        )
        .expect("generate proof pack");
        let state = delivery_review_state_for_task(&storage, "task-s12-proof")
            .expect("read delivery review");
        assert!(!state.can_merge);
        assert_eq!(state.privacy_ledger_summary.status, "blocked");
        assert!(state
            .blockers
            .iter()
            .any(|blocker| blocker.contains("privacy ledger blocked")));

        if root.exists() {
            fs::remove_dir_all(root).expect("clean proof test root");
        }
    }

    #[test]
    fn token_budget_overflow_blocks_delivery_review() {
        let (storage, root) = proof_pack_storage();
        seed_proof_task(&storage);

        {
            let store = storage.store.lock().expect("lock storage");
            TokenBudgetRepository::new(store.connection())
                .record(NewTokenBudgetRecord {
                    id: "token-overflow",
                    task_id: "task-s12-proof",
                    run_id: Some("run-s12-proof"),
                    call_type: "chat_completion",
                    provider: Some("openai-compatible"),
                    model_id: Some("gpt-5-codex"),
                    phase: "validation",
                    input_tokens_estimate: 1200,
                    output_tokens_estimate: 600,
                    total_tokens_estimate: 1800,
                    budget_limit: 1000,
                    budget_remaining: -800,
                    overflow_policy: "block",
                    quality_fallback: "require_manual_review",
                })
                .expect("record token overflow");
        }

        generate_task_proof_pack_inner(
            &storage,
            GenerateTaskProofPackRequest {
                task_id: "task-s12-proof".to_string(),
            },
        )
        .expect("generate proof pack");
        let state = delivery_review_state_for_task(&storage, "task-s12-proof")
            .expect("read delivery review");
        assert!(!state.can_merge);
        assert_eq!(state.token_budget_summary.status, "blocked");
        assert!(state
            .blockers
            .iter()
            .any(|blocker| blocker.contains("token budget exceeded")));

        if root.exists() {
            fs::remove_dir_all(root).expect("clean proof test root");
        }
    }

    #[test]
    fn rule_hit_blocks_delivery_review_and_enters_proof_manifest() {
        let (storage, root) = proof_pack_storage();
        seed_proof_task(&storage);

        record_rule_hit_inner(
            &storage,
            RecordRuleHitRequest {
                task_id: "task-s12-proof".to_string(),
                rule: "security_review_required".to_string(),
                status: "blocked".to_string(),
                message: "Sensitive path requires manual review.".to_string(),
                evidence_path: Some(
                    "D:/codemax/app-data/tasks/task-s12-proof/risk.json".to_string(),
                ),
            },
        )
        .expect("record rule hit");

        let proof = generate_task_proof_pack_inner(
            &storage,
            GenerateTaskProofPackRequest {
                task_id: "task-s12-proof".to_string(),
            },
        )
        .expect("generate proof pack");
        let manifest = fs::read_to_string(&proof.manifest_path).expect("read proof manifest");
        assert!(manifest.contains("security_review_required"));

        let state = delivery_review_state_for_task(&storage, "task-s12-proof")
            .expect("read delivery review");
        assert!(!state.can_merge);
        assert!(state
            .blockers
            .iter()
            .any(|blocker| blocker.contains("security_review_required")));
        assert!(state
            .rule_hits
            .iter()
            .any(|hit| hit.rule == "security_review_required"));

        if root.exists() {
            fs::remove_dir_all(root).expect("clean proof test root");
        }
    }

    #[test]
    fn hook_approval_and_model_arena_decision_are_delivery_review_state() {
        let (storage, root) = proof_pack_storage();
        seed_proof_task(&storage);

        let approval = request_hook_approval_inner(
            &storage,
            RequestHookApprovalRequest {
                task_id: "task-s12-proof".to_string(),
                hook: "before_merge_command".to_string(),
                reason: "Merge hook wants to execute a generated verification command.".to_string(),
            },
        )
        .expect("request hook approval");
        record_hook_run_inner(
            &storage,
            RecordHookRunRequest {
                task_id: "task-s12-proof".to_string(),
                hook: "before_merge_command".to_string(),
                lifecycle: "before_merge".to_string(),
                status: "approvalRequired".to_string(),
                message: "Command execution requires a second confirmation.".to_string(),
                command: Some("npm run check".to_string()),
                evidence_path: None,
                approval_id: Some(approval.id.clone()),
            },
        )
        .expect("record hook run");
        record_model_arena_decision_inner(
            &storage,
            RecordModelArenaDecisionRequest {
                task_id: "task-s12-proof".to_string(),
                status: "selected".to_string(),
                selected_model: Some("gpt-5-codex".to_string()),
                selected_proposal_id: Some("proposal-hardened".to_string()),
                rationale: "Hardened review path has better audit coverage.".to_string(),
                compared_models: vec!["gpt-5-codex".to_string(), "gpt-5-mini".to_string()],
            },
        )
        .expect("record model arena decision");
        generate_task_proof_pack_inner(
            &storage,
            GenerateTaskProofPackRequest {
                task_id: "task-s12-proof".to_string(),
            },
        )
        .expect("generate proof pack");

        let blocked = delivery_review_state_for_task(&storage, "task-s12-proof")
            .expect("read blocked delivery review");
        assert!(!blocked.can_merge);
        assert!(blocked
            .blockers
            .iter()
            .any(|blocker| blocker.contains("hook")));
        assert_eq!(blocked.model_arena_decision.status, "selected");
        assert_eq!(
            blocked.model_arena_decision.selected_proposal_id.as_deref(),
            Some("proposal-hardened")
        );

        resolve_hook_approval_inner(
            &storage,
            ResolveHookApprovalRequest {
                task_id: "task-s12-proof".to_string(),
                approval_id: approval.id,
                approved: true,
                reviewer: Some("tester".to_string()),
                reason: "Command is validation-only and within the task contract.".to_string(),
            },
        )
        .expect("resolve hook approval");

        let passed = delivery_review_state_for_task(&storage, "task-s12-proof")
            .expect("read passed delivery review");
        assert!(passed.can_merge);
        assert!(passed
            .hook_runs
            .iter()
            .any(|run| run.approval_status.as_deref() == Some("approved")));

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
        TodoRepository::new(connection)
            .create(NewTodo {
                id: "todo-s12-proof-1",
                task_id: "task-s12-proof",
                title: "Build delivery evidence",
                description: "Collect validation, privacy, contract, and merge evidence.",
                status: "completed",
            })
            .expect("create todo");
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
        RunContractRepository::new(connection)
            .upsert(NewRunContract {
                id: "contract-s12-proof",
                task_id: "task-s12-proof",
                profile_id: None,
                mode: "delivery_review",
                model_id: Some("gpt-5-codex"),
                reasoning_effort: "medium",
                permission_level: "workspace-write",
                network_policy: "disabled",
                allowed_paths_json: r#"["D:/codemax"]"#,
                allowed_commands_json: r#"["npm run check"]"#,
                validation_command: Some("npm run check"),
                token_budget_total: 4000,
                token_budget_per_call: 1200,
                output_language: "zh-CN",
                memory_scope: "task",
                budget_overflow_policy: "block",
                contract_json: r#"{"mode":"delivery_review","tokenBudgetTotal":4000}"#,
            })
            .expect("record run contract");
        PrivacyLedgerRepository::new(connection)
            .record(NewPrivacyLedgerEntry {
                id: "privacy-s12-proof-read",
                task_id: "task-s12-proof",
                event_type: "context.read",
                data_kind: "source_file",
                source_type: "file",
                source_ref: "apps/desktop/src/main.tsx",
                destination: "model",
                provider: Some("openai-compatible"),
                model_id: Some("gpt-5-codex"),
                action: "included",
                sensitivity_level: "none",
                findings_json: "[]",
                redacted: false,
                blocked: false,
                reason: "Source file was included as task context.",
                size_bytes: 512,
            })
            .expect("record privacy ledger");
        TokenBudgetRepository::new(connection)
            .record(NewTokenBudgetRecord {
                id: "token-s12-proof",
                task_id: "task-s12-proof",
                run_id: Some("run-s12-proof"),
                call_type: "chat_completion",
                provider: Some("openai-compatible"),
                model_id: Some("gpt-5-codex"),
                phase: "validation",
                input_tokens_estimate: 900,
                output_tokens_estimate: 250,
                total_tokens_estimate: 1150,
                budget_limit: 4000,
                budget_remaining: 2850,
                overflow_policy: "block",
                quality_fallback: "keep_context_summary",
            })
            .expect("record token budget");
        ContextSourceRepository::new(connection)
            .record(NewContextSource {
                id: "context-s12-proof",
                task_id: "task-s12-proof",
                run_id: Some("run-s12-proof"),
                source_type: "file",
                source_ref: "apps/desktop/src/main.tsx",
                layer: "file_fragment",
                included: true,
                tokens_estimate: 900,
                sensitivity_level: "none",
                redacted: false,
                blocked: false,
                reason: "Changed file fragment was needed for delivery verification.",
            })
            .expect("record context source");
        let approval = ApprovalRepository::new(connection)
            .create(NewApproval {
                id: "approval-s12-proof",
                task_id: "task-s12-proof",
                approval_type: "delivery_review",
                risk_level: "low",
                content: "Delivery evidence has been reviewed.",
                reason: "Baseline approval seed for proof pack verification.",
            })
            .expect("create approval");
        ApprovalRepository::new(connection)
            .decide(&approval.id, "approved", Some("Seed approval accepted."))
            .expect("approve seed approval");
        MergeRecordRepository::new(connection)
            .record(NewMergeRecord {
                id: "merge-s12-proof",
                task_id: "task-s12-proof",
                status: "previewed",
                target_branch: "main",
                source_branch: "codex/task-s12-proof",
                commit_sha: "",
                commit_message: "feat: proof task",
                conflict_files: "[]",
                error_reason: None,
                record_path: Some("D:/codemax/app-data/tasks/task-s12-proof/merge.json"),
            })
            .expect("record merge preview");
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
