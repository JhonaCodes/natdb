//! On Apple targets LMDB must lock with POSIX semaphores: the iOS and macOS App
//! Sandbox forbid SysV IPC, so a SysV semaphore set would make every
//! `mdb_env_open` fail inside a sandboxed app.
//!
//! With SysV semaphores LMDB creates a set keyed by `ftok(<lock file>, 'M')`
//! (`mdb_env_setup_locks` in `mdb.c`); this test checks that no such set exists
//! while an environment is open.
#![cfg(target_vendor = "apple")]

use std::ffi::{CString, c_int};
use std::io;
use std::ptr;

use natdb_sys::{MDB_SUCCESS, MDB_env, mdb_env_close, mdb_env_create, mdb_env_open};

fn check(rc: c_int) {
    assert_eq!(rc, MDB_SUCCESS, "LMDB call failed with code {rc}");
}

#[test]
fn open_environment_uses_no_sysv_semaphore_set() {
    let dir = tempfile::tempdir().expect("create temporary directory");
    let env_path = CString::new(dir.path().to_str().expect("UTF-8 temp path")).expect("path without NUL");
    let lock_path =
        CString::new(dir.path().join("lock.mdb").to_str().expect("UTF-8 temp path")).expect("path without NUL");
    let mut env: *mut MDB_env = ptr::null_mut();

    // SAFETY: `env` is created before use; `env_path` outlives the call.
    unsafe {
        check(mdb_env_create(&mut env));
        check(mdb_env_open(env, env_path.as_ptr(), 0, 0o664));
    }

    // SAFETY: `lock_path` is a valid NUL-terminated path to the existing lock file.
    let key = unsafe { libc::ftok(lock_path.as_ptr(), c_int::from(b'M')) };
    assert_ne!(key, -1, "ftok failed: {}", io::Error::last_os_error());
    // SAFETY: `semget` has no memory-safety preconditions; nsems = 0 and no flags
    // only look up an existing set.
    let semid = unsafe { libc::semget(key, 0, 0) };
    let lookup_error = io::Error::last_os_error();

    // SAFETY: `env` was opened above and is closed exactly once.
    unsafe { mdb_env_close(env) };

    assert_eq!(semid, -1, "LMDB created a SysV semaphore set for the lock file");
    assert_eq!(lookup_error.raw_os_error(), Some(libc::ENOENT), "unexpected semget error: {lookup_error}");
}
