use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;
use uuid::Uuid;

use crate::{
    core::error::{AppResult, CommandError},
    privacy::{
        record_context_observation, record_token_budget_observation, redact_known_user_paths,
        sanitize_for_model_context, ContextObservation, TokenBudgetObservation,
    },
    storage::{
        ApprovalRepository, ContextSourceRecord, ContextSourceRepository, ContractBreachRecord,
        ContractBreachRepository, ManagedStorage, MemoryItemRecord, MemoryRepository, NewApproval,
        NewContractBreachRecord, NewMemoryItem, NewPersonalProfile, NewPreferenceCandidate,
        NewTaskMemoryUsage, PersonalProfileRecord, PersonalProfileRepository,
        PreferenceCandidateRecord, PreferenceCandidateRepository, PrivacyLedgerEntryRecord,
        PrivacyLedgerRepository, RunContractRecord, RunContractRepository, StorageError,
        TaskMemoryUsageRecord, TaskMemoryUsageRepository, TokenBudgetRecord, TokenBudgetRepository,
    },
};

const MEMORY_VALUE_PREVIEW_CHARS: usize = 400;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveProfileView {
    pub id: String,
    pub name: String,
    pub scope: String,
    pub scope_id: Option<String>,
    pub mode: String,
    pub model_id: Option<String>,
    pub reasoning_effort: String,
    pub permission_level: String,
    pub network_policy: String,
    pub privacy_mode: String,
    pub token_budget_total: i64,
    pub token_budget_per_call: i64,
    pub validation_policy: String,
    pub output_language: String,
    pub memory_scope: String,
    pub quality_gate_policy: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunContractView {
    pub id: String,
    pub task_id: String,
    pub profile_id: Option<String>,
    pub mode: String,
    pub model_id: Option<String>,
    pub reasoning_effort: String,
    pub permission_level: String,
    pub network_policy: String,
    pub allowed_paths: Vec<String>,
    pub allowed_commands: Vec<String>,
    pub validation_command: Option<String>,
    pub token_budget_total: i64,
    pub token_budget_per_call: i64,
    pub output_language: String,
    pub memory_scope: String,
    pub budget_overflow_policy: String,
    pub contract: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyLedgerEntryView {
    pub id: String,
    pub task_id: String,
    pub event_type: String,
    pub data_kind: String,
    pub source_type: String,
    pub source_ref: String,
    pub destination: String,
    pub provider: Option<String>,
    pub model_id: Option<String>,
    pub action: String,
    pub sensitivity_level: String,
    pub findings: Value,
    pub redacted: bool,
    pub blocked: bool,
    pub reason: String,
    pub size_bytes: i64,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyLedgerSummaryView {
    pub task_id: String,
    pub total_entries: usize,
    pub allowed_count: usize,
    pub redacted_count: usize,
    pub blocked_count: usize,
    pub provider_count: usize,
    pub sensitivity_counts: BTreeMap<String, usize>,
    pub latest_entries: Vec<PrivacyLedgerEntryView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenBudgetRecordView {
    pub id: String,
    pub task_id: String,
    pub run_id: Option<String>,
    pub call_type: String,
    pub provider: Option<String>,
    pub model_id: Option<String>,
    pub phase: String,
    pub input_tokens_estimate: i64,
    pub output_tokens_estimate: i64,
    pub total_tokens_estimate: i64,
    pub budget_limit: i64,
    pub budget_remaining: i64,
    pub overflow_policy: String,
    pub quality_fallback: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSourceView {
    pub id: String,
    pub task_id: String,
    pub run_id: Option<String>,
    pub source_type: String,
    pub source_ref: String,
    pub layer: String,
    pub included: bool,
    pub tokens_estimate: i64,
    pub sensitivity_level: String,
    pub redacted: bool,
    pub blocked: bool,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenBudgetSummaryView {
    pub task_id: String,
    pub budget_limit: i64,
    pub used_tokens_estimate: i64,
    pub remaining_tokens_estimate: i64,
    pub record_count: usize,
    pub context_source_count: usize,
    pub included_context_source_count: usize,
    pub redacted_context_source_count: usize,
    pub blocked_context_source_count: usize,
    pub records: Vec<TokenBudgetRecordView>,
    pub context_sources: Vec<ContextSourceView>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileCreateRequest {
    pub id: Option<String>,
    pub name: String,
    pub scope: Option<String>,
    pub scope_id: Option<String>,
    pub mode: Option<String>,
    pub model_id: Option<String>,
    pub reasoning_effort: Option<String>,
    pub permission_level: Option<String>,
    pub network_policy: Option<String>,
    pub privacy_mode: Option<String>,
    pub token_budget_total: Option<i64>,
    pub token_budget_per_call: Option<i64>,
    pub validation_policy: Option<String>,
    pub output_language: Option<String>,
    pub memory_scope: Option<String>,
    pub quality_gate_policy: Option<String>,
    pub activate: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileUpdateRequest {
    pub profile_id: String,
    pub name: Option<String>,
    pub scope: Option<String>,
    pub scope_id: Option<String>,
    pub clear_scope_id: Option<bool>,
    pub mode: Option<String>,
    pub model_id: Option<String>,
    pub clear_model_id: Option<bool>,
    pub reasoning_effort: Option<String>,
    pub permission_level: Option<String>,
    pub network_policy: Option<String>,
    pub privacy_mode: Option<String>,
    pub token_budget_total: Option<i64>,
    pub token_budget_per_call: Option<i64>,
    pub validation_policy: Option<String>,
    pub output_language: Option<String>,
    pub memory_scope: Option<String>,
    pub quality_gate_policy: Option<String>,
    pub activate: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStartPreviewRequest {
    pub repository_path: String,
    pub title: Option<String>,
    pub description: String,
    pub model_id: Option<String>,
    pub validation_command: Option<String>,
    pub mode: Option<String>,
    pub reasoning_effort: Option<String>,
    pub permission_level: Option<String>,
    pub network_policy: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyPreviewSourceView {
    pub data_kind: String,
    pub source_type: String,
    pub source_ref: String,
    pub destination: String,
    pub action: String,
    pub sensitivity_level: String,
    pub findings: Value,
    pub redacted: bool,
    pub blocked: bool,
    pub included: bool,
    pub reason: String,
    pub size_bytes: i64,
    pub tokens_estimate: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyPreviewView {
    pub repository_path: String,
    pub provider: String,
    pub model_id: Option<String>,
    pub total_sources: usize,
    pub redacted_count: usize,
    pub blocked_count: usize,
    pub input_tokens_estimate: i64,
    pub sources: Vec<PrivacyPreviewSourceView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunContractPreviewView {
    pub source_profile_id: String,
    pub source_profile_name: String,
    pub mode: String,
    pub model_id: Option<String>,
    pub reasoning_effort: String,
    pub permission_level: String,
    pub network_policy: String,
    pub allowed_paths: Vec<String>,
    pub allowed_commands: Vec<String>,
    pub validation_command: Option<String>,
    pub token_budget_total: i64,
    pub token_budget_per_call: i64,
    pub output_language: String,
    pub memory_scope: String,
    pub budget_overflow_policy: String,
    pub contract: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContractBreachRecordView {
    pub id: String,
    pub task_id: String,
    pub contract_id: Option<String>,
    pub breach_type: String,
    pub requested_value: String,
    pub policy_value: String,
    pub status: String,
    pub approval_id: Option<String>,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordContractBreachRequest {
    pub task_id: String,
    pub breach_type: String,
    pub requested_value: String,
    pub policy_value: String,
    pub reason: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMemoryUsageView {
    pub id: String,
    pub task_id: String,
    pub memory_id: Option<String>,
    pub memory_key: String,
    pub memory_scope: String,
    pub memory_scope_id: Option<String>,
    pub usage_type: String,
    pub value_preview: String,
    pub tokens_estimate: i64,
    pub redacted: bool,
    pub blocked: bool,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryItemView {
    pub id: String,
    pub scope: String,
    pub scope_id: Option<String>,
    pub key: String,
    pub value: String,
    pub confidence: f64,
    pub source: String,
    pub is_user_editable: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordMemoryUsageRequest {
    pub task_id: String,
    pub memory_id: Option<String>,
    pub memory_key: String,
    pub memory_scope: String,
    pub memory_scope_id: Option<String>,
    pub usage_type: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryItemsRequest {
    pub scope: Option<String>,
    pub scope_id: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMemoryItemRequest {
    pub id: Option<String>,
    pub scope: String,
    pub scope_id: Option<String>,
    pub key: String,
    pub value: String,
    pub confidence: Option<f64>,
    pub source: Option<String>,
    pub is_user_editable: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMemoryItemRequest {
    pub memory_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceCandidatesRequest {
    pub task_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceCandidateView {
    pub id: String,
    pub task_id: Option<String>,
    pub scope: String,
    pub scope_id: Option<String>,
    pub preference_key: String,
    pub candidate_value: String,
    pub evidence: String,
    pub confidence: f64,
    pub status: String,
    pub redacted: bool,
    pub blocked: bool,
    pub reason: String,
    pub decision_comment: Option<String>,
    pub accepted_memory_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub decided_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePreferenceCandidateRequest {
    pub task_id: Option<String>,
    pub scope: String,
    pub scope_id: Option<String>,
    pub preference_key: String,
    pub candidate_value: String,
    pub evidence: Option<String>,
    pub confidence: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecidePreferenceCandidateRequest {
    pub candidate_id: String,
    pub decision: String,
    pub edited_value: Option<String>,
    pub comment: Option<String>,
}

#[tauri::command]
pub fn active_profile(storage: State<'_, ManagedStorage>) -> AppResult<ActiveProfileView> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let profile = PersonalProfileRepository::new(store.connection())
        .active_profile()
        .map_err(storage_error)?;
    Ok(ActiveProfileView::from(profile))
}

#[tauri::command]
pub fn profile_list(storage: State<'_, ManagedStorage>) -> AppResult<Vec<ActiveProfileView>> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    PersonalProfileRepository::new(store.connection())
        .list()
        .map(|profiles| profiles.into_iter().map(ActiveProfileView::from).collect())
        .map_err(storage_error)
}

#[tauri::command]
pub fn profile_create(
    storage: State<'_, ManagedStorage>,
    request: ProfileCreateRequest,
) -> AppResult<ActiveProfileView> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let profiles = PersonalProfileRepository::new(store.connection());
    let base = profiles.active_profile().map_err(storage_error)?;
    let profile_id = clean_optional(request.id.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| format!("profile-{}", Uuid::new_v4()));
    let name = required_text(
        &request.name,
        "profile.nameRequired",
        "Profile name is required.",
    )?;
    let scope = clean_optional(request.scope.as_deref()).unwrap_or(&base.scope);
    let scope_id = clean_optional(request.scope_id.as_deref()).or(base.scope_id.as_deref());
    let mode = clean_optional(request.mode.as_deref()).unwrap_or(&base.mode);
    let model_id = clean_optional(request.model_id.as_deref()).or(base.model_id.as_deref());
    let reasoning_effort =
        clean_optional(request.reasoning_effort.as_deref()).unwrap_or(&base.reasoning_effort);
    let permission_level =
        clean_optional(request.permission_level.as_deref()).unwrap_or(&base.permission_level);
    let network_policy =
        clean_optional(request.network_policy.as_deref()).unwrap_or(&base.network_policy);
    let privacy_mode =
        clean_optional(request.privacy_mode.as_deref()).unwrap_or(&base.privacy_mode);
    let validation_policy =
        clean_optional(request.validation_policy.as_deref()).unwrap_or(&base.validation_policy);
    let output_language =
        clean_optional(request.output_language.as_deref()).unwrap_or(&base.output_language);
    let memory_scope =
        clean_optional(request.memory_scope.as_deref()).unwrap_or(&base.memory_scope);
    let quality_gate_policy =
        clean_optional(request.quality_gate_policy.as_deref()).unwrap_or(&base.quality_gate_policy);
    let token_budget_total = positive_budget(
        request.token_budget_total,
        base.token_budget_total,
        "profile.tokenBudgetTotalInvalid",
    )?;
    let token_budget_per_call = positive_budget(
        request.token_budget_per_call,
        base.token_budget_per_call,
        "profile.tokenBudgetPerCallInvalid",
    )?;

    let saved = profiles
        .save(NewPersonalProfile {
            id: &profile_id,
            name,
            scope,
            scope_id,
            mode,
            model_id,
            reasoning_effort,
            permission_level,
            network_policy,
            privacy_mode,
            token_budget_total,
            token_budget_per_call,
            validation_policy,
            output_language,
            memory_scope,
            quality_gate_policy,
            is_active: false,
        })
        .map_err(storage_error)?;
    let saved = if request.activate.unwrap_or(false) {
        profiles.activate(&saved.id).map_err(storage_error)?
    } else {
        saved
    };

    Ok(ActiveProfileView::from(saved))
}

#[tauri::command]
pub fn profile_update(
    storage: State<'_, ManagedStorage>,
    request: ProfileUpdateRequest,
) -> AppResult<ActiveProfileView> {
    let profile_id = required_text(
        &request.profile_id,
        "profile.idRequired",
        "Profile id is required.",
    )?;
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let profiles = PersonalProfileRepository::new(store.connection());
    let existing = profiles.get_required(profile_id).map_err(storage_error)?;
    let name = clean_optional(request.name.as_deref()).unwrap_or(&existing.name);
    let scope = clean_optional(request.scope.as_deref()).unwrap_or(&existing.scope);
    let scope_id = if request.clear_scope_id.unwrap_or(false) {
        None
    } else {
        clean_optional(request.scope_id.as_deref()).or(existing.scope_id.as_deref())
    };
    let mode = clean_optional(request.mode.as_deref()).unwrap_or(&existing.mode);
    let model_id = if request.clear_model_id.unwrap_or(false) {
        None
    } else {
        clean_optional(request.model_id.as_deref()).or(existing.model_id.as_deref())
    };
    let reasoning_effort =
        clean_optional(request.reasoning_effort.as_deref()).unwrap_or(&existing.reasoning_effort);
    let permission_level =
        clean_optional(request.permission_level.as_deref()).unwrap_or(&existing.permission_level);
    let network_policy =
        clean_optional(request.network_policy.as_deref()).unwrap_or(&existing.network_policy);
    let privacy_mode =
        clean_optional(request.privacy_mode.as_deref()).unwrap_or(&existing.privacy_mode);
    let validation_policy =
        clean_optional(request.validation_policy.as_deref()).unwrap_or(&existing.validation_policy);
    let output_language =
        clean_optional(request.output_language.as_deref()).unwrap_or(&existing.output_language);
    let memory_scope =
        clean_optional(request.memory_scope.as_deref()).unwrap_or(&existing.memory_scope);
    let quality_gate_policy = clean_optional(request.quality_gate_policy.as_deref())
        .unwrap_or(&existing.quality_gate_policy);
    let token_budget_total = positive_budget(
        request.token_budget_total,
        existing.token_budget_total,
        "profile.tokenBudgetTotalInvalid",
    )?;
    let token_budget_per_call = positive_budget(
        request.token_budget_per_call,
        existing.token_budget_per_call,
        "profile.tokenBudgetPerCallInvalid",
    )?;

    let saved = profiles
        .save(NewPersonalProfile {
            id: &existing.id,
            name,
            scope,
            scope_id,
            mode,
            model_id,
            reasoning_effort,
            permission_level,
            network_policy,
            privacy_mode,
            token_budget_total,
            token_budget_per_call,
            validation_policy,
            output_language,
            memory_scope,
            quality_gate_policy,
            is_active: existing.is_active,
        })
        .map_err(storage_error)?;
    let saved = if request.activate.unwrap_or(false) {
        profiles.activate(&saved.id).map_err(storage_error)?
    } else {
        saved
    };

    Ok(ActiveProfileView::from(saved))
}

#[tauri::command]
pub fn profile_activate(
    storage: State<'_, ManagedStorage>,
    profile_id: String,
) -> AppResult<ActiveProfileView> {
    let profile_id = required_text(&profile_id, "profile.idRequired", "Profile id is required.")?;
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let profile = PersonalProfileRepository::new(store.connection())
        .activate(profile_id)
        .map_err(storage_error)?;
    Ok(ActiveProfileView::from(profile))
}

#[tauri::command]
pub fn privacy_preview(
    storage: State<'_, ManagedStorage>,
    request: TaskStartPreviewRequest,
) -> AppResult<PrivacyPreviewView> {
    let repository_path = required_text(
        &request.repository_path,
        "privacy.repositoryPathRequired",
        "Repository path is required for privacy preview.",
    )?;
    let description = required_text(
        &request.description,
        "privacy.descriptionRequired",
        "Task description is required for privacy preview.",
    )?;
    let title = clean_optional(request.title.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| preview_title(description));
    let validation_command = clean_optional(request.validation_command.as_deref());
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let profile = PersonalProfileRepository::new(store.connection())
        .active_profile()
        .map_err(storage_error)?;
    let model_id = clean_optional(request.model_id.as_deref())
        .map(str::to_string)
        .or(profile.model_id);
    let mut sources = Vec::new();
    sources.push(preview_source(
        "task_title",
        "user_input",
        "task.title",
        "local_task_record",
        &title,
    )?);
    sources.push(preview_source(
        "task_description",
        "user_input",
        "task.description",
        "local_task_record",
        description,
    )?);
    if let Some(command) = validation_command {
        sources.push(preview_source(
            "validation_command",
            "user_input",
            "task.validationCommand",
            "local_task_record",
            command,
        )?);
    }
    let redacted_count = sources.iter().filter(|source| source.redacted).count();
    let blocked_count = sources.iter().filter(|source| source.blocked).count();
    let input_tokens_estimate = sources
        .iter()
        .filter(|source| source.included)
        .map(|source| source.tokens_estimate)
        .sum();
    let total_sources = sources.len();

    Ok(PrivacyPreviewView {
        repository_path: redact_known_user_paths(repository_path),
        provider: "local-desktop".to_string(),
        model_id,
        total_sources,
        redacted_count,
        blocked_count,
        input_tokens_estimate,
        sources,
    })
}

#[tauri::command]
pub fn run_contract_preview(
    storage: State<'_, ManagedStorage>,
    request: TaskStartPreviewRequest,
) -> AppResult<RunContractPreviewView> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let profile = PersonalProfileRepository::new(store.connection())
        .active_profile()
        .map_err(storage_error)?;
    run_contract_preview_from_request(&profile, request)
}

#[tauri::command]
pub fn run_contract(
    storage: State<'_, ManagedStorage>,
    task_id: String,
) -> AppResult<Option<RunContractView>> {
    let task_id = require_task_id(&task_id)?;
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let contract = RunContractRepository::new(store.connection())
        .get_for_task(task_id)
        .map_err(storage_error)?;
    contract.map(RunContractView::try_from).transpose()
}

#[tauri::command]
pub fn privacy_ledger_summary(
    storage: State<'_, ManagedStorage>,
    task_id: String,
) -> AppResult<PrivacyLedgerSummaryView> {
    let task_id = require_task_id(&task_id)?;
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let entries = PrivacyLedgerRepository::new(store.connection())
        .list_for_task(task_id)
        .map_err(storage_error)?;
    Ok(privacy_summary(task_id, entries))
}

#[tauri::command]
pub fn token_budget_summary(
    storage: State<'_, ManagedStorage>,
    task_id: String,
) -> AppResult<TokenBudgetSummaryView> {
    let task_id = require_task_id(&task_id)?;
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let records = TokenBudgetRepository::new(store.connection())
        .list_for_task(task_id)
        .map_err(storage_error)?;
    let sources = ContextSourceRepository::new(store.connection())
        .list_for_task(task_id)
        .map_err(storage_error)?;
    Ok(token_summary(task_id, records, sources))
}

#[tauri::command]
pub fn privacy_ledger_entries(
    storage: State<'_, ManagedStorage>,
    task_id: String,
) -> AppResult<Vec<PrivacyLedgerEntryView>> {
    let task_id = require_task_id(&task_id)?;
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    PrivacyLedgerRepository::new(store.connection())
        .list_for_task(task_id)
        .map(|entries| {
            entries
                .into_iter()
                .map(PrivacyLedgerEntryView::from)
                .collect()
        })
        .map_err(storage_error)
}

#[tauri::command]
pub fn context_sources(
    storage: State<'_, ManagedStorage>,
    task_id: String,
) -> AppResult<Vec<ContextSourceView>> {
    let task_id = require_task_id(&task_id)?;
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    ContextSourceRepository::new(store.connection())
        .list_for_task(task_id)
        .map(|sources| sources.into_iter().map(ContextSourceView::from).collect())
        .map_err(storage_error)
}

#[tauri::command]
pub fn contract_breach_records(
    storage: State<'_, ManagedStorage>,
    task_id: String,
) -> AppResult<Vec<ContractBreachRecordView>> {
    let task_id = require_task_id(&task_id)?;
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    ContractBreachRepository::new(store.connection())
        .list_for_task(task_id)
        .map(|records| {
            records
                .into_iter()
                .map(ContractBreachRecordView::from)
                .collect()
        })
        .map_err(storage_error)
}

#[tauri::command]
pub fn record_contract_breach(
    storage: State<'_, ManagedStorage>,
    request: RecordContractBreachRequest,
) -> AppResult<ContractBreachRecordView> {
    let task_id = require_task_id(&request.task_id)?;
    let breach_type = required_text(
        &request.breach_type,
        "contract.breachTypeRequired",
        "Contract breach type is required.",
    )?;
    let requested_value = required_text(
        &request.requested_value,
        "contract.requestedValueRequired",
        "Requested value is required.",
    )?;
    let policy_value = required_text(
        &request.policy_value,
        "contract.policyValueRequired",
        "Policy value is required.",
    )?;
    let status = clean_optional(request.status.as_deref()).unwrap_or("pending_approval");
    let reason = clean_optional(request.reason.as_deref())
        .unwrap_or("Run contract breach requires user approval.");
    let requested_value = redact_known_user_paths(requested_value);
    let policy_value = redact_known_user_paths(policy_value);
    let reason = redact_known_user_paths(reason);

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    let contract = RunContractRepository::new(connection)
        .get_for_task(task_id)
        .map_err(storage_error)?;
    let approval_id = if status == "pending_approval" {
        let approval_id = format!("approval-{}", Uuid::new_v4());
        ApprovalRepository::new(connection)
            .create(NewApproval {
                id: &approval_id,
                task_id,
                approval_type: "contract_breach",
                risk_level: "high",
                content: &requested_value,
                reason: &reason,
            })
            .map_err(storage_error)?;
        Some(approval_id)
    } else {
        None
    };
    let breach_id = format!("contract-breach-{}", Uuid::new_v4());
    let record = ContractBreachRepository::new(connection)
        .record(NewContractBreachRecord {
            id: &breach_id,
            task_id,
            contract_id: contract.as_ref().map(|contract| contract.id.as_str()),
            breach_type,
            requested_value: &requested_value,
            policy_value: &policy_value,
            status,
            approval_id: approval_id.as_deref(),
            reason: &reason,
        })
        .map_err(storage_error)?;

    Ok(ContractBreachRecordView::from(record))
}

#[tauri::command]
pub fn memory_items(
    storage: State<'_, ManagedStorage>,
    request: MemoryItemsRequest,
) -> AppResult<Vec<MemoryItemView>> {
    let limit = request.limit.unwrap_or(100).clamp(1, 500);
    let scope = clean_optional(request.scope.as_deref());
    let scope_id = clean_optional(request.scope_id.as_deref());
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    MemoryRepository::new(store.connection())
        .list_memory_items(scope, scope_id, limit)
        .map(|items| items.into_iter().map(MemoryItemView::from).collect())
        .map_err(storage_error)
}

#[tauri::command]
pub fn save_memory_item(
    storage: State<'_, ManagedStorage>,
    request: SaveMemoryItemRequest,
) -> AppResult<MemoryItemView> {
    let scope = required_text(
        &request.scope,
        "memory.scopeRequired",
        "Memory scope is required.",
    )?;
    let key = required_text(
        &request.key,
        "memory.keyRequired",
        "Memory key is required.",
    )?;
    let sanitized = sanitize_for_model_context(&request.value, &format!("memory.{}", key));
    if sanitized.blocked {
        return Err(CommandError::new(
            "memory.blockedValue",
            "Memory value was blocked by privacy policy.",
        ));
    }

    let memory_id = clean_optional(request.id.as_deref())
        .map(str::to_string)
        .unwrap_or_else(|| format!("memory-{}", Uuid::new_v4()));
    let source = clean_optional(request.source.as_deref())
        .unwrap_or("user_manual")
        .to_string();
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let saved = MemoryRepository::new(store.connection())
        .upsert_memory_item(NewMemoryItem {
            id: &memory_id,
            scope,
            scope_id: clean_optional(request.scope_id.as_deref()),
            key,
            value: &sanitized.content,
            confidence: request.confidence.unwrap_or(0.8).clamp(0.0, 1.0),
            source: &source,
            is_user_editable: request.is_user_editable.unwrap_or(true),
        })
        .map_err(storage_error)?;

    Ok(MemoryItemView::from(saved))
}

#[tauri::command]
pub fn delete_memory_item(
    storage: State<'_, ManagedStorage>,
    request: DeleteMemoryItemRequest,
) -> AppResult<()> {
    let memory_id = required_text(
        &request.memory_id,
        "memory.idRequired",
        "Memory item id is required.",
    )?;
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    MemoryRepository::new(store.connection())
        .delete_memory_item(memory_id)
        .map_err(storage_error)
}

#[tauri::command]
pub fn memory_used_by_task(
    storage: State<'_, ManagedStorage>,
    task_id: String,
) -> AppResult<Vec<TaskMemoryUsageView>> {
    let task_id = require_task_id(&task_id)?;
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    TaskMemoryUsageRepository::new(store.connection())
        .list_for_task(task_id)
        .map(|usages| usages.into_iter().map(TaskMemoryUsageView::from).collect())
        .map_err(storage_error)
}

#[tauri::command]
pub fn record_memory_used_by_task(
    storage: State<'_, ManagedStorage>,
    request: RecordMemoryUsageRequest,
) -> AppResult<TaskMemoryUsageView> {
    let task_id = require_task_id(&request.task_id)?;
    let memory_key = required_text(
        &request.memory_key,
        "memory.keyRequired",
        "Memory key is required.",
    )?;
    let memory_scope = required_text(
        &request.memory_scope,
        "memory.scopeRequired",
        "Memory scope is required.",
    )?;
    let usage_type = required_text(
        &request.usage_type,
        "memory.usageTypeRequired",
        "Memory usage type is required.",
    )?;
    let memory_id = clean_optional(request.memory_id.as_deref());
    let memory_scope_id = clean_optional(request.memory_scope_id.as_deref());
    let sanitized = sanitize_for_model_context(&request.value, &format!("memory.{memory_key}"));
    let value_preview = preview_text(&sanitized.content, MEMORY_VALUE_PREVIEW_CHARS);

    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    let usage_id = format!("memory-use-{}", Uuid::new_v4());
    let usage = TaskMemoryUsageRepository::new(connection)
        .record(NewTaskMemoryUsage {
            id: &usage_id,
            task_id,
            memory_id,
            memory_key,
            memory_scope,
            memory_scope_id,
            usage_type,
            value_preview: &value_preview,
            tokens_estimate: sanitized.tokens_estimate,
            redacted: sanitized.redacted,
            blocked: sanitized.blocked,
            reason: &sanitized.reason,
        })
        .map_err(storage_error)?;

    record_context_observation(
        connection,
        ContextObservation {
            task_id,
            run_id: Some("memory-use"),
            event_type: "memory_used",
            data_kind: "long_term_memory",
            source_type: "memory",
            source_ref: memory_id.unwrap_or(memory_key),
            destination: "agent_context",
            provider: Some("local-desktop"),
            model_id: None,
            layer: "memory",
        },
        &sanitized,
    )
    .map_err(storage_error)?;
    if let Ok(profile) = PersonalProfileRepository::new(connection).active_profile() {
        record_token_budget_observation(
            connection,
            TokenBudgetObservation {
                task_id,
                run_id: Some("memory-use"),
                call_type: "memory_context",
                provider: Some("local-desktop"),
                model_id: profile.model_id.as_deref(),
                phase: "memory",
                input_tokens_estimate: sanitized.tokens_estimate,
                output_tokens_estimate: 0,
                budget_limit: profile.token_budget_total,
                overflow_policy: "pause_for_approval",
                quality_fallback: "",
            },
        )
        .map_err(storage_error)?;
    }

    Ok(TaskMemoryUsageView::from(usage))
}

#[tauri::command]
pub fn preference_candidates(
    storage: State<'_, ManagedStorage>,
    request: PreferenceCandidatesRequest,
) -> AppResult<Vec<PreferenceCandidateView>> {
    let task_id = request
        .task_id
        .as_deref()
        .map(str::trim)
        .filter(|task_id| !task_id.is_empty());
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    PreferenceCandidateRepository::new(store.connection())
        .list(task_id)
        .map(|candidates| {
            candidates
                .into_iter()
                .map(PreferenceCandidateView::from)
                .collect()
        })
        .map_err(storage_error)
}

#[tauri::command]
pub fn preference_candidate_create(
    storage: State<'_, ManagedStorage>,
    request: CreatePreferenceCandidateRequest,
) -> AppResult<PreferenceCandidateView> {
    let task_id = clean_optional(request.task_id.as_deref());
    let scope = required_text(
        &request.scope,
        "preference.scopeRequired",
        "Preference scope is required.",
    )?;
    let preference_key = required_text(
        &request.preference_key,
        "preference.keyRequired",
        "Preference key is required.",
    )?;
    let candidate_value = required_text(
        &request.candidate_value,
        "preference.valueRequired",
        "Preference candidate value is required.",
    )?;
    let sanitized_value =
        sanitize_for_model_context(candidate_value, &format!("preference.{preference_key}"));
    let evidence = request.evidence.unwrap_or_default();
    let sanitized_evidence =
        sanitize_for_model_context(&evidence, &format!("preference.{preference_key}.evidence"));
    let redacted = sanitized_value.redacted || sanitized_evidence.redacted;
    let blocked = sanitized_value.blocked || sanitized_evidence.blocked;
    let reason = [
        sanitized_value.reason.as_str(),
        sanitized_evidence.reason.as_str(),
    ]
    .into_iter()
    .filter(|reason| !reason.is_empty())
    .collect::<Vec<_>>()
    .join(" ");
    let status = if blocked { "blocked" } else { "pending" };
    let candidate_id = format!("preference-{}", Uuid::new_v4());
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let candidate = PreferenceCandidateRepository::new(store.connection())
        .record(NewPreferenceCandidate {
            id: &candidate_id,
            task_id,
            scope,
            scope_id: clean_optional(request.scope_id.as_deref()),
            preference_key,
            candidate_value: &sanitized_value.content,
            evidence: &sanitized_evidence.content,
            confidence: request.confidence.unwrap_or(0.5).clamp(0.0, 1.0),
            status,
            redacted,
            blocked,
            reason: &reason,
        })
        .map_err(storage_error)?;

    Ok(PreferenceCandidateView::from(candidate))
}

#[tauri::command]
pub fn preference_candidate_decide(
    storage: State<'_, ManagedStorage>,
    request: DecidePreferenceCandidateRequest,
) -> AppResult<PreferenceCandidateView> {
    let candidate_id = required_text(
        &request.candidate_id,
        "preference.idRequired",
        "Preference candidate id is required.",
    )?;
    let decision = normalize_preference_decision(&request.decision)?;
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let connection = store.connection();
    let candidates = PreferenceCandidateRepository::new(connection);
    let candidate = candidates
        .get_required(candidate_id)
        .map_err(storage_error)?;
    let comment = request
        .comment
        .as_deref()
        .map(|comment| sanitize_for_model_context(comment, "preference.decisionComment").content)
        .and_then(|comment| clean_optional(Some(&comment)).map(str::to_string));

    let accepted_memory_id = if matches!(decision, "accepted" | "accepted_edited") {
        if candidate.blocked {
            return Err(CommandError::new(
                "preference.blockedCandidate",
                "Blocked preference candidates cannot be accepted into long-term memory.",
            ));
        }
        let memory_value = if decision == "accepted_edited" {
            let edited = request.edited_value.as_deref().ok_or_else(|| {
                CommandError::new(
                    "preference.editedValueRequired",
                    "Edited value is required for edited acceptance.",
                )
            })?;
            let sanitized = sanitize_for_model_context(
                edited,
                &format!("preference.{}.editedValue", candidate.preference_key),
            );
            if sanitized.blocked {
                return Err(CommandError::new(
                    "preference.blockedEditedValue",
                    "Edited preference value was blocked by privacy policy.",
                ));
            }
            sanitized.content
        } else {
            candidate.candidate_value.clone()
        };
        let memory_id = format!("memory-{}", Uuid::new_v4());
        MemoryRepository::new(connection)
            .upsert_memory_item(NewMemoryItem {
                id: &memory_id,
                scope: &candidate.scope,
                scope_id: candidate.scope_id.as_deref(),
                key: &candidate.preference_key,
                value: &memory_value,
                confidence: candidate.confidence,
                source: "preference_candidate",
                is_user_editable: true,
            })
            .map_err(storage_error)?;
        Some(memory_id)
    } else {
        None
    };
    let decided = candidates
        .decide(
            candidate_id,
            decision,
            comment.as_deref(),
            accepted_memory_id.as_deref(),
        )
        .map_err(storage_error)?;

    Ok(PreferenceCandidateView::from(decided))
}

fn privacy_summary(
    task_id: &str,
    entries: Vec<PrivacyLedgerEntryRecord>,
) -> PrivacyLedgerSummaryView {
    let total_entries = entries.len();
    let redacted_count = entries.iter().filter(|entry| entry.redacted).count();
    let blocked_count = entries.iter().filter(|entry| entry.blocked).count();
    let allowed_count = entries
        .iter()
        .filter(|entry| entry.action == "allowed")
        .count();
    let mut providers = BTreeMap::new();
    let mut sensitivity_counts = BTreeMap::new();
    for entry in &entries {
        if let Some(provider) = entry.provider.as_deref() {
            providers.insert(provider.to_string(), true);
        }
        *sensitivity_counts
            .entry(entry.sensitivity_level.clone())
            .or_insert(0) += 1;
    }
    let mut latest_entries = entries
        .into_iter()
        .rev()
        .take(10)
        .map(PrivacyLedgerEntryView::from)
        .collect::<Vec<_>>();
    latest_entries.reverse();

    PrivacyLedgerSummaryView {
        task_id: task_id.to_string(),
        total_entries,
        allowed_count,
        redacted_count,
        blocked_count,
        provider_count: providers.len(),
        sensitivity_counts,
        latest_entries,
    }
}

fn token_summary(
    task_id: &str,
    records: Vec<TokenBudgetRecord>,
    sources: Vec<ContextSourceRecord>,
) -> TokenBudgetSummaryView {
    let budget_limit = records
        .last()
        .map(|record| record.budget_limit)
        .unwrap_or(0);
    let remaining_tokens_estimate = records
        .last()
        .map(|record| record.budget_remaining)
        .unwrap_or(budget_limit);
    let used_tokens_estimate = records
        .iter()
        .map(|record| record.total_tokens_estimate)
        .sum();
    let included_context_source_count = sources.iter().filter(|source| source.included).count();
    let redacted_context_source_count = sources.iter().filter(|source| source.redacted).count();
    let blocked_context_source_count = sources.iter().filter(|source| source.blocked).count();
    let record_count = records.len();
    let context_source_count = sources.len();

    TokenBudgetSummaryView {
        task_id: task_id.to_string(),
        budget_limit,
        used_tokens_estimate,
        remaining_tokens_estimate,
        record_count,
        context_source_count,
        included_context_source_count,
        redacted_context_source_count,
        blocked_context_source_count,
        records: records
            .into_iter()
            .map(TokenBudgetRecordView::from)
            .collect(),
        context_sources: sources.into_iter().map(ContextSourceView::from).collect(),
    }
}

fn require_task_id(task_id: &str) -> AppResult<&str> {
    let task_id = task_id.trim();
    if task_id.is_empty() {
        Err(CommandError::new(
            "task.taskIdRequired",
            "Task id is required.",
        ))
    } else {
        Ok(task_id)
    }
}

fn required_text<'a>(value: &'a str, code: &str, message: &str) -> AppResult<&'a str> {
    let value = value.trim();
    if value.is_empty() {
        Err(CommandError::new(code, message))
    } else {
        Ok(value)
    }
}

fn clean_optional(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn positive_budget(value: Option<i64>, fallback: i64, code: &str) -> AppResult<i64> {
    let value = value.unwrap_or(fallback);
    if value <= 0 {
        Err(CommandError::new(
            code,
            "Token budget values must be greater than zero.",
        ))
    } else {
        Ok(value)
    }
}

fn preview_title(description: &str) -> String {
    description
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Untitled task")
        .chars()
        .take(80)
        .collect()
}

fn preview_source(
    data_kind: &str,
    source_type: &str,
    source_ref: &str,
    destination: &str,
    content: &str,
) -> AppResult<PrivacyPreviewSourceView> {
    let sanitized = sanitize_for_model_context(content, source_ref);
    let findings = serde_json::to_value(&sanitized.findings).map_err(|error| {
        CommandError::new(
            "privacy.invalidFindings",
            format!("Unable to encode privacy findings: {error}"),
        )
    })?;
    Ok(PrivacyPreviewSourceView {
        data_kind: data_kind.to_string(),
        source_type: source_type.to_string(),
        source_ref: redact_known_user_paths(source_ref),
        destination: destination.to_string(),
        action: sanitized.action,
        sensitivity_level: sanitized.sensitivity_level,
        findings,
        redacted: sanitized.redacted,
        blocked: sanitized.blocked,
        included: !sanitized.blocked,
        reason: sanitized.reason,
        size_bytes: sanitized.original_size_bytes,
        tokens_estimate: sanitized.tokens_estimate,
    })
}

fn preview_text(value: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if index >= max_chars {
            output.push_str("...");
            break;
        }
        output.push(character);
    }
    output
}

fn normalize_preference_decision(decision: &str) -> AppResult<&'static str> {
    match decision.trim() {
        "accepted" | "accept" => Ok("accepted"),
        "accepted_edited" | "acceptEdited" | "edited" => Ok("accepted_edited"),
        "ignored" | "ignore" => Ok("ignored"),
        "rejected" | "reject" => Ok("rejected"),
        "suppressed" | "suppress" => Ok("suppressed"),
        other => Err(CommandError::new(
            "preference.invalidDecision",
            format!("Unsupported preference decision: {other}"),
        )),
    }
}

impl From<PersonalProfileRecord> for ActiveProfileView {
    fn from(record: PersonalProfileRecord) -> Self {
        Self {
            id: record.id,
            name: record.name,
            scope: record.scope,
            scope_id: record.scope_id,
            mode: record.mode,
            model_id: record.model_id,
            reasoning_effort: record.reasoning_effort,
            permission_level: record.permission_level,
            network_policy: record.network_policy,
            privacy_mode: record.privacy_mode,
            token_budget_total: record.token_budget_total,
            token_budget_per_call: record.token_budget_per_call,
            validation_policy: record.validation_policy,
            output_language: record.output_language,
            memory_scope: record.memory_scope,
            quality_gate_policy: record.quality_gate_policy,
            is_active: record.is_active,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

impl TryFrom<RunContractRecord> for RunContractView {
    type Error = CommandError;

    fn try_from(record: RunContractRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: record.id,
            task_id: record.task_id,
            profile_id: record.profile_id,
            mode: record.mode,
            model_id: record.model_id,
            reasoning_effort: record.reasoning_effort,
            permission_level: record.permission_level,
            network_policy: record.network_policy,
            allowed_paths: parse_json_array(&record.allowed_paths_json)?,
            allowed_commands: parse_json_array(&record.allowed_commands_json)?,
            validation_command: record.validation_command,
            token_budget_total: record.token_budget_total,
            token_budget_per_call: record.token_budget_per_call,
            output_language: record.output_language,
            memory_scope: record.memory_scope,
            budget_overflow_policy: record.budget_overflow_policy,
            contract: serde_json::from_str(&record.contract_json).unwrap_or(Value::Null),
            created_at: record.created_at,
            updated_at: record.updated_at,
        })
    }
}

fn run_contract_preview_from_request(
    profile: &PersonalProfileRecord,
    request: TaskStartPreviewRequest,
) -> AppResult<RunContractPreviewView> {
    let repository_path = required_text(
        &request.repository_path,
        "contract.repositoryPathRequired",
        "Repository path is required for run contract preview.",
    )?;
    let _description = required_text(
        &request.description,
        "contract.descriptionRequired",
        "Task description is required for run contract preview.",
    )?;
    let model_id = clean_optional(request.model_id.as_deref())
        .map(str::to_string)
        .or_else(|| profile.model_id.clone());
    let validation_command =
        clean_optional(request.validation_command.as_deref()).map(str::to_string);
    let mode = clean_optional(request.mode.as_deref())
        .unwrap_or(&profile.mode)
        .to_string();
    let reasoning_effort = clean_optional(request.reasoning_effort.as_deref())
        .unwrap_or(&profile.reasoning_effort)
        .to_string();
    let permission_level = clean_optional(request.permission_level.as_deref())
        .unwrap_or(&profile.permission_level)
        .to_string();
    let network_policy = clean_optional(request.network_policy.as_deref())
        .unwrap_or(&profile.network_policy)
        .to_string();
    let allowed_paths = vec![redact_known_user_paths(repository_path)];
    let allowed_commands = validation_command
        .as_ref()
        .map(|command| vec![command.clone()])
        .unwrap_or_default();
    let contract = json!({
        "profileId": &profile.id,
        "mode": &mode,
        "modelId": &model_id,
        "reasoningEffort": &reasoning_effort,
        "permissionLevel": &permission_level,
        "networkPolicy": &network_policy,
        "allowedPaths": &allowed_paths,
        "allowedCommands": &allowed_commands,
        "validationCommand": &validation_command,
        "tokenBudgetTotal": profile.token_budget_total,
        "tokenBudgetPerCall": profile.token_budget_per_call,
        "outputLanguage": &profile.output_language,
        "memoryScope": &profile.memory_scope,
        "budgetOverflowPolicy": "pause_for_approval",
        "source": "active_profile_preview",
        "worktreePathKnown": false
    });

    Ok(RunContractPreviewView {
        source_profile_id: profile.id.clone(),
        source_profile_name: profile.name.clone(),
        mode,
        model_id,
        reasoning_effort,
        permission_level,
        network_policy,
        allowed_paths,
        allowed_commands,
        validation_command,
        token_budget_total: profile.token_budget_total,
        token_budget_per_call: profile.token_budget_per_call,
        output_language: profile.output_language.clone(),
        memory_scope: profile.memory_scope.clone(),
        budget_overflow_policy: "pause_for_approval".to_string(),
        contract,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::PersonalProfileRecord;

    #[test]
    fn run_contract_preview_prefers_request_overrides_to_active_profile() {
        let profile = PersonalProfileRecord {
            id: "profile-default".to_string(),
            name: "Default".to_string(),
            scope: "global".to_string(),
            scope_id: None,
            mode: "standard".to_string(),
            model_id: Some("model-default".to_string()),
            reasoning_effort: "balanced".to_string(),
            permission_level: "worktree_write".to_string(),
            network_policy: "approval_required".to_string(),
            privacy_mode: "standard".to_string(),
            token_budget_total: 120000,
            token_budget_per_call: 24000,
            validation_policy: "auto".to_string(),
            output_language: "zh-CN".to_string(),
            memory_scope: "task".to_string(),
            quality_gate_policy: "strict".to_string(),
            is_active: true,
            created_at: "2026-07-09T00:00:00Z".to_string(),
            updated_at: "2026-07-09T00:00:00Z".to_string(),
        };

        let preview = run_contract_preview_from_request(
            &profile,
            TaskStartPreviewRequest {
                repository_path: "D:/repo".to_string(),
                title: None,
                description: "Preview override".to_string(),
                model_id: Some("gpt-5-codex".to_string()),
                validation_command: Some("npm run check".to_string()),
                mode: Some("review".to_string()),
                reasoning_effort: Some("max".to_string()),
                permission_level: Some("read_only".to_string()),
                network_policy: Some("enabled".to_string()),
            },
        )
        .expect("build run contract preview");

        assert_eq!(preview.mode, "review");
        assert_eq!(preview.reasoning_effort, "max");
        assert_eq!(preview.permission_level, "read_only");
        assert_eq!(preview.network_policy, "enabled");
        assert_eq!(preview.model_id.as_deref(), Some("gpt-5-codex"));
    }
}

impl From<ContractBreachRecord> for ContractBreachRecordView {
    fn from(record: ContractBreachRecord) -> Self {
        Self {
            id: record.id,
            task_id: record.task_id,
            contract_id: record.contract_id,
            breach_type: record.breach_type,
            requested_value: redact_known_user_paths(&record.requested_value),
            policy_value: redact_known_user_paths(&record.policy_value),
            status: record.status,
            approval_id: record.approval_id,
            reason: record.reason,
            created_at: record.created_at,
        }
    }
}

impl From<PrivacyLedgerEntryRecord> for PrivacyLedgerEntryView {
    fn from(record: PrivacyLedgerEntryRecord) -> Self {
        Self {
            id: record.id,
            task_id: record.task_id,
            event_type: record.event_type,
            data_kind: record.data_kind,
            source_type: record.source_type,
            source_ref: record.source_ref,
            destination: record.destination,
            provider: record.provider,
            model_id: record.model_id,
            action: record.action,
            sensitivity_level: record.sensitivity_level,
            findings: serde_json::from_str(&record.findings_json).unwrap_or(Value::Array(vec![])),
            redacted: record.redacted,
            blocked: record.blocked,
            reason: record.reason,
            size_bytes: record.size_bytes,
            created_at: record.created_at,
        }
    }
}

impl From<TokenBudgetRecord> for TokenBudgetRecordView {
    fn from(record: TokenBudgetRecord) -> Self {
        Self {
            id: record.id,
            task_id: record.task_id,
            run_id: record.run_id,
            call_type: record.call_type,
            provider: record.provider,
            model_id: record.model_id,
            phase: record.phase,
            input_tokens_estimate: record.input_tokens_estimate,
            output_tokens_estimate: record.output_tokens_estimate,
            total_tokens_estimate: record.total_tokens_estimate,
            budget_limit: record.budget_limit,
            budget_remaining: record.budget_remaining,
            overflow_policy: record.overflow_policy,
            quality_fallback: record.quality_fallback,
            created_at: record.created_at,
        }
    }
}

impl From<ContextSourceRecord> for ContextSourceView {
    fn from(record: ContextSourceRecord) -> Self {
        Self {
            id: record.id,
            task_id: record.task_id,
            run_id: record.run_id,
            source_type: record.source_type,
            source_ref: record.source_ref,
            layer: record.layer,
            included: record.included,
            tokens_estimate: record.tokens_estimate,
            sensitivity_level: record.sensitivity_level,
            redacted: record.redacted,
            blocked: record.blocked,
            reason: record.reason,
            created_at: record.created_at,
        }
    }
}

impl From<MemoryItemRecord> for MemoryItemView {
    fn from(record: MemoryItemRecord) -> Self {
        Self {
            id: record.id,
            scope: record.scope,
            scope_id: record.scope_id,
            key: record.key,
            value: record.value,
            confidence: record.confidence,
            source: record.source,
            is_user_editable: record.is_user_editable,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

impl From<TaskMemoryUsageRecord> for TaskMemoryUsageView {
    fn from(record: TaskMemoryUsageRecord) -> Self {
        Self {
            id: record.id,
            task_id: record.task_id,
            memory_id: record.memory_id,
            memory_key: record.memory_key,
            memory_scope: record.memory_scope,
            memory_scope_id: record.memory_scope_id,
            usage_type: record.usage_type,
            value_preview: record.value_preview,
            tokens_estimate: record.tokens_estimate,
            redacted: record.redacted,
            blocked: record.blocked,
            reason: record.reason,
            created_at: record.created_at,
        }
    }
}

impl From<PreferenceCandidateRecord> for PreferenceCandidateView {
    fn from(record: PreferenceCandidateRecord) -> Self {
        Self {
            id: record.id,
            task_id: record.task_id,
            scope: record.scope,
            scope_id: record.scope_id,
            preference_key: record.preference_key,
            candidate_value: record.candidate_value,
            evidence: record.evidence,
            confidence: record.confidence,
            status: record.status,
            redacted: record.redacted,
            blocked: record.blocked,
            reason: record.reason,
            decision_comment: record.decision_comment,
            accepted_memory_id: record.accepted_memory_id,
            created_at: record.created_at,
            updated_at: record.updated_at,
            decided_at: record.decided_at,
        }
    }
}

fn parse_json_array(value: &str) -> AppResult<Vec<String>> {
    serde_json::from_str(value).map_err(|error| {
        CommandError::new(
            "privacy.invalidContractJson",
            format!("Stored run contract JSON is invalid: {error}"),
        )
    })
}

fn storage_lock_error() -> CommandError {
    CommandError::new(
        "storage.lockUnavailable",
        "Local storage is temporarily unavailable.",
    )
}

fn storage_error(error: StorageError) -> CommandError {
    match error {
        StorageError::NotFound(message) => CommandError::new("storage.notFound", message),
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
