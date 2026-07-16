use std::{collections::HashSet, env};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::storage::{
    ContextSourceRepository, NewContextSource, NewPrivacyLedgerEntry, NewTokenBudgetRecord,
    PrivacyLedgerRepository, StorageError, StorageResult, TokenBudgetRepository,
};

const BLOCKED_CONTEXT_PLACEHOLDER: &str =
    "[BLOCKED: sensitive content omitted before model context]";
const REDACTED_PLACEHOLDER: &str = "[REDACTED]";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SensitiveFinding {
    pub kind: String,
    pub severity: String,
    pub action: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizedContent {
    pub content: String,
    pub action: String,
    pub sensitivity_level: String,
    pub findings: Vec<SensitiveFinding>,
    pub redacted: bool,
    pub blocked: bool,
    pub reason: String,
    pub original_size_bytes: i64,
    pub tokens_estimate: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct ContextObservation<'a> {
    pub task_id: &'a str,
    pub run_id: Option<&'a str>,
    pub event_type: &'a str,
    pub data_kind: &'a str,
    pub source_type: &'a str,
    pub source_ref: &'a str,
    pub destination: &'a str,
    pub provider: Option<&'a str>,
    pub model_id: Option<&'a str>,
    pub layer: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct TokenBudgetObservation<'a> {
    pub task_id: &'a str,
    pub run_id: Option<&'a str>,
    pub call_type: &'a str,
    pub provider: Option<&'a str>,
    pub model_id: Option<&'a str>,
    pub phase: &'a str,
    pub input_tokens_estimate: i64,
    pub output_tokens_estimate: i64,
    pub budget_limit: i64,
    pub overflow_policy: &'a str,
    pub quality_fallback: &'a str,
}

pub fn sanitize_for_model_context(content: &str, source_ref: &str) -> SanitizedContent {
    let original_size_bytes = content.len() as i64;
    let mut findings = Vec::new();
    let mut redacted = redact_known_user_paths(content);
    let mut was_redacted = redacted != content;
    let mut blocked = false;

    if was_redacted {
        findings.push(finding(
            "user_home_path",
            "medium",
            "redact",
            "User home path was replaced before model context use.",
        ));
    }

    if is_sensitive_source_ref(source_ref) {
        blocked = true;
        findings.push(finding(
            "sensitive_path",
            "high",
            "block",
            "Sensitive file path is not allowed to enter model context by default.",
        ));
    }

    if contains_private_material(&redacted) {
        blocked = true;
        findings.push(finding(
            "private_key_or_certificate",
            "high",
            "block",
            "Private key or certificate material was blocked before model context use.",
        ));
    }

    if redact_assignment_secrets(&mut redacted) {
        was_redacted = true;
        findings.push(finding(
            "secret_assignment",
            "high",
            "redact",
            "Secret-like assignment was masked before model context use.",
        ));
    }

    for marker in ["sk-", "ghp_", "github_pat_", "xoxb-", "AKIA"] {
        if redact_marker_tokens(&mut redacted, marker) {
            was_redacted = true;
            findings.push(finding(
                "api_token_marker",
                "high",
                "redact",
                "API token marker was masked before model context use.",
            ));
        }
    }

    findings.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.action.cmp(&right.action))
    });
    findings.dedup_by(|left, right| left.kind == right.kind && left.action == right.action);

    let sensitivity_level = if blocked {
        "blocked"
    } else if findings.iter().any(|finding| finding.severity == "high") {
        "high"
    } else if !findings.is_empty() {
        "medium"
    } else {
        "none"
    };
    let action = if blocked {
        "blocked"
    } else if was_redacted {
        "redacted"
    } else {
        "allowed"
    };
    let reason = if findings.is_empty() {
        "No sensitive data detected.".to_string()
    } else {
        findings
            .iter()
            .map(|finding| finding.reason.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let content = if blocked {
        BLOCKED_CONTEXT_PLACEHOLDER.to_string()
    } else {
        redacted
    };
    let tokens_estimate = estimate_tokens(&content);

    SanitizedContent {
        content,
        action: action.to_string(),
        sensitivity_level: sensitivity_level.to_string(),
        findings,
        redacted: was_redacted,
        blocked,
        reason,
        original_size_bytes,
        tokens_estimate,
    }
}

pub fn record_context_observation(
    connection: &Connection,
    observation: ContextObservation<'_>,
    sanitized: &SanitizedContent,
) -> StorageResult<()> {
    let findings_json = serde_json::to_string(&sanitized.findings).map_err(json_storage_error)?;
    let source_ref = redact_known_user_paths(observation.source_ref);

    PrivacyLedgerRepository::new(connection).record(NewPrivacyLedgerEntry {
        id: &format!("privacy-{}", Uuid::new_v4()),
        task_id: observation.task_id,
        event_type: observation.event_type,
        data_kind: observation.data_kind,
        source_type: observation.source_type,
        source_ref: &source_ref,
        destination: observation.destination,
        provider: observation.provider,
        model_id: observation.model_id,
        action: &sanitized.action,
        sensitivity_level: &sanitized.sensitivity_level,
        findings_json: &findings_json,
        redacted: sanitized.redacted,
        blocked: sanitized.blocked,
        reason: &sanitized.reason,
        size_bytes: sanitized.original_size_bytes,
    })?;

    ContextSourceRepository::new(connection).record(NewContextSource {
        id: &format!("context-{}", Uuid::new_v4()),
        task_id: observation.task_id,
        run_id: observation.run_id,
        source_type: observation.source_type,
        source_ref: &source_ref,
        layer: observation.layer,
        included: !sanitized.blocked,
        tokens_estimate: sanitized.tokens_estimate,
        sensitivity_level: &sanitized.sensitivity_level,
        redacted: sanitized.redacted,
        blocked: sanitized.blocked,
        reason: &sanitized.reason,
    })?;

    Ok(())
}

pub fn record_token_budget_observation(
    connection: &Connection,
    observation: TokenBudgetObservation<'_>,
) -> StorageResult<()> {
    let total_tokens = observation.input_tokens_estimate + observation.output_tokens_estimate;
    let used_tokens = used_token_estimate(connection, observation.task_id)?;
    let budget_remaining = observation
        .budget_limit
        .saturating_sub(used_tokens)
        .saturating_sub(total_tokens)
        .max(0);

    TokenBudgetRepository::new(connection).record(NewTokenBudgetRecord {
        id: &format!("token-budget-{}", Uuid::new_v4()),
        task_id: observation.task_id,
        run_id: observation.run_id,
        call_type: observation.call_type,
        provider: observation.provider,
        model_id: observation.model_id,
        phase: observation.phase,
        input_tokens_estimate: observation.input_tokens_estimate,
        output_tokens_estimate: observation.output_tokens_estimate,
        total_tokens_estimate: total_tokens,
        budget_limit: observation.budget_limit,
        budget_remaining,
        overflow_policy: observation.overflow_policy,
        quality_fallback: observation.quality_fallback,
    })?;

    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelRequestAuditState {
    request_id: String,
    task_id: String,
    provider: String,
    model_id: String,
    phase: String,
    status: String,
    request_digest: String,
    input_tokens_estimate: i64,
    output_tokens: i64,
    total_tokens: i64,
    budget_limit: i64,
    budget_per_call: i64,
    sources: Vec<ModelRequestAuditSourceState>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelRequestAuditSourceState {
    data_kind: String,
    source_ref: String,
    action: String,
    sensitivity_level: String,
    findings: Vec<String>,
    redacted: bool,
    blocked: bool,
    size_bytes: i64,
    tokens_estimate: i64,
}

pub fn sync_model_request_audits(
    connection: &Connection,
    task_id: &str,
    state: &Value,
) -> StorageResult<()> {
    let Some(raw_audits) = state.get("modelRequestAudits") else {
        return Ok(());
    };
    let audits: Vec<ModelRequestAuditState> = serde_json::from_value(raw_audits.clone())
        .map_err(|error| invalid_model_audit(format!("invalid model request audits: {error}")))?;
    let mut request_ids = HashSet::with_capacity(audits.len());
    for audit in &audits {
        validate_model_audit(task_id, audit)?;
        if !request_ids.insert(audit.request_id.as_str()) {
            return Err(invalid_model_audit("duplicate model request audit id"));
        }
    }

    let transaction = connection.unchecked_transaction()?;
    for audit in audits {
        let budget_id = format!("token-budget-model-{}", audit.request_id);
        let counted_total = if audit.status == "succeeded" {
            audit.total_tokens
        } else {
            0
        };
        let used_before: i64 = transaction.query_row(
            "SELECT COALESCE(SUM(total_tokens_estimate), 0) FROM token_budget_records WHERE task_id = ?1 AND id <> ?2",
            params![task_id, &budget_id],
            |row| row.get(0),
        )?;
        let budget_remaining = audit
            .budget_limit
            .saturating_sub(used_before)
            .saturating_sub(counted_total)
            .max(0);
        let call_type = format!("model_request_{}", audit.status);
        transaction.execute(
            "INSERT OR IGNORE INTO token_budget_records (
                id, task_id, run_id, call_type, provider, model_id, phase,
                input_tokens_estimate, output_tokens_estimate, total_tokens_estimate,
                budget_limit, budget_remaining, overflow_policy, quality_fallback, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, datetime('now'))",
            params![
                &budget_id,
                task_id,
                &audit.request_id,
                &call_type,
                &audit.provider,
                &audit.model_id,
                &audit.phase,
                audit.input_tokens_estimate,
                audit.output_tokens,
                counted_total,
                audit.budget_limit,
                budget_remaining,
                "pause_for_approval",
                "task_intervention",
            ],
        )?;

        for (index, source) in audit.sources.iter().enumerate() {
            let ledger_id = format!("privacy-model-{}-{index}", audit.request_id);
            let findings_json = serde_json::to_string(&source.findings)
                .map_err(|error| invalid_model_audit(format!("invalid findings: {error}")))?;
            let source_ref = format!("model_request:{}:{}", audit.request_id, source.source_ref);
            let reason = match audit.status.as_str() {
                "blocked" => "Model request was blocked before the provider transport boundary.",
                "failed" => "Model request reached the audited gateway and failed without persisting payload content.",
                _ if source.redacted => "Sensitive values were redacted at the audited model gateway boundary.",
                _ => "Model request source passed the audited model gateway boundary.",
            };
            transaction.execute(
                "INSERT OR IGNORE INTO privacy_ledger_entries (
                    id, task_id, event_type, data_kind, source_type, source_ref, destination,
                    provider, model_id, action, sensitivity_level, findings_json, redacted,
                    blocked, reason, size_bytes, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, datetime('now'))",
                params![
                    &ledger_id,
                    task_id,
                    "model_request",
                    &source.data_kind,
                    "agent_model_gateway",
                    &source_ref,
                    "external_model",
                    &audit.provider,
                    &audit.model_id,
                    &source.action,
                    &source.sensitivity_level,
                    &findings_json,
                    i64::from(source.redacted),
                    i64::from(source.blocked || audit.status == "blocked"),
                    reason,
                    source.size_bytes,
                ],
            )?;
        }
    }
    transaction.commit()?;
    Ok(())
}

fn validate_model_audit(task_id: &str, audit: &ModelRequestAuditState) -> StorageResult<()> {
    if audit.task_id != task_id
        || !safe_audit_identifier(&audit.request_id, 128)
        || !safe_audit_identifier(&audit.provider, 64)
        || !safe_audit_identifier(&audit.model_id, 128)
        || !safe_audit_identifier(&audit.phase, 64)
        || !matches!(audit.status.as_str(), "succeeded" | "failed" | "blocked")
        || audit.request_digest.len() != 64
        || !audit
            .request_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || audit.input_tokens_estimate < 0
        || audit.output_tokens < 0
        || audit.total_tokens < 0
        || audit.budget_limit < 0
        || audit.budget_per_call < 0
        || audit.sources.is_empty()
        || audit.sources.len() > 512
    {
        return Err(invalid_model_audit("model request audit validation failed"));
    }
    for source in &audit.sources {
        if !safe_audit_identifier(&source.data_kind, 64)
            || !safe_source_ref(&source.source_ref)
            || !matches!(source.action.as_str(), "allowed" | "redacted" | "blocked")
            || !safe_audit_identifier(&source.sensitivity_level, 32)
            || source.size_bytes < 0
            || source.tokens_estimate < 0
            || source.findings.len() > 64
            || source
                .findings
                .iter()
                .any(|finding| !safe_audit_identifier(finding, 64))
        {
            return Err(invalid_model_audit(
                "model request audit source validation failed",
            ));
        }
    }
    Ok(())
}

fn safe_audit_identifier(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}

fn safe_source_ref(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'[' | b']' | b'.' | b'_' | b'-')
        })
}

fn invalid_model_audit(message: impl Into<String>) -> StorageError {
    StorageError::Sqlite(rusqlite::Error::InvalidParameterName(message.into()))
}

pub fn estimate_tokens(content: &str) -> i64 {
    let chars = content.chars().count() as i64;
    (chars / 4).max(1)
}

pub fn redact_known_user_paths(value: &str) -> String {
    let mut redacted = value.to_string();
    for (key, replacement) in [("USERPROFILE", "%USERPROFILE%"), ("HOME", "$HOME")] {
        if let Ok(path) = env::var(key) {
            if path.len() >= 4 {
                redacted = redacted.replace(&path, replacement);
                redacted = redacted.replace(&path.replace('\\', "/"), replacement);
            }
        }
    }
    redacted
}

fn used_token_estimate(connection: &Connection, task_id: &str) -> StorageResult<i64> {
    connection
        .query_row(
            "SELECT COALESCE(SUM(total_tokens_estimate), 0)
             FROM token_budget_records
             WHERE task_id = ?1",
            params![task_id],
            |row| row.get(0),
        )
        .map_err(StorageError::from)
}

fn is_sensitive_source_ref(source_ref: &str) -> bool {
    let normalized = source_ref.replace('\\', "/").to_ascii_lowercase();
    let name = normalized.rsplit('/').next().unwrap_or(normalized.as_str());

    name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
        || name.ends_with(".crt")
        || name.ends_with(".cer")
        || name == "id_rsa"
        || name == "id_ed25519"
}

fn contains_private_material(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("-----begin private key-----")
        || lower.contains("-----begin rsa private key-----")
        || lower.contains("-----begin openssh private key-----")
        || lower.contains("-----begin certificate-----")
}

fn redact_assignment_secrets(value: &mut String) -> bool {
    let mut changed = false;
    let mut output = Vec::new();

    for line in value.lines() {
        let lower = line.to_ascii_lowercase();
        let sensitive_keys = [
            "password",
            "passwd",
            "api_key",
            "apikey",
            "access_token",
            "secret",
            "token",
        ];
        let sensitive_key = sensitive_keys.iter().any(|key| lower.contains(key));
        let separator = line.find('=').or_else(|| line.find(':'));

        if sensitive_key {
            if let Some(index) = separator {
                let (left, right) = line.split_at(index + 1);
                if right.trim().len() >= 4 {
                    output.push(format!("{left} {REDACTED_PLACEHOLDER}"));
                    changed = true;
                    continue;
                }
            }

            let trimmed = line.trim_start();
            let indentation_len = line.len() - trimmed.len();
            if let Some(key) = sensitive_keys.iter().find(|key| {
                trimmed
                    .get(..key.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(key))
                    && trimmed.get(key.len()..).is_some_and(|suffix| {
                        suffix.starts_with(char::is_whitespace) && suffix.trim().len() >= 4
                    })
            }) {
                output.push(format!(
                    "{}{} {}",
                    &line[..indentation_len],
                    &trimmed[..key.len()],
                    REDACTED_PLACEHOLDER
                ));
                changed = true;
                continue;
            }
        }

        output.push(line.to_string());
    }

    if changed {
        *value = output.join("\n");
    }
    changed
}

fn redact_marker_tokens(value: &mut String, marker: &str) -> bool {
    let mut changed = false;
    let mut output = String::with_capacity(value.len());
    let mut index = 0;

    while let Some(offset) = value[index..].find(marker) {
        let start = index + offset;
        output.push_str(&value[index..start]);
        let mut end = start + marker.len();
        for character in value[end..].chars() {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                end += character.len_utf8();
            } else {
                break;
            }
        }
        if end > start + marker.len() + 2 {
            output.push_str(REDACTED_PLACEHOLDER);
            changed = true;
        } else {
            output.push_str(&value[start..end]);
        }
        index = end;
    }

    if changed {
        output.push_str(&value[index..]);
        *value = output;
    }

    changed
}

fn finding(kind: &str, severity: &str, action: &str, reason: &str) -> SensitiveFinding {
    SensitiveFinding {
        kind: kind.to_string(),
        severity: severity.to_string(),
        action: action.to_string(),
        reason: reason.to_string(),
    }
}

fn json_storage_error(error: serde_json::Error) -> StorageError {
    StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{NewTask, TaskRepository};

    fn seed_task(store: &crate::storage::SqliteStore, task_id: &str) {
        TaskRepository::new(store.connection())
            .create(NewTask {
                id: task_id,
                title: "Model audit persistence fixture",
                description: "Persist audited model request metadata",
                task_type: "custom",
                status: "running",
                repository_path: "D:/codemax",
                worktree_path: Some("D:/codemax/.worktrees/model-audit"),
                branch_name: Some("codex/model-audit"),
                target_branch: "main",
                workspace_kind: "git_worktree",
                source_path: "D:/codemax",
                original_write_authorized: false,
                workspace_estimated_bytes: 0,
                model_id: Some("test-model"),
            })
            .expect("seed model audit task");
    }

    #[test]
    fn scanner_redacts_api_tokens_and_secret_assignments() {
        let sanitized = sanitize_for_model_context(
            "OPENAI_API_KEY=sk-test-secret-token\npassword: hunter2",
            "task.description",
        );

        assert!(sanitized.redacted);
        assert!(!sanitized.blocked);
        assert!(!sanitized.content.contains("sk-test-secret-token"));
        assert!(!sanitized.content.contains("hunter2"));
        assert!(sanitized.content.contains("[REDACTED]"));
    }

    #[test]
    fn scanner_blocks_private_key_material() {
        let sanitized = sanitize_for_model_context(
            "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----",
            "key.pem",
        );

        assert!(sanitized.blocked);
        assert_eq!(sanitized.content, BLOCKED_CONTEXT_PLACEHOLDER);
    }

    #[test]
    fn scanner_blocks_dotenv_source_refs() {
        let sanitized = sanitize_for_model_context("SAFE=value", "repo/.env.local");

        assert!(sanitized.blocked);
        assert_eq!(sanitized.action, "blocked");
    }

    #[test]
    fn model_request_audits_link_ledger_and_budget_idempotently() {
        let store = crate::storage::SqliteStore::open_in_memory().expect("store");
        store.migrate().expect("migrate");
        seed_task(&store, "task-audit-1");
        let state = serde_json::json!({
            "modelRequestAudits": [{
                "requestId": "request-audit-1",
                "taskId": "task-audit-1",
                "provider": "openai-compatible",
                "modelId": "test-model",
                "phase": "planning",
                "status": "succeeded",
                "requestDigest": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "inputTokensEstimate": 12,
                "outputTokens": 5,
                "totalTokens": 17,
                "budgetLimit": 120000,
                "budgetPerCall": 24000,
                "blockedReason": "sensitive-canary-must-not-persist",
                "sources": [{
                    "dataKind": "prompt",
                    "sourceRef": "messages[0].content",
                    "action": "redacted",
                    "sensitivityLevel": "high",
                    "findings": ["credential_or_user_path"],
                    "redacted": true,
                    "blocked": false,
                    "sizeBytes": 80,
                    "tokensEstimate": 12
                }]
            }]
        });

        sync_model_request_audits(store.connection(), "task-audit-1", &state).expect("first sync");
        sync_model_request_audits(store.connection(), "task-audit-1", &state).expect("second sync");

        let ledger_count: i64 = store
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM privacy_ledger_entries WHERE task_id = 'task-audit-1'",
                [],
                |row| row.get(0),
            )
            .expect("ledger count");
        let budget: (i64, String) = store
            .connection()
            .query_row(
                "SELECT COUNT(*), run_id FROM token_budget_records WHERE task_id = 'task-audit-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("budget linkage");
        let persisted: String = store.connection().query_row(
            "SELECT source_ref || findings_json || reason FROM privacy_ledger_entries WHERE task_id = 'task-audit-1'",
            [],
            |row| row.get(0),
        ).expect("persisted metadata");

        assert_eq!(ledger_count, 1);
        assert_eq!(budget, (1, "request-audit-1".to_string()));
        assert!(!persisted.contains("sensitive-canary-must-not-persist"));
    }

    #[test]
    fn model_request_audit_rejects_task_mismatch() {
        let store = crate::storage::SqliteStore::open_in_memory().expect("store");
        store.migrate().expect("migrate");
        let state = serde_json::json!({
            "modelRequestAudits": [{
                "requestId": "request-audit-2", "taskId": "other-task",
                "provider": "provider", "modelId": "model", "phase": "planning",
                "status": "blocked",
                "requestDigest": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "inputTokensEstimate": 1, "outputTokens": 0, "totalTokens": 1,
                "budgetLimit": 10, "budgetPerCall": 10,
                "sources": [{
                    "dataKind": "prompt", "sourceRef": "messages[0].content",
                    "action": "blocked", "sensitivityLevel": "blocked",
                    "findings": ["private_key_or_certificate"],
                    "redacted": true, "blocked": true, "sizeBytes": 10, "tokensEstimate": 1
                }]
            }]
        });

        assert!(sync_model_request_audits(store.connection(), "task-audit-2", &state).is_err());
    }
}
