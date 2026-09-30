use std::ffi::CString;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::Mutex;
use std::{fmt, mem, ptr, result};

use libc::{c_int, c_uint, size_t};

use crate::cursor::Cursor;
use crate::database::Database;
use crate::error::{Error, Result, lmdb_result};
use crate::ffi;
use crate::flags::{DatabaseFlags, EnvironmentFlags};
use crate::legacy;
use crate::transaction::{RoTransaction, RwTransaction, Transaction};

/// Bytes of `path` as LMDB expects them: raw bytes on Unix, UTF-8 on Windows.
#[cfg(unix)]
fn path_bytes(path: &Path) -> Result<&[u8]> {
    Ok(path.as_os_str().as_bytes())
}

/// Bytes of `path` as LMDB expects them: raw bytes on Unix, UTF-8 on Windows.
#[cfg(windows)]
fn path_bytes(path: &Path) -> Result<&[u8]> {
    path.to_str().map(str::as_bytes).ok_or(Error::Invalid)
}

/// An LMDB environment.
///
/// An environment supports multiple databases, all residing in the same shared-memory map.
pub struct Environment {
    env: *mut ffi::MDB_env,
    dbi_open_mutex: Mutex<()>,
}

impl Environment {
    /// Creates a new builder for specifying options for opening an LMDB environment.
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> EnvironmentBuilder {
        EnvironmentBuilder {
            flags: EnvironmentFlags::empty(),
            max_readers: None,
            max_dbs: None,
            map_size: None,
        }
    }

    /// Returns a raw pointer to the underlying LMDB environment.
    ///
    /// The caller **must** ensure that the pointer is not dereferenced after the lifetime of the
    /// environment.
    pub fn env(&self) -> *mut ffi::MDB_env {
        self.env
    }

    /// Opens a handle to an LMDB database.
    ///
    /// If `name` is `None`, then the returned handle will be for the default database.
    ///
    /// If `name` is not `None`, then the returned handle will be for a named database. In this
    /// case the environment must be configured to allow named databases through
    /// `EnvironmentBuilder::set_max_dbs`.
    ///
    /// The returned database handle may be shared among any transaction in the environment.
    ///
    /// This function will fail with `Error::BadRslot` if called by a thread which has an ongoing
    /// transaction.
    ///
    /// The database name may not contain the null character.
    pub fn open_db(&self, name: Option<&str>) -> Result<Database> {
        let mutex = self.dbi_open_mutex.lock();
        let txn = self.begin_ro_txn()?;
        // SAFETY: `dbi_open_mutex` serializes every database open/create done through
        // this environment, and `txn` is committed before the lock is released.
        let db = unsafe { txn.open_db(name)? };
        txn.commit()?;
        drop(mutex);
        Ok(db)
    }

    /// Opens a handle to an LMDB database, creating the database if necessary.
    ///
    /// If the database is already created, the given option flags will be added to it.
    ///
    /// If `name` is `None`, then the returned handle will be for the default database.
    ///
    /// If `name` is not `None`, then the returned handle will be for a named database. In this
    /// case the environment must be configured to allow named databases through
    /// `EnvironmentBuilder::set_max_dbs`.
    ///
    /// The returned database handle may be shared among any transaction in the environment.
    ///
    /// This function will fail with `Error::BadRslot` if called by a thread with an open
    /// transaction.
    pub fn create_db(&self, name: Option<&str>, flags: DatabaseFlags) -> Result<Database> {
        let mutex = self.dbi_open_mutex.lock();
        let txn = self.begin_rw_txn()?;
        // SAFETY: `dbi_open_mutex` serializes every database open/create done through
        // this environment, and `txn` is committed before the lock is released.
        let db = unsafe { txn.create_db(name, flags)? };
        txn.commit()?;
        drop(mutex);
        Ok(db)
    }

    /// Retrieves the set of flags which the database is opened with.
    ///
    /// The database must belong to to this environment.
    pub fn get_db_flags(&self, db: Database) -> Result<DatabaseFlags> {
        let txn = self.begin_ro_txn()?;
        let mut flags: c_uint = 0;
        // SAFETY: `txn` is a live transaction of this environment and `flags` is a
        // valid out-pointer for the duration of the call.
        lmdb_result(unsafe { ffi::mdb_dbi_flags(txn.txn(), db.dbi(), &mut flags) })?;
        Ok(DatabaseFlags::from_bits_retain(flags))
    }

    /// Copies this environment into the directory `path`, while it stays open.
    ///
    /// `path` must already exist and be empty. With `compact`, free pages are
    /// omitted and pages are renumbered, so the copy is as small as possible
    /// (slower than a plain copy). No lock file is written; it is recreated
    /// when the copy is opened.
    pub fn copy(&self, path: &Path, compact: bool) -> Result<()> {
        let c_path = CString::new(path_bytes(path)?).map_err(|_| Error::Invalid)?;
        let flags = if compact {
            ffi::MDB_CP_COMPACT
        } else {
            0
        };
        // SAFETY: `self.env` is an open environment and `c_path` outlives the call.
        lmdb_result(unsafe { ffi::mdb_env_copy2(self.env, c_path.as_ptr(), flags) })
    }

    /// Create a read-only transaction for use with the environment.
    pub fn begin_ro_txn(&self) -> Result<RoTransaction<'_>> {
        RoTransaction::new(self)
    }

    /// Create a read-write transaction for use with the environment. This method will block while
    /// there are any other read-write transactions open on the environment.
    pub fn begin_rw_txn(&self) -> Result<RwTransaction<'_>> {
        RwTransaction::new(self)
    }

    /// Flush data buffers to disk.
    ///
    /// Data is always written to disk when `Transaction::commit` is called, but the operating
    /// system may keep it buffered. LMDB always flushes the OS buffers upon commit as well, unless
    /// the environment was opened with `MDB_NOSYNC` or in part `MDB_NOMETASYNC`.
    pub fn sync(&self, force: bool) -> Result<()> {
        // SAFETY: `self.env` is an open environment for the lifetime of `self`.
        lmdb_result(unsafe { ffi::mdb_env_sync(self.env(), c_int::from(force)) })
    }

    /// Closes the database handle. Normally unnecessary.
    ///
    /// Closing a database handle is not necessary, but lets `Transaction::open_database` reuse the
    /// handle value. Usually it's better to set a bigger `EnvironmentBuilder::set_max_dbs`, unless
    /// that value would be large.
    ///
    /// ## Safety
    ///
    /// This call is not mutex protected. Databases should only be closed by a single thread, and
    /// only if no other threads are going to reference the database handle or one of its cursors
    /// any further. Do not close a handle if an existing transaction has modified its database.
    /// Doing so can cause misbehavior from database corruption to errors like
    /// `Error::BadValSize` (since the DB name is gone).
    pub unsafe fn close_db(&mut self, db: Database) {
        // SAFETY: the caller upholds the contract documented above; `self.env` is open.
        unsafe { ffi::mdb_dbi_close(self.env, db.dbi()) };
    }

    /// Retrieves statistics about this environment.
    pub fn stat(&self) -> Result<Stat> {
        let mut stat = Stat::new();
        // SAFETY: `self.env` is open and `stat.mdb_stat()` points to a live `MDB_stat`.
        lmdb_try!(unsafe { ffi::mdb_env_stat(self.env(), stat.mdb_stat()) });
        Ok(stat)
    }

    /// Retrieves info about this environment.
    pub fn info(&self) -> Result<Info> {
        // SAFETY: `MDB_envinfo` is a plain C struct (integers and a raw pointer) for
        // which all-zero bytes are a valid value.
        let mut info = Info(unsafe { mem::zeroed() });
        // SAFETY: `self.env` is open and `info.0` is a valid out-pointer.
        lmdb_try!(unsafe { ffi::mdb_env_info(self.env(), &mut info.0) });
        Ok(info)
    }

    /// Retrieves the total number of pages on the freelist.
    ///
    /// Along with `Environment::info()`, this can be used to calculate the exact number
    /// of used pages as well as free pages in this environment.
    ///
    /// ```ignore
    /// let env = Environment::new().open("/tmp/test").unwrap();
    /// let info = env.info().unwrap();
    /// let stat = env.stat().unwrap();
    /// let freelist = env.freelist().unwrap();
    /// let last_pgno = info.last_pgno() + 1; // pgno is 0 based.
    /// let total_pgs = info.map_size() / stat.page_size() as usize;
    /// let pgs_in_use = last_pgno - freelist;
    /// let pgs_free = total_pgs - pgs_in_use;
    /// ```
    ///
    /// Note:
    ///
    /// * LMDB stores all the freelists in the designated database 0 in each environment,
    ///   and the freelist count is stored at the beginning of the value as `libc::size_t`
    ///   in the native byte order.
    ///
    /// * It will create a read transaction to traverse the freelist database.
    pub fn freelist(&self) -> Result<size_t> {
        let mut freelist: size_t = 0;
        let db = Database::freelist_db();
        let txn = self.begin_ro_txn()?;
        let mut cursor = txn.open_ro_cursor(db)?;

        for result in cursor.iter() {
            let (_key, value) = result?;
            if value.len() < mem::size_of::<size_t>() {
                return Err(Error::Corrupted);
            }

            let mut count = [0u8; mem::size_of::<size_t>()];
            count.copy_from_slice(&value[..mem::size_of::<size_t>()]);
            freelist += size_t::from_ne_bytes(count);
        }

        Ok(freelist)
    }

    /// Sets the size of the memory map to use for the environment.
    ///
    /// This could be used to resize the map when the environment is already open.
    ///
    /// Note:
    ///
    /// * No active transactions allowed when performing resizing in this process.
    ///
    /// * The size should be a multiple of the OS page size. Any attempt to set
    ///   a size smaller than the space already consumed by the environment will
    ///   be silently changed to the current size of the used space.
    ///
    /// * In the multi-process case, once a process resizes the map, other
    ///   processes need to either re-open the environment, or call set_map_size
    ///   with size 0 to update the environment. Otherwise, new transaction creation
    ///   will fail with `Error::MapResized`.
    pub fn set_map_size(&self, size: size_t) -> Result<()> {
        // SAFETY: `self.env` is open; LMDB validates the requested size itself.
        lmdb_result(unsafe { ffi::mdb_env_set_mapsize(self.env(), size) })
    }
}

/// Environment statistics.
///
/// Contains information about the size and layout of an LMDB environment or database.
pub struct Stat(ffi::MDB_stat);

impl Stat {
    /// Create a new Stat with zero'd inner struct `ffi::MDB_stat`.
    pub(crate) fn new() -> Stat {
        // SAFETY: `MDB_stat` only contains integers, so all-zero bytes are a valid value.
        Stat(unsafe { mem::zeroed() })
    }

    /// Returns a mut pointer to `ffi::MDB_stat`.
    pub(crate) fn mdb_stat(&mut self) -> *mut ffi::MDB_stat {
        &mut self.0
    }
}

impl Stat {
    /// Size of a database page. This is the same for all databases in the environment.
    #[inline]
    pub fn page_size(&self) -> u32 {
        self.0.ms_psize
    }

    /// Depth (height) of the B-tree.
    #[inline]
    pub fn depth(&self) -> u32 {
        self.0.ms_depth
    }

    /// Number of internal (non-leaf) pages.
    #[inline]
    pub fn branch_pages(&self) -> usize {
        self.0.ms_branch_pages
    }

    /// Number of leaf pages.
    #[inline]
    pub fn leaf_pages(&self) -> usize {
        self.0.ms_leaf_pages
    }

    /// Number of overflow pages.
    #[inline]
    pub fn overflow_pages(&self) -> usize {
        self.0.ms_overflow_pages
    }

    /// Number of data items.
    #[inline]
    pub fn entries(&self) -> usize {
        self.0.ms_entries
    }
}

/// Environment information.
///
/// Contains environment information about the map size, readers, last txn id etc.
pub struct Info(ffi::MDB_envinfo);

impl Info {
    /// Size of memory map.
    #[inline]
    pub fn map_size(&self) -> usize {
        self.0.me_mapsize
    }

    /// Last used page number
    #[inline]
    pub fn last_pgno(&self) -> usize {
        self.0.me_last_pgno
    }

    /// Last transaction ID
    #[inline]
    pub fn last_txnid(&self) -> usize {
        self.0.me_last_txnid
    }

    /// Max reader slots in the environment
    #[inline]
    pub fn max_readers(&self) -> u32 {
        self.0.me_maxreaders
    }

    /// Max reader slots used in the environment
    #[inline]
    pub fn num_readers(&self) -> u32 {
        self.0.me_numreaders
    }
}

// SAFETY: an `MDB_env` may be used from any thread; LMDB serializes writers
// internally and database opening is serialized by `dbi_open_mutex`.
unsafe impl Send for Environment {}
// SAFETY: see the `Send` impl above.
unsafe impl Sync for Environment {}

impl fmt::Debug for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> result::Result<(), fmt::Error> {
        f.debug_struct("Environment").finish()
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        // SAFETY: `self.env` was opened by `EnvironmentBuilder::open_with_permissions`
        // and every transaction borrows `self`, so none can outlive this call.
        unsafe { ffi::mdb_env_close(self.env) }
    }
}

// Environment builder.

/// Options for opening or creating an environment.
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct EnvironmentBuilder {
    flags: EnvironmentFlags,
    max_readers: Option<c_uint>,
    max_dbs: Option<c_uint>,
    map_size: Option<size_t>,
}

impl EnvironmentBuilder {
    /// Open an environment.
    ///
    /// On UNIX, the database files will be opened with 644 permissions.
    ///
    /// The path may not contain the null character, Windows UNC (Uniform Naming Convention)
    /// paths are not supported either.
    pub fn open(&self, path: &Path) -> Result<Environment> {
        self.open_with_permissions(path, 0o644)
    }

    /// Open an environment with the provided UNIX permissions.
    ///
    /// On Windows, the permissions will be ignored.
    ///
    /// Fails with `Error::LegacyFormat`, without modifying the data file, when the
    /// environment was created by LMDB 0.9.x.
    ///
    /// The path may not contain the null character, Windows UNC (Uniform Naming Convention)
    /// paths are not supported either.
    pub fn open_with_permissions(&self, path: &Path, mode: ffi::mdb_mode_t) -> Result<Environment> {
        // Validate the path before creating the environment, so that an invalid
        // path cannot leak a freshly created handle.
        let c_path = CString::new(path_bytes(path)?).map_err(|_| Error::Invalid)?;
        let mut env: *mut ffi::MDB_env = ptr::null_mut();
        // SAFETY: `mdb_env_create` only writes the new handle into `env`. Every later call
        // receives that handle and, when an LMDB call fails, the handle is closed exactly
        // once before returning. `c_path` outlives the `mdb_env_open` call that borrows it.
        unsafe {
            lmdb_try!(ffi::mdb_env_create(&mut env));
            if let Some(max_readers) = self.max_readers {
                lmdb_try_with_cleanup!(ffi::mdb_env_set_maxreaders(env, max_readers), ffi::mdb_env_close(env))
            }
            if let Some(max_dbs) = self.max_dbs {
                lmdb_try_with_cleanup!(ffi::mdb_env_set_maxdbs(env, max_dbs), ffi::mdb_env_close(env))
            }
            if let Some(map_size) = self.map_size {
                lmdb_try_with_cleanup!(ffi::mdb_env_set_mapsize(env, map_size), ffi::mdb_env_close(env))
            }
            let err_code = ffi::mdb_env_open(env, c_path.as_ptr(), self.flags.bits(), mode);
            if err_code != ffi::MDB_SUCCESS {
                ffi::mdb_env_close(env);
                return Err(legacy::open_error(err_code, path, self.flags));
            }
        }
        Ok(Environment {
            env,
            dbi_open_mutex: Mutex::new(()),
        })
    }

    /// Sets the provided options in the environment.
    pub fn set_flags(&mut self, flags: EnvironmentFlags) -> &mut EnvironmentBuilder {
        self.flags = flags;
        self
    }

    /// Sets the maximum number of threads or reader slots for the environment.
    ///
    /// This defines the number of slots in the lock table that is used to track readers in the
    /// the environment. The default is 126. Starting a read-only transaction normally ties a lock
    /// table slot to the current thread until the environment closes or the thread exits. If
    /// `MDB_NOTLS` is in use, `Environment::open_txn` instead ties the slot to the `Transaction`
    /// object until it or the `Environment` object is destroyed.
    pub fn set_max_readers(&mut self, max_readers: c_uint) -> &mut EnvironmentBuilder {
        self.max_readers = Some(max_readers);
        self
    }

    /// Sets the maximum number of named databases for the environment.
    ///
    /// This function is only needed if multiple databases will be used in the
    /// environment. Simpler applications that use the environment as a single
    /// unnamed database can ignore this option.
    ///
    /// Currently a moderate number of slots are cheap but a huge number gets
    /// expensive: 7-120 words per transaction, and every `Transaction::open_db`
    /// does a linear search of the opened slots.
    pub fn set_max_dbs(&mut self, max_dbs: c_uint) -> &mut EnvironmentBuilder {
        self.max_dbs = Some(max_dbs);
        self
    }

    /// Sets the size of the memory map to use for the environment.
    ///
    /// The size should be a multiple of the OS page size. The default is
    /// 1048576 bytes. The size of the memory map is also the maximum size
    /// of the database. The value should be chosen as large as possible,
    /// to accommodate future growth of the database. It may be increased at
    /// later times.
    ///
    /// Any attempt to set a size smaller than the space already consumed
    /// by the environment will be silently changed to the current size of the used space.
    pub fn set_map_size(&mut self, map_size: size_t) -> &mut EnvironmentBuilder {
        self.map_size = Some(map_size);
        self
    }
}
