# XSH-57: One entry model, one compression lifecycle, one extraction boundary

Status: accepted by the owner for implementation

## Summary

Archive and compression support is rebuilt as a reusable workspace crate,
`crates/xsh-archive`, with no XSH `Value`, span or evaluator state. It has three
layers: compression (bytes to bytes), archive formats (bytes to ordered entries)
and a filesystem adapter built on `crates/xsh-root`'s retained root. A shared
driver owns iteration, body consumption, integrity checks, one resource budget
and explicit completion levels. A thin adapter in `src/modules/archive` converts
XSH values and errors.

The nine format-named functions (`tar_list`, `tar_extract`, `tar_create`,
`zip_list`, `zip_extract`, `cpio_list`, `cpio_extract`, `cpio_create`) become
three format-parameterized functions, `archive.list`, `archive.extract` and
`archive.create`. Format, compression and every other closed option set become
real enums. Every caller migrates in the same campaign and the old functions are
deleted with no compatibility adapters.

TAR and CPIO listing and extraction accept sequential byte streams. ZIP needs
random access and rejects a non-seekable input with a specific error; nothing
spools silently.

This RFC records the verified baseline, the public signatures with the enum
mechanism they need, the error model, the policy object, the stream and seek
rules, the ordered increments and the test plan.

## Baseline

Revisions: XSH `7544cb16d`, Laputa `8607ab1`. Probes ran on the baseline
`xsh` build, in scratch under `.work/`.

Current code, about 2,100 lines: `src/modules/archive/{mod,policy,tar,zip,cpio}.rs`
and `src/modules/compression.rs`, plus an evaluator-specialized expression row,
`ExprArchiveTarCreate`, in `src/runtime/eval/` (lowering, the indexed schema,
`indexed_run.rs`, `module_archive.rs`).

Public functions in module `archive`: `tar_list`, `tar_extract`, `tar_create`,
`zip_list`, `zip_extract`, `cpio_list`, `cpio_extract`, `cpio_create`,
`compress`, `decompress`, `decompress_bytes`. Module `compression` has
`transform` and `gzip_name`, used by the `gzip`, `xz` and `bzip2` applets
(`core/lib/compress.xsh`) and `core/curl.xsh`. Every parameter that names a
format or option set is a string (`"auto"`, `"gz"`, `"xz"`) or a boolean
(`overwrite`).

Defects, each verified by source reading and, where marked, by probe on the
baseline build:

1. ZIP listing reports `modified: 0` for every entry (`zip.rs:245`). Probe: a
   ZIP entry stamped 2023-11-14 lists `modified: 0`.
2. TAR link names go through `to_string_lossy` (`tar.rs:456`), so a non-UTF-8
   link target is corrupted in the listing. `link_name` is `""` when absent, so
   absent and empty are the same value.
3. Compression has two completion paths: `transform` (`encode`) and
   `ArchiveWriter::finish`. They configure encoders separately, so `tar_create`
   cannot take a level and the two paths already differ (gzip level 0 to 9 is
   validated in `level()`, Zstandard in `transform_io`, bzip2 only in `encode`).
4. TAR listing buffers every record to avoid a self-referential borrow between
   `tar::Archive` and `tar::Entries` (`tar.rs:40`), so listing is not lazy and
   its memory is unbounded by archive size.
5. Each format applies entries to the filesystem itself, with check-then-join
   paths (`policy.rs` `validate_archive_ancestors`, then `dest.join`). A symlink
   swapped in between the check and the open is followed. ZIP extraction also
   plans every file before extracting any.
6. A missing end-of-archive trailer is accepted. Probe: a 1,024-byte TAR holding
   one header and one data block extracts without error.
7. Outer compression is never verified to completion. Probe: a `.tar.gz` with a
   corrupted gzip CRC32 and a `.tar.gz` with its gzip trailer cut off both
   extract without error, because the TAR reader stops at its end blocks before
   the decoder reaches the trailer.
8. No resource budget. A decompression bomb extracts until the disk fills;
   `cpio` allocates `namesize` and symlink targets from untrusted lengths
   (`vec![0; entry.size as usize]`).
9. Every extracted file's mode is applied verbatim, including setuid, setgid and
   sticky bits, and device, FIFO and hardlink entries are handled differently by
   each format (TAR creates hardlinks, CPIO ignores them, ZIP skips
   non-file entries).
10. CPIO records no `link_name` and TAR/CPIO extraction of non-regular kinds is
    silent (`_ => skip`): unsupported entries are dropped without a report.
11. Inputs are paths only. No format reads a pipe or an XSH byte stream.

Evidence that the pieces exist and compose:

- `tar` (0.4.46) parses headers, GNU long names and PAX records and verifies
  header checksums. `Entry::path_bytes` and `link_name_bytes` give raw bytes.
  `Archive` over `&mut R` reads straight through, so one `Archive` can be built
  per header over a shared reader without buffering.
- `rawzip` 0.5.1 gives the central directory and `ZipEntry::reader()` /
  `verifying_reader()` return readers that do not borrow the temporary
  `ZipEntry`, so a body can outlive the call that opened it. The verifier checks
  CRC32 and size at end of data. `ZipFileHeaderRecord::last_modified()` exists.
- `crates/xsh-root` opens files and directories beneath a retained descriptor
  with `openat2(RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS)` (Linux) and
  `O_RESOLVE_BENEATH` (macOS). It has no mutation operations.
- A built-in nominal enum is cheap in the existing checker. Probe: with a
  registry `Type::Enum(name)` converted to the sema `Type::Tag(name)`, and the
  variants seeded into `tag_variants` and the compact declarations the same way
  built-in error families are, `archive.tar_list(path, compression: .Gzip)`
  type-checks, lowers and delivers a `Value::Tag` to the host operation (about
  40 lines across `crates/xsh-registry`, `src/modules/signature.rs`,
  `src/sema/check.rs` and `checked_declarations.rs`). Today the registry has no
  enum type: only `ErrorFamily(&'static str)`.

Tool availability. The pinned `Dockerfile.test` (Alpine) has BusyBox `tar`,
`cpio` and `unzip`, `zip` and Python 3 with `tarfile`/`zipfile`; it has no GNU
tar and no `bsdtar`. The development host has BusyBox `tar`, `unzip`, `zip` and
`cpio`. Interoperability tests therefore use BusyBox and Python's `tarfile` and
`zipfile` as external writers and readers, and skip with a named reason when a
tool is absent. GNU tar and bsdtar cover is not claimed.

Callers to migrate (found with `rg`):

- XSH: `core/tar.xsh` (the `tar` applet: `decompress_bytes`, `compress`),
  `src/runner.rs` embedded program, `docs/snippets/{tour,api}/*`,
  `docs/templates/user-tour.md`, `examples/release-package.xsh`,
  `showcase/{release-pack,archive-unpack}.xsh` and
  `showcase/tests/test-archive-unpack.xsh`, `tools/archive-fat-trim-demo.xsh`,
  `dev/release.xsh`, `dev/tests/test-targets.xsh`, `core/cpio.xsh` and
  `core/tests/test-{cpio,tar,bb-tar,compress}.xsh`, and the native tests
  `tests/xsh/stdlib/archive.xsh`, `tests/xsh/{named-argument-spreading,
  lowering-coverage,expression-route-oracles,fs-path-frame-oracles,api-tool}.xsh`.
- Laputa: `pm/sources.xsh`, `pm/root.xsh`, `pm/execute.xsh`, `pm/build.xsh`,
  `pm/generation_adapter.xsh`, `installer/setup-laputa.xsh`,
  `build-installer-image.xsh`, `seed/xsh_seed.xsh`,
  `packages/{amd-ucode,mandoc,xsh}/*`, `packages/linux/kbuild.xsh`
  (`archive.compress`), and tests under `tests/pm/` (`pm_build`, `pm_root`,
  `pm_cli`, `pm_generation`, `pm_maintainer`, `pm_sources`, `linux_recipe`,
  `xsh_recipe`, `fixtures/root-published-runner-metadata`).

## Design

### Public API

All closed option sets are built-in nominal enums declared in module `archive`.
A variant is written target-typed, `.Tar`, where the parameter (or the other
operand of `==`) is the enum; this is the SPEC's existing target-typed variant
rule (5.5), verified against `docs/SPEC.md` and by probe above. A built-in
enum's variants are not bare names or module members, so two enums may share a
variant name; a `match` over a built-in enum value is not supported, and values
are compared with `==`. A type annotation spells the enum `archive.Format`
(open question 3).

| Enum | Variants |
|---|---|
| `archive.Format` | `Tar`, `Zip`, `Cpio` |
| `archive.Compression` | `Auto`, `Plain`, `Gzip`, `Bzip2`, `Xz`, `Lzma`, `Zstd` |
| `archive.Collision` | `Refuse`, `Replace` |
| `archive.Links` | `Refuse`, `Contained`, `Verbatim` |
| `archive.Special` | `Refuse`, `Create` |
| `archive.Modes` | `Plain`, `Exact` |
| `archive.Ownership` | `Ignore`, `Apply` |
| `archive.EntryKind` | `File`, `Directory`, `Symlink`, `Hardlink`, `Fifo`, `CharDevice`, `BlockDevice`, `Other` |

`Compression.Auto` detects by magic number when reading and by the destination
suffix when writing; `Plain` is no compression. `Format` has no `Auto`:
the archive format is the caller's decision (Laputa chooses by suffix).

Signatures (parameter labels are free, so positional and named calls both work):

```
archive.list(source: Path | Bytes | Stream[Bytes], format: archive.Format,
             compression: archive.Compression = .Auto, members: List[Path] = [])
    -> Result[Stream[ArchiveEntry], ArchiveError]

archive.extract(source: Path | Bytes | Stream[Bytes], dest: Path, format: archive.Format,
                compression: archive.Compression = .Auto, members: List[Path] = [],
                strip_components: UInt = 0,
                collision: archive.Collision = .Refuse,
                links: archive.Links = .Contained,
                special: archive.Special = .Refuse,
                modes: archive.Modes = .Plain,
                ownership: archive.Ownership = .Ignore,
                max_output: UInt? = null, max_entry: UInt? = null, max_entries: UInt? = null)
    -> Result[Unit, ArchiveError]

archive.create(dest: Path, root: Path, entries: List[Path], format: archive.Format,
               compression: archive.Compression = .Auto, level: Int? = null,
               collision: archive.Collision = .Refuse)
    -> Result[Unit, ArchiveError]
```

`archive.create` accepts `.Tar` and `.Cpio`; `.Zip` creation is not implemented
and fails with `ArchiveError.Unsupported` (no migrated caller needs it; the
writer interface does not preclude it). `compress`, `decompress` and
`decompress_bytes` stay as single-file operations and retype their `format`
parameter to `archive.Compression` and `level` to `Int?`; `decompress_bytes`
does not fold into the archive operations (it has no entries), so it stays and
shares the compression layer. `compression.transform` and `compression.gzip_name`
keep their signatures (applet-facing, comma-list detection) but run on the same
codec layer and lifecycle. `max_*` and the omitted limits default to the
`Limits::default()` below.

`ArchiveEntry` is a breaking record change:

```
{ path: Path, kind: archive.EntryKind, size: UInt?, mode: UInt?, modified: Int?,
  uid: UInt?, gid: UInt?, link_name: Path?, device_major: UInt?, device_minor: UInt? }
```

`null` means unknown or not present in this format; it is never zero. `path` and
`link_name` carry the raw archive bytes. `modified` is seconds since the epoch;
ZIP reports it only when the entry carries a UTC timestamp, otherwise `null`
(a DOS local time has no zone). `kind` is `Other` for an entry the format could not classify. The previous `kind: Str` ("file", "dir", "symlink", "hardlink",
"other") and `link_name: Str` change. Absent `size` occurs for formats and entry
kinds that do not record one.

Rust-side representation. In the crate each enum is `#[repr(u8)]`, `Copy`, with a
total conversion from the registry variant name and a static name table; options
travel as these enums and never as strings. At the language level a payload-free
variant is the existing `Value::Tag { type_name, name, .. }`; compacting that
value for every enum is a runtime-wide change this campaign does not make
(open question 2).

### Error model

One built-in error family, `ArchiveError`, replaces the `archive-*` kind strings.
Fields: `message: Str`, `format: archive.Format?`, `entry: Path?`, `offset: UInt?`.
Variants and facets:

| Variant | Facets | Meaning |
|---|---|---|
| `Open` | host I/O facet of the OS error | the source or destination could not be opened or created |
| `Io` | host I/O facet | a read, write or filesystem call failed after opening |
| `Corrupt` | `InvalidData` | malformed header, bad checksum, bad trailer or trailing data |
| `Truncated` | `InvalidData` | input ended early, including a missing archive trailer or compression trailer |
| `Unsupported` | none | a feature, method or combination the crate does not implement (`Zip` with a compression, sparse TAR, encrypted ZIP) |
| `NeedsSeek` | none | the format needs random access and the input is a pipe or stream |
| `Unsafe` | `InvalidData` | a path, link or entry kind the policy forbids; `entry` names it |
| `Collision` | none | a destination exists under `Refuse`, or a member name repeats |
| `Limit` | `CaptureLimit` | the shared budget was exceeded; `message` names the limit |
| `Canceled` | `Canceled` | cancellation was observed; no partial output remains |
| `Options` | none | an invalid level, member filter or argument combination |

Errors keep the layer: a `Corrupt` from a compression layer carries
`format: null` and the codec in `message`; one from a container carries the
format. A failing finish of an encoder is `Io` (or `Truncated` on read). Cleanup
failures after a primary error are appended to the primary error's `message`
(`cleanup failed: ...`), never replace it, and never turn a failure into
success. An absent file on `Open` carries the `NotFound` facet, so existing
`is NotFound` patterns still match.

### The crate

`crates/xsh-archive`, a workspace member following `xsh-net`. Dependencies are
those already in the tree (`tar`, `rawzip`, `flate2`, `bzip2`, `lzma-rust2`,
`zstd`, `rustix`, `xsh-root`); no new crate enters `Cargo.lock`. It exposes:

- `compression`: `Codec` (`Gzip`, `Bzip2`, `Xz`, `Lzma`, `Zstd`), `Encode` options
  with a per-algorithm constructor that validates the level once
  (`Gzip` 0 to 9, `Bzip2` 1 to 9, `Xz` and `Lzma` presets 0 to 9, `Zstd`
  -131072 to 22); no universal level. `Decoder` wraps a `BufRead` with bounded
  buffers and the workspace limit (LZMA dictionary memory, Zstandard window).
  `Encoder<W>` has `finish(self) -> Result<W>`, the only completion path;
  dropping an unfinished encoder cannot publish because publication is
  downstream of `finish`. Raw DEFLATE for ZIP is a distinct type from gzip
  framing.
- `entry`: `EntryKind` (`#[repr(u8)]`) and `EntryMeta { name: Vec<u8>, kind,
  size: Option<u64>, mode: Option<u32>, modified: Option<i64>, uid, gid:
  Option<u32>, link: Option<Vec<u8>>, device: Option<(u32, u32)> }`.
- `cursor`: the scoped entry cursor. Conceptually the protocol is
  `BeginEntry(meta)`, `Data(offset, bytes)*`, `EndEntry`, `EndArchive`.
  Concretely `Cursor::next() -> Result<Option<Entry<'_>>>` where `Entry<'a>` owns
  its `EntryMeta` and a body reader that borrows the cursor. The previous
  entry's unread body (and padding) is skipped on the next call, so there are no
  independently readable entry streams and a body cannot outlive its entry.
  `Cursor::finish()` verifies structure and outer compression.
- `tar`, `cpio`, `zip`: cursor implementations over the existing engines. TAR
  builds one `tar::Archive` over the shared counting reader per header, takes
  the metadata (raw name, link name, PAX fields) and drops it, then reads the
  body itself from the counted position: this removes the self-referential
  borrow and the buffering without `unsafe` or a parser of our own. CPIO keeps
  the existing newc code, moved behind the cursor, with name and body sizes
  charged to the budget before allocation. ZIP reads the central directory (its
  size is charged as index memory) and opens each body through `rawzip`.
- `limits`: `Limits { total_output, entry_output, entries, index_bytes,
  workspace_bytes }` and `Meter`. Defaults: 256 GiB total, 64 GiB per entry,
  4,194,304 entries, 256 MiB index memory, 1 GiB decoder workspace. The meter is
  shared by every layer: bytes produced by a decoder (an outer codec or a ZIP
  inflater) are charged once to `total_output`, bytes of any single entry body to
  `entry_output`, names, link targets, PAX records and the ZIP central directory
  to `index_bytes`.
- `driver`: opens an `Input`, applies compression, builds the cursor, drives
  iteration, exposes `Cancel` (a `&dyn Fn() -> bool` polled at every buffer
  refill and every entry) and reports `Completion`.
- `policy`, `stage`, `extract`: the filesystem adapter below.
- `create`: TAR and CPIO writers from a tree below a root, reading sources
  through the retained root, writing through `Encoder`, publishing by a
  no-clobber (or replacing) rename of a temporary file in the destination
  directory.

### Completion levels

`Completion` is ordered; an operation reports the highest level it reached and
the XSH operations succeed only at the level they promise.

| Level | Meaning |
|---|---|
| `Inspected` | headers were read; bodies skipped or unread |
| `PayloadConsumed` | every selected body was read to its declared end |
| `ChecksumVerified` | each consumed body matched its stored checksum, where the format stores one (ZIP CRC32, CPIO `070702`; TAR stores only header checksums, which are checked for every header) |
| `StructureComplete` | the end-of-archive trailer was found and nothing but zero padding follows |
| `OuterComplete` | the compression stream read to its end and its trailer checks passed |

`archive.extract` succeeds only at `OuterComplete` (and `ChecksumVerified` when
the format stores checksums); a missing or damaged trailer is `Truncated` or
`Corrupt`. A fully consumed `archive.list` stream reaches `StructureComplete` and
`OuterComplete`, and its last pull returns the error if either fails; ZIP also
checks the central directory count. A listing that is abandoned early has
verified nothing, and listing two names does not verify an archive. Checksums
are integrity checks, not authenticity: nothing here proves who wrote the
archive.

### Stream and seek rules

`Input` is `Seekable` (a regular file or an in-memory `Bytes` value) or
`Sequential` (anything else readable once: a pipe, a FIFO, a character device, a
socket, process output, a `Stream[Bytes]`).

- A `Path` is opened and classified by `fstat`: regular files are `Seekable`,
  everything else `Sequential`.
- `Bytes` is `Seekable` (it is already resident; ZIP from `Bytes` does not spool).
- `Stream[Bytes]` is `Sequential`; its chunks are pulled on demand with bounded
  buffering. A script-produced stream is materialized by the caller before the
  call; only live host streams and already-materialized streams are read
  directly.
- TAR and CPIO accept any `Input`, with any compression.
- ZIP requires `Seekable` and `compression` of `Auto` or `Plain` (ZIP compresses
  entries itself). A `Sequential` input fails with `ArchiveError.NeedsSeek`
  naming the input kind; a compression other than `Auto`/`Plain` fails with
  `Unsupported`. Nothing is read from the stream first.
- A compression layer makes its output sequential, so no format that needs
  seeking can sit behind it.

### Policy object and filesystem adapter

`Policy` carries the XSH parameters: `Collision`, `Links`, `Special`, `Modes`,
`Ownership`, `strip_components`, member filters and `Limits`. The default is the
safe untrusted policy: refuse an existing destination, links that stay inside
the root, no devices or FIFOs, permission bits only (`& 0o777`), no ownership.
Rootfs building asks for more explicitly (`links: .Verbatim`, `special: .Create`,
`modes: .Exact`, `ownership: .Apply`, `collision: .Replace`); each unprivileged
failure of an applied request (EPERM on `chown`, `mknod`) is an error, not a skip.

Extraction goes through `xsh-root`. Retained roots are opened once: the
destination (created if absent) and a private staging directory beneath it,
`.xsh-archive-XXXXXXXX` (mode 0700, random name, created exclusively). No path
is joined to a host path. Every component of an entry's path is resolved
beneath the staging root with `RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS` (Linux)
or `O_RESOLVE_BENEATH | O_NOFOLLOW_ANY` (macOS), so an entry can never be
written through a symlink the archive created earlier, whatever the link target.
Names must be relative, free of `..` after normalization of `.` components, and
not empty; a rejected name is `Unsafe`.

`xsh-root` gains the missing mutation operations, each applied to a parent
directory opened with the strict resolver and a validated single leaf name:
`mkdir`, `symlink`, `hard_link`, `rename` (no-replace, replace and exchange),
`remove_file`, `remove_dir`, `set_mode`, `set_owner`, `make_node`, plus an
`OpenOptions::no_symlinks`. They use `rustix` (already a dependency of `xsh`).
This is the smallest shared host boundary that XSH-37 asks for; it is not
advertised as a process sandbox.

Entry handling in staging, in archive order:

- `File`: created with `create_new` (a repeat is `Collision` unless `Replace`),
  content written through the budget, mode applied after the content is written.
- `Directory`: created or reused (a repeated directory entry is not a
  collision); modes are applied deepest-first at the end so a read-only
  directory does not block its children.
- `Symlink`: `Refuse` rejects; `Contained` requires a relative target that
  resolves, lexically, to a path inside the extraction root and stays inside
  after each `..`; `Verbatim` stores the bytes as given. A link is never
  followed during extraction under any policy, because no later entry resolves
  through a symlink.
- `Hardlink`: `Refuse` rejects; otherwise the target must be an earlier regular
  file already in staging (after `strip_components`), linked with `linkat` and
  never through a symlink.
- `Fifo`, `CharDevice`, `BlockDevice`: `Refuse` rejects with `Unsafe`; `Create`
  uses `mknodat`.
- `Other` (an entry the format could not classify, or a sparse member): always
  `Unsupported`; it is never skipped silently.

Publication. After the last entry, `StructureComplete` and `OuterComplete` hold,
and the stage is complete, it is published into the destination root by renaming
each top-level staged entry with no-replace (or replace under `Replace`) and
merging directories that already exist in the destination. Before the first
rename the destination is checked once for conflicts so a `Refuse` collision
publishes nothing. The guarantee named is exactly:

- Before publication the destination is untouched. Any failure, cancellation or
  limit in reading leaves the destination exactly as it was and removes the
  staging directory.
- Each published file or directory becomes visible complete and atomically.
  Publication of several entries is not one transaction: a rename that fails
  after others succeeded (for example a concurrent writer takes a name after the
  conflict check) leaves the earlier entries published and reports which.
- Atomic visibility is not durability. Nothing is `fsync`ed; a power loss can
  lose or truncate published data. A durable mode is not provided.

When the destination does not exist, the staging directory is its parent's
sibling and publication is a single directory rename, which is atomic for the
whole tree; this is the case Laputa's source extraction uses. (The staging
directory is then created beside the destination under the same retained
parent.)

### Resource and cancellation behavior

Every `read` of a body and every refill of a decoder charges the shared `Meter`
and polls `Cancel`. A breach is `Limit` (or `Canceled`) at the next boundary
inside one buffer (64 KiB), so a bomb stops at the limit rather than at the disk.
Cancellation removes the stage before returning. Nothing in the crate spawns a
thread or holds a lock across a callback.

### Adapter

`src/modules/archive/` shrinks to `mod.rs`, which converts `Value` to
crate types (the `Stream[Bytes]` reader, `Path` bytes, enum tags), calls the
crate, converts `ArchiveEntry` records and `ArchiveError`, and implements `Cancel`
with the runtime's cancellation signal. The listing stream is a `LiveStream`
that owns the cursor and pulls one entry per `next`, with `Completion` checked
when the cursor ends. The `archive` effect remains `fs`. The
`ExprArchiveTarCreate` specialization and `ArchiveTar*`/`ArchiveZip*`/`ArchiveCpio*`
`RuntimeOp`s are deleted with the old functions.

## Ordered increments

Each increment is one or more commits that pass their focused gates; the first
commit on each lists its contract changes in the body.

| # | Increment | Tests added |
|---|---|---|
| 0 | This RFC, the TODO ticket and the roadmap row | none |
| 1 | `xsh-archive`: errors, `Limits`/`Meter`, `Cancel`, `Codec`/`Encode`/`Decoder`/`Encoder` with the single finish path, outer-complete verification | level ranges, truncated and corrupt streams for all five codecs, failing sink at finish, bounded buffers, expansion budget |
| 2 | Entry model and cursor over TAR, CPIO, ZIP; `Input` and seek rules; `Completion` | malformed and truncated archives, missing trailer, non-UTF-8 names and link targets, ZIP times, ZIP-from-pipe rejection, stream input for TAR and CPIO, large bodies streamed |
| 3 | `xsh-root` mutation primitives and strict resolver | traversal, absolute names, symlink and hardlink races, special files |
| 4 | Rooted extraction: policy, staging, publication, cleanup | collision and duplicate names, links escaping the root, cancel and failure cleanup, mode and special-bit policy |
| 5 | Writers: TAR and CPIO create with the unified encoder | interop with BusyBox and Python readers, level validation, failing finish leaves no destination |
| 6 | Registry: built-in enums, `ArchiveError`, the three new signatures and the retyped `compress`/`decompress`/`decompress_bytes`; adapter in `src/modules/archive`; sema/lowering support; `compression.transform` on the shared codec layer | native tests for every enum parameter, stream sources, error variants |
| 7 | Migrate XSH callers and tests; SPEC template, registry docs, generated docs | existing archive native tests ported |
| 8 | Delete the nine functions, `ExprArchiveTarCreate`, `src/modules/archive/{tar,zip,cpio,policy}.rs` and `src/modules/compression.rs` | none |
| 9 | Laputa migration | Laputa PM, installer and seed tests |

Increments 1 to 5 land in the crate with Rust tests and no behavior change.
Increment 6 adds the new API next to the old; 7 moves the callers; 8 removes the
old path. A branch is reported for merge after each group (1 to 5, 6 to 8, 9).

## Test plan

Crate tests (Rust, `crates/xsh-archive/tests`), each written before its fix:

- Compression: all five codecs round-trip; level boundaries accepted and one
  past rejected per algorithm; a corrupted checksum, a cut trailer and trailing
  garbage are `Corrupt`/`Truncated`; a sink that fails on `finish` returns the
  error and publishes nothing; decoder memory stays within the workspace limit;
  a stream of zeros expanding beyond `total_output` stops within one buffer.
- Archives: truncated at every 512-byte boundary and inside a header; missing
  trailer; bad header checksum; oversize `namesize`; negative and overflowing
  sizes; non-UTF-8 names and link targets round-trip through the cursor; ZIP
  `modified` matches the writer's timestamp; duplicate names.
- Streams: TAR and CPIO read from a one-byte-at-a-time reader and from a pipe;
  ZIP from a pipe is `NeedsSeek` without reading; ZIP from memory works; a 4 GiB
  sparse-looking body streams with bounded memory.
- Extraction: absolute names, `..`, a name through an earlier symlink, a
  symlink whose target escapes (`Contained`), absolute links under each `Links`
  value, a hardlink to a missing or non-file target, duplicate and
  case-different names, destination collisions under both `Collision` values,
  `special_bits` stripped under `Modes.Plain`, devices refused, cancellation in
  the middle of a large body leaves the destination unchanged and no stage.
- Interop: archives written by BusyBox `tar` and `cpio`, Python `tarfile` (ustar,
  GNU, PAX) and `zipfile`/`zip` list and extract identically; archives written by
  the crate are read by BusyBox `tar`/`cpio`, `unzip -t` where applicable, and
  Python.

Native XSH tests (`tests/xsh/stdlib/archive.xsh` rewritten): every migrated
operation, every enum parameter, an `ArchiveError` variant per failure class, a
`Stream[Bytes]` source from `run.stream --bytes`, and ZIP rejection of a pipe.

Gates: `cargo dev gate --plan <paths>` for each increment's files; the
`test-sema-unit` case when `src/sema` changes; Laputa's `pm.root/compose`,
`pm.execute/reuse-count` and `pm.plan/inventory` scenarios, plus the installer
and seed native tests, for increment 9.

## Alternatives considered

- Write our own TAR parser so the cursor can borrow nothing: rejected; `tar`'s
  header, PAX and long-name handling is correct and widely exercised, and the
  per-header `Archive` over a shared reader gives a lending cursor without it.
- Run the existing `tar::Entries` iteration on a producer thread behind a bounded
  channel: avoids the self-referential borrow, but adds a thread, a channel and a
  shutdown path for no gain over the cursor.
- Spool a non-seekable ZIP to a temporary file: rejected by the owner; it hides
  unbounded disk use behind a type that looks streaming.
- Keep format-named functions and add an enum only for compression: rejected by
  the owner; the format would remain a name in three places.
- Strings with validation for options: rejected; typed enums let the checker
  reject `format: "tarr"` before the program runs.
- Declare the enums in embedded XSH source (`stdlib/archive.xsh`): the registry
  signature cannot name a type declared there, so a call would have no expected
  type for `.Tar`.
- Per-entry atomic publication only: simpler, but then a missing trailer would
  leave published entries. Staging closes that without a journal.
- Evaluate `compcol` or `libarchive_oxide` now: out of scope by the owner; the
  crate's cursor trait is the seam a later backend would implement.
- Durable publication (`fsync` of every file and directory): named as not
  provided; adding it is a flag on the same publication step.

## Open questions

1. `Format` has no `Auto`. Default: callers choose by suffix; a later helper in
   userspace can map a name to a format.
2. A payload-free enum value is a `Value::Tag` with an interned name, not a small
   integer. Compacting it would make every enum cheaper and is a runtime-wide
   change. Default: not in this campaign; the crate's enums are `repr(u8)`.
3. Type annotations that spell `archive.Format`: the checker resolves variants
   and parameter types without them. Default: annotations are added only if a
   migrated caller needs one.
4. Ownership `Apply` and `special: .Create` need privilege that the unit tests
   lack outside a user namespace. Default: tested in a private user namespace
   where the kernel allows it, skipped with a named reason otherwise.
5. macOS rooted extraction is implemented against the `O_NOFOLLOW_ANY` and
   `O_RESOLVE_BENEATH` flags and compiled but not run in this campaign.

## Decision

Accepted by the owner for implementation. The binding decisions are the
format-parameterized API with real enums for every closed option set, a breaking
migration of every caller with no compatibility adapters, stream input for TAR
and CPIO in the first slice, and explicit ZIP rejection of non-seekable input.

## Outcome

Open.
