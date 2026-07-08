# Archive And Compression Unification

This note records the current archive and compression implementation shape and
the remaining archive-specific follow-up work.

The completed direction is: keep the XSH-visible archive APIs synchronous,
centralize archive compression policy in XSH, and use async-shaped archive
crates only as focused parser/writer helpers. There is no Tokio archive island
anymore. Tokio is not part of XSH's resolved dependency tree.

## Goals

- Keep public `archive.*` APIs synchronous and stable.
- Preserve the current `archive.*` API contract and error kinds.
- Keep XSH's archive extraction policy as the source of truth.
- Keep codec policy in XSH instead of delegating format selection, extension
  inference, magic detection, or error kinds to generic adapter crates.
- Reuse the shared compression layer for archive APIs and Linux module metadata
  reads where practical.
- Avoid introducing an async task runtime into the evaluator, process runtime,
  checker, or public module dispatch.
- Prefer the simplest dependency surface that preserves intended archive
  behavior.

## Current Dependency Shape

The archive dependency tree is intentionally not Tokio-shaped:

- Tar uses the local `astral-futures-tar` port with `default-features = false`
  and `features = ["futures"]`.
- The local tar crate's plain `futures` feature is trait-only. Its path-based
  async filesystem helpers are behind `futures-fs`, which XSH does not enable.
- ZIP uses `astral_async_zip` with `default-features = false` and
  `features = ["deflate"]`.
- `tokio`, `tokio-stream`, `astral-tokio-tar`, and the old `tar` crate are not
  in the resolved dependency graph.
- Compression dependencies are:
  - `flate2` for gzip;
  - `bzip2` for bzip2;
  - `lzma-rust2` for xz/lzma;
  - `async-compression` only as a transitive ZIP-entry decoder through
    `astral_async_zip`.

`astral_async_zip`'s compression features control only compression methods used
inside `.zip` entries. They do not affect XSH's standalone
`archive.compress`, `archive.decompress`, `archive.decompress_bytes`, or
compressed tar support.

## Current Implementation Shape

Archive and compression are organized around three internal layers:

1. Synchronous XSH module entrypoints.
2. A small private `futures_lite::future::block_on` boundary in
   `src/modules/archive/mod.rs`.
3. Archive/codec implementation code using XSH-owned blocking-to-async adapters
   where async traits are required by tar or ZIP crates.

The archive future boundary is not a task runtime. It drives futures that are
backed mostly by blocking filesystem and codec operations. Callers in
`runtime/eval` and standard module dispatch keep seeing ordinary
`Result<T, RuntimeError>` functions.

Do not introduce async into:

- `src/runtime/eval.rs`
- standard module dispatch signatures
- `Value`
- process orchestration
- checker APIs

## Blocking I/O And Async-Shaped APIs

XSH currently uses async-shaped tar and ZIP APIs without making archive I/O
truly non-blocking.

For tar:

- `archive_reader(...)` returns `Box<dyn Read>`.
- `ArchiveWriter` implements `Write`.
- `BlockingAsyncIo<T>` adapts those blocking readers and writers to
  `futures_lite::io::AsyncRead` and `AsyncWrite`.
- `block_on_archive(...)` drives the async tar calls to completion.

This is intentional. Archive operations are focused host APIs in XSH's runtime
model, not fine-grained concurrent application workloads. Making tar or ZIP
"maximally async" would also require async filesystem I/O, async compression
codecs, scheduling, cancellation policy, and a real async runtime. That would
add complexity without a clear scripting-level benefit and would fight the
boundary described in `docs/CHAPTER-15-why-not-xsh.md`.

The dependency opportunity has already been narrowed: XSH does not use the tar
crate's path-opening helpers, and the local `astral-futures-tar` futures backend
no longer pulls in `async-fs`, `blocking`, or `async-channel`.

### What Async Tar Crates Buy

Async tar crates are mostly about composition with an async I/O graph, not
about making local disk tar operations automatically faster.

`astral-tokio-tar` is useful when an application is already Tokio-shaped:

- tar readers and writers accept Tokio `AsyncRead` and `AsyncWrite`;
- tar streams compose with Tokio sockets, subprocess pipes, HTTP bodies, and
  async compression layers;
- callers can stay inside Tokio tasks instead of blocking reactor threads;
- path helpers expose Tokio-flavored filesystem APIs.

That is a good shape for Tokio services, package downloaders, and other tools
whose surrounding runtime is already async. It was a poor fit for XSH because
archive commands are synchronous host operations at the language boundary, and
XSH already owns archive path validation and extraction policy.

`astral-futures-tar` keeps the useful part of that design without committing
XSH to Tokio. It lets tar parsing and writing work over generic futures
`AsyncRead`/`AsyncWrite` values, so XSH can use `futures-lite` and a small local
`block_on` boundary. In the current XSH integration, local files are still
opened and written with `std::fs`; `BlockingAsyncIo` only adapts those blocking
objects to the async traits the tar crate expects.

Do not assume `async-fs` means native kernel async file I/O. On many platforms,
Rust async filesystem crates expose an async facade over blocking filesystem
operations scheduled on a thread pool. That can be the right abstraction inside
an async application, but it is not automatically faster than XSH's existing
synchronous and threaded filesystem code. True kernel async file I/O would be a
separate backend decision, such as Linux-specific `io_uring`, with different
portability and policy tradeoffs.

## Tar

Tar uses `astral-futures-tar`.

The crate package is `astral-futures-tar`; the Rust crate name is
`astral_futures_tar`.

XSH uses it for:

- tar entry parsing
- tar entry metadata
- tar writing through async reader/writer traits
- long paths and PAX behavior
- hardlink and symlink entry handling

XSH does not use `Archive::unpack` as the public extraction implementation, and
does not use the tar crate's path-based filesystem helpers for archive
creation. Creation walks the source tree with XSH-owned `std::fs` policy,
streams regular files through `BlockingAsyncIo`, and calls low-level tar writer
methods for records, paths, and link targets.

XSH already has stricter extraction behavior than generic tar unpack helpers:

- member filtering
- `strip_components`
- explicit `overwrite`
- rejection of absolute paths and parent-directory paths
- rejection of symlink ancestor escapes
- stable XSH error kinds such as `archive-path`, `archive-escape`, and
  `archive-extract`

This policy stays in XSH code and receives data from `astral_futures_tar`
entries.

### Tar Extraction Boundary

The extraction boundary is already tightened in the important sense: untarring
means parsing tar headers and streaming entry bytes through async traits, while
filesystem writes and security policy remain in XSH. That keeps the fast,
streaming tar decode path without handing extraction policy to a generic
`Archive::unpack`.

Making the actual file writes "fully async" is not a free speedup for XSH. The
current implementation is a synchronous host operation driven by a local
`block_on` boundary. A truly nonblocking extractor would require an async
filesystem backend, async compression all the way down, scheduler/cancellation
policy, and likely concurrent extraction rules. It would also reintroduce the
same dependency weight this work removed unless we wrote or adopted a small
runtime-independent filesystem layer.

The useful redesign target is therefore narrower:

- Keep tar record parsing and payload copy async-trait based.
- Keep path validation, overwrite behavior, symlink rules, and error kinds in
  XSH.
- Avoid tar crate filesystem helpers in the default build.
- Consider concurrent or nonblocking extraction only if profiling shows archive
  extraction is a user-visible bottleneck and we can preserve deterministic
  error behavior.

For now, the material win is lower dependency weight without regressing archive
behavior. `tools/archive-fat-trim-demo.xsh` demonstrates that `tokio`,
`async-fs`, `blocking`, and `async-channel` are absent while tar create/list/
extract still round trips an executable and a symlink.

## Compression

Archive compression policy lives in `src/modules/compression.rs`.

Supported standalone compression formats are:

- gzip
- bzip2
- xz
- lzma

The active backend choices are:

- `flate2` with the `zlib-rs` backend for gzip.
- `bzip2` with the Trifecta Rust backend through `libbz2-rs-sys`.
- `lzma-rust2` with `std`, `encoder`, `xz`, and `optimization`.

Do not add `async-compression` as a direct archive compression layer unless it
also preserves XSH-owned format parsing, extension inference, magic detection,
level validation, and stable error kinds.

Codec compatibility means decompressed content and public behavior stay
compatible. Compressed output is not byte-identical across backends, levels,
headers, or block layouts, and tests should not require byte-identical
archives.

The implementation should preserve existing format detection and extension
behavior:

- explicit `compression` argument wins;
- `auto` detects by magic bytes where supported;
- create paths infer compression from output extension;
- unsupported compression stays `archive-compression`.

zstd is not part of the standalone archive compression API today. Add it only
when the dependency and API policy are explicit.

## Zip

ZIP uses Astral's `astral_async_zip` crate through its `async_zip` crate name.

XSH enables only ZIP deflate support. Stored entries need no compression
feature. This keeps ordinary ZIP listing and extraction working while avoiding
extra ZIP-entry codec dependencies for bzip2, LZMA, zstd, xz, and deflate64.

ZIP still has a different shape from tar because of central-directory and seek
behavior. XSH reads the ZIP file into memory and uses
`async_zip::base::read::mem::ZipFileReader`, then validates the full extraction
plan with its own archive path policy before writing files.

Extraction is currently sequential. It writes output files through blocking
`std::fs` APIs after `astral_async_zip` decodes each entry through futures I/O
traits.

This migration does not add `archive.zip_create`. Tests create ZIP fixtures
with `/usr/bin/zip` and assert that it is Info-ZIP 3.0, avoiding dependency on
the same Rust ZIP reader used by production code.

## Linux Module Decompression

Linux module metadata reads use `src/modules/compression.rs` through
`linux_module_reader` for `.ko.gz`, `.ko.xz`, and `.ko.bz2`. This keeps Linux's
module extension policy while avoiding duplicate gzip/xz/bzip2 handling in
`src/modules/linux/kernel.rs`.

## Completed Migration Stages

### Stage 1: Coverage

Regression coverage was added before replacing the original tar implementation.

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

### Stage 2: Shared Archive Codec Layer

Archive compression policy moved to `src/modules/compression.rs`.

The public functions remain:

- `archive.compress`
- `archive.decompress`
- `archive.decompress_bytes`
- tar create/list/extract compression handling

This stage kept:

- individual codec crates instead of a generic direct `async-compression`
  layer;
- synchronous public `archive.*` APIs;
- XSH-owned format parsing, extension inference, magic detection, level
  validation, reader creation, and writer creation;
- existing archive error kinds and path-safety policy.

The codec internals may still block. True non-blocking codecs remain future
work only if the language runtime model changes enough to justify them.

### Stage 3: Linux Module Codec Reuse

Linux module metadata reads now use the shared codec layer for `.ko.gz`,
`.ko.xz`, and `.ko.bz2`.

This removes duplicate gzip/xz/bzip2 handling from `src/modules/linux/kernel.rs`
while preserving Linux's module extension policy.

### Stage 4: Futures-Backed Tar

Tar moved to `astral-futures-tar` with the futures backend.

This stage kept:

- synchronous `archive.*` Rust functions;
- synchronous XSH-visible behavior;
- existing error kinds;
- existing path policy;
- existing compression behavior;
- no Tokio dependency.

XSH now uses the tar writer's lower-level async trait methods instead of its
path-based filesystem helpers, so the local `astral-futures-tar` crate can keep
`async-fs` behind the opt-in `futures-fs` feature.

### Stage 5: ZIP Migration And Feature Slimming

ZIP moved to `astral_async_zip`'s base futures API.

The dependency is configured without `full`, `tokio`, or `tokio-fs`, and with
only `deflate` enabled. Public `archive.zip_list` and `archive.zip_extract`
signatures are unchanged.

## Verification Gates

For code changes, use the narrowest gate first, then the full relevant gate:

- `cargo test --test runtime archive`
- `cargo test --test runtime module`
- `cargo build`
- `cargo test` for broader dependency or network/archive runtime changes

Do not build release binaries for this migration.

For closeout-note-only edits, no Rust tests are required. Check for stale
archive note references with:

```sh
rg 'tokio|tokio_tar|astral-tokio-tar|ZipFileReader::new\\(path|tokio::read|JoinSet|features = \\["full"\\]|name = "tar"' ARCHIVE-UNIFICATION.md
```

## Future Work

- Keep the shared codec layer in `src/modules/compression.rs` unless
  compression policy grows beyond archive and Linux module users.
- Revisit true non-blocking archive codecs only if XSH gains a concrete host
  API requirement that justifies an async runtime boundary.
- Add non-deflate ZIP compression methods only when XSH intentionally supports
  those ZIP entry formats.
