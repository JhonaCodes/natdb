//! The vendored engine, and the bindings generated from its header, are LMDB 1.0.2.

use std::ffi::{CStr, c_int};

use natdb_sys::{MDB_VERSION_MAJOR, MDB_VERSION_MINOR, MDB_VERSION_PATCH, mdb_version};

#[test]
fn compiled_library_is_lmdb_1_0_2() {
    let (mut major, mut minor, mut patch): (c_int, c_int, c_int) = (0, 0, 0);
    // SAFETY: the three out-pointers are valid; LMDB returns a pointer to a static string.
    let version = unsafe { CStr::from_ptr(mdb_version(&mut major, &mut minor, &mut patch)) };

    assert_eq!((major, minor, patch), (1, 0, 2), "mdb_version() returned {version:?}");
    assert!(version.to_bytes().starts_with(b"LMDB 1.0.2:"), "mdb_version() returned {version:?}");
}

#[test]
fn bindings_were_generated_from_the_lmdb_1_0_2_header() {
    assert_eq!((MDB_VERSION_MAJOR, MDB_VERSION_MINOR, MDB_VERSION_PATCH), (1, 0, 2));
}
