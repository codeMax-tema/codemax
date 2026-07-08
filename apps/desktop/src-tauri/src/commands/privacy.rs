use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::{
    core::error::{AppResult, CommandError},
    storage::{
        ContextSourceRecord, ContextSourceRepository, ManagedStorage, PersonalProfileRecord,
        PersonalProfileRepository, PrivacyLedgerEntryRecord, PrivacyLedgerRepository,
        RunContractRecord, RunContractRepository, StorageError, TokenBudgetRecord,
        TokenBudgetRepository,
    },
};

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

#[tauri::command]
pub fn active_profile(storage: State<'_, ManagedStorage>) -> AppResult<ActiveProfileView> {
    let store = storage.store.lock().map_err(|_| storage_lock_error())?;
    let profile = PersonalProfileRepository::new(store.connection())
        .active_profile()
        .map_err(storage_error)?;
    Ok(ActiveProfileView::from(profile))
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
