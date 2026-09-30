//! Integration tests for `Error`.

use natdb::Error;

/// Message of the errno-style code 13 (`EACCES`) as rendered by `mdb_strerror`.
///
/// On Unix `mdb_strerror` falls back to `strerror`. On Windows LMDB 1.0 only uses
/// `strerror` for `ENOMEM`/`EINVAL` and formats every other code as a Win32 error,
/// and Win32 error 13 is `ERROR_INVALID_DATA` (LMDB 0.9 still used `strerror` here).
#[cfg(unix)]
const EACCES_MESSAGE: &str = "Permission denied";
#[cfg(windows)]
const EACCES_MESSAGE: &str = "The data is invalid.";

/// The message of an error comes from `mdb_strerror`: the OS message for errno
/// values and LMDB's own message for LMDB codes.
#[test]
fn test_description() {
    assert_eq!(EACCES_MESSAGE, Error::from_err_code(13).to_string().trim_end());
    assert_eq!("MDB_NOTFOUND: No matching key/data pair found", Error::NotFound.to_string());
}
