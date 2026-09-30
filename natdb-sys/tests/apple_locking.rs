//! On Apple targets LMDB locks with process-shared pthread mutexes stored in
//! the lock file, because the App Sandbox forbids SysV semaphores and named
//! POSIX semaphores (see `build.rs`). This test exercises the writer mutex from
//! several threads: every increment must survive.
#![cfg(target_vendor = "apple")]

use std::ffi::{CString, c_int, c_uint};
use std::ptr;
use std::sync::Arc;
use std::thread;

use natdb_sys::{
    MDB_SUCCESS, MDB_dbi, MDB_env, MDB_txn, MDB_val, mdb_dbi_open, mdb_env_close, mdb_env_create, mdb_env_open,
    mdb_get, mdb_put, mdb_txn_abort, mdb_txn_begin, mdb_txn_commit,
};

fn check(rc: c_int) {
    assert_eq!(rc, MDB_SUCCESS, "LMDB call failed with code {rc}");
}

/// An open environment shared between threads.
struct Env(*mut MDB_env);

// SAFETY: an LMDB environment may be used from several threads; each thread
// below uses its own transactions.
unsafe impl Send for Env {}
// SAFETY: as above.
unsafe impl Sync for Env {}

#[test]
fn concurrent_writers_are_serialized() {
    let dir = tempfile::tempdir().expect("create temporary directory");
    let path = CString::new(dir.path().to_str().expect("UTF-8 temp path")).expect("path without NUL");
    let mut raw: *mut MDB_env = ptr::null_mut();
    // SAFETY: `raw` is created before use; `path` outlives the call.
    unsafe {
        check(mdb_env_create(&mut raw));
        check(mdb_env_open(raw, path.as_ptr(), 0, 0o664));
    }
    let env = Arc::new(Env(raw));
    let key = b"counter";
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let env = Arc::clone(&env);
            thread::spawn(move || {
                for _ in 0..100 {
                    // SAFETY: the environment is open; every transaction is
                    // committed or aborted before the next one starts on this
                    // thread, and the values point to live buffers.
                    unsafe {
                        let mut txn: *mut MDB_txn = ptr::null_mut();
                        check(mdb_txn_begin(env.0, ptr::null_mut(), 0, &mut txn));
                        let mut dbi: MDB_dbi = 0;
                        check(mdb_dbi_open(txn, ptr::null(), 0 as c_uint, &mut dbi));
                        let mut k = MDB_val {
                            mv_size: key.len(),
                            mv_data: key.as_ptr() as *mut _,
                        };
                        let mut v = MDB_val {
                            mv_size: 0,
                            mv_data: ptr::null_mut(),
                        };
                        let current = if mdb_get(txn, dbi, &mut k, &mut v) == MDB_SUCCESS {
                            let bytes = std::slice::from_raw_parts(v.mv_data as *const u8, v.mv_size);
                            u64::from_le_bytes(bytes.try_into().expect("8-byte counter"))
                        } else {
                            0
                        };
                        let next = (current + 1).to_le_bytes();
                        let mut value = MDB_val {
                            mv_size: next.len(),
                            mv_data: next.as_ptr() as *mut _,
                        };
                        let rc = mdb_put(txn, dbi, &mut k, &mut value, 0);
                        if rc != MDB_SUCCESS {
                            mdb_txn_abort(txn);
                            panic!("mdb_put failed with code {rc}");
                        }
                        check(mdb_txn_commit(txn));
                    }
                }
            })
        })
        .collect();
    for handle in threads {
        handle.join().expect("writer thread");
    }
    // SAFETY: all threads finished; read the final value in one transaction.
    let total = unsafe {
        let mut txn: *mut MDB_txn = ptr::null_mut();
        check(mdb_txn_begin(env.0, ptr::null_mut(), natdb_sys::MDB_RDONLY, &mut txn));
        let mut dbi: MDB_dbi = 0;
        check(mdb_dbi_open(txn, ptr::null(), 0, &mut dbi));
        let mut k = MDB_val {
            mv_size: key.len(),
            mv_data: key.as_ptr() as *mut _,
        };
        let mut v = MDB_val {
            mv_size: 0,
            mv_data: ptr::null_mut(),
        };
        check(mdb_get(txn, dbi, &mut k, &mut v));
        let bytes = std::slice::from_raw_parts(v.mv_data as *const u8, v.mv_size);
        let total = u64::from_le_bytes(bytes.try_into().expect("8-byte counter"));
        mdb_txn_abort(txn);
        total
    };
    // SAFETY: the environment is closed exactly once, after every transaction.
    unsafe { mdb_env_close(env.0) };
    assert_eq!(total, 800);
}
