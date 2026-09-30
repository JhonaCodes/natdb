//! Regenerates `src/bindings.rs` from the vendored `lmdb/lmdb.h`.
//!
//! Only compiled into the build script when the `bindgen` feature is enabled.
//! The generated file is committed, so regular builds never need libclang.

use std::fmt;
use std::io;
use std::path::Path;

use bindgen::callbacks::{IntKind, ParseCallbacks};
use bindgen::{BindgenError, Builder, RustEdition, RustTarget};

/// Minor version of the Rust MSRV declared in the workspace manifest (1.85).
const MSRV_MINOR: u64 = 85;

/// Errors that can happen while regenerating the bindings.
#[derive(Debug)]
pub enum Error {
    /// The pinned Rust target is not supported by bindgen (its error type is
    /// not exported, so only the message is kept).
    RustTarget(String),
    /// libclang could not parse `lmdb.h`.
    Bindgen(BindgenError),
    /// The generated file could not be written.
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::RustTarget(message) => write!(f, "invalid bindgen Rust target: {message}"),
            Error::Bindgen(err) => write!(f, "bindgen failed to parse lmdb.h: {err}"),
            Error::Io(err) => write!(f, "failed to write src/bindings.rs: {err}"),
        }
    }
}

/// Types LMDB integer macros so they match the C types they are compared with.
#[derive(Debug)]
struct Callbacks;

impl ParseCallbacks for Callbacks {
    fn int_macro(&self, name: &str, value: i64) -> Option<IntKind> {
        // Return codes (`MDB_SUCCESS` and the negative `MDB_*` error codes) are
        // compared against `int` results; every other macro is an unsigned
        // flag, cursor operation or version number.
        if name == "MDB_SUCCESS" || value < 0 {
            Some(IntKind::Int)
        } else {
            Some(IntKind::UInt)
        }
    }
}

/// Parses `<lmdb_dir>/lmdb.h` and writes the bindings to `<out_dir>/bindings.rs`.
pub fn generate(lmdb_dir: &Path, out_dir: &Path) -> Result<(), Error> {
    let rust_target = RustTarget::stable(MSRV_MINOR, 0).map_err(|err| Error::RustTarget(err.to_string()))?;
    let bindings = Builder::default()
        .header(lmdb_dir.join("lmdb.h").to_string_lossy())
        .allowlist_var("^(MDB|mdb)_.*")
        .allowlist_type("^(MDB|mdb)_.*")
        .allowlist_function("^(MDB|mdb)_.*")
        // Defined by hand in `lib.rs` because they differ between Unix and Windows.
        .blocklist_item("mode_t")
        .blocklist_item("mdb_mode_t")
        .blocklist_item("mdb_filehandle_t")
        // printf/scanf helpers and limits whose values depend on the host platform.
        .blocklist_item("MDB_FMT_Z")
        .blocklist_item("MDB_SIZE_MAX")
        .blocklist_item("^__.*")
        // Implemented in upstream's `module.c`, which is not vendored: they
        // `dlopen` crypto plugins at runtime, which iOS does not allow.
        .blocklist_function("mdb_modload")
        .blocklist_function("mdb_modunload")
        .blocklist_function("mdb_modsetup")
        .size_t_is_usize(true)
        .ctypes_prefix("::libc")
        .parse_callbacks(Box::new(Callbacks))
        .layout_tests(false)
        .prepend_enum_name(false)
        .merge_extern_blocks(true)
        .rust_target(rust_target)
        .rust_edition(RustEdition::Edition2024)
        .generate()
        .map_err(Error::Bindgen)?;

    bindings.write_to_file(out_dir.join("bindings.rs")).map_err(Error::Io)
}
