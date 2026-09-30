//! Recognition of environments written by LMDB 0.9.x.
//!
//! LMDB 1.0 cannot open 0.9 data files: its page header gained a transaction id,
//! so it looks for the meta page at a different offset and reports `MDB_INVALID`
//! ("not an LMDB file"), or `MDB_VERSION_MISMATCH` when a 0.9 process still holds
//! the lock file. Neither code says the file is a valid, older environment, so
//! after such a failure natdb reads the first bytes of the data file itself and
//! reports `Error::LegacyFormat` when they carry the 0.9 meta page signature.

use std::fs::File;
use std::io::Read;
use std::mem;
use std::path::{Path, PathBuf};

use libc::c_int;

use crate::error::Error;
use crate::ffi;
use crate::flags::EnvironmentFlags;

/// `MDB_MAGIC`: stamp at the start of every LMDB meta page (0.9 and 1.0).
const MDB_MAGIC: u32 = 0xBEEF_C0DE;

/// `MDB_DATA_VERSION` written by LMDB 0.9.x (1.0 writes 3).
const LMDB_0_9_DATA_VERSION: u32 = 1;

/// Offset of `mm_magic` in the first meta page of a 0.9 file: the 0.9 page header
/// is `pgno_t` (a `size_t`) followed by four `u16` fields (pad, flags, lower, upper).
const LMDB_0_9_MAGIC_OFFSET: usize = mem::size_of::<usize>() + 4 * mem::size_of::<u16>();

/// Name of the data file inside an environment directory.
const DATA_FILE_NAME: &str = "data.mdb";

/// Maps an `mdb_env_open` failure for `path` to an `Error`, reporting
/// `Error::LegacyFormat` when the data file was written by LMDB 0.9.x.
pub(crate) fn open_error(err_code: c_int, path: &Path, flags: EnvironmentFlags) -> Error {
    let may_be_legacy = err_code == ffi::MDB_INVALID || err_code == ffi::MDB_VERSION_MISMATCH;
    if may_be_legacy && is_lmdb_0_9_data_file(&data_file_path(path, flags)) {
        Error::LegacyFormat
    } else {
        Error::from_err_code(err_code)
    }
}

fn data_file_path(path: &Path, flags: EnvironmentFlags) -> PathBuf {
    if flags.contains(EnvironmentFlags::NO_SUB_DIR) {
        path.to_path_buf()
    } else {
        path.join(DATA_FILE_NAME)
    }
}

/// Returns whether the file at `path` starts with an LMDB 0.9 meta page.
///
/// The file is only opened for reading. Any I/O failure means "not recognized".
fn is_lmdb_0_9_data_file(path: &Path) -> bool {
    let mut header = [0u8; LMDB_0_9_MAGIC_OFFSET + 2 * mem::size_of::<u32>()];
    let read = File::open(path).and_then(|mut file| file.read_exact(&mut header));
    if read.is_err() {
        return false;
    }
    native_u32_at(&header, LMDB_0_9_MAGIC_OFFSET) == Some(MDB_MAGIC)
        && native_u32_at(&header, LMDB_0_9_MAGIC_OFFSET + mem::size_of::<u32>()) == Some(LMDB_0_9_DATA_VERSION)
}

/// Reads a native-endian `u32` (LMDB files use the byte order of the host) at `offset`.
fn native_u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let field = bytes.get(offset..offset.checked_add(mem::size_of::<u32>())?)?;
    let mut value = [0u8; mem::size_of::<u32>()];
    value.copy_from_slice(field);
    Some(u32::from_ne_bytes(value))
}
