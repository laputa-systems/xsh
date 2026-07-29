# Ports — foundational CLI tools as XSH forcing functions

Porting real, well-known CLI tools to XSH is how we stress-test the language and
runtime against *production-shaped* workloads and discover where the
implementation needs to get better. `tokei` proved this out: it drove most of the
lowered-IR work (see `docs/FRONTEND.md` and the header of `showcase/tokei.xsh`) and took
the default scan from ~2.3× slower than native to faster-than-native.

The goal of each new port is **diversification**: pick tools that load-bear on
*different* XSH axes than the ones already exercised, so each port teaches us
something new instead of re-proving the same thing. The axes we care about:

## Greppable implementation handles

| Workload | Symbols | Owner and coverage |
|---|---|---|
| lowered compute path | `FullBuilder::build_compact`, `indexed_run`, `Evaluator::prepare_compact_indexed_only` | `src/runtime/eval/indexed/full.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval.rs`; indexed frontend tests and benchmarks |
| tokei forcing benchmark | `xsh_tokei_json_report_assembly_4000`, `xsh_tokei_json_report_assembly_4000_execution` | `crates/xsh-multicall/benches/bench.rs`; `showcase/tokei.xsh` |
| CLI parsing and effects | `parse_cli`, `parse_cli_full`, `execute_run`, `time.measure` | `src/modules/cli.rs`, `src/runtime/run.rs`; `showcase/hyperfine.xsh` and runtime process tests |
| dynamic JSON workload | `raw_json_*`, `json.decode`, `Value` | `src/modules/json.rs`, `src/runtime/value.rs`; `showcase/jq.xsh`, `showcase/tests/test-jq.xsh` |

The port status below describes user-facing workloads; these handles identify
the runtime path and test/benchmark owner to inspect before changing one.

- **compute / text-scan** — pure lowerable loops, byte/string predicates, parallel
  `par-map |> reduce-by` aggregation. (Proven by tokei.)
- **dynamic data / allocation** — nested `Value`/`Map`/`List` churn, `Arc` traffic,
  record representation. This is the largest *remaining* perf bucket (`docs/FRONTEND.md`),
  and the lowered IR barely helps it.
- **effects / process orchestration** — `proc` bodies, subprocess spawning, timing,
  signals, env. Deliberately AST-only (never lowered), so a distinct surface.
- **interactive / TUI** — raw terminal, event loop, latency over throughput.
- **structured parsing / stream aggregation** — streaming row parsing, type
  inference, `group-by` / `sort-by` / `reduce-by`.

## Status

| Tool | Status | Primary axis | Why it's worth porting |
| --- | --- | --- | --- |
| **tokei** | ✅ done (`showcase/tokei.xsh`) | compute / text-scan | Drove the lowered IR (SCC co-lowering, borrowed `for line`, byte predicates) and the fused `par-map \|> reduce-by`. Byte-for-byte table format + exact file selection; line-classification counts are a deliberate approximation (full tokenizer parity costs the speed lead — see its header). |
| **hyperfine** | 🚧 in progress (`showcase/hyperfine.xsh`) | effects / process | Almost pure effects: spawn subprocesses, time them, warm up, compute mean/σ, compare. Exercises `proc`/`[time]`/`[fs]`, `time.measure`, `Command`, float stats, JSON export. Already forced a runtime improvement (see below). |
| **jq** | 🚧 in progress (`showcase/jq.xsh`) | dynamic data / allocation | A JSON query mini-language *interpreted on top of XSH*: constant traversal/rebuild of nested dynamic values. Directly attacks the #1 remaining perf bucket (alloc / `Value` movement). A full lexer + precedence parser + tree-walking evaluator with jq's stream/backtracking semantics, path expressions, assignment, `def`/`reduce`/`foreach`/closures, string interpolation, `@`-formats, and a regex subset. Passes **308 / 550** of jq's own `tests/jq.test` (value-compared, mirroring jq's `--run-tests`). Maximally different from tokei. |
| **fzf** | 💡 proposed | interactive / TUI | Real-time fuzzy finder: raw terminal mode, async keyboard input, incremental scoring, redraw-per-keystroke. Stresses the `tui` module, an event loop, and *latency* — a dimension no batch tool exercises. |
| **xsv / awk** | 💡 proposed | structured parsing / streams | CSV toolkit (streaming row parse, type inference, columnar `group-by`/`sort`/`join`) matures the typed parallel-aggregation path. `awk` is the spicier variant: another embedded mini-language (patterns/actions, fields, assoc arrays, string↔number coercion). |
| **ripgrep** | ⛔ rejected (for now) | compute / text-scan | Too close to tokei in spirit (walk → scan bytes → match → aggregate); same lowered surface, little new to learn. A minimal `showcase/rgrep.xsh` already exists. Revisit only if we want to push regex-engine performance specifically. |

## Suggested order

1. **jq** — attacks the documented top perf frontier (allocation / value movement)
   and is the most orthogonal to tokei.
2. **hyperfine** *(prototype landed)* — forces the effectful/process path.
3. **fzf** — forces the interactive/TUI path.

Each should grow into a forcing benchmark the same way tokei did: a real corpus or
workload, a release A/B against the native tool, and a documented list of the
runtime/IR improvements it unlocked.

## jq — feature coverage & status

`showcase/jq.xsh` is a real jq interpreter written entirely in XSH `pure` functions:
a lexer, a precedence (Pratt-style) parser into a tag-union AST, and a two-mode
tree-walking evaluator (value mode + path mode). It reads a stream of JSON values on
stdin and a jq program from argv (`-c -r -n -s -S` are accepted; `-c` is the path
exercised by the test harness).

**Test parity.** It passes **308 / 550** records of jq's own upstream
`~/d/jq/tests/jq.test`, scored by `showcase/tests/jq-score.py` — a dev driver that
mirrors jq's `--run-tests` (`src/jq_test.c`): outputs are compared by **value**
(`jv_equal`: numeric equality, order-insensitive objects), not textually, so
non-decnum number formatting (`19.0` vs `19`, `1E+3` vs `1000`) is *not* a failure.
A curated subset runs in the normal `xsht` suite as `showcase/tests/test-jq.xsh`
(14 cases across the feature axes). Run the scoreboard with
`python3 showcase/tests/jq-score.py` (also takes `base64.test`/`uri.test`).

**Implemented.** Identity/recurse/field/index/slice/iterate; pipe/comma; array &
object construction (with the full cartesian-product semantics); arithmetic with jq's
type rules (`+`/`-`/`*`/`/`/`%` over numbers/strings/arrays/objects/null, string×number
repeat, object deep-merge, array difference, string split); comparison over jq's total
order; `and`/`or`/`not`/`//`/`if-elif-else`/`try-catch`/`?`; ~70 builtins
(length, keys, to/from_entries, map, select, sort/sort_by, group_by, unique(_by),
min/max(_by), add, range, flatten, recurse, type filters, contains/inside, walk,
index/indices, …); full path machinery (`getpath`/`setpath`/`delpaths`/`paths`/
`leaf_paths`/`del`) and assignment (`=`, `|=`, `+=` family, with jq's
root-capture/first-output quirks); string interpolation `\(…)` and the `@base64`,
`@base64d`, `@base32`, `@json`, `@text`, `@csv`, `@tsv`, `@html`, `@uri`, `@sh`
formats; `def` (with value **and** filter/closure parameters, and recursion),
`reduce`, `foreach`, `EXPR as $pat` with array/object destructuring, `limit`/`nth`;
and a regex subset (`test`/`scan`/`splits`/`split/2`/`sub`/`gsub`, basic `match`).

**Runtime improvement it forced.** The first cut parsed input JSON by
`s.split("")` into a `List[Str]` and indexed it with `List.get(i)`. Because list
indexing is **not O(1)**, that made parsing **O(n²) in input size** — measured at
0.18 s / 0.68 s / 2.72 s / 11.0 s for 50 / 100 / 200 / 400 records (release), i.e. a
clean 4×-per-doubling blow-up that made the tool unusable on real files. Rewriting the
input parser to walk bytes directly via `Str.byte_at`/`Str.byte_slice` (both O(1)) —
slicing string/number tokens out to `json.decode` so multibyte UTF-8 still
round-trips — made parsing **linear**: 0.03 s / 0.03 s / 0.05 s for the same sizes,
and 0.63 s to parse a 5 000-record / 429 KB corpus. This is the concrete
"value movement / indexing cost" lesson the port was meant to surface.

**Perf A/B (release, 5 000 records, `map(select(.active).price)|add`).** xsh-jq
**1.23 s** (0.63 s of it parsing) vs native `jq` **0.01 s** — ~120× slower. That gap
is expected for a mini-language *interpreted on top of* an interpreter, and unlike
tokei it is not chasing a speed win: jq's value is the **allocation / `Value`
churn** axis. The remaining cost is dominated by (a) per-scalar `json.decode` +
`json.encode` round-trips during parse/serialize, (b) immutable `List.push`/`.extend`
rebuilding arrays as streams are materialized (eager streams mean every pipe stage
allocates a fresh `List[Json]`), and (c) cons-cell `Env` + tag-union `Json` `Arc`
traffic. These are exactly the buckets `docs/FRONTEND.md` flags; a NaN-boxed / small-value
`Value` lane and a persistent (O(1)-append) list representation would move the needle.

**Deliberate gaps (documented, not bugs).**
- **Number formatting / decnum.** All numbers are IEEE `Float`; integral values print
  without a decimal point (matching non-decnum jq). jq 1.7+ decimal **literal
  preservation** (`1.0`→`1.0`, `1E+3`), arbitrary-precision integers, and `>2^53`
  exactness are out of scope. (The value-comparing scoreboard hides most of these.)
- **Regex.** XSH's `regex` module (regex-lite) has no named groups, per-match capture
  spans, or Oniguruma extensions, so `capture`, full `match` capture objects, and
  `onig.test`/`manonig.test` features are unsupported; `test`/`scan`/`split`/`sub`/
  `gsub` are faithful.
- **Streaming & misc.** `--stream`/`tostream`, `input`/`inputs`, `$__loc__`, `env`/
  `$ENV`, dates/`strftime`, SQL-style builtins, and exact `%%FAIL` error-message text
  are not implemented. Infinite generators outside a `limit` evaluate eagerly and so
  would hang rather than stream lazily (a consequence of the eager-stream model).

**Why a self-contained JSON model.** XSH records/maps are `BTreeMap`-backed
(key-sorted — `src/runtime/value.rs`, `src/modules/json.rs`), but jq preserves object
**insertion order**. Rather than change the runtime, `jq.xsh` carries its own ordered
`Json` tag union (objects = ordered key/value pairs) and its own serializer, and only
delegates *scalar* conversion to `json.decode`/`json.encode` (which never see an
ordered object). Making native records insertion-ordered remains the alternative that
would let a future jq port use native values directly.

## hyperfine — feature gap & status

A faithful hyperfine port is the forcing function for XSH's **timing/process
primitives**, not just more script. Status:

**Runtime improvements it already forced.**
- `time.measure` returned only `{status, duration_ms}` (millisecond, wall-only) —
  too coarse to benchmark anything. It now also returns `wall_ns` (nanosecond wall
  clock) and `user_ns`/`system_ns` (child user/system CPU via
  `getrusage(RUSAGE_CHILDREN)` deltas), and takes `quiet: Bool` to discard the
  child's stdout/stderr (`run_quiet_with_policy`) so benchmarking doesn't flood the
  terminal. The CPU split distinguishes compute-bound from sleep/IO-bound commands.
- `xsh --startup` boots the interpreter and exits immediately, exposing the fixed
  startup cost as a calibration baseline (cleaner than an `xsh -c ""` probe). Measured
  at **~10.7 ms — on par with `sh -c ""` (~11.6 ms)**, so **no startup fastpath was
  needed** ("add one if needed" → it wasn't).

**Validation.** With commands run through xsh's own launcher (no `/bin/sh`), the port
now matches native `hyperfine -N` on the tokei A/B almost exactly: xsh-tokei wall
683.9 ms vs 688.4 ms (within ~0.7%), user-CPU within ~2%, same ~1.1× ratio. Wrapping
in `/bin/sh` (the earlier default) had inflated walls by ~150 ms/command because that
shell-spawn wasn't subtracted; dropping the shell removed the offset.

**Implemented in `showcase/hyperfine.xsh`:** multiple commands, `--warmup`/`--runs`,
mean ± σ (sample) + median + min … max, `[User: …, System: …]` CPU line,
σ-propagated relative-speed summary ("N ± M× faster"), direct execution via xsh's
launcher (default; tokenizes the command, no `/bin/sh`), optional `--shell S`
wrapper for pipes/globs, `--subtract-startup` (subtract the `xsh --startup` baseline,
for isolating xsh-script work), `--ignore-failure`, and `--export-json` (hyperfine's
JSON shape). `fsqrt` is a hand-rolled Newton's method because the float surface has
no `sqrt`.

**Still missing (the long tail), and what each would force:**

- **peak memory** per run — needs `wait4(pid, …, rusage)` (the `RUSAGE_CHILDREN`
  delta can't give a per-child max-RSS); a `time.measure` runtime extension.
- **outlier detection** (modified Z-score) + warnings, **median/quantile** polish.
- **adaptive run count** (`--min-runs`/`--max-runs`/`--min-benchmarking-time`).
- **parameterized benchmarks** (`--parameter-scan`/`--parameter-list` + `{var}`
  templating) — exercises string templating + the benchmark matrix.
- **more exports** (CSV, Markdown, AsciiDoc, OrgMode), `--time-unit` auto-scaling,
  `--sort`, **live progress bar** (the interactive/TUI axis).
- **prepare/setup/conclude/cleanup hooks** — per-run cache-clearing commands; more
  process-orchestration surface.

**Done since the first pass:** shell-startup calibration (now moot for the default
path — dropping `/bin/sh` removed the offset; `xsh --startup` exposes the interpreter
floor for `--subtract-startup`), and output control (`time.measure(.., quiet: true)`).

**XSH stdlib gaps still open:** `Float.sqrt` (worked around with `fsqrt`) and
per-child max-RSS (needs `wait4`).
