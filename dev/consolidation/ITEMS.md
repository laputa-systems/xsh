# Consolidation items and ownership

Approved campaign: `CAMPAIGN.md`. Start commit:
`5e66b6b896a47d0c692739135d51200b007a2a67`. Integration branch:
`consolidation`; compatibility has a separate worktree and does not move this
baseline. All six design drafts were approved on 2026-10-11.

**Resumed after reboot on 2026-10-11, with Luna xhigh subagents.** [`HANDOFF.md`](HANDOFF.md) records the
latest checkpoint and supersedes the older progress states below. Unfinished
lane diffs and interrupted gates are preserved; the owner has resumed work.

Every editing item has one exclusive source/test owner in its own worktree.
The integrator owns shared registrations, facades, registry, documentation,
development automation, and merges. Source changes are accepted only after
the stated gates; scouting is not verification.

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
| W9 borrowed bundle view | resolved reachable modules, source identities, shared declaration consumers | +200 / 2,000, interim; final W9 net reduction required |
| W9 immutable preparation records | effect probes share resolved seeds within one bundle/domain | +200 lexical / 2,500, interim; cross-root reuse remains separate |
| W7 deep commitment repair | shared scope/block ownership predicates and retained generated counterexamples | +320 lexical / 2,400, expanded interim allowance; existing language stays unchanged |
| W1 stage authority | semantic enum, codec rows, tags and coverage-tool stage listing | net <= 0 / 1,500 |
| R4 captured stream resources | creation registration, exhaustive borrowed frame/capture transfer and balanced cleanup on failed traversal | +450 lexical / 2,500, interim; sweep removal follows lifecycle proof |
| W6 complete rule table | shared checked facts, scope facts and all declaration/dispatch hooks; remove duplicated enabled-state authority | +1,200 lexical / 4,200 interim; final net <= 0 remains required |
| W2 frame strategy and first apply cohort | schema strategy, leaf/Field/Index frames, strict hidden forcing policy | +200 / 1,800, interim; legacy routes must ultimately disappear |
| W2 typed continuations | active Int/Bool frame continuations and typed statement scheduling | +350 lexical / 2,500 interim; reader acceptance waits baseline coverage |
| W1 borrowed pattern views | all 18 rows, aggregate payload bridges and store identity checks | +200 lexical / 1,200 interim; runtime consumers remain separate |
| W9 borrowed workspace entries | immutable entry ranges, view-aware consumers and lint workers | +180 lexical / 2,000 interim; final W9 reduction required |
| W9 full module records | session-owned loader/cache, dependency-first local checking, cached Entry records and exact argument-domain selection | +1,900 lexical / 4,200 interim against the refreshed root; exclude identical prerequisites and separate W5 classifiers; final W9 reduction required |
| W5 remaining semantic policies | explicit declaration, resource, scope and constant boundaries | +300 lexical / 1,800 interim |
| W5 remaining lint policies | explicit core and sequence boundaries | +250 lexical / 1,400 interim, separate from rule-table scope |
| W9 selected-entry preparation | borrowed entry ranges through compact preparation and evaluator construction | +180 lexical / 1,400 interim, separate from full module records |
| W6 shared checker probes | source-owned candidate checking and fixed-text validation through one transport | +275 lexical / 1,600 interim; preserve named entry policies and complete body-proof inventory |
| W4 typed callee expressions | independently checked field/call-result types and strict verifier derivation | +650 lexical / 2,200 interim; preserve legal callable initializers and reject malformed/cyclic metadata; no copied call-signature proof |
| Structural metric scope correction | exact legacy/current schema validation and blanket-versus-item scope inventory | +240 development lexical / 1,200; integrated +233, Rust production unchanged; preserve original baseline bytes and final zero target |
| W2 complete stage engine | all 46 rows on the active frame machine; remove recursive pipeline materializer and use borrowed stage views | net <= 0 / 5,800 interim touched; stable statement-list references avoid copied row metadata; lifecycle and counter closure gate acceptance |
| W2 next operand apply cohort | nine operand rows and shared operand continuation; preserve observable order | +350 lexical / 1,600 interim; final deletion offsets required |
| W2 statement frames | Print/Cd/Env on the active machine with per-operand conversion and balanced scope restoration | +200 lexical / 1,800 interim; remove replaced readers and recursive handlers; final W2 shrink required |
| W2 binary recursive retirement | reuse the existing FrameControl binary handler and delete its duplicate recursive reader/helper; preserve assertion-specific reporting | net < 0 / 350 touched; isolated binary hunks, no new schema or machine factory |
| Indexed dead state | callerless owned ID/tag inspection readers and unconsumed defer-presence metadata chain | net < 0 / 500 touched; preserve all tag rows and live function-kind validation |
| W1 stable live wire ordinals | preserve surviving tag numbers after three proven-dead row deletions | net <= 0 / 250 touched; no decoder fallback |
| W1 decoder gap tests | isolated native stage and pattern scenarios plus exact frozen counter proof | zero production / 500 test touched per owner; unreachable arms need writer/caller closure |
| W3 shared storage conversions | canonical forward/inverse mappings with strict caller-specific guards | +150 lexical / 1,200 interim; delete replaced mapping bodies |
| W3 borrowed display | one display operation over Runtime/Lowered views and canonical error data accessors | net <= 0 / 250 touched; preserve published scalar display and executor error policies |
| Binding recovery regressions | explicit checker recovery identity and With initializer handler | +80 / 800, interim; no valid-binding name fallback |
| Dead formatter code | formatter allowance and compiler-confirmed unused items | net <= 0 / 1,500 |

The W9 expansion follows the private source audit at `63d79c2f`: its complete
record driver exceeds the earlier draft estimate, and Entry caching and direct
Bytes-domain checking remain missing. The audit identifies actual preparation
and effect-projection deletions but no honest 856-line offset. The expanded
interim slice must finish those contracts and remove the identified dead state;
it does not accept the current candidate or waive final W9 reduction.

These prerequisite allowances require their eventual deletion offsets and do
not relax the final net reduction or structural ratchets. Shared changes are
merged by the integrator only. W4 owns option/API hunks; W9 owns module driver
hunks; R3 owns transfer emissions; W2 owns dense-slot callable metadata; the
BindingId adapter owns identity resolution. Source-aware arena text lookup is
an exact additional adapter carve-out for imported command words.

| Item | Ownership / deliverable | Prerequisite | State |
|---|---|---|---|
| S0 | Integrator: approvals, baseline commit, ownership and handoff | owner approval | active |
| S1 | Integrator: pinned Linux environment, baseline correctness gates and coverage | S0 | native baseline green; Rust and coverage pending |
| S2 | Integrator: XSH structural scanner, baseline JSON, evidence for each metric | S0; verified binaries to run | 13 fixtures green; exact-source baseline measured; CLI/ratchet wiring active |
| S3 | Integrator: measured no-LTO optimized test profile and path-mapped lane gate | S1 | tooling integrated be681661; 51 automation tests green; profile measurements pending |
| S4 | Pure partitions of lower, indexed executor, module executor, and lint | coverage; baseline gates | four partitions integrated; body proofs and compiler gates green; combined coverage acceptance pending |
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

## Revalidation evidence

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
  It measures 47,705 lines under inherited blanket dead-code allowances; the
  earlier 15,800 figure omitted containing scopes. Semantic proof rows stay
  explicitly unmeasured until their respective checks run.
- Module support inventories' 24-name difference includes native-test and
  script-backed operations. It is not evidence of 24 runtime defects.
- Module-check reuse is scoped to a resolved import graph and args domain.
  Preserve diagnostic phase order and fresh snapshots after fixes; the
  detailed approved design excludes reuse across fresh fix-round arenas.

## Contract and oracle conflicts

These are concrete contradictions between approved end states and existing
expectations. On 2026-10-11 the owner selected the approved contracts as the
authority and authorized updating contradicted expectations with coverage
retained. The integrator records each migration in the handoff log.

| Conflict | Approved end state | Current expectation / correction needed |
|---|---|---|
| Local typed callee | verifier ties every callee to its declared callable type | `indexed/full.rs` typed-call corruption test explicitly accepts a local Proc row relabeled Pure, then expects runtime rejection; it must assert verifier rejection instead |
| Unsafe prefer-fail fix | no behavior-changing autofix | native `fail.xsh` and `lint_prefer_fail.rs` tests require nominal errors to become strings and the error family to disappear; replace unsafe-fix expectations with preserved nominal failures |
| Run propagation initializer | removable propagation is reported and fixed | `lint_redundant_propagation.rs` tests require silence for the initializer forms named as defects; preserve cases where propagation terminates run grammar |
| Flat-map result callback | one engine, List/Stream-only behavior from evaluator design | `stage-result-contracts.xsh::test_flat_map_preserves_existing_result_collection_boundary` requires implicit Ok(List) flattening and nominal Err propagation; new tests must assert the approved accepted/rejected contract |
| Opaque Any fix fixture | navigation requires validation or json.get | design promises the existing contextual-fix test unchanged although its input and expected output navigate raw Any; migrate fixture without losing contextual require-fix coverage |
| Reveal-type publication | publication options do not change user diagnostics | existing native test requires xsh rejection of reveal_type; resolve publication versus entry legality explicitly |

Factually stale examples also need correction without inventing new APIs:
`process.command_argv` has concrete List[Str]/List[Path] overloads, not an Any
parameter; guarded Any indexing already has a rejection code; invalid UTF-8
imports currently use `source.invalid-utf8`, not `parse.module-read`.

## Verification state

The pinned native image is `xsh-test:latest`, image ID
`sha256:99e195582ba55dc427b1551022fbe56f8f344ed7838e971ecba86ce43a7733e9`.
Baseline release products were rebuilt from the recorded source inside that
image with the Dockerfile's static musl flags and compiler-only jemalloc.
The full native corpus passed: 5,996 passed, 0 failed, 118 capability/fixture
skips. Five existing redundant-parentheses findings in `core/lib/compress.xsh`
were repaired manually in `44c7567d`; its focused tests passed 52/0 with 3
skips. XSH check then passed 1,162 files, and the initial lightweight Laputa
check passed 305 files. Rust full-gate and coverage evidence remain pending.

Logs, launchers, frozen baseline products, and draft evidence live in the
integration worktree's disk-backed `.work/consolidation/`. Lane worktrees
live in `/home/josh/d/laputa-systems/xsh-consolidation-lanes/`. No worktree or
build cache goes in `/tmp`. Compilation is serialized independently of agent
concurrency, and only one full native corpus runs at a time.

The owner selected native x86_64 musl development gates and aarch64 final
verification. The unchanged Dockerfile also produced
`xsh-test:consolidation-aarch64`; a bounded ARM help probe passed. Final ARM
build/test evidence is pending. Only this campaign's binfmt registration is
removed at close.

The owner subsequently excluded `dev/compat` from check and lint while its
campaign remains WIP. Project discovery excludes its XSH scripts; the
consolidation batch also omits compatibility ratchets. Preserve tests and
tools for that campaign's explicit runs. Laputa verification stays focused
and lightweight; full sibling formatting and lint-invariance corpus tests
are filtered from automatic consolidation gates.
