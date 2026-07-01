# Archive And Compression Unification

This note records the completed archive and compression I/O migration and the
remaining archive-specific follow-up work.

The chosen direction is Option B: keep archive/compression implementation
details behind a small private Tokio boundary, and use that boundary as the
start of a shared XSH-owned compression layer. The tar migration is complete:
XSH now uses `astral-tokio-tar` instead of the old `tar` crate. The XSH
evaluator and public module APIs remain synchronous.

## Goals

- Keep tar on `astral-tokio-tar`.
- Keep async contained inside archive/compression implementation modules.
- Avoid spreading Tokio through the evaluator, process runtime, checker, or
  public module dispatch.
- Preserve the current `archive.*` API contract and error kinds.
- Keep XSH's archive extraction policy as the source of truth.
- Converge archive, file compression, and Linux module decompression on one
  implementation path where practical.
- Keep codec policy in XSH instead of delegating format selection, extension
  inference, or error kinds to a generic adapter crate.
- Prefer Tokio and Rustix-backed infrastructure for file I/O over ad hoc direct
  syscall or blocking-std code when a module is being migrated.

## Current Dependency Shape

After the archive migration:

- The normal dependency tree includes Tokio through the private archive
  implementation.
- `rustix` is already a direct dependency and also comes through `tempfile`.
- The old `tar` crate is no longer in the tree.
- `astral-tokio-tar` pulls `filetime`.
- Archive and compression dependencies are currently split across:
  - `astral-tokio-tar`
  - `astral_async_zip`
  - `flate2`
  - `bzip2`
  - `lzma-rust2`
- `async-compression`, `zstd`, and related codec crates are transitive state
  through `astral_async_zip`; they are not XSH's direct archive compression
  policy layer.
- Compression policy is centralized in `src/modules/compression.rs` for archive
  APIs and Linux module metadata reads.

Stage 3 centralized archive codec policy, and Stage 4 moved Linux module
metadata reads onto the same layer. The underlying direct codec crates are still
synchronous; true non-blocking codecs remain future work.

## Target Shape

Archive and compression should move toward three internal layers:

1. Synchronous XSH module entrypoints.
2. A private async boundary that owns the Tokio runtime handle.
3. Archive/codec implementation code with XSH-owned sync-to-async adapters
   where blocking codec crates are still used.

The runtime boundary should be small and boring. Callers in `runtime/eval` and
standard module dispatch should keep seeing ordinary `Result<T, RuntimeError>`
functions. Tokio should be an implementation detail of the archive/compression
module, not a property of the language runtime.

## Async Boundary

Use a private Tokio runtime for archive/compression work.

The boundary exists in `src/modules/archive/mod.rs` as a local
`block_on_archive(...)` helper. The blocking-to-async I/O adapter now lives in
`src/modules/compression.rs` with the codec policy it serves.

If more modules adopt the same pattern, promote the boundary to a shared host
I/O helper.

Do not introduce async into:

- `src/runtime/eval.rs`
- standard module dispatch signatures
- `Value`
- process orchestration
- checker APIs

## Tar

Tar has been migrated from the old `tar` crate to `astral-tokio-tar`.

The crate package is `astral-tokio-tar`; the Rust crate name is `tokio_tar`.

XSH uses it for:

- tar entry parsing
- tar entry metadata
- tar writing
- long paths and PAX behavior
- hardlink and symlink entry handling

XSH does not use `Archive::unpack` as the public extraction implementation.

XSH already has stricter extraction behavior than generic tar unpack helpers:

- member filtering
- `strip_components`
- explicit `overwrite`
- rejection of absolute paths and parent-directory paths
- rejection of symlink ancestor escapes
- stable XSH error kinds such as `archive-path`, `archive-escape`, and
  `archive-extract`

This policy stays in XSH code and receives data from `tokio_tar` entries.

## Compression

Stage 3 uses individual codec crates and keeps XSH responsible for format
policy:

- gzip
- bzip2
- xz
- lzma

Do not add `async-compression` as a direct archive compression layer for this
stage. It does not remove the need for XSH-owned format parsing, extension
inference, magic detection, level validation, or stable error kinds.

The active backend choices are:

- `flate2` with the `zlib-rs` backend for gzip.
- `bzip2` with the Trifecta Rust backend through `libbz2-rs-sys`.
- `lzma-rust2` with `std`, `encoder`, `xz`, and `optimization`.

Codec compatibility means decompressed content and public behavior stay
compatible. Compressed output is not byte-identical across backends, levels,
headers, or block layouts and tests should not require byte-identical archives.

The tar adoption created the first async boundary and routes tar I/O through
async traits. Stage 3 centralizes the codec layer behind that boundary, but it
does not make synchronous codec crates magically non-blocking.

The migration should preserve existing format detection and extension behavior:

- explicit `compression` argument wins
- `auto` detects by magic bytes where supported
- create paths infer compression from output extension
- unsupported compression stays `archive-compression`

zstd is deferred. It is the preferred future target once `libzstd-rs-sys`
matures enough for XSH's dependency policy.

## Zip

ZIP now uses Astral's `astral_async_zip` crate through its `async_zip` crate
name. XSH intentionally enables the crate's `full` feature so ZIP listing and
extraction can read Stored, Deflate, bzip2, LZMA, zstd, xz, and deflate64
entries where the crate supports them.

ZIP still has a different shape from tar because of central-directory and seek
behavior. The implementation uses `async_zip::tokio::read::fs::ZipFileReader`
so extraction can open independent entry readers over the same filesystem path.
XSH validates the full extraction plan with its own archive path policy before
spawning bounded file extraction work. The private concurrency limit is four
regular file entries, with no public tuning knob yet.

This migration does not add `archive.zip_create`. Tests create ZIP fixtures with
`/usr/bin/zip` and assert that it is Info-ZIP 3.0, avoiding dependency on the
same Rust ZIP reader used by production code.

## Linux Module Decompression

Linux module metadata reads now use `src/modules/compression.rs` through
`linux_module_reader` for `.ko.gz`, `.ko.xz`, and `.ko.bz2`. This keeps Linux's
module extension policy while avoiding duplicate gzip/xz/bzip2 handling in
`src/modules/linux/kernel.rs`.

## Migration Stages

### Completed Stage 1: Coverage

Regression coverage was added before replacing the tar crate.

Coverage includes:

- tar create/list/extract round trips
- compressed tar round trips
- archive entry record fields
- file mode preservation
- symlink listing and extraction
- hardlink extraction
- member prefix filtering
- overwrite behavior
- `strip_components`
- path traversal rejection
- symlink escape rejection
- wrapper coverage for `core/tar.xsh`

### Completed Stage 2: Tokio Tar Island

The first private async boundary exists, and tar now uses `astral-tokio-tar`.

This stage kept:

- synchronous `archive.*` Rust functions
- synchronous XSH-visible behavior
- existing error kinds
- existing path policy
- existing compression behavior

The old `tar` crate is gone from the tree. Tokio enters through the private
archive tar island. `filetime` remains archive-related transitive state through
`astral-tokio-tar`.

### Completed Stage 3: Archive Codec Layer

Archive compression policy now lives in `src/modules/compression.rs`.

This stage moved archive compression and decompression through that layer first.

The public functions remain:

- `archive.compress`
- `archive.decompress`
- `archive.decompress_bytes`
- tar create/list/extract compression handling

This stage kept:

- individual codec crates instead of `async-compression`
- synchronous public `archive.*` APIs
- XSH-owned format parsing, extension inference, magic detection, level
  validation, reader creation, and writer creation
- existing archive error kinds and path-safety policy
- no zstd dependency

The codec internals may still block. True non-blocking codecs remain future
work.

### Completed Stage 4: Linux Module Codec Reuse

Linux module metadata reads now use the shared codec layer for `.ko.gz`,
`.ko.xz`, and `.ko.bz2`.

This removes duplicate gzip/xz/bzip2 handling from `src/modules/linux/kernel.rs`
while preserving Linux's module extension policy.

### Completed Stage 5: ZIP Migration

ZIP has migrated to Astral async ZIP. The `full` feature is intentionally
enabled for broad read compatibility, and extraction uses bounded parallelism
behind the private archive Tokio runtime. Public `archive.zip_list` and
`archive.zip_extract` signatures are unchanged.

## Verification Gates

For code changes, use the narrowest gate first, then the full relevant gate:

- `cargo test --test runtime archive_module`
- `cargo test --test runtime`
- `cargo test --test core`

If docs or examples change:

- `make docs`

Do not build release binaries for this migration.

For closeout-note-only edits, no Rust tests are required. Check for stale
archive note references with:

```sh
rg 'TIME-J[I]FF|J[i]ff|libc::str[f]time|current-[t]hread|src/modules/archive[.]rs|  - [`]zip[`]|name = "z[i]p"' ARCHIVE-UNIFICATION.md
```

## Future Work

- Consider promoting the archive-local Tokio boundary to a shared host I/O
  helper only if another module adopts the same pattern.
- Keep the shared codec layer in `src/modules/compression.rs` unless compression
  policy grows beyond archive and Linux module users.
- Revisit true non-blocking codecs and zstd only when mature dependency options
  satisfy XSH's dependency policy.
