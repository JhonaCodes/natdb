# Triage of the upstream issues and pull requests

natdb continues `danburkert/lmdb-rs` through the `mozilla/lmdb-rs` fork (178
commits ahead of upstream). This is the outcome of every issue and pull request
that was still open in `danburkert/lmdb-rs` when natdb was forked
(2026-10-01). Each entry either points to the test that pins the behavior, or
explains why it does not apply.

## Issues

| # | Title | Outcome | Evidence |
|---|---|---|---|
| 63 | `iter_dup_of` for nonexistent key returns wrong results | Fixed upstream (Mozilla #80) | `tests/cursor.rs::issue_63_iter_dup_of_missing_key_is_empty` |
| 61 | Repository status | natdb is a maintained continuation | `README.md` |
| 60 | Provide safe access to LMDB version | Fixed in natdb: `natdb::version()` | `tests/regressions.rs::issue_60_lmdb_version` |
| 59 | Allow linking with system dylib | Not applicable: natdb requires LMDB 1.0, system packages ship 0.9 (incompatible file format and ABI) | `natdb-sys/build.rs` always builds the vendored source |
| 58 | Potential issue with `DUP_SORT` | Works as expected | `tests/regressions.rs::issue_58_del_dup_sort_value` |
| 57 | Pointer aliasing with read-write cursors | Fixed in natdb: reads go through `RwCursor::reader()`, whose slices borrow the cursor mutably | `compile_fail` example on `RwCursorReader` |
| 52 | `set_max_dbs` confusing signature | Fixed upstream (parameter is `max_dbs`) | `src/environment.rs` |
| 51 | Safe access to `MDB_envinfo` | Fixed upstream: `Environment::info()` | `tests/environment.rs` |
| 42 | Return `Result` from fallible iteration | Fixed upstream: iterators yield `Result` items and never panic | `tests/cursor.rs` |
| 31 | `Cursor::iter_from` panics on empty databases | Fixed upstream | `tests/regressions.rs::issues_27_31_iterating_an_empty_database` |
| 27 | `Cursor::iter_start` panics on empty database | Fixed upstream | same test |
| 20 | Support for `MDB_MULTIPLE` | Not implemented; open feature request | — |
| 13 | `iter_from` panics with duplicate keys | Fixed upstream (iterators return errors instead of panicking) | `tests/cursor.rs` |
| 11 | rustfmt | Done: `cargo fmt --check` runs in CI | `.github/workflows/ci.yml` |
| 4 | Consider merging with `lmdb_rs` | Not applicable | — |

## Pull requests

| # | Title | Outcome |
|---|---|---|
| 62 | Search lmdb library also by name "lmdb" | Not applicable: no system library linking (see #59) |
| 56 | Consistent argument name of `set_max_dbs` | Already consistent (see #52) |
| 54 | Include license file into `lmdb-sys` crate | Done: `natdb-sys` ships `LICENSE` and `NOTICE` |
| 53 | Use upstream name for pkg-config | Not applicable (see #59) |
| 48 | Return error result only from `Iterator::next()` | Merged upstream by Mozilla |
| 46 | Correct size of default memory map | Documented on `EnvironmentBuilder::set_map_size` (LMDB default: 1 MiB) |
| 43 | Add `Environment::copy` | Done: `Environment::copy(path, compact)` — `tests/regressions.rs::pr_43_environment_copy` |
| 40 | Feature for unstable tests/benches on nightly | Not applicable: the nightly benches were removed |
| 32 | Iter getter test that segfaults | Fixed upstream together with #42 |

## Bugs found while forking (not reported upstream)

| Bug | Fix | Test |
|---|---|---|
| `begin_nested_txn` ignored the return code of `mdb_txn_begin` and returned a null transaction | The error is propagated | `tests/regressions.rs::nested_txn_failure_is_reported` |
| `create_db`/`open_db` with a NUL in the name panicked | `Error::Invalid` | `tests/regressions.rs::database_name_with_nul_is_an_error` |
| `open` with a NUL in the path leaked the environment handle | The path is validated before the handle is created | `tests/regressions.rs::path_with_nul_is_an_error` |
| `Error::description()` read `strerror` output with `from_utf8_unchecked` | Removed; `Display` uses `mdb_strerror` | `tests/error.rs` |
