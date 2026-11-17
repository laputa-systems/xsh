# XSH-96: Reuse checked programs across processes

Status: draft

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

## Prototype investigation (incomplete)

At `6f7da1baab34a8ebe2a6e3b5996c3f7777249d22`, a profiling build ran
`xsh core/awk.xsh -- 'BEGIN{print 1}'` three times under
`perf stat -e instructions:u`. Each run printed `1`; instruction counts were
4,569,447,715, 4,569,533,896 and 4,569,323,225 (median 4,569,447,715).
These are whole-process counts, not phase counts. One `xsht runtime-stats`
run reported process CPU times of 14.53 ms for project parse/load, 301.59 ms
for checking, 4.79 ms for declaration publication and 73.79 ms for combined
lowering, verification and installation. These times cannot substitute for
instruction counts or separate lower from verification.

No complete serialized artifact was produced, so its size, decode and verify
instruction count, and decoded execution are unmeasured. The existing raw-store
fixture captures all pools in memory, but its wire decoder supports only a
subset (`src/runtime/eval/indexed/full/formal_bridge/raw_full.rs:6`,
`:325`, `:382`). A complete encoder must also handle prepared CLI plans;
`FullStore` owns them (`src/runtime/eval/indexed/full.rs:836`), while
`CliDescriptorPlan` keeps its fields private (`src/modules/cli.rs:471`).
A scoped implementation needs a way to export and reconstruct those fields,
which is outside the prototype's permitted edit paths.

There is a separate correctness issue for the proposed trust model. The
existing test at `src/runtime/eval/indexed/full/formal_bridge/raw_full.rs:150`
changes an integer in captured IR, passes `FullVerifier`, and demonstrates a
changed result. Verification establishes IR invariants, not equivalence to the
source identified by the key. A production design therefore needs an integrity
and source-binding argument in addition to re-verification; verification alone
cannot establish that a cache entry represents the loaded source.

Execution also depends on state outside `FullProgram`: the runner uses checker
parameter types to choose the byte argument domain (`src/runner.rs:582`),
the evaluator installs tag arities and error families from compact declarations
(`src/runtime/eval.rs:4985`, `:6285`), and constructs the signature CLI plan
from checked entries (`src/runtime/eval.rs:5025`) before using it to parse
arguments (`src/runtime/eval.rs:5389`). The run plan also owns auto-main
arguments and dispatch state (`src/runtime/eval.rs:289`). This is evidence of
specific dependencies, not a completed audit of every runtime path.

A focused AWK scope probe at base `5f41e9ad381a50fd6f9e4747bc2a90048747db41`
loaded the normal closure, checked and lowered it, and ran `FullVerifier` again.
It found 855 marked resource-transfer instructions and no prepared module CLI
plans, prepared schemas, or registered calls in that program. The existing
wire decoder rejects marked resource transfers; the verifier checks their
alignment and eligible instruction kinds, not whether the checker established
each mark. A complete untrusted decoder needs an independent authentication
argument for those marks. This probe does not measure a serialized artifact or
establish a performance result.

## Open questions

1. How large is the serialized program for `core/awk.xsh`, and how long do
   decode plus verification take? This decides whether the win is the expected
   order of magnitude. A prototype that serializes one program and measures
   both is the first step.
2. Which runtime paths read checker state that is not part of the Full IR
   today? Each one must move into the cached artifact or be shown unnecessary.
3. Should `xsht check` populate the cache, so checking a script also warms its
   next run?
4. Eviction bound and default size.

## Recommendation

Defer the build decision until a complete round trip and phase instruction
measurements are available. Resolve the source-binding and integrity question
before treating the cache as an untrusted input.

## Decision

Pending. The owner asked for the design to be scoped; implementation starts
only after the prototype answers open question 1.
