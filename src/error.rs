use std::ffi::CStr;
use std::{fmt, result};

use libc::c_int;

use crate::ffi;

/// An LMDB error kind.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum Error {
    /// key/data pair already exists.
    KeyExist,
    /// key/data pair not found (EOF).
    NotFound,
    /// Requested page not found - this usually indicates corruption.
    PageNotFound,
    /// Located page was wrong type.
    Corrupted,
    /// Update of meta page failed or environment had fatal error.
    Panic,
    /// Environment version mismatch.
    VersionMismatch,
    /// File is not a valid LMDB file.
    Invalid,
    /// Environment mapsize reached.
    MapFull,
    /// Environment maxdbs reached.
    DbsFull,
    /// Environment maxreaders reached.
    ReadersFull,
    /// Too many TLS keys in use - Windows only.
    TlsFull,
    /// Txn has too many dirty pages.
    TxnFull,
    /// Cursor stack too deep - internal error.
    CursorFull,
    /// Page has not enough space - internal error.
    PageFull,
    /// Database contents grew beyond environment mapsize.
    MapResized,
    /// MDB_Incompatible: Operation and DB incompatible, or DB flags changed.
    Incompatible,
    /// Invalid reuse of reader locktable slot.
    BadRslot,
    /// Transaction cannot recover - it must be aborted.
    BadTxn,
    /// Unsupported size of key/DB name/data, or wrong DUP_FIXED size.
    BadValSize,
    /// The specified DBI was changed unexpectedly.
    BadDbi,
    /// The environment was created by LMDB 0.9.x.
    ///
    /// LMDB 1.0 changed the on-disk format and cannot open 0.9 files; natdb does not
    /// migrate them. The data file is left untouched. `to_err_code` maps this variant
    /// to `MDB_VERSION_MISMATCH`, the closest LMDB code.
    LegacyFormat,
    /// Other error.
    Other(c_int),
}

impl Error {
    /// Converts a raw error code to an `Error`.
    pub fn from_err_code(err_code: c_int) -> Error {
        match err_code {
            ffi::MDB_KEYEXIST => Error::KeyExist,
            ffi::MDB_NOTFOUND => Error::NotFound,
            ffi::MDB_PAGE_NOTFOUND => Error::PageNotFound,
            ffi::MDB_CORRUPTED => Error::Corrupted,
            ffi::MDB_PANIC => Error::Panic,
            ffi::MDB_VERSION_MISMATCH => Error::VersionMismatch,
            ffi::MDB_INVALID => Error::Invalid,
            ffi::MDB_MAP_FULL => Error::MapFull,
            ffi::MDB_DBS_FULL => Error::DbsFull,
            ffi::MDB_READERS_FULL => Error::ReadersFull,
            ffi::MDB_TLS_FULL => Error::TlsFull,
            ffi::MDB_TXN_FULL => Error::TxnFull,
            ffi::MDB_CURSOR_FULL => Error::CursorFull,
            ffi::MDB_PAGE_FULL => Error::PageFull,
            ffi::MDB_MAP_RESIZED => Error::MapResized,
            ffi::MDB_INCOMPATIBLE => Error::Incompatible,
            ffi::MDB_BAD_RSLOT => Error::BadRslot,
            ffi::MDB_BAD_TXN => Error::BadTxn,
            ffi::MDB_BAD_VALSIZE => Error::BadValSize,
            ffi::MDB_BAD_DBI => Error::BadDbi,
            other => Error::Other(other),
        }
    }

    /// Converts an `Error` to the raw error code.
    pub fn to_err_code(&self) -> c_int {
        match *self {
            Error::KeyExist => ffi::MDB_KEYEXIST,
            Error::NotFound => ffi::MDB_NOTFOUND,
            Error::PageNotFound => ffi::MDB_PAGE_NOTFOUND,
            Error::Corrupted => ffi::MDB_CORRUPTED,
            Error::Panic => ffi::MDB_PANIC,
            Error::VersionMismatch => ffi::MDB_VERSION_MISMATCH,
            Error::Invalid => ffi::MDB_INVALID,
            Error::MapFull => ffi::MDB_MAP_FULL,
            Error::DbsFull => ffi::MDB_DBS_FULL,
            Error::ReadersFull => ffi::MDB_READERS_FULL,
            Error::TlsFull => ffi::MDB_TLS_FULL,
            Error::TxnFull => ffi::MDB_TXN_FULL,
            Error::CursorFull => ffi::MDB_CURSOR_FULL,
            Error::PageFull => ffi::MDB_PAGE_FULL,
            Error::MapResized => ffi::MDB_MAP_RESIZED,
            Error::Incompatible => ffi::MDB_INCOMPATIBLE,
            Error::BadRslot => ffi::MDB_BAD_RSLOT,
            Error::BadTxn => ffi::MDB_BAD_TXN,
            Error::BadValSize => ffi::MDB_BAD_VALSIZE,
            Error::BadDbi => ffi::MDB_BAD_DBI,
            Error::LegacyFormat => ffi::MDB_VERSION_MISMATCH,
            Error::Other(err_code) => err_code,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Error::LegacyFormat {
            return fmt.write_str("environment was created by LMDB 0.9, whose file format LMDB 1.0 cannot open");
        }
        // SAFETY: `mdb_strerror` accepts any error code and returns a pointer to a
        // NUL-terminated string that is either static (LMDB codes) or owned by the C
        // runtime (`strerror`); it is only borrowed for the duration of this call.
        let message = unsafe { CStr::from_ptr(ffi::mdb_strerror(self.to_err_code())) };
        fmt.write_str(&message.to_string_lossy())
    }
}

impl std::error::Error for Error {}

/// An LMDB result.
pub type Result<T> = result::Result<T, Error>;

pub(crate) fn lmdb_result(err_code: c_int) -> Result<()> {
    if err_code == ffi::MDB_SUCCESS {
        Ok(())
    } else {
        Err(Error::from_err_code(err_code))
    }
}
