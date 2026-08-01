# Streaming API Plan

This document tracks the work required to make enumerable standard-library
APIs streaming by default. It complements `docs/STREAMS.md`, which documents the
pipeline engine, and is an implementation plan rather than a language proposal.

## Goal

An API that produces an unbounded or potentially large sequence should return a
live `Stream[T]`. Callers that need random access or a reusable snapshot should
spell that boundary explicitly with `.collect()`.

The public type and runtime representation must agree. A function declared as
`Result[Stream[T]]` must produce a `Value::Stream` backed by a live source; it
must not produce a `List` or collect a `Vec` before the caller consumes it.

## Greppable implementation handles

| Concern | Symbols | Owner and coverage |
|---|---|---|
| stream value contract | `Value::Stream`, `StreamValue`, `LiveStream::next` | `src/runtime/value.rs`; stream behavior in `tests/runtime/streams.rs` |
| stream materialization | `Evaluator::collect_stream_values`, `collect` | `src/runtime/eval/stream.rs`, `src/runtime/eval.rs`; collection and stream tests |
| stream signatures and operation IDs | `RuntimeOp`, `api_spec`, `LoweredValue::Stream` | `crates/xsh-registry/src/signature/modules.rs`, `crates/xsh-registry/src/runtime_op.rs`, `src/runtime/eval/lower.rs` |
| live archive/process sources | `tar_list`, `cpio_list`, `zip_list`, `process.list`, `process.threads` | `src/modules/archive/*`, `src/modules/process.rs`; `tests/runtime/modules.rs`, `process.rs`, and `streams.rs` |
| short-circuit coverage | `take`, `first`, `any`, `all` | stream fixtures and tests under `tests/runtime/streams.rs` |

The XSH API names remain the contract. The Rust handles above identify the
signature, lowering, live-source, and collection boundaries that implement it.

The primary user-facing shape is:

```xsh
for entry in archive.zip_list(archive_path)? {
  print entry.path
}
```

Materialization remains available when intentional:

```xsh
let entries = archive.zip_list(archive_path)?.collect()
print entries.len()
```

## Scope

### High-priority API conversions

These enumerable records or potentially large data sources should be converted
from `Result[List[T]]` to `Result[Stream[T]]`:

- `archive.cpio_list`
- `archive.zip_list`
- `fs.mounts`
- `linux.interfaces`
- `linux.routes`
- `linux.modules`
- `linux.dmesg`
- `linux.disk_usage`
- `linux.block_devices`
- `linux.rfkill_list`
- `linux.loop_list`
- `linux.open_files`
- `unix.reap_child_events`

The already-converted `archive.tar_list` is the reference archive API. Its
former `tar_list_stream` name must stay removed; one operation should not have
separate eager and streaming spellings.

### Declared-stream APIs that are currently eager

These APIs already advertise streams but violate the contract in the lowered
runtime:

- `process.list`
- `process.threads`
- `process.port`
- `process.ports`

Their module helpers return `Vec<Value>`, and lowered dispatch routes them
through `lowered_runtime_list_result`. They must construct live stream sources
end to end, not merely retain `Stream` in their signatures.

Relevant owners are:

- signatures and operation IDs: `crates/xsh-registry/src/signature/modules.rs`
  and `crates/xsh-registry/src/runtime_op.rs`;
- host enumeration: `src/modules/archive/*`, `src/modules/fs.rs`,
  `src/modules/linux/*`, `src/modules/process.rs`, and `src/modules/unix.rs`;
- lowered dispatch: `src/runtime/eval/lower.rs` and
  `src/runtime/eval/lowered_run.rs`;
- live-source and pipeline behavior: `src/runtime/value.rs` and
  `src/runtime/eval/stream.rs`.

## Semantic invariants

1. `Result[Stream[T]]` is opened or initialized at the API call, then produces
   items on demand through `LiveStream::next`.
2. Archive and filesystem sources preserve documented ordering. Process and
   kernel sources preserve their existing ordering guarantees; if ordering is
   unspecified, document that rather than sorting to make streaming easier.
3. Errors while opening or validating the source are returned by the `Result`.
   Errors while reading a later item are reported during stream consumption
   with the original operation kind and source span.
4. `for`, pipeline terminals, and short-circuit stages such as `take`, `first`,
   `any`, and `all` can stop the source without draining it.
5. A live stream is single-use. `.collect()` drains remaining items and returns
   a `List[T]`; it does not make the source reusable.
6. No lowered path for a streaming operation may call
   `lowered_runtime_list_result`, `collect_stream_values`, or an equivalent
   whole-source conversion before the consumer requests materialization.
7. Required source initialization may remain eager. For example, a ZIP
   implementation may need to read its central directory, but it must not
   construct all public entry records before the first `next()`.

## Implementation phases

### 1. Establish the lowered-stream contract

- Add a focused runtime test that distinguishes a live source from a collected
  list. A source with a valid first entry and an invalid later entry should
  succeed under `|> take(1)` or a one-item `for` loop.
- Audit every `RuntimeOp` whose registry return type is `Stream`.
- Remove or rename lowered helpers that silently turn stream results into lists.
- Make generic module calls and specialized lowered expression paths return
  `LoweredValue::Stream` consistently.
- Add a code comment near the conversion boundary documenting that a declared
  stream must never be materialized there.

### 2. Finish process streaming

Replace `Vec<Value>` process enumerators with live producers:

- Linux `/proc` enumeration should retain an iterator over directory entries or
  parsed socket rows and emit one process/port record per `next()`.
- macOS process, thread, and socket enumeration should expose the smallest
  platform iterator that preserves current filtering and visibility behavior.
- Keep validation and snapshot setup at creation, but defer record conversion
  and per-item metadata work until consumption where safe.
- Ensure `process.list |> take(1)`, `process.threads |> take(1)`, and the port
  variants do not scan or convert the complete source first.

Update process tests for normal iteration and short-circuit consumption.

### 3. Convert archive listings

Implement live sources for:

- `archive.cpio_list`, using a buffered reader and one CPIO header/data skip per
  `next()`;
- `archive.zip_list`, using the ZIP reader's entry index as the source and
  converting one entry record per `next()`.

ZIP central-directory loading may remain eager if required by `async_zip`, but
entry records and validation must not be collected into a `Vec<Value>` up front.
Preserve archive order and existing path/type validation.

Add tests for direct `for`, pipeline filtering, `.collect()`, member counts, and
short-circuit behavior. Update archive examples and all `.len()` callers to use
`.collect().len()`.

### 4. Convert filesystem and kernel snapshots

Convert the remaining high-priority snapshot enumerators to live sources. Use
one adapter pattern where platform code already exposes an iterator; do not
introduce a generic abstraction solely to unify unrelated kernel APIs.

Priority order:

1. `fs.mounts`, `linux.open_files`, and `linux.dmesg`, because their output can
   be large or tied to external system state;
2. `linux.modules`, `linux.disk_usage`, `linux.block_devices`, and
   `linux.loop_list`;
3. `linux.interfaces`, `linux.routes`, and `linux.rfkill_list`.

For APIs whose platform syscall necessarily returns a complete snapshot, keep
snapshot acquisition eager but stream record conversion and delivery. The API
should still be one-pass and compatible with `for` and pipeline terminals.

### 5. Convert child-event draining

Change `unix.reap_child_events` to return a stream over events drained for that
call. It remains a finite snapshot of currently available events, not a
long-lived subscription; `linux.uevent_stream` remains the API for a live event
subscription.

Document that distinction in `docs/SPEC.md` and add tests for empty, one-event,
and multiple-event drains.

### 6. Update the public contract and corpus

For every converted API, update together:

- `docs/SPEC.md`;
- `docs/STDLIB.md` and the standard-module fixture;
- nearest module/runtime tests;
- examples, `core/`, and `showcase/` callers that use indexing or `.len()`;
- stream documentation where the list of live sources is maintained.

Regenerate the Markdown documentation only through the documented docs gate
when implementation is complete.

## Explicitly out of scope for this pass

These are finite transformations or control APIs, not external enumeration
sources, and should remain eager unless a separate use case appears:

- `Str.split`, `Str.fields`, `Str.words`, and `Str.wrap`;
- `Bytes.chunks` and `Bytes.strings`;
- `Regex.find` and `Regex.captures`;
- `Map.keys`, `Map.values`, and `Record.keys`;
- `cli.tokens`, DNS result helpers, and environment parsing helpers;
- `process.wait_ready` and multi-handle wait status lists;
- archive create and extract operations, which return `Unit` rather than a
  collection.

## Verification

Run the narrow test for each converted module first, then the relevant gates:

- `cargo test --test runtime archive_module_`
- `cargo test --test runtime streams`
- `cargo test --test runtime process`
- `cargo test --test runtime os`
- `cargo test --test runtime unix`
- `cargo test --test sema`
- `cargo check`

For the completed cross-cutting change, run `cargo test --test runtime` and the
docs gate from `docs/TEST-MAP.md`. Do not run formatters or autofixers.

Completion means there are no standard APIs declared as `Stream` that route
through a list-producing lowered helper, and the high-priority enumerable APIs
above all support direct `for` consumption without implicit collection.
