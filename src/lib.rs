//! Idiomatic and safe APIs for interacting with the
//! [Lightning Memory-mapped Database (LMDB)](https://www.symas.com/lmdb).
//!
//! `natdb` is a maintained fork of Mozilla's `lmdb-rkv` (itself a fork of Dan
//! Burkert's `lmdb-rs`). The raw FFI layer lives in the `natdb-sys` crate, which
//! builds LMDB from vendored upstream sources.

#![deny(missing_docs)]

use natdb_sys as ffi;

pub use crate::cursor::{Cursor, Iter, IterDup, RoCursor, RwCursor, RwCursorReader};
pub use crate::database::Database;
pub use crate::environment::{Environment, EnvironmentBuilder, Info, Stat};
pub use crate::error::{Error, Result};
pub use crate::flags::*;
pub use crate::transaction::{InactiveTransaction, RoTransaction, RwTransaction, Transaction};
pub use crate::version::{Version, version};

macro_rules! lmdb_try {
    ($expr:expr) => {{
        match $expr {
            $crate::ffi::MDB_SUCCESS => (),
            err_code => return Err($crate::Error::from_err_code(err_code)),
        }
    }};
}

macro_rules! lmdb_try_with_cleanup {
    ($expr:expr, $cleanup:expr) => {{
        match $expr {
            $crate::ffi::MDB_SUCCESS => (),
            err_code => {
                let _ = $cleanup;
                return Err($crate::Error::from_err_code(err_code));
            },
        }
    }};
}

mod cursor;
mod database;
mod environment;
mod error;
mod flags;
mod legacy;
mod transaction;
mod version;
