# XSH-96: Reuse checked programs across processes

Status: declined. The prototype measured a 5.57-fold startup cut against the
10-fold bar (open question 1), and verifying the decoded program dominates
the cached start. Reopen only with a cheaper verifier for decoded programs
that also checks transfer marks independently.

## Summary

Every `xsh SCRIPT` process parses, loads, checks and lowers the script and its
whole import closure before it runs a line. For large scripts such as the core
applets this preparation dominates the run. This RFC proposes a persistent
cache of the *verified indexed program*, keyed by everything that can change
it. A later run with the same key decodes the program and verifies it again
instead of checking and lowering. Verification stays mandatory, so the cache is
an untrusted input like any other program source.

## Motivation

- **Startup.** `xsh core/awk.xsh -- 'BEGIN{print 1}'` costs 4.34 G user
  instructions on the release build after the stream-origin convergence change
  (`docs/PERFORMANCE-INFERENCE.md`, "Stream origin transfer closure"). The
  profile before that change attributed 83.94% of samples to module
  convergence and 66.22% to function-body checking; runtime verification was
  5.48% and scheme verification 3.78%. Nearly all of the remaining startup is
  checking and lowering work that does not change between runs of an unchanged
  script.
- **Test suite.** The 2026-11-17 suite audit found that per-process startup
  dominates the largest native test files. `core/tests/test-awk.xsh` alone was
  18.6% of native case time (3,192 s), and roughly 70% of each AWK case was
  startup rather than AWK work; `test-sed.xsh` (1,009 s), the `find`, `patch`,
  `ls`, `expr` and `numfmt` corpora have the same shape. These tests spawn the
  same unchanged applet thousands of times.
- **Users.** Core commands are XSH scripts. A command that pays a second of
  checking on every invocation is unusable in a shell loop.

## Design

### What is cached

The cached artifact is the program that `FullBuilder` produced and
`FullVerifier` accepted, plus the checker output that execution and the
diagnostic stream still need: published warnings with their spans and source
identities, the entry's argument domain and CLI plan, and the source map
entries needed to render runtime errors and traces. Tool-only facts (reveal
types, annotation facts, lint inputs) are not cached; `xsht` keeps checking
from source.

### Key

The key is a digest over:

- the build identity of the running `xsh` binary (a value fixed at build time
  that changes with any code change, not the version string);
- the canonical path and content digest of every source file in the loaded
  closure, in load order, including embedded standard-library modules;
- the check options that affect the program (`CheckOptions` fields used for
  execution) and the selected argument domain;
- any module-resolution input that changes which file an import names
  (`XSH_MODULE_PATH`, project roots).

A run computes the closure's digests during loading, which already reads every
source. Loading stays; checking and lowering are what the cache skips. A hit
therefore still pays parse and load. If measurement shows parse/load is a large
share after the cache lands, a later change can key on file metadata first.

### Storage

Entries live under `$XDG_CACHE_HOME/xsh/programs/` (default
`~/.cache/xsh/programs/`), created mode 0700, one file per key, written to a
temporary file in the same directory and renamed into place. Concurrent writers
of the same key race harmlessly: both entries are identical. The cache is
bounded by total size with least-recently-used eviction at write time.
`XSH_PROGRAM_CACHE=off` disables reading and writing, for reproducible
measurement and debugging.

### Format

The format is private to one build identity: an entry from another build is a
miss, so no compatibility or migration is needed. It serializes the indexed rows
and pools that `FullBuilder` already produces. Decoding produces the same
in-memory Full IR, and `FullVerifier` runs on it before installation, exactly as
for a freshly lowered program. A decode or verification failure deletes the
entry and falls back to the full pipeline; it never runs a partially trusted
program.

### Invariants kept

- "Verify the indexed program before installation or execution" holds on every
  path.
- "Make each semantic decision once" holds: the cache stores the decisions; it
  never re-derives them differently.
- Diagnostics: a hit replays the cached warnings in the same order with the
  same spans, so output is identical to a miss.

## Alternatives considered

- **Embed precompiled core applets in the binary at build time.** It gives the
  same startup win for shipped commands and needs no cache directory, but it
  does nothing for user scripts, ties the binary size to the applet set, and
  still needs the serialized format. It can be layered on the same format later
  by shipping pre-populated entries.
- **Cache checked module records instead of the lowered program.** The checker's
  module facts are large, mutable during convergence and not designed for
  serialization; lowering would still run on every start.
- **A resident server that keeps programs warm.** It removes parse and load as
  well, but adds a daemon, its lifecycle, and a cross-process trust boundary
  for effects and signals. Out of proportion for the problem.
- **Run many applet invocations inside one test process.** It would speed the
  test suite only, and stop testing the real process boundary.

## Prototype investigation

At `6f7da1baab34a8ebe2a6e3b5996c3f7777249d22`, a profiling build ran
`xsh core/awk.xsh -- 'BEGIN{print 1}'` three times under
`perf stat -e instructions:u`. Each run printed `1`; instruction counts were
4,569,447,715, 4,569,533,896 and 4,569,323,225 (median 4,569,447,715).
These are whole-process counts, not phase counts. One `xsht runtime-stats`
run reported process CPU times of 14.53 ms for project parse/load, 301.59 ms
for checking, 4.79 ms for declaration publication and 73.79 ms for combined
lowering, verification and installation. These times cannot substitute for
instruction counts or separate lower from verification.

At `bd048949d`, a test-only prototype
(`src/runtime/eval/indexed/full/formal_bridge/raw_full/program_image.rs`)
performs a complete round trip. It prepares `core/awk.xsh` as the runner does
and encodes every Full store pool, the semantic tables, the function
definition spans, the closure's source names and BLAKE3 digests, the live
symbol table, and the tag arities and error families the entry installs. The
round-trip test then drops the evaluator, run plan and program, loads the
closure again, decodes, runs the unchanged `FullVerifier::verify`, installs the
program and recomputes the run plan from the loaded arena as preparation does
(`program_image.rs:1204`). `BEGIN{print 1}` gives the ordinary run's status,
stdout and stderr, and re-encoding the decoded rows reproduces the image
bytes. A second process decodes an image written by a first process and
prints `1`.

The prototype encoder rejects what it does not encode: sealed operation,
method and wrapper bindings, prepared schemas, module call plans, registered
call plans (`program_image.rs:972`), signature CLI entries and byte-argument
entries (`program_image.rs:1084`). The AWK program contains none of them. The
older JSON wire decoder supports only a subset
(`src/runtime/eval/indexed/full/formal_bridge/raw_full.rs:6`, `:350`, `:407`).
A general-purpose encoder must also handle prepared CLI plans; `FullStore`
owns them (`src/runtime/eval/indexed/full.rs:836`), while
`CliDescriptorPlan` keeps its fields private (`src/modules/cli.rs:471`).
A general-purpose implementation needs a way to export and reconstruct those
fields.

There is a separate correctness issue for the proposed trust model. The
existing test at `src/runtime/eval/indexed/full/formal_bridge/raw_full.rs:175`
changes an integer in captured IR, passes `FullVerifier`, and demonstrates a
changed result. Verification establishes IR invariants, not equivalence to the
source identified by the key. The prototype rejects an image whose source
digests differ from the loaded closure, but nothing authenticates the image
bytes themselves. A production design therefore needs an integrity and
source-binding argument in addition to re-verification; verification alone
cannot establish that a cache entry represents the loaded source.

Execution also depends on state outside `FullProgram`: the runner uses checker
parameter types to choose the byte argument domain (`src/runner.rs:582`),
the evaluator installs tag arities and error families from compact declarations
(`src/runtime/eval.rs:4985`, `:6285`), and constructs the signature CLI plan
from checked entries (`src/runtime/eval.rs:5025`) before using it to parse
arguments (`src/runtime/eval.rs:5389`). The run plan also owns auto-main
arguments and dispatch state (`src/runtime/eval.rs:289`). The prototype carries
the tag arities and error families and rebuilds the run plan; it rejects the
byte and signature CLI cases instead of carrying them. This is evidence of
specific dependencies, not a completed audit of every runtime path.

The verified AWK program carries 855 checker-marked resource transfers. The
older wire decoder rejects marked resource transfers; the verifier checks their
alignment and eligible instruction kinds, not whether the checker established
each mark. The image carries the marks and the verifier accepts them. A real
cache must have the verifier independently check transfer marks; without that
change the cached program cannot be treated as untrusted input. The
measurement below assumes the carried marks are true, so it is an upper bound
on the speedup, not a safe cache design.

The Full IR also depends on process-local identities. A `Name` in an
instruction payload or a semantic type row is the process interner's symbol
number (`src/runtime/eval/indexed/full.rs:20640`;
`src/runtime/eval/indexed/semantic.rs:1634`, `:1667`, `:1873`). The prototype
records the 1,505 live dynamic symbols at write time. After loading, it
re-interns them at the same numbers, filling gaps and failing on any mismatch
(`program_image.rs:1050`). This works in a fresh process because loading
interns deterministically. It does not check that each payload name is in the
recorded table, and it cannot install a program into a process whose
interner already holds other names at those numbers. The scheme callable
owner and the binding namespace are process counters, so decoding draws fresh
ones. A production format needs names that do not depend on interning order.

## Open questions

1. How large is the serialized program for `core/awk.xsh`, and how long do
   decode plus verification take? This decides whether the win is the expected
   order of magnitude. Answered below: the image is 1,133,229 bytes, and a
   cached start takes a median 791,239,144 user instructions against
   4,409,608,312 for the ordinary start, 5.57 times fewer. That is short of
   the order-of-magnitude bar: **no-go** for this design.
2. Which runtime paths read checker state that is not part of the Full IR
   today? Each one must move into the cached artifact or be shown unnecessary.
3. Should `xsht check` populate the cache, so checking a script also warms its
   next run?
4. Eviction bound and default size.

### Measurement for open question 1

The release profile (O2, thin LTO) built the library test binary at
`bd048949d` for the x86_64 glibc host. Five interleaved runs per side of the
two ignored tests below ran under `perf stat -e instructions:u`. Every run
printed `1`, passed, and exited 0, and `perf-gate-ready -1` exited 0 before
and after each run. Both sides run in the same test binary with its counting
mimalloc allocator. Loading the closure is paid on both sides.

| Start | Median user instructions | Range |
|---|---:|---:|
| (a) ordinary: load, check, lower, verify, install, run | 4,409,608,312 | 4,409,525,055–4,409,752,835 |
| (b) image: load, decode, verify, install, run | 791,239,144 | 791,237,207–791,239,430 |

The cached start costs 17.9% of the ordinary start, a 5.57-fold reduction.
The order-of-magnitude bar needs at most 440,960,831. A test process that
selects no test costs 1,578,158 instructions; subtracting it from both sides
gives 5.58. Five runs of the release `xsh` binary built from the same source
took a median 4,358,483,017 (range 4,358,433,421–4,358,645,284), so the test
binary's ordinary start is within 1.2% of the product's.

Three more ignored tests stop the cached start after a stage. Their medians
over five interleaved runs split (b) as follows; each stage's range was within
2,582 instructions.

| Stage | Added user instructions | Share of (b) |
|---|---:|---:|
| Test process, image read, load of the closure to the arena | 221,679,594 | 28.0% |
| Decode: source digests, symbol restoration, all pools | 19,274,826 | 2.4% |
| Verify the decoded program | 486,887,221 | 61.5% |
| Install, rebuild the run plan, run `BEGIN{print 1}` | 63,397,503 | 8.0% |

The largest pools are `extra` (346,680 bytes), `data` (202,492), `blocks`
(147,704), `locations` (78,228), `scheme_schema_expectations` (49,065),
`location_sources` (39,116) and the symbol table (30,283).

Verification dominates the cached start. A start without it would cost
304,351,923 (14.5-fold), but verification stays mandatory. Removing parse and
load entirely would still leave 571,137,708 (7.7-fold). At the bar, with
loading unchanged, verifying the decoded program could cost at most about
137 M instructions, 28% of its measured cost. Closing the transfer-mark gap
adds verifier work, so it can only lower the measured 5.57-fold gain.

To repeat the measurement, build with `cargo test --release --lib --no-run`
and set `BIN` to the test executable it reports.

```sh
T=runtime::eval::indexed::full::formal_bridge::raw_full::program_image
export XSH_PROGRAM_IMAGE="$PWD/target/awk.image"
"$BIN" --exact "$T::awk_program_image_write" --ignored
perf stat -e instructions:u "$BIN" --exact "$T::awk_program_image_checked_start" --ignored --nocapture --test-threads=1
perf stat -e instructions:u "$BIN" --exact "$T::awk_program_image_cached_start" --ignored --nocapture --test-threads=1
```

The stage tests are `awk_program_image_cached_load`,
`awk_program_image_cached_decode` and `awk_program_image_cached_verify`.

## Recommendation

Do not build the cache on the current verifier. The prototype answers open
question 1 at a 5.57-fold gain, below the order-of-magnitude bar, and
verification of the decoded program is 61.5% of the cached start. A later
attempt is worth measuring only if verifying a decoded program becomes several
times cheaper. Any later go remains conditional on the verifier checking
transfer marks independently. It also needs names that do not depend on
interning order and an integrity argument for the image beyond verification.

## Decision

Pending. Open question 1 is answered below the bar. The owner decides whether
to pursue cheaper verification, accept a smaller gain, or close XSH-96.
