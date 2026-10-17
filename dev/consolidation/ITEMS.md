# Consolidation items and ownership

Approved campaign: `CAMPAIGN.md`. Start commit:
`5e66b6b896a47d0c692739135d51200b007a2a67`. The accepted designs and historical
ownership records remain below. The [handoff](HANDOFF.md) owns the live queue.

**Current status: active frozen source `7eb62cd99`.** Main and the frozen
candidate use tree `736b4bc16b1bcf07a0250b97e849506c19fb0753`. The
[active handoff](HANDOFF.md#active-candidate) owns exact pins and the live
queue.

The accepted b235 canonical result has 13,916 passes, 78 skips, and zero
failures across 13,994 cases. Phase A reuses it for 7eb because only two
test-only permission spellings changed. The b235 receipt remains pinned to its
original source.

The first b235 R6 attempt failed at stage 6 of 13 on two Clippy warnings after
five passing stages. Stages 7–13 did not run. On 7eb, the fresh R6 run exited
zero at all 13 stages and root accepted it. It recorded 23 boundaries, 58
extra cases with three ignored, two split checks, two privileged checks, and
18 traceroute cases with two `NET_RAW` skips. The [R6 evidence index](winddown/evidence/final-correctness-gates/README.md) preserves the failed b235 attempt and accepted 7eb run.

Phase A reuses the seven accepted QEMU cases for 7eb by scope. R6 products use
a separate variant and are not compared with the protected canonical
products. No 7eb canonical build or QEMU guest run is claimed. The accepted
tail repair passed all eight written/desugared selectors. W9 parity at
`0deeb9c7` remains reusable under Phase A; the b235 input-0 probe is not a
full-corpus result, and the interrupted 3c repeat earns no credit.

Compatibility origins and exclusions reconcile to frozen denominators. No
new upstream pass is claimed. All 428 route runtime invocations completed
without failures, with source checks clean. The strict matrix remains red on
one direct-IR `ExprIf` proof-admission gap; a narrow collector extension and
one exact qualified canonical debug Rust test are in progress. The existing
188 XSH proofs are unchanged. Route acceptance and functional close remain
open.

The root residual has 36 unique passes across 37 executions. The standard
context passed 34 cases and retained one Unix-socket `sun_path` skip; the exact
selector passed through an owned `/t` alias. The namespace case and both positive
`NET_RAW` cases passed, with the latter run under root with `CAP_NET_RAW`.
Initial Git-guard attempts selected zero tests and earn no credit. See the
[root residual evidence](evidence/linux-residual-0deeb9c7/README.md). The
ten-suite performance sequence is not GO before functional close. Lean and
Phase F stay paused. Inference remains isolated until consolidation closes.

Phase D remains isolated at checkpoint `877`, commit
`338b82238a67ee36dd1b025b7a923f9fc5d2da40`, on pair
`a972c8d139ecbb43174271842988eefb2c8638cc`. It removed 348/693 local and
529/2,190 internal annotations. All eleven annotated and eleven reduced root
checks pass. The original 13-case native trial had 12 passes and one lifecycle
failure because `flock` was absent from the historical `ddc` test PATH. The
accepted helper and call correction `3059676c3` added no annotations and kept
both views outside the 48 program sources. The exact lifecycle selector now
passes in each view:
the original has 13 passes, while the reduced view has one pass and its other
contexts remain pending. Targets remain unmet, and all 202 eligible effects
remain retained. Host component `5277` is closed for its six exact native
checks, with guarded source/product identity and cleanup; see the [host native
evidence](evidence/typing-host-native6/README.md). See the [active handoff](HANDOFF.md#active-candidate)
for the full checkpoint details.

The isolated generic module export slice remains accepted at `8475984a9`.
Its main merge waits for consolidation closure. Broader inference, corpus
removal, performance baselines, and formal work remain open.

**Earlier checkpoints:** the progress and verification entries below are
historical and source-specific. They are not the current work queue. The
separate formal inventory does not establish Rust conformance. The pinned
verification image remains
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`;
development and final platform gates use x86_64 Linux musl.

**Owner update, 2026-10-11:** source-line and touched-line budgets below are
advisory estimates. Source growth, temporary ceilings, and final strict shrink
no longer block acceptance; measured performance has priority. Report exact
counts and preserve original baselines. Structural targets, correctness and
final verification remain required.

**Owner platform update, 2026-10-11:** ARM64 and macOS are out of scope.
All development and final platform gates use host-native x86_64 Linux musl
in the pinned Docker environment. Older ARM final-gate requirements below
are historical and superseded; retain their artifacts.

Every editing item has one exclusive source/test owner in its own worktree.
The integrator owns shared registrations, facades, registry, documentation,
development automation, and merges. Source changes are accepted only after
the stated gates; scouting is not verification.

## Resumed scheduler reference qualification

The retained aggregate overlay
`winddown/unfinished/475-native-scheduler-reference-2083821edb768d2485479c7386289b82e43b1c7c.patch`
reconstructs the existing default-worker reference. Its source changes only
scheduler policy and matching help text. Qualify its exact bytes and the three
ancestor scheduler changes from Git instead of requiring unretained scratch
patch files. Preserve the original baseline and failed receipts.

Keep the comparator's existing source-path restrictions, reference queue and
worker policy checks, help and test-list equality, default-worker invocations,
resource allocation, outcome validation, readiness checks, and timing policy.
Record the aggregate overlay in the existing derivation-patch map. Do not add
a second report format, new thresholds, or a selectable baseline policy.
Reject a missing or changed overlay, invalid ancestry, and altered reference
source. This restores input qualification, not a performance result.

## Setup and ready items

Resumed bounded slices have separate worktrees and disjoint shared-file hunks:

| Slice | Exclusive scope | Interim production / touched budget |
|---|---|---|
| W1 stage tags | `full.rs` stage codec declaration and ordinal proofs | net <= 0 / 900 |
| W2 local callable proof | dense slot types, FunctionBuild/FullFunction metadata, local callee verifier | +180 / 1,200, integrated `d325c86c` |
| W3 annotation storage | checked root facts and namespace callers | integrated `b4d5ad37`, -103 production |
| W3 full annotation types | explicit root annotation metadata only; template/pattern contexts excluded | net <= 0 / 600 |
| W4 option parity | named profiles, unconditional migration warnings, publication equality proof | +100 / 1,000 |
| W5 grouping | immediate-child policy wrapper and exhaustive grouping classifiers | +80 / 650, explicit semantic cases offset later |
| W6 expression hooks | next six rule bodies and exact table/dispatcher registration | net <= 0 / 2,200 |
| R3 checked transfer facts | affine locals, shared type predicate, strict transfer-site facts | +600 / 1,800 |
| R3 runtime bridge | strict statement/instruction sidecar and typed transfer continuations | +340 / 1,500, expanded for all transfer sinks; final deletion offsets required |
| R4 stream ownership | stream tokens, scope registry, boundary stamping and worker teardown | +250 / 1,800 |
| W9 namespace isolation | restore qualified callable maps at module boundaries | +40 / 650 |
| W9 borrowed bundle view | resolved reachable modules, source identities, shared declaration consumers | +200 / 2,000 interim estimate; source delta advisory, module-view structure required |
| W9 immutable preparation records | effect probes share resolved seeds within one bundle/domain | +200 lexical / 2,500, interim; cross-root reuse remains separate |
| W7 deep commitment repair | shared scope/block ownership predicates and retained generated counterexamples | +320 lexical / 2,400, expanded interim allowance; existing language stays unchanged |
| W1 stage authority | semantic enum, codec rows, tags and coverage-tool stage listing | net <= 0 / 1,500 |
| R4 captured stream resources | creation registration, exhaustive borrowed frame/capture transfer and balanced cleanup on failed traversal | +450 lexical / 2,500, interim; sweep removal follows lifecycle proof |
| W6 complete rule table | shared checked facts, scope facts and all declaration/dispatch hooks; remove duplicated enabled-state authority | +1,200 lexical / 4,200 interim; final net <= 0 remains required |
| W2 frame strategy and first apply cohort | schema strategy, leaf/Field/Index frames, strict hidden forcing policy | +200 / 1,800, interim; legacy routes must ultimately disappear |
| W2 typed continuations | active Int/Bool frame continuations and typed statement scheduling | +350 lexical / 2,500 interim; reader acceptance waits baseline coverage |
| W1 borrowed pattern views | all 18 rows, aggregate payload bridges and store identity checks | +200 lexical / 1,200 interim; runtime consumers remain separate |
| W9 borrowed workspace entries | immutable entry ranges, view-aware consumers and lint workers | +180 lexical / 2,000 interim estimate; source delta advisory, immutable range and consumer contracts required |
| W9 full module records | session-owned loader/cache, dependency-first local checking, cached Entry records and exact argument-domain selection | +1,900 lexical / 4,200 interim estimate against the refreshed root; source delta advisory, full record/cache contracts required |
| W5 remaining semantic policies | explicit declaration, resource, scope and constant boundaries | +300 lexical / 1,800 interim |
| W5 remaining lint policies | explicit core and sequence boundaries | +250 lexical / 1,400 interim, separate from rule-table scope |
| W9 selected-entry preparation | borrowed entry ranges through compact preparation and evaluator construction | +180 lexical / 1,400 interim, separate from full module records |
| W6 explicit lint baseline facts | absent checked_output never synthesizes baseline checker facts; supplied facts preserve checked rules and fix arbitration | net <= 0 / 650 touched; retain all five failing assertions and explicit CLI checking; rule-local proof transport and full SourceOnly closure remain separate |
| W6 shared checker probes | source-owned candidate checking and fixed-text validation through one transport | +275 lexical / 1,600 interim; preserve named entry policies and complete body-proof inventory |
| W4 typed callee expressions | independently checked field/call-result types and strict verifier derivation | +650 lexical / 2,200 interim; preserve legal callable initializers and reject malformed/cyclic metadata; no copied call-signature proof |
| Structural metric scope correction | exact legacy/current schema validation and blanket-versus-item scope inventory | +240 development lexical / 1,200; integrated +233, Rust production unchanged; preserve original baseline bytes and final zero target |
| W2 complete stage engine | all 46 rows on the active frame machine; remove recursive pipeline materializer and use borrowed stage views | net <= 0 / 5,800 interim touched; stable statement-list references avoid copied row metadata; lifecycle and counter closure gate acceptance |
| W2 next operand apply cohort | nine operand rows and shared operand continuation; preserve observable order | +350 lexical / 1,600 interim; final deletion offsets required |
| Statement/callerless helper batch | Print/Cd/Env cohort including typed cursor plus new compiler-confirmed indexed helper subtraction | combined <= +200 lexical / 2,000 touched; count new helper deletion once, no reuse of reader-batch offsets or final shrink waiver |
| W2 statement frames | Print/Cd/Env on the active machine with per-operand conversion and balanced scope restoration | +200 lexical / 1,800 interim; remove replaced readers and recursive handlers; final W2 shrink required |
| W2 loop/retry expression frames | borrowed body/delay cursors, active loop completion and retry capture policy; delete recursive readers and retry-only wrappers | net <= 0 / 1,200 touched; preserve lexical transfers, cleanup, selection, tracebacks and delay order; no new machine factory |
| W2 process-command construction frames | command_argv and command builder construction use borrowed entry cursors on the active machine; delete duplicate owned entry decoder and recursive handlers | net <= 0 / 1,800 touched; preserve command validation, operand order, trace and host execution contracts |
| W2 binary recursive retirement | reuse the existing FrameControl binary handler and delete its duplicate recursive reader/helper; preserve assertion-specific reporting | net < 0 / 350 touched; isolated binary hunks, no new schema or machine factory |
| W2 branch and host leaves | PatternIf, specialized Str/Tag expression matches, Glob and LastStatus use the active frame machine and borrowed views | net <= 0 / 2,000 touched; remove replaced recursive handlers, preserve branch scopes and current diagnostics |
| Indexed callerless helper retirement | compiler-confirmed unused IrRange constant, instruction field cursor and FullExecution decode/finish wrappers | net < 0 / 200 touched; preserve live FullDecoder methods, struct-payload cursor and all wire rows |
| Indexed dead state | callerless owned ID/tag inspection readers and unconsumed defer-presence metadata chain | net < 0 / 500 touched; preserve all tag rows and live function-kind validation |
| W2 error/failure frames | Error/Fail/Abort borrowed operands and original checked propagation/trace semantics | advisory source/touch reporting; delete replaced recursive/manual readers, preserve cleanup and force-abort priority |
| W2 process execution frames | RunCapture/RunPipeline/SpawnRun/SpawnCommand/Wait and StmtRun on the active machine | advisory source/touch reporting; preserve process lifecycle, conversions, cancellation and resource roots |
| W2 statement match frames | Str/Tag statement matching with borrowed arm selection on the active machine | advisory source/touch reporting; preserve fallback, body flow and scope cleanup |
| W2 scanner frames | ScanLines/ScanBytes plus typed ScanCheck/ScanCondition borrowed codecs | advisory source/touch reporting; retain synchronous optimized loops, input storage, slot/counter and signal cadence |
| W2 cache callback frames | utils.cache miss schedules its callee on the active machine, hit completes immediately | advisory source/touch reporting; preserve key errors, result conversion, hit/miss and resource behavior |
| Frame cold-state performance | Box builder/retry state once per expression; follow up with process-execution state on refreshed Root | advisory source/touch accounting; exact compiler layouts, allocation probes and focused behavior gates |
| W2 deferred cleanup frames | Active-machine cleanup queue and resumable error discard; preserve LIFO, deadlines, traceback precedence and resource ownership | advisory source/touch accounting; finite-depth small-stack regression plus existing cleanup/error gates |
| W1 pipeline borrowed readers | Projection/reduction shape helpers and serial stage lists with identity-preserving shared codec adapter | advisory source/touch accounting; no duplicated wire authority or copied future expression descriptors |
| W1 scalar statement readers | Borrowed Assert, scalar Let/Assign, expression discard and control leaves; exclude complex paths and branch lists | advisory source/touch accounting; canonical type conversion and unchanged continuation behavior |
| W1 borrowed driver rows | one typed authority for driver field order, borrowed export/slot views and runtime reader migration | report source/touch deltas; preserve eager corruption validation, child/slot ownership, signal-hook registration and top-level error precedence |
| W1 stable live wire ordinals | preserve surviving tag numbers after three proven-dead row deletions | net <= 0 / 250 touched; no decoder fallback |
| W1 decoder gap tests | isolated native stage and pattern scenarios plus exact frozen counter proof | zero production / 500 test touched per owner; unreachable arms need writer/caller closure |
| W3 shared storage conversions | canonical forward/inverse mappings with strict caller-specific guards | +150 lexical / 1,200 interim; delete replaced mapping bodies |
| W3 borrowed display | one display operation over Runtime/Lowered views and canonical error data accessors | net <= 0 / 250 touched; preserve published scalar display and executor error policies |
| Borrowed runtime integration batch | binary retirement, callerless indexed metadata, borrowed JSON/display and FS/branch frame readers against the joint frame/stage base | combined net <= 0 / 3,000 Rust touches; reconcile scoped estimates with exact contributions, count each deletion once; original final root shrink and envelopes unchanged |
| Binding recovery regressions | explicit checker recovery identity and With initializer handler | +80 / 800, interim; no valid-binding name fallback |
| Dead formatter code | formatter allowance and compiler-confirmed unused items | net <= 0 / 1,500 |

The W9 expansion follows the private source audit at `63d79c2f`: its complete
record driver exceeds the earlier draft estimate, and Entry caching and direct
Bytes-domain checking remain missing. The audit identifies actual preparation
and effect-projection deletions but no honest 856-line offset. The expanded
interim slice must finish those contracts and remove the identified dead state;
it does not accept the current candidate. The original W9 net source-reduction
target is advisory under the 2026-10-11 owner update; W9's structural and
behavioral end states remain required.

These prerequisite allowances are planning estimates. They do not accept the
implementation or waive removal of replaced bodies where the approved
structural end state requires it. Numerical source totals and final strict
shrink are advisory; structural ratchets remain binding. Shared changes are
merged by the integrator only. W4 owns option/API hunks; W9 owns module driver
hunks; R3 owns transfer emissions; W2 owns dense-slot callable metadata; the
BindingId adapter owns identity resolution. Source-aware arena text lookup is
an exact additional adapter carve-out for imported command words.

| Item | Ownership / deliverable | Prerequisite | State |
|---|---|---|---|
| S0 | Integrator: approvals, baseline commit, ownership and handoff | owner approval | active |
| S1 | Integrator: pinned Linux environment, baseline correctness gates and coverage | S0 | Frozen-source LLVM and API measurements were recorded as satisfying the pre-refactor prerequisite; their detailed results are not retained in the current handoff. The baseline gate retains failures and timeouts; no current-source coverage result or green Rust baseline is claimed |
| S2 | Integrator: XSH structural scanner, baseline JSON, evidence for each metric | S0; verified binaries to run | 13 fixtures green; exact-source baseline measured; CLI/ratchet wiring active |
| S3 | Integrator: measured no-LTO optimized test profile and path-mapped lane gate | S1 | tooling integrated be681661; 51 automation tests green; on 2026-10-14 the owner waived profile-selection measurement for final acceptance; ordinary verification retains opt3/no LTO/64 codegen units; path-mapped gate remains integrated; final release opt3 gate remains pending |
| S4 | Pure partitions of lower, indexed executor, module executor, and lint | frozen-source coverage measurement; baseline gates | four partitions integrated; body proofs and compiler gates green; the pre-refactor coverage prerequisite was recorded as met; current-source runtime acceptance remains pending and no current-source coverage claim is made |
| S5 | Integrator: instruction-table, traversal, and lint-rule designs; campaign skill | revalidation | designs and campaign skill established |
| D1 | `src/diagnostic.rs`, new native diagnostic-rendering coverage | recorded contract below; S1 for execution | integrated 053a546b; 4 native, 18 parser and 7 renderer tests green |
| D2 | `src/sema/types.rs`, imported-enum diagnostic coverage | recorded contract below; S1 for execution | integrated a13ea703; 36 native and one unit green |
| D3 | `tests/xsh/stdlib/unix_process.xsh`: prove buffered output survives both exec forms | S1 for execution | integrated `b6f1af4f`; 21 passed, 2 privilege skips |
| W7A | grammar and parser rule sharing; parser native tests and grammar proofs | S1 for execution | integrated 854e48da; 23 grammar/50 native green; deep proof exposed nine counterexamples, repair active |
| W7B | `crates/xsh-fuzz/src/mutate.rs`, soundness tests: recognize every cleanly parsed mutant | S1; W7A before acceptance | recognizer and corpus-walker regressions active; focused gates green |
| W5A | `src/syntax/arena.rs`: immediate exhaustive child enumerators and sugar views | traversal design; S1 for execution | integrated e45beb30; 8 units and compiler gate green |
| W1 | instruction/row/tag/decoder table, patterns/stages, return analysis, module inventory | S1, S4, instruction-table design | authoritative construction/schema slice active; runtime reader migration waits coverage |
| W2 | bounded nested machines, verifier mark, shared apply functions, frame scheduling, one pipeline | W1; oracle resolution below | bounded machines integrated bbb3cc7a; 66 native and lifecycle unit green; frame/apply work follows W1 |
| W3 | checked type-expression facts, storage mappings, one borrowed value/type view | W2, checked declarations | borrowed predicate slice active; checked type-expression/storage mappings follow facts |
| W4 | boundary context, lexical identities, callee proof, module commands, checked declarations/session | S1, S4; exclusive checker/IR owners | A integrated a2ca731a with 104 native tests; B lexical identities, module commands and ordinary diagnostics active |
| W5 | exhaustive behavior matches and recursive-only enumerator migrations | W5A, S4 | structural grep/refactor and lint walker migrations active |
| W6 | lint declaration table, one descent, shared checked facts/probes, rule repairs | W5, check-session; oracle resolution below | repairs integrated 1d5f9a96; common descent/table slice active |
| W8A | opaque Any plus repository/Laputa migration | checker owner; design corrections below | opacity tests reproduced; candidate checker and semantic migration inventory active |
| W8R | opaque locks, resource table/tokens/scopes/escape facts/streams/affine checks and migrations | W1/W3/checker owners; D12 recount | R1 integrated 7b8b4f01; 24 native plus four host/registry tests green; R2 scopes/ancestry active |
| W8E | socket effect sets and explicit-clause migrations | effect consumers' exclusive ownership | integrated 305ac0f9 plus 710bbe47/bf88ad3b migrations; focused API/native/compiler gates green |
| W9 | isolated module checking/records/reuse; no stage program clone; wide-spread lookup | check-session; lowering ownership | planned |
| W10 | feature-touch recount, reliability, comparative workloads, docs and closing report | functional completion | starting feature probe complete; seven-tool workload parity green; closing evidence pending |

The first ready implementation set is independent by source ownership.
Preparation may happen before the baseline completes; changes cannot be
accepted before the baseline and their own gate pass. Each later slice gets
an exact file set and budget after the pure partitions establish it.

Pure partition scaffolding budgets are explicitly +400 production Rust lines
for lower, indexed execution, and lint, and +700 for module execution. Their
verified deltas are +143, +137, +136, and +603 respectively; moved bodies and
comments are retained. The traversal prerequisite has a +650 production-line
budget before consumer deletion offsets it. Grammar rule sharing has +80.
Boundary-context A has +100 / 1,800 touched; effect sets +150 / 900 touched;
resource foundation preparation +600 / 2,500 touched. Subsequent scoped
exceptions: R2 +250 / 1,800 touched, explicit Any rejection +80 / 1,800,
module command facts +100 / 800, ordinary diagnostics +100 / 900. The restart
handoff's combined module-plan core and execution-hook allowance is +250
production lines; core +95 and hooks +86 fit that scope, while the hooks fit
the earlier facts-only allowance. The resumed source-session and telemetry
adapter allows +450 / 2,200 with prerequisite removal offsets and no final
ratchet waiver. W1
construction/schema allows 4,500 touched with a completed net reduction of
at least 200 lines. These are scoped
exceptions to the default zero-growth budget, not waivers of final ratchets.

## Ordinary defect contracts

- D1: human diagnostic source excerpts and markers are bounded to 160
  rendered characters around the label. Clipping uses an ellipsis and keeps
  source locations and machine spans absolute and unchanged. Color/plain
  rendering share the same window; Unicode and multiline labels are covered.
- D2: imported enum type display uses its declaration name instead of its
  internal canonical filesystem identity. Equality and resolution continue
  using the canonical identity; nested type display follows the same rule.
- D3: both `unix.exec` variants retain exact previously buffered stdout and
  stderr before replacement. Source already flushes; regression execution
  will determine whether the TODO can be closed without production changes.
- Optional field/index diagnostics keep existing codes and explain present
  value narrowing and guarded access, preserving checker recovery.
- `FsRoot.symlink` requires `target:` and `path:` without changing runtime
  parameter slots or source evaluation order. Caller migration adds labels
  without reordering operands.
- `xshi` has no current implementation: its binary runs a disabled stub.
  Adding a sized thread to the stub does not fix a stack defect. Reconcile
  the stale TODO rather than restore the removed interactive implementation.

## Historical revalidation evidence

- Instruction table: 158 instruction variants (6 integer, 12 boolean,
  97 expression, 43 statement), 18 patterns and 46 stages. Manual execution
  reads/finishes: 923 calls across 922 physical lines, plus 43 optional raw reads. Pattern Text encoding
  and existing tag ordinals must remain unchanged.
- Wide arena wildcards: 57, including aliased pattern variants. Semantic
  classifiers stay exhaustive behavior matches; recursive-only walkers use
  immediate child enumerators.
- D12: zero roots and zero locks in the derived-value escape hazard row,
  including Laputa `048592b0473bcee57b5dd0573297115ab78ef633`. The printed
  731-site denominator is inaccurate and is not reused as a baseline.
- Raw physical Rust lines are 205,842 under `src/` and 60,041 under
  `crates/xsht/src`; they include tests. The verified lexical scanner measures 189,274 and 49,968 non-test lines,
  retaining mixed platform/test items and ignoring comments/string contents.
  The corrected same-source baseline measures 47,061 lines under blanket
  dead-code scopes. The original 47,705 report was mis-scoped because it
  counted narrow item allowances as scopes; the earlier 15,800 figure omitted
  containing scopes. Semantic proof rows stay explicitly unmeasured until
  their respective checks run.
- Module support inventories' 24-name difference includes native-test and
  script-backed operations. It is not evidence of 24 runtime defects.
- Module-check reuse is scoped to a resolved import graph and args domain.
  Preserve diagnostic phase order and fresh snapshots after fixes; the
  detailed approved design excludes reuse across fresh fix-round arenas.

## Recorded contract and oracle conflicts

These entries preserve concrete contradictions found against approved end
states. On 2026-10-11 the owner selected the approved contracts as the
authority and authorized updating contradicted expectations with coverage
retained. The table records starting expectations and current dispositions
where known; it is not the current work queue.

| Conflict | Approved end state | Starting expectation / disposition |
|---|---|---|
| Local typed callee | verifier ties every callee to its declared callable type | The starting corruption test accepted a local Proc row relabeled Pure and expected runtime rejection. Merged verifier work now rejects the malformed callable during verification; the corruption case is retained. |
| Unsafe prefer-fail fix | no behavior-changing autofix | Starting tests expected nominal errors to become strings and disappear. XSH expectations preserve nominal failures; Laputa's four Asterinas error families were restored at `ca658ce` with a red-to-green diagnostic-type regression. |
| Run propagation initializer | removable propagation is reported and fixed | `lint_redundant_propagation.rs` tests require silence for the initializer forms named as defects; preserve cases where propagation terminates run grammar |
| Flat-map result callback | one engine, List/Stream-only behavior from evaluator design | `stage-result-contracts.xsh::test_flat_map_preserves_existing_result_collection_boundary` requires implicit Ok(List) flattening and nominal Err propagation; new tests must assert the approved accepted/rejected contract |
| Opaque Any fix fixture | navigation requires validation or json.get | design promises the existing contextual-fix test unchanged although its input and expected output navigate raw Any; migrate fixture without losing contextual require-fix coverage |
| Reveal-type publication | checker diagnostics are profile-independent; reveal notes are separately published; execution rejects `reveal_type` outside `xsht check` | Resolved against SPEC §5.4 by `b1b4e99f` and `c8fc9e26`; existing Rust and native tests cover diagnostic parity, separate note publication, and execution rejection. |

Factually stale examples also need correction without inventing new APIs:
`process.command_argv` has concrete List[Str]/List[Path] overloads, not an Any
parameter; guarded Any indexing already has a rejection code; invalid UTF-8
imports currently use `source.invalid-utf8`, not `parse.module-read`.

## Verification state

The active pinned image is
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
Development and final platform gates use x86_64 Linux musl in
`Dockerfile.test`; ARM64 and macOS are outside campaign scope. No full native
suite has run on current main `be0b79a48403d74b11186166ba739a81dcbed702`.
The last full run used 7b source and failed; its receipt and logs are linked in
[`HANDOFF.md`](HANDOFF.md).

Remaining acceptance includes resolving the canonical GNU rm and uu-sort
failures, completing the full canonical gate, resolving collector `917` binding
holes, applying the W9 helper-path qualification fix before validation and
comparison, rerunning the current inference group and completing its named
matrix, repairing supplemental gate r4, closing ten semantic review rows, and
functional close.
Resource, profile, forced-frame, and scheduler comparisons follow functional
close and must use the existing readiness checks. The initial source-qualified
native baseline and 1,162-file check are historical evidence, not current
acceptance.

Campaign worktrees and scratch stay under ignored `.work/`; only one full
native corpus runs at a time. The owner excludes `dev/compat` from check/lint
while that campaign remains WIP; its XSH scripts and ratchets stay outside this
campaign's automatic gates. Laputa verification remains focused and lightweight.
