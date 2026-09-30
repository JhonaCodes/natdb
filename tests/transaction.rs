//! Integration tests for read-only, read-write and nested transactions.

mod common;

use std::io::Write;
use std::sync::{Arc, Barrier};
use std::thread::{self, JoinHandle};

use natdb::{Cursor, DatabaseFlags, Environment, Error, Transaction, WriteFlags};

#[test]
fn test_put_get_del() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.open_db(None).unwrap();

    let mut txn = env.begin_rw_txn().unwrap();
    txn.put(db, b"key1", b"val1", WriteFlags::empty()).unwrap();
    txn.put(db, b"key2", b"val2", WriteFlags::empty()).unwrap();
    txn.put(db, b"key3", b"val3", WriteFlags::empty()).unwrap();
    txn.commit().unwrap();

    let mut txn = env.begin_rw_txn().unwrap();
    assert_eq!(b"val1", txn.get(db, b"key1").unwrap());
    assert_eq!(b"val2", txn.get(db, b"key2").unwrap());
    assert_eq!(b"val3", txn.get(db, b"key3").unwrap());
    assert_eq!(txn.get(db, b"key"), Err(Error::NotFound));

    txn.del(db, b"key1", None).unwrap();
    assert_eq!(txn.get(db, b"key1"), Err(Error::NotFound));
}

#[test]
fn test_put_get_del_multi() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.create_db(None, DatabaseFlags::DUP_SORT).unwrap();

    let mut txn = env.begin_rw_txn().unwrap();
    txn.put(db, b"key1", b"val1", WriteFlags::empty()).unwrap();
    txn.put(db, b"key1", b"val2", WriteFlags::empty()).unwrap();
    txn.put(db, b"key1", b"val3", WriteFlags::empty()).unwrap();
    txn.put(db, b"key2", b"val1", WriteFlags::empty()).unwrap();
    txn.put(db, b"key2", b"val2", WriteFlags::empty()).unwrap();
    txn.put(db, b"key2", b"val3", WriteFlags::empty()).unwrap();
    txn.put(db, b"key3", b"val1", WriteFlags::empty()).unwrap();
    txn.put(db, b"key3", b"val2", WriteFlags::empty()).unwrap();
    txn.put(db, b"key3", b"val3", WriteFlags::empty()).unwrap();
    txn.commit().unwrap();

    let txn = env.begin_rw_txn().unwrap();
    {
        let mut cur = txn.open_ro_cursor(db).unwrap();
        let iter = cur.iter_dup_of(b"key1");
        let vals = iter.map(|x| x.unwrap()).map(|(_, x)| x).collect::<Vec<_>>();
        assert_eq!(vals, vec![b"val1", b"val2", b"val3"]);
    }
    txn.commit().unwrap();

    let mut txn = env.begin_rw_txn().unwrap();
    txn.del(db, b"key1", Some(b"val2")).unwrap();
    txn.del(db, b"key2", None).unwrap();
    txn.commit().unwrap();

    let txn = env.begin_rw_txn().unwrap();
    {
        let mut cur = txn.open_ro_cursor(db).unwrap();
        let iter = cur.iter_dup_of(b"key1");
        let vals = iter.map(|x| x.unwrap()).map(|(_, x)| x).collect::<Vec<_>>();
        assert_eq!(vals, vec![b"val1", b"val3"]);

        let iter = cur.iter_dup_of(b"key2");
        assert_eq!(0, iter.count());
    }
    txn.commit().unwrap();
}

#[test]
fn test_reserve() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.open_db(None).unwrap();

    let mut txn = env.begin_rw_txn().unwrap();
    {
        let mut writer = txn.reserve(db, b"key1", 4, WriteFlags::empty()).unwrap();
        writer.write_all(b"val1").unwrap();
    }
    txn.commit().unwrap();

    let mut txn = env.begin_rw_txn().unwrap();
    assert_eq!(b"val1", txn.get(db, b"key1").unwrap());
    assert_eq!(txn.get(db, b"key"), Err(Error::NotFound));

    txn.del(db, b"key1", None).unwrap();
    assert_eq!(txn.get(db, b"key1"), Err(Error::NotFound));
}

#[test]
fn test_inactive_txn() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.open_db(None).unwrap();

    {
        let mut txn = env.begin_rw_txn().unwrap();
        txn.put(db, b"key", b"val", WriteFlags::empty()).unwrap();
        txn.commit().unwrap();
    }

    let txn = env.begin_ro_txn().unwrap();
    let inactive = txn.reset();
    let active = inactive.renew().unwrap();
    assert!(active.get(db, b"key").is_ok());
}

#[test]
fn test_nested_txn() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.open_db(None).unwrap();

    let mut txn = env.begin_rw_txn().unwrap();
    txn.put(db, b"key1", b"val1", WriteFlags::empty()).unwrap();

    {
        let mut nested = txn.begin_nested_txn().unwrap();
        nested.put(db, b"key2", b"val2", WriteFlags::empty()).unwrap();
        assert_eq!(nested.get(db, b"key1").unwrap(), b"val1");
        assert_eq!(nested.get(db, b"key2").unwrap(), b"val2");
    }

    assert_eq!(txn.get(db, b"key1").unwrap(), b"val1");
    assert_eq!(txn.get(db, b"key2"), Err(Error::NotFound));
}

#[test]
fn test_clear_db() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.open_db(None).unwrap();

    {
        let mut txn = env.begin_rw_txn().unwrap();
        txn.put(db, b"key", b"val", WriteFlags::empty()).unwrap();
        txn.commit().unwrap();
    }

    {
        let mut txn = env.begin_rw_txn().unwrap();
        txn.clear_db(db).unwrap();
        txn.commit().unwrap();
    }

    let txn = env.begin_ro_txn().unwrap();
    assert_eq!(txn.get(db, b"key"), Err(Error::NotFound));
}

#[test]
fn test_drop_db() {
    let dir = common::temp_dir();
    let env = Environment::new().set_max_dbs(2).open(dir.path()).unwrap();
    let db = env.create_db(Some("test"), DatabaseFlags::empty()).unwrap();

    {
        let mut txn = env.begin_rw_txn().unwrap();
        txn.put(db, b"key", b"val", WriteFlags::empty()).unwrap();
        txn.commit().unwrap();
    }
    {
        let mut txn = env.begin_rw_txn().unwrap();
        // SAFETY: no other thread, transaction or cursor references `db`.
        unsafe {
            txn.drop_db(db).unwrap();
        }
        txn.commit().unwrap();
    }

    assert_eq!(env.open_db(Some("test")), Err(Error::NotFound));
}

#[test]
fn test_concurrent_readers_single_writer() {
    let dir = common::temp_dir();
    let env: Arc<Environment> = Arc::new(Environment::new().open(dir.path()).unwrap());

    let n = 10usize; // Number of concurrent readers
    let barrier = Arc::new(Barrier::new(n + 1));
    let mut threads: Vec<JoinHandle<bool>> = Vec::with_capacity(n);

    let key = b"key";
    let val = b"val";

    for _ in 0..n {
        let reader_env = env.clone();
        let reader_barrier = barrier.clone();

        threads.push(thread::spawn(move || {
            let db = reader_env.open_db(None).unwrap();
            {
                let txn = reader_env.begin_ro_txn().unwrap();
                assert_eq!(txn.get(db, key), Err(Error::NotFound));
                txn.abort();
            }
            reader_barrier.wait();
            reader_barrier.wait();
            {
                let txn = reader_env.begin_ro_txn().unwrap();
                txn.get(db, key).unwrap() == val
            }
        }));
    }

    let db = env.open_db(None).unwrap();
    let mut txn = env.begin_rw_txn().unwrap();
    barrier.wait();
    txn.put(db, key, val, WriteFlags::empty()).unwrap();
    txn.commit().unwrap();
    barrier.wait();

    assert!(threads.into_iter().all(|b| b.join().unwrap()))
}

#[test]
fn test_concurrent_writers() {
    let dir = common::temp_dir();
    let env = Arc::new(Environment::new().open(dir.path()).unwrap());

    let n = 10usize; // Number of concurrent writers
    let mut threads: Vec<JoinHandle<bool>> = Vec::with_capacity(n);

    let key = "key";
    let val = "val";

    for i in 0..n {
        let writer_env = env.clone();

        threads.push(thread::spawn(move || {
            let db = writer_env.open_db(None).unwrap();
            let mut txn = writer_env.begin_rw_txn().unwrap();
            txn.put(db, &format!("{}{}", key, i), &format!("{}{}", val, i), WriteFlags::empty()).unwrap();
            txn.commit().is_ok()
        }));
    }
    assert!(threads.into_iter().all(|b| b.join().unwrap()));

    let db = env.open_db(None).unwrap();
    let txn = env.begin_ro_txn().unwrap();

    for i in 0..n {
        assert_eq!(format!("{}{}", val, i).as_bytes(), txn.get(db, &format!("{}{}", key, i)).unwrap());
    }
}

#[test]
fn test_stat() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.create_db(None, DatabaseFlags::empty()).unwrap();

    let mut txn = env.begin_rw_txn().unwrap();
    txn.put(db, b"key1", b"val1", WriteFlags::empty()).unwrap();
    txn.put(db, b"key2", b"val2", WriteFlags::empty()).unwrap();
    txn.put(db, b"key3", b"val3", WriteFlags::empty()).unwrap();
    txn.commit().unwrap();

    {
        let txn = env.begin_ro_txn().unwrap();
        let stat = txn.stat(db).unwrap();
        assert_eq!(stat.entries(), 3);
    }

    let mut txn = env.begin_rw_txn().unwrap();
    txn.del(db, b"key1", None).unwrap();
    txn.del(db, b"key2", None).unwrap();
    txn.commit().unwrap();

    {
        let txn = env.begin_ro_txn().unwrap();
        let stat = txn.stat(db).unwrap();
        assert_eq!(stat.entries(), 1);
    }

    let mut txn = env.begin_rw_txn().unwrap();
    txn.put(db, b"key4", b"val4", WriteFlags::empty()).unwrap();
    txn.put(db, b"key5", b"val5", WriteFlags::empty()).unwrap();
    txn.put(db, b"key6", b"val6", WriteFlags::empty()).unwrap();
    txn.commit().unwrap();

    {
        let txn = env.begin_ro_txn().unwrap();
        let stat = txn.stat(db).unwrap();
        assert_eq!(stat.entries(), 4);
    }
}

#[test]
fn test_stat_dupsort() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.create_db(None, DatabaseFlags::DUP_SORT).unwrap();

    let mut txn = env.begin_rw_txn().unwrap();
    txn.put(db, b"key1", b"val1", WriteFlags::empty()).unwrap();
    txn.put(db, b"key1", b"val2", WriteFlags::empty()).unwrap();
    txn.put(db, b"key1", b"val3", WriteFlags::empty()).unwrap();
    txn.put(db, b"key2", b"val1", WriteFlags::empty()).unwrap();
    txn.put(db, b"key2", b"val2", WriteFlags::empty()).unwrap();
    txn.put(db, b"key2", b"val3", WriteFlags::empty()).unwrap();
    txn.put(db, b"key3", b"val1", WriteFlags::empty()).unwrap();
    txn.put(db, b"key3", b"val2", WriteFlags::empty()).unwrap();
    txn.put(db, b"key3", b"val3", WriteFlags::empty()).unwrap();
    txn.commit().unwrap();

    {
        let txn = env.begin_ro_txn().unwrap();
        let stat = txn.stat(db).unwrap();
        assert_eq!(stat.entries(), 9);
    }

    let mut txn = env.begin_rw_txn().unwrap();
    txn.del(db, b"key1", Some(b"val2")).unwrap();
    txn.del(db, b"key2", None).unwrap();
    txn.commit().unwrap();

    {
        let txn = env.begin_ro_txn().unwrap();
        let stat = txn.stat(db).unwrap();
        assert_eq!(stat.entries(), 5);
    }

    let mut txn = env.begin_rw_txn().unwrap();
    txn.put(db, b"key4", b"val1", WriteFlags::empty()).unwrap();
    txn.put(db, b"key4", b"val2", WriteFlags::empty()).unwrap();
    txn.put(db, b"key4", b"val3", WriteFlags::empty()).unwrap();
    txn.commit().unwrap();

    {
        let txn = env.begin_ro_txn().unwrap();
        let stat = txn.stat(db).unwrap();
        assert_eq!(stat.entries(), 8);
    }
}
