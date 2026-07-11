use std::env;

use rusqlite::{params, Connection};
use serde::Serialize;
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
}
