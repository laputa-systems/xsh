# Consolidation restart handoff

## Reader integration and native image recovery — 2026-10-11, `a5ad6e9f`

Accepted since the last checkpoint: isolated command decoder coverage
(`4e77164b`), shared borrowed type naming (`0922848b`, -2 production lines),
dead archive/command/simple-error representations (`08a9fc5a`, -271), and
borrowed pattern matching (`ed805ec8`, -143). Surviving instruction ordinals
retain holes 97, 98 and 147; pattern ordinal 14 is also a hole. Type naming
uses the executor's existing `Error` label for process-error values. Pattern
lane native gates pass 74/74 and schema/store/corruption units 20/20. Three
existing negative fixtures now have the missing match-arm commas and retain
their original checker assertions; their parser failures were reproduced on
the source-matched frozen reference before repair.

`a5ad6e9f` fixes CoreAssert grouping at its actual cause: a selected module
can have statements while the workspace's global root is empty. The existing
grouping normalizer now receives the selected source identity. The original
fixture and recheck, a selected-module unit, and a complex-expression/source-
order native regression pass. The complete lint table remains private and
still requires deletion offsets; this repair does not accept that table.

The decoder closure ledger now reconciles all original 115 rows, the final
41/72/2 canonical counts, finalized pattern and StmtProc overlays, and exact
writer/caller proofs for every remaining zero row. It explicitly supersedes
the earlier 110-row prose. There is no unresolved source-reachable row in
that frozen inventory. It qualifies reader migration, not final current-tree
coverage. Ledger JSON SHA is
`8d4127de341932898b6a94499492c4c574c33c5dd6f5abf55fb2c4ab2cbf6d82`,
in the preserved coverage-baseline worktree's
`.work/consolidation/coverage/profile-aggregate/decoder-closure-ledger/`.
Original failures, corrupt profiles and the external dependency mismatch
remain recorded. Metadata preflight confirms all 10,996 original raw files
and 115,716,334,536 bytes remain present at their expected sizes.

External Docker cleanup removed native index `65dc744f...`. Recovery from
cached layers produced protected tag `xsh-test:consolidation-native` with
index `3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
Its AMD64 manifest
`e8459b82618dff6d193055ecb58e9988382beccd4fb8f54f8e6082836f90b6cf`
and all 15 rootfs layers are exactly the old payload; only the outer
provenance index differs. Compiler, LLVM, Clang, musl, CRT hashes and alias
smoke checks match. The three ignored native wrappers now pin that index
and set `RUSTUP_HOME=/root/.rustup` for unprivileged compiler probes. Receipts
are under root `.work/consolidation/infra/recovery-native/`. Do not substitute
the mutable `latest` tag or change the ARM image.

Private work remains active: complete module records pass unchanged
1,415-source/four-mode checker parity but are over their source budget;
the full stage engine and borrowed views are undergoing final metric/gate
checks; typed/frame and filesystem cohorts await combined integration;
typed callee proof must preserve indexed and composite callable initializers;
the lint walker still needs genuine graph/collector deletion offsets. Final
source shrink, zero structural counts, documentation, platform, semantic
review and performance acceptance remain required.

## Recount and reader preparation — 2026-10-11, `78aaa3af`

The exact `6db4c917` production recount is complete. `src` has 193,062
lexical lines against 189,274 at the start, only 112 below its interim
193,174 ceiling. `crates/xsht/src` has 49,710 against 49,968, below its
50,368 interim ceiling. Positive runtime and module work therefore waits for
a genuinely source-deleting batch. The full private lint table and proof
transport also need additional deletion before their combined batch fits the
xsht envelope. Final strict shrink remains required for both roots.

The same source-bound scan reports zero lowering type resolvers, one static
type predicate, two storage mappings, 916 manual payload calls, 42 optional
raw calls, five remaining wide arena matches, five machine factory sites,
and 10,770 lines under the remaining indexed blanket. The live inventory at
this revision is still 158 instructions, 18 patterns and 46 stages. Recount
and provenance artifacts are in the private `root6db4c917` worktree under
`.work/consolidation/recount-6db4c917/`; scanner SHA is
`da2c3e70cf6cc08e79ff7eba3ce5ebaf05848203cd7f21797150141155f35e79`.

`c78a8031` accepts the focused pattern mismatch test. Its isolated frozen
profile proves the previously zero Facet and Tag completion reads; the
coverage overlay is separate from the original aggregate. Exact writer/caller
audits qualify unreachable specialized error-pattern, archive and recursive
pattern-control paths. The frozen StmtProc gap now has an isolated native
profile with nonzero payload and completion reads; its new fixture still
needs the current opaque-Any validation migration before integration.

The CoreAssert migration fixture fails identically under three frozen pairs.
Its retained recheck assertion also rejects the fixer's redundant parentheses,
so this is an existing invalid-fix bug, not permission to change the expected
text and accept unchecked fixes. The repair remains private. Independent
frame review found that operand-classified rows could enter a new machine
through an enclosing in-place selection; the private fix marks those rows
scheduled and retains a test-first verifier regression. Neither candidate is
accepted yet.

The campaign skill now records the owner's Luna xhigh policy. Stopped
accepted-lane incremental caches were removed with receipts under root
`.work/consolidation/restart/`; binaries, harnesses, source evidence and all
original coverage data remain preserved. No final gate is claimed.

## Integration checkpoint — 2026-10-11, `bfb18460`

The campaign remains active under the owner's unattended completion instruction.
All subagents use Luna xhigh. The historical checkpoints below retain their
source-qualified results and do not describe the current tree.

Since `9ff2c334`, the integrator accepted stage order/drain oracles
(`b6c737e4`), exhaustive lint and semantic policy cases (`1ae2c671`,
`cb5ea4fc`), borrowed captured-resource traversal and balanced failure cleanup
(`fd27eb38`, `f6fe4479`), and strong script-stream ownership with removal of
the global producer registry and statement sweeps (`a5f8b89f`). The latter's
follow-up `08479519` clears an unstarted cancelled producer's frame; a
test-first host regression reproduces the retained-resource cycle without
that fix. Ownership, producer and error-cleanup lane gates pass 26/26, 6/6
and 16/16, with eight capture lifecycle units passing. The last joint frozen
pair is `joint-cb5e-bin`, before strong ownership; its 48 combined native
cases pass. A source-matched joint build after the next accepted batch remains
required.

Structural scope correction is integrated in `381e06b6`, with documentation
clarified in `bfb18460`. Original baseline bytes are unchanged. The verified
companion measures blanket scope with the same canonical scanner on both
revisions: start 47,061 lines and `9ff2c334` 10,770 lines. Item allowances are
inventoried separately; they are not a replacement for blanket removal.
Provenance and 38 ratchet / seven gate cases pass. The latest root has not
yet been recounted. Final source shrink and zero structural targets remain
unchanged.

Frozen baseline coverage completed 6,122 outcomes in 545 groups, plus 18
isolated exact-ID replays. Original 107.77 GiB raw evidence, original failures
and invalid/truncated profiles are preserved. Bounded aggregation and exact
object attribution are complete; the sole final xsh mismatch is an external
`ignore` dependency function from mixed-domain data, with its exact attribution
recorded. The source-owned decoder inventory initially has 41 covered and 72
zero rows in 21 groups; a separate targeted pattern profile proves two more
rows. Missing rows still require source-reachable tests or exact writer and
caller closure. Passing stage tests did not hit the old indexed block decoder
rows and are not coverage acceptance. Artifacts live under
`/home/josh/d/laputa-systems/xsh-consolidation-lanes/coverage-baseline/.work/consolidation/coverage/profile-aggregate/`.

Private work remains unaccepted: the full lint table, source-owned lint proof
transport, complete per-module records, typed-callee verifier follow-ups,
typed frame cohorts and the full 46-row stage engine. W9 may retain one
file-local effect probe and final pass, as its approved design specifies;
preparation reuse around repeated whole-bundle probes is not its end state.
Stage scheduling, runtime reader migration, final dead-code and classifier
closure, docs, final platform and performance gates remain outstanding.

## Integration checkpoint — 2026-10-11, `9ff2c334`

The campaign remains active and unattended, with Luna xhigh subagents.
The latest joint optimized build passes in the pinned native image. Frozen
binaries, source hashes and the build log are under
`.work/consolidation/restart/joint-9ff2-*`. Combined native gates pass context
scopes 25/25, typed callables 20/20, formatter layout 65/65 and modules 31/31.
The existing binary-only `lint_files` warning remains; no blanket was added.

Since the previous checkpoint, accepted changes repair imported error-facet
canonicalization (`051a828d`), remove unused semantic helpers (`0d46d837`),
share guarded storage conversions (`66ce7b80`), generate borrowed stage views
(`8187bd11`), publish nested checked RunForm types (`35a132d5`), preserve string
contents during broken-call formatting (`aa262c64`), and stabilize example and
process-fixture verification (`45f5af63`). Resolver and storage-authority
metrics now classify their actual source shapes (`e6aa1c0d`, `9ff2c334`).
Borrowed views are preparation, not completed runtime-reader migration.

Baseline canonical coverage has completed all 545 groups and 6,122 outcomes.
Original failures and profile gaps remain explicit. Supplemental exact-ID
replays are running; the original 107.77 GiB raw evidence remains untouched.
Complete combined counter coverage and missing-arm tests still gate runtime
reader acceptance. The captured-resource traversal is being rebased and
verified against this joint root before strong producer ownership proceeds.

Full module-session checking, shared lint proof/fix validation, the complete
lint rule authority, typed callee verification, remaining exhaustive policies,
frame/stage scheduling, dead-code closure, generated documentation, final
platform and performance gates remain in progress. Private lane results do
not substitute for integration acceptance or final campaign completion.

## Current integration checkpoint — 2026-10-11, `65594365`

Follow-up checkpoint `ffabb4c7`: the joint `65594365` build passes in the
pinned native environment. Its frozen binaries and source manifest are in
`.work/consolidation/restart/joint-6559-bin` and `joint-6559-source.json`.
Focused combined native gates pass 15 pattern conditionals, six module-command
contracts and 16 test declarations. The initial invocation incorrectly passed
multiple filters; its rejection is preserved, and the corrected sequential
invocation ran all 37 cases. Borrowed pattern views and bridge fixes are now
integrated (`602ccd51`, `f7df0fe8`); the rebuilt root static harness passes
five actual schema/store/corruption cases. Linux module allowances are removed
in `45e70000`; this warning-only change has Linux compiler evidence and no
claim of non-Linux validation. `ffabb4c7` removes temporary child vectors
from annotation linting, with 60 focused lane tests passing.

Baseline coverage is through group 400, 4,400/6,122 outcomes. Group 385 has
three instrumented parallel-map timeout failures with valid profiles; those
failures are retained. Existing profile gaps remain groups 30 and 181.
The full W9 driver needs a real owning multi-entry session and complete
module output projection; earlier source-aware plumbing was not such a
session. Necessary temporary budgets are expanded in ITEMS for that driver
and the full lint table/enabled-state consolidation, without relaxing final
source reduction or structural acceptance.

The owner’s unattended completion instruction remains active. All subagents
use Luna xhigh. The historical checkpoints below are retained; they are not
the current branch state.

Since `d15511a3`, accepted slices include checked constructor facts, shared
stage authority, grammar ownership repairs and the fixed-seed CLI grammar
regression, the first forced frame cohort, exhaustive formatter/checker/lint
and runtime policies, resource escape lint repairs, transfer telemetry, and
script-backed JSON profile publication proof. The latest batch integrates
runtime policies (`a63b00ef`), mutation recognition (`4aec944a`), required
replacement argument printing (`176061b0`), generated stage metrics
(`3392c25b`), and immutable borrowed workspace entry views (`65594365`).
The borrowed view adds public `CheckBundleView` entry-range APIs; the arena
remains immutable and site IDs remain stable. Its lane gates passed the
two-root isolation proof, module/cache integration and optimized checker tests.
The fuzz lane passed the unchanged 1,500 grammar and runtime seeds and 1,000
mutants; the runtime gate took 13.41 seconds. These remain lane results until
the latest joint candidate is verified.

The joint optimized build is running. Its initial attempt encountered
read-only cached compiler outputs; the integrator detached 521 read-only
target artifacts without modifying their linked cache originals and restarted
the same pinned build. Both logs are preserved under
`.work/consolidation/restart/joint-6559-build*`.

Baseline canonical coverage has passed group 372 of 545, with 4,088 of
6,122 outcomes recorded. Groups 30 and 181 retain explicit profile gaps;
the original 107.77 GiB raw evidence remains untouched. Runtime reader
acceptance still waits for complete combined coverage and missing-arm tests.
The captured-resource fix, typed frame continuation cohort and borrowed
pattern views are private review candidates, not accepted root changes.

Full module record reuse across entries, the complete lint rule authority,
all frame/stage scheduling and borrowed runtime reader migrations, remaining
wide policies and dead-code blankets, generated docs, final platform gates,
semantic reviews and comparative performance acceptance remain outstanding.
The imported error-facet regression is bracketed between `b7e5c5f4` and
`edac89d0`, and is being repaired with the preserved assertion.

## Current integration batch — 2026-10-11

Latest root batch ends at `d15511a3`: ListRest codec validation ordering is
repaired (`8d7ca885`), ten unused checker impl allowances and their dead arity
helper are removed (`ce633f7b`), nullable storage mapping is shared
(`5accb8c4`), lint facts share one immutable checked output (`bc1708cb`), and
redundant-close uses are collected during the existing walk (`10ac1f4a`).
The reachable module stream fixture is corrected separately (`41c4a0f4`),
and immutable preparation is reused across effect probes (`d15511a3`).
The ListRest lane passed 15 pattern-conditionals, 24 pattern tests, and the
retained verifier corruption unit. Lint lane gates passed 244 lint units,
38 CLI lint units and five redundant-close units. Module preparation passed
its reuse/domain unit, shared-arena integration case, module/stream native
gates and all 46 sema integration tests. These are lane results; the joint
optimized build and focused combined gates remain in progress.

Joint `d15511a3` optimized build now passes. Its frozen pair is in
`.work/consolidation/restart/joint-d155-bin` (xsh SHA-256
`a10e1bdd27d12a5a6e065a2518360a8f3b8478e7f4b85b255b4ab4fe5ce40dde`,
xsht `39271709b4eb79388d70b6d4a46ca2398823d253ca796c2193ba9f6c75ff17b1`).
Combined native gates pass pattern conditionals 15/15, compact declarations
4/4, module-command contracts 6/6 and resource ownership 22/22. Both the
ListRest corruption unit and module preparation/domain unit pass one actual
case each using the rebuilt static test harness. The initial timeout argument
`30` was invalid, and an initial wrong module filename ran zero cases; those
logs are retained and excluded. Corrected runs use `30s` and the actual file.
Thirteen lint-proven redundant FsRoot defers are removed in `78052d1d`;
all 13 nearest native cases and the retained child/parent lifecycle case pass.
The intentionally retained child defer still has its lint finding.

The public lint Rust options now accept shared `checked_output` rather than
21 separately copied fact maps; synthetic maps remain test-only. Cross-root
module reuse, the complete lint declaration/dispatch table, all frame and
runtime-reader migrations remain unfinished. A returned producer stream still
closes a captured FsRoot too early; the retained failing native test owns the
capture-transfer fix before reachability sweep removal. Grammar repair must
preserve existing parser ownership behavior rather than change the language.

The campaign remains active under the owner's unattended-completion instruction,
using Luna xhigh subagents. The older reboot pause below is historical.
Current implementation includes the complete BindingId adapter (`498e02e0`,
`57aa1abb`), R3 checked resource facts (`1f6c6db1`), full explicit annotation
facts (`01f786b9`), named checker profiles and unconditional warning publication
(`c8fc9e26`, `530d957c`), module namespace isolation (`5a4d87ff`), eight further
lint hooks (`7d384613`, `d873b984`), and the error-family fixture correction
(`5d2661a0`). `CheckOptions::migration_diagnostics` is removed; named execution,
check CLI, and interactive factories choose entry policy without hiding warnings.
Execution gates on errors, while checker-only entry constructs remain rejected.

The latest accepted slices are resource runtime fixture coverage (`b5f9a685`),
opaque FsLock/API and missing-module fixtures (`62d67705`), grouping traversal
and retained slice-range regression (`4b2363ba`), stream ownership/worker cleanup
(`f10506cb`), and exact local/capture callable verification (`d325c86c`). Each
has its lane's focused green gates; the joint all-feature XSH/XSHT check passes.
Combined optimized native gates are pending. Neither stream cleanup nor grouping
closes the larger resource or traversal workstream. The R3 runtime bridge awaits
budget and strict missing-fact review; the Any checker remains unintegrated.

Baseline canonical coverage is running under the shared suite lock with the
sealed instrumented pair explicitly on PATH and XSH_BIN, unprivileged UID 1000,
and tmpfs test storage. Original 107.77 GiB raw evidence remains untouched.
New group profiles are hashed and validated before bounded LLVM aggregation;
only validated new raw inputs are deleted, retaining aggregate evidence rather
than claiming lossless individual raw archival. Earlier wrong-environment and
wrong-top-level-binary phases are preserved as superseded supplemental evidence.
Coverage and final runtime-reader acceptance remain incomplete.

Further integration: `c49c05e5` replaces `lint.prefer-with-scope` with
default-on `lint.redundant-close` and shares checked output through lint workers;
its focused lint gate passes 282/282. `349124ad` corrects the whole-initializer
run propagation fixture while proving guarded hops and runtime output unchanged.
W9's borrowed reachable view and per-root isolation proof are `a8cd70a2` /
`a963a005`; repeated probe preparation remains for the next slice. Any opacity
and its nine hidden-fixture migrations are now integrated (`4a09be2d`,
`2be723b9`, `c2676b8b`). The joint optimized build passes; focused opacity
102/102 and migration 155/155 gates pass. Known shell recovery and With
initializer failures remain assigned to the binding owner. A newly exposed
`fs.dirs` owner-registration regression is assigned to the stream owner;
do not claim the full candidate gate green.

`1e35ab66` removes the inherited CLI dead-code blanket and unused Rust
convenience APIs `check_script`, `check_script_with_options`,
`check_paths_with_options`, `check_paths_with_summary_options`, and
`CliOutput::success`. CLI dispatch still uses `check_paths_timed`; its focused
check/coverage gates pass. Legitimate exported coverage constructors and syscall
summary data retain item-level allowances. `4a7447b9` removes four lint-proven
redundant FsRoot defers in cp/mv/curl/split, preserving explicit unlocks;
focused core gates pass with two TLS fixture skips. The SPEC template now
records affine local checks and stream cleanup; generated docs are still pending.

R3 bridge integration is now `e513f574`, `1d452ec1`, and `eb70eb1a`, with strict
`ResourceTransferSite::Assignment` facts: locals carry explicit false; missing
outer facts block lowering. The complete bridge uses the expanded +340 interim
allowance and still requires final source deletion offsets. Binding recovery
and in-place With error handling are `ae9269bd` / `9c95f599`. The joint optimized
pair at 9c95 passes resource ownership 21/21, With 19/19, mistakes 5/5, nine
resource units, the missing-fact blocker unit, and three callable units. An
initial exact unit filter ran zero cases; it is retained but excluded, and the
corrected filter passes one actual case.

The latest integration batch includes shared field lookup (`b7e5c5f4`), two
pattern traversal helpers (`7aeeceee`, `bdd01efb`), trace/table blanket removal
and unused helper deletion (`213f5a08`), checker declaration allowance removal
(`c0fe54a7`), checked type-fact resolver contraction (`edac89d0`), checker-side
main spread diagnostics (`6d42c588`), and the generated 18-pattern codec plus
coverage-tool schema repair (`a99aba94`). Undocumented Rust trace helpers
`TraceSink`, `MemoryTraceSink`, and `TraceNormalizer` are removed; normalization
needed by tests is test-local. The coverage report now includes
`pattern_variants`. R4 delegated-owner and fs.dirs registration fixes are
`2147695c` / `a514c417`; their lane gates pass foreign workers 6/6, filesystem
24/24, and stream producers 5/5. Current joint compile/build gates are queued.
Reachability sweep removal and one expression-pattern success scan still need
their typed lifecycle proof; neither is silently deleted.

W9's canonical no-argument baseline lint completed on 1,160 discovered files;
the contended 160.97-second run and same-process profile are setup attribution,
not comparative performance acceptance. ARM compiler/environment recovery is
verified, but final ARM product and release gates have not run. Generated docs,
structural closure, fixed-seed reliability, semantic review rows, and comparative
timings remain required before goal completion.

## Resume update — 2026-10-11

The owner resumed the campaign from this checkpoint and changed all subagents
to **gpt-6-luna at xhigh effort**. This supersedes the older Sol-only policy
below and in the campaign skill. The preserved checkpoint remains historical
evidence; interrupted gates still require rerunning. Integration resumed at
`e8533c12`. Docker was stopped after reboot; the integrator restarted the
daemon, but the pinned image is not currently listed. Builds remain held
while the preserved test environment is recovered.

Recovery update: the original native image is absent from Docker's store.
The compatibility session rebuilt the byte-identical `Dockerfile.test`
(SHA-256 `7b8017b69d3ecc51f04ac2fe481af7e5059e0bcf0f1494fb39abecadcfb0e5ff`).
Resumed native gates use immutable image
`sha256:65dc744f6c560f189716ea193bd765149f4691c80392663e630813033ebc53a3`.
The base and rustup installer layers match; rustc commit `574ff7d98`,
LLVM 23.1.1, clang 23.1.3-r0, and musl/musl-dev 1.2.6-r5 match retained
historical evidence. Rootfs hashes differ, so this is regenerated environment
evidence, not byte identity with the old image. Primary/secondary/third native
wrappers pin the resumed image; their original copies remain on disk.
Image metadata and compiler/CRT/libc hashes are saved under
`.work/consolidation/restart/resumed-native-*`. Focused lane gates are released.

Active independent lanes: W1 schema classes, module hooks, checked declaration
facts, source-aware check session and telemetry, BindingId adapter, grammar,
fuzz grammar, lint rule inventory, and opaque Any discovery. Each uses its
existing worktree and preserves unfinished diffs. Source-aware session plumbing
now has an assigned owner; shared-file edits are scoped to disjoint hunks.

### Resumed integration and outstanding gates

Implementation now includes W1 `1867ad25`, class column `8bccf75f`, module
hooks `562bb656`, stage constructor reconciliation `052e6192`, named argument
indexing `6154d0d1`, shared lazy-consumer descent `2b83fc91`, lint selection
`8f68cf1f`, schema-aware metrics `ba614735`, and declaration/type facts
`4b0ebefc`. These are prerequisite slices, not closed workstreams.

The class test passed 1/1 after resume. Joint all-feature compilation passed
after the two stage constructors adopted `call_plan`; the optimized product
build passed. Six actual focused files passed 54 native cases: module commands,
checker commands, command operation boundaries, stdlib CLI, named argument
spreading, and stage functions. Two initially mistyped filters ran zero tests;
those logs are retained but excluded from evidence, and the correct files
were run. The lint selector's literal `lint::` gate passed 282/282. Scanner
tests passed 15/15 and the current generated schema counts 158 instructions.
Generated documentation remains pending; canonical R2 cleanup prose is updated
in `c6b4d97c`. No full integration or ARM gate has passed.

E's focused declaration/fact units and neighboring native cases passed. Its
signature-CLI unresolved-name failure reproduced on exact before-E `d561e2c4`;
it was not introduced by E. A new source-session joint worktree at
`xsh-consolidation-lanes/check-session-joint` carries the required helper and
all loaded XSHT/runner callers. The original dirty session draft is retained.
The BindingId joint worktree is `xsh-consolidation-lanes/lower-bindings-joint`;
it preserves the integrated stage lowering and drops only its two obsolete
pipeline lookup edits. Source-aware checking is needed for imported command
word identities; the adapter fixes the broader unresolved command-splice sites.
The adapter's fold-layout leak has a failing reproduction and cleanup fix.
Do not delete legacy constant maps until the joint adapter gates pass.

Opaque Any broad discovery ran twice with changed source between runs. The
preserved first candidate recorded 3,304 passed / 21 failed / 32 skipped;
the refreshed candidate recorded 2,812 / 54 / 32. Nine hidden fixture files
were migrated in `f632c945` and `ed43ecac`, retaining behavior and rejected
program coverage; their original opacity-pair gates passed 62 and 93 cases.
The refreshed candidate's six original focused files passed 102/102. Most
remaining failures are the known command-site identity regression; R3/R4 and
other distinct failures remain recorded. Any is not integrated or accepted.
Raw logs and classifications remain under the Any lane's `.work/any/logs/`.

The Linux errno test's arbitrary family 9999 returns 1 instead of 97 with
frozen baseline, Any, and current integration binaries in the resumed
environment. Both frozen and current binaries return 97 for `c.AF_UNSPEC`.
Commit `c9a0c980` uses that standard unsupported family and retains the exact
errno assertion; the focused test passes. This repairs an environment-sensitive
test input without changing the socket wrapper or security settings. Evidence
is under `.work/consolidation/restart/net-errno/`; no full Linux gate is claimed.

Source-session/telemetry has a bounded interim +450 production / 2,200 touched
allowance for mandatory source plumbing and retained fact/index accounting,
offset by the prerequisite batch's source reductions. This does not waive
the final reduction requirement. R3 facts use explicit true/false entries at
typed transfer sites; absent facts cannot silently mean scalar values. R4,
W3 fact consumers, coverage resumption, and reveal publication versus entry
legality have assigned preparation/implementation owners. The owner explicitly
requested unattended continuation through completion; no further pause exists.

Further integrated prerequisites: reveal publication/entry legality
`b1b4e99f`, source-aware checking and telemetry `3e7e2164`, and derived
in-place instruction proofs `d51149a5`. The current combined all-feature
compile check passes. Source-session units passed 5/5 and module tests 30/30;
W2's focused verifier and native gates pass, with allocation scaling improving
from a baseline delta of 64 to 32. These do not close their workstreams.
The adapter's current joint tree is `lower-bindings-final` on `d51149a5`;
imported `cd` binding resolution still needs correction before acceptance.
Its approved stream trace fixture migration removes redundant parentheses
and retains the trace assertions. E12/E13 lint hooks are committed in the lane
but await the joint adapter gate before integration.

Baseline coverage continuation is authorized for the remaining 5,586 tests
across 538 invocations, preserving 14 failures and three timeouts already
recorded. Expected runtime is several hours; the shared suite lock and bounded
resources remain required. All 103 flagged raw profiles were accepted by the
pinned LLVM tool; one empty profile contributes no coverage. Final aggregation
and API counter completion are still pending. W2 follow-up, W4 session closure,
W5 behavior traversal, W9 module reuse, R3 runtime transfer transport, and dead
code inventories now have independent preparation owners.

Further accepted structural slices: W3 annotation storage facts `b4d5ad37`
(-103 production; seven focused native cases and corpus lowering consistency
proof pass), formatter dead-code contraction `7b231e21` (-39 production and
removal of the formatter's inherited blanket allowance), and generated stage
tags `153d8595` (-43 production; three stage proofs pass, all 46 ordinals
preserved). The unused public `Formatter::format_program_with_source` method
was removed with no repository callers. Test-only source-session option
qualification is fixed in `feaf24df`. The formatter proof exposes a preexisting
slice-range grouping defect; the grouping owner is retaining its per-case
assertions and repairing that defect, not claiming the full proof green yet.

R3 checker/type/transfer facts are ready in lane commit `76fa3df4`, based on
`4b0ebefc`, not yet integrated. Strict units and reassignment pass with the
sealed adapter assignment-root hunk in an isolated joint probe. The runtime
bridge lane uses separate prerequisite commits and a typed instruction sidecar
without payload/tag changes. The full adapter has a sealed 14-file prerequisite
patch (`b57c11f0...9ec526`); its final source-aware arena text and fixture fixes
are outside that earlier artifact. Imported module tests now pass 30/30 and
all 18 expanded native suites pass; the final lint gate remains running.

ARM infrastructure has been recovered from the exact Dockerfile. Final gates
will use image `sha256:7aec577a12163bc2aee78e759dde9ae571c692632183a83a564ae1bb88384026`
and the host-visible `xsh-consolidation-aarch64-5e66b6b8` binfmt registration.
The bounded compiler/QEMU identity probe passes; no ARM product gate has run.
Evidence is `.work/consolidation/infra/arm-environment-current.md`.

Coverage continuation preserves an observed `dmesg` failure and profiler
permission errors rather than normalizing them. The latter contaminate child
stderr and require an explicitly linked retry with writable disk-backed profile
storage. Noninstrumented frozen/current `dmesg` cases both pass; instrumentation
is being investigated before changing that test. Original statuses and profiles
remain intact. Primary compiler launches now have 12-CPU/24-GiB limits;
secondary/third retain their separate bounded queues. An in-place launcher
update caused EOF errors after some successful compiles; affected commands
must rerun incrementally to obtain a valid launch return code.

Paused at the owner's request on 2026-10-11 for a computer reboot. The campaign
is **incomplete**. Do not resume implementation, tests, or profiling until the
owner resumes it. This checkpoint supersedes older progress rows; approved
contracts in `CAMPAIGN.md` and `designs/` remain authoritative.

## Location and preservation

- Integration: `/home/josh/d/laputa-systems/xsh-consolidation`, branch
  `consolidation`. Implementation HEAD at pause:
  `5d1c1623802be11bb3b297f43caeb652570a9f18`.
- Original XSH worktree remains on the campaign start commit
  `5e66b6b896a47d0c692739135d51200b007a2a67`; compatibility worktrees are
  separate and must not be reset, merged, cleaned, or have their jobs stopped.
- XSH lanes: `/home/josh/d/laputa-systems/xsh-consolidation-lanes/`.
- Laputa integration: `/home/josh/d/laputa-systems/laputa-consolidation`, branch
  `consolidation`, at `048592b0473bcee57b5dd0573297115ab78ef633`. The symlink
  migration lane is under `laputa-consolidation-lanes/symlink`; no Laputa edits
  were required. Original Laputa remains untouched.
- All worktrees, Git objects, unfinished diffs, binaries, build caches, logs,
  profiles, launchers, and required evidence are on disk. Nothing required to
  resume is stored in `/tmp` or `/dev/shm`.
- An extra snapshot of all 32 campaign worktrees is saved in integration
  `.work/consolidation/restart/state/manifest.json`. Each tree has staged and
  unstaged binary patches, a status file, and copies of untracked source files.
  These are backups, not instructions to overwrite the live worktrees.
  `.work/consolidation/restart/snapshot.py` recreates the snapshot.
- The six dirty lanes are checker-foundation, coverage-baseline, fuzz-grammar,
  grammar, lower-bindings, and partition-lower. Preserve their current files.
  The baseline coverage lane contains measurement-only overlays, not candidate
  implementation to merge wholesale.
- `.work/consolidation/restart/temp-link-audit.json` records 25,942 examined
  entries under campaign evidence directories and no temporary-directory
  symlink dependencies. Container temporary test inputs can be regenerated
  from the retained source/fixtures/seeds.

The goal is paused, not complete. All active agents were instructed to stop.
Owned test/build containers, queued lock waiters, and lane processes were
stopped/reaped. Unrelated compatibility and other user processes were left
alone. Interrupted gates must be rerun; cancellation is not verification.

## Owner decisions to retain

- Implement the entire approved campaign. Feature and structural completeness
  precede stricter performance gates. Do not treat a partial implementation as
  completion after restart.
- Parallelize ready, disjoint lanes using **gpt-6.1-sol medium** for routine
  work and **high** for complex work. No Luna. Use the orchestrate skill and
  exact source ownership; do not create worktrees or build caches in `/tmp`.
- Approved contracts override contradicted old tests. Retain coverage, update
  the incorrect expectations, and record each migration. Approval is already
  given; do not ask again for the recorded conflicts.
- Development verification uses native **x86_64 musl in the exact
  `Dockerfile.test` image**. Final verification requires aarch64 musl in that
  same environment. Do not substitute a host toolchain or libc.
- Exclude **dev/compat from check and lint while it is WIP**. Preserve its
  explicit tooling/tests. XSH and sibling Laputa otherwise remain in scope;
  avoid heavy Laputa checks.
- No formatter/autofixer, push, new dependency, or broad automatic checks.
  Existing tests that format temporary corpus copies are retained as behavior
  coverage. Test process boundaries with optimized binaries; debug is allowed
  for root library units and narrow compile checks.
- Generated docs are not hand edited. Edit templates/snippets and regenerate
  with `make docs` once the implementation and contracts settle.

## Integrated implementation

The integration branch contains the approved campaign and skill, verification
profile/gate tooling, four pure partitions, child enumeration, structural
scanner/baseline/envelope, and starting feature/comparative evidence. These
prerequisites do not close their semantic workstreams.

Accepted behavior slices include bounded diagnostic excerpts, imported enum
display, buffered exec regressions, boundary contexts, lexical BindingId
facts, the 32 nested-machine limit, socket net effects with clause migrations,
ordinary Optional/named-argument diagnostics, required FsRoot.symlink labels,
opaque FsLock/evaluator tokens, scope-owned root/lock transfer and cleanup,
one borrowed static-type predicate, direct checked stage lowering, common
lint/traversal removals, and the shared user-module graph loader.

Recent integration commits:

| Commit | Slice / completed focused evidence |
|---|---|
| `771ad5a4` | R2 scope ancestry, descendant resource transfer, roots/locks released after defers; 135 unique native cases and eight host invariants |
| `46b87fc6` | One borrowed runtime static-type predicate; 120 native passes, one DNS fixture skip, two units, zero allocation/no-pull invariant |
| `12925062` | Checked stage lowering without whole-program/body clones; 29 native cases; bounded allocation improvement recorded with profile caveat |
| `5d1c1623` | One user-module graph loader for runtime/lint; missing import and invalid UTF-8 contracts; 38 native and 13 units across accepted loader slices |
| `9af93baf` | Lexical identities; 120 native cases and five units |
| `b160fecd` | Optional guidance, label insertion fixes, 97 symlink caller migrations; 35 native cases, four Rust tests, caller/light Laputa checks |
| `1cec6df9` | Eleven ordered lint expression hooks; cumulative removal work continued in the unintegrated lint commit below |

All 116 directly checked Any migration sites are integrated across tokei,
collections, stdlib, NVMe/EFI and system-report tests. The **Any checker is
not yet integrated**; broad discovery of embedded-script conflicts remains.
Do not count migration checks as a whole integration gate. The full system
report dev harness remains unresolved: 223 passed, one powercap replay failed;
that case then passed individually with both baseline and candidate. Preserve
the failure and investigate/recheck rather than calling the module green.

## Ready commits and unfinished lanes

Paths below are relative to the XSH lane directory unless specified. Read the
durable lane handoff before cherry-picking, rebasing, or resuming a build.

| Lane | Preserved state | Resume detail |
|---|---|---|
| partition-lower | **`1cd0e625` unintegrated**: authoritative 158-instruction schema, generated rows/codecs/effects and borrowed views; warning-free all-feature library build, 95/95 units; production net -1,105, 4,369 touched. Separate 86 Operands/72 Scheduled class draft is uncommitted and its queued gate was cancelled. Module companion hooks remain uncommitted deliberately. | `.work/consolidation/w1/REBOOT-HANDOFF.md`, `classes-paused.patch`, layout proof |
| partition-modules | **`96ddf70f` unintegrated**: checked module-command plans/conversions; warning-free optimized build, 30 native and 95 schema units; production +181 within +250. | `.work/module-command/REBOOT-HANDOFF.md`. Order: core `910bd163` (already integrated as `38e92981`) → W1 `1cd0e625` → hooks `96ddf70f`. Local W1 equivalent is `2191aeec`. |
| partition-lint | **`bf62c20e` unintegrated**: removes lazy visitor recursion, preserves finding order; before observable oracle green, after 281 lint units green; cumulative W6 -393 non-test lines. Clean. RuleSelection facade patch is unapplied and unapproved. | Integration `.work/consolidation/infra/lint-rules/PAUSE-HANDOFF.md` and `rule-selection-facade.patch` |
| lowering-costs | **`012a18c5` unintegrated**: parameter name indexing/checked argument grouping; 19 native cases; production -3. Clean. Initial 16,000-field guard timeout retained; exact retry passed in 1.7s, threshold unchanged. | `.work/lowering-costs/`; stage commit `9917f3cf` is already integrated as `12925062`. |
| any | **`020685bd` unintegrated** opaque Any checker; focused 102/102 native cases, production +32/487 touched. The broad run was queued then cancelled before starting. | `.work/any/HANDOFF.md`; `.work/any/run-broad-checker.sh` acquires the shared suite lock. Candidate binaries and hashes retained. |
| checker-foundation | Uncommitted E: checked declarations/type-expression facts, removal of CompactDeclCollector and duplicate revalidation. Earlier compile green; final-source unit/native checks incomplete. Six modified/two new files, production-path net -118/1,062 touched. | `.work/checker-foundation/reboot-handoff.md`; source-aware helper/caller plumbing remains unassigned. |
| lower-bindings | Uncommitted BindingId slot adapter; pinned optimized build green and real argv-splice regression passes after frozen baseline failed at four sites. Wider focused gate never ran: timeout option error corrected in script, not rerun. Net -195/905 touched. | `.work/lower-bindings/HANDOFF.md`, `paused-draft.patch`, `checker-sites.patch`, metrics. Preserve checker facade additions when combining E/module hooks. |
| grammar | Uncommitted repair of deep counterexamples and generator rollback. Two final guards applied but uncompiled at pause. Last normal proof ~88.26% had seed1419 failure; deep proof previously 58.323% failed the unchanged 80% threshold. Net +47/919 touched. | `.work/grammar/PAUSE-HANDOFF.md`, `pause-handoff.json`, `pause-remediation.patch`; focused/normal/native/deep gates pending. |
| fuzz-grammar | Uncommitted W7B recognizer/corpus traversal draft; three library units and 11/13 soundness tests green; five mutant disagreements wait grammar repairs; 1,500-seed runtime property had timed out at 180s. | `.work/fuzz-grammar/`; authorize one bounded longer retry after grammar integration, without reducing seeds. |
| in-place-scheduler | Clean, no implementation started. Exact grants, class/proof design, reachability removals and tests saved. Frozen scalar execution allocations: 143 for one pass, 207 for 17 passes, delta64. | `.work/in-place/HANDOFF.md`; refresh on W1/module/R2 integration and seed an immutable disk cache after its writer stops. |
| coverage-baseline | Frozen production with documented measurement overlays. Canonical and early phase containers cancelled and removed; full LLVM baseline is incomplete. | `.work/consolidation/coverage/REBOOT-HANDOFF.md`, manifest, phase logs, source seals, raw profiles, preliminary reports. Do not restart an old whole driver blindly or duplicate completed phases. |

## Next coordination decisions and ownership

1. Reconcile W1 and module hooks together; retain registered signature pointer,
   scalar conversion order, default/flag validation and corrupted-plan tests.
   Do not cherry-pick companion hooks from two lanes. Runtime manual readers
   have not been migrated yet; starting count is **923 calls across 922 lines**
   plus 43 optional reads. Update scanner counting for the macro schema before
   relying on current instruction counts; otherwise it can falsely report zero.
2. Give source-aware CheckSession/helper plumbing one owner. Required helper
   signature is `check_compact_declarations(program, sources: &SourceMap,
   entry_source_id: SourceId)`. Check each module against its own source ID and
   text; no empty-text/root-source fallback. Update Evaluator, stdlib catalog,
   loader, telemetry and unit callers together. The empty-source helper currently
   classifies a real Boolean command flag as a word. E owns declaration/type
   facts; session adapter ownership was **not assigned before pause**.
3. E's exact four-line constraint_probe publication-history take/restore was
   approved immediately before pause. Semantic tables must stay available.
   Telemetry must count CheckedDeclarations and annotation/template/instance
   type facts. Do not use Arc::get_mut uniqueness: Checker derives Clone.
   Lower-bindings merges already-selected command sites once at publication.
4. Complete the BindingId adapter's focused gate, reconcile its two pipeline
   lookups with integrated stage lowering, then coordinate deletion of old
   PreparedConstants global/tail maps. Deletion was cancelled at pause and has
   **not** happened; do not remove fields while old readers still exist.
5. Apply and test the shared-loader's two stale project-module-path expectation
   migrations from check-session `.work/check-session/`, retaining missing-import
   behavior coverage. Resolve reveal_type publication versus entry legality
   explicitly against the approved same-diagnostics contract; do not invent a
   runtime spelling or silence a diagnostic to bridge the conflict.
6. Resume W2 instance marks using generated operand traversal and the single
   schema class column. Pure tag classification alone is insufficient: mark
   only instances whose transitive operands cannot run script code, with proof
   height 128. Preserve the independent verifier/cache/corruption oracles.
   Shared apply functions, scheduling all unmarked instructions and the single
   pipeline engine remain substantial unfinished work.
7. Resource R3 local affine checks/MayHoldResource/clone-on-return and R4 stream
   ownership/worker teardown remain **unstarted**. R2 releases explicit_run.rs
   to W2. Coordinate metadata with W1, type facts with E, and stream files with
   the evaluator owner. Remove reachability sweep only after owned streams work.
8. W6 still needs the complete rule table, scope facts, selection/listing, shared
   probe/check session and all remaining private descents. Review its prepared
   RuleSelection patch before granting facade edits. W5 still has remaining wide
   behavior matches and traversal consumers. W3 storage/type-resolution
   consolidation waits checked facts; its borrowed predicate is only one slice.
9. W9 per-module records/isolation/reuse and bundle view remain unfinished beyond
   shared loading and stage/spread lowering. Reuse is within the resolved graph;
   fresh fix-round arenas recheck affected bundles, never reuse an older arena.
10. Integrate Any only after broad embedded-script discovery/migrations and a
    focused candidate gate. Current direct migrations are not proof of hidden
    fixture completeness. Preserve all approved expected-error coverage.

## Verification and coverage limitations

Frozen release binaries are integration `.work/consolidation/infra/baseline-bin/`.
Initial native baseline: **5,996 passed, zero failed, 118 fixture/capability
skips**. Compression focused 52/0/3; initial XSH source check 1,162 clean and
lightweight Laputa 305 clean. These results precede subsequent implementation.
No full final integration or ARM pass exists.

The instrumented Rust baseline is not all green. It retained stale label/API
counts/policy offsets, a signal fixture race, corpus symlink/read failures and
grammar disagreement. Candidate fixes have their own focused regressions.
XSHT retained actual core formatter/desugar failures (`test-stat.xsh`,
`test-ln.xsh`), large layout/redundant-parens findings and a 15s lint-budget
failure under instrumentation. Layout copied discovery also included ignored
measurement `.work` scripts; record that contamination. Do not call instrumented
wall times performance evidence, skip failed coverage oracles, or rerun every
long phase automatically.

Canonical fuzz and native API phases were interrupted for reboot. The native
API suite had discovered 6,122 cases and recorded 519 successes, 14 failures and
three timeouts before interruption; final API JSON is empty, not a passing report.
Fuzz printed 10 passed/one failed during
shutdown, with generated `.replace` positional-argument rejections through
seed1482; retain the terminal failure evidence separately from the interrupted
phase. Linux privilege, final API/report and drift retry phases did not complete.
All 10,996 raw profiles (107.77 GiB) remain on disk; the inventory marks 103
shutdown-window/empty profiles for explicit validation before merging. Both
preliminary decode-gap reports remain on disk. They are
provisional upper bounds, not final runtime-reader coverage acceptance.

Corpus measurement used temporary read-only masks for the known directory
symlink loop and WIP compatibility before Rust corpus phases; masks were removed
before native tests and disappeared with stopped containers. Production walkers
have retained symlink/WIP/error regressions. Never claim an unmasked baseline.

## Environment after reboot

Native image expected ID:
`sha256:99e195582ba55dc427b1551022fbe56f8f344ed7838e971ecba86ce43a7733e9`.
Docker image/registry storage is on disk. Recheck that ID before using launchers.
Required target flags:
`-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol`.
Jemalloc is preloaded for compilation only, never for test execution.

Integration `.work/consolidation/infra/run-native.sh TREE build|run COMMAND...`
is the primary wrapper (12 compiler jobs, cargo.lock). Secondary and third
wrappers have separate locks and eight-job bounds; no shared writable targets.
Use a candidate's own PATH/XSH_BIN for behavior gates. Nightly artifact paths are
recursive `target/TRIPLE/PROFILE/build/.../out/`, not conventional deps paths.
`xsht test` accepts one filter. One full native suite at a time is guarded by the
hard-linked integration and coverage `native-suite.lock`; do not replace that
inode. Lock holders are gone, but retain the files.

ARM prep logs exist, but the **actual pause-time Docker state lacks**
`xsh-test:consolidation-aarch64` and `xsh-consolidation-aarch64-target`. Recreate
them using the unchanged `Dockerfile.test` when final ARM verification is due;
do not rely on earlier preparation text as current image evidence. QEMU is the
disk file `/usr/bin/qemu-aarch64`, not a temporary artifact. Binfmt registration
is kernel state and must be recreated after reboot, using the own-name helper
`.work/consolidation/infra/binfmt.py` and preserved `binfmt-register.log`.
Register only `xsh-consolidation-aarch64-5e66b6b8`, binding the host QEMU path and
the helper into the privileged native container. Preserve other registrations.
`run-arm.sh` and `cleanup.sh` are saved on disk. Revalidate registration/image
before final builds; do not launch ARM work while paused.

## Closing work still required

Generated SPEC/tour/reference docs are deliberately not regenerated yet. Update
R2 canonical cleanup prose (old text still says roots/locks are not scope-owned),
type/fact/session architecture, Optional/label guidance and resource API docs,
then `make docs`. Update TODOs only after each defect is actually closed.

The unchanged baseline scanner reports 189,274 src and 49,968 xsht non-test
lines; 47,705 lines fall under inherited blanket dead-code scopes. Intermediate
envelope ceilings are 193,174 / 50,368 / 52,005. Final closure requires net source
reduction, zero blankets and no envelope. Ten semantic review rows remain
pending; establish verified evidence consumed by the ratchet rather than
manually erasing Reviews. Current verification opt3/no-LTO profile is provisional;
measure opt1/2/3 build-plus-native cost in isolation after functional close.

Final feature probe (manual sites zero), reliability/fixed-seed gates, pinned
seven-tool comparative parity/timings, complete candidate Linux/ARM verification,
cleanup and the closing report all remain. No comparative timings were taken;
prepared suite has 35 parity cases and eight harness tests green. Continue to
campaign completion only after the owner's resume request.
