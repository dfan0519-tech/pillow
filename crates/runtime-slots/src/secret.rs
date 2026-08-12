use std::fmt;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{Ordering, compiler_fence};

use uuid::Uuid;

use crate::RuntimeSlotError;

/// Secret material that cannot be formatted or serialized and is zeroed on drop.
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    pub fn new(value: String) -> Self {
        Self(value.into_bytes())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn into_bytes(mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBytes([REDACTED])")
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        for byte in &mut self.0 {
            // SAFETY: `byte` is a valid uniquely borrowed byte in this vector.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        compiler_fence(Ordering::SeqCst);
    }
}

pub trait SecretStore: Send + Sync {
    fn put(&self, secret_ref: &str, value: &SecretBytes) -> Result<(), RuntimeSlotError>;
    fn get(&self, secret_ref: &str) -> Result<SecretBytes, RuntimeSlotError>;
    fn exists(&self, secret_ref: &str) -> Result<bool, RuntimeSlotError>;
    fn delete(&self, secret_ref: &str) -> Result<(), RuntimeSlotError>;
}

/// DPAPI ciphertext files rooted under Pillow's app-data directory.
///
/// The file contains no plaintext. DPAPI uses the current Windows user scope;
/// no machine-wide key or Pillow-managed master key exists.
pub struct DpapiFileSecretStore {
    root: PathBuf,
}

impl DpapiFileSecretStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path_for(&self, secret_ref: &str) -> Result<PathBuf, RuntimeSlotError> {
        let id = Uuid::parse_str(secret_ref).map_err(|_| RuntimeSlotError::InvalidInput)?;
        Ok(self.root.join(format!("{id}.dpapi")))
    }
}

impl SecretStore for DpapiFileSecretStore {
    fn put(&self, secret_ref: &str, value: &SecretBytes) -> Result<(), RuntimeSlotError> {
        if value.is_empty() || value.as_bytes().len() > 65_536 {
            return Err(RuntimeSlotError::InvalidInput);
        }
        let target = self.path_for(secret_ref)?;
        std::fs::create_dir_all(&self.root)
            .map_err(|_| RuntimeSlotError::SecretStoreUnavailable)?;
        let protected = protect(value.as_bytes())?;
        let temporary = self
            .root
            .join(format!(".{secret_ref}.{}.tmp", Uuid::new_v4()));
        let write_result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|_| RuntimeSlotError::SecretStoreUnavailable)?;
            file.write_all(&protected)
                .and_then(|_| file.sync_all())
                .map_err(|_| RuntimeSlotError::SecretStoreUnavailable)?;
            if target.exists() {
                return Err(RuntimeSlotError::SecretAlreadyExists);
            }
            std::fs::rename(&temporary, &target)
                .map_err(|_| RuntimeSlotError::SecretStoreUnavailable)
        })();
        if write_result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        write_result
    }

    fn get(&self, secret_ref: &str) -> Result<SecretBytes, RuntimeSlotError> {
        let path = self.path_for(secret_ref)?;
        let protected = match std::fs::read(path) {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(RuntimeSlotError::SecretMissing);
            }
            Err(_) => return Err(RuntimeSlotError::SecretStoreUnavailable),
        };
        unprotect(&protected).map(SecretBytes)
    }

    fn exists(&self, secret_ref: &str) -> Result<bool, RuntimeSlotError> {
        Ok(self.path_for(secret_ref)?.is_file())
    }

    fn delete(&self, secret_ref: &str) -> Result<(), RuntimeSlotError> {
        let path = self.path_for(secret_ref)?;
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(RuntimeSlotError::SecretStoreUnavailable),
        }
    }
}

#[cfg(windows)]
fn protect(value: &[u8]) -> Result<Vec<u8>, RuntimeSlotError> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
    };

    let input = blob(value)?;
    let entropy_bytes = b"Pillow/runtime-slot-secret/v1";
    let entropy = blob(entropy_bytes)?;
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: all blobs point at live slices for the duration of the call;
    // `output` is released with LocalFree below.
    let success = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            &entropy,
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if success == 0 || output.pbData.is_null() {
        return Err(RuntimeSlotError::SecretStoreUnavailable);
    }
    // SAFETY: DPAPI returned `cbData` initialized bytes at `pbData`.
    let protected = unsafe {
        std::slice::from_raw_parts(output.pbData.cast_const(), output.cbData as usize).to_vec()
    };
    // SAFETY: DPAPI allocates output with LocalAlloc.
    unsafe {
        let _ = LocalFree(output.pbData.cast());
    }
    Ok(protected)
}

#[cfg(windows)]
fn unprotect(value: &[u8]) -> Result<Vec<u8>, RuntimeSlotError> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
    };

    let input = blob(value)?;
    let entropy_bytes = b"Pillow/runtime-slot-secret/v1";
    let entropy = blob(entropy_bytes)?;
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let mut description = std::ptr::null_mut();
    // SAFETY: input/entropy point at live slices, and returned allocations are
    // released with LocalFree before returning.
    let success = unsafe {
        CryptUnprotectData(
            &input,
            &mut description,
            &entropy,
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if success == 0 || output.pbData.is_null() {
        if !output.pbData.is_null() {
            // SAFETY: DPAPI initialized this LocalAlloc buffer even though the
            // operation did not produce a usable result.
            unsafe {
                zero_local_plaintext(output.pbData, output.cbData as usize);
                let _ = LocalFree(output.pbData.cast());
            }
        }
        if !description.is_null() {
            // SAFETY: DPAPI description allocation uses LocalAlloc.
            unsafe {
                let _ = LocalFree(description.cast());
            }
        }
        return Err(RuntimeSlotError::SecretStoreUnavailable);
    }
    // SAFETY: DPAPI returned `cbData` initialized bytes at `pbData`.
    let plaintext = unsafe {
        std::slice::from_raw_parts(output.pbData.cast_const(), output.cbData as usize).to_vec()
    };
    // SAFETY: DPAPI allocations use LocalAlloc. Erase the DPAPI-owned
    // plaintext copy before releasing it; the returned Vec is zeroed by
    // SecretBytes when its owner drops it.
    unsafe {
        zero_local_plaintext(output.pbData, output.cbData as usize);
        let _ = LocalFree(output.pbData.cast());
        if !description.is_null() {
            let _ = LocalFree(description.cast());
        }
    }
    Ok(plaintext)
}

#[cfg(windows)]
unsafe fn zero_local_plaintext(value: *mut u8, len: usize) {
    for index in 0..len {
        // SAFETY: the caller guarantees `value` identifies `len` writable
        // bytes returned by DPAPI and has not yet been released.
        unsafe { std::ptr::write_volatile(value.add(index), 0) };
    }
    compiler_fence(Ordering::SeqCst);
}

#[cfg(windows)]
fn blob(
    value: &[u8],
) -> Result<windows_sys::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB, RuntimeSlotError> {
    let len = u32::try_from(value.len()).map_err(|_| RuntimeSlotError::InvalidInput)?;
    Ok(
        windows_sys::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB {
            cbData: len,
            pbData: value.as_ptr().cast_mut(),
        },
    )
}

#[cfg(not(windows))]
fn protect(_: &[u8]) -> Result<Vec<u8>, RuntimeSlotError> {
    Err(RuntimeSlotError::SecretStoreUnsupported)
}

#[cfg(not(windows))]
fn unprotect(_: &[u8]) -> Result<Vec<u8>, RuntimeSlotError> {
    Err(RuntimeSlotError::SecretStoreUnsupported)
}
