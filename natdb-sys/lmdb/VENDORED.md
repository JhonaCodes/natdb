# Vendored LMDB sources

These files are an unmodified copy of the official LMDB release.

| Field      | Value |
|------------|-------|
| Release    | LMDB 1.0.2 (released 2026-09-08) |
| Repository | https://git.openldap.org/openldap/openldap.git |
| Tag        | `LMDB_1.0.2` (annotated, PGP-signed; tag object `d8587416d516c30c87a0112e8e7c1f46e621efdf`) |
| Commit     | `c279e0477c713076893e8c2b1a6337e421c15867` ("Prep for release (LMDB 1.0.2)", 2026-09-08) |
| Path       | `libraries/liblmdb/` |
| Vendored   | 2026-10-01 |

## Files

Every file is byte-identical to the tagged tree; the git blob id (`git hash-object`)
matches `git rev-parse LMDB_1.0.2^{commit}:libraries/liblmdb/<file>`.

| File        | Git blob                                   | SHA-256 |
|-------------|--------------------------------------------|---------|
| `mdb.c`     | `c20786da21a97e6195fd84b6126c030ac76d05d9` | `6a84f672a05b1dda9f4e3aaae2a4a0bac73683752ec9640518b904b2f9a11109` |
| `midl.c`    | `8a8fda7ab53dc14d53c4ebe1960c694a980825af` | `26a37f2e94e147e7cce4c8875c94d239d528d1b7b6ac6dfa5d73673dd7fd29f6` |
| `lmdb.h`    | `164caa9e637225d190d33a69364990a8909ed923` | `fe585a964eb6123f0569608a7d94485f8ca227a49a1ef9e51cc27805afefa5e2` |
| `midl.h`    | `1cccb92eab4d8d8ab7151a0c3969ba4db48ff7a5` | `ff8babe69bb4a90aca5798789264c53de4f9efaee2f8cea3ab617c05d7e842d8` |
| `LICENSE`   | `05ad7571e448b9d83ead5d4691274d9484574714` | `310fe25c858a9515fc8c8d7d1f24a67c9496f84a91e0a0e41ea9975b1371e569` |
| `COPYRIGHT` | `14eb1493d625c27ba4eecd41e0454946658d972c` | `3c87f63656b6a12fda225b6f798c547fe89e3404f499e890b445761eb310fd4f` |
| `CHANGES`   | `cb4bb48202e5adac6fa1a2418fac891183119df6` | `badd6eeca60fa469af5d8edc753c83d6f98e61801ee64471f2aeeb8e8d97d924` |

`LICENSE` is the OpenLDAP Public License 2.8, which covers these files.

## Not vendored

- `module.c`: helpers (`mdb_modload`, `mdb_modunload`, `mdb_modsetup`) that
  `dlopen` an encryption/checksum plugin at runtime. They are not needed to build
  the core, and loading code at runtime is not allowed on iOS. The core
  `mdb_env_set_encrypt` / `mdb_env_set_checksum` API is in `mdb.c` and is declared
  by `natdb-sys`; the three module functions are left out of the bindings.
- `crypto.c`, `cryptoc.c`: sample plugins for that module interface (they need
  libsodium).
- `chacha8.c`, `chacha8.h`: a toy cipher used only by `mtest_enc.c`.
- Command-line tools (`mdb_*.c`), tests (`mtest*.c`, `mplay.c`), man pages,
  `Makefile`, `Doxyfile` and documentation.

## Updating

1. Fetch the new tag into a scratch clone and copy the files listed above.
2. Update this file (tag, commit, date, hashes).
3. Regenerate the bindings with a host libclang:
   `LIBCLANG_PATH=<dir with libclang> cargo build -p natdb-sys --features bindgen`.
4. Update the version assertions in `natdb-sys/tests/version.rs`.
