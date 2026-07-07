use std::{
    fs,
    path::{Path, PathBuf},
};

use thiserror::Error;

const SECRET_REF_PREFIX: &str = "secret://codemax/model-config/";

#[derive(Debug, Error)]
pub enum SecretStoreError {
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid secret reference")]
    InvalidSecretRef,
    #[error("invalid model config id: {0}")]
    InvalidConfigId(String),
}

pub type SecretStoreResult<T> = Result<T, SecretStoreError>;

#[derive(Debug, Clone)]
pub struct SecretStore {
    root: PathBuf,
}

impl SecretStore {
    pub fn new(app_data_dir: impl AsRef<Path>) -> Self {
        Self {
            root: app_data_dir.as_ref().join("secrets").join("model-configs"),
        }
    }

    pub fn storage_kind(&self) -> &'static str {
        platform::STORAGE_KIND
    }

    pub fn storage_dir(&self) -> &Path {
        &self.root
    }

    pub fn put_model_api_key(&self, config_id: &str, api_key: &str) -> SecretStoreResult<String> {
        let config_id = validate_config_id(config_id)?;
        fs::create_dir_all(&self.root)?;

        let protected = platform::protect(api_key.as_bytes())?;
        fs::write(self.secret_path(config_id), protected)?;

        Ok(secret_ref(config_id))
    }

    pub fn read_secret_ref(&self, secret_ref: &str) -> SecretStoreResult<Option<String>> {
        let config_id = config_id_from_secret_ref(secret_ref)?;
        let path = self.secret_path(config_id);
        if !path.is_file() {
            return Ok(None);
        }

        let protected = fs::read(path)?;
        let plain = platform::unprotect(&protected)?;

        Ok(Some(String::from_utf8_lossy(&plain).to_string()))
    }

    pub fn remove_secret_ref(&self, secret_ref: &str) -> SecretStoreResult<()> {
        let config_id = config_id_from_secret_ref(secret_ref)?;
        let path = self.secret_path(config_id);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn secret_exists(&self, secret_ref: &str) -> bool {
        config_id_from_secret_ref(secret_ref)
            .map(|config_id| self.secret_path(config_id).is_file())
            .unwrap_or(false)
    }

    pub fn secret_location(&self, secret_ref: &str) -> SecretStoreResult<PathBuf> {
        let config_id = config_id_from_secret_ref(secret_ref)?;
        Ok(self.secret_path(config_id))
    }

    fn secret_path(&self, config_id: &str) -> PathBuf {
        self.root.join(format!("{config_id}.bin"))
    }
}

fn secret_ref(config_id: &str) -> String {
    format!("{SECRET_REF_PREFIX}{config_id}")
}

fn config_id_from_secret_ref(secret_ref: &str) -> SecretStoreResult<&str> {
    let config_id = secret_ref
        .strip_prefix(SECRET_REF_PREFIX)
        .ok_or(SecretStoreError::InvalidSecretRef)?;
    validate_config_id(config_id)
}

fn validate_config_id(config_id: &str) -> SecretStoreResult<&str> {
    let trimmed = config_id.trim();
    let valid = !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));

    if valid {
        Ok(trimmed)
    } else {
        Err(SecretStoreError::InvalidConfigId(config_id.to_string()))
    }
}

#[cfg(windows)]
mod platform {
    use std::{io, ptr};

    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };

    pub const STORAGE_KIND: &str = "windows-dpapi";

    pub fn protect(data: &[u8]) -> io::Result<Vec<u8>> {
        let mut input = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };

        let ok = unsafe {
            CryptProtectData(
                &mut input,
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };

        if ok == 0 {
            return Err(io::Error::last_os_error());
        }

        unsafe { take_blob(output) }
    }

    pub fn unprotect(data: &[u8]) -> io::Result<Vec<u8>> {
        let mut input = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: ptr::null_mut(),
        };

        let ok = unsafe {
            CryptUnprotectData(
                &mut input,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };

        if ok == 0 {
            return Err(io::Error::last_os_error());
        }

        unsafe { take_blob(output) }
    }

    unsafe fn take_blob(blob: CRYPT_INTEGER_BLOB) -> io::Result<Vec<u8>> {
        if blob.pbData.is_null() {
            return Ok(Vec::new());
        }

        let bytes = std::slice::from_raw_parts(blob.pbData, blob.cbData as usize).to_vec();
        LocalFree(blob.pbData.cast());
        Ok(bytes)
    }
}

#[cfg(not(windows))]
mod platform {
    use std::io;

    pub const STORAGE_KIND: &str = "local-file";

    pub fn protect(data: &[u8]) -> io::Result<Vec<u8>> {
        let _ = data;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "secure model key storage is only implemented with Windows DPAPI",
        ))
    }

    pub fn unprotect(data: &[u8]) -> io::Result<Vec<u8>> {
        let _ = data;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "secure model key storage is only implemented with Windows DPAPI",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[cfg(windows)]
    #[test]
    fn model_secret_round_trips_without_plaintext_file_content() {
        let root = std::env::temp_dir().join(format!("codemax-secret-{}", Uuid::new_v4()));
        let store = SecretStore::new(&root);
        let secret = "sk-test-secret-value";

        let secret_ref = store
            .put_model_api_key("model-default", secret)
            .expect("save secret");
        let location = store.secret_location(&secret_ref).expect("secret path");
        let stored = fs::read(&location).expect("read stored secret");

        assert!(store.secret_exists(&secret_ref));
        assert!(!String::from_utf8_lossy(&stored).contains(secret));
        assert_eq!(
            store.read_secret_ref(&secret_ref).expect("read secret"),
            Some(secret.to_string())
        );

        fs::remove_dir_all(root).expect("clean temp secret dir");
    }

    #[test]
    fn rejects_secret_refs_outside_model_config_scope() {
        let root = std::env::temp_dir().join(format!("codemax-secret-{}", Uuid::new_v4()));
        let store = SecretStore::new(&root);

        assert!(store
            .read_secret_ref("secret://codemax/other/model-default")
            .is_err());
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_secret_storage_fails_closed() {
        let root = std::env::temp_dir().join(format!("codemax-secret-{}", Uuid::new_v4()));
        let store = SecretStore::new(&root);

        assert!(store.put_model_api_key("model-default", "sk-test").is_err());
    }
}
