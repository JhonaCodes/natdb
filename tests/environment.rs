//! Integration tests for `Environment` and `EnvironmentBuilder`.

mod common;

use natdb::{Database, DatabaseFlags, Environment, EnvironmentFlags, Transaction, WriteFlags};

/// `MAX_PAGESIZE` in LMDB 1.0.2's `mdb.c`: the cap applied to the OS page size.
const LMDB_MAX_PAGE_SIZE: u32 = 0x10000;

/// Page size LMDB 1.0.2 gives a new environment when none is configured: the OS
/// page size capped at `MAX_PAGESIZE` (`mdb_env_open2` in `mdb.c`). That is 4 KiB
/// on most Linux/Windows/x86 machines but 16 KiB on Apple Silicon.
fn expected_default_page_size() -> u32 {
    os_page_size().min(LMDB_MAX_PAGE_SIZE)
}

/// The OS page size, as LMDB reads it (`sysconf(_SC_PAGE_SIZE)`).
#[cfg(unix)]
fn os_page_size() -> u32 {
    // SAFETY: `sysconf` has no preconditions; it returns -1 on failure.
    let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    u32::try_from(size).expect("sysconf(_SC_PAGESIZE) failed")
}

/// Layout of the Win32 `SYSTEM_INFO` struct, filled by `GetSystemInfo`.
///
/// Only `page_size` is read; the other fields exist to give the struct its C layout.
#[cfg(windows)]
#[repr(C)]
#[allow(dead_code)]
struct SystemInfo {
    processor_architecture: u16,
    reserved: u16,
    page_size: u32,
    minimum_application_address: *mut libc::c_void,
    maximum_application_address: *mut libc::c_void,
    active_processor_mask: usize,
    number_of_processors: u32,
    processor_type: u32,
    allocation_granularity: u32,
    processor_level: u16,
    processor_revision: u16,
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetSystemInfo(info: *mut SystemInfo);
}

/// The OS page size, as LMDB reads it (`GetSystemInfo().dwPageSize`).
#[cfg(windows)]
fn os_page_size() -> u32 {
    let mut info = std::mem::MaybeUninit::<SystemInfo>::uninit();
    // SAFETY: `GetSystemInfo` fully initializes the struct it is given and cannot fail.
    unsafe { GetSystemInfo(info.as_mut_ptr()) };
    // SAFETY: initialized by the call above.
    unsafe { info.assume_init() }.page_size
}

/// Commits `count` single-item transactions whose key and value are the
/// little-endian bytes of `0..count`.
fn put_u64_keys(env: &Environment, db: Database, count: u64) {
    for i in 0..count {
        let value = i.to_le_bytes();
        let mut tx = env.begin_rw_txn().expect("begin_rw_txn");
        tx.put(db, &value, &value, WriteFlags::default()).expect("tx.put");
        tx.commit().expect("tx.commit")
    }
}

#[test]
fn test_open() {
    let dir = common::temp_dir();

    // opening non-existent env with read-only should fail
    assert!(Environment::new().set_flags(EnvironmentFlags::READ_ONLY).open(dir.path()).is_err());

    // opening non-existent env should succeed
    assert!(Environment::new().open(dir.path()).is_ok());

    // opening env with read-only should succeed
    assert!(Environment::new().set_flags(EnvironmentFlags::READ_ONLY).open(dir.path()).is_ok());
}

#[test]
fn test_begin_txn() {
    let dir = common::temp_dir();

    {
        // writable environment
        let env = Environment::new().open(dir.path()).unwrap();

        assert!(env.begin_rw_txn().is_ok());
        assert!(env.begin_ro_txn().is_ok());
    }

    {
        // read-only environment
        let env = Environment::new().set_flags(EnvironmentFlags::READ_ONLY).open(dir.path()).unwrap();

        assert!(env.begin_rw_txn().is_err());
        assert!(env.begin_ro_txn().is_ok());
    }
}

#[test]
fn test_open_db() {
    let dir = common::temp_dir();
    let env = Environment::new().set_max_dbs(1).open(dir.path()).unwrap();

    assert!(env.open_db(None).is_ok());
    assert!(env.open_db(Some("testdb")).is_err());
}

#[test]
fn test_create_db() {
    let dir = common::temp_dir();
    let env = Environment::new().set_max_dbs(11).open(dir.path()).unwrap();
    assert!(env.open_db(Some("testdb")).is_err());
    assert!(env.create_db(Some("testdb"), DatabaseFlags::empty()).is_ok());
    assert!(env.open_db(Some("testdb")).is_ok())
}

#[test]
fn test_close_database() {
    let dir = common::temp_dir();
    let mut env = Environment::new().set_max_dbs(10).open(dir.path()).unwrap();

    let db = env.create_db(Some("db"), DatabaseFlags::empty()).unwrap();
    // SAFETY: single-threaded, no open transaction or cursor references `db`.
    unsafe {
        env.close_db(db);
    }
    assert!(env.open_db(Some("db")).is_ok());
}

#[test]
fn test_sync() {
    let dir = common::temp_dir();
    {
        let env = Environment::new().open(dir.path()).unwrap();
        assert!(env.sync(true).is_ok());
    }
    {
        let env = Environment::new().set_flags(EnvironmentFlags::READ_ONLY).open(dir.path()).unwrap();
        assert!(env.sync(true).is_err());
    }
}

#[test]
fn test_stat() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();

    // Stats should be empty initially.
    let stat = env.stat().unwrap();
    assert_eq!(stat.page_size(), expected_default_page_size());
    assert_eq!(stat.depth(), 0);
    assert_eq!(stat.branch_pages(), 0);
    assert_eq!(stat.leaf_pages(), 0);
    assert_eq!(stat.overflow_pages(), 0);
    assert_eq!(stat.entries(), 0);

    let db = env.open_db(None).unwrap();

    // Write a few small values.
    put_u64_keys(&env, db, 64);

    // Stats should now reflect inserted values.
    let stat = env.stat().unwrap();
    assert_eq!(stat.page_size(), expected_default_page_size());
    assert_eq!(stat.depth(), 1);
    assert_eq!(stat.branch_pages(), 0);
    assert_eq!(stat.leaf_pages(), 1);
    assert_eq!(stat.overflow_pages(), 0);
    assert_eq!(stat.entries(), 64);
}

#[test]
fn test_info() {
    let map_size = 1024 * 1024;
    let dir = common::temp_dir();
    let env = Environment::new().set_map_size(map_size).open(dir.path()).unwrap();

    let info = env.info().unwrap();
    assert_eq!(info.map_size(), map_size);
    assert_eq!(info.last_pgno(), 1);
    assert_eq!(info.last_txnid(), 0);
    // The default max readers is 126.
    assert_eq!(info.max_readers(), 126);
    assert_eq!(info.num_readers(), 0);
}

#[test]
fn test_freelist() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();

    let db = env.open_db(None).unwrap();
    let mut freelist = env.freelist().unwrap();
    assert_eq!(freelist, 0);

    // Write a few small values.
    put_u64_keys(&env, db, 64);
    let mut tx = env.begin_rw_txn().expect("begin_rw_txn");
    tx.clear_db(db).expect("clear");
    tx.commit().expect("tx.commit");

    // Freelist should not be empty after clear_db.
    freelist = env.freelist().unwrap();
    assert!(freelist > 0);
}

#[test]
fn test_set_map_size() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();

    let mut info = env.info().unwrap();
    let default_size = info.map_size();

    // Resizing to 0 merely reloads the map size
    env.set_map_size(0).unwrap();
    info = env.info().unwrap();
    assert_eq!(info.map_size(), default_size);

    env.set_map_size(2 * default_size).unwrap();
    info = env.info().unwrap();
    assert_eq!(info.map_size(), 2 * default_size);

    env.set_map_size(4 * default_size).unwrap();
    info = env.info().unwrap();
    assert_eq!(info.map_size(), 4 * default_size);

    // Decreasing is also fine if the space hasn't been consumed.
    env.set_map_size(2 * default_size).unwrap();
    info = env.info().unwrap();
    assert_eq!(info.map_size(), 2 * default_size);
}
