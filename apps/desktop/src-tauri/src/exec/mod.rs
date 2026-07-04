#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRequest {
    pub command: String,
    pub cwd: String,
    pub timeout_ms: Option<u64>,
}

