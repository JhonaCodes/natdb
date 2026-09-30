//! Build script for `natdb-sys`.
//!
//! Compiles the vendored LMDB sources in `lmdb/` into a static library and,
//! with the `bindgen` feature, regenerates `src/bindings.rs` from `lmdb/lmdb.h`.

use std::env::{self, VarError};
use std::fmt;
use std::path::PathBuf;
use std::process::ExitCode;

#[cfg(feature = "bindgen")]
#[path = "bindgen.rs"]
mod generate;

/// Values of `N` accepted by the `mdb_idl_logn_N` features.
const IDL_LOGN_VALUES: std::ops::RangeInclusive<u8> = 8..=15;

/// Errors that abort the build.
#[derive(Debug)]
enum BuildError {
    /// A variable that Cargo always provides to build scripts is missing or not UTF-8.
    CargoEnv(&'static str, VarError),
    /// More than one `mdb_idl_logn_N` feature is enabled; they are mutually exclusive.
    ConflictingIdlLogn(Vec<u8>),
    /// The C compiler failed on the vendored LMDB sources.
    Compile(cc::Error),
    /// Regenerating the bindings failed.
    #[cfg(feature = "bindgen")]
    Bindings(generate::Error),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::CargoEnv(name, err) => write!(f, "cargo did not provide {name}: {err}"),
            BuildError::ConflictingIdlLogn(values) => {
                write!(f, "the mdb_idl_logn_N features are mutually exclusive, got N = {values:?}")
            },
            BuildError::Compile(err) => write!(f, "compiling the vendored LMDB sources failed: {err}"),
            #[cfg(feature = "bindgen")]
            BuildError::Bindings(err) => write!(f, "{err}"),
        }
    }
}

fn cargo_env(name: &'static str) -> Result<String, BuildError> {
    env::var(name).map_err(|err| BuildError::CargoEnv(name, err))
}

fn feature_enabled(name: &str) -> bool {
    env::var_os(format!("CARGO_FEATURE_{}", name.to_uppercase())).is_some()
}

/// Returns the `MDB_IDL_LOGN` value selected by the `mdb_idl_logn_N` features, if any.
fn selected_idl_logn() -> Result<Option<u8>, BuildError> {
    let selected: Vec<u8> = IDL_LOGN_VALUES.filter(|n| feature_enabled(&format!("mdb_idl_logn_{n}"))).collect();
    match selected.as_slice() {
        [] => Ok(None),
        [n] => Ok(Some(*n)),
        _ => Err(BuildError::ConflictingIdlLogn(selected)),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("natdb-sys build failed: {err}");
            ExitCode::FAILURE
        },
    }
}

fn run() -> Result<(), BuildError> {
    let lmdb = PathBuf::from(cargo_env("CARGO_MANIFEST_DIR")?).join("lmdb");
    println!("cargo:rerun-if-changed=lmdb");

    #[cfg(feature = "bindgen")]
    {
        let src = PathBuf::from(cargo_env("CARGO_MANIFEST_DIR")?).join("src");
        generate::generate(&lmdb, &src).map_err(BuildError::Bindings)?;
    }

    let target_os = cargo_env("CARGO_CFG_TARGET_OS")?;
    let target_vendor = cargo_env("CARGO_CFG_TARGET_VENDOR")?;

    let mut builder = cc::Build::new();
    builder
        .file(lmdb.join("mdb.c"))
        .file(lmdb.join("midl.c"))
        // Warning set of upstream's Makefile (`W = -W -Wall -Wno-unused-parameter ...`).
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wbad-function-cast")
        .flag_if_supported("-Wuninitialized");

    if let Some(logn) = selected_idl_logn()? {
        builder.define("MDB_IDL_LOGN", logn.to_string().as_str());
    }

    if target_vendor == "apple" {
        // The iOS and macOS App Sandbox forbid SysV semaphores; LMDB must use
        // named POSIX semaphores for its reader/writer locks. LMDB 1.0.2 already
        // picks them on Apple unless MDB_USE_ROBUST is set (mdb.c, "__APPLE__"
        // branch of the platform block); defining it here keeps that guarantee
        // independent of upstream defaults.
        builder.define("MDB_USE_POSIX_SEM", "1");
    }

    builder.try_compile("lmdb").map_err(BuildError::Compile)?;

    if target_os == "windows" {
        // mdb.c calls InitializeSecurityDescriptor/SetSecurityDescriptorDacl.
        println!("cargo:rustc-link-lib=advapi32");
    }

    Ok(())
}
