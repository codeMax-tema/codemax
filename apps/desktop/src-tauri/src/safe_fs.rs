use std::{
    io,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", tag = "operation")]
pub enum SafeFileOperation {
    Create { path: String, content: String },
    Update { path: String, content: String },
    Delete { path: String },
    Rename { path: String, destination: String },
    CreateDirectory { path: String },
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SafeFileOperationResult {
    pub operation: String,
    pub path: String,
    pub destination: Option<String>,
}

pub fn execute_operations(
    workspace: impl AsRef<Path>,
    operations: &[SafeFileOperation],
) -> io::Result<Vec<SafeFileOperationResult>> {
    platform::execute_operations(workspace.as_ref(), operations)
}

pub fn read_utf8(workspace: impl AsRef<Path>, relative: &str) -> io::Result<String> {
    platform::read_utf8(workspace.as_ref(), relative)
}

fn checked_relative(raw: &str) -> io::Result<PathBuf> {
    let path = Path::new(raw);
    if raw.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("path must be a normalized workspace-relative path: {raw}"),
        ));
    }
    #[cfg(windows)]
    for component in path.components() {
        let value = component.as_os_str().to_string_lossy();
        let trimmed = value.trim_end_matches([' ', '.']);
        let stem = trimmed
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if value.contains(':')
            || trimmed != value
            || matches!(
                stem.as_str(),
                "CON"
                    | "PRN"
                    | "AUX"
                    | "NUL"
                    | "COM1"
                    | "COM2"
                    | "COM3"
                    | "COM4"
                    | "COM5"
                    | "COM6"
                    | "COM7"
                    | "COM8"
                    | "COM9"
                    | "LPT1"
                    | "LPT2"
                    | "LPT3"
                    | "LPT4"
                    | "LPT5"
                    | "LPT6"
                    | "LPT7"
                    | "LPT8"
                    | "LPT9"
            )
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unsafe Windows path component: {raw}"),
            ));
        }
    }
    Ok(path.to_path_buf())
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        ffi::OsStr,
        fs::{self, File, OpenOptions},
        io::Write,
        os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
    };
    use uuid::Uuid;
    use windows_sys::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            FileAttributeTagInfo, FileDispositionInfoEx, FileIdInfo, FileRenameInfo,
            GetFileInformationByHandleEx, GetFinalPathNameByHandleW, SetFileInformationByHandle,
            DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO,
            FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
            FILE_DISPOSITION_INFO_EX, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_ID_INFO, FILE_NAME_NORMALIZED, FILE_RENAME_INFO, FILE_RENAME_INFO_0,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        },
    };

    struct LockedDirectory {
        path: PathBuf,
        file: File,
    }
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct FileIdentity {
        volume: u64,
        id: [u8; 16],
    }

    pub(super) fn read_utf8(workspace: &Path, raw: &str) -> io::Result<String> {
        let root = open_directory(workspace)?;
        let root_path = final_path(&root.file)?;
        let relative = checked_relative(raw)?;
        let (_locks, mut file) = open_existing_file_chain(&root_path, &relative)?;
        verify_inside(&root_path, &final_path(&file)?)?;
        let mut bytes = Vec::new();
        use std::io::Read;
        file.read_to_end(&mut bytes)?;
        String::from_utf8(bytes)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "file is not UTF-8 text"))
    }

    pub(super) fn execute_operations(
        workspace: &Path,
        operations: &[SafeFileOperation],
    ) -> io::Result<Vec<SafeFileOperationResult>> {
        reject_device_path(workspace)?;
        let root = open_directory(workspace)?;
        let root_path = final_path(&root.file)?;
        let mut results = Vec::with_capacity(operations.len());
        for operation in operations {
            results.push(execute_one(&root_path, operation)?);
        }
        Ok(results)
    }

    fn execute_one(
        root: &Path,
        operation: &SafeFileOperation,
    ) -> io::Result<SafeFileOperationResult> {
        match operation {
            SafeFileOperation::Create { path, content } => {
                atomic_write(root, path, content.as_bytes(), false)?;
                Ok(result("create", path, None))
            }
            SafeFileOperation::Update { path, content } => {
                atomic_write(root, path, content.as_bytes(), true)?;
                Ok(result("update", path, None))
            }
            SafeFileOperation::Delete { path } => {
                let relative = checked_relative(path)?;
                let (_locks, target) = match open_existing_file_chain(root, &relative) {
                    Ok(value) => value,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        return Ok(result("delete", path, None))
                    }
                    Err(error) => return Err(error),
                };
                verify_inside(root, &final_path(&target)?)?;
                delete_by_handle(&target)?;
                drop(target);
                Ok(result("delete", path, None))
            }
            SafeFileOperation::Rename { path, destination } => {
                let source = checked_relative(path)?;
                let destination_relative = checked_relative(destination)?;
                let (_source_locks, source_file) = open_existing_file_chain(root, &source)?;
                verify_inside(root, &final_path(&source_file)?)?;
                let _destination_locks = lock_parent_chain(root, &destination_relative)?;
                let destination_path = root.join(&destination_relative);
                if destination_path.exists() {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "rename destination exists",
                    ));
                }
                rename_by_handle(&source_file, &destination_path, false)?;
                drop(source_file);
                let renamed = open_file_no_reparse(&destination_path)?;
                verify_inside(root, &final_path(&renamed)?)?;
                Ok(result("rename", path, Some(destination.clone())))
            }
            SafeFileOperation::CreateDirectory { path } => {
                let relative = checked_relative(path)?;
                let _locks = lock_parent_chain(root, &relative)?;
                let target = root.join(relative);
                fs::create_dir(&target)?;
                let directory = open_directory(&target)?;
                verify_inside(root, &final_path(&directory.file)?)?;
                Ok(result("createDirectory", path, None))
            }
        }
    }

    fn atomic_write(root: &Path, raw: &str, content: &[u8], must_exist: bool) -> io::Result<()> {
        let relative = checked_relative(raw)?;
        let locks = lock_parent_chain(root, &relative)?;
        let parent = locks
            .last()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing parent"))?;
        let target_path = root.join(&relative);
        let original = if must_exist {
            let file = open_file_no_reparse(&target_path)?;
            verify_inside(root, &final_path(&file)?)?;
            if fs::read(&target_path)? == content {
                return Ok(());
            }
            Some(identity(&file)?)
        } else {
            if target_path.exists() {
                let existing = open_file_no_reparse(&target_path)?;
                verify_inside(root, &final_path(&existing)?)?;
                if fs::read(&target_path)? == content {
                    return Ok(());
                }
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "create target exists",
                ));
            }
            None
        };
        let temp_path = parent
            .path
            .join(format!(".codemax-safe-{}.tmp", Uuid::new_v4()));
        let mut temp = OpenOptions::new()
            .write(true)
            .create_new(true)
            .access_mode(DELETE | 0x4000_0000)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .open(&temp_path)?;
        temp.write_all(content)?;
        temp.sync_all()?;
        verify_inside(root, &final_path(&temp)?)?;
        if let Some(expected) = original {
            let current = open_file_no_reparse(&target_path)?;
            if identity(&current)? != expected {
                drop(temp);
                let _ = fs::remove_file(&temp_path);
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "target identity changed before replace",
                ));
            }
        }
        rename_by_handle(&temp, &target_path, must_exist)?;
        drop(temp);
        let committed = open_file_no_reparse(&target_path)?;
        verify_inside(root, &final_path(&committed)?)?;
        Ok(())
    }

    fn lock_parent_chain(root: &Path, relative: &Path) -> io::Result<Vec<LockedDirectory>> {
        let mut locks = vec![open_directory(root)?];
        let mut current = root.to_path_buf();
        let components: Vec<_> = relative.components().collect();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            current.push(component.as_os_str());
            let directory = open_directory(&current)?;
            verify_inside(root, &final_path(&directory.file)?)?;
            locks.push(directory);
        }
        Ok(locks)
    }

    #[cfg(test)]
    pub(super) fn assert_target_locked(root: &Path, relative: &Path) -> io::Result<()> {
        let (_locks, file) = open_existing_file_chain(root, relative)?;
        let target = root.join(relative);
        let moved = root.join("codemax-target-replacement-test");
        let result = fs::rename(&target, &moved);
        drop(file);
        if result.is_ok() {
            let _ = fs::rename(&moved, &target);
            Err(io::Error::new(
                io::ErrorKind::Other,
                "target replacement was not blocked",
            ))
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    pub(super) fn assert_parent_locked(root: &Path, relative: &Path) -> io::Result<()> {
        let locks = lock_parent_chain(root, relative)?;
        let parent = root.join(relative.parent().unwrap_or_else(|| Path::new("")));
        let moved = root.join("codemax-parent-replacement-test");
        let result = fs::rename(&parent, &moved);
        drop(locks);
        if result.is_ok() {
            let _ = fs::rename(&moved, &parent);
            Err(io::Error::new(
                io::ErrorKind::Other,
                "parent replacement was not blocked",
            ))
        } else {
            Ok(())
        }
    }

    fn open_existing_file_chain(
        root: &Path,
        relative: &Path,
    ) -> io::Result<(Vec<LockedDirectory>, File)> {
        let locks = lock_parent_chain(root, relative)?;
        let file = open_file_no_reparse(&root.join(relative))?;
        Ok((locks, file))
    }

    fn open_directory(path: &Path) -> io::Result<LockedDirectory> {
        reject_device_path(path)?;
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        reject_reparse(&file)?;
        Ok(LockedDirectory {
            path: final_path(&file)?,
            file,
        })
    }

    fn open_file_no_reparse(path: &Path) -> io::Result<File> {
        reject_device_path(path)?;
        let file = OpenOptions::new()
            .read(true)
            .access_mode(DELETE | 0x8000_0000)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        reject_reparse(&file)?;
        Ok(file)
    }

    fn rename_by_handle(file: &File, destination: &Path, replace: bool) -> io::Result<()> {
        let destination_wide: Vec<u16> = destination.as_os_str().encode_wide().collect();
        let base = size_of::<FILE_RENAME_INFO>() - size_of::<u16>();
        let bytes = base + destination_wide.len() * size_of::<u16>();
        let mut buffer = vec![0u8; bytes];
        let info = buffer.as_mut_ptr() as *mut FILE_RENAME_INFO;
        unsafe {
            (*info).Anonymous = FILE_RENAME_INFO_0 {
                ReplaceIfExists: u8::from(replace),
            };
            (*info).RootDirectory = std::ptr::null_mut();
            (*info).FileNameLength = (destination_wide.len() * size_of::<u16>()) as u32;
            std::ptr::copy_nonoverlapping(
                destination_wide.as_ptr(),
                (*info).FileName.as_mut_ptr(),
                destination_wide.len(),
            );
        }
        let ok = unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle() as HANDLE,
                FileRenameInfo,
                buffer.as_ptr() as *const _,
                buffer.len() as u32,
            )
        };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn delete_by_handle(file: &File) -> io::Result<()> {
        let info = FILE_DISPOSITION_INFO_EX {
            Flags: FILE_DISPOSITION_FLAG_DELETE | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS,
        };
        let ok = unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle() as HANDLE,
                FileDispositionInfoEx,
                &info as *const _ as *const _,
                size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
            )
        };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn reject_reparse(file: &File) -> io::Result<()> {
        let mut info = FILE_ATTRIBUTE_TAG_INFO {
            FileAttributes: 0,
            ReparseTag: 0,
        };
        let ok = unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle() as HANDLE,
                FileAttributeTagInfo,
                &mut info as *mut _ as *mut _,
                size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        if info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "reparse points are not allowed",
            ));
        }
        Ok(())
    }

    fn identity(file: &File) -> io::Result<FileIdentity> {
        let mut info: FILE_ID_INFO = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle() as HANDLE,
                FileIdInfo,
                &mut info as *mut _ as *mut _,
                size_of::<FILE_ID_INFO>() as u32,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(FileIdentity {
            volume: info.VolumeSerialNumber,
            id: info.FileId.Identifier,
        })
    }

    fn final_path(file: &File) -> io::Result<PathBuf> {
        let handle = file.as_raw_handle() as HANDLE;
        let needed = unsafe {
            GetFinalPathNameByHandleW(handle, std::ptr::null_mut(), 0, FILE_NAME_NORMALIZED)
        };
        if needed == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buffer = vec![0u16; needed as usize + 1];
        let written = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                FILE_NAME_NORMALIZED,
            )
        };
        if written == 0 {
            return Err(io::Error::last_os_error());
        }
        let raw = String::from_utf16_lossy(&buffer[..written as usize]);
        let normalized = raw
            .strip_prefix(r"\\?\UNC\")
            .map(|value| format!(r"\\{value}"))
            .or_else(|| raw.strip_prefix(r"\\?\").map(str::to_owned))
            .unwrap_or(raw);
        Ok(PathBuf::from(normalized))
    }

    fn verify_inside(root: &Path, target: &Path) -> io::Result<()> {
        let root_text = root
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_lowercase();
        let target_text = target.to_string_lossy().to_lowercase();
        if target_text == root_text
            || target_text
                .strip_prefix(&root_text)
                .is_some_and(|rest| rest.starts_with(['\\', '/']))
        {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "resolved handle escapes workspace",
            ))
        }
    }

    fn reject_device_path(path: &Path) -> io::Result<()> {
        let upper = path.as_os_str().to_string_lossy().to_ascii_uppercase();
        if upper.starts_with(r"\\.\")
            || upper.starts_with(r"\\?\GLOBALROOT")
            || upper.starts_with(r"\??\")
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "device paths are not allowed",
            ));
        }
        Ok(())
    }

    #[allow(dead_code)]
    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }
    fn result(operation: &str, path: &str, destination: Option<String>) -> SafeFileOperationResult {
        SafeFileOperationResult {
            operation: operation.to_string(),
            path: path.to_string(),
            destination,
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    pub(super) fn read_utf8(_: &Path, _: &str) -> io::Result<String> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "safe file editing is only available on Windows",
        ))
    }
    pub(super) fn execute_operations(
        _: &Path,
        _: &[SafeFileOperation],
    ) -> io::Result<Vec<SafeFileOperationResult>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "safe file editing is only available on Windows",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_absolute_and_parent_paths() {
        assert!(checked_relative("../escape.txt").is_err());
        assert!(checked_relative("C:\\escape.txt").is_err());
        assert!(checked_relative(r"\\server\share\escape.txt").is_err());
        assert!(checked_relative("safe/file.txt").is_ok());
        #[cfg(windows)]
        {
            assert!(checked_relative("safe/file.txt:stream").is_err());
            assert!(checked_relative("safe/CON.txt").is_err());
            assert!(checked_relative("safe/trailing. ").is_err());
        }
    }

    #[cfg(windows)]
    mod windows {
        use super::*;
        use std::{fs, os::windows::fs::symlink_dir};
        use uuid::Uuid;

        struct TempDir(PathBuf);
        impl TempDir {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!("codemax-safe-fs-{}", Uuid::new_v4()));
                fs::create_dir(&path).unwrap();
                Self(path)
            }
        }
        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        #[test]
        fn applies_handle_pinned_file_lifecycle_without_temp_residue() {
            let temp = TempDir::new();
            execute_operations(
                &temp.0,
                &[SafeFileOperation::CreateDirectory { path: "src".into() }],
            )
            .unwrap();
            execute_operations(
                &temp.0,
                &[SafeFileOperation::Create {
                    path: "src/a.txt".into(),
                    content: "one".into(),
                }],
            )
            .unwrap();
            execute_operations(
                &temp.0,
                &[SafeFileOperation::Update {
                    path: "src/a.txt".into(),
                    content: "two".into(),
                }],
            )
            .unwrap();
            execute_operations(
                &temp.0,
                &[SafeFileOperation::Rename {
                    path: "src/a.txt".into(),
                    destination: "src/b.txt".into(),
                }],
            )
            .unwrap();
            assert_eq!(fs::read_to_string(temp.0.join("src/b.txt")).unwrap(), "two");
            execute_operations(
                &temp.0,
                &[SafeFileOperation::Delete {
                    path: "src/b.txt".into(),
                }],
            )
            .unwrap();
            assert!(!temp.0.join("src/b.txt").exists());
            assert!(fs::read_dir(temp.0.join("src")).unwrap().all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".codemax-safe-")));
        }

        #[test]
        fn refuses_reparse_parent_without_touching_outside_file() {
            let temp = TempDir::new();
            let outside = TempDir::new();
            fs::write(outside.0.join("secret.txt"), "outside").unwrap();
            if symlink_dir(&outside.0, temp.0.join("link")).is_err() {
                return;
            }
            let error = execute_operations(
                &temp.0,
                &[SafeFileOperation::Update {
                    path: "link/secret.txt".into(),
                    content: "changed".into(),
                }],
            )
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(
                fs::read_to_string(outside.0.join("secret.txt")).unwrap(),
                "outside"
            );
        }

        #[test]
        fn locked_target_cannot_be_replaced_during_operation() {
            let temp = TempDir::new();
            fs::write(temp.0.join("target.txt"), "original").unwrap();
            platform::assert_target_locked(&temp.0, Path::new("target.txt")).unwrap();
            assert_eq!(
                fs::read_to_string(temp.0.join("target.txt")).unwrap(),
                "original"
            );
        }

        #[test]
        fn locked_parent_cannot_be_replaced_during_operation() {
            let temp = TempDir::new();
            fs::create_dir(temp.0.join("parent")).unwrap();
            platform::assert_parent_locked(&temp.0, Path::new("parent/file.txt")).unwrap();
            assert!(temp.0.join("parent").is_dir());
        }

        #[test]
        fn failed_create_leaves_no_partial_or_temp_file() {
            let temp = TempDir::new();
            fs::write(temp.0.join("exists.txt"), "original").unwrap();
            assert!(execute_operations(
                &temp.0,
                &[SafeFileOperation::Create {
                    path: "exists.txt".into(),
                    content: "changed".into()
                }]
            )
            .is_err());
            assert_eq!(
                fs::read_to_string(temp.0.join("exists.txt")).unwrap(),
                "original"
            );
            assert!(fs::read_dir(&temp.0).unwrap().all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".codemax-safe-")));
        }
    }
}
