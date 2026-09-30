use std::marker::PhantomData;
use std::{fmt, mem, ptr, result, slice};

use libc::{EINVAL, c_uint, c_void, size_t};

use crate::database::Database;
use crate::error::{Error, Result, lmdb_result};
use crate::ffi;
use crate::flags::WriteFlags;
use crate::transaction::Transaction;

/// An LMDB cursor.
pub trait Cursor<'txn> {
    /// Returns a raw pointer to the underlying LMDB cursor.
    ///
    /// The caller **must** ensure that the pointer is not used after the
    /// lifetime of the cursor.
    fn cursor(&self) -> *mut ffi::MDB_cursor;

    /// Retrieves a key/data pair from the cursor. Depending on the cursor op,
    /// the current key may be returned.
    fn get(&self, key: Option<&[u8]>, data: Option<&[u8]>, op: c_uint) -> Result<(Option<&'txn [u8]>, &'txn [u8])> {
        let mut key_val = slice_to_val(key);
        let mut data_val = slice_to_val(data);
        let key_ptr = key_val.mv_data;
        // SAFETY: `self.cursor()` is live; `key_val`/`data_val` are null or borrow `key`/`data`,
        // which outlive the call, and LMDB only writes through them as out-parameters.
        lmdb_result(unsafe { ffi::mdb_cursor_get(self.cursor(), &mut key_val, &mut data_val, op) })?;
        let key_out = if key_ptr != key_val.mv_data {
            // SAFETY: LMDB pointed `key_val` at a key inside the map, valid for 'txn.
            Some(unsafe { val_to_slice(key_val) })
        } else {
            None
        };
        // SAFETY: on success `data_val` points at data inside the map, valid for 'txn.
        let data_out = unsafe { val_to_slice(data_val) };
        Ok((key_out, data_out))
    }

    /// Iterate over database items. The iterator will begin with item next
    /// after the cursor, and continue until the end of the database. For new
    /// cursors, the iterator will begin with the first item in the database.
    ///
    /// For databases with duplicate data items (`DatabaseFlags::DUP_SORT`), the
    /// duplicate data items of each key will be returned before moving on to
    /// the next key.
    fn iter(&mut self) -> Iter<'txn> {
        Iter::new(self.cursor(), ffi::MDB_NEXT, ffi::MDB_NEXT)
    }

    /// Iterate over database items starting from the beginning of the database.
    ///
    /// For databases with duplicate data items (`DatabaseFlags::DUP_SORT`), the
    /// duplicate data items of each key will be returned before moving on to
    /// the next key.
    fn iter_start(&mut self) -> Iter<'txn> {
        Iter::new(self.cursor(), ffi::MDB_FIRST, ffi::MDB_NEXT)
    }

    /// Iterate over database items starting from the given key.
    ///
    /// For databases with duplicate data items (`DatabaseFlags::DUP_SORT`), the
    /// duplicate data items of each key will be returned before moving on to
    /// the next key.
    fn iter_from<K>(&mut self, key: K) -> Iter<'txn>
    where
        K: AsRef<[u8]>,
    {
        match self.get(Some(key.as_ref()), None, ffi::MDB_SET_RANGE) {
            Ok(_) | Err(Error::NotFound) => (),
            Err(error) => return Iter::Err(error),
        };
        Iter::new(self.cursor(), ffi::MDB_GET_CURRENT, ffi::MDB_NEXT)
    }

    /// Iterate over duplicate database items. The iterator will begin with the
    /// item next after the cursor, and continue until the end of the database.
    /// Each item will be returned as an iterator of its duplicates.
    fn iter_dup(&mut self) -> IterDup<'txn> {
        IterDup::new(self.cursor(), ffi::MDB_NEXT)
    }

    /// Iterate over duplicate database items starting from the beginning of the
    /// database. Each item will be returned as an iterator of its duplicates.
    fn iter_dup_start(&mut self) -> IterDup<'txn> {
        IterDup::new(self.cursor(), ffi::MDB_FIRST)
    }

    /// Iterate over duplicate items in the database starting from the given
    /// key. Each item will be returned as an iterator of its duplicates.
    fn iter_dup_from<K>(&mut self, key: K) -> IterDup<'txn>
    where
        K: AsRef<[u8]>,
    {
        match self.get(Some(key.as_ref()), None, ffi::MDB_SET_RANGE) {
            Ok(_) | Err(Error::NotFound) => (),
            Err(error) => return IterDup::Err(error),
        };
        IterDup::new(self.cursor(), ffi::MDB_GET_CURRENT)
    }

    /// Iterate over the duplicates of the item in the database with the given key.
    fn iter_dup_of<K>(&mut self, key: K) -> Iter<'txn>
    where
        K: AsRef<[u8]>,
    {
        match self.get(Some(key.as_ref()), None, ffi::MDB_SET) {
            Ok(_) => (),
            Err(Error::NotFound) => {
                self.get(None, None, ffi::MDB_LAST).ok();
                return Iter::new(self.cursor(), ffi::MDB_NEXT, ffi::MDB_NEXT);
            },
            Err(error) => return Iter::Err(error),
        };
        Iter::new(self.cursor(), ffi::MDB_GET_CURRENT, ffi::MDB_NEXT_DUP)
    }
}

/// A read-only cursor for navigating the items within a database.
pub struct RoCursor<'txn> {
    cursor: *mut ffi::MDB_cursor,
    _marker: PhantomData<fn() -> &'txn ()>,
}

impl<'txn> Cursor<'txn> for RoCursor<'txn> {
    fn cursor(&self) -> *mut ffi::MDB_cursor {
        self.cursor
    }
}

impl<'txn> fmt::Debug for RoCursor<'txn> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> result::Result<(), fmt::Error> {
        f.debug_struct("RoCursor").finish()
    }
}

impl Drop for RoCursor<'_> {
    fn drop(&mut self) {
        // SAFETY: `self.cursor` was opened in `RoCursor::new` and is closed only here.
        unsafe { ffi::mdb_cursor_close(self.cursor) }
    }
}

impl<'txn> RoCursor<'txn> {
    /// Creates a new read-only cursor in the given database and transaction.
    /// Prefer using `Transaction::open_cursor`.
    pub(crate) fn new<T>(txn: &'txn T, db: Database) -> Result<RoCursor<'txn>>
    where
        T: Transaction,
    {
        let mut cursor: *mut ffi::MDB_cursor = ptr::null_mut();
        // SAFETY: `txn` is live for 'txn and `cursor` is a valid out-pointer.
        lmdb_result(unsafe { ffi::mdb_cursor_open(txn.txn(), db.dbi(), &mut cursor) })?;
        Ok(RoCursor {
            cursor,
            _marker: PhantomData,
        })
    }
}

/// A read-write cursor for navigating items within a database.
pub struct RwCursor<'txn> {
    cursor: *mut ffi::MDB_cursor,
    _marker: PhantomData<fn() -> &'txn ()>,
}

/// Read access to the position of a [`RwCursor`], obtained with [`RwCursor::reader`].
///
/// Data read through a write transaction points into pages that later writes of
/// the same transaction may modify. Every slice read through this type
/// therefore borrows the [`RwCursor`] mutably: the cursor cannot write while
/// any of those slices is alive.
///
/// ```compile_fail
/// # use natdb::{Cursor, Environment, WriteFlags};
/// # let dir = std::env::temp_dir().join("natdb_rw_cursor_reader_doc");
/// # std::fs::create_dir_all(&dir).unwrap();
/// # let env = Environment::new().open(&dir).unwrap();
/// # let db = env.open_db(None).unwrap();
/// let mut txn = env.begin_rw_txn().unwrap();
/// let mut cursor = txn.open_rw_cursor(db).unwrap();
/// cursor.put(b"key", b"value", WriteFlags::empty()).unwrap();
/// let (_, value) = cursor.reader().get(None, None, natdb_sys::MDB_GET_CURRENT).unwrap();
/// cursor.put(b"key", b"other", WriteFlags::empty()).unwrap(); // error: `cursor` is still borrowed
/// assert_eq!(value, b"value");
/// ```
pub struct RwCursorReader<'cursor> {
    cursor: *mut ffi::MDB_cursor,
    _marker: PhantomData<&'cursor mut ()>,
}

impl<'cursor> Cursor<'cursor> for RwCursorReader<'cursor> {
    fn cursor(&self) -> *mut ffi::MDB_cursor {
        self.cursor
    }
}

impl fmt::Debug for RwCursorReader<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> result::Result<(), fmt::Error> {
        f.debug_struct("RwCursorReader").finish()
    }
}

impl<'txn> fmt::Debug for RwCursor<'txn> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> result::Result<(), fmt::Error> {
        f.debug_struct("RwCursor").finish()
    }
}

impl Drop for RwCursor<'_> {
    fn drop(&mut self) {
        // SAFETY: `self.cursor` was opened in `RwCursor::new` and is closed only here.
        unsafe { ffi::mdb_cursor_close(self.cursor) }
    }
}

impl<'txn> RwCursor<'txn> {
    /// Creates a new read-only cursor in the given database and transaction.
    /// Prefer using `RwTransaction::open_rw_cursor`.
    pub(crate) fn new<T>(txn: &'txn T, db: Database) -> Result<RwCursor<'txn>>
    where
        T: Transaction,
    {
        let mut cursor: *mut ffi::MDB_cursor = ptr::null_mut();
        // SAFETY: `txn` is live for 'txn and `cursor` is a valid out-pointer.
        lmdb_result(unsafe { ffi::mdb_cursor_open(txn.txn(), db.dbi(), &mut cursor) })?;
        Ok(RwCursor {
            cursor,
            _marker: PhantomData,
        })
    }

    /// Returns read access to this cursor. The slices it yields keep this
    /// cursor mutably borrowed, so no write can modify them while they are
    /// alive (see [`RwCursorReader`]).
    pub fn reader(&mut self) -> RwCursorReader<'_> {
        RwCursorReader {
            cursor: self.cursor,
            _marker: PhantomData,
        }
    }

    /// Puts a key/data pair into the database. The cursor will be positioned at
    /// the new data item, or on failure usually near it.
    pub fn put<K, D>(&mut self, key: &K, data: &D, flags: WriteFlags) -> Result<()>
    where
        K: AsRef<[u8]>,
        D: AsRef<[u8]>,
    {
        let key = key.as_ref();
        let data = data.as_ref();
        let mut key_val: ffi::MDB_val = ffi::MDB_val {
            mv_size: key.len() as size_t,
            mv_data: key.as_ptr() as *mut c_void,
        };
        let mut data_val: ffi::MDB_val = ffi::MDB_val {
            mv_size: data.len() as size_t,
            mv_data: data.as_ptr() as *mut c_void,
        };
        // SAFETY: `key_val` and `data_val` borrow `key` and `data`, which outlive the call.
        lmdb_result(unsafe { ffi::mdb_cursor_put(self.cursor, &mut key_val, &mut data_val, flags.bits()) })
    }

    /// Deletes the current key/data pair.
    ///
    /// ### Flags
    ///
    /// `WriteFlags::NO_DUP_DATA` may be used to delete all data items for the
    /// current key, if the database was opened with `DatabaseFlags::DUP_SORT`.
    pub fn del(&mut self, flags: WriteFlags) -> Result<()> {
        // SAFETY: `self.cursor()` is a live cursor of a write transaction.
        lmdb_result(unsafe { ffi::mdb_cursor_del(self.cursor, flags.bits()) })
    }
}

fn slice_to_val(slice: Option<&[u8]>) -> ffi::MDB_val {
    match slice {
        Some(slice) => ffi::MDB_val {
            mv_size: slice.len() as size_t,
            mv_data: slice.as_ptr() as *mut c_void,
        },
        None => ffi::MDB_val {
            mv_size: 0,
            mv_data: ptr::null_mut(),
        },
    }
}

/// # Safety
///
/// `val` must describe `mv_size` initialized bytes at `mv_data` that stay valid and
/// unmodified for `'a`.
unsafe fn val_to_slice<'a>(val: ffi::MDB_val) -> &'a [u8] {
    // SAFETY: guaranteed by the caller.
    unsafe { slice::from_raw_parts(val.mv_data as *const u8, val.mv_size) }
}

/// An iterator over the key/value pairs in an LMDB database.
pub enum Iter<'txn> {
    /// An iterator that returns an error on every call to Iter.next().
    /// Cursor.iter*() creates an Iter of this type when LMDB returns an error
    /// on retrieval of a cursor.  Using this variant instead of returning
    /// an error makes Cursor.iter()* methods infallible, so consumers only
    /// need to check the result of Iter.next().
    Err(Error),

    /// An iterator that returns an Item on calls to Iter.next().
    /// The Item is a Result<(&'txn [u8], &'txn [u8])>, so this variant
    /// might still return an error, if retrieval of the key/value pair
    /// fails for some reason.
    Ok {
        /// The LMDB cursor with which to iterate.
        cursor: *mut ffi::MDB_cursor,

        /// The first operation to perform when the consumer calls Iter.next().
        op: c_uint,

        /// The next and subsequent operations to perform.
        next_op: c_uint,

        /// A marker to ensure the iterator doesn't outlive the transaction.
        _marker: PhantomData<fn(&'txn ())>,
    },
}

impl<'txn> Iter<'txn> {
    /// Creates a new iterator backed by the given cursor.
    fn new<'t>(cursor: *mut ffi::MDB_cursor, op: c_uint, next_op: c_uint) -> Iter<'t> {
        Iter::Ok {
            cursor,
            op,
            next_op,
            _marker: PhantomData,
        }
    }
}

impl<'txn> fmt::Debug for Iter<'txn> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> result::Result<(), fmt::Error> {
        f.debug_struct("Iter").finish()
    }
}

impl<'txn> Iterator for Iter<'txn> {
    type Item = Result<(&'txn [u8], &'txn [u8])>;

    fn next(&mut self) -> Option<Result<(&'txn [u8], &'txn [u8])>> {
        match self {
            Iter::Ok {
                cursor,
                op,
                next_op,
                ..
            } => {
                let mut key = ffi::MDB_val {
                    mv_size: 0,
                    mv_data: ptr::null_mut(),
                };
                let mut data = ffi::MDB_val {
                    mv_size: 0,
                    mv_data: ptr::null_mut(),
                };
                let op = mem::replace(op, *next_op);
                // SAFETY: `cursor` stays open while the iterator borrows the transaction
                // ('txn); `key` and `data` are valid out-parameters.
                match unsafe { ffi::mdb_cursor_get(*cursor, &mut key, &mut data, op) } {
                    // SAFETY: on success both values point inside the map, valid for 'txn.
                    ffi::MDB_SUCCESS => Some(Ok(unsafe { (val_to_slice(key), val_to_slice(data)) })),
                    // EINVAL can occur when the cursor was previously seeked to a non-existent value,
                    // e.g. iter_from with a key greater than all values in the database.
                    ffi::MDB_NOTFOUND | EINVAL => None,
                    error => Some(Err(Error::from_err_code(error))),
                }
            },
            Iter::Err(err) => Some(Err(*err)),
        }
    }
}

/// An iterator over the keys and duplicate values in an LMDB database.
///
/// The yielded items of the iterator are themselves iterators over the duplicate values for a
/// specific key.
pub enum IterDup<'txn> {
    /// An iterator that returns an error on every call to Iter.next().
    /// Cursor.iter*() creates an Iter of this type when LMDB returns an error
    /// on retrieval of a cursor.  Using this variant instead of returning
    /// an error makes Cursor.iter()* methods infallible, so consumers only
    /// need to check the result of Iter.next().
    Err(Error),

    /// An iterator that returns an Item on calls to Iter.next().
    /// The Item is a Result<(&'txn [u8], &'txn [u8])>, so this variant
    /// might still return an error, if retrieval of the key/value pair
    /// fails for some reason.
    Ok {
        /// The LMDB cursor with which to iterate.
        cursor: *mut ffi::MDB_cursor,

        /// The first operation to perform when the consumer calls Iter.next().
        op: c_uint,

        /// A marker to ensure the iterator doesn't outlive the transaction.
        _marker: PhantomData<fn(&'txn ())>,
    },
}

impl<'txn> IterDup<'txn> {
    /// Creates a new iterator backed by the given cursor.
    fn new<'t>(cursor: *mut ffi::MDB_cursor, op: c_uint) -> IterDup<'t> {
        IterDup::Ok {
            cursor,
            op,
            _marker: PhantomData,
        }
    }
}

impl<'txn> fmt::Debug for IterDup<'txn> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> result::Result<(), fmt::Error> {
        f.debug_struct("IterDup").finish()
    }
}

impl<'txn> Iterator for IterDup<'txn> {
    type Item = Iter<'txn>;

    fn next(&mut self) -> Option<Iter<'txn>> {
        match self {
            IterDup::Ok {
                cursor,
                op,
                ..
            } => {
                let mut key = ffi::MDB_val {
                    mv_size: 0,
                    mv_data: ptr::null_mut(),
                };
                let mut data = ffi::MDB_val {
                    mv_size: 0,
                    mv_data: ptr::null_mut(),
                };
                let op = mem::replace(op, ffi::MDB_NEXT_NODUP);
                // SAFETY: `cursor` stays open while the iterator borrows the transaction
                // ('txn); `key` and `data` are valid out-parameters.
                let err_code = unsafe { ffi::mdb_cursor_get(*cursor, &mut key, &mut data, op) };

                if err_code == ffi::MDB_SUCCESS {
                    Some(Iter::new(*cursor, ffi::MDB_GET_CURRENT, ffi::MDB_NEXT_DUP))
                } else {
                    None
                }
            },
            IterDup::Err(err) => Some(Iter::Err(*err)),
        }
    }
}
