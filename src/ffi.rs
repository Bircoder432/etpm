use std::ffi::{CString, c_char, c_int};
use std::sync::Mutex;
use tokio::runtime::Runtime;

use crate::{PackageManager, TpmError};

pub struct EtpmManager {
    manager: PackageManager,
    runtime: Runtime,
    last_error: Mutex<Option<String>>,
}

#[repr(C)]
#[allow(dead_code)]
pub enum EtpmStatus {
    EtpmOk = 0,
    EtpmErrNullPtr = 1,
    EtpmErrInvalidUtf8 = 2,
    EtpmErrPackageNotFound = 3,
    EtpmErrIo = 4,
    EtpmErrNetwork = 5,
    EtpmErrInvalidVersion = 6,
    EtpmErrPathTraversal = 7,
    EtpmErrRepository = 8,
    EtpmErrRonParse = 9,
    EtpmErrUrlParse = 10,
    EtpmErrInvalidSignature = 11,
    EtpmErrUnknown = 99,
}

impl From<&TpmError> for EtpmStatus {
    fn from(err: &TpmError) -> Self {
        match err {
            TpmError::PackageNotFound(_, _) => EtpmStatus::EtpmErrPackageNotFound,
            TpmError::Io(_) => EtpmStatus::EtpmErrIo,
            TpmError::Network(_) => EtpmStatus::EtpmErrNetwork,
            TpmError::InvalidVersion(_) => EtpmStatus::EtpmErrInvalidVersion,
            TpmError::PathTraversal => EtpmStatus::EtpmErrPathTraversal,
            TpmError::Repository(_) => EtpmStatus::EtpmErrRepository,
            TpmError::RonParse(_) => EtpmStatus::EtpmErrRonParse,
            TpmError::UrlParse(_) => EtpmStatus::EtpmErrUrlParse,
            TpmError::InvalidSignature => EtpmStatus::EtpmErrInvalidSignature,
        }
    }
}

fn c_str_to_str<'a>(c_str: *const c_char) -> Result<&'a str, EtpmStatus> {
    if c_str.is_null() {
        return Err(EtpmStatus::EtpmErrNullPtr);
    }
    unsafe {
        std::ffi::CStr::from_ptr(c_str)
            .to_str()
            .map_err(|_| EtpmStatus::EtpmErrInvalidUtf8)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn etpm_manager_new() -> *mut EtpmManager {
    let runtime = match Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return std::ptr::null_mut(),
    };

    let manager = Box::new(EtpmManager {
        manager: PackageManager::new(),
        runtime,
        last_error: Mutex::new(None),
    });

    Box::into_raw(manager)
}

#[unsafe(no_mangle)]
pub extern "C" fn etpm_manager_free(ptr: *mut EtpmManager) {
    if !ptr.is_null() {
        unsafe {
            drop(Box::from_raw(ptr));
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn etpm_set_root(ptr: *mut EtpmManager, path: *const c_char) -> EtpmStatus {
    if ptr.is_null() {
        return EtpmStatus::EtpmErrNullPtr;
    }
    let manager = unsafe { &mut *ptr };
    match c_str_to_str(path) {
        Ok(p) => match manager.manager.set_root(p) {
            Ok(_) => EtpmStatus::EtpmOk,
            Err(e) => {
                *manager.last_error.lock().unwrap() = Some(e.to_string());
                (&e).into()
            }
        },
        Err(e) => e,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn etpm_set_packages(ptr: *mut EtpmManager, path: *const c_char) -> EtpmStatus {
    if ptr.is_null() {
        return EtpmStatus::EtpmErrNullPtr;
    }
    let manager = unsafe { &mut *ptr };
    match c_str_to_str(path) {
        Ok(p) => match manager.manager.set_packages(p) {
            Ok(_) => EtpmStatus::EtpmOk,
            Err(e) => {
                *manager.last_error.lock().unwrap() = Some(e.to_string());
                (&e).into()
            }
        },
        Err(e) => e,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn etpm_add_repository(ptr: *mut EtpmManager, url: *const c_char) -> EtpmStatus {
    if ptr.is_null() {
        return EtpmStatus::EtpmErrNullPtr;
    }
    let manager = unsafe { &mut *ptr };
    match c_str_to_str(url) {
        Ok(u) => match manager.manager.add_repository(u) {
            Ok(_) => EtpmStatus::EtpmOk,
            Err(e) => {
                *manager.last_error.lock().unwrap() = Some(e.to_string());
                (&e).into()
            }
        },
        Err(e) => e,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn etpm_add_trusted_key(
    ptr: *mut EtpmManager,
    key_base64: *const c_char,
) -> EtpmStatus {
    if ptr.is_null() {
        return EtpmStatus::EtpmErrNullPtr;
    }
    let manager = unsafe { &mut *ptr };
    match c_str_to_str(key_base64) {
        Ok(k) => match manager.manager.add_trusted_key(k) {
            Ok(_) => EtpmStatus::EtpmOk,
            Err(e) => {
                *manager.last_error.lock().unwrap() = Some(e.to_string());
                (&e).into()
            }
        },
        Err(e) => e,
    }
}

/// Enables or disables the requirement for package signature verification.
/// `allow` should be 1 (true) or 0 (false).
#[unsafe(no_mangle)]
pub extern "C" fn etpm_set_allow_unsigned(ptr: *mut EtpmManager, allow: c_int) -> EtpmStatus {
    if ptr.is_null() {
        return EtpmStatus::EtpmErrNullPtr;
    }
    let manager = unsafe { &mut *ptr };
    manager.manager.set_allow_unsigned(allow != 0);
    EtpmStatus::EtpmOk
}

#[unsafe(no_mangle)]
pub extern "C" fn etpm_fetch_package(
    ptr: *mut EtpmManager,
    name: *const c_char,
    version: *const c_char,
    dest: *const c_char,
    out_path: *mut *mut c_char,
) -> EtpmStatus {
    if ptr.is_null() || out_path.is_null() {
        return EtpmStatus::EtpmErrNullPtr;
    }

    let manager = unsafe { &*ptr };
    let (n, v, d) = match (
        c_str_to_str(name),
        c_str_to_str(version),
        c_str_to_str(dest),
    ) {
        (Ok(n), Ok(v), Ok(d)) => (n, v, d),
        (_, _, Err(e)) | (_, Err(e), _) | (Err(e), _, _) => return e,
    };

    let result = manager
        .runtime
        .block_on(async { manager.manager.fetch_package(n, v, d).await });

    match result {
        Ok(path) => {
            let c_path = CString::new(path.to_string_lossy().as_ref()).unwrap();
            unsafe { *out_path = c_path.into_raw() };
            EtpmStatus::EtpmOk
        }
        Err(e) => {
            *manager.last_error.lock().unwrap() = Some(e.to_string());
            (&e).into()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn etpm_install_package(
    ptr: *mut EtpmManager,
    path: *const c_char,
    name: *const c_char,
    version: *const c_char,
) -> EtpmStatus {
    if ptr.is_null() {
        return EtpmStatus::EtpmErrNullPtr;
    }

    let manager = unsafe { &*ptr };
    let (p, n, v) = match (
        c_str_to_str(path),
        c_str_to_str(name),
        c_str_to_str(version),
    ) {
        (Ok(p), Ok(n), Ok(v)) => (p, n, v),
        (_, _, Err(e)) | (_, Err(e), _) | (Err(e), _, _) => return e,
    };

    let result = manager
        .runtime
        .block_on(async { manager.manager.install_package(p, n, v).await });

    match result {
        Ok(_) => EtpmStatus::EtpmOk,
        Err(e) => {
            *manager.last_error.lock().unwrap() = Some(e.to_string());
            (&e).into()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn etpm_uninstall_package(
    ptr: *mut EtpmManager,
    name: *const c_char,
    version: *const c_char,
) -> EtpmStatus {
    if ptr.is_null() {
        return EtpmStatus::EtpmErrNullPtr;
    }

    let manager = unsafe { &*ptr };
    let (n, v) = match (c_str_to_str(name), c_str_to_str(version)) {
        (Ok(n), Ok(v)) => (n, v),
        (_, Err(e)) | (Err(e), _) => return e,
    };

    let result = manager
        .runtime
        .block_on(async { manager.manager.uninstall_package(n, v).await });

    match result {
        Ok(_) => EtpmStatus::EtpmOk,
        Err(e) => {
            *manager.last_error.lock().unwrap() = Some(e.to_string());
            (&e).into()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn etpm_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            drop(CString::from_raw(ptr));
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn etpm_get_last_error(ptr: *mut EtpmManager) -> *mut c_char {
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    let manager = unsafe { &*ptr };
    let guard = manager.last_error.lock().unwrap();
    match guard.as_ref() {
        Some(err) => CString::new(err.clone()).unwrap().into_raw(),
        None => std::ptr::null_mut(),
    }
}
