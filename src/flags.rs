use bitflags::bitflags;
use libc::c_uint;

use crate::ffi::*;

bitflags! {
    /// Environment options.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub struct EnvironmentFlags: c_uint {
        /// Use a fixed address for the mmap region. This flag must be specified
        /// when creating the environment, and is stored persistently in the environment.
        /// If successful, the memory map will always reside at the same virtual address
        /// and pointers used to reference data items in the database will be constant
        /// across multiple invocations. This option may not always work, depending on
        /// how the operating system has allocated memory to shared libraries and other uses.
        /// The feature is highly experimental.
        const FIXED_MAP = MDB_FIXEDMAP;

        /// By default, LMDB creates its environment in a directory whose pathname is given in
        /// `path`, and creates its data and lock files under that directory. With this option,
        /// `path` is used as-is for the database main data file. The database lock file is the
        /// `path` with `-lock` appended.
        const NO_SUB_DIR = MDB_NOSUBDIR;

        /// Use a writeable memory map unless `READ_ONLY` is set. This is faster and uses
        /// fewer mallocs, but loses protection from application bugs like wild pointer writes
        /// and other bad updates into the database. Incompatible with nested transactions.
        /// Processes with and without `WRITE_MAP` on the same environment do not cooperate
        /// well.
        const WRITE_MAP = MDB_WRITEMAP;

        /// Open the environment in read-only mode. No write operations will be allowed.
        /// When opening an environment, LMDB will still modify the lock file - except on
        /// read-only filesystems, where LMDB does not use locks.
        const READ_ONLY = MDB_RDONLY;

        /// Flush system buffers to disk only once per transaction, omit the metadata flush.
        /// Defer that until the system flushes files to disk, or next non-`READ_ONLY` commit
        /// or `Environment::sync`. This optimization maintains database integrity, but a
        /// system crash may undo the last committed transaction. I.e. it preserves the ACI
        /// (atomicity, consistency, isolation) but not D (durability) database property.
        ///
        /// This flag may be changed at any time using `Environment::set_flags`.
        const NO_META_SYNC = MDB_NOMETASYNC;

        /// Don't flush system buffers to disk when committing a transaction. This optimization
        /// means a system crash can corrupt the database or lose the last transactions if
        /// buffers are not yet flushed to disk. The risk is governed by how often the system
        /// flushes dirty buffers to disk and how often `Environment::sync` is called. However,
        /// if the filesystem preserves write order and the `WRITE_MAP` flag is not used,
        /// transactions exhibit ACI (atomicity, consistency, isolation) properties and only
        /// lose D (durability). I.e. database integrity is maintained, but a system
        /// crash may undo the final transactions. Note that (`NO_SYNC | WRITE_MAP`) leaves the
        /// system with no hint for when to write transactions to disk, unless
        /// `Environment::sync` is called. (`MAP_ASYNC | WRITE_MAP`) may be preferable.
        ///
        /// This flag may be changed at any time using `Environment::set_flags`.
        const NO_SYNC = MDB_NOSYNC;

        /// When using `WRITE_MAP`, use asynchronous flushes to disk. As with `NO_SYNC`, a
        /// system crash can then corrupt the database or lose the last transactions. Calling
        /// `Environment::sync` ensures on-disk database integrity until next commit.
        ///
        /// This flag may be changed at any time using `Environment::set_flags`.
        const MAP_ASYNC = MDB_MAPASYNC;

        /// Don't use thread-local storage. Tie reader locktable slots to transaction objects
        /// instead of to threads. I.e. `RoTransaction::reset` keeps the slot reserved for the
        /// transaction object. A thread may use parallel read-only transactions. A read-only
        /// transaction may span threads if the user synchronizes its use. Applications that
        /// multiplex many user
        /// threads over individual OS threads need this option. Such an application must also
        /// serialize the write transactions in an OS thread, since LMDB's write locking is
        /// unaware of the user threads.
        const NO_TLS = MDB_NOTLS;

        /// Do not do any locking. If concurrent access is anticipated, the caller must manage
        /// all concurrency themself. For proper operation the caller must enforce
        /// single-writer semantics, and must ensure that no readers are using old
        /// transactions while a writer is active. The simplest approach is to use an exclusive
        /// lock so that no readers may be active at all when a writer begins.
        const NO_LOCK = MDB_NOLOCK;

        /// Turn off readahead. Most operating systems perform readahead on read requests by
        /// default. This option turns it off if the OS supports it. Turning it off may help
        /// random read performance when the DB is larger than RAM and system RAM is full.
        /// The option is not implemented on Windows.
        const NO_READAHEAD = MDB_NORDAHEAD;

        /// Do not initialize malloc'd memory before writing to unused spaces in the data file.
        /// By default, memory for pages written to the data file is obtained using malloc.
        /// While these pages may be reused in subsequent transactions, freshly malloc'd pages
        /// will be initialized to zeroes before use. This avoids persisting leftover data from
        /// other code (that used the heap and subsequently freed the memory) into the data
        /// file. Note that many other system libraries may allocate and free memory from the
        /// heap for arbitrary uses. E.g., stdio may use the heap for file I/O buffers. This
        /// initialization step has a modest performance cost so some applications may want to
        /// disable it using this flag. This option can be a problem for applications which
        /// handle sensitive data like passwords, and it makes memory checkers like Valgrind
        /// noisy. This flag is not needed with `WRITE_MAP`, which writes directly to the mmap
        /// instead of using malloc for pages. The initialization is also skipped if writing
        /// with reserve; the caller is expected to overwrite all of the memory that was
        /// reserved in that case.
        ///
        /// This flag may be changed at any time using `Environment::set_flags`.
        const NO_MEM_INIT = MDB_NOMEMINIT;
    }
}

bitflags! {
    /// Database options.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub struct DatabaseFlags: c_uint {
        /// Keys are strings to be compared in reverse order, from the end of the strings
        /// to the beginning. By default, Keys are treated as strings and compared from
        /// beginning to end.
        const REVERSE_KEY = MDB_REVERSEKEY;

        /// Duplicate keys may be used in the database. (Or, from another perspective,
        /// keys may have multiple data items, stored in sorted order.) By default
        /// keys must be unique and may have only a single data item.
        const DUP_SORT = MDB_DUPSORT;

        /// Keys are binary integers in native byte order. Setting this option requires all
        /// keys to be the same size, typically 32 or 64 bits.
        const INTEGER_KEY = MDB_INTEGERKEY;

        /// This flag may only be used in combination with `DUP_SORT`. This option tells
        /// the library that the data items for this database are all the same size, which
        /// allows further optimizations in storage and retrieval. When all data items are
        /// the same size, the `GET_MULTIPLE` and `NEXT_MULTIPLE` cursor operations may be
        /// used to retrieve multiple items at once.
        const DUP_FIXED = MDB_DUPFIXED;

        /// This option specifies that duplicate data items are also integers, and
        /// should be sorted as such.
        const INTEGER_DUP = MDB_INTEGERDUP;

        /// This option specifies that duplicate data items should be compared as strings
        /// in reverse order.
        const REVERSE_DUP = MDB_REVERSEDUP;
    }
}

bitflags! {
    /// Write options.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
    pub struct WriteFlags: c_uint {
        /// Insert the new item only if the key does not already appear in the database.
        /// The function will return `Error::KeyExist` if the key already appears in the
        /// database, even if the database supports duplicates (`DUP_SORT`).
        const NO_OVERWRITE = MDB_NOOVERWRITE;

        /// Insert the new item only if it does not already appear in the database.
        /// This flag may only be specified if the database was opened with `DUP_SORT`.
        /// The function will return `Error::KeyExist` if the item already appears in the
        /// database.
        const NO_DUP_DATA = MDB_NODUPDATA;

        /// For `Cursor::put`. Replace the item at the current cursor position. The key
        /// parameter must match the current position. If using sorted duplicates (`DUP_SORT`)
        /// the data item must still sort into the same position. This is intended to be used
        /// when the new data is the same size as the old. Otherwise it will simply perform a
        /// delete of the old record followed by an insert.
        const CURRENT = MDB_CURRENT;

        /// Append the given item to the end of the database. No key comparisons are performed.
        /// This option allows fast bulk loading when keys are already known to be in the
        /// correct order. Loading unsorted keys with this flag will cause data corruption.
        const APPEND = MDB_APPEND;

        /// Same as `APPEND`, but for sorted dup data.
        const APPEND_DUP = MDB_APPENDDUP;
    }
}
