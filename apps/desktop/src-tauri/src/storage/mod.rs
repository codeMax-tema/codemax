pub const DEFAULT_DATABASE_FILE: &str = "app.db";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageRoots {
    pub app_data_dir: String,
    pub artifact_root: String,
    pub worktree_root: String,
}

