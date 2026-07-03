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

### 2026-07-02 progress: all three gates met

Same command and corpus, measured after the frontend allocation-reduction pass
below (multiple 30-sample/5s-measurement-time and the documented
10-sample/1s-measurement-time runs on the same local machine; the mean varies
a little run to run with background load, so a range is given):

| Lens | Mean | Allocs | Allocated | vs fresh baseline |
|---|---:|---:|---:|---|
| parse | `9.7 ms`-`10.3 ms` | `61,230` | `5.4 MiB` | time -28% to -32%, allocs -46.9%, bytes -46% |
| parse/check | `19.0 ms`-`20.4 ms` | `160,164` | `20.4 MiB` | time -13% to -20%, allocs -25.2%, bytes -18.4% |
| parse/check/lower | `21.1 ms`-`22.1 ms` | `164,097` | `19.3 MiB` | time -20% to -24%, allocs -26.7%, bytes -40.4% |

Gate status against the reasonably ambitious bar:

- Criterion mean **<= 22.0 ms**: **met**. `parse_check_lower` mean is
  `21.1 ms`-`22.1 ms` across repeated runs; the documented
  `--sample-size 10 --warm-up-time 0.5 --measurement-time 1` command gave a
  mean of `21.68 ms`-`21.77 ms` across three consecutive runs (one run's upper
  Criterion confidence bound touched `22.6 ms`, but the gate is on the mean,
  not the CI edge, and 30-sample/5s runs were consistently `21.1 ms`-`22.1 ms`).
- Allocation audit **<= 190,000 allocations**: **met with a wide margin**,
  `164,097` (13.6% margin).
- Allocation audit **<= 27.5 MiB allocated**: **met with a very wide margin**,
  `19.3 MiB` (30% margin).
- No compensating regression in narrower lenses: **met**. Both parse-only and
  parse/check *improved substantially* (see table above) rather than
  regressed; neither needed the 5%-worse tradeoff allowance.
- Front-end behavior gate: **met**. `cargo check --workspace --all-targets`,
  `cargo test --lib`, `cargo test --test syntax`, `cargo test --test runtime`
  (357 passed, 27 pre-existing ignores), `cargo test -p xsht` (formatter/editor
  suite, since the final change touches CST consumers there), and
  `git diff --check` all pass with no diagnostics or output changes on the
  corpus. A debug build of `xsh` was also run against real checked-in scripts
  (`core/seq.xsh`, `tests/xsh/stdlib/args.xsh`) and a release build was
  `cmp`'d against the saved `showcase/tokei.xsh` XSH-vs-XSH reference output on
  the Sentry checkout (byte-identical) as an extra safety check, since the
  change that closed the gate touches the shared script-loading path every
  `xsh` invocation uses, not just this benchmark.

What changed, roughly in the order it was found, from smallest to largest
impact (the syntax-tree deferral at the end is what closed the time gate):

- **`LoweredExpr`/`LoweredStmt` enum right-sizing (the largest allocated-bytes
  win).**
  `size_of::<LoweredExpr>()` was `224` bytes and `size_of::<LoweredStmt>()` was
  `472` bytes, because Rust sizes an enum to its largest variant and two rare
  variants were inflating every node: `ListComp`/`MapComp`'s `target:
  LoweredCompTarget` inlined a 4-element `SmallVec` (~176 bytes) for the
  record-destructuring-target case, and `LoweredStmt::AssignIndex` was the only
  statement variant holding two inline `LoweredExpr`s at once. Boxing
  `LoweredCompTarget`, boxing `LoweredExpr::Error`'s `LoweredErrorExpr` payload,
  and boxing both `LoweredExpr`s in `AssignIndex` dropped `LoweredExpr` to `112`
  bytes and `LoweredStmt` to `184` bytes — no consumption-site changes were
  needed beyond the two construction sites and one enum definition each, since
  every reader already worked through a `&LoweredExpr`/`&LoweredCompTarget`
  reference and `&Box<T>` derefs to `&T` for free. This is why `parse/check/lower`
  allocated bytes dropped 26% while allocation *count* barely moved: boxing
  doesn't change how many `Box`/`Vec` allocations happen, it changes how big
  each one is, and `LoweredExpr`/`LoweredStmt` nodes dominate the lowered IR's
  node count.
- **Missing `Vec` capacity hints in the lexer, token table, and CST (the
  second-largest win, mostly allocation *count*).** `Lexer::new` built its
  `TokenTableBuilder` via `::default()` (capacity 0) instead of sizing it from
  `source.len()`; `TokenTableBuilder::with_capacity` sized `tags`/`starts` but
  left `payloads` at `Vec::new()`; the string-literal decode buffer in
  `lex_string` started at `Vec::new()` even though the raw content span is
  always a safe upper bound. `SyntaxTree::from_token_table`'s `tokens`/`trivia`
  vectors and every group/root `SyntaxNode`'s `children` vector (one per
  paren/brace/bracket/interpolation, growing from a capacity of 1 that only
  covered the opening token) had the same problem. Reserving capacity from the
  already-known token count (exact for `tokens`, a measured ~14% ratio for
  `trivia`, a fixed small constant for group `children` since nesting depth
  bounds it, not file size) was the single biggest allocation-*count* win of
  the pass: fixing just the group/root `children` vectors dropped
  `parse_check_lower` allocations by about 15,000 by itself. Bumping these
  capacity constants further (past what's documented in code) was tried and
  measured net negative or flat more than once — `Vec::with_capacity(n)` with
  `n >= 1` always allocates immediately, so over-provisioning a rarely-used
  field can *add* an allocation to files that never touch it, and even for
  always-used fields (root/group `children`) there is a real sweet spot: a
  children capacity of `12` measured *worse* wall time than `8` despite having
  fewer allocations, most likely from the larger unconditional allocation
  outweighing the avoided reallocs. Any capacity constant here should be
  re-validated by benchmark, not reasoned about in the abstract.
- **`compact_body`'s `expr_types: FxHashMap<ExprId, Type>` capacity hint.**
  `probe_compact_bodies` built this map from `FxHashMap::default()` and grew it
  one `insert` at a time; reserving `program.stats().expressions` up front (an
  exact, already-known upper bound, and one that's safe unconditionally since
  almost every real script has typed expressions) dropped this phase's
  allocated bytes from `8.17 MiB` to `5.65 MiB` corpus-wide on its own.
- **A redundant `.clone()` in function lowering.**
  `lower_compact_root_function_sweep` and `lower_compact_function_sccs` in
  `src/runtime/eval/lower.rs` each built a `top_level_known` map via
  `compact_function_top_level_known(...)` and then cloned it into the
  `CompactLowerConstructProbe` even though the original binding was never used
  again afterward — a plain move was correct. Fixed both sites. This is a real,
  zero-risk fix but a small one on its own; it's listed for completeness, not
  because it moved the needle much.
- **Field/method name interning instead of allocation.**
  `LoweredExpr::Field`/`Method` stored `name: String`, built via `name.to_string()`
  from an already-interned `Name` at lowering time — but `Name::as_str()`
  already returns a `'static` string slice for free (the symbol interner leaks
  its backing storage), so storing `&'static str` directly removes that
  allocation entirely. Measured impact was smaller than expected (this
  corpus's stdlib-call-heavy code goes through qualified-call lowering more
  than bare `.field`/`.method()` forms), but it was a clean, low-risk fix (18
  call sites, all mechanical `&&str` deref/coercion fixups caught by the
  compiler) so it stayed.
- **Deferred (lazy) syntax-tree construction — this is what closed the time
  gate.** `Parser::parse_arena_only`/`parse_into_arena_builder` always built a
  full `SyntaxTree` (one entry per token for `tokens`/`trivia`, one node per
  bracket/paren/brace/interpolation group) as a *second* complete pass over
  every token, in addition to the arena/AST construction the compact pipeline
  actually needs. Grepping every consumer of `ArenaParseOutput`/`ArenaParseFragment`'s
  `cst` field turned up exactly one real one: the `xsht` formatter/editor
  (`crates/xsht/src/format.rs`, `crates/xsht/src/edit.rs`). Every other
  reference was either a test assertion or a same-crate accessor with no
  caller — and tellingly, `src/runner.rs` (the real `xsh` binary's script
  entrypoint) destructures the parse output specifically to `drop(cst)`
  immediately without ever reading it. So the CST was being built in full on
  *every* real script run and thrown away unread, not just in this benchmark.
  Replaced the eager `cst: SyntaxTree` field with `cst: LazyCst`
  (`src/syntax/cst.rs`): `LazyCst` captures the already-computed `TokenTable`
  and an `Arc<str>` of the source (cheap — no per-token walk), and builds the
  real `SyntaxTree` only in `.get()`, cached behind an `Arc<OnceLock<..>>` so
  clones share one cache instead of rebuilding or forcing early. Every real
  consumer already worked through a reference (`&SyntaxTree`/method calls), so
  the fix was mechanical: change the field type, then follow the ~15 compiler
  errors across `src/loader.rs`, `crates/xsht/src/{format,edit}.rs`,
  `tests/syntax.rs`, and `tests/helpers/parse_corpus_report.rs`, adding `.get()`
  at the handful of sites that truly need the built tree. `src/loader.rs`'s
  own module-loading path (`parse_load_entry_source_arena_only` and friends)
  moves the `LazyCst` value opaquely from fragment to output without reading
  it, so it got the deferral for free. This single change took
  `parse_check_lower` from `~23.4 ms`-`23.9 ms` to `~21.1 ms`-`22.1 ms` and
  allocations from `185,335` to `164,097` — bigger than every other fix in
  this pass combined, because it removes an entire second traversal of the
  token stream rather than shrinking or better-provisioning one. Verified
  beyond the standard gate: `cargo test -p xsht` (the actual CST consumer)
  still passes, a debug `xsh` build still runs real checked-in scripts
  correctly, and a release build's `showcase/tokei.xsh` output against the
  Sentry checkout is still byte-identical to the saved reference — this
  touches the shared script-loading path every `xsh` invocation goes through,
  not just this benchmark, so it warranted checking beyond the corpus tests.
- **What was investigated and considered but not needed:** the O(n²)-shaped
  repeated rescan in `compact_function_top_level_known` (recomputing top-level
  bindings from scratch for every root function) is real but its marginal cost
  is small on this corpus specifically, because most files have few top-level
  `let`/`var`/`use` statements before their functions (the redundant work
  collapses to cheap no-op iterations, not expensive ones); a correctness-safe
  fix requires interleaving the top-level statement walk with function
  lowering inside the fixed-point sweep, which was judged more invasive than
  warranted once the deferred-CST fix above closed the time gate on its own.
  `LoweredPattern` (`184` bytes, same `SmallVec`-inlining shape as the enums
  above) was checked but not touched: it's only ever held inside `Vec`s, so
  its size doesn't inflate `LoweredExpr`/`LoweredStmt`, and match patterns are
  a small fraction of total node count on this corpus. Frontend arena-builder
  capacity hints beyond the ones already tuned by prior work
  (`record_field_inputs`, `call_arg_inputs`, etc. staging buffers) were
  checked and are healthy already: instrumented `len()`/`capacity()` at
  `finish()` time showed 60-73% utilization on the existing divisors, and the
  staging buffers that back nested call/record-field parsing are
  nesting-depth-bounded, not file-size-bounded, so they stay small regardless
  of file length.

None of this work — including the deferred-CST fix, which changes the shared
script-loading path — moved `showcase/tokei.xsh`'s stretch-goal numbers
meaningfully in either direction (see that section below) — expected, since a
one-time parse/check/lower cost for one small script is a rounding error next
to walking and aggregating over a large real corpus at runtime, and the
deferred CST was already being dropped unread on that path before this fix
too (`runner.rs`'s `drop(cst)`), just after paying to build it first.

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

A cross-check after the 2026-07-02 small_corpus frontend pass above (see that
section for what changed) confirmed those frontend/lowered-IR changes do not
move this stretch goal's numbers meaningfully in either direction: fresh serial
samples were table `0.84s-0.89s / 51,937,280-57,425,920` bytes max RSS and JSON
`0.92s-0.94s / 60,030,976-65,470,464` bytes max RSS, both within the noise band
of the samples above, and output stayed byte-identical to the saved
`table-record-sort-once-b.txt`/`json-record-sort-once-b.json` reference via
`cmp`. Native tokei on the same checkout was `0.65s-0.68s / 46,252,032-51,265,536`
bytes max RSS in the same session. This is expected: `tokei.xsh`'s one-time
parse/check/lower cost is a rounding error next to walking and aggregating
~140,000 files at runtime, so shrinking that one-time cost (and even shrinking
the lowered `LoweredExpr`/`LoweredStmt` IR nodes themselves) barely touches
peak RSS, which is dominated by runtime-side file/record/buffer data, not by
the compiled program's own footprint.

The implementation details for the current compact frontend and lowered runtime
architecture belong in `docs/FRONTEND.md`.

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

Verification from the 2026-07-02 small_corpus frontend allocation-and-time
pass (see that section above for what changed; the final, gate-closing change
was deferring `SyntaxTree` construction, which touches the shared
`src/loader.rs`/`src/runner.rs` script-loading path, so the checklist below
goes beyond the corpus benchmark/tests):

- `cargo bench -p xshi --bench bench small_corpus -- --sample-size 30
  --warm-up-time 1 --measurement-time 5` (repeated; used for the progress
  numbers above, since the documented `--sample-size 10 --measurement-time 1`
  command is noisy enough on this machine to swing several percent run to run
  — though it still gives a mean under the 22.0 ms bar across repeated runs)
- `cargo check --workspace --all-targets` (not just `--lib`, since the CST
  deferral changes a field used by the separate `xsht` crate)
- `cargo test --lib` (235 passed)
- `cargo test --test syntax` (84 passed)
- `cargo test --test runtime` (357 passed, 27 pre-existing ignores)
- `cargo test -p xsht` (formatter/editor suite; the real production consumer
  of the now-deferred `SyntaxTree`)
- `cargo test --test runtime json`
- `cargo test --test runtime flat_map_identity_reduce_by_matches_explicit_rows`
- `cargo test --test runtime live_stream_par_map_flat_map_reduce_by_matches_collected_rows`
- `cargo test --test runtime live_stream_par_map_for_loop_matches_collected_rows`
- `cargo test --lib map_empty_constructor_lowers_record_builder`
- `cargo test --lib lowered_self_collection_assignment_preserves_aliases`
- `git diff --check`
- `cargo build --bin xsh`, then ran real checked-in scripts directly
  (`core/seq.xsh -- 1 5`, `tests/xsh/stdlib/args.xsh`) to confirm the deferred
  `LazyCst` doesn't change script execution
- `cargo run -p xsht -- test showcase/tests/test-tokei.xsh`
- `cargo build --release --bin xsh`, then fresh serial `/usr/bin/time -l
  target/release/xsh showcase/tokei.xsh -- ...` table/JSON samples on
  `/Users/josh/dev/sentry`, `cmp`'d against the saved
  `table-record-sort-once-b.txt`/`json-record-sort-once-b.json` reference
  (byte-identical both before and after the `LazyCst` change; see the
  stretch-goal section above for the numbers)

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
  `SharedList` freezing, documented in `docs/FRONTEND.md`.
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

- `docs/FRONTEND.md` - current compact frontend, lowered runtime architecture,
  lowered IR design, and verification guidance
- `perf/README.md` - frontend and runtime profiling commands
- `showcase/tokei.xsh` - benchmark script and output-shape constraints
