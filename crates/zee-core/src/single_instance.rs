use std::fs::{File, OpenOptions};

/// Cross-platform single instance lock to prevent duplicate GUI/CLI application instances.
pub struct SingleInstanceLock {
    #[cfg(unix)]
    _file: File,
    #[cfg(windows)]
    _handle: *mut std::ffi::c_void,
}

unsafe impl Send for SingleInstanceLock {}
unsafe impl Sync for SingleInstanceLock {}

impl SingleInstanceLock {
    /// Try to acquire an exclusive single-instance lock with the given unique identifier.
    /// Returns `Some(SingleInstanceLock)` if this is the only instance running,
    /// or `None` if another instance is already holding the lock.
    pub fn try_acquire(name: &str) -> Option<Self> {
        #[cfg(unix)]
        {
            let lock_path = crate::config::Config::config_dir()
                .map(|d| d.join(format!("{}.lock", name)))
                .unwrap_or_else(|| std::env::temp_dir().join(format!("{}.lock", name)));
            if let Some(parent) = lock_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&lock_path)
                .ok()?;

            use std::os::unix::io::AsRawFd;
            let fd = file.as_raw_fd();
            let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
            if ret == 0 {
                Some(Self { _file: file })
            } else {
                None
            }
        }

        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let mutex_name: Vec<u16> = std::ffi::OsStr::new(&format!("Local\\Zee_{}", name))
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            extern "system" {
                fn CreateMutexW(
                    lpMutexAttributes: *mut std::ffi::c_void,
                    bInitialOwner: i32,
                    lpName: *const u16,
                ) -> *mut std::ffi::c_void;
                fn GetLastError() -> u32;
                fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
            }
            const ERROR_ALREADY_EXISTS: u32 = 183;
            let handle = unsafe { CreateMutexW(std::ptr::null_mut(), 1, mutex_name.as_ptr()) };
            if handle.is_null() {
                return None;
            }
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                unsafe { CloseHandle(handle) };
                return None;
            }
            Some(Self { _handle: handle })
        }
    }
}

#[cfg(windows)]
impl Drop for SingleInstanceLock {
    fn drop(&mut self) {
        extern "system" {
            fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
        }
        if !self._handle.is_null() {
            unsafe { CloseHandle(self._handle) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_instance_lock_mutual_exclusion() {
        let lock1 = SingleInstanceLock::try_acquire("test_mutex_unique_abc123");
        assert!(lock1.is_some(), "First lock acquisition should succeed");

        let lock2 = SingleInstanceLock::try_acquire("test_mutex_unique_abc123");
        assert!(lock2.is_none(), "Concurrent second lock acquisition must fail");

        drop(lock1);

        let lock3 = SingleInstanceLock::try_acquire("test_mutex_unique_abc123");
        assert!(lock3.is_some(), "Acquisition after dropping previous lock must succeed");
    }
}
