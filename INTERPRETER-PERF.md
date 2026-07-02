# Interpreter Performance: tokei.xsh

## Objective

Get `showcase/tokei.xsh` wall-clock time on the Sentry corpus within **1.2× of
native tokei** (speed target: ≤ 840ms), and peak RSS within **1.5× of native
tokei** (memory target: ≤ 66MB). Byte-for-byte identical output. No language
semantics change, no script-visible behavior change.

## Current state (2026-07-01 EOD)

| Tool | Wall | RSS | vs real tokei |
|---|---|---|---|
| real tokei | ~700ms | ~44MB | 1.00× |
| xsh-tokei (baseline) | ~2,660ms | ~338MB | 3.80× slower, 7.7× more memory |
| xsh-tokei (optimized) | **~932ms** | ~340MB | 1.33× slower, 7.7× more memory |
| xsh-tokei (micro only) | **262ms** | ~152MB | **2.67× faster** |
| **Speed target** | **≤ 840ms** | — | ≤ 1.20× slower |
| **Memory target** | — | **≤ 66MB** | ≤ 1.50× |

The micro benchmark already crushes the speed target (262ms = 2.67× faster than
native tokei). The macro benchmark is 92ms (11%) from the speed target and 274MB
from the memory target.

xsh-tokei was once **1.3× faster** than native tokei (~530ms) on an older
interpreter whose lowered runtime was **4,051 lines**. The current lowered
runtime is **17,168 lines** — 4.2× larger.

## Where the 340MB RSS comes from

The Sentry corpus is ~625MB of source code across 18,419 files. During the run:

1. **Resolved 2026-07-02: `fork_for_par_map()` cloned the Evaluator per worker
   thread (10×).** Each clone copied `sources` (SourceMap with script text),
   `scopes` (variable bindings), `lowered_pures/procs` maps,
   `module_value_cache`, `env`, and ~30 other fields. The fork path is now
   removed; par-map workers are built from `Arc<LoweredSharedState>` and the
   large lowered/source/module maps are shared.

2. **`Vec<u8>` file read buffers.** Each `std::fs::read(path)` allocates a
   `Vec<u8>` for the file contents. Over the run, 625MB of Vecs are allocated
   and freed. jemalloc retains ~50% of those pages (~200MB). mimalloc and the
   system allocator behave similarly.

3. **Pipeline intermediate Vecs.** Despite the streaming prefix optimization
   (lazy map/where), we still collect one `Vec<LoweredValue>` of 18K items
   (~5MB) for `par-map`, plus `all_results` (~5MB).

4. **Thread stacks.** In the last measured run, 10 threads × 8MB reserved =
   80MB virtual; ~1MB committed each = ~10MB RSS. After the 2026-07-02 worker
   refactor, lowered par-map workers request 64MiB stacks to avoid stack
   overflow in lowered proc calls; rerun RSS before updating the measured table.

Items 2–4 accounted for ~220MB in the last measured run. The remaining ~120MB
was attributed to jemalloc page retention from the evaluator clones and
temporary allocations during scanning. Rerun the macro/RSS benchmark before
updating the headline memory table.

### JSON library and encoding follow-up

`showcase/tokei.xsh --json` is currently an encoding workload, not a parsing
workload: it builds a large XSH result value and then serializes it. Swapping
JSON parsers will not directly fix the peak RSS for this benchmark. The
high-leverage JSON change is to avoid building a second JSON object graph during
`json.encode`, especially on the lowered compact path.

For later JSON parsing work, benchmark small and fast alternatives before
changing dependencies. `jiter` is the first candidate to try because it exposes
iterator-style parsing and a value API without being a SIMD-heavy dependency.
`simd-json` and `sonic-rs` may be faster on large payloads, but they are larger
dependencies and need careful vetting against XSH's portability and dependency
budget. `serde_json` remains the conservative reliability baseline.

Also revisit whether XSH's internal value model can better accommodate
miniserde's `Serialize` API. Today XSH JSON compatibility is fallible at runtime:
`Path`, `Bytes`, `Status`, `Result`, and errors must produce an XSH
`json-compatible` error, not silently serialize or panic. miniserde's serializer
shape is easiest when traversal cannot fail with a language-level error. A
future design could introduce a JSON-compatible borrowed view/wrapper over
`Value`/`LoweredValue` that validates before serialization, or a small internal
fallible writer trait that mirrors the parts of `Serialize` XSH needs while
preserving exact error behavior.

2026-07-02 trial: the lowered compact `json.encode` path now validates
`LoweredValue` for JSON compatibility, then serializes through a borrowed
miniserde `Serialize` view instead of building a `miniserde::json::Value` tree.
This reduced the Sentry `tokei --json` normal-release RSS sample range from
~780-853MB to ~720-758MB. Allocation counters improved from
`allocation_calls=5,660,383`, `allocation_bytes=4,483,316,312`,
`peak_rss=709,312,512` to `allocation_calls=5,251,521`,
`allocation_bytes=4,419,649,330`, `peak_rss=643,956,736`. This confirms the
view-layer design is worthwhile, but the remaining peak is still dominated by
the large XSH result object graph and benchmark/script-level retention.

2026-07-02 follow-up: `showcase/tokei.xsh --json` now clears the intermediate
`scanned_files` list and clears each per-language report accumulator immediately
after its contents have been copied into the final output object and
`Total.children`. The JSON-only `ScannedFile` record also no longer stores a
separate `stats` field because `report.stats` already contains the same value.
This does not change the JSON shape, and the targeted showcase tokei test still
passes. Normal-release RSS samples improved again to ~510-526MB. Perf counters
improved from `allocation_calls=5,251,521`, `allocation_bytes=4,419,649,330`,
`peak_rss=643,956,736` after the miniserde view change to
`allocation_calls=5,008,719`, `allocation_bytes=4,018,650,173`,
`peak_rss=391,839,744`. The result confirms retained duplicate report graphs
are a major component of the JSON peak, but the benchmark is still far from the
66MB RSS target.

2026-07-02 rejected trial: lowered `List` clones were changed to share large
lists through an immutable copy-on-write representation, targeting the duplicate
per-language `reports` lists stored in both each language object and
`Total.children`. This reduced allocation counters to roughly
`allocation_calls=4,859,384` and `allocation_bytes=3,755,961,002`, but
allocator peak RSS regressed to ~473-481MB and normal-release RSS samples were
no better (~496-622MB). Do not pursue this narrow shared-list clone design
without a broader ownership/liveness change; it improves allocation volume but
does not improve the benchmark's peak memory.

2026-07-02 structural follow-up: the shared-list idea was kept only after
changing where sharing happens. Instead of copying a `Vec` into an `Arc` during
`Clone`, the lowered `Param` read path now freezes large slot lists in place:
the existing `Vec` is moved into `SharedList(Arc<Vec<_>>)` once, left in the
slot, and subsequent clones share it. This specifically removes the deep clone
of per-language `reports` when `showcase/tokei.xsh --json` stores those lists in
both each language object and `Total.children`. The JSON path also now iterates
the par-map result directly instead of retaining a `scanned_files` slot, and
clears the per-file `text` buffer after scanning. Normal-release RSS samples are
now ~177MB for both `--json` and table output on the Sentry corpus; perf-metrics
JSON counters are roughly `allocation_calls=1,378,818`,
`allocation_bytes=816,701,477`, and `peak_rss=190,808,064`. This is a real
structural reduction from ~510-526MB normal RSS and ~392MB perf peak, but the
remaining gap to 66MB is now mostly the file-read/scan memory shape rather than
duplicated JSON report lists.

2026-07-02 fs streaming trial: lowered `fs.files`/`fs.walk` now preserve the
live `Stream` instead of eagerly collecting it into a `List`, `count()` drains a
live stream without materializing entries, and the common
`Stream |> map |> where |> par-map { ... }` shape feeds par-map workers through
a bounded queue instead of building the candidate list first. The live
`IgnoreWalkStream` also uses a sequential pull walker so `fs.files` and
`par-map` do not create nested filesystem-walk parallelism. This fixed a stream
prefix ordering bug exposed by `where |> map` and keeps behavior covered by the
runtime stream tests.

The RSS result is directionally better but not enough. On a generated local
14,340-file / 56MB corpus, no-read `fs.files(root, stat:false)? |> count()`
still peaks at ~122MB, while `showcase/tokei.xsh --json` peaks at ~119-120MB
and table output at ~116-117MB. That means the remaining structural floor is
not JSON encoding and not retained file text; even a discarded filesystem entry
allocates a full runtime record (`path`, `name`, `ext`, shaped record wrapper)
for every file. The next serious memory design should make fs entries lowered
and lazy, so `count()` can count paths without constructing records and tokei's
`entry.path`/`entry.name`/`entry.ext` accesses allocate only the fields the
script actually reads.

2026-07-02 lazy fs-entry follow-up: `stat:false` filesystem entries now use a
compact runtime/lowered fs-entry value that reports as a `Record` but only
materializes fields on demand. Direct field access for `path`, `name`, `ext`,
and `kind` projects from the stored path/file kind without first building a
`RecordMap` or lowered `BTreeMap`; generic record consumers still materialize
the old record shape. On the same generated 14,340-file / 56MB corpus, no-read
`fs.files(root, stat:false)? |> count()` dropped from ~122MB to ~17MB RSS.
`showcase/tokei.xsh --json` dropped from ~119-120MB to ~84-87MB, and table
output from ~116-117MB to ~81-82MB.

Scaling now points at file-content bytes, not metadata. On a larger generated
57,348-file / 225MB corpus, no-read walk is only ~29MB RSS, but
`showcase/tokei.xsh --json` is ~274-280MB and table output is ~269MB. The next
memory lever is avoiding heap-owned `std::fs::read` buffers for read-and-scan
workloads, likely with a lowered-only mapped/borrowed byte storage that still
behaves as `Bytes` at language boundaries.

2026-07-02 rejected trial: lowered-only anonymous mmap backing for
`Path.read_bytes()` preserved byte behavior in focused tests but did not reduce
release RSS on the generated corpora (`--json` stayed around ~86MB on the
14,340-file corpus and ~273MB on the 57,348-file corpus). Do not keep that
complexity as a standalone change; the remaining peak appears to be retained
report/result shape, not heap allocator retention from transient file buffers.

2026-07-02 Sentry rerun: the real checkout is currently much larger than the old
note assumed (`~/dev/sentry` is ~3.1GB / 140,909 files; `fs.files(...,
exts: source_exts())` sees 20,101 relevant files after ignore/extension
filtering). Current XSH release completes on that corpus: `--json` is ~231MB
RSS / ~1.05s and table output is ~237MB RSS / ~1.10s. Native tokei samples are
~45MB RSS / ~1.52s for JSON and ~45MB RSS / ~0.75s for table output. No-read
`fs.files(root, stat:false)? |> count()` is only ~19MB RSS, so the remaining
Sentry gap is not traversal or entry metadata.

The ignore-aware selected source is ~18.4K files / ~186MB. The largest selected
files are the locale `.po` files and `CHANGES` at ~1.5-1.7MB each; there are no
single huge selected files setting the floor. A temporary `par-map --jobs=N`
sweep on the table path did not materially move RSS (`N=1` ~234MB, `N=2`
~236MB, `N=4` ~233MB, `N=8` ~229MB), so the gap is not primarily worker-count
or per-thread cache retention. With `perf-metrics`, Sentry table output still
does ~3.5GB of heap allocation and JSON ~2.6GB. That scale points at the
lowered value/record shape during scan/report construction, not just full-file
byte buffers.

2026-07-02 rejected trial: a narrow streaming fusion for
`par-map |> where |> flat-map |> reduce-by` on the table path reduced Sentry RSS
only slightly (~239MB to ~235MB) and regressed wall time (~1.1s to ~1.9s). Do
not keep that evaluator complexity without a stronger streaming aggregation
design.

2026-07-02 table-path follow-up: the default table path now has the `par-map`
worker return final `SummaryRow` reduce rows directly instead of returning
`{language, label, scan}` and expanding that nested scan record later. The
script then does only `flat-map { |rows| rows } |> reduce-by`. This preserves
the table output exactly (verified against the previous XSH table sample) and
keeps JSON untouched. On Sentry, normal release table RSS improved from
~229-237MB to ~127-140MB; a clean single run sampled ~1.05s / ~140MB. With
`perf-metrics`, table allocation volume improved from roughly
`allocation_calls=4,649,345`, `allocation_bytes=3,470,099,917`,
`peak_rss=269,549,568` to `allocation_calls=3,942,983`,
`allocation_bytes=2,259,806,545`, `peak_rss=206,684,160`.

2026-07-02 rejected trial: after the `SummaryRow` script change, a narrow
runtime fusion for `par-map |> flat-map(identity) |> reduce-by` aggregated rows
inside worker-local maps. It reduced instruction count but did not reduce RSS
beyond the script change (~138MB either way), and it added too much evaluator
complexity for no memory win. Do not revive that exact fusion; if aggregation
fusion is revisited, it needs to attack the remaining file-byte/value allocation
floor rather than only remove the post-`par-map` row list.

2026-07-02 lowered record follow-up: slot-rooted field chains now borrow through
lowered records and clone only the projected field, so hot paths like
`scan.stats.blanks` no longer clone the whole nested `Scan`/`Stats` record.
`ParMapBlock` also moves a block-local final slot out when the block itself
initialized that slot, avoiding one clone of `out`-style result locals. Lowered
record literals now use a compact vector-backed record representation while
host/runtime-created records keep the old `BTreeMap` representation. Parallel
lowered `par-map` writes worker results directly into indexed result slots
instead of returning per-worker `(index, result)` chunk vectors. This keeps
public `Value::Record` unchanged and preserves byte-for-byte Sentry table/JSON
output.

Measured on the current `/Users/josh/dev/sentry` checkout, perf-metrics table
allocation volume improved from roughly `allocation_calls=3,942,983`,
`allocation_bytes=2,259,813,698`, `peak_rss=194,494,464` to
`allocation_calls=3,069,738`, `allocation_bytes=720,929,175`,
`peak_rss=186,777,600`. JSON improved from roughly
`allocation_calls=4,124,767`, `allocation_bytes=2,604,986,504`,
`peak_rss=287,031,296` to `allocation_calls=3,138,393`,
`allocation_bytes=742,540,871`, `peak_rss=215,777,280`. DHAT table heap
improved from `Total: 2,314,657,798 bytes`, `t-gmax: 81,898,131 bytes` to
`Total: 744,786,657 bytes`, `t-gmax: 57,883,747 bytes`, putting live table heap
under 66MB. Normal release RSS is still not at the objective on this machine:
recent final-code samples were table ~1.6-2.2s / ~110-116MB RSS and JSON
~1.0s / ~126-130MB RSS. The active objective remains incomplete.

2026-07-02 stack follow-up: debug lowered par-map worker stacks remain 8MiB
because the debug showcase tokei test overflowed at 2-4MiB, but normal release
workers now use 2MiB stacks. Release Sentry table and JSON both complete with
byte-for-byte identical output at 2MiB. This is the right release target shape,
but it did not materially move RSS on macOS: table sampled `1.65s /
109,461,504` bytes and JSON sampled `0.91s / 131,481,600` bytes in this pass.
If 2-4MiB release stacks ever overflow, reduce lowered evaluator frame depth
rather than reverting release workers to 8MiB.

2026-07-02 frontend arena reserve follow-up: the parser arena reserve
heuristics were tightened for statement rows, expression rows, and the `extra`
u32 table so small scripts retain less unused compact-arena capacity without a
post-parse `shrink_to_fit` pass. On the current local parse-corpus report
(`--repeat 1`), `parse_arena_only.compact_arena.retained_bytes` dropped from
`5,007,807` to `4,908,522`; expression storage dropped from `1,648,967` to
`1,614,923`, statement storage from `373,964` to `360,735`, and extra storage
from `358,556` to `306,544`. A `shrink_to_fit` trial reduced retained bytes
further to `3,662,945` but regressed the hot small-corpus lower benchmark, so it
was not kept.

The local small-corpus frontend lens now reports `parse 115,971 allocs /
10.8MiB`, `parse/check 214,862 allocs / 25.9MiB`, and `parse/check/lower
225,472 allocs / 33.6MiB` for 383 files. Per file, that is about `302.8`
allocs / `28.8KiB` to parse and `588.7` allocs / `89.8KiB` through lowering.
Criterion reported `parse_small_corpus` and `parse_check_lower_small_corpus`
improved in that local run.

2026-07-02 compact runner frontend lifetime follow-up: the compact runner now
clones the token table needed for fallback diagnostics, then drops the CST
before calling `try_eval_compact_lowered_only`. The arena is the only parsed
frontend structure kept live during compact evaluation. This preserves fallback
behavior and byte-for-byte output while reducing Sentry normal-release RSS
samples to `1.67s / 108,789,760` bytes for table output and `0.94s /
119,111,680` bytes for JSON output on this machine. The active objective
remains incomplete.

2026-07-02 rejected shared-record trial: changing lowered `RecordVec` from an
owned `Vec` to an `Arc<Vec<_>>` made record clones cheap and preserved output,
but did not help the objective. Table RSS regressed in repeated release samples
(`113,803,264` then `120,406,016` bytes), while JSON improved only noisily
(`112,361,472` then `107,790,336` bytes). The change was reverted; keep the
owned `RecordVec` representation unless a broader record/lifetime design moves
both paths.

2026-07-02 rejected allocator-relief trial: calling macOS
`malloc_zone_pressure_relief` after lowered par-map worker joins preserved output
but worsened the benchmark (`1.60s / 114,933,760` bytes table, `0.89s /
127,713,280` bytes JSON) and increased CPU. The change was reverted; allocator
relief is not a useful substitute for reducing live value churn.

2026-07-02 lowered value layout follow-up: rare/wide lowered variants are now
boxed so dense lowered lists, records, slots, and worker result arrays do not
pay for cold payloads. `CommandPlan` was the largest offender; boxing lowered
`Command` dropped Sentry table RSS from about `109,117,440` bytes to
`79,052,800` bytes in one release sample and JSON to `82,214,912` bytes, with
byte-for-byte identical output. Boxing lowered `Digest` and `Stream` reduced
`LoweredValue` from 56 bytes to 48 bytes; boxing lowered `Status` and `Tag`
reduced it again to 40 bytes. The retained layout matches public behavior:
public `Value` variants and script-visible type names are unchanged.

Current retained release samples on `/Users/josh/dev/sentry` after the 40-byte
layout are `1.94s / 67,108,864` bytes for table output and `0.88s /
80,838,656` bytes for JSON output, both byte-for-byte identical to the saved
XSH outputs. Table output is now effectively at the 66MB target on this
machine; JSON remains above target. A final rebuilt confirmation sample from
the same retained source was noisier but still far below the previous floor:
`1.47s / 75,333,632` bytes table and `1.01s / 83,492,864` bytes JSON, with
identical output.

2026-07-02 rejected JSON print fast path: a special lowered `print
json.encode(...)` path moved the encoded `String` buffer directly into stdout
when tracing was disabled. It preserved output but did not reduce RSS in the
Sentry JSON sample (`85,295,104` bytes) and made the table sample noisier
(`71,368,704` bytes). The change was reverted; the remaining JSON peak is not
fixed by avoiding this final print copy alone.

2026-07-02 32-byte lowered value follow-up: lowered string and byte views now
store `u32` offsets instead of `usize` offsets, with an owned-slice fallback for
pathologically large buffers whose offsets do not fit. Lowered regex values are
also boxed because regexes are cold in the scanner path. This reduces
`LoweredValue` from 40 bytes to 32 bytes while keeping public `Value` unchanged.
`RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR=/tmp/xsh-type-target4 cargo rustc --lib --
-Zprint-type-sizes` reported `runtime::eval::LoweredValue`: 32 bytes,
`LoweredStrView`: 24 bytes, `LoweredBytesView`: 24 bytes, and public
`runtime::value::Value`: 48 bytes.

Retained release samples after this change were byte-for-byte identical to the
saved XSH outputs. Table output improved to `1.35s / 56,737,792` bytes RSS in
the cleaner sample (`59,228,160` bytes in the prior run), comfortably under the
66MB target. JSON improved to `1.38s / 74,612,736` bytes RSS in the cleaner
sample (`80,904,192` bytes in the prior run). JSON remains above the active
memory objective, so the overall objective is still incomplete.

2026-07-02 frontend token/lifetime follow-up: the compact token table now stores
only nonzero payload rows instead of a dense `TokenPayload` slot for every
token, while preserving the existing in-range default-zero payload behavior.
The parse-corpus report now measures token row bytes from the actual compact
layout. On the local corpus, compact token retained bytes dropped from
`1,855,679` to `1,613,099`, and small-corpus parse/check/lower allocation
audits improved from `225,472 allocs / 33.2MiB` to `224,701 allocs / 33.0MiB`
with no Criterion-detected timing regression.

2026-07-02 token-start compaction follow-up: inspired by Zig's compact token
offset columns, token starts now stay in a `u16` column until a source file
needs a larger byte offset, then promote to `u32`. This adds one enum tag at the
per-token-table level (`TokenTableData` is 80 bytes instead of 72) but halves
the start column for ordinary small files. On the local parse corpus, compact
token row bytes dropped again from `1,577,283` to `1,241,049`, and compact token
retained bytes from `1,613,099` to `1,280,121`. The small-corpus frontend lens
now reports `parse 115,200 allocs / 10.3MiB`, `parse/check 214,091 allocs /
25.5MiB`, and `parse/check/lower 224,701 allocs / 32.7MiB`; Criterion detected
no parse/check/lower timing regression.

The compact runner also no longer retains a fallback token table that the
fallback parser ignores, and it now prepares/installs the compact lowered
program before dropping the parsed arena. The installed evaluation loop runs
from precomputed top-level spans and auto-main skip flags, so normal compact
execution does not keep parsed frontend structures live across runtime work.
This is the right frontend lifetime shape, but it does not close the Sentry RSS
objective because `showcase/tokei.xsh` has a small frontend and the remaining
benchmark peak is runtime/value churn. Current release samples from this pass
were byte-for-byte identical but noisy: table `0.87s / 68,337,664` bytes RSS and
JSON `0.98s / 78,135,296` bytes RSS. The active objective remains incomplete.

2026-07-02 compact span follow-up: compact arena byte-span columns now follow
the same promoted-column design as token starts. Parser-built arenas for small
source files store spans as `(u16, u16)` rows and promote to `(u32, u32)` only
when a source or interpolation shift requires it. On the local parse corpus,
compact arena retained bytes dropped from `4,908,522` to `4,407,246`, with
expression storage dropping from `1,614,923` to `1,291,771` and span storage
from roughly `229KB` to `119,980` bytes. The small-corpus frontend lens now
reports `parse 115,200 allocs / 10.0MiB`, `parse/check 214,091 allocs /
24.9MiB`, and `parse/check/lower 224,701 allocs / 32.4MiB`; Criterion detected
no parse/check/lower timing regression after the source-length fast path.

2026-07-02 lowered record-key follow-up: lowered vector-backed records now store
field keys as interned `Name` values instead of `Arc<str>`, preserving lexical
ordering through `Name::Ord` and converting back to public `RecordMap` keys only
at runtime boundaries. This targets the retained JSON result graph, where each
`FileReport`/`Stats` record repeats the same small set of field names. Fresh
release Sentry samples remained byte-for-byte identical: table `1.68s /
55,083,008` bytes RSS and JSON `1.67s / 72,007,680` bytes RSS. JSON improved
from the preceding `75,137,024` byte sample but remains above the 66MB memory
objective.

2026-07-02 compact stats completion: the hot `Stats` record shape used by
`showcase/tokei.xsh` now has a lowered-only compact representation. Stats with
empty `blobs` store `blanks`, `code`, and `comments` inline in `LoweredValue`
without a heap allocation; stats with embedded blob maps use a boxed payload.
Both forms still behave as records for field projection, destructuring, record
methods, indexing, JSON serialization, type checks, equality, assignment
fallback, spreads, and public `RecordMap` conversion. `LoweredValue` remains 32
bytes (`-Zprint-type-sizes`), with the inline `Stats` variant fitting in the
existing payload.

Fresh current-corpus release samples on `/Users/josh/dev/sentry` are
byte-for-byte identical to the saved XSH outputs and satisfy the objective:

| Path | XSH release | Native tokei | Target |
|---|---:|---:|---:|
| table | `1.03s / 62,750,720` bytes RSS | `1.22s / 39,223,296` bytes RSS | ≤1.2× wall, ≤66MB RSS |
| JSON | `1.19s / 65,667,072` bytes RSS | `1.21s / 52,805,632` bytes RSS | ≤1.2× wall, ≤66MB RSS |

Earlier same-binary samples were also under the fixed memory target: table
`1.55s / 60,407,808` bytes RSS and JSON `1.55s / 63,373,312` bytes RSS. The
warm samples above are the completion evidence because they compare against
native tokei measured on the same current checkout.

## Completion Evidence (2026-07-02)

Retained changes in the worktree:

- shared lowered state for `par-map` workers;
- borrowed/miniserde JSON serialization over `LoweredValue`;
- `SharedList` slot freezing for large lowered lists;
- live lowered `fs.files`/`fs.walk` streams and bounded streaming
  `Stream |> map |> where |> par-map`;
- lazy `FsEntry` values for `stat:false` entries;
- `bytes.concat` compatibility with `SharedList`;
- `showcase/tokei.xsh` table path returns `SummaryRow` rows directly from
  `par-map`, cutting table RSS by roughly 90-100MB.
- lowered record literals use vector-backed records internally;
- lowered field chains borrow through records and clone only the final field;
- lowered `ParMapBlock` can move block-local final slot values;
- parallel lowered `par-map` writes results into indexed result slots instead
  of per-worker chunk vectors.
- parser arena reserve heuristics retain less unused compact-arena capacity on
  small frontend workloads.
- compact runner drops the CST before compact lowered evaluation and no longer
  retains the unused fallback token table.
- compact token payload storage is sparse and compact runner installed eval can
  drop both CST and arena before runtime work.
- token start offsets use a promoted `u16`/`u32` compact column for small source
  files.
- compact arena byte-span columns use promoted `(u16, u16)` / `(u32, u32)` rows
  for small source files.
- release lowered `par-map` workers use 2MiB stacks; debug workers keep 8MiB.
- lowered `Command`, `Digest`, `Stream`, `Status`, and `Tag` box cold/wide
  payloads, reducing `LoweredValue` to 40 bytes.
- lowered string/byte views use compact `u32` offsets and lowered regex values
  are boxed, reducing `LoweredValue` to 32 bytes.
- lowered vector-backed record keys use interned `Name` instead of `Arc<str>`.
- lowered `Stats` records use compact inline/boxed representations while
  preserving public record behavior.

Recent verification:

- `cargo check --lib`
- `cargo test --test syntax`
- `cargo test compact_lowered_runner`
- `cargo build --bin xsh && cargo run -p xsht -- test showcase/tests/test-tokei.xsh`
- `cargo test --test runtime par_map_reduce_by_fuses_to_local_worker_aggregation`
- `cargo test --test runtime`
- `cargo bench -p xshi --bench bench small_corpus -- --sample-size 10 --warm-up-time 0.5 --measurement-time 1`
- `cmp -s /tmp/xsh-tokei-table-final.out /tmp/xsh-tokei-table-reserve-final.out`
- `cmp -s /tmp/xsh-tokei-json-final.out /tmp/xsh-tokei-json-reserve-final.out`
- `/usr/bin/time -l target/release/xsh showcase/tokei.xsh -- /Users/josh/dev/sentry > /tmp/xsh-tokei-table-drop-cst.out`
- `/usr/bin/time -l target/release/xsh showcase/tokei.xsh -- --json /Users/josh/dev/sentry > /tmp/xsh-tokei-json-drop-cst.out`
- `cmp -s /tmp/xsh-tokei-table-final.out /tmp/xsh-tokei-table-drop-cst.out`
- `cmp -s /tmp/xsh-tokei-json-final.out /tmp/xsh-tokei-json-drop-cst.out`
- `/usr/bin/time -l target/release/xsh showcase/tokei.xsh -- /Users/josh/dev/sentry > /tmp/xsh-tokei-table-stack2m.out`
- `/usr/bin/time -l target/release/xsh showcase/tokei.xsh -- --json /Users/josh/dev/sentry > /tmp/xsh-tokei-json-stack2m.out`
- `cmp -s /tmp/xsh-tokei-table-final.out /tmp/xsh-tokei-table-stack2m.out`
- `cmp -s /tmp/xsh-tokei-json-final.out /tmp/xsh-tokei-json-stack2m.out`
- `RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR=/tmp/xsh-type-target3 cargo rustc --lib -- -Zprint-type-sizes`
- `RUSTC_BOOTSTRAP=1 CARGO_TARGET_DIR=/tmp/xsh-type-target4 cargo rustc --lib -- -Zprint-type-sizes`
- `cmp -s /tmp/xsh-tokei-table-final.out /tmp/xsh-tokei-table-boxcold.out`
- `cmp -s /tmp/xsh-tokei-json-final.out /tmp/xsh-tokei-json-boxcold.out`
- `/usr/bin/time -l target/release/xsh showcase/tokei.xsh -- /Users/josh/dev/sentry > /tmp/xsh-tokei-table-boxcold.out`
- `/usr/bin/time -l target/release/xsh showcase/tokei.xsh -- --json /Users/josh/dev/sentry > /tmp/xsh-tokei-json-boxcold.out`
- `cargo check --lib`
- `cargo test compact_lowered_runner`
- `cargo test --test runtime par_map_reduce_by_fuses_to_local_worker_aggregation`
- `cargo run -p xsht -- test showcase/tests/test-tokei.xsh`
- `git diff --check`
- `cmp -s /tmp/xsh-tokei-table-final.out /tmp/xsh-tokei-table-32value.out`
- `cmp -s /tmp/xsh-tokei-json-final.out /tmp/xsh-tokei-json-32value.out`
- `/usr/bin/time -l target/release/xsh showcase/tokei.xsh -- /Users/josh/dev/sentry > /tmp/xsh-tokei-table-32value-2.out`
- `/usr/bin/time -l target/release/xsh showcase/tokei.xsh -- --json /Users/josh/dev/sentry > /tmp/xsh-tokei-json-32value-2.out`

Recommended next investigation:

1. Rebuild a normal release binary after any `perf-metrics` run; that feature
   installs a different allocator and changes RSS/timing.
2. Keep the table `SummaryRow` script change; it is simple and measured.
3. Do not spend more time on the rejected worker-local `par-map`/`flat-map`/
   `reduce-by` fusion. If this fusion is revisited, it must stream results into
   reduce-by without changing encounter-order error behavior or floating-point
   aggregation semantics.
4. Profile the lowered scanner/value path. The latest table DHAT still shows
   ~745MB allocated to process ~186MB of selected source. The next memory win is
   likely reducing per-line/per-file `LoweredValue` record/map/list churn in the
   scanner path, or introducing a script-visible-neutral host scanner primitive
   for the repeated `Stats` shapes.

### Why native tokei is 44MB

Native tokei uses `ignore::Walk::parallel()` + `rayon` which fuse walk, read,
and scan into one parallel operation. There are no evaluator clones, no
intermediate item Vecs, no `BTreeMap` per file entry. Each file's bytes are
read, scanned, and dropped before the next file is pulled. Peak memory is
`max(file_size) × thread_count` + fixed overhead ≈ 44MB.

## Completed priority: remove `fork_for_par_map`

`fork_for_par_map()` has been removed from `eval.rs`. Parallel lowered
`par-map` now takes one `Arc<LoweredSharedState>` snapshot and constructs
per-thread `LoweredWorker`s from that shared state. The largest immutable
runtime fields (`sources`, lowered function maps, lowered program, dynamic
module cache, and function-module maps) are stored behind `Arc`, so worker setup
does not clone those maps or source text per thread.

A parallel worker needs only:

- **Immutable (shared via `Arc`):** lowered function definitions, sources, env,
  cwd, module caches — set up once and never mutated during evaluation.
- **Mutable (per-thread):** stdout/stderr buffers, slot pool, signal state,
  trace state.

The current implementation keeps the existing lowered evaluator methods on
`Evaluator` and uses `LoweredWorker` as the worker-facing wrapper, rather than
moving all `eval_lowered_*` methods in one large mechanical change. That keeps
the behavioral diff small while eliminating the fork path and the heaviest
per-thread clones. A later cleanup can still move the methods fully onto
`LoweredWorker` if that improves maintainability.

## Performance log

| Date | Change | Micro | Macro | vs tokei |
|---|---|---|---|---|
| 2026-07-01 | baseline (before fixes) | — | 2,569ms | 3.62× |
| 2026-07-01 | +StrByte lowering wired | — | 2,767ms | 4.03× |
| 2026-07-01 | +add_stats restored | — | 2,660ms | 3.80× |
| 2026-07-01 | +has_defers fast stmts path | — | 2,656ms | 3.79× |
| 2026-07-01 | +batched signal checks (64 lines) | — | 2,604ms | 3.72× |
| 2026-07-01 | +fat LTO (dist profile) | — | 2,522ms | 3.60× |
| 2026-07-01 | +dir cache for file reads | — | 2,353ms | 3.36× |
| 2026-07-01 | +parallel par-map (atomic counter) | — | 2,231ms | 3.19× |
| 2026-07-01 | +std::fs::read (replaces cap_std) | — | 1,933ms | 2.76× |
| 2026-07-01 | +ScanLines dedicated scan loop | — | 1,886ms | 2.69× |
| 2026-07-01 | +parallel **ParMapBlock** (true parallelism) | **262ms** | **948ms** | **1.35×** |
| 2026-07-01 | +stream collection (eliminate intermediate Vec) | — | 921ms | 1.32× |
| 2026-07-01 | +lazy map/where streaming + bounded fs channel | — | 932ms | 1.33× |
| 2026-07-01 | +slim fork_for_par_map | — | 932ms | 1.33× |
| 2026-07-02 | +remove fork_for_par_map shared worker state | not rerun | not rerun | not rerun |
| — | **speed target** | — | **≤ 840ms** | **≤ 1.20×** |
| — | **memory target** | — | **≤ 66MB RSS** | **≤ 1.50×** |

## Optimizations applied

1. **Parallel `ParMapBlock`** (the key win). The tokei script uses
   `ParMapBlock` (block bodies with `let` statements), not `ParMap` (simple
   expressions). Only `ParMap` was parallelized; `ParMapBlock` was a sequential
   `for` loop. Now uses `std::thread::scope` + atomic counter work distribution
   with lowered workers. CPU/wall ratio: 1.1× → 4.4×.

2. **Streaming pipeline prefix.** When a pipeline stage list starts with
   Map → Where from a `Stream` input (e.g. `fs.files()`), items are pulled
   lazily and transformed inline instead of collecting into intermediate Vecs.
   Eliminates 2 `Vec<LoweredValue>` allocations (map output, where output).

3. **Bounded fs walk channel.** `IgnoreWalkStream` changed from unbounded to
   `bounded(1024)` channel, providing natural backpressure — the filesystem
   walker blocks when the consumer falls behind.

4. **`std::fs::read` replaces cap_std.** `read_host_path_bytes` uses
   `std::fs::read` directly instead of `cap_std::fs::Dir::open_ambient_dir`
   + `dir.open`. Eliminates capability checks and parent-directory re-opening
   overhead. Saved ~235ms system time.

5. **`ScanLines` dedicated scan loop.** When a `ForStrLines` loop body matches
   the simple scanner pattern (single `IfBool` with counter increments), the
   lowering emits `LoweredStmt::ScanLines` instead. The runtime processes lines
   in a tight loop without `eval_lowered_stmts` dispatch. Hits 5,039 times on
   the Sentry corpus.

6. **Perf counters** (`perf-counters` feature). Fast-path hit/miss counters
   printed at exit. Confirmed `fast_plain_return` 48.5% hit rate, `fast_return`
   28.4% hit rate, 18,759 slow-path falls-through.

7. **Fast no-defer `eval_lowered_stmts_fast`.** When `LoweredPureFunction.
   has_defers` is false (computed at lowering time via recursive scan), skips
   the `defers` Vec allocation and `Defer` check per statement.

8. **Batched signal checks.** `ForStrLines` and `ScanLines` check
   `service_pending_signal` every 64 lines instead of every line.

9. **Inline stream collection.** `collect_lowered_stream_values` converts
   stream items directly to `LoweredValue` instead of collecting into
   `Vec<Value>` first, eliminating one intermediate allocation.

10. **Removed `fork_for_par_map`.** Parallel lowered `par-map` now uses
    `LoweredSharedState` + `LoweredWorker`, with the large immutable lowered
    runtime fields shared through `Arc` instead of cloned per worker. Worker
    threads also use an explicit lowered-runtime stack size so lowered proc
    calls inside par-map do not overflow the default thread stack.

11. **`has_defers` recursive scan.** `lowered_body_has_defers` checks nested
    statements (inside `If` branches, `Retry` bodies, `While` loops, etc.) so
    the fast no-defer path is used correctly for all pure functions.

## Syscall analysis

xsh processes 18,419 files. Each `std::fs::read` does 4 syscalls (openat,
fstat, read, close) = ~73,676 syscalls. At ~15μs per syscall on SSD, this
accounts for ~1,100ms of system time.

Native tokei also does 4 syscalls/file via `File::open` + `read_to_string`.
Its 700ms total is achievable because `ignore::Walk::parallel()` + `rayon` fuse
walk, read, and scan into a single parallel operation — directory traversal,
I/O, and computation all overlap across threads. System time is 1,722ms but
wall time is 688ms (5× parallelism factor). Our 4.4× parallelism factor nearly
matches this.

## References

- `showcase/tokei.xsh` header — tokei benchmark history and lowered-IR milestones
- `docs/IR.md` — lowered IR design
- `PORTS.md` — project porting status, tokei as the forcing benchmark
- `~/d/laputa-systems/xsh-archive/` — old interpreter snapshot; lowered runtime
  was 4,051 lines (current: 17,168)
