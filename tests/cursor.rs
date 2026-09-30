//! Integration tests for cursors and their iterators.

mod common;

use natdb::{Cursor, DatabaseFlags, Environment, Result, Transaction, WriteFlags};
use natdb_sys::{
    MDB_FIRST, MDB_FIRST_DUP, MDB_GET_BOTH, MDB_GET_BOTH_RANGE, MDB_GET_CURRENT, MDB_GET_MULTIPLE, MDB_LAST,
    MDB_LAST_DUP, MDB_NEXT, MDB_NEXT_DUP, MDB_NEXT_MULTIPLE, MDB_NEXT_NODUP, MDB_PREV, MDB_PREV_DUP, MDB_PREV_NODUP,
    MDB_SET, MDB_SET_KEY, MDB_SET_RANGE,
};

#[test]
fn test_get() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.open_db(None).unwrap();

    let mut txn = env.begin_rw_txn().unwrap();
    txn.put(db, b"key1", b"val1", WriteFlags::empty()).unwrap();
    txn.put(db, b"key2", b"val2", WriteFlags::empty()).unwrap();
    txn.put(db, b"key3", b"val3", WriteFlags::empty()).unwrap();

    let cursor = txn.open_ro_cursor(db).unwrap();
    assert_eq!((Some(&b"key1"[..]), &b"val1"[..]), cursor.get(None, None, MDB_FIRST).unwrap());
    assert_eq!((Some(&b"key1"[..]), &b"val1"[..]), cursor.get(None, None, MDB_GET_CURRENT).unwrap());
    assert_eq!((Some(&b"key2"[..]), &b"val2"[..]), cursor.get(None, None, MDB_NEXT).unwrap());
    assert_eq!((Some(&b"key1"[..]), &b"val1"[..]), cursor.get(None, None, MDB_PREV).unwrap());
    assert_eq!((Some(&b"key3"[..]), &b"val3"[..]), cursor.get(None, None, MDB_LAST).unwrap());
    assert_eq!((None, &b"val2"[..]), cursor.get(Some(b"key2"), None, MDB_SET).unwrap());
    assert_eq!((Some(&b"key3"[..]), &b"val3"[..]), cursor.get(Some(&b"key3"[..]), None, MDB_SET_KEY).unwrap());
    assert_eq!((Some(&b"key3"[..]), &b"val3"[..]), cursor.get(Some(&b"key2\0"[..]), None, MDB_SET_RANGE).unwrap());
}

#[test]
fn test_get_dup() {
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

    let cursor = txn.open_ro_cursor(db).unwrap();
    assert_eq!((Some(&b"key1"[..]), &b"val1"[..]), cursor.get(None, None, MDB_FIRST).unwrap());
    assert_eq!((None, &b"val1"[..]), cursor.get(None, None, MDB_FIRST_DUP).unwrap());
    assert_eq!((Some(&b"key1"[..]), &b"val1"[..]), cursor.get(None, None, MDB_GET_CURRENT).unwrap());
    assert_eq!((Some(&b"key2"[..]), &b"val1"[..]), cursor.get(None, None, MDB_NEXT_NODUP).unwrap());
    assert_eq!((Some(&b"key2"[..]), &b"val2"[..]), cursor.get(None, None, MDB_NEXT_DUP).unwrap());
    assert_eq!((Some(&b"key2"[..]), &b"val3"[..]), cursor.get(None, None, MDB_NEXT_DUP).unwrap());
    assert!(cursor.get(None, None, MDB_NEXT_DUP).is_err());
    assert_eq!((Some(&b"key2"[..]), &b"val2"[..]), cursor.get(None, None, MDB_PREV_DUP).unwrap());
    assert_eq!((None, &b"val3"[..]), cursor.get(None, None, MDB_LAST_DUP).unwrap());
    assert_eq!((Some(&b"key1"[..]), &b"val3"[..]), cursor.get(None, None, MDB_PREV_NODUP).unwrap());
    assert_eq!((None, &b"val1"[..]), cursor.get(Some(&b"key1"[..]), None, MDB_SET).unwrap());
    assert_eq!((Some(&b"key2"[..]), &b"val1"[..]), cursor.get(Some(&b"key2"[..]), None, MDB_SET_KEY).unwrap());
    assert_eq!((Some(&b"key2"[..]), &b"val1"[..]), cursor.get(Some(&b"key1\0"[..]), None, MDB_SET_RANGE).unwrap());
    assert_eq!((None, &b"val3"[..]), cursor.get(Some(&b"key1"[..]), Some(&b"val3"[..]), MDB_GET_BOTH).unwrap());
    assert_eq!((None, &b"val1"[..]), cursor.get(Some(&b"key2"[..]), Some(&b"val"[..]), MDB_GET_BOTH_RANGE).unwrap());
}

#[test]
fn test_get_dupfixed() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.create_db(None, DatabaseFlags::DUP_SORT | DatabaseFlags::DUP_FIXED).unwrap();

    let mut txn = env.begin_rw_txn().unwrap();
    txn.put(db, b"key1", b"val1", WriteFlags::empty()).unwrap();
    txn.put(db, b"key1", b"val2", WriteFlags::empty()).unwrap();
    txn.put(db, b"key1", b"val3", WriteFlags::empty()).unwrap();
    txn.put(db, b"key2", b"val4", WriteFlags::empty()).unwrap();
    txn.put(db, b"key2", b"val5", WriteFlags::empty()).unwrap();
    txn.put(db, b"key2", b"val6", WriteFlags::empty()).unwrap();

    let cursor = txn.open_ro_cursor(db).unwrap();
    assert_eq!((Some(&b"key1"[..]), &b"val1"[..]), cursor.get(None, None, MDB_FIRST).unwrap());
    assert_eq!((None, &b"val1val2val3"[..]), cursor.get(None, None, MDB_GET_MULTIPLE).unwrap());
    assert!(cursor.get(None, None, MDB_NEXT_MULTIPLE).is_err());
}

#[test]
fn test_iter() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.open_db(None).unwrap();

    let items: Vec<(&[u8], &[u8])> =
        vec![(b"key1", b"val1"), (b"key2", b"val2"), (b"key3", b"val3"), (b"key5", b"val5")];

    {
        let mut txn = env.begin_rw_txn().unwrap();
        for (key, data) in &items {
            txn.put(db, key, data, WriteFlags::empty()).unwrap();
        }
        txn.commit().unwrap();
    }

    let txn = env.begin_ro_txn().unwrap();
    let mut cursor = txn.open_ro_cursor(db).unwrap();

    // Because Result implements FromIterator, we can collect the iterator
    // of items of type Result<_, E> into a Result<Vec<_, E>> by specifying
    // the collection type via the turbofish syntax.
    assert_eq!(items, cursor.iter().collect::<Result<Vec<_>>>().unwrap());

    // Alternately, we can collect it into an appropriately typed variable.
    let retr: Result<Vec<_>> = cursor.iter_start().collect();
    assert_eq!(items, retr.unwrap());

    cursor.get(Some(b"key2"), None, MDB_SET).unwrap();
    assert_eq!(
        items.clone().into_iter().skip(2).collect::<Vec<_>>(),
        cursor.iter().collect::<Result<Vec<_>>>().unwrap()
    );

    assert_eq!(items, cursor.iter_start().collect::<Result<Vec<_>>>().unwrap());

    assert_eq!(
        items.clone().into_iter().skip(1).collect::<Vec<_>>(),
        cursor.iter_from(b"key2").collect::<Result<Vec<_>>>().unwrap()
    );

    assert_eq!(
        items.clone().into_iter().skip(3).collect::<Vec<_>>(),
        cursor.iter_from(b"key4").collect::<Result<Vec<_>>>().unwrap()
    );

    assert_eq!(
        vec!().into_iter().collect::<Vec<(&[u8], &[u8])>>(),
        cursor.iter_from(b"key6").collect::<Result<Vec<_>>>().unwrap()
    );
}

#[test]
fn test_iter_empty_database() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.open_db(None).unwrap();
    let txn = env.begin_ro_txn().unwrap();
    let mut cursor = txn.open_ro_cursor(db).unwrap();

    assert_eq!(0, cursor.iter().count());
    assert_eq!(0, cursor.iter_start().count());
    assert_eq!(0, cursor.iter_from(b"foo").count());
}

#[test]
fn test_iter_empty_dup_database() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.create_db(None, DatabaseFlags::DUP_SORT).unwrap();
    let txn = env.begin_ro_txn().unwrap();
    let mut cursor = txn.open_ro_cursor(db).unwrap();

    assert_eq!(0, cursor.iter().count());
    assert_eq!(0, cursor.iter_start().count());
    assert_eq!(0, cursor.iter_from(b"foo").count());
    assert_eq!(0, cursor.iter_dup().count());
    assert_eq!(0, cursor.iter_dup_start().count());
    assert_eq!(0, cursor.iter_dup_from(b"foo").count());
    assert_eq!(0, cursor.iter_dup_of(b"foo").count());
}

#[test]
fn test_iter_dup() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.create_db(None, DatabaseFlags::DUP_SORT).unwrap();

    let items: Vec<(&[u8], &[u8])> = vec![
        (b"a", b"1"),
        (b"a", b"2"),
        (b"a", b"3"),
        (b"b", b"1"),
        (b"b", b"2"),
        (b"b", b"3"),
        (b"c", b"1"),
        (b"c", b"2"),
        (b"c", b"3"),
        (b"e", b"1"),
        (b"e", b"2"),
        (b"e", b"3"),
    ];

    {
        let mut txn = env.begin_rw_txn().unwrap();
        for (key, data) in &items {
            txn.put(db, key, data, WriteFlags::empty()).unwrap();
        }
        txn.commit().unwrap();
    }

    let txn = env.begin_ro_txn().unwrap();
    let mut cursor = txn.open_ro_cursor(db).unwrap();
    assert_eq!(items, cursor.iter_dup().flatten().collect::<Result<Vec<_>>>().unwrap());

    cursor.get(Some(b"b"), None, MDB_SET).unwrap();
    assert_eq!(
        items.clone().into_iter().skip(4).collect::<Vec<(&[u8], &[u8])>>(),
        cursor.iter_dup().flatten().collect::<Result<Vec<_>>>().unwrap()
    );

    assert_eq!(items, cursor.iter_dup_start().flatten().collect::<Result<Vec<(&[u8], &[u8])>>>().unwrap());

    assert_eq!(
        items.clone().into_iter().skip(3).collect::<Vec<(&[u8], &[u8])>>(),
        cursor.iter_dup_from(b"b").flatten().collect::<Result<Vec<_>>>().unwrap()
    );

    assert_eq!(
        items.clone().into_iter().skip(3).collect::<Vec<(&[u8], &[u8])>>(),
        cursor.iter_dup_from(b"ab").flatten().collect::<Result<Vec<_>>>().unwrap()
    );

    assert_eq!(
        items.clone().into_iter().skip(9).collect::<Vec<(&[u8], &[u8])>>(),
        cursor.iter_dup_from(b"d").flatten().collect::<Result<Vec<_>>>().unwrap()
    );

    assert_eq!(
        vec!().into_iter().collect::<Vec<(&[u8], &[u8])>>(),
        cursor.iter_dup_from(b"f").flatten().collect::<Result<Vec<_>>>().unwrap()
    );

    assert_eq!(
        items.clone().into_iter().skip(3).take(3).collect::<Vec<(&[u8], &[u8])>>(),
        cursor.iter_dup_of(b"b").collect::<Result<Vec<_>>>().unwrap()
    );

    assert_eq!(0, cursor.iter_dup_of(b"foo").count());
}

#[test]
fn test_iter_del_get() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.create_db(None, DatabaseFlags::DUP_SORT).unwrap();

    let items: Vec<(&[u8], &[u8])> = vec![(b"a", b"1"), (b"b", b"2")];
    let r: Vec<(&[u8], &[u8])> = Vec::new();
    {
        let txn = env.begin_ro_txn().unwrap();
        let mut cursor = txn.open_ro_cursor(db).unwrap();
        assert_eq!(r, cursor.iter_dup_of(b"a").collect::<Result<Vec<_>>>().unwrap());
    }

    {
        let mut txn = env.begin_rw_txn().unwrap();
        for (key, data) in &items {
            txn.put(db, key, data, WriteFlags::empty()).unwrap();
        }
        txn.commit().unwrap();
    }

    let mut txn = env.begin_rw_txn().unwrap();
    let mut cursor = txn.open_rw_cursor(db).unwrap();
    assert_eq!(items, cursor.reader().iter_dup().flatten().collect::<Result<Vec<_>>>().unwrap());

    assert_eq!(
        items.clone().into_iter().take(1).collect::<Vec<(&[u8], &[u8])>>(),
        cursor.reader().iter_dup_of(b"a").collect::<Result<Vec<_>>>().unwrap()
    );

    assert_eq!((None, &b"1"[..]), cursor.reader().get(Some(b"a"), Some(b"1"), MDB_SET).unwrap());

    cursor.del(WriteFlags::empty()).unwrap();

    assert_eq!(r, cursor.reader().iter_dup_of(b"a").collect::<Result<Vec<_>>>().unwrap());
}

#[test]
fn test_put_del() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.open_db(None).unwrap();

    let mut txn = env.begin_rw_txn().unwrap();
    let mut cursor = txn.open_rw_cursor(db).unwrap();

    cursor.put(b"key1", b"val1", WriteFlags::empty()).unwrap();
    cursor.put(b"key2", b"val2", WriteFlags::empty()).unwrap();
    cursor.put(b"key3", b"val3", WriteFlags::empty()).unwrap();

    assert_eq!((Some(&b"key3"[..]), &b"val3"[..]), cursor.reader().get(None, None, MDB_GET_CURRENT).unwrap());

    cursor.del(WriteFlags::empty()).unwrap();
    assert_eq!((Some(&b"key2"[..]), &b"val2"[..]), cursor.reader().get(None, None, MDB_LAST).unwrap());
}

/// https://github.com/danburkert/lmdb-rs/issues/63: `iter_dup_of` on a missing
/// key must be empty, even when the next key holds a single value.
#[test]
fn issue_63_iter_dup_of_missing_key_is_empty() {
    let dir = common::temp_dir();
    let env = Environment::new().open(dir.path()).unwrap();
    let db = env.create_db(None, DatabaseFlags::DUP_SORT).unwrap();
    let mut txn = env.begin_rw_txn().unwrap();
    txn.put(db, b"B", b"only", WriteFlags::empty()).unwrap();
    txn.commit().unwrap();

    let txn = env.begin_ro_txn().unwrap();
    let mut cursor = txn.open_ro_cursor(db).unwrap();
    let found: Vec<_> = cursor.iter_dup_of(b"A").collect::<Result<Vec<_>>>().unwrap();

    assert!(found.is_empty(), "got {found:?}");
}
