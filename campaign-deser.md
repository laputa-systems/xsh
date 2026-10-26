# Replace XSH serialization with a pruned, vendored Deser

Status: executing 2026-10-26 on `master` under the owner's explicit launch,
including repo-local commits and final manual verification. No push is authorized.

Goal: use Deser for structured-data parsing and emission while XSH continues to
own values, schemas, effects and resources. Vendor only the useful codec family;
make one hard cutover and delete the replaced implementations.

Current checkpoint: implementation through S5 is integrated; final cost and
Full/Linux qualification remain. Native macOS qualification is externally
blocked by the unavailable host. No campaign completion is claimed.
The measured production implementation at `89d2645eb` keeps compact shared records, one
sequence reader type and canonical documentation loaded only on request.
Public API construction still validates documentation eagerly; clones retain
their original documentation catalog. The focused registry tests pass 20/20.
The Linux xsh constructor disables eager arena commitment before allocation,
preserving explicit allocator environment overrides and the original purge delay.
The constructor object links only into xsh. Pinned production checks confirm its
initialization order, arena protection, child environment and missing-descriptor
behavior. Its exact direct sys dependency supplies the already-locked mimalloc
header; it adds no package or transitive feature.
Production sizes pass: xsh 27,212,992, xsht 39,115,392 and
xshi 44,232 bytes.

Matched memory sampling now verifies both executable files are evicted, then
fully resident after identical warming before each pair. A control on the same
frozen artifacts establishes that prior cache history changes file-backed RSS;
warming alone did not make the previous startup comparison equivalent. Tokei
can delay eviction eligibility for about 50 ms after exit. Preparation waits at
most one second for verified zero residency and still rejects persistent
residency; it never retries a workload or accepts an unmet precondition.
All eight unchanged peak and both retained-memory cases pass after this control.
Five runs each also complete an exact 64 MiB document, reject its one-byte
overrun and keep stream memory bounded at 64/256 MiB and the stream overrun.
All outputs and statuses match. The 18-workload CPU batch accepts 130 samples
with successful readiness checks before and after. Its remaining failure is
large JSON: 198.466 to 226.633 ms median across five alternating pairs.
The 28.167 ms increase exceeds the 20.208 ms allowance from the paired spread.
Every pair regresses, by 17.727 to 37.935 ms. A separate phase probe identifies
read/decode as the strongest lead and encoding as a secondary contributor.
Build CPU qualification has not run. Earlier timing results below are historical.
The owner is replacing BTreeMap and BTreeSet with bbtree on master. Keep the
pre-cutover baseline frozen and take final measurements after those edits settle.
The measurements above qualify only the recorded production source.

The grammar repair passes 43/43 checks, including 5,398 shallow and 20,097 deep
generated sentences and the parsed repository corpus. It also repairs the
spawn-run boundary and lets guard conditions consume terminal builder/pattern
payloads before else. The exact former guard failure now prints checked under
the matching verification tool; the obsolete guard negative is retained as a
positive. The generated grammar reference is refreshed. Full passes its
238-test integration aggregate, then exposes missing filesystem-boundary
registrations. `e9090ccb1` registers the two admitted codec adapters and retires
the pure INI allowance; the focused policy checks pass 2/2. Production effects
and cold authority checks are unchanged. The latest Full run passes integration
238, filesystem policy 2, symbol plateau 1 and core boundaries 23. It then fails
eight core compatibility cases because `e78cd5afa` rejects a valid inferred
iteration receiver. `bc267943b` narrows that diagnostic to propagation from an
authenticated Err constructor. Its 37 focused debug checks preserve cold
contracts and both execution routes. Pinned compatibility and fuzz checks remain
pending in an isolated checkout. Full's remaining owners and the isolated Linux
resource case are pending. Final qualification must cover the combined master.
The owner separately authorized integrating the optional-module-call compiler
repair; `cadc4468b` preserves checked result/effect contracts and its focused
cold-authority and native regressions pass. The unchanged Laputa xinit help
command passes with that repair; Laputa lifecycle assertions remain unverified
and are excluded from this campaign by the owner.

S5 adds five list/stream operations with filesystem effects only for read_all.
The frozen codec check passes 34 tests, including 18 sequence cases covering
lazy construction, reader close, recovery, host errors, frame/input/item/output
limits and cumulative YAML alias replay. YAML passes 12 unit, 81 integration,
402 upstream corpus and 23 doc tests; the bounded core reader passes three.
Binary scalar guards retain CBOR 8 unit/105 integration and MessagePack 6 unit/
86 integration passes. Negative controls reproduce the former semantic scalar,
discarded block-break, alias-reset, frame-context and buffer-growth defects.
Independent final source review finds no remaining issue. Pinned native tests
run as UID 1000 pass sequence 4, file boundaries 9, binary 6, generic codec 3,
configuration 6 and API 67, with no skips. These retain the existing byte concat,
invariant List[Any] and explicit try contracts. The API inventory adds exactly
five codec functions, with no unrelated delta. No global acceptance is claimed.

The matched production builds use the pinned container/nightly, static musl,
x86-64-v3, fat LTO and the distribution feature set. Baseline sizes are xsh
26,184,512, xsht 37,574,016 and xshi 44,232 bytes. Initial S5 sizes are xsh
27,511,744, xsht 39,158,144 and xshi 44,232 bytes. Xsh exceeds its 5% allowance by
18,007 bytes; one bounded reader-monomorphization investigation is open. Both
distribution commands compile successfully but their existing xshi --help smoke
fails because that Linux feature set intentionally builds the disabled shell
stub. Do not claim distribution smoke or any timing acceptance from these runs.
The initial matched large-JSON transform also fails memory acceptance: peak RSS
is 178,782,208 versus baseline 132,894,720 bytes with identical complete output.
A decode-only quiescent probe retains 146,296,832 versus 96,124,928 bytes.
Baseline objects use the existing compact RecordMap shape owner; the new decode
sink retains dynamic B-trees for completed objects. `958892462` restores the
existing shared shape owner for completed objects; its 35 codec tests pass.
Review caught the public host INI adapter's ownerless path; `ca79546be` scopes
symbols at that entry point and removes the narrower project-only patch. The
public lifetime regression fails before and passes after; project 5, encoder 5
and typed/cold 2 pass. Final focused source review has no open finding.
The private reader-boxing candidate passes 34 codec tests and shares five typed
pull-loop implementations instead of fifteen reader-specific ones; its actual
production size and runtime effects remain unmeasured. The final matched build
is running on frozen `ca79546be`; an earlier rebuild was stopped when the public
boundary repair made its source stale. Generated SPEC and
stdlib references are refreshed through the same documentation generator used
by make docs, using the matching verification tools admitted by the Full owner.

Acceleration checkpoint: the complete lint-invariance partition passes all 11
selected tests. The bounded corpus check passes with package optimization level
1, debug assertions and overflow checks retained. The same lowering group
reports 1,509 passes and eight failures: five incorrect test expectations are
corrected by `265832e60`; three baseline generic filesystem failures are fixed
by `8f391a30d`. Thirteen focused tests pass, including cold authority controls,
and independent source review finds no gap. The default aggregate gate remains
incomplete; no tracked profile or timeout policy changed.

The pinned tail gate passes syntax 59, grammar 40, native formatter 12, the
workspace compile check, route-witness unit 12, inherited-environment 2 and
native environment 1. Its tooling native case reports 161 passes, one failure
and one skip. The failure reproduces before Deser: an erased Record initializer
with an empty-list member lacks contextual producer proof. `6d6be4da9` routes
that exact record into existing writer/payload authentication; six focused tests
pass with scalar, owner and writer defenses. Ordinary require is unchanged.
The formerly failing native case now passes under the pinned S4 tools.

The combined S4 implementation is integrated at `d68438993`. Libraries compile
without warnings. Codec unit checks pass 16/16, borrowed encoders 5/5 and retained
strict schema guards 3/3; typed cold and both-runtime-route tests pass 2/2.
Patched upstream CSV passes 9 unit and 60 integration tests; PapaParse retains
125 passes, 40 skips and 34 known failures. New duplicate-header, scanner-field
and completed-record limit controls fail when their corresponding repairs are
disabled. The pinned tools pass typed native 9, table native 6, API native 67,
API Rust 4, registry 18, signature 6, wire enums 11 and strict record policy 7.
The table file case checks option ordering, no-clobber, failed-write preservation,
overwrite, UTF-8 path context and filesystem effects. The strengthened imported
nominal alias case passes. Native tooling now passes 162 with one existing skip.
The API fixture records only the intended delta: one csv module, six functions
and seven documented/queryable items.

Steering checkpoint: 130 remaining tooling integration cases and all 11 lint
invariance cases pass on the S4 tree. The exact desugar parity case selects 204
files, completes original checks/tests, transforms them and compares diagnostics,
then reaches transformed native tests before its 900-second deadline. That run
is incomplete, not a pass. Defer its next attempt to the mandatory final Full
owner lifecycle, which has no automatic cumulative deadline; do not replay this
intermediate corpus before the feature set is complete. Preserve its exact
before/after outcome comparison and all existing exclusions.
Treat the integrated S2-S4 serialization work as one provisional validation
cohort and S5 as the second. This changes batching only: no phase/global pass is
claimed, no assertions are removed, and final Full/Linux, production cost and
macOS qualification obligations remain. The next delivery is lazy sequences and
parser/file hardening; no third feature cohort or optional tooling work starts.

Deliverables:

1. A normal in-tree Deser crate family with seven codecs: JSON, YAML, TOML,
   INI, CSV/TSV, CBOR and MessagePack. No submodules, Git dependencies or excluded
   codec source in the vendored tree.
2. Direct construction of XSH values and borrowed serialization of runtime and
   lowered values; no mandatory dynamic Deser tree in language execution.
3. Complete JSON replacement and workspace-wide `miniserde` removal, including
   tooling and host adapters. Migrate callers; leave no compatibility backend.
4. Coherent public text/binary, file, typed lexical and sequence APIs, with
   explicit representation limits and structured failures.
5. Behavioral acceptance, authored and generated documentation, dependency
   accounting, final production-size/RSS checks and readiness-qualified timing.

Launch baseline: `9d893e0075d284db4b158eef419654677e8b1288` on master.
S0 focused baseline passes: JSON module 18/18 and the selected INI file case 1/1.
S1/S2 adapter, vendor and host migrations are committed at `6a4bdfd56` with
focused proof. `aaeaf17f3` also migrates formal-bridge and statistics JSON
transport. The combined Change gate exposes an existing empty-map receiver proof
failure after 913 native stdlib passes and 34 skips. `efb63cab0` (formerly `ac91aa5b0`) safely authenticates
a fresh empty-map receiver when its domain is known, but the combined retry on
`f42b527ea` (same implementation as current `f286c59e4`) still fails: 914 pass, 2 fail, 34 skip. Driver proof has lost the
receiver domain (`Any`); the second causal repair did not restore it. The item
is parked after two repairs, with a compiler-only failing regression on branch
`campaign/deser-proof` at `e1974166409cfc22bfec25c1ef165c6598c8259d`.
Retry requires evidence about driver initializer obligations, payload annotations
and writer mapping; repeating the optimized gate is not useful.
The S1/S2 phase exit is not accepted yet. The performance/deployment
campaign is paused; its outstanding acceptance does not disappear here.
Current ruling: feature completeness and behavior first, final optimization last.
Acceleration ruling: the explained verifier debt blocks acceptance, not isolated
implementation. Two provisional S3 lanes implement configuration and binary
backends, while the coordinator owns their shared sink, borrowed emitters and
public dispatch in a third isolated worktree. Master implementation is frozen
until a coherent integration wave. No S2/S3 phase pass is claimed.
The owner pulled additional TODO documentation while the implementation was
isolated; the master rebase changed commit identifiers but no implementation.
S3 is integrated on master through `7adcfcf32`. The pinned Linux native gate
passes all 19 tests: generic codec/file dispatch 3, configuration 6, binary 4,
INI 6. Combined Rust checks pass codec adapters 13, borrowed encoders 5,
no-clobber/symlink boundary 1, registry 18 and signature adapter 5. Retained
upstream checks pass YAML 77, TOML 59, INI 25 and CBOR 8. Initial native fixture
errors (module shadowing and an incorrect Path helper name) were corrected;
no codec behavior was weakened. Master differs from the qualified S3 tree only
by owner TODO/campaign documentation and the separately checked verifier repair.

The parked verifier prerequisite was resolved by source tracing: runtime-complete
storage validation skips the initializer Expression obligation, despite preserving
its annotation. `2c39f0d56` now seeds only the exact authenticated driver
initializer's method context, preserving general runtime-validation elision.
Seven focused Rust checks pass, including warm/cold driver cases, forged target,
annotation/validation/initializer/owner/factory/arguments and checked dynamic
admission. Independent source review found no authority gap. This is a new
causal correction after the named retry condition changed, not a repeated
unchanged attempt. Pinned native/broad qualification remains pending.
Further S3 qualification on `7261b0420` passed the catalog, all 929 native
stdlib cases (34 skips), library facade 5, registry 18, signature 5 and API Rust
3. The API native gate exposed the intentional inventory delta and an old
ambiguous JSON property. `6c3f9f8b0` uses Deser for the summary, keeps the receiver
inventory at `method_receivers`, and names its numeric count
`method_receiver_count`; all 67 native API checks pass. The inventory fixture
records the intended 40 modules and 498 functions. This is an explicit wire
shape correction, with no duplicate-property compatibility path.

The tooling corpus exposed new-module name collisions in Tokei's `toml` counter
and compression's `codec` bindings. `a6ee73b21` renames only those locals;
both files check, Tokei 3/3 and compression 52 pass with 3 existing skips, and
both previously failing formatter/lint controls pass in isolation. A CBOR
underflow probe failed with UnsupportedValue instead of Overflow; `79d7ceb94`
classifies native wide integer atoms and bignums as Overflow. The retained
negative test fails before repair; binary Rust 5/5 and pinned native 5/5 pass.

The aggregate tooling leaf exhausted Change's cumulative five-minute budget;
it is incomplete, not a pass. Preserve every remaining assertion through
existing disjoint Rust filters: integration remainder, lint-invariance module,
and exact desugar corpus parity. The latter runs selected native files before
and after transformation and cannot be replaced by ordinary native results.
Its execution owns the native slot. No workflow or CI change is needed.
The final Full/Linux floor remains binding. Next milestone: finish remaining
selected S2/S3 cases and the unresolved tooling partitions on the frozen tree,
then deliver CSV/TSV and explicit typed lexical decoding.

## Charter

Follow repository instructions and the `orchestrate` campaign procedure. Decide
routine methods within this plan's bounds, record consequential rulings here,
and continue until every deliverable passes or a named hard stop parks it.
This document describes effects; it cannot grant permission to execute them.

Use the narrowest existing Edit/Change gate. Reuse evidence only while relevant
source, dependencies, options, environment and acceptance inputs remain valid.
Batch compatible candidates into one acceptance wave; do not claim intermediate
commits passed the combined wave. Never waive a failed behavior to keep moving.

Start with one implementation lane for the adapter, JSON and shared contracts.
After that lands, allow at most two disjoint codec lanes and one coordinator.
The coordinator alone owns Cargo manifests/lockfile, registries, schema/IR
contracts and generated references. At most two builds and one native suite may
run concurrently, with two Cargo jobs each. Reserve 12 GiB host memory; admit a
build only with at least 8 GiB additional available memory, then use observed
peak RSS to narrow concurrency. Run `perf-gate-ready -s` before adding lanes;
exit 75 means add no width. Headroom is a ceiling, not a target.

Keep at most two provisional capabilities awaiting acceptance. At every wave,
after two failed attempts, or after 45 minutes without a substantive delivery,
the parent performs the steering checkpoint: identify accepted progress, the
limiting stage, drift, the next useful delivery and any changed method. Reduce
writers when review or verification queues grow. Two checkpoints dominated by
tooling require a smaller method and a real codec delivery next.

A failing item gets two causal repairs after its minimal reproducer, then parks
with cause, evidence, prerequisite and retry condition. Do not retry an unchanged
external blocker. One coordinator owns shared environment problems. A focused
loop over five minutes gets one bounded loop repair with a two-attempt limit;
return to delivery after it works. Fix a baseline failure only when it blocks the
selected capability. Record unrelated findings without starting another sweep.

All worktrees live at `.work/wt/deser-NAME`; scratch lives at `.work/tmp` and
`TMPDIR` points there. Confirm `.work/` is ignored in `.git/info/exclude` before
writing. Commit durable source and decisions under the later launch grant;
keep disposable probes outside tracked source. Preserve useful artifacts before
normal worktree removal. Follow `orchestrate`'s worktree lifecycle and use
`campaign-winddown`/the closeout guard if execution is paused.

Local commits, if authorized by the launch, must be Conventional Commits; put
item IDs in bodies. Breaking contracts use `!` or a `BREAKING CHANGE:` footer.
Never push, change CI, run formatters/autofixers, change the pinned Rust nightly,
modify another repository or update installed systems. Reserve release builds
for final verification. No background retry or watcher survives a pause.

## Decisions

### Owner-decided, 2026-10-25

- Vendor Deser directly, without submodules. Upstream development is active;
  published-release availability is not a prerequisite.
- Keep the important codecs and physically omit unnecessary ones, including PHP
  and pickle. Codec selection is delegated to this preflight.
- Hard cutover: no old parser, compatibility flag, fallback, deprecated alias or
  requirement to preserve obsolete implementation quirks.
- Continue on master. Feature completeness takes priority over waiting on perf.
  Busy timing can defer final acceptance; it cannot stop independent delivery.

### Owner-decided, 2026-10-26 launch

- Local implementation commits and final manual verification are authorized.
- Laputa verification is not required for this campaign. Keep codec, XSH tool
  and Linux acceptance in scope. Do not run external Laputa qualification.
- No macOS host is available. Keep native macOS qualification explicitly open.

### Prescribed

- Retain the seven codecs in the inventory below. This covers configuration,
  tabular data and common binary interchange without niche object-graph or
  document-model support. A future codec needs a separate owner request.
- Freeze upstream commit `8c15055178fcf2488dcd1ac70e6d1ff18689b486`, inspected
  2026-10-25, whose packages declare `0.10.1`, edition 2024 and Rust 1.88.
  Do not chase upstream HEAD during execution. Updates are separate reviewed
  changes with the same focused contract tests; do not change XSH's toolchain.
- Keep useful JSON language functions as the canonical Deser-backed API. Their
  continued names are not a compatibility layer. Preserve documented language
  invariants; retire backend-specific error strings, old INI dialect quirks and
  duplicate-key last-wins behavior as explicitly listed below.
- Use a direct XSH sink plus borrowed emitter. A second dynamic tree for all
  language values loses on allocation, ownership and the owner's requirement;
  an unsafe thread-safety assertion is forbidden. Scratch proof demonstrates
  actual runtime `Value: Send + Sync`. Check private `LoweredValue` separately
  during S1; project supported borrowed nodes if needed, without copying an
  entire value tree or exposing resources.
- Typed lexical parsing uses explicit schema-bearing calls, not hidden
  decode/require fusion. Ordinary `.require`, `.require_exact` and
  `.require_stripped` remain unchanged. The draft's exact CSV example is
  superseded by the typed API below: a plain decoded string remains a string.
- Use existing prepared schemas and the existing validator for final admission,
  including nominal/refined values and wire enums. No new schema language,
  generated Rust type, first-class runtime type object or duplicate validator.
- Keep a narrowly scoped `deser-value` dependency for existing Rust/tooling JSON
  documents and temporary reference comparisons. It replaces the public
  miniserde DOM used by host consumers and preserves their full u64 domain.
  It must never become the language decoder or a fallback from the direct sink.
  Replace the old raw-helper family with `xsh::host::json::{Value, parse, encode,
  encode_pretty}`: a Deser DOM re-export and fallible parse/render functions.
  Callers use its normal constructors/accessors; migrate the old helper imports
  and miniserde derives together. Exposing this dependency type is deliberate
  host-tooling interop, not XSH's runtime value representation.
- No public transcode operation, environment codec or debug codec. Cross-format
  workflows go through XSH values and must honor representation failures.
- Measure cost throughout; optimize after completeness. Final gates below are
  binding and cannot be waived by dropping an included codec.

### Delegated within bounds

The executor chooses private file splits, exact native test names, the smallest
safe event/sink lifetimes, and upstream configuration calls. Preserve these
contracts and ownership boundaries. Use an upstream option before a local
parser patch. A local patch needs a reproduced missing behavior and focused
proof; do not reshape upstream unsafe arenas or driver lifetime erasure.
If a required behavior cannot be delivered within two causal attempts, park the
item with a concrete prerequisite. Do not silently reduce the supported subset.

## Baseline and foundation proof

The source is the truth: JSON parsing uses miniserde in `src/modules/json.rs`;
`src/runtime/eval/json.rs` already traverses runtime and lowered values directly
for compact output and has tree/pretty routes. Runtime integers are i64.
`UInt` is a nonnegative refinement of that representation, not a u64 variant.
Records use owned string keys; maps have the closed XSH scalar-key vocabulary.

`src/modules/ini.rs` is a custom parser/encoder. It folds keys and rejects
repeats, unlike upstream's default dialect. `src/lib.rs` exports raw JSON host
helpers whose types expose miniserde. Removing only the language decoder would
leave locale loading, route-witness/formal-bridge code, statistics, fake Linux
fixtures, XSHT and Rust tests dependent on it. S2 owns these consumers.

The paused campaign's canonical run has 230 Rust integration passes, two known
failures, one filtered test; the daemon and nested-parameter repairs have focused
proof, but the full retry was cancelled. No current-source full acceptance is
available. Use `docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md` for the exact state.
Do not inherit old full passes or run a new full baseline as an entry fee here.

Preflight ran scratch code against the inspected Deser source and actual XSH
`Value`. It directly decoded a nested JSON record/list into XSH values, emitted
it through borrowed scalar/container views, reused the sink for YAML and CSV,
rejected duplicate JSON keys and an integer above i64, and enforced a depth
limit. It compiled the actual `Value: Send + Sync` assertion without unsafe.
A typed upstream CSV sink parsed `8080` as i64 while the untyped XSH sink retained
`"8080"`; a JSON string could not deserialize as i64. This proves the adapter
shape and the need for an explicit lexical schema boundary, not full parity.

Diagnostic loop observations on this host, with existing verification products:

| Check | Result | Observed wall time |
|---|---|---|
| Existing JSON sorted-key/compact/pretty/file native case | 1 pass | 0.12 s |
| Existing INI decode/encode/file native case | 1 pass | 0.03 s |
| Warm scratch compile and adapter probe | pass | 2.87 s |

These are single loop observations, not performance claims. The scratch crate
resolved its own transitive lockfile; production dependency qualification must
use XSH's locked graph. No production size, RSS, cold-build, startup or codec
speed claim follows. The larger initial scratch build is not an incremental
loop baseline. Scratch artifacts are disposable and not required to resume.

## Vendoring and codec inventory

Place the vendored family under `crates/deser/`, with upstream package names in
child directories and explicit members in XSH's root workspace. There is no
nested workspace or nested `.git`. Keep relative local dependencies; remove
workspace-package inheritance or supply explicit edition/rust-version fields.
Do not flatten upstream's crate boundaries into one improvised implementation.

Copy only `deser`, `deser-core`, the seven codec packages, `deser-path`,
`deser-location`, `deser-value`, and `deser-derive`; keep `deser-encoding` only
for retained conformance tests that actually import it. Location/encoding are
test support unless a named production caller proves otherwise. Derive is
allowed for existing Rust-owned structures such as locale data and route-witness
artifacts; it does not generate Rust schemas for user-defined XSH types. The value
crate has the narrow host-tooling use above. Enable only std, synchronous
IO and existing codec speedups that the selected packages need. Enable derive
only for those actual Rust-owned types; disable open-enums and unrelated
external-type adapters in production.

Retain Apache-2.0 LICENSE and required notices, relevant package READMEs, source
and codec/core tests with their local fixtures. Keep one `crates/deser/README.md`
for the upstream revision, retained scope and update procedure, because future
vendor updates need it. Do not add a provenance manifest, updater daemon or
second lockfile. Inspect retained dev-dependencies and test imports; prune
unneeded cross-format examples/aggregators rather than pulling excluded codecs
back in merely to compile their old examples. Retain selected upstream driver,
partial-drop, errors, numeric and parser conformance coverage without weakening
its assertions or blessing new failures.

Do not copy excluded packages, their tests/examples/fixtures, benchmark projects,
compile-time experiments, fuzz projects, workflow files, template generators for
excluded JSON variants, Tokio/Serde bridges or upstream repository metadata.
The checked-in JSON generated Rust sources are usable without retaining a
multi-dialect generator. Patch only the needed generated JSON sources locally;
a future vendor update replaces them from the frozen upstream source. No
excluded format can be enabled through a dormant feature or dependency.

All selected upstream packages declare Apache-2.0. Necessary production
additions are number formatting (`itoa`, `zmij`), optional `simdutf8`
acceleration and `indexmap` for the host-tooling DOM; derive adds proc-macro
build dependencies. Inspect the resolved license/transitive graph in S1 and
reject unrelated package upgrades. Resolve additions once against the existing
Cargo.lock, then use `--locked`. These dependencies serve the selected codecs
or actual host consumers; the standard library cannot supply their parsers.
No maintenance or speed guarantee follows from upstream activity. Final size,
memory, startup and build gates constrain the whole dependency addition.

| Upstream codec | Disposition and language surface |
|---|---|
| `deser-json` | Keep; canonical `json` API plus generic `codec` dispatch and JSON Lines. |
| `deser-yaml` | Keep; `yaml` module, YAML 1.2 and document streams. |
| `deser-toml` | Keep; `toml` module, TOML 1.1 input and upstream output contract. |
| `deser-ini` | Keep; canonical `ini` module, one configured common dialect. |
| `deser-csv` | Keep; CSV and TSV identifiers, record streams and explicit table options. |
| `deser-cbor` | Keep; bytes and CBOR sequences, explicit canonical encoding option. |
| `deser-msgpack` | Keep; bytes and consecutive-value sequences. |
| `deser-jsonc`, `deser-json5`, `deser-hj` | Omit physically; overlapping configuration dialects do not justify three more parsers. |
| `deser-xml` | Omit physically; namespaces, attributes and mixed content require a separate document contract. |
| `deser-plist` | Omit physically, all dialects; specialized platform/object-reference model. |
| `deser-php`, `deser-pickle` | Omit physically; explicitly unwanted object-graph formats. |
| `deser-urlencoded` | Omit physically; HTTP form nesting/multimap policy is outside this serialization delivery. |
| `deser-env`, `deser-debug` | Omit physically; existing environment and display owners remain canonical. |

No claim of supporting all 17 codecs remains. Unselected identifiers fail
`CodecError.UnsupportedFormat`; no aliases or filename-based detection.

## Public and runtime contracts

### Operations

Retain `json.decode/encode/read/write/get/set/remove/encode_lines/write_lines`
and existing overloads. JSON path utilities remain XSH-owned structural
operations, not a second parser. Preserve current JSON write behavior and the
existing INI `overwrite` contract; do not infer that JSON has an overwrite flag.

Add `yaml` and `toml` decode/encode/read/write as thin canonical entries to the
same implementation. The generic format identifiers are exactly `json`, `yaml`,
`toml`, `ini`, `csv`, `tsv`, `cbor`, `msgpack`. Parse them to a private closed
Rust enum; one capability definition drives dispatch and discovery.

| Generic operation | Contract |
|---|---|
| `codec.decode(format, text)` | `Result[Any, CodecError]`; text formats only. |
| `codec.decode_bytes(format, bytes)` | `Result[Any, CodecError]`; binary directly, text after strict UTF-8 validation. |
| `codec.encode(format, value)` | `Result[Str, CodecError]`; text formats only. |
| `codec.encode_bytes(format, value)` | `Result[Bytes, CodecError]`; binary directly, text as UTF-8 bytes. |
| `codec.read(format, path)` | `Result[Any]`; checked filesystem boundary plus codec errors. |
| `codec.write(format, path, value, overwrite: false)` | `Result[Unit]`; encode before touching destination; existing no-clobber/replace host operations. |
| `codec.decode_all(format, text)` | `Result[Stream[Result[Any, CodecError]], CodecError]`; supported text sequences only. |
| `codec.decode_all_bytes(format, bytes)` | Same, including binary sequences. |
| `codec.read_all(format, path)` | `Result[Stream[Result[Any, CodecError]]]`; lazy owned file reader. |
| `codec.encode_all` / `codec.encode_all_bytes` | Corresponding Str/Bytes result from an explicit `List[Any]`, for supported sequences only. |
| `codec.formats()` | Stable ordered capability records: identifier, text/binary, single/sequence and lexical typing. |

Use normal `Result` error unions at I/O boundaries. Decode/encode on supplied
values is pure; file operations declare filesystem/IO effects. Lazy file-stream
pulls retain their existing checked effect/ownership contract. No ambient
process, environment, cwd, network or schema-file access is permitted.

Add a small `csv` module for options with real table meaning: headers default
true, delimiter default comma (one ASCII byte), columns default inferred from
the first row, and `escape_formulas` default false for output. TSV's explicit
preset uses tabs, its documented escaping and null policy. Reject conflicting
options and invalid delimiters; headers must be unique and row widths
must match. Headerless decoding returns row lists. Explicit columns apply to
encoding record rows and specify their output order; decode calls reject a
columns argument. Headerless encoding omits the header but retains that order. Generic csv/tsv dispatch uses the
presets; it does not accept an untyped options bag. Add `codec.encode_bytes`
named `canonical: false` for CBOR only; true on any other format errors.
Pretty JSON remains the existing named option, without promising preservation
of input formatting in any codec.

### Typed lexical input

Introduce explicit `codec.decode_as(format, text, T) -> Result[T]` and
`codec.read_as(format, path, T) -> Result[T]` for CSV/TSV and INI. The third operand uses the
existing schema type grammar as `.require(T)` does. This is a bounded addition
of a type operand to these two intrinsic calls; it is not an ordinary runtime
argument. The registry, parser/checker, AST/IR/verifier and generated API must
all describe it. No expression-level generic syntax or first-class type values.

Example: `codec.read_as("csv", p"services.csv", List[Service])?`. This replaces
the draft's implicit `codec.read(... )?.require(List[Service])?` coercion.
The plain form still returns strings and strict require still rejects them as
integers. Explicit typed calls reject self-describing formats, whose callers
use ordinary decode followed by require. No dependence on an adjacent require,
expected assignment type, optimization level or inlining changes the result.

Compile the closed target through existing prepared-schema machinery. A
schema-guided sink interprets lexical atoms only at typed primitive leaves,
then invokes the existing validator once for admission. No validation predicate
runs twice. No type information comes from untrusted tags. Missing/extra-field,
optional, union, nominal, refinement and wire-enum behavior remains owned by
that validator. Typed entry points use ordinary preserve-extra-fields admission;
exact/stripped policies remain explicit subsequent require calls.

Lexical `Str` stays exact text. `Int` accepts signed decimal within i64; `UInt`
adds the existing nonnegative check; Bool accepts exactly `true` or `false`;
Float accepts finite decimal/exponent values with normal XSH f64 rounding.
Empty lexical fields become null only for optional targets. No truncation,
whitespace repair, arbitrary truthy spellings or date inference. If a union
admits Str, preserve the string and validate normally; otherwise accept a
conversion only when exactly one distinct primitive interpretation validates.
Two matching interpretations are an explicit schema error. Unconstrained Any
and extra fields stay strings. Nested records/maps/lists use their existing
schema shapes; CSV nested field encodings are unsupported rather than guessed.

### Representation and semantic changes

- Scalars: null, Bool, Str, i64 Int and finite f64 Float. Preserve existing JSON
  integral-float normalization, exponent/negative-zero behavior and sorted
  compact/pretty output through differential fixtures. Reject out-of-i64 integer
  tokens rather than converting them to approximate floats. Ordinary fractional
  numbers retain XSH's documented f64 rounding; do not advertise arbitrary
  decimal precision or add a bigint/decimal runtime type.
- String-keyed objects become XSH records; preserve existing shape-interning
  safeguards. CBOR/MessagePack maps may use existing scalar MapKey types.
  Reject complex/nonfinite keys; never stringify them. Empty binary maps become
  Map, and empty textual objects become Record. Non-string-keyed textual maps
  fail; YAML complex-key support is outside the represented subset.
- Duplicate keys are rejected for all included formats, including JSON and INI.
  This intentionally replaces old JSON last-wins behavior. Repeated INI sections
  may merge only when no key repeats or conflicts; duplicate sections with
  conflicting keys fail. No compatibility switch.
- INI uses Deser's common dialect, case-preserving names, quoted values and
  whitespace-delimited inline comments. Untyped fields are strings. This
  intentionally replaces XSH's lowercasing and old quote/comment interpretation.
  Keep existing continuation/file safety invariants; migrate affected callers
  and golden tests with before/after examples. No second INI parser or Git/Python
  dialect API in this campaign.
- YAML uses version 1.2; quoted scalars stay strings, plain scalars use that
  version's resolution. Support bounded anchors/aliases, explicit standard
  scalar tags, byte scalars and multiple documents. Reject arbitrary custom
  tags, merge-key conflict/duplicate ambiguity and cyclic aliases. Do not allow
  an implicit scalar fallback to turn a quoted numeric string into a number.
- TOML accepts arrays of tables and its native date/time syntax. Recognize
  datetime extensions and return UnsupportedValue: XSH has no exact calendar
  value, and silently flattening it to Str loses type/precision. This is an
  explicit supported-subset limit, not a claim of universal TOML round trips.
- CBOR/MessagePack carry native Bytes. Honor supported scalar/container lengths,
  reject integer overflow, unsupported CBOR tags and MessagePack extensions,
  including timestamps. Recognize supported standard byte/integer atoms;
  inspect extension/event metadata before accepting fallbacks. Unknown semantic
  tags must not silently disappear. Canonical CBOR uses upstream's canonical
  option and byte-exact fixtures, not an inference from sorted record keys.
- Sets encode as ordered sequences and wire enums as their checked string
  mappings. Nominal records encode as records. Reject capabilities, ordinary
  enums, Path, Digest, Duration, Error, Result, Status, command plans and resource
  values unless the existing JSON contract already explicitly allows the case.
  Encoding never resumes a Stream or executes script callbacks.
- No cycles or shared-reference graph identity is promised. Repeated immutable
  children may encode as repeated values; cyclic/unsupported input fails.
  No coercion creates nominal identities or resource handles.

### Errors and limits

Add one canonical `CodecError` family through the existing registry, with
variants Syntax, Encoding, UnsupportedFormat, UnsupportedValue, Overflow,
DuplicateKey, Limit and Options. Each has message, format, optional input path,
optional line/column/byte offset and optional value path. Coordinates are
one-based line/column and zero-based byte offset; preserve XSH call-site spans.
Parser failures implement InvalidData, budget failures CaptureLimit; file errors
retain actual existing host facets and causes. Schema failures retain their
existing schema boundary. Unexpected driver invalid-state/configuration errors
are invariant defects, not malformed-input success or blanket catch-all errors.

The error fields have types `message: Str`, `format: Str`, `input_path: Path?`,
`line: UInt?`, `column: UInt?`, `offset: UInt?`, `value_path: Str?`. Keep error
coordinates absent when unavailable rather than using a zero sentinel.

Codec-backed JSON/INI decode/encode failures migrate to this family; path-helper
errors stay `json-path`. Update all affected tests/docs/callers in the cutover.
Do not require byte-identical upstream prose or parse error messages to classify
failures. Locations are optional where upstream cannot supply them. A later
standalone require has its ordinary schema location; do not invent retained
per-node source metadata for every decoded value.

Bound untrusted supplied input and single documents to 64 MiB, scalar strings
and byte strings to 16 MiB, nesting to 128, entries/items per container to
1,000,000, events per value to 4,000,000 and encoded output to 128 MiB. YAML
alias expansions are at most 10,000 and consume the same expanded-event/output
budget. File reads stop after the input limit plus one byte; reject before
allocating from unchecked declared lengths. Streaming limits are cumulative:
256 MiB input, 1,000,000 documents/records and 128 MiB emitted output per
invocation, plus the per-value bounds. Expose these fixed limits in the SPEC;
no process-global knobs or unlimited sentinel in this delivery. Enforce parser
buffer/anchor limits before unbounded pre-event growth; core event limits alone
do not bound whole-document parsing. Test boundary and boundary-plus-one cases.

New generic writes use existing host operations: no-clobber uses exclusive
creation and rejects any existing destination including a dangling symlink;
overwrite follows the existing path/symlink policy and truncates the destination.
Validate/encode before opening. A later write failure can leave a partial file,
as the existing nontransactional writer can; do not promise atomic replacement
or crash durability. Readers follow the existing checked host-path policy.
No string-to-path lossiness or new confinement claim is introduced.

### Stream lifetime

JSON Lines, YAML documents, CSV/TSV records, CBOR sequences and consecutive
MessagePack values use the existing XSH Stream owner, not another runtime.
Constructing the stream performs no item consumption. Supplied memory input is
owned for its lifetime; file input has one reader owner, closed on exhaustion,
error, cancellation or early drop. Keep incomplete-token state for incremental
formats; YAML may buffer one bounded document and CSV one bounded record.
Do not borrow local parser buffers into a returned stream or reparse prefixes
on every pull. Never collect the whole file to claim streaming.

JSON Lines and CSV may report an item error and recover at their documented
line/record boundary. Binary syntax and YAML errors terminate after one error;
no guessed resynchronization. Index errors by one-based item number and preserve
source offsets. Budget exhaustion is terminal across every format. Preserve
existing JSON line skip/recovery behavior where it is already a language
contract. Backpressure follows pulls; no worker thread, Tokio runtime or hidden
producer queue. TOML and INI reject sequence calls rather than silently wrap
one whole document. Single CSV decode returns the complete bounded table.

## Sequence

Sizes indicate coherent capabilities, not a fixed number of commits. Every
item includes behavior, native tests, relevant host proof and authored docs.
Shared contract changes stay with the coordinator.

| Item / phase | Size, prerequisites | Ownership and delivery | Acceptance |
|---|---|---|---|
| S0 Ground | S; launch | Coordinator: freeze current master, inspect source/tests/callers and approved image; characterize JSON/INI boundaries and select existing fixtures. | Existing focused JSON/INI cases, gate plan, known-red record; a comparison fixture fails when a meaningful output/value/error is changed. No cold global entry fee. |
| S1 Direct adapter and vendor | L; S0 | Coordinator: selected `crates/deser/`, manifests/lock, `src/runtime/eval/` codec view/sinks and host dispatch. Establish direct runtime/lowered traversal, error family and limits with a JSON/YAML pilot. | Actual nested Value and LoweredValue decoding/encoding, numeric/key/unsupported-resource tests; partial failure/drop and limit controls; no dynamic DOM in language decoding. Scoped cargo checks and selected upstream driver/parser tests. |
| S2 JSON hard cutover | L; S1 | Same lane: `src/modules/json.rs`, runtime JSON routes, locale, formal bridge, route witness, statistics, `src/lib.rs`, XSHT consumers and Rust fixtures. Replace miniserde, migrate facade consumers and remove old paths atomically with successful tests. | Full JSON behavior corpus and exact raw-host u64/locale/tooling/witness cases; no miniserde in workspace source, manifests, resolved graph or lockfile; no old serializer selection. Advance first accepted capability. |
| S3 Configuration and binary formats | L; S2 | Two disjoint lanes: YAML/TOML/INI backends and native tests; CBOR/MessagePack backends and native tests. Coordinator integrates public registry/runtime IDs and module dispatch. | Each included format's positive/negative matrix, YAML aliases/documents and TOML datetime rejection, INI intentional changes, native bytes/scalar keys/extensions and canonical CBOR controls. Phase gate on combined source. |
| S4 Tables and typed lexical decoding | L; S2, S3 INI | One lane: CSV/TSV backend/tests; coordinator: intrinsic type operand, schema preparation, lowering and physical verifier authority. Same prepared validator admits results. | Header/width/quote/UTF-8/null/columns/formula cases, typed primitive/nested/union/refinement controls, no JSON coercion; cold forged schema/owner/target tests fail before execution. No broad inference redesign. |
| S5 Sequences and hardening | L; S3, S4 | One owner of existing stream integration and file adapters; backends contribute incremental state. Finish docs, effects, unsupported/error/location contracts and hostile-input limits across all formats. | Lazy read/cancel/drop/error/recovery controls, chunk boundaries, complete cumulative limits and bounded RSS; missing/read-only/symlink/non-UTF8 path and failed-write preservation cases. All feature deliverables before optimization. |
| S6 Cost and polish | M; S2-S5 accepted | Coordinator: production artifacts, existing benchmark fixtures/adapters and dependency/features audit. Remove avoidable allocations/linkage only on measured evidence. | Matched size/RSS/build/startup/codec measurements against frozen pre-cutover source; thresholds below, with timing deferred on busy host. No codec silently dropped. |
| S7 Close | M; S6 | Stop writers, delete dead fallback/temporary comparison code and unused dependencies; generated docs; focused independent `review` using the applicable `ultrareview` lenses on the changed serialization boundary, without an unrelated repository audit. | Fix demonstrated P1/P2 findings, final integration/Linux scope once, no open behavior, clean local commits and normal checkout cleanup. If external proof is blocked, pause with explicit unfinished acceptance. |

A phase exit runs the touched-area checks selected by `cargo dev gate`; S2's
first broad parity gate covers the complete JSON boundary, not every repository
scenario. S3-S5 share compatible products and exact selectors. Do not start a
new unrelated verification framework or merge the parked Pyramid tree here.

## Acceptance and the fast loop

Use current commands, with kache and Linux mold as repository instructions
require. Verify selection before dispatch:

```sh
cargo dev gate src/modules/json.rs src/runtime/eval/json.rs --plan
cargo dev gate src/modules/json.rs src/runtime/eval/json.rs
```

Extend the changed-path arguments to the actual item; inspect unmapped vendor
paths and run the relevant explicit package checks rather than ignoring them.
Use `cargo check --locked -p xsh -p xsht -p xshi` for shared dependency/facade
changes, not for every codec edit. Native behavior belongs in XSH tests; Rust
covers adapter lifetimes, exact numeric/byte transport, verifier forgeries,
allocation and file-descriptor boundaries that native tests cannot express.

Focused native examples, using the matching verification xsht/xsh products:

```sh
xsht test --exact tests/xsh/stdlib/json.xsh::test_json_record_keys_are_sorted_in_every_encoder
xsht test --exact tests/xsh/stdlib/ini.xsh::test_ini_decode_encode_and_files
```

`--exact` selects one test; use separate calls or repeated `--exact-id` after
listing IDs to batch selectors. New behavior tests receive exact selectors in
the item report. Exclude no failing test for convenience. Intentional semantic
changes update only the assertions whose contract changed, with the old/new
behavior demonstrated and documented.

### Required behavior corpus

| Area | Observable proof required |
|---|---|
| JSON decode | Null/bool/string, i64 endpoints and overflows, UInt validation, finite fractions, exponents, negative zero/integral floats, empty/nested arrays/objects, Unicode/escapes/surrogates, malformed/trailing input, invalid UTF-8 bytes, duplicates and depth/resource failures. |
| JSON encode | Sorted record/map keys, compact/pretty/escaping/float goldens, set order, wire enums, dynamic records and host Stats/StatsBlob shapes, runtime/lowered agreement, unsupported resource/map-key/nonfinite errors, large bounded output. |
| JSON language helpers | Every get overload, set/remove missing/intermediate/root paths, read/write and JSON line newline/order/blank-line/failure behavior, actual filesystem writes and error propagation. |
| All seven codecs | Decode and encode through the real XSH boundary, representable round trips, nested/empty containers, numeric limits, explicit duplicate policy, unsupported values, structured errors and real read/write behavior. Do not substitute existence or smoke tests. |
| Configuration | YAML 1.2 quoted/plain types, standard/custom tags, aliases/cycles/expansion budgets and multiple docs; TOML arrays of tables/date-time rejection; INI case/quotes/comments/continuations/conflicts and caller migrations. |
| Tables | Headers/headerless lists, unique and explicit columns/order, field-count failures, malformed quote failure, delimiters, CRLF/UTF-8, repeated rows, empty/null, TSV escapes, formula escaping opt-in and unchanged default strings. |
| Binary | Byte-exact buffers including NUL and invalid UTF-8, all representable scalar keys, complex key rejection, integer overflow, truncated input, unsupported tags/extensions, canonical CBOR vectors and multi-value sequences. |
| Typed boundary | Nested/optional/list/map/union targets, missing and extra fields, nominal identity, Str-backed enums, refinements exactly once, invalid primitive/enum, ambiguity and value paths; unchanged strict require/exact/stripped; JSON/YAML strings never gain numeric coercion. |
| Compiler authority | Closed/reifiable target checks; imported/alias schema identity; malformed intrinsic operands; cold IR mutations of schema ID, owner, target type and producer; schema metadata never grants its own authority. |
| Streaming | No pull at construction, one-byte and UTF-8/delimiter chunk splits, early close, repeated independent reads, item index/location, line recovery versus terminal errors, bounded parser buffers and cumulative byte/item/alias/output budgets. |
| Effects and file safety | Pure encode/decode cannot open files, read environment or run resources; Path stays byte-safe; file effects are checked; no-clobber/overwrite and symlink policy use existing host boundary; invalid value/options fail before destination mutation; partial writes close readers/writers and leave the documented destination outcome. |
| Workspace consumers | Locale lookup; raw JSON host u64 limits; actual XSHT highlight/coverage/test/example/trace/report output; route-witness/formal bridge and runtime-stat fixtures. Preserve their wire/schema contracts unless an explicit entry here changes them. |
| Cross-format | YAML -> JSON -> value and CBOR -> YAML for the shared subset; impossible Bytes/extension conversions error. Compare values and effects, not arbitrary serializer whitespace. |
| Upstream core | Selected driver arena lifetime, partial-drop/error/deep-nesting and codec corpus controls under pinned nightly; retain known failures visibly, never bless a new failure. |

The runnable pre-cutover JSON implementation and its native tests are the
reference, not a second production backend. Freeze the corpus once, run both
products on identical inputs, and commit only needed golden fixtures. Preserve
all specified language behavior except the explicit semantic changes above.
Normalize only real nondeterminism; preserve failure categories, effect order,
key order and multiplicity. Prove a comparison detects a corrupted value,
missing output, changed error category and wrong lowered projection. Delete
temporary old/new runtime selection after parity; the old Git revision remains
an oracle. A new codec's reference is its selected upstream fixtures and its
specified XSH subset, not universal format fidelity.

At close, run one full local integration scope (`cargo dev gate --full`) because
this change replaces a foundational boundary used by all three tools. Run
`cargo dev test linux` through the approved pinned `Dockerfile.test` environment
for Linux-specific coverage the local gate did not provide. Inspect the existing
owners first and reuse identical completed coverage; do not run duplicate full
native suites merely because both commands can dispatch them. Preserve complete
counts and skips. Native macOS tests run only on an available macOS host; absent
platform evidence is an explicit external qualification blocker, never a pass.
This future launch's final full run is the only comprehensive milestone; item
loops and phase gates remain scoped. Preflight itself runs none of these full
commands and authorizes no new CI work.

### Final cost acceptance

Freeze the immediate pre-cutover baseline source and products with identical
nightly, ef7 container, target, production flags and inputs. Reuse the existing
campaign adapters. The protected 57a/R13 products are historical references,
not this migration's automatically comparable baseline. Do not rebaseline to
hide an added codec, new dependency or regression.

| Cost | Final acceptance |
|---|---|
| Binary size | Each actual production `xsh`, `xsht`, `xshi` at most 5% above its matched pre-cutover product. Count all enabled codecs and embedded bytes. |
| Memory | Peak RSS and retained memory independently no more than baseline + max(10%, 2 MiB) on unchanged workloads. Also test bounded document/stream memory near limits. |
| Build | Incremental and release median CPU at most 10% above matched baseline outside observed spread; same flags, jobs, cache state and absolute staging path. |
| Unchanged startup | No additional helper process/failure; per-case increase at most max(5%, 1 ms), subject to paired spread. Include no-codec, JSON and YAML usage. |
| Existing substantial JSON work | Median CPU at most baseline + max(5%, paired spread), with the same complete outputs and workload size. No speedup requirement. |
| New codecs | Publish complete-work CPU/RSS/output results for YAML/TOML, CSV streams and binary round trips. They have no old-codec speed baseline; make no cross-library speed claim from upstream numbers. |

Measure small and large decode/encode, pretty output, nested/numeric-heavy JSON,
JSON Lines and the shared real JSON-to-Tokei/representative corpus where still
valid. Compare direct adapter and current baseline. A Deser-value route is a
bounded scratch diagnostic only if it answers a measured allocation question;
it is not a prerequisite benchmark framework or shipping fallback.

Use at least five alternating pairs per timed claim; median and spread for
compute, appropriate percentiles only for actual tail claims. Verify useful
output and failures outside sampling. `perf-gate-ready` must exit 0 immediately
before timing and `perf-gate-ready -1` immediately after; discard contaminated
runs. If the host stays busy, report exactly `perf gate deferred: host busy`,
continue independent features and pause final acceptance. Size/RSS still run.
A greater-than-2x CPU or RSS regression on a matched complete workload earns one
bounded investigation before proceeding; it does not authorize a broad rewrite.
Final budgets remain binding even if features are complete. Never retain two
JSON implementations to evade the migration's cost gate.

## Execution boundary and non-goals

The owner launched execution on 2026-10-26, authorizing implementation,
vendoring into tracked source, local implementation commits and the named final
manual verification. No repeated approval is needed for routine choices already
settled here. Effects outside that grant park only the affected item.
Pushes, upstream edits, other-repository changes, destructive user-data
operations and installed-system changes remain forbidden.

Contract changes to surface in the final report: typed intrinsic schema operands,
new codec modules/operations and error family, fixed resource limits, JSON/INI
duplicate/error changes, INI dialect changes, Rust host JSON facade replacement,
format/extension subsets, stream recovery and filesystem policies. Migrate all
tracked callers in this repository and update tests, authored docs and generated
references together. Do not silently change persisted tooling wire schemas.
If an external consumer must migrate, identify the new canonical API; do not
retain the old implementation or deprecated aliases for it.

Not in scope: excluded codecs, arbitrary extension/cycle fidelity, new runtime
value kinds, language reflection/generics, generated schemas, ORM/database work,
compile-time configuration reads, JSON Schema, AST/comment-preserving editing,
plugin loading, network discovery, transcode API, Tokio, foreign-language codec
commands, new process runtimes, unrelated filesystem/network redesign, CI and
formatters. Do not revive older performance or Pyramid campaigns as prerequisites.

## Source map and references

Read the topic owners before editing: [tour](docs/user-tour.md),
[SPEC](docs/SPEC.md), [architecture](docs/ARCHITECTURE.md),
[design](docs/DESIGN.md), [testing](docs/TESTING.md) and the
[paused campaign](docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md).

| Concern | Existing owner |
|---|---|
| Runtime data and keys | `src/runtime/value.rs`, `src/map_key.rs`, `src/runtime/eval/type_view.rs`; public value facade in `src/execution.rs`. |
| JSON and INI | `src/modules/json.rs`, `src/modules/ini.rs`, `src/runtime/eval/json.rs`, `src/runtime/eval/lowered_run.rs`, `src/lib.rs`. |
| Schema admission | `src/runtime/eval/require.rs`, `src/runtime/eval/lower/types.rs`, `src/sema/check/expr.rs`, `src/syntax/parser/expr.rs`, indexed schema/producers under `src/runtime/eval/indexed/full/`. |
| APIs/errors/dispatch | `crates/xsh-registry/src/signature/modules.rs`, `crates/xsh-registry/src/runtime_op.rs`, `crates/xsh-registry/src/errors.rs`, `src/modules/mod.rs`, `src/modules/signature.rs`, evaluator dispatch. `module_codecs.rs` currently handles hash/byte operations, not a preexisting generic codec framework. |
| Language tests | `tests/xsh/stdlib/json.xsh`, `tests/xsh/stdlib/ini.xsh`, `tests/xsh/record-exactness.xsh`, native stream/API tests and Rust host-boundary tests. |
| Other miniserde consumers | `src/modules/locale.rs`, Linux net/storage fixtures, `src/runtime_stats.rs`, `src/runtime/eval/route_witness.rs`, `src/runtime/eval/indexed/full/formal_bridge.rs`, `tests/runtime/common.rs`, `tests/linux_priv.rs`, `tests/libc_hygiene.rs`, `crates/xsht/` sources and tests. Re-run `rg -n miniserde` before claiming complete removal. |
| Authored docs | `docs/templates/SPEC.md`, registry reference/docs sources, `docs/templates/user-tour.md`, codec examples and the existing docs-generation owner. |

Upstream source, inspected at the frozen commit:

- [Deser](https://github.com/mitsuhiko/deser/tree/8c15055178fcf2488dcd1ac70e6d1ff18689b486): core event/sink/emitter drivers, lexical/implicit atoms, extension metadata and explicit limits.
- [Limitations](https://github.com/mitsuhiko/deser/blob/8c15055178fcf2488dcd1ac70e6d1ff18689b486/LIMITATIONS.md): thread-safety, whole-document buffering, dynamic dispatch and arena safety constraints.
- [Serde comparison](https://github.com/mitsuhiko/deser/blob/8c15055178fcf2488dcd1ac70e6d1ff18689b486/SERDE.md): architecture reference, not a replacement language contract.
- [Benchmark methodology](https://github.com/mitsuhiko/deser/tree/8c15055178fcf2488dcd1ac70e6d1ff18689b486/benchmark) and [compile/size notes](https://github.com/mitsuhiko/deser/tree/8c15055178fcf2488dcd1ac70e6d1ff18689b486/compile-times): motivation to measure locally, not XSH acceptance evidence.
- [Reylo](https://github.com/colinhacks/reylo): inspiration for one owner of types only. No database access, code generation or external-file type discovery is imported into XSH.

## Decision and campaign log

2026-10-26 launch: clean `master` at `9d893e007`, newer than the preflight
checkpoint. The approved `xsh-test` image is present at `ef7c79658855`.
`cargo dev gate src/modules/json.rs src/runtime/eval/json.rs --plan` selects
10 Change cases, including native stdlib, runtime, facade, registry, signature,
API and lowering coverage. Full remains reserved for closeout.
Existing verification products pass all 18 JSON native cases and the selected
INI file case, with no skips. These products establish runnable comparison
behavior, not matched production cost evidence or current-source full acceptance.
The inherited 230-pass/two-failure integration attempt remains unqualified.
One foundation lane owns the adapter and vendoring; the coordinator migrates
disjoint host consumers. The next acceptance wave is the combined S1/S2 tree.
No additional codec writers start before that wave passes.
The owner confirms no macOS test host is available. Native macOS verification
remains an external final-qualification blocker. Implementation and Linux
verification continue.
The native corpus rejects four scratch mutations: a changed JSON value, missing
child output, a wrong error category and a wrong lowered-record projection.
Reference executables are retained under `.work/tmp/deser/reference` until parity
is accepted. A detached launch-source checkout at `.work/wt/deser-cost` supplies
the matched baseline for S6.

S2 dependency ruling: the existing `rustybench` 0.1 dev-dependency also brings in
miniserde. Preserve its benchmark and allocation contracts with a local library
copy under `third_party/rustybench`, migrating only JSON report derives/rendering
to Deser. Omit the standalone CLI and its fixtures. This necessary host-consumer
migration adds no production dependency or benchmark framework. The external
benchmark driver and its repository remain unchanged.

S1/S2 focused checkpoint `6a4bdfd56`: upstream core 403/403 and JSON 39/39;
direct decoder 3/3 and borrowed runtime/lowered encoder 5/5; native JSON 21/21,
wire enums 11/11, locale 4/4 and three focused consumers. Host facade, locale,
route witness, formal bridge, runtime statistics and benchmark-report checks
pass. XSHT trace 8/8 and coverage 5/5 pass. Actual verification binaries pass
highlight reconstruction, trace events, native-event reconciliation and
coverage-output checks. No miniserde package remains in Cargo.lock.
Shared `cargo check` and the interactive benchmark consumer check pass.
The first verification build's 4,273,080 KiB peak RSS is resource-admission
evidence only. It is not a production memory result or timing comparison.
Compilation limits the loop. Freeze behavior changes before optimized builds
and batch acceptance. The next phase remains blocked on its Change gate and
the last two custom host JSON transports, not on more implementation lanes.
The first pinned Linux Change attempt stops before tests because Cargo denies
vendored manifest warnings. `a0264db5b` removes redundant readme metadata,
declares upstream lint ownership and keeps the YAML pilot test-only until S3.
`CARGO_BUILD_WARNINGS=deny cargo check --locked -p xsh -p xsht -p xshi` then
passes. Retry the same selected scope after integrating the host transport repair.
An external pull also adds the documentation-only `757722845` to master during
execution. Its updated design guidance is read and retained. It changes no
baseline implementation or frozen cost source.
`aaeaf17f3` removes formal transport JSON tokenization/punctuation and statistics
JSON formatting. Formal transport checks pass 7/7, including duplicate/schema
mutations and existing byte/depth limits. Statistics checks pass 6/6, including
the unchanged exact allocation-report golden, newline and full-u128 timing
values. Public signatures and wire schemas remain unchanged in this repair.
The same 18-case pinned Linux Change gate now runs on the combined source.
Its native stdlib case reports 913 passes, one failure and 34 skips. The failed
compound-assignment test reduces to `map.empty().set("entry", [1])`: cold
verification cannot derive the empty receiver's key/value contract. The retained
pre-cutover executable reproduces the identical failure without any JSON call.
An explicitly typed intermediate empty map and a literal-map control pass.
Treat this as exposed baseline verifier debt blocking the selected gate. Repair
only this producer-proof gap, preserve cold authority and retain a minimal native
regression. Other gate cases remain unrun; no S1/S2 phase exit is claimed.
`ac91aa5b0` authenticates the physical zero-argument MapEmpty constructor against
its closed method domain, then preserves the existing receiver/argument/effect/
result checks. The minimal native reproducer fails before repair. New cold
factory/key/member mutations and the existing cold-collection controls pass.
An independent focused source review finds no authority bypass. Alias/named-call
probe failures remain separate baseline debt and are not expanded into a sweep.
`f42b527ea` retains a nested pretty-output golden. Compact and pretty output for
18 documents matches the runnable reference byte for byte, including numeric
boundaries, exponent/negative-zero normalization, controls and Unicode.
That retry remained red; the later acceleration ruling above allowed isolated
S3 delivery while retaining this acceptance failure.

2026-10-25: owner overrides the draft's all-17/published-crate/compatibility
requirements with a pruned source vendor and hard cutover. Preflight selects
seven codec crates (eight identifiers including TSV), explicit lexical schema
calls, direct runtime adapters and workspace-wide dependency removal. The draft's
XML/plist/object-graph/env/debug/form/transcode capabilities are out of scope;
their security and data-model sections are superseded by physical exclusion.

2026-10-26: foundation probes pass; two existing focused native cases pass.
No implementation, production performance gate, full suite or macOS run started.
The hard-cutover semantics and final proof floor above replace the draft's
compatibility and “gates pass or blockers” completion ambiguity: externally
blocked final acceptance is a pause, not completion.

## For the owner afterwards

Read the accepted codec/API contract and intentional semantic changes, inspect
the vendor/dependency reduction and final matched cost results, and review local
commits on master. No cherry-pick is needed for work already integrated there.
The final report names exact revisions, modules/functions/traits changed,
upstream pin/features, adapter/schema mechanism, all included/excluded codecs,
remaining representation restrictions, miniserde/dead-code removal, native/Rust/
Linux/macOS results, production sizes/RSS/timing and every pending blocker.

Launch prompt: “Execute campaign-deser.md on master, including its repo-local
implementation commits and final manual verification; preserve its authority
boundaries and hard-cutover decisions.”
