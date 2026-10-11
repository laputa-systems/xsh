# Consolidation items and ownership

Approved campaign: `CAMPAIGN.md`. Start commit:
`5e66b6b896a47d0c692739135d51200b007a2a67`. Integration branch:
`consolidation`; compatibility has a separate worktree and does not move this
baseline. All six design drafts were approved on 2026-10-11.

Every editing item has one exclusive source/test owner in its own worktree.
The integrator owns shared registrations, facades, registry, documentation,
development automation, and merges. Source changes are accepted only after
the stated gates; scouting is not verification.

## Setup and ready items

| Item | Ownership / deliverable | Prerequisite | State |
|---|---|---|---|
| S0 | Integrator: approvals, baseline commit, ownership and handoff | owner approval | active |
| S1 | Integrator: pinned Linux environment, baseline correctness gates and coverage | S0 | native baseline green; Rust and coverage pending |
| S2 | Integrator: XSH structural scanner, baseline JSON, evidence for each metric | S0; verified binaries to run | 13 fixture tests green; exact-source scan running |
| S3 | Integrator: measured no-LTO optimized test profile and path-mapped lane gate | S1 | draft reviewed; corrections and candidate measurements pending |
| S4 | Pure partitions of lower, indexed executor, module executor, and lint | coverage; baseline gates | body-preservation proofs green; compiler gates queued; acceptance pending coverage |
| S5 | Integrator: instruction-table, traversal, and lint-rule designs; campaign skill | revalidation | active |
| D1 | `src/diagnostic.rs`, new native diagnostic-rendering coverage | recorded contract below; S1 for execution | baseline failure reproduced; focused native green; unit gate pending |
| D2 | `src/sema/types.rs`, imported-enum diagnostic coverage | recorded contract below; S1 for execution | baseline failure reproduced; fixed gates active |
| D3 | `tests/xsh/stdlib/unix_process.xsh`: prove buffered output survives both exec forms | S1 for execution | integrated `b6f1af4f`; 21 passed, 2 privilege skips |
| W7A | grammar and parser rule sharing; parser native tests and grammar proofs | S1 for execution | baseline probes recorded; context-specific lookahead repair active |
| W7B | `crates/xsh-fuzz/src/mutate.rs`, soundness tests: recognize every cleanly parsed mutant | S1; W7A before acceptance | ready |
| W5A | `src/syntax/arena.rs`: immediate exhaustive child enumerators and sugar views | traversal design; S1 for execution | prepared; compiler/unit gate active |
| W1 | instruction/row/tag/decoder table, patterns/stages, return analysis, module inventory | S1, S4, instruction-table design | planned |
| W2 | bounded nested machines, verifier mark, shared apply functions, frame scheduling, one pipeline | W1; oracle resolution below | planned |
| W3 | checked type-expression facts, storage mappings, one borrowed value/type view | W2, checked declarations | planned |
| W4 | boundary context, lexical identities, callee proof, module commands, checked declarations/session | S1, S4; exclusive checker/IR owners | boundary context regressions reproduced; A compiler gate queued; B/E wait for traversal/A |
| W5 | exhaustive behavior matches and recursive-only enumerator migrations | W5A, S4 | planned |
| W6 | lint declaration table, one descent, shared checked facts/probes, rule repairs | W5, check-session; oracle resolution below | planned |
| W8A | opaque Any plus repository/Laputa migration | checker owner; design corrections below | planned |
| W8R | opaque locks, resource table/tokens/scopes/escape facts/streams/affine checks and migrations | W1/W3/checker owners; D12 recount | regression and shared-type drafts active; runtime ownership waits for partition |
| W8E | socket effect sets and explicit-clause migrations | effect consumers' exclusive ownership | four intended baseline failures; static effect-set implementation active |
| W9 | isolated module checking/records/reuse; no stage program clone; wide-spread lookup | check-session; lowering ownership | planned |
| W10 | feature-touch recount, reliability, comparative workloads, docs and closing report | functional completion | planned |

The first ready implementation set is independent by source ownership.
Preparation may happen before the baseline completes; changes cannot be
accepted before the baseline and their own gate pass. Each later slice gets
an exact file set and budget after the pure partitions establish it.

Pure partition scaffolding budgets are explicitly +400 production Rust lines
for lower, indexed execution, and lint, and +700 for module execution. Their
prepared deltas are +142, +137, +136, and +609 respectively; moved bodies and
comments are retained. The traversal prerequisite has a +650 production-line
budget before consumer deletion offsets it. Grammar rule sharing has +80.
Boundary-context A has +100 / 1,800 touched; effect sets +150 / 900 touched;
resource foundation preparation +600 / 2,500 touched. These are scoped
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
  reads/finishes: 922 calls plus 43 optional raw reads. Pattern Text encoding
  and existing tag ordinals must remain unchanged.
- Wide arena wildcards: 57, including aliased pattern variants. Semantic
  classifiers stay exhaustive behavior matches; recursive-only walkers use
  immediate child enumerators.
- D12: zero roots and zero locks in the derived-value escape hazard row,
  including Laputa `048592b0473bcee57b5dd0573297115ab78ef633`. The printed
  731-site denominator is inaccurate and is not reused as a baseline.
- Raw physical Rust lines are 205,842 under `src/` and 60,041 under
  `crates/xsht/src`; they include tests. Non-test ratchets require a lexical
  counter that excludes test-only items without excluding production
  `cfg(any(..., test))` items or matching comments and strings.
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
