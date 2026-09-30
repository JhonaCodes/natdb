//! Smoke test of the raw FFI surface: create an environment, write through a
//! named database and copy the environment to a file descriptor.

use std::ffi::{CString, c_int, c_void};
use std::fs::File;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::ptr;

use natdb_sys::{
    MDB_CREATE, MDB_SUCCESS, MDB_dbi, MDB_env, MDB_txn, MDB_val, mdb_dbi_close, mdb_dbi_open, mdb_env_close,
    mdb_env_copyfd, mdb_env_create, mdb_env_open, mdb_env_set_maxdbs, mdb_filehandle_t, mdb_put, mdb_txn_begin,
    mdb_txn_commit,
};

fn check(rc: c_int) {
    assert_eq!(rc, MDB_SUCCESS, "LMDB call failed with code {rc}");
}

#[cfg(unix)]
fn file_handle(file: &File) -> mdb_filehandle_t {
    file.as_raw_fd()
}

#[cfg(windows)]
fn file_handle(file: &File) -> mdb_filehandle_t {
    file.as_raw_handle()
}

#[test]
fn test_simple() {
    let dir = tempfile::tempdir().expect("create temporary directory");
    let env_path = CString::new(dir.path().to_str().expect("UTF-8 temp path")).expect("path without NUL");
    let subdb = CString::new("subdb").expect("name without NUL");
    let mut key_bytes = *b"foo";
    let mut data_bytes = *b"bar";

    let mut env: *mut MDB_env = ptr::null_mut();
    let mut dbi: MDB_dbi = 0;
    let mut txn: *mut MDB_txn = ptr::null_mut();
    let mut key = MDB_val {
        mv_size: key_bytes.len(),
        mv_data: key_bytes.as_mut_ptr() as *mut c_void,
    };
    let mut data = MDB_val {
        mv_size: data_bytes.len(),
        mv_data: data_bytes.as_mut_ptr() as *mut c_void,
    };

    // SAFETY: every handle is created by LMDB before use and closed once at the
    // end; `env_path`, `subdb`, `key_bytes` and `data_bytes` outlive all calls.
    unsafe {
        check(mdb_env_create(&mut env));
        check(mdb_env_set_maxdbs(env, 2));
        check(mdb_env_open(env, env_path.as_ptr(), 0, 0o664));

        check(mdb_txn_begin(env, ptr::null_mut(), 0, &mut txn));
        check(mdb_dbi_open(txn, subdb.as_ptr(), MDB_CREATE, &mut dbi));
        check(mdb_txn_commit(txn));

        check(mdb_txn_begin(env, ptr::null_mut(), 0, &mut txn));
        check(mdb_put(txn, dbi, &mut key, &mut data, 0));
        check(mdb_txn_commit(txn));
    }

    let copy = File::create(dir.path().join("copytestdb.mdb")).expect("create copy file");

    // SAFETY: `env` is open and `copy` stays open for the duration of the call.
    unsafe {
        check(mdb_env_copyfd(env, file_handle(&copy)));
        mdb_dbi_close(env, dbi);
        mdb_env_close(env);
    }
}
