use std::{
    path::{Component, Path, PathBuf},
    sync::OnceLock,
};

use serde::{Deserialize, Serialize};

const BLOCKLIST_JSON: &str = include_str!("../../../../../config/commands.blocklist.json");
const ALLOWLIST_JSON: &str = include_str!("../../../../../config/commands.allowlist.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RiskOperation {
    DeletePath,
    DependencyChange,
    DatabaseSchemaChange,
    DangerousCommand,
    RemotePush,
    MergeMainCode,
    PathOutsideWorktree,
}

impl RiskOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DeletePath => "deletePath",
            Self::DependencyChange => "dependencyChange",
            Self::DatabaseSchemaChange => "databaseSchemaChange",
            Self::DangerousCommand => "dangerousCommand",
            Self::RemotePush => "remotePush",
            Self::MergeMainCode => "mergeMainCode",
            Self::PathOutsideWorktree => "pathOutsideWorktree",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskAssessment {
    pub level: RiskLevel,
    pub operations: Vec<RiskOperation>,
    pub reason: String,
    pub requires_approval: bool,
    pub denied: bool,
    pub matched_rule_ids: Vec<String>,
    pub allowed_by: Option<String>,
}

impl RiskAssessment {
    fn low(allowed_by: Option<String>) -> Self {
        Self {
            level: RiskLevel::Low,
            operations: Vec::new(),
            reason: "Command is low risk under the current safety policy.".to_string(),
            requires_approval: false,
            denied: false,
            matched_rule_ids: Vec::new(),
            allowed_by,
        }
    }

    fn approval(
        operations: Vec<RiskOperation>,
        matched_rule_ids: Vec<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            level: RiskLevel::High,
            operations,
            reason: reason.into(),
            requires_approval: true,
            denied: false,
            matched_rule_ids,
            allowed_by: None,
        }
    }

    fn denied(
        operations: Vec<RiskOperation>,
        matched_rule_ids: Vec<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            level: RiskLevel::High,
            operations,
            reason: reason.into(),
            requires_approval: false,
            denied: true,
            matched_rule_ids,
            allowed_by: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlocklistConfig {
    patterns: Vec<BlocklistPattern>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BlocklistPattern {
    id: String,
    #[serde(rename = "match")]
    pattern: String,
    action: PolicyAction,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AllowlistConfig {
    commands: Vec<AllowlistCommand>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AllowlistCommand {
    id: String,
    command: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum PolicyAction {
    Deny,
    Approval,
}

#[derive(Debug, Clone)]
pub struct SafetyPolicy {
    blocklist: Vec<BlocklistPattern>,
    allowlist: Vec<AllowlistCommand>,
}

impl Default for SafetyPolicy {
    fn default() -> Self {
        let mut blocklist = serde_json::from_str::<BlocklistConfig>(BLOCKLIST_JSON)
            .map(|config| config.patterns)
            .unwrap_or_default();
        blocklist.extend(builtin_blocklist());

        let allowlist = serde_json::from_str::<AllowlistConfig>(ALLOWLIST_JSON)
            .map(|config| config.commands)
            .unwrap_or_default();

        Self {
            blocklist,
            allowlist,
        }
    }
}

pub fn default_policy() -> &'static SafetyPolicy {
    static POLICY: OnceLock<SafetyPolicy> = OnceLock::new();
    POLICY.get_or_init(SafetyPolicy::default)
}

pub fn assess_command(command: &str, cwd: &Path, worktree: &Path) -> RiskAssessment {
    default_policy().assess_command(command, cwd, worktree)
}

impl SafetyPolicy {
    pub fn assess_command(&self, command: &str, cwd: &Path, worktree: &Path) -> RiskAssessment {
        let command = command.trim();
        if command.is_empty() {
            return RiskAssessment::low(None);
        }

        let normalized = normalize_command(command);
        let mut operations = Vec::new();
        let mut reasons = Vec::new();
        let mut matched_rule_ids = Vec::new();

        for pattern in &self.blocklist {
            if normalized.contains(&normalize_command(&pattern.pattern)) {
                matched_rule_ids.push(pattern.id.clone());
                let operation = operation_for_match(&pattern.id, &pattern.pattern);
                push_operation(&mut operations, operation);

                match pattern.action {
                    PolicyAction::Deny => {
                        return RiskAssessment::denied(
                            operations,
                            matched_rule_ids,
                            format!("Command is blocked by safety rule {}.", pattern.id),
                        );
                    }
                    PolicyAction::Approval => {
                        reasons.push(format!("Command matches high-risk rule {}.", pattern.id));
                    }
                }
            }
        }

        if let Some(escape) = detect_path_escape(command, cwd, worktree) {
            return RiskAssessment::denied(
                vec![RiskOperation::PathOutsideWorktree],
                vec!["path.outsideWorktree".to_string()],
                format!(
                    "Command targets a path outside the task worktree: {}.",
                    escape.to_string_lossy()
                ),
            );
        }

        collect_builtin_risks(&normalized, &mut operations, &mut reasons);

        if operations.is_empty() {
            let allowed_by = self
                .allowlist
                .iter()
                .find(|entry| normalize_command(&entry.command) == normalized)
                .map(|entry| entry.id.clone());
            return RiskAssessment::low(allowed_by);
        }

        if reasons.is_empty() {
            reasons.push("Command matches one or more high-risk operation categories.".to_string());
        }

        RiskAssessment::approval(operations, matched_rule_ids, reasons.join(" "))
    }
}

fn builtin_blocklist() -> Vec<BlocklistPattern> {
    [
        ("danger.format.disk", "format", PolicyAction::Deny),
        ("danger.windows.del.recursive", "del /s", PolicyAction::Deny),
        ("danger.windows.rd.recursive", "rd /s", PolicyAction::Deny),
        (
            "danger.git.force.push.short",
            "git push -f",
            PolicyAction::Deny,
        ),
    ]
    .into_iter()
    .map(|(id, pattern, action)| BlocklistPattern {
        id: id.to_string(),
        pattern: pattern.to_string(),
        action,
    })
    .collect()
}

fn collect_builtin_risks(
    normalized: &str,
    operations: &mut Vec<RiskOperation>,
    reasons: &mut Vec<String>,
) {
    if has_command_word(normalized, "rm")
        || has_command_word(normalized, "del")
        || has_command_word(normalized, "rmdir")
        || has_command_word(normalized, "rd")
        || normalized.contains("remove-item")
    {
        push_operation(operations, RiskOperation::DeletePath);
        reasons.push("Deleting files or directories requires approval.".to_string());
    }

    if normalized.contains("sudo")
        || normalized.contains("chmod -r")
        || normalized.contains("chown -r")
        || normalized.contains("takeown")
    {
        push_operation(operations, RiskOperation::DangerousCommand);
        reasons.push("Dangerous shell or permission command requires approval.".to_string());
    }

    if normalized.contains("git push") {
        push_operation(operations, RiskOperation::RemotePush);
        reasons.push("Pushing to a remote repository requires approval.".to_string());
    }

    if normalized.contains("git merge")
        || normalized.contains("git rebase")
        || normalized.contains("git cherry-pick")
    {
        push_operation(operations, RiskOperation::MergeMainCode);
        reasons.push("Merging or replaying commits requires approval.".to_string());
    }

    if is_dependency_change(normalized) {
        push_operation(operations, RiskOperation::DependencyChange);
        reasons
            .push("Installing, upgrading, or removing dependencies requires approval.".to_string());
    }

    if is_database_schema_change(normalized) {
        push_operation(operations, RiskOperation::DatabaseSchemaChange);
        reasons.push("Database schema or migration command requires approval.".to_string());
    }
}

fn is_dependency_change(normalized: &str) -> bool {
    [
        "npm install",
        "npm i",
        "npm uninstall",
        "npm update",
        "yarn add",
        "yarn remove",
        "yarn upgrade",
        "yarn install",
        "pnpm add",
        "pnpm remove",
        "pnpm update",
        "pnpm install",
        "bun add",
        "bun remove",
        "bun install",
        "pip install",
        "pip uninstall",
        "poetry add",
        "poetry remove",
        "poetry update",
        "cargo add",
        "cargo remove",
        "cargo update",
        "cargo install",
        "dotnet add package",
        "gem install",
        "gem uninstall",
        "go get",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn is_database_schema_change(normalized: &str) -> bool {
    [
        "prisma migrate",
        "sequelize db:migrate",
        "alembic upgrade",
        "alembic downgrade",
        "diesel migration",
        "knex migrate",
        "rails db:migrate",
        "typeorm migration",
        "sqlx migrate",
        "migration run",
        "migrate deploy",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn detect_path_escape(command: &str, cwd: &Path, worktree: &Path) -> Option<PathBuf> {
    if !contains_filesystem_write(command) {
        return None;
    }

    let cwd = normalize_path(cwd);
    let worktree = normalize_path(worktree);
    tokenize_command(command)
        .into_iter()
        .filter(|token| is_path_like_token(token))
        .filter_map(|token| resolve_command_path(&cwd, &token))
        .find(|path| !path_starts_with(path, &worktree))
}

fn contains_filesystem_write(command: &str) -> bool {
    let normalized = normalize_command(command);
    [
        "rm",
        "del",
        "rd",
        "rmdir",
        "remove-item",
        "move-item",
        "copy-item",
        "set-content",
        "out-file",
        "new-item",
        "mv",
        "cp",
        "mkdir",
        "touch",
        ">",
        ">>",
    ]
    .iter()
    .any(|needle| has_command_word(&normalized, needle) || normalized.contains(needle))
}

fn resolve_command_path(cwd: &Path, token: &str) -> Option<PathBuf> {
    let token = token.trim_matches(|character| {
        matches!(
            character,
            '"' | '\'' | '`' | ',' | ';' | ')' | '(' | '[' | ']' | '{' | '}'
        )
    });
    if token.is_empty()
        || token.starts_with('-')
        || token.starts_with('$')
        || token.starts_with('%')
    {
        return None;
    }

    let path = PathBuf::from(token);
    if is_home_path(token) {
        return Some(path);
    }

    if path.is_absolute() || has_windows_drive_prefix(token) {
        return Some(normalize_path(path));
    }

    if token.contains("..") || token.contains('/') || token.contains('\\') || token.starts_with('.')
    {
        return Some(normalize_path(cwd.join(path)));
    }

    None
}

fn is_path_like_token(token: &str) -> bool {
    let trimmed = token.trim_matches('"').trim_matches('\'');
    trimmed.contains("..")
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.starts_with('.')
        || trimmed.starts_with('~')
        || has_windows_drive_prefix(trimmed)
}

fn tokenize_command(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for character in command.chars() {
        if matches!(quote, Some(active) if active == character) {
            quote = None;
            continue;
        }

        if quote.is_none() && matches!(character, '"' | '\'' | '`') {
            quote = Some(character);
            continue;
        }

        if quote.is_none() && character.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }

        current.push(character);
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn operation_for_match(id: &str, pattern: &str) -> RiskOperation {
    let normalized = normalize_command(&format!("{id} {pattern}"));
    if normalized.contains("push") {
        RiskOperation::RemotePush
    } else if normalized.contains("reset") || normalized.contains("merge") {
        RiskOperation::MergeMainCode
    } else if normalized.contains("remove")
        || normalized.contains("rm")
        || normalized.contains("del")
    {
        RiskOperation::DeletePath
    } else {
        RiskOperation::DangerousCommand
    }
}

fn push_operation(operations: &mut Vec<RiskOperation>, operation: RiskOperation) {
    if !operations.contains(&operation) {
        operations.push(operation);
    }
}

fn normalize_command(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn has_command_word(normalized: &str, word: &str) -> bool {
    if word == ">" || word == ">>" {
        return normalized.contains(word);
    }

    normalized
        .split(|character: char| {
            character.is_whitespace()
                || matches!(character, '&' | '|' | ';' | '(' | ')' | '"' | '\'')
        })
        .any(|token| token == word)
}

fn has_windows_drive_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 3 && bytes[1] == b':' && (bytes[2] == b'\\' || bytes[2] == b'/')
}

fn is_home_path(value: &str) -> bool {
    value == "~" || value.starts_with("~/") || value.starts_with("~\\")
}

fn normalize_path(path: impl AsRef<Path>) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.as_ref().components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn path_starts_with(path: &Path, root: &Path) -> bool {
    #[cfg(windows)]
    {
        let path = path.to_string_lossy().to_ascii_lowercase();
        let root = root.to_string_lossy().to_ascii_lowercase();
        path == root || path.starts_with(&format!("{root}\\"))
    }

    #[cfg(not(windows))]
    {
        path.starts_with(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> SafetyPolicy {
        SafetyPolicy::default()
    }

    #[test]
    fn blocklist_denies_recursive_remove() {
        let assessment = policy().assess_command(
            "rm -rf dist",
            Path::new("D:/repo/worktree"),
            Path::new("D:/repo/worktree"),
        );

        assert!(assessment.denied);
        assert!(assessment.operations.contains(&RiskOperation::DeletePath));
    }

    #[test]
    fn blocklist_can_require_approval() {
        let assessment = policy().assess_command(
            "git reset --hard HEAD~1",
            Path::new("D:/repo/worktree"),
            Path::new("D:/repo/worktree"),
        );

        assert!(!assessment.denied);
        assert!(assessment.requires_approval);
        assert!(assessment
            .operations
            .contains(&RiskOperation::MergeMainCode));
    }

    #[test]
    fn dependency_change_requires_approval() {
        let assessment = policy().assess_command(
            "npm install left-pad",
            Path::new("D:/repo/worktree"),
            Path::new("D:/repo/worktree"),
        );

        assert!(assessment.requires_approval);
        assert!(assessment
            .operations
            .contains(&RiskOperation::DependencyChange));
    }

    #[test]
    fn allowlisted_command_stays_low_risk() {
        let assessment = policy().assess_command(
            "git status --short",
            Path::new("D:/repo/worktree"),
            Path::new("D:/repo/worktree"),
        );

        assert!(!assessment.denied);
        assert!(!assessment.requires_approval);
        assert_eq!(assessment.level, RiskLevel::Low);
        assert_eq!(assessment.allowed_by.as_deref(), Some("git.status"));
    }

    #[test]
    fn write_path_outside_worktree_is_denied() {
        let assessment = policy().assess_command(
            "Set-Content ..\\..\\outside.txt ok",
            Path::new("D:/repo/worktree/src"),
            Path::new("D:/repo/worktree"),
        );

        assert!(assessment.denied);
        assert!(assessment
            .operations
            .contains(&RiskOperation::PathOutsideWorktree));
    }
}
