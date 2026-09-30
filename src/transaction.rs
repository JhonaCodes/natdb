use std::marker::PhantomData;
use std::{fmt, mem, ptr, result, slice};

use libc::{c_uint, c_void, size_t};

use crate::cursor::{RoCursor, RwCursor};
use crate::database::Database;
use crate::environment::{Environment, Stat};
use crate::error::{Error, Result, lmdb_result};
use crate::ffi;
use crate::flags::{DatabaseFlags, EnvironmentFlags, WriteFlags};

/// An LMDB transaction.
///
/// All database operations require a transaction.
pub trait Transaction: Sized {
    /// Returns a raw pointer to the underlying LMDB transaction.
    ///
    /// The caller **must** ensure that the pointer is not used after the
    /// lifetime of the transaction.
    fn txn(&self) -> *mut ffi::MDB_txn;

    /// Commits the transaction.
    ///
    /// Any pending operations will be saved.
    fn commit(self) -> Result<()> {
        // SAFETY: `self.txn()` is live; `mdb_txn_commit` frees the handle whether it
        // succeeds or fails, so `self` is forgotten to keep `Drop` from aborting it again.
        let result = lmdb_result(unsafe { ffi::mdb_txn_commit(self.txn()) });
        mem::forget(self);
        result
    }

    /// Aborts the transaction.
    ///
    /// Any pending operations will not be saved.
    fn abort(self) {
        // Abort should be performed in transaction destructors.
    }

    /// Opens a database in the transaction.
    ///
    /// If `name` is `None`, then the default database will be opened, otherwise
    /// a named database will be opened. The database handle will be private to
    /// the transaction until the transaction is successfully committed. If the
    /// transaction is aborted the returned database handle should no longer be
    /// used.
    ///
    /// Prefer using `Environment::open_db`.
    ///
    /// ## Safety
    ///
    /// This function (as well as `Environment::open_db`,
    /// `Environment::create_db`, and `Database::create`) **must not** be called
    /// from multiple concurrent transactions in the same environment. A
    /// transaction which uses this function must finish (either commit or
    /// abort) before any other transaction may use this function.
    unsafe fn open_db(&self, name: Option<&str>) -> Result<Database> {
        // SAFETY: `self.txn()` is live; the caller guarantees open/create serialization.
        unsafe { Database::new(self.txn(), name, 0) }
    }

    /// Gets an item from a database.
    ///
    /// This function retrieves the data associated with the given key in the
    /// database. If the database supports duplicate keys
    /// (`DatabaseFlags::DUP_SORT`) then the first data item for the key will be
    /// returned. Retrieval of other items requires the use of
    /// `Transaction::cursor_get`. If the item is not in the database, then
    /// `Error::NotFound` will be returned.
    fn get<'txn, K>(&'txn self, database: Database, key: &K) -> Result<&'txn [u8]>
    where
        K: AsRef<[u8]>,
    {
        let key = key.as_ref();
        let mut key_val: ffi::MDB_val = ffi::MDB_val {
            mv_size: key.len() as size_t,
            mv_data: key.as_ptr() as *mut c_void,
        };
        let mut data_val: ffi::MDB_val = ffi::MDB_val {
            mv_size: 0,
            mv_data: ptr::null_mut(),
        };
        // SAFETY: `key_val` borrows `key`, which outlives the call, and `data_val` is a
        // valid out-parameter.
        match unsafe { ffi::mdb_get(self.txn(), database.dbi(), &mut key_val, &mut data_val) } {
            // SAFETY: on success LMDB points `data_val` at `mv_size` bytes inside the memory
            // map, which stay valid and unmodified until this transaction ends ('txn).
            ffi::MDB_SUCCESS => Ok(unsafe { slice::from_raw_parts(data_val.mv_data as *const u8, data_val.mv_size) }),
            err_code => Err(Error::from_err_code(err_code)),
        }
    }

    /// Open a new read-only cursor on the given database.
    fn open_ro_cursor(&self, db: Database) -> Result<RoCursor<'_>> {
        RoCursor::new(self, db)
    }

    /// Gets the option flags for the given database in the transaction.
    fn db_flags(&self, db: Database) -> Result<DatabaseFlags> {
        let mut flags: c_uint = 0;
        // SAFETY: `self.txn()` is live and `flags` is a valid out-pointer.
        lmdb_result(unsafe { ffi::mdb_dbi_flags(self.txn(), db.dbi(), &mut flags) })?;
        Ok(DatabaseFlags::from_bits_truncate(flags))
    }

    /// Retrieves database statistics.
    fn stat(&self, db: Database) -> Result<Stat> {
        let mut stat = Stat::new();
        // SAFETY: `self.txn()` is live and `stat.mdb_stat()` points to a live `MDB_stat`.
        lmdb_try!(unsafe { ffi::mdb_stat(self.txn(), db.dbi(), stat.mdb_stat()) });
        Ok(stat)
    }
}

/// An LMDB read-only transaction.
pub struct RoTransaction<'env> {
    txn: *mut ffi::MDB_txn,
    _marker: PhantomData<&'env ()>,
}

impl<'env> fmt::Debug for RoTransaction<'env> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> result::Result<(), fmt::Error> {
        f.debug_struct("RoTransaction").finish()
    }
}

impl Drop for RoTransaction<'_> {
    fn drop(&mut self) {
        // SAFETY: `self.txn` is live: every path that ends it (`commit`, `reset`)
        // forgets `self` first, so this is the only abort.
        unsafe { ffi::mdb_txn_abort(self.txn) }
    }
}

impl<'env> RoTransaction<'env> {
    /// Creates a new read-only transaction in the given environment. Prefer
    /// using `Environment::begin_ro_txn`.
    pub(crate) fn new(env: &'env Environment) -> Result<RoTransaction<'env>> {
        let mut txn: *mut ffi::MDB_txn = ptr::null_mut();
        // SAFETY: `env` is open for 'env and `txn` is a valid out-pointer.
        lmdb_result(unsafe { ffi::mdb_txn_begin(env.env(), ptr::null_mut(), ffi::MDB_RDONLY, &mut txn) })?;
        Ok(RoTransaction {
            txn,
            _marker: PhantomData,
        })
    }

    /// Resets the read-only transaction.
    ///
    /// Abort the transaction like `Transaction::abort`, but keep the
    /// transaction handle.  `InactiveTransaction::renew` may reuse the handle.
    /// This saves allocation overhead if the process will start a new read-only
    /// transaction soon, and also locking overhead if
    /// `EnvironmentFlags::NO_TLS` is in use. The reader table lock is released,
    /// but the table slot stays tied to its thread or transaction. Reader locks
    /// generally don't interfere with writers, but they keep old versions of
    /// database pages allocated. Thus they prevent the old pages from being
    /// reused when writers commit new data, and so under heavy load the
    /// database size may grow much more rapidly than otherwise.
    pub fn reset(self) -> InactiveTransaction<'env> {
        let txn = self.txn;
        mem::forget(self);
        // SAFETY: `txn` is a live read-only transaction whose ownership moves into the
        // returned `InactiveTransaction` (`self` was forgotten, so it is not aborted).
        unsafe { ffi::mdb_txn_reset(txn) };
        InactiveTransaction {
            txn,
            _marker: PhantomData,
        }
    }
}

impl<'env> Transaction for RoTransaction<'env> {
    fn txn(&self) -> *mut ffi::MDB_txn {
        self.txn
    }
}

/// An inactive read-only transaction.
pub struct InactiveTransaction<'env> {
    txn: *mut ffi::MDB_txn,
    _marker: PhantomData<&'env ()>,
}

impl<'env> fmt::Debug for InactiveTransaction<'env> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> result::Result<(), fmt::Error> {
        f.debug_struct("InactiveTransaction").finish()
    }
}

impl Drop for InactiveTransaction<'_> {
    fn drop(&mut self) {
        // SAFETY: `self.txn` is a reset (not freed) handle; `renew` forgets `self`
        // before handing it over, so this is the only abort.
        unsafe { ffi::mdb_txn_abort(self.txn) }
    }
}

impl<'env> InactiveTransaction<'env> {
    /// Renews the inactive transaction, returning an active read-only
    /// transaction.
    ///
    /// This acquires a new reader lock for a transaction handle that had been
    /// released by `RoTransaction::reset`.
    pub fn renew(self) -> Result<RoTransaction<'env>> {
        let txn = self.txn;
        mem::forget(self);
        // SAFETY: `txn` is a reset read-only handle; ownership moves to the returned
        // `RoTransaction` (`self` was forgotten, so it is not aborted twice).
        lmdb_result(unsafe { ffi::mdb_txn_renew(txn) })?;
        Ok(RoTransaction {
            txn,
            _marker: PhantomData,
        })
    }
}

/// An LMDB read-write transaction.
pub struct RwTransaction<'env> {
    txn: *mut ffi::MDB_txn,
    _marker: PhantomData<&'env ()>,
}

impl<'env> fmt::Debug for RwTransaction<'env> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> result::Result<(), fmt::Error> {
        f.debug_struct("RwTransaction").finish()
    }
}

impl Drop for RwTransaction<'_> {
    fn drop(&mut self) {
        // SAFETY: `self.txn` is live: `commit` forgets `self` first, so this is the
        // only abort.
        unsafe { ffi::mdb_txn_abort(self.txn) }
    }
}

impl<'env> RwTransaction<'env> {
    /// Creates a new read-write transaction in the given environment. Prefer
    /// using `Environment::begin_ro_txn`.
    pub(crate) fn new(env: &'env Environment) -> Result<RwTransaction<'env>> {
        let mut txn: *mut ffi::MDB_txn = ptr::null_mut();
        // SAFETY: `env` is open for 'env and `txn` is a valid out-pointer.
        lmdb_result(unsafe {
            ffi::mdb_txn_begin(env.env(), ptr::null_mut(), EnvironmentFlags::empty().bits(), &mut txn)
        })?;
        Ok(RwTransaction {
            txn,
            _marker: PhantomData,
        })
    }

    /// Opens a database in the provided transaction, creating it if necessary.
    ///
    /// If `name` is `None`, then the default database will be opened, otherwise
    /// a named database will be opened. The database handle will be private to
    /// the transaction until the transaction is successfully committed. If the
    /// transaction is aborted the returned database handle should no longer be
    /// used.
    ///
    /// Prefer using `Environment::create_db`.
    ///
    /// ## Safety
    ///
    /// This function (as well as `Environment::open_db`,
    /// `Environment::create_db`, and `Database::open`) **must not** be called
    /// from multiple concurrent transactions in the same environment. A
    /// transaction which uses this function must finish (either commit or
    /// abort) before any other transaction may use this function.
    pub unsafe fn create_db(&self, name: Option<&str>, flags: DatabaseFlags) -> Result<Database> {
        // SAFETY: `self.txn()` is live; the caller guarantees open/create serialization.
        unsafe { Database::new(self.txn(), name, flags.bits() | ffi::MDB_CREATE) }
    }

    /// Opens a new read-write cursor on the given database and transaction.
    pub fn open_rw_cursor(&mut self, db: Database) -> Result<RwCursor<'_>> {
        RwCursor::new(self, db)
    }

    /// Stores an item into a database.
    ///
    /// This function stores key/data pairs in the database. The default
    /// behavior is to enter the new key/data pair, replacing any previously
    /// existing key if duplicates are disallowed, or adding a duplicate data
    /// item if duplicates are allowed (`DatabaseFlags::DUP_SORT`).
    pub fn put<K, D>(&mut self, database: Database, key: &K, data: &D, flags: WriteFlags) -> Result<()>
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
        lmdb_result(unsafe { ffi::mdb_put(self.txn(), database.dbi(), &mut key_val, &mut data_val, flags.bits()) })
    }

    /// Returns a buffer which can be used to write a value into the item at the
    /// given key and with the given length. The buffer must be completely
    /// filled by the caller.
    pub fn reserve<'txn, K>(
        &'txn mut self,
        database: Database,
        key: &K,
        len: size_t,
        flags: WriteFlags,
    ) -> Result<&'txn mut [u8]>
    where
        K: AsRef<[u8]>,
    {
        let key = key.as_ref();
        let mut key_val: ffi::MDB_val = ffi::MDB_val {
            mv_size: key.len() as size_t,
            mv_data: key.as_ptr() as *mut c_void,
        };
        let mut data_val: ffi::MDB_val = ffi::MDB_val {
            mv_size: len,
            mv_data: ptr::null_mut::<c_void>(),
        };
        // SAFETY: `key_val` borrows `key`, which outlives the call; with `MDB_RESERVE`
        // LMDB fills `data_val` with the address of the reserved space.
        lmdb_result(unsafe {
            ffi::mdb_put(self.txn(), database.dbi(), &mut key_val, &mut data_val, flags.bits() | ffi::MDB_RESERVE)
        })?;
        // SAFETY: LMDB reserved `mv_size` writable bytes at `mv_data`; they stay valid
        // while this transaction is mutably borrowed ('txn).
        Ok(unsafe { slice::from_raw_parts_mut(data_val.mv_data as *mut u8, data_val.mv_size) })
    }

    /// Deletes an item from a database.
    ///
    /// This function removes key/data pairs from the database. If the database
    /// does not support sorted duplicate data items (`DatabaseFlags::DUP_SORT`)
    /// the data parameter is ignored.  If the database supports sorted
    /// duplicates and the data parameter is `None`, all of the duplicate data
    /// items for the key will be deleted. Otherwise, if the data parameter is
    /// `Some` only the matching data item will be deleted. This function will
    /// return `Error::NotFound` if the specified key/data pair is not in the
    /// database.
    pub fn del<K>(&mut self, database: Database, key: &K, data: Option<&[u8]>) -> Result<()>
    where
        K: AsRef<[u8]>,
    {
        let key = key.as_ref();
        let mut key_val: ffi::MDB_val = ffi::MDB_val {
            mv_size: key.len() as size_t,
            mv_data: key.as_ptr() as *mut c_void,
        };
        let data_val: Option<ffi::MDB_val> = data.map(|data| ffi::MDB_val {
            mv_size: data.len() as size_t,
            mv_data: data.as_ptr() as *mut c_void,
        });

        if let Some(mut d) = data_val {
            // SAFETY: `key_val` and `d` borrow `key` and `data`, which outlive the call.
            lmdb_result(unsafe { ffi::mdb_del(self.txn(), database.dbi(), &mut key_val, &mut d) })
        } else {
            // SAFETY: `key_val` borrows `key`, which outlives the call; a null data
            // pointer asks LMDB to delete every item of the key.
            lmdb_result(unsafe { ffi::mdb_del(self.txn(), database.dbi(), &mut key_val, ptr::null_mut()) })
        }
    }

    /// Empties the given database. All items will be removed.
    pub fn clear_db(&mut self, db: Database) -> Result<()> {
        // SAFETY: `self.txn()` is a live write transaction; `0` empties without deleting.
        lmdb_result(unsafe { ffi::mdb_drop(self.txn(), db.dbi(), 0) })
    }

    /// Drops the database from the environment.
    ///
    /// ## Safety
    ///
    /// This method is unsafe in the same ways as `Environment::close_db`, and
    /// should be used accordingly.
    pub unsafe fn drop_db(&mut self, db: Database) -> Result<()> {
        // SAFETY: the caller upholds the `close_db` contract; `self.txn` is live.
        lmdb_result(unsafe { ffi::mdb_drop(self.txn, db.dbi(), 1) })
    }

    /// Begins a new nested transaction inside of this transaction.
    pub fn begin_nested_txn(&mut self) -> Result<RwTransaction<'_>> {
        let mut nested: *mut ffi::MDB_txn = ptr::null_mut();
        // SAFETY: `self.txn()` is a live write transaction, so its environment is open;
        // the nested handle mutably borrows `self`, so the parent cannot end first.
        unsafe {
            let env: *mut ffi::MDB_env = ffi::mdb_txn_env(self.txn());
            lmdb_result(ffi::mdb_txn_begin(env, self.txn(), 0, &mut nested))?;
        }
        Ok(RwTransaction {
            txn: nested,
            _marker: PhantomData,
        })
    }
}

impl<'env> Transaction for RwTransaction<'env> {
    fn txn(&self) -> *mut ffi::MDB_txn {
        self.txn
    }
}
