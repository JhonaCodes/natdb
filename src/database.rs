use std::ffi::CString;
use std::ptr;

use libc::c_uint;

use crate::error::{Error, Result, lmdb_result};
use crate::ffi;

/// A handle to an individual database in an environment.
///
/// A database handle denotes the name and parameters of a database in an environment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Database {
    dbi: ffi::MDB_dbi,
}

impl Database {
    /// Opens a new database handle in the given transaction.
    ///
    /// Prefer using `Environment::open_db`, `Environment::create_db`, `TransactionExt::open_db`,
    /// or `RwTransaction::create_db`.
    ///
    /// # Safety
    ///
    /// `txn` must be a live transaction handle, and no other transaction may open or
    /// create databases in the same environment concurrently.
    pub(crate) unsafe fn new(txn: *mut ffi::MDB_txn, name: Option<&str>, flags: c_uint) -> Result<Database> {
        let c_name = name.map(CString::new).transpose().map_err(|_| Error::Invalid)?;
        let name_ptr = if let Some(ref c_name) = c_name {
            c_name.as_ptr()
        } else {
            ptr::null()
        };
        let mut dbi: ffi::MDB_dbi = 0;
        // SAFETY: the caller guarantees `txn` is live and that database opening is
        // serialized; `name_ptr` is null or points into `c_name`, which outlives the call.
        lmdb_result(unsafe { ffi::mdb_dbi_open(txn, name_ptr, flags, &mut dbi) })?;
        Ok(Database {
            dbi,
        })
    }

    pub(crate) fn freelist_db() -> Database {
        Database {
            dbi: 0,
        }
    }

    /// Returns the underlying LMDB database handle.
    ///
    /// The caller **must** ensure that the handle is not used after the lifetime of the
    /// environment, or after the database has been closed.
    pub fn dbi(&self) -> ffi::MDB_dbi {
        self.dbi
    }
}

// SAFETY: a `Database` is only a `MDB_dbi` integer; LMDB allows DBI handles to be
// shared by any transaction of the environment, from any thread.
unsafe impl Sync for Database {}
// SAFETY: see the `Sync` impl above.
unsafe impl Send for Database {}
