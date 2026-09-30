//! Regression tests carried over from upstream `lmdb-rs`.

mod common;

use natdb::{Cursor, DatabaseFlags, Environment, EnvironmentFlags, Error, Transaction, WriteFlags};

/// Regression test for https://github.com/danburkert/lmdb-rs/issues/21.
/// This test reliably segfaults when run against lmbdb compiled with opt level -O3 and newer
/// GCC compilers.
#[test]
fn issue_21_regression() {
    const HEIGHT_KEY: [u8; 1] = [0];

    let dir = common::temp_dir();

    let env = {
        let mut builder = Environment::new();
        builder.set_max_dbs(2);
        builder.set_map_size(1_000_000);
        builder.open(dir.path()).expect("open lmdb env")
    };
    let index = env.create_db(None, DatabaseFlags::DUP_SORT).expect("open index db");

    for height in 0..1000u64 {
        let value = height.to_le_bytes();
        let mut tx = env.begin_rw_txn().expect("begin_rw_txn");
        tx.put(index, &HEIGHT_KEY, &value, WriteFlags::empty()).expect("tx.put");
        tx.commit().expect("tx.commit")
    }
}

/// `begin_nested_txn` used to ignore the return code of `mdb_txn_begin` and
/// hand back a transaction wrapping a null handle. LMDB rejects nested
/// transactions on a `WRITE_MAP` environment, so the call must fail.
#[test]
fn nested_txn_failure_is_reported() {
    let dir = common::temp_dir();
    let env = Environment::new().set_flags(EnvironmentFlags::WRITE_MAP).open(dir.path()).expect("open lmdb env");
    let mut txn = env.begin_rw_txn().expect("begin_rw_txn");

    let nested = txn.begin_nested_txn();

    assert!(nested.is_err(), "a nested transaction on WRITE_MAP must fail");
}

/// A database name with an interior NUL used to panic in `CString::new`.
#[test]
fn database_name_with_nul_is_an_error() {
    let dir = common::temp_dir();
    let env = Environment::new().set_max_dbs(2).open(dir.path()).expect("open lmdb env");

    let db = env.create_db(Some("bad\0name"), DatabaseFlags::empty());

    assert_eq!(db.err(), Some(Error::Invalid));
}

/// A path with an interior NUL is rejected before an environment is created.
#[test]
fn path_with_nul_is_an_error() {
    let dir = common::temp_dir();
    let path = dir.path().join("bad\0path");

    let env = Environment::new().open(&path);

    assert_eq!(env.err(), Some(Error::Invalid));
}

/// https://github.com/danburkert/lmdb-rs/issues/58: deleting one value of a
/// `DUP_SORT` key by passing that value.
#[test]
fn issue_58_del_dup_sort_value() {
    let dir = common::temp_dir();
    let env = Environment::new().set_max_dbs(2).open(dir.path()).expect("open lmdb env");
    let db = env.create_db(Some("dups"), DatabaseFlags::DUP_SORT).expect("create db");
    let (key, val) = ([3u8; 32], [4u8; 32]);

    let mut txn = env.begin_rw_txn().expect("begin_rw_txn");
    txn.put(db, &key, &val, WriteFlags::empty()).expect("put");
    txn.commit().expect("commit");

    let mut txn = env.begin_rw_txn().expect("begin_rw_txn");
    txn.del(db, &key, Some(&val)).expect("del of an existing dup value");
    txn.commit().expect("commit");

    let txn = env.begin_ro_txn().expect("begin_ro_txn");
    assert_eq!(txn.get(db, &key).err(), Some(Error::NotFound));
}

/// https://github.com/danburkert/lmdb-rs/issues/27 and #31: iterating an empty
/// database yields nothing instead of panicking.
#[test]
fn issues_27_31_iterating_an_empty_database() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).expect("open lmdb env");
    let db = env.open_db(None).expect("open db");
    let txn = env.begin_ro_txn().expect("begin_ro_txn");
    let mut cursor = txn.open_ro_cursor(db).expect("open cursor");

    assert_eq!(cursor.iter_start().count(), 0);
    assert_eq!(cursor.iter_from(b"any").count(), 0);
    assert_eq!(cursor.iter().count(), 0);
}

/// https://github.com/danburkert/lmdb-rs/issues/60: safe access to the
/// version of the linked LMDB.
#[test]
fn issue_60_lmdb_version() {
    let version = natdb::version();

    assert_eq!((version.major, version.minor, version.patch), (1, 0, 2));
    assert!(version.string.starts_with("LMDB 1.0.2"), "{}", version.string);
}

/// https://github.com/danburkert/lmdb-rs/pull/43: copying an environment,
/// optionally compacted, while it stays open.
#[test]
fn pr_43_environment_copy() {
    let src = common::temp_dir();
    let dst_plain = common::temp_dir();
    let dst_compact = common::temp_dir();
    let env = Environment::new().open(src.path()).expect("open lmdb env");
    let db = env.open_db(None).expect("open db");
    let mut txn = env.begin_rw_txn().expect("begin_rw_txn");
    for i in 0..1000u32 {
        txn.put(db, &i.to_be_bytes(), &[7u8; 256], WriteFlags::empty()).expect("put");
    }
    for i in 0..900u32 {
        txn.del(db, &i.to_be_bytes(), None).expect("del");
    }
    txn.commit().expect("commit");

    env.copy(dst_plain.path(), false).expect("plain copy");
    env.copy(dst_compact.path(), true).expect("compacted copy");

    let size = |dir: &std::path::Path| std::fs::metadata(dir.join("data.mdb")).expect("data.mdb").len();
    assert!(size(dst_compact.path()) < size(dst_plain.path()), "compaction must shrink the copy");
    let copy = Environment::new().open(dst_compact.path()).expect("open copy");
    let copy_db = copy.open_db(None).expect("open copy db");
    let txn = copy.begin_ro_txn().expect("begin_ro_txn");
    assert_eq!(txn.get(copy_db, &950u32.to_be_bytes()).expect("copied record"), &[7u8; 256][..]);
    let mut cursor = txn.open_ro_cursor(copy_db).expect("cursor");
    assert_eq!(cursor.iter_start().count(), 100);
}
