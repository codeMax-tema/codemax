use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{self, BufReader, BufWriter, Write},
    path::{Component, Path, PathBuf},
};

const FILESYSTEM_ALLOCATION_UNIT: u64 = 4096;
const MINIMUM_SAFETY_MARGIN_BYTES: u64 = 64 * 1024;

const EXCLUDED_DIRECTORIES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "__pycache__",
    "app-data",
    "output",
    ".worktrees",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IsolatedCopyEstimate {
    pub logical_bytes: u64,
    pub estimated_bytes: u64,
    pub estimated_files: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolatedCopyResult {
    pub source_path: PathBuf,
    pub workspace_path: PathBuf,
    pub estimated_bytes: u64,
    pub copied_bytes: u64,
    pub copied_files: u64,
}

pub fn estimate_isolated_copy(source: impl AsRef<Path>) -> io::Result<IsolatedCopyEstimate> {
    estimate_isolated_copy_with_exclusions(source, &[])
}

pub fn estimate_isolated_copy_with_exclusions(
    source: impl AsRef<Path>,
    user_exclusions: &[String],
) -> io::Result<IsolatedCopyEstimate> {
    validate_user_exclusions(user_exclusions)?;
    let source = canonical_source_directory(source.as_ref())?;
    let mut estimate = IsolatedCopyEstimate::default();
    estimate_directory(&source, &mut estimate, user_exclusions)?;
    apply_estimate_safety_margin(&mut estimate)?;
    Ok(estimate)
}

pub fn prepare_isolated_copy(
    source: impl AsRef<Path>,
    workspace_root: impl AsRef<Path>,
    task_id: &str,
) -> io::Result<IsolatedCopyResult> {
    prepare_isolated_copy_with_exclusions(source, workspace_root, task_id, &[])
}

pub fn prepare_isolated_copy_with_exclusions(
    source: impl AsRef<Path>,
    workspace_root: impl AsRef<Path>,
    task_id: &str,
    user_exclusions: &[String],
) -> io::Result<IsolatedCopyResult> {
    prepare_isolated_copy_with_space_probe(
        source.as_ref(),
        workspace_root.as_ref(),
        task_id,
        user_exclusions,
        &mut stream_copy_file,
        &mut available_space_for_path,
    )
}

#[cfg(test)]
fn prepare_isolated_copy_with(
    source: &Path,
    workspace_root: &Path,
    task_id: &str,
    copy_file: &mut dyn FnMut(&Path, &Path) -> io::Result<u64>,
) -> io::Result<IsolatedCopyResult> {
    prepare_isolated_copy_with_space_probe(
        source,
        workspace_root,
        task_id,
        &[],
        copy_file,
        &mut |_| Ok(u64::MAX),
    )
}

fn prepare_isolated_copy_with_space_probe(
    source: &Path,
    workspace_root: &Path,
    task_id: &str,
    user_exclusions: &[String],
    copy_file: &mut dyn FnMut(&Path, &Path) -> io::Result<u64>,
    available_space: &mut dyn FnMut(&Path) -> io::Result<u64>,
) -> io::Result<IsolatedCopyResult> {
    validate_task_id(task_id)?;
    validate_user_exclusions(user_exclusions)?;
    let source = canonical_source_directory(source)?;
    let unresolved_workspace_path = workspace_root.join(task_id);
    let checked_workspace_path = resolve_path_for_boundary_check(&unresolved_workspace_path)?;

    if checked_workspace_path.starts_with(&source) {
        return Err(invalid_input(
            "task destination cannot be nested inside the source directory",
        ));
    }

    let mut estimate = IsolatedCopyEstimate::default();
    estimate_directory(&source, &mut estimate, user_exclusions)?;
    apply_estimate_safety_margin(&mut estimate)?;
    let available_bytes = available_space(workspace_root)?;
    if available_bytes < estimate.estimated_bytes {
        return Err(io::Error::new(
            io::ErrorKind::StorageFull,
            format!(
                "isolated workspace needs {} bytes but only {available_bytes} bytes are available",
                estimate.estimated_bytes
            ),
        ));
    }

    fs::create_dir_all(workspace_root)
        .map_err(|error| io_path_error(error, "create workspace root", workspace_root))?;
    let workspace_root = workspace_root
        .canonicalize()
        .map_err(|error| io_path_error(error, "resolve workspace root", workspace_root))?;
    let workspace_path = workspace_root.join(task_id);

    if !workspace_path.starts_with(&workspace_root) {
        return Err(invalid_input("task destination escapes the workspace root"));
    }
    if workspace_path.starts_with(&source) {
        return Err(invalid_input(
            "task destination cannot be nested inside the source directory",
        ));
    }

    match fs::symlink_metadata(&workspace_path) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "isolated workspace already exists: {}",
                    workspace_path.display()
                ),
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(io_path_error(
                error,
                "inspect workspace destination",
                &workspace_path,
            ))
        }
    }

    fs::create_dir(&workspace_path)
        .map_err(|error| io_path_error(error, "create workspace destination", &workspace_path))?;
    let copy_result = copy_directory(&source, &workspace_path, user_exclusions, copy_file);

    match copy_result {
        Ok((copied_bytes, copied_files)) => Ok(IsolatedCopyResult {
            source_path: source,
            workspace_path,
            estimated_bytes: estimate.estimated_bytes,
            copied_bytes,
            copied_files,
        }),
        Err(copy_error) => {
            if let Err(cleanup_error) = fs::remove_dir_all(&workspace_path) {
                return Err(combine_copy_cleanup_error(
                    copy_error,
                    cleanup_error,
                    &workspace_path,
                ));
            }
            Err(copy_error)
        }
    }
}

fn combine_copy_cleanup_error(
    copy_error: io::Error,
    cleanup_error: io::Error,
    workspace_path: &Path,
) -> io::Error {
    io::Error::new(
        copy_error.kind(),
        format!(
            "isolated copy failed ({copy_error}); failed to clean {} ({cleanup_error})",
            workspace_path.display()
        ),
    )
}

fn available_space_for_path(path: &Path) -> io::Result<u64> {
    let mut existing = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| io_path_error(error, "resolve current directory", path))?
            .join(path)
    };
    while !existing.exists() {
        existing = existing
            .parent()
            .ok_or_else(|| invalid_input("workspace root has no existing parent"))?
            .to_path_buf();
    }
    match fs2::available_space(&existing) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => fs2::free_space(&existing)
            .map_err(|fallback_error| {
                io_path_error(fallback_error, "inspect free disk space", &existing)
            }),
        Err(error) => Err(io_path_error(
            error,
            "inspect available disk space",
            &existing,
        )),
    }
}

fn canonical_source_directory(source: &Path) -> io::Result<PathBuf> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| io_path_error(error, "inspect source directory", source))?;
    if metadata.file_type().is_symlink() {
        return Err(invalid_input("source directory cannot be a symbolic link"));
    }
    if !metadata.is_dir() {
        return Err(invalid_input("source path must be a directory"));
    }
    source
        .canonicalize()
        .map_err(|error| io_path_error(error, "resolve source directory", source))
}

fn validate_task_id(task_id: &str) -> io::Result<()> {
    let mut components = Path::new(task_id).components();
    let valid = matches!(components.next(), Some(Component::Normal(value)) if !value.is_empty())
        && components.next().is_none();
    if valid {
        Ok(())
    } else {
        Err(invalid_input(
            "task id must be exactly one non-empty path component",
        ))
    }
}

fn resolve_path_for_boundary_check(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                io_path_error(error, "resolve current directory for boundary check", path)
            })?
            .join(path)
    };
    let mut existing = absolute.as_path();
    let mut missing_components = Vec::new();

    loop {
        match existing.canonicalize() {
            Ok(mut resolved) => {
                for component in missing_components.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let Some(component) = existing.file_name() else {
                    return Err(io_path_error(error, "resolve boundary path", &existing));
                };
                missing_components.push(component.to_os_string());
                existing = existing.parent().ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "boundary path has no existing parent: {}",
                            existing.display()
                        ),
                    )
                })?;
            }
            Err(error) => {
                return Err(io_path_error(error, "resolve boundary path", &existing));
            }
        }
    }
}

fn estimate_directory(
    directory: &Path,
    estimate: &mut IsolatedCopyEstimate,
    user_exclusions: &[String],
) -> io::Result<()> {
    estimate.estimated_bytes = checked_sum(
        estimate.estimated_bytes,
        FILESYSTEM_ALLOCATION_UNIT,
        "isolated copy directory estimate overflowed",
    )?;

    let entries = fs::read_dir(directory)
        .map_err(|error| io_path_error(error, "read source directory", directory))?;
    for entry in entries {
        let entry = entry
            .map_err(|error| io_path_error(error, "read source directory entry", directory))?;
        let entry_path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| io_path_error(error, "inspect source entry", &entry_path))?;

        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if !is_excluded_directory(&entry.file_name(), user_exclusions) {
                estimate_directory(&entry_path, estimate, user_exclusions)?;
            }
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let file_bytes = entry
            .metadata()
            .map_err(|error| io_path_error(error, "read source metadata", &entry_path))?
            .len();
        estimate.logical_bytes = checked_sum(
            estimate.logical_bytes,
            file_bytes,
            "isolated copy logical byte estimate overflowed",
        )?;
        estimate.estimated_bytes = checked_sum(
            estimate.estimated_bytes,
            allocated_file_bytes(file_bytes)?,
            "isolated copy disk byte estimate overflowed",
        )?;
        estimate.estimated_files = checked_sum(
            estimate.estimated_files,
            1,
            "isolated copy file estimate overflowed",
        )?;
    }
    Ok(())
}

fn allocated_file_bytes(file_bytes: u64) -> io::Result<u64> {
    let non_zero = file_bytes.max(1);
    let units = non_zero
        .checked_add(FILESYSTEM_ALLOCATION_UNIT - 1)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "file allocation estimate overflowed",
            )
        })?
        / FILESYSTEM_ALLOCATION_UNIT;
    units
        .checked_mul(FILESYSTEM_ALLOCATION_UNIT)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "file allocation estimate overflowed",
            )
        })
}

fn apply_estimate_safety_margin(estimate: &mut IsolatedCopyEstimate) -> io::Result<()> {
    let proportional_margin = estimate.estimated_bytes / 20;
    let safety_margin = proportional_margin.max(MINIMUM_SAFETY_MARGIN_BYTES);
    estimate.estimated_bytes = checked_sum(
        estimate.estimated_bytes,
        safety_margin,
        "isolated copy safety margin overflowed",
    )?;
    Ok(())
}

fn copy_directory(
    source: &Path,
    destination: &Path,
    user_exclusions: &[String],
    copy_file: &mut dyn FnMut(&Path, &Path) -> io::Result<u64>,
) -> io::Result<(u64, u64)> {
    let mut copied_bytes = 0_u64;
    let mut copied_files = 0_u64;
    let entries = fs::read_dir(source)
        .map_err(|error| io_path_error(error, "read source directory", source))?;

    for entry in entries {
        let entry =
            entry.map_err(|error| io_path_error(error, "read source directory entry", source))?;
        let source_path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| io_path_error(error, "inspect source entry", &source_path))?;

        if file_type.is_symlink() {
            continue;
        }

        let destination_path = destination.join(entry.file_name());
        if file_type.is_dir() {
            if is_excluded_directory(&entry.file_name(), user_exclusions) {
                continue;
            }
            fs::create_dir(&destination_path).map_err(|error| {
                io_path_error(error, "create destination directory", &destination_path)
            })?;
            let (directory_bytes, directory_files) =
                copy_directory(&source_path, &destination_path, user_exclusions, copy_file)?;
            copied_bytes = checked_sum(
                copied_bytes,
                directory_bytes,
                "isolated copy byte count overflowed",
            )?;
            copied_files = checked_sum(
                copied_files,
                directory_files,
                "isolated copy file count overflowed",
            )?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let file_bytes = copy_file(&source_path, &destination_path)
            .map_err(|error| io_path_error(error, "copy source file", &source_path))?;
        copied_bytes = checked_sum(
            copied_bytes,
            file_bytes,
            "isolated copy byte count overflowed",
        )?;
        copied_files = checked_sum(copied_files, 1, "isolated copy file count overflowed")?;
    }

    Ok((copied_bytes, copied_files))
}

fn stream_copy_file(source: &Path, destination: &Path) -> io::Result<u64> {
    let source_file = open_source_file_without_following_links(source)?;
    let destination_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| io_path_error(error, "create destination file", destination))?;
    let mut reader = BufReader::new(source_file);
    let mut writer = BufWriter::new(destination_file);
    let copied_bytes = io::copy(&mut reader, &mut writer)
        .map_err(|error| io_path_error(error, "stream source file", source))?;
    writer
        .flush()
        .map_err(|error| io_path_error(error, "flush destination file", destination))?;
    Ok(copied_bytes)
}

fn open_source_file_without_following_links(source: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    configure_no_follow(&mut options);
    let file = options
        .open(source)
        .map_err(|error| io_path_error(error, "open source file", source))?;
    let metadata = file
        .metadata()
        .map_err(|error| io_path_error(error, "inspect opened source file", source))?;

    let windows_reparse_tag = windows_reparse_tag(&file)
        .map_err(|error| io_path_error(error, "inspect Windows reparse tag", source))?;
    if metadata.file_type().is_symlink() || is_windows_name_surrogate_tag(windows_reparse_tag) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "source file cannot be a symbolic link: {}",
                source.display()
            ),
        ));
    }

    if windows_reparse_tag.is_some() {
        drop(file);
        return File::open(source)
            .map_err(|error| io_path_error(error, "open reparse-backed source file", source));
    }

    Ok(file)
}

#[cfg(target_os = "linux")]
fn configure_no_follow(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;

    const O_NOFOLLOW: i32 = 0x20_000;
    options.custom_flags(O_NOFOLLOW);
}

#[cfg(windows)]
fn configure_no_follow(options: &mut OpenOptions) {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
}

#[cfg(not(any(target_os = "linux", windows)))]
fn configure_no_follow(_options: &mut OpenOptions) {}

#[cfg(windows)]
fn windows_reparse_tag(file: &File) -> io::Result<Option<u32>> {
    use std::{mem::size_of, os::windows::io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        FileAttributeTagInfo, GetFileInformationByHandleEx, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_ATTRIBUTE_TAG_INFO,
    };

    let mut info = FILE_ATTRIBUTE_TAG_INFO {
        FileAttributes: 0,
        ReparseTag: 0,
    };
    let success = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileAttributeTagInfo,
            (&mut info as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
            size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    };
    if success == 0 {
        return Err(io::Error::last_os_error());
    }
    if info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0 {
        Ok(None)
    } else {
        Ok(Some(info.ReparseTag))
    }
}

#[cfg(not(windows))]
fn windows_reparse_tag(_file: &File) -> io::Result<Option<u32>> {
    Ok(None)
}

fn is_windows_name_surrogate_tag(reparse_tag: Option<u32>) -> bool {
    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
    const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;
    matches!(
        reparse_tag,
        Some(IO_REPARSE_TAG_MOUNT_POINT | IO_REPARSE_TAG_SYMLINK)
    )
}

fn is_excluded_directory(name: &OsStr, user_exclusions: &[String]) -> bool {
    name.to_str().is_some_and(|name| {
        EXCLUDED_DIRECTORIES
            .iter()
            .any(|excluded| name.eq_ignore_ascii_case(excluded))
            || user_exclusions
                .iter()
                .any(|excluded| name.eq_ignore_ascii_case(excluded.trim()))
    })
}

fn validate_user_exclusions(user_exclusions: &[String]) -> io::Result<()> {
    for exclusion in user_exclusions {
        let exclusion = exclusion.trim();
        if exclusion.is_empty() {
            continue;
        }
        let mut components = Path::new(exclusion).components();
        let valid = matches!(components.next(), Some(Component::Normal(value)) if !value.is_empty())
            && components.next().is_none();
        if !valid || exclusion.contains('/') || exclusion.contains('\\') {
            return Err(invalid_input(
                "workspace exclusions must be single directory names",
            ));
        }
    }
    Ok(())
}

fn checked_sum(left: u64, right: u64, message: &'static str) -> io::Result<u64> {
    left.checked_add(right)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, message))
}

fn io_path_error(error: io::Error, operation: &str, path: &Path) -> io::Error {
    io::Error::new(
        error.kind(),
        format!("failed to {operation} {}: {error}", path.display()),
    )
}

fn invalid_input(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs, io,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "codemax-isolated-copy-{label}-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create temporary test directory");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write(path: impl AsRef<Path>, contents: &[u8]) {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create test file parent");
        }
        fs::write(path, contents).expect("write test file");
    }

    #[test]
    fn copies_nested_files_and_reports_the_same_totals_as_estimate() {
        let temp = TempDir::new("nested");
        let source = temp.path().join("source");
        let workspace_root = temp.path().join("workspaces");
        write(source.join("README.md"), b"hello");
        write(source.join("src/nested/data.bin"), &[0, 1, 2, 3]);

        let estimate = estimate_isolated_copy(&source).expect("estimate isolated copy");
        let result = prepare_isolated_copy(&source, &workspace_root, "task-001")
            .expect("prepare isolated copy");

        assert_eq!(estimate.logical_bytes, 9);
        assert!(estimate.estimated_bytes >= 2 * 4096);
        assert_eq!(estimate.estimated_files, 2);
        assert_eq!(result.copied_bytes, estimate.logical_bytes);
        assert_eq!(result.copied_files, estimate.estimated_files);
        assert_eq!(result.source_path, source.canonicalize().unwrap());
        assert_eq!(
            result.workspace_path,
            workspace_root.canonicalize().unwrap().join("task-001")
        );
        assert_eq!(
            fs::read(result.workspace_path.join("src/nested/data.bin")).unwrap(),
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn estimates_allocation_for_empty_files_instead_of_zero_bytes() {
        let temp = TempDir::new("empty-file-estimate");
        let source = temp.path().join("source");
        write(source.join("one.empty"), b"");
        write(source.join("two.empty"), b"");

        let estimate = estimate_isolated_copy(&source).expect("estimate empty files");

        assert_eq!(estimate.logical_bytes, 0);
        assert_eq!(estimate.estimated_files, 2);
        assert!(estimate.estimated_bytes >= 2 * 4096);
    }

    #[test]
    fn excludes_dependency_build_and_codemax_artifact_directories() {
        let temp = TempDir::new("excluded");
        let source = temp.path().join("source");
        let workspace_root = temp.path().join("workspaces");
        write(source.join("keep.txt"), b"keep");

        for name in [
            ".git",
            "node_modules",
            "target",
            "dist",
            "build",
            ".venv",
            "__pycache__",
            "app-data",
            "output",
            ".worktrees",
        ] {
            write(source.join(name).join("ignored.txt"), b"ignored");
        }

        let estimate = estimate_isolated_copy(&source).expect("estimate filtered source");
        let result = prepare_isolated_copy(&source, &workspace_root, "task-filtered")
            .expect("copy filtered source");

        assert_eq!(estimate.logical_bytes, 4);
        assert_eq!(estimate.estimated_files, 1);
        assert!(result.workspace_path.join("keep.txt").is_file());
        for name in [
            ".git",
            "node_modules",
            "target",
            "dist",
            "build",
            ".venv",
            "__pycache__",
            "app-data",
            "output",
            ".worktrees",
        ] {
            assert!(!result.workspace_path.join(name).exists(), "copied {name}");
        }
    }

    #[test]
    fn excludes_user_configured_directory_names() {
        let temp = TempDir::new("custom-exclusions");
        let source = temp.path().join("source");
        let workspace_root = temp.path().join("workspaces");
        write(source.join("keep.txt"), b"keep");
        write(source.join("coverage/report.json"), b"coverage");
        write(source.join("src/.cache/data.bin"), b"cache");

        let result = prepare_isolated_copy_with_exclusions(
            &source,
            &workspace_root,
            "task-custom-exclusions",
            &["coverage".to_string(), ".cache".to_string()],
        )
        .expect("copy with custom exclusions");

        assert!(result.workspace_path.join("keep.txt").is_file());
        assert!(!result.workspace_path.join("coverage").exists());
        assert!(!result.workspace_path.join("src/.cache").exists());
    }

    #[test]
    fn rejects_user_exclusions_that_contain_path_components() {
        let temp = TempDir::new("invalid-custom-exclusion");
        let source = temp.path().join("source");
        let workspace_root = temp.path().join("workspaces");
        write(source.join("keep.txt"), b"keep");

        let error = prepare_isolated_copy_with_exclusions(
            &source,
            &workspace_root,
            "task-invalid-exclusion",
            &["../outside".to_string()],
        )
        .expect_err("path-like exclusions must fail");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(!workspace_root.exists());
    }

    #[test]
    fn rejects_an_existing_destination_without_changing_it() {
        let temp = TempDir::new("existing");
        let source = temp.path().join("source");
        let workspace_root = temp.path().join("workspaces");
        write(source.join("source.txt"), b"source");
        write(workspace_root.join("task-existing/marker.txt"), b"preserve");

        let error = prepare_isolated_copy(&source, &workspace_root, "task-existing")
            .expect_err("existing destination must fail");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(
            fs::read(workspace_root.join("task-existing/marker.txt")).unwrap(),
            b"preserve"
        );
    }

    #[test]
    fn cleans_the_partial_destination_when_copying_fails() {
        let temp = TempDir::new("rollback");
        let source = temp.path().join("source");
        let workspace_root = temp.path().join("workspaces");
        write(source.join("one.txt"), b"one");
        write(source.join("two.txt"), b"two");
        let mut attempts = 0;

        let error = prepare_isolated_copy_with(
            &source,
            &workspace_root,
            "task-failing",
            &mut |from, to| {
                attempts += 1;
                if attempts == 2 {
                    return Err(io::Error::new(io::ErrorKind::Other, "injected failure"));
                }
                stream_copy_file(from, to)
            },
        )
        .expect_err("injected copy failure must be returned");

        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(!workspace_root.join("task-failing").exists());
    }

    #[test]
    fn combined_cleanup_errors_keep_the_original_copy_error_kind() {
        let copy_error = io::Error::new(io::ErrorKind::StorageFull, "disk full");
        let cleanup_error = io::Error::new(io::ErrorKind::PermissionDenied, "cleanup denied");
        let combined = combine_copy_cleanup_error(
            copy_error,
            cleanup_error,
            Path::new("D:/workspaces/task-001"),
        );

        assert_eq!(combined.kind(), io::ErrorKind::StorageFull);
        assert!(combined.to_string().contains("cleanup denied"));
    }

    #[test]
    fn rejects_insufficient_space_before_creating_the_workspace_root() {
        let temp = TempDir::new("insufficient-space");
        let source = temp.path().join("source");
        let workspace_root = temp.path().join("workspaces");
        write(source.join("payload.bin"), &[0, 1, 2, 3]);

        let error = prepare_isolated_copy_with_space_probe(
            &source,
            &workspace_root,
            "task-no-space",
            &[],
            &mut stream_copy_file,
            &mut |_| Ok(3),
        )
        .expect_err("insufficient target space must fail");

        assert_eq!(error.kind(), io::ErrorKind::StorageFull);
        assert!(!workspace_root.exists());
    }

    #[test]
    fn rejects_task_ids_that_can_escape_the_workspace_root() {
        let temp = TempDir::new("boundary");
        let source = temp.path().join("source");
        let workspace_root = temp.path().join("workspaces");
        write(source.join("file.txt"), b"data");

        for task_id in ["", ".", "..", "../escaped", "nested/task"] {
            let error = prepare_isolated_copy(&source, &workspace_root, task_id)
                .expect_err("unsafe task id must fail");
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{task_id}");
        }

        let absolute = temp.path().join("escaped");
        let error = prepare_isolated_copy(
            &source,
            &workspace_root,
            absolute.to_string_lossy().as_ref(),
        )
        .expect_err("absolute task id must fail");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(!temp.path().join("escaped").exists());
    }

    #[test]
    fn rejects_a_destination_nested_inside_the_source() {
        let temp = TempDir::new("recursive-target");
        let source = temp.path().join("source");
        let workspace_root = source.join("workspaces");
        write(source.join("file.txt"), b"data");

        let error = prepare_isolated_copy(&source, &workspace_root, "task-001")
            .expect_err("destination inside source must fail");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(
            !workspace_root.exists(),
            "rejection must not create the workspace root inside the source"
        );
    }

    #[test]
    fn stream_copy_rejects_an_existing_destination_without_overwriting_it() {
        let temp = TempDir::new("stream-existing");
        let source = temp.path().join("source.txt");
        let destination = temp.path().join("destination.txt");
        write(&source, b"new contents");
        write(&destination, b"preserve me");

        let error = stream_copy_file(&source, &destination)
            .expect_err("stream copy must not overwrite an existing destination");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(destination).unwrap(), b"preserve me");
    }

    #[test]
    fn stream_copy_rejects_a_symbolic_link_source() {
        let temp = TempDir::new("stream-source-symlink");
        let original = temp.path().join("original.txt");
        let source_link = temp.path().join("source-link.txt");
        let destination = temp.path().join("destination.txt");
        write(&original, b"secret");

        if skip_when_symlink_is_unavailable(
            "stream_copy_rejects_a_symbolic_link_source",
            create_file_symlink(&original, &source_link),
        ) {
            return;
        }

        stream_copy_file(&source_link, &destination)
            .expect_err("stream copy must not follow a symbolic link source");
        assert!(!destination.exists());
    }

    #[test]
    fn rejects_a_symbolic_link_directory_used_as_the_source() {
        let temp = TempDir::new("source-directory-symlink");
        let original = temp.path().join("original");
        let source_link = temp.path().join("source-link");
        let workspace_root = temp.path().join("workspaces");
        write(original.join("file.txt"), b"data");

        if skip_when_symlink_is_unavailable(
            "rejects_a_symbolic_link_directory_used_as_the_source",
            create_directory_symlink(&original, &source_link),
        ) {
            return;
        }

        let error = prepare_isolated_copy(&source_link, &workspace_root, "task-001")
            .expect_err("a symbolic link directory cannot be used as the source");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(!workspace_root.exists());
    }

    #[test]
    fn skips_symbolic_links_when_the_platform_allows_creating_them() {
        let temp = TempDir::new("symlink");
        let source = temp.path().join("source");
        let workspace_root = temp.path().join("workspaces");
        write(source.join("regular.txt"), b"regular");
        write(temp.path().join("outside/secret.txt"), b"secret");

        if skip_when_symlink_is_unavailable(
            "skips_symbolic_links_when_the_platform_allows_creating_them (file link)",
            create_file_symlink(
                &temp.path().join("outside/secret.txt"),
                &source.join("secret-link.txt"),
            ),
        ) {
            return;
        }
        if skip_when_symlink_is_unavailable(
            "skips_symbolic_links_when_the_platform_allows_creating_them (directory link)",
            create_directory_symlink(&temp.path().join("outside"), &source.join("outside-link")),
        ) {
            return;
        }

        let estimate = estimate_isolated_copy(&source).expect("estimate source with symlink");
        let result = prepare_isolated_copy(&source, &workspace_root, "task-symlink")
            .expect("copy source with symlink");

        assert_eq!(estimate.estimated_files, 1);
        assert_eq!(estimate.logical_bytes, 7);
        assert!(result.workspace_path.join("regular.txt").is_file());
        assert!(!result.workspace_path.join("secret-link.txt").exists());
        assert!(!result.workspace_path.join("outside-link").exists());
    }

    fn skip_when_symlink_is_unavailable(test_name: &str, result: io::Result<()>) -> bool {
        match result {
            Ok(()) => false,
            Err(error) => {
                if should_skip_symlink_error(&error) {
                    eprintln!(
                        "SKIPPED {test_name}: symbolic-link privilege is unavailable: {error}"
                    );
                    return true;
                }

                panic!("{test_name}: unable to create symbolic link: {error}");
            }
        }
    }

    fn should_skip_symlink_error(error: &io::Error) -> bool {
        #[cfg(windows)]
        {
            return error.raw_os_error() == Some(1314);
        }

        #[cfg(not(windows))]
        {
            let _ = error;
            false
        }
    }

    #[test]
    fn symlink_tests_do_not_skip_unexpected_creation_errors() {
        let unexpected = io::Error::new(io::ErrorKind::PermissionDenied, "unexpected denial");
        assert!(!should_skip_symlink_error(&unexpected));
    }

    #[cfg(unix)]
    fn create_file_symlink(original: &Path, link: &Path) -> io::Result<()> {
        std::os::unix::fs::symlink(original, link)
    }

    #[cfg(unix)]
    fn create_directory_symlink(original: &Path, link: &Path) -> io::Result<()> {
        std::os::unix::fs::symlink(original, link)
    }

    #[cfg(windows)]
    fn create_file_symlink(original: &Path, link: &Path) -> io::Result<()> {
        std::os::windows::fs::symlink_file(original, link)
    }

    #[cfg(windows)]
    fn create_directory_symlink(original: &Path, link: &Path) -> io::Result<()> {
        std::os::windows::fs::symlink_dir(original, link)
    }
}
