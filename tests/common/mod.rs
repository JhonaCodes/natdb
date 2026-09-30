//! Helpers shared by the integration tests.

use tempfile::TempDir;

/// Creates a fresh, empty temporary directory to host an LMDB environment.
///
/// The directory and everything in it is deleted when the returned guard drops.
pub fn temp_dir() -> TempDir {
    tempfile::Builder::new().prefix("natdb-test").tempdir().expect("create temporary directory")
}
