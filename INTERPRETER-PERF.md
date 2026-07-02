# Interpreter Performance Goals

## Primary Objective: small_corpus Frontend Floor

Make the checked-in small-script front-end corpus as lean as possible. This is
the primary performance objective because it isolates the floor cost of parsing,
checking, and lowering ordinary standalone scripts without the large-runtime
noise of `showcase/tokei.xsh`.

Baseline command:

```sh
cargo bench -p xshi --bench bench small_corpus -- --sample-size 10 --warm-up-time 0.5 --measurement-time 1
```

Fresh 2026-07-02 local baseline in `perf/small-corpus-baseline-aarch64.json`:

| Lens | Current mean | Current allocs | Current allocated |
|---|---:|---:|---:|
| parse | `14.337 ms` | `115,274` | `10.0 MiB` |
| parse/check | `23.915 ms` | `214,208` | `25.0 MiB` |
| parse/check/lower | `27.707 ms` | `223,725` | `32.4 MiB` |

The selected corpus is 383 checked-in `.xsh` files with at most 200 lines and
16 KiB: 15,369 lines and 413,461 bytes. Files that do not parse or check
cleanly as standalone scripts are excluded by the benchmark.

The reasonably ambitious bar is:

- `parse_check_lower_small_corpus_le200_lines_16k` Criterion mean **<= 22.0 ms**
  with the same command and corpus rules.
- `parse_check_lower_small_corpus_le200_lines_16k` allocation audit **<= 190,000
  allocations** and **<= 27.5 MiB allocated**.
- No compensating regression in narrower lenses: parse-only and parse/check
  means, allocation counts, and allocated bytes should not be more than 5% worse
  than the fresh baseline unless the full parse/check/lower gate is already met
  and the tradeoff is documented.
- The front-end behavior gate still passes: `cargo check --lib`, relevant
  runtime/parser tests, and `git diff --check`. If lowering behavior changes,
  update the nearest lowering/runtime tests.

The current slowest phase totals from the setup audit are parse (`13.966480 ms`)
and compact install (`10.400462 ms`), followed by functions (`5.356127 ms`) and
body probing (`2.632869 ms`). The first useful paths are therefore parser/token
churn, compact install/module setup, and body/function probing allocation
pressure.

## Stretch Goal: tokei.xsh Native Parity

Get `showcase/tokei.xsh` wall-clock time on the Sentry corpus within **1x native
tokei** and peak RSS within **1x of native tokei**. The byte-for-byte output
parity gate is only **XSH against XSH's own saved output** for the same corpus
and options. It is explicitly **not** XSH against native tokei. Native tokei is
the performance baseline and an accuracy comparison lens, not the output oracle:
XSH may intentionally differ from native tokei's line classification,
child-language treatment, JSON field order, and report ordering when those
differences are part of the current showcase behavior.

This is now a stretch interpreter and lowered-runtime objective. It must not
change language semantics or script-visible behavior.

The stretch native-tokei objective is not complete as of the 2026-07-02 audit on
the current `/Users/josh/dev/sentry` checkout. The 64 MiB lowered eval-frame
work is complete. The checkout is about 3.1 GB and 140,909 files.

Fresh release samples from `target/release/xsh` and the local native `tokei`:

| Path | XSH release | Native tokei | Status |
|---|---:|---:|---|
| table recent samples | `0.88s-0.93s / 54,034,432-57,475,072` bytes max RSS | `0.62s / 48,971,776` bytes max RSS | fails wall and RSS |
| JSON recent samples | `0.88s-0.94s / 63,078,400-66,584,576` bytes max RSS | `0.62s / 56,197,120` bytes max RSS | fails wall and RSS |

These are single serial macOS `/usr/bin/time -l` samples, so rerun before making
a final keep/revert decision on a narrow change. RSS moved closer after the
lowered `par-map` worker cap, inline table child-blob guard, streaming JSON
`for` fold, direct lowered `Map.len()`, compacted streaming `par-map` result
buffers, and in-place lowered self-assignment for `Map.push`/`Map.remove`, but
the strict 1x wall-clock and RSS targets are still not met.

The output-parity check is XSH-vs-XSH, not XSH-vs-native-tokei. Raw comparison
against native tokei is useful only as an accuracy lens. Known native
differences on the Sentry corpus include line-classification counts, JSON field
order, and report ordering; those do not by themselves fail this objective. A
native diff is not a regression unless the XSH-vs-XSH saved-output gate also
changes unexpectedly. Fresh XSH table and JSON output from the current binary
compared byte-for-byte identical to the saved `table-final-a.txt` and
`json-final-a.json` artifacts after the MDX prose-only change.

The 2026-07-02 MDX change routes `LangMdx` through the prose/plain-text counter
instead of the Markdown fence scanner. This matches native tokei's MDX
child-language treatment on the Sentry corpus and removes the former MDX child
rows without adding scanner work.

The 2026-07-02 direct-reduce change keeps the table path's
`par-map |> flat-map { |rows| rows } |> reduce-by` shape but avoids building the
flattened transient row list. It first validates that every outer item is a list,
then reduces nested rows in encounter order. For live stream sources feeding a
matching `par-map |> flat-map(identity) |> reduce-by` projection, the lowered
runner now drains completed par-map results into the reducer in encounter order
instead of retaining the whole par-map result graph. This is distinct from the
rejected worker-local aggregation fusion below.

The direct-reduce path also recognizes empty-body `reduce-by --sum` projections
of the form `{key: item.field, value: {out: item.field, ...}}`. For matching
reducers it skips the transient outer `{key, value}` record and updates occupied
record accumulators field-by-field with the same internal record-sum logic.
Perf-metrics on the Sentry table path moved from roughly `358 MB` allocated
before direct-reduce/projection to `346 MB`; release RSS still fails the target.

The 2026-07-02 compact JSON encoder change replaces the lowered compact
`json.encode` validate-then-miniserde path with a validating writer that reserves
an estimated output capacity. On the Sentry JSON path this preserved byte output
and reduced perf-metrics allocation volume and output-string reallocation bytes,
but the release RSS target still fails.

The 2026-07-02 lowered `par-map` worker cap limits default lowered workers to 6
and uses 1 MiB release worker stacks. On the Sentry corpus this reduced retained
concurrency overhead while preserving XSH-vs-XSH output, but it is not enough to
meet native wall-clock or RSS. The table path also guards child-blob key
iteration with an inline language match so languages that cannot produce
embedded child blobs do not allocate empty blob key lists in the hot par-map
body.

The lowered JSON `for scanned in fs.files(...) |> ... |> par-map |> where`
shape now streams ordered par-map results directly into simple loop bodies
instead of first materializing the whole post-par-map result list. This preserved
XSH-vs-XSH JSON output and moved JSON RSS down, but native RSS remains lower.

The 2026-07-02 `Map.len()` change closes a core method-surface gap and lets hot
map cardinality checks avoid allocating `keys()` lists. On the Sentry `tokei.xsh`
paths this is a small allocation win, not a goal-closing change. The streaming
lowered `par-map` result buffer also now compacts drained prefixes for both the
streaming reduce and streaming `for` paths. This preserved XSH-vs-XSH output and
moved recent release RSS samples slightly lower, but native wall/RSS remain
ahead.

The lowered self-assignment fast path now handles `map = map.push(key, value)`
and `map = map.remove(key)`, matching the existing in-place paths for
`list.push` and `map.set`. This reduces clone-heavy map value movement while
preserving alias behavior. On the Sentry JSON path, perf-metrics moved from
roughly `3,086,326` allocation calls / `372,859,819` bytes after `Map.len()` and
streaming-result compaction to `3,082,648` calls / `372,474,227` bytes. Release
RSS samples remain noisy and still fail native.

The first scoped 2026-07-02 "shrink lowered eval frames" trial split the
collection self-assignment specialization out of the main `eval_lowered_stmt`
match behind a guarded `Set`-method dispatch. This keeps the large in-place
collection update logic out of the recursive statement evaluator's primary
match arm without making ordinary assignments pay a helper call. It preserved
XSH-vs-XSH table and JSON bytes. Measured tokei impact was mixed/noisy rather
than goal-closing: fresh serial samples after the guarded split were `0.88s /
65,830,912` bytes for JSON and `0.92s / 55,525,376` bytes for the table path.
The local toolchain could emit stack-size metadata, but the installed tools
lacked `llvm-readobj`/`llvm-objdump`, so no per-function stack-size numbers were
decoded for this trial.

The 64 MiB stack audit then reduced `run_eval_on_large_stack` from a 1 GiB stack
reservation to `64 * 1024 * 1024`. With that setting, the required debug/runtime
and release gates passed. Serial Sentry samples that were not run concurrently
preserved XSH-vs-XSH bytes and stayed inside the 64 MiB audit's 5% no-regress
band: JSON `0.88s / 64,143,360` and `0.89s / 63,258,624`, table `0.90s /
56,164,352` and `0.91s / 57,475,072`. Slower JSON/table runs taken while other
benchmark commands were running were treated as contaminated and not used for
the gate.

The next lowered value-movement trial targeted fixed-shape record work rather
than more pipeline fusion. Projected `reduce-by --sum` now caches `RecordVec`
source-field indexes when item layouts stay stable, and record literal
construction appends/replaces fields during construction then sorts once before
creating the final `RecordVec`/inline stats value. This preserved XSH-vs-XSH
bytes. The table path showed a useful best RSS sample (`0.88s / 56,328,192`,
with one slower `1.37s / 54,034,432` outlier), while JSON stayed neutral
(`0.92s / 64,618,496` to `65,716,224`). This is progress on core record/value
movement, but it does not close the stretch native wall/RSS gap.

The implementation details for the current compact frontend and lowered runtime
architecture belong in `PIPELINE.md`.

## Verification

Recent verification from the 2026-07-02 audit:

- `cargo bench -p xshi --bench bench small_corpus -- --sample-size 10
  --warm-up-time 0.5 --measurement-time 1`
- `cargo check --lib`
- `cargo test --test runtime json`
- `cargo test --test syntax`
- `cargo test compact_lowered_runner`
- `cargo test --test runtime flat_map_identity_reduce_by_matches_explicit_rows`
- `cargo test --test runtime live_stream_par_map_flat_map_reduce_by_matches_collected_rows`
- `cargo test --test runtime live_stream_par_map_for_loop_matches_collected_rows`
- `cargo test map_empty_constructor_lowers_record_builder`
- `cargo test lowered_self_collection_assignment_preserves_aliases`
- `cargo build --bin xsh && cargo run -p xsht -- test showcase/tests/test-tokei.xsh`
- `RUSTFLAGS="-Z emit-stack-sizes" cargo rustc --lib -- -Z emit-stack-sizes`
  (built successfully, but local tools could not decode per-function stack
  sizes)
- `cargo test --test runtime`
- `cargo build --release --bin xsh`
- `cmp -s target/perf/tokei-current/table-final-a.txt
  target/perf/tokei-current/table-map-push-a.txt`
- `cmp -s target/perf/tokei-current/json-final-a.json
  target/perf/tokei-current/json-map-push-b.json`
- `cmp -s target/perf/tokei-current/table-map-push-a.txt
  target/perf/tokei-current/table-frame-split-guarded-a.txt`
- `cmp -s target/perf/tokei-current/json-map-push-b.json
  target/perf/tokei-current/json-frame-split-guarded-b.json`
- `cmp -s target/perf/tokei-current/table-frame-split-guarded-a.txt
  target/perf/tokei-current/table-stack64-c.txt`
- `cmp -s target/perf/tokei-current/json-frame-split-guarded-b.json
  target/perf/tokei-current/json-stack64-e.json`
- `cmp -s target/perf/tokei-current/table-stack64-c.txt
  target/perf/tokei-current/table-record-sort-once-b.txt`
- `cmp -s target/perf/tokei-current/json-stack64-e.json
  target/perf/tokei-current/json-record-sort-once-b.json`
- raw `cmp -s` checks of XSH table and JSON output against native tokei output
  on `/Users/josh/dev/sentry` (both differed; this is expected and is only an
  accuracy lens; it is not the objective's output-parity gate)
- repeated `/usr/bin/time -l target/release/xsh showcase/tokei.xsh -- ...`
  table and JSON samples on `/Users/josh/dev/sentry`
- `git diff --check`
- `cargo build --release --bin xsh -vv` spot check confirmed normal release
  builds do not use PGO unless the opt-in PGO flow is requested.

One attempted verification command failed because it used a native-tokei-style
flag that the XSH showcase does not accept: `target/release/xsh
showcase/tokei.xsh -- --output json /Users/josh/dev/sentry`. The correct XSH
showcase flag is `--json`.

When rerunning measurements after a `perf-metrics` build, rebuild a normal
release binary first. The `perf-metrics` feature installs a different allocator
and changes RSS and timing.

## Rejected directions

These trials preserved behavior but were reverted or should not be repeated as
narrow changes:

- Lowered `List` clones as immutable copy-on-write `Arc<Vec<_>>` during clone.
  Allocation volume improved, but RSS did not. The kept design is slot-rooted
  `SharedList` freezing, documented in `PIPELINE.md`.
- Lowered-only anonymous mmap backing for `Path.read_bytes()`. Focused byte
  behavior tests passed, but release RSS did not improve on generated corpora.
- Narrow streaming fusion for `par-map |> where |> flat-map |> reduce-by`.
  Sentry RSS moved only slightly and wall time regressed.
- Worker-local runtime fusion for `par-map |> flat-map(identity) |> reduce-by`
  after the `SummaryRow` script change. It reduced instruction count but did
  not reduce RSS beyond the script change and added evaluator complexity.
- Changing lowered `RecordVec` to `Arc<Vec<_>>`. Table RSS regressed in repeated
  release samples and JSON movement was noisy. Keep owned `RecordVec` unless a
  broader lifetime design changes both paths.
- Calling macOS `malloc_zone_pressure_relief` after lowered `par-map` worker
  joins. RSS and CPU got worse.
- A special lowered `print json.encode(...)` fast path that moved the encoded
  string buffer directly into stdout. It did not reduce the JSON RSS peak.
- Consuming adjacent `where` stages in place after lowered streaming `par-map`.
  It preserved output in targeted tests, but final Sentry JSON RSS did not
  improve, so the optimization was reverted.
- Guarding the table path's child-blob loop through a helper function in
  `showcase/tokei.xsh`. It preserved output, but the extra hot helper call
  increased allocation volume, so that helper-call shape was reverted. The kept
  version is an inline language match in the hot block.
- Reducing release lowered `par-map` worker stacks below 1 MiB and capping
  default workers at 4 or 5. Those variants preserved output, but the wall/RSS
  tradeoff was worse than the kept 6-worker cap.
- Writing top-level JSON object members with repeated `io.write_stdout` calls in
  `showcase/tokei.xsh`. It preserved XSH-vs-XSH bytes, but JSON RSS regressed;
  keep the single compact `json.encode(output)` path unless the runtime can
  stream that encoding without extra retained state.

If further aggregation fusion is revisited, it needs to preserve the current
encounter-order error behavior and floating-point aggregation semantics. It also
needs to attack the remaining file-byte/value allocation floor, not just remove
another post-`par-map` container.

## Why native tokei is small

Native tokei uses `ignore::Walk::parallel()` plus `rayon` to fuse walk, read,
and scan into one parallel operation. There are no evaluator clones, no
intermediate item vectors, and no record/map value construction for each file
entry. Each file's bytes are read, scanned, and dropped before the next file is
pulled. Peak memory is roughly `max(file_size) * thread_count` plus fixed
overhead.

XSH has to preserve language-level values and boundaries, so the successful
work focused on making those values compact and lazy where behavior allows it,
then avoiding retained duplicate result graphs in `showcase/tokei.xsh`.

## References

- `PIPELINE.md` - current compact frontend and lowered runtime architecture
- `docs/IR.md` - lowered IR design and verification guidance
- `perf/README.md` - frontend and runtime profiling commands
- `showcase/tokei.xsh` - benchmark script and output-shape constraints
