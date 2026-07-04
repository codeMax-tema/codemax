#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryInfo {
    pub path: String,
    pub branch: String,
    pub dirty: bool,
}

