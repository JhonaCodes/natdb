# Changelog

## 0.1.0 — 2026-10-01

First release of natdb, a maintained continuation of `mozilla/lmdb-rs`
(`lmdb-rkv` 0.14.0), itself a fork of `danburkert/lmdb-rs`.

- LMDB 1.0.2, vendored from the official OpenLDAP sources (tag `LMDB_1.0.2`).
- Crates renamed to `natdb` and `natdb-sys`; edition 2024, MSRV 1.85.
- `Error::LegacyFormat` when opening an LMDB 0.9 environment, which LMDB 1.0
  cannot read; the data file is left untouched.
- `natdb::version()` and `Environment::copy(path, compact)`.
- Soundness: read-write cursors read through `RwCursor::reader()`, so their
  slices cannot be modified by a later write while alive (upstream #57).
- Fixes: `begin_nested_txn` reports `mdb_txn_begin` failures instead of
  returning a null transaction; a NUL in a database name or path is an error
  instead of a panic or a leaked environment; `Error::description()` removed.
- Dependencies updated (bitflags 2, cc 1.5, bindgen 0.73 optional); `byteorder`,
  `pkg-config`, `rand` and the deprecated `tempdir` removed.
- POSIX semaphores on Apple targets (required by the App Sandbox).
- Tests moved to `tests/`; CI on macOS, Linux and Windows plus iOS and Android
  cross builds.
