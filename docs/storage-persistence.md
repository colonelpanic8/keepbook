# File persistence

`src/storage/file_io.rs` supplies file operations shared by `JsonFileStorage` and
`JsonlMarketDataStore`. JSON, TOML, and JSONL data formats are unchanged.

## Locking and visibility

Each data file has a persistent sibling named `.keepbook-lock-<filename>.lock`.
Cooperating writers acquire this lock before reading or mutating the data file.
The lock covers the entire read/modify/write operation, including compaction and
metadata backfills. Never delete a sidecar while readers or writers may be active:
recreating it would let different processes lock different inodes for the same
logical file.

The implementation uses Rust's [standard file locks](https://doc.rust-lang.org/std/fs/struct.File.html#method.lock),
available since Rust 1.89. These coordinate distinct handles and processes. No
in-memory mutex or per-instance cache is relied on for write exclusion.

JSONL appends additionally lock the open data file exclusively. JSONL readers take
a shared lock on their open data file. Readers therefore see complete append
batches. A reader that opened a file before an atomic replacement may read the
previous complete version. Reads do not create sidecars or require a writable
data directory.

Locks and I/O run in one `spawn_blocking` closure per operation. Once that closure
starts, cancelling the async caller does not release its lock or stop its write.
The caller cannot infer whether a cancelled operation committed; retries should
use the existing deduplication rules. Waiting for file locks does not block a
Tokio runtime worker.

## Writes and recovery

Replacements are written to a temporary sibling file, preserving existing file
permissions. The temporary file is synced before it is atomically persisted over
the target. On Unix, the containing directory is then synced. Errors before the
replacement leave the original file unchanged; normal error paths remove the
temporary file. A directory-sync error after replacement can report failure even
though the new file is already visible.

Appends serialize the whole batch before modifying a file, then append and sync
under the locks. A complete existing record without a trailing newline is
separated from the new records. If an append reports an I/O failure, it attempts
to truncate back to the previous length and sync; rollback failures are reported.
This keeps ordinary appends proportional to the new batch rather than copying
the entire existing history.

The market-data write path reads the current file under its writer lock, bypassing
cached observations. After mutation it invalidates cached values instead of
labelling a locally constructed vector with filesystem metadata that another
writer may already have changed.

Keepbook's Git operations add the reserved patterns to the data repository's
local `info/exclude`, preserving existing rules and avoiding duplicate entries.
Automatic staging and clean-worktree checks also filter these files. Nothing is
added to the tracked `.gitignore`. Repositories used only with manual Git can
ignore the files using these patterns:

```gitignore
.keepbook-lock-*
.keepbook-tmp-*
```

## Limits

- Locking coordinates cooperating Keepbook file operations. Older binaries,
  external scripts, editors, Git checkout/pull, and concurrent account/connection
  deletion do not participate. Do not run these mutations concurrently with
  storage writes.
- This is per-file consistency, not a transaction spanning a whole sync or a
  consistent snapshot across all files. Application-level read/modify/save
  sequences still need their own concurrency semantics.
- A process crash during an append can leave an incomplete final batch. Existing
  parsers report malformed JSONL rather than silently discarding data. Atomic
  batch recovery after crashes needs a journal or a different storage design.
- File/directory syncing is subject to filesystem and platform support. Newly
  created ancestor directories are not all explicitly synced. Power-loss
  durability and Windows behavior were not fault-tested in this change.
- Sidecars persist; interrupted processes may leave temporary siblings. Do not
  remove live lock files as routine cleanup. Temporary files from a stopped
  process can be inspected and removed separately.
- Lexical path validation does not provide containment against pre-existing
  symlinks, and atomic replacement does not preserve arbitrary extended metadata
  such as ACLs or xattrs.

## Tests

`tests/storage_concurrency.rs` covers shared and independent store instances,
multiple OS processes, concurrent reads, price/FX/registry writes, balance and
transaction appends, compaction, and metadata backfills. Unit tests cover
cancellation while locked, failed replacement/transform/serialization,
permissions, trailing newlines, and exclusion of internal files from Git.
