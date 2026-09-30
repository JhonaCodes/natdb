//! Environments written by LMDB 0.9.x must be rejected with a typed error and
//! without touching their data file.
//!
//! The fixture in `tests/fixtures/lmdb_0_9` is a copy of
//! `offline_first_core/tests/fixtures/v0_5_0/compat.lmdb`, created by LMDB 0.9.21
//! on Apple Silicon (16 KiB pages).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use natdb::{Environment, EnvironmentFlags, Error};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join("lmdb_0_9")
}

/// Copies the 0.9 fixture into `dir` as `<data_name>` and `<lock_name>`.
fn copy_fixture(dir: &Path, data_name: &str, lock_name: &str) {
    fs::copy(fixture_dir().join("data.mdb"), dir.join(data_name)).expect("copy data.mdb");
    fs::copy(fixture_dir().join("lock.mdb"), dir.join(lock_name)).expect("copy lock.mdb");
}

/// Opens `path` read-write and read-only with the given base flags and returns both errors.
fn open_errors(path: &Path, base: EnvironmentFlags) -> (Option<Error>, Option<Error>) {
    let read_write = Environment::new().set_flags(base).open(path).err();
    let read_only = Environment::new().set_flags(base | EnvironmentFlags::READ_ONLY).open(path).err();
    (read_write, read_only)
}

#[test]
fn opening_a_0_9_environment_fails_with_legacy_format_and_keeps_data_intact() {
    let dir = common::temp_dir();
    copy_fixture(dir.path(), "data.mdb", "lock.mdb");
    let data_before = fs::read(dir.path().join("data.mdb")).expect("read data.mdb");

    let (read_write, read_only) = open_errors(dir.path(), EnvironmentFlags::empty());

    assert_eq!(read_write, Some(Error::LegacyFormat), "read-write open");
    assert_eq!(read_only, Some(Error::LegacyFormat), "read-only open");
    let data_after = fs::read(dir.path().join("data.mdb")).expect("read data.mdb");
    assert!(data_before == data_after, "data.mdb was modified by the failed open");
}

#[test]
fn opening_a_0_9_environment_with_no_sub_dir_fails_with_legacy_format() {
    let dir = common::temp_dir();
    copy_fixture(dir.path(), "legacy.mdb", "legacy.mdb-lock");
    let data_path = dir.path().join("legacy.mdb");
    let data_before = fs::read(&data_path).expect("read legacy.mdb");

    let (read_write, read_only) = open_errors(&data_path, EnvironmentFlags::NO_SUB_DIR);

    assert_eq!(read_write, Some(Error::LegacyFormat), "read-write open");
    assert_eq!(read_only, Some(Error::LegacyFormat), "read-only open");
    assert!(data_before == fs::read(&data_path).expect("read legacy.mdb"), "legacy.mdb was modified");
}

/// Guard: only genuine 0.9 files are reported as legacy; any other invalid file
/// keeps LMDB's own error.
#[test]
fn a_file_that_is_not_lmdb_is_still_invalid() {
    let dir = common::temp_dir();
    fs::write(dir.path().join("data.mdb"), vec![0x5A_u8; 64 * 1024]).expect("write data.mdb");

    let (read_write, read_only) = open_errors(dir.path(), EnvironmentFlags::empty());

    assert_eq!(read_write, Some(Error::Invalid), "read-write open");
    assert_eq!(read_only, Some(Error::Invalid), "read-only open");
}
