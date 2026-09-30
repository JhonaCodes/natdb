//! Version of the LMDB library natdb is linked against.

use std::ffi::CStr;

use libc::c_int;

use crate::ffi;

/// Version of the LMDB C library compiled into this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    /// Major version number.
    pub major: i32,
    /// Minor version number.
    pub minor: i32,
    /// Patch version number.
    pub patch: i32,
    /// Human readable version string, for example `"LMDB 1.0.2: (...)"`.
    pub string: &'static str,
}

/// Returns the version of the LMDB library natdb is linked against.
pub fn version() -> Version {
    let (mut major, mut minor, mut patch): (c_int, c_int, c_int) = (0, 0, 0);
    // SAFETY: the out-pointers are valid for the call, and `mdb_version` returns a
    // pointer to a static NUL-terminated string that lives for the whole program.
    let string = unsafe {
        let raw = ffi::mdb_version(&mut major, &mut minor, &mut patch);
        CStr::from_ptr(raw)
    };
    Version {
        major,
        minor,
        patch,
        string: string.to_str().unwrap_or_default(),
    }
}
