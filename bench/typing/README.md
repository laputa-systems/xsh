# Type-and-effect inference execution

**Incomplete — Gates A, B and C passed; D is in progress.** Full inference acceptance remains pending.

The isolated candidate is on `typing-inference-campaign-v2`; the immutable baseline
is revision `d6f09bc54305515b4c34d3b872d7f9f13b874061` in the adjacent
`xsh-typing-baseline-v2` worktree. Their Cargo build directories are separate.
The preceding ergonomics corrections are committed in the shared checkout.
The original cohort, eligibility ledger, and denominators remain frozen at
`877a114d6dc403db32c50f035b38aab7f193147a`. Historical results and policies
are preserved under `history/877a114d/`; their measurements retain their original
revision and product provenance and do not count as corrected-handoff acceptance. Local checkpoint commits disable hooks;
merging, pushing, dependency additions, formatter/autofix commands, and Linux
execution are outside this run. The explicitly requested repository read-only
lint integration gate remains required.

## Frozen scope and historical evidence

The following observations describe the original handoff unless an artifact
explicitly records the corrected revision. Fresh verification is in progress;
`results.json` records current gate status without carrying historical passes
forward as new acceptance.

- The exact baseline `xsh`, `xsht`, frontend stats and runtime stats products
  built successfully with the pinned toolchain and default supported features.
  `baseline-build.json` records commands, source/binary hashes and settings.
- A frontend-only scan covers 356 real source roots. The complete system-report
  and dev module graphs are the largest type-heavy workloads. The frozen cohort
  retains these inconvenient cases alongside startup, streams, stdlib, import
  graphs, native modules, and valid/invalid annotated programs.
- The source cohort is frozen at 23 entry closures and 38 unique files. All 22
  valid roots check and prepare; the remaining root retains its intentional
  unchecked-JSON rejection. Original and stabilized safe entry observations
  match. Stabilization adds 40 protected `Result[Unit]` return annotations.
- `annotations.json` reconciles 9,114 original sites: 778 eligible locals,
  2,807 eligible internal sites and 5,529 protected sites. Internal sites contain
  1,368 parameters, 746 returns, one producer item and 692 effect clauses. Shared
  import sites count once. Added stabilizers stay outside these denominators.
- `operations.json` freezes 730 finite registry/operation entries, with default
  and feature-disabled inventories, authoritative source hashes, relationships,
  effects, evidence responsibilities and coverage owners.
- `semantic-cases.json` records 38 independent source observations. The 24
  preserved cases pass; eight intended new acceptances and six intended
  rejections remain candidate-pending. Canonical typing specifications now state
  the fixed value/assertion, Result, default and producer decisions.
- `owner-audit.json` identifies actual existing consolidation and the semantic
  mechanisms to retire. The current memory estimator double-counts some type
  storage and does not track every retained allocation; it is unsuitable for
  the mandatory retained-memory comparison. `baseline-retention.json` instead
  records 138 fresh profiling processes across all 23 roots. All 92 finalized
  fact-owner drops are exact, opposite drop orders agree, and teardown accounting
  reconciles. The whole finalized-facts union includes duplicated consumer views
  and non-type fields; parsed source and symbol owners remain separate. Its
  largest retained union is 703,261,390 bytes in the native system-report module.
  Opaque prepared typing/pool attribution remains unavailable and is not a pass.
- Baseline semantic Rust checks pass 173 tests; authorized syntax checks pass
  155 tests. A fresh baseline reproduces the interactive completion ordering
  golden failure. The process-group cleanup case passes. Commands, exact hashes,
  counts and retained logs are in `baseline-verification.json`.
- Twenty-five host measurement tests pass. Regular scaling fixtures freeze seven
  families at four sizes, with annotated/inferred variants and eight adversarial
  cases. Generator and supported small annotated grammar checks pass; actual
  solver work/scaling acceptance remains pending implementation.

## Gates

| Gate | Status |
|---|---|
| A — semantic freeze and baseline | Passed at corrected handoff; exhaustive timing waived by user |
| B — shared inference and early indexed execution | Passed; exact commands and source identities in `gate-b-verification.json` |
| C — declaration inference and solved consumers | Passed; coherent source/product verification in `gate-c-verification.json` |
| D — full indexed integration and module reuse | In progress; exhaustive execution obligations remain |
| E — joint annotation removal and adversarial witnesses | Not run |
| F — paired time, memory and scaling acceptance | Not run |
| G — consolidation and maintained-source migration | Not run |

The original pilot and noise verification remain historical evidence. The user
removed quiet product timing and exhaustive benchmarking on 2026-09-30.
`benchmark-amendment.json` records the change. Future performance checks use five
fresh-process observations and one warmup, with medians/ranges and no claimed p95.
Concurrent builds do not prevent them. Annotation denominators, semantics, runtime
evidence, deterministic complexity, resource limits, and the requested strict
60-second repository lint gate remain required.

The corrected handoff built in release and debug profiles. All 62 product
observation workloads passed. All 23 preparation statuses, 138 retained-memory
processes, 46 original/stabilized frontend checks, 14 runtime runs, 14 native
observations, and 38 semantic baseline observations reconciled. Twenty-four
preserved native tests passed. The mandatory inference programs remain pending;
missing required-parameter grammar cannot satisfy a negative type test.

Fresh Rust verification passed 174 semantic, 155 authorized syntax, 74 indexed
verifier/runtime, and one process-group test. The independently reproduced
interactive completion directory-order golden failure remains documented.
Those counts describe the baseline. The candidate now passes B: 41 core/reference
tests, 17 solved-fact tests, 96 indexed tests, 174 semantic compatibility tests,
eight frontend-drop lifecycle tests, four prepared-frame tests, ten execution
tests and ten frozen native witnesses. The source-to-runtime slice covers shared
identity, row projection, Add and forwarding bodies, with independently verified
evidence and fixed declaration return plans. `gate-b-verification.json` records
the exact source hashes, commands and raw logs. No annotation reduction,
performance improvement or campaign completion is claimed.

C passed on one final source snapshot. The final debug products execute all ten
ordinary early generic witnesses, including row projection and forwarded Add
requirements. The full library passes 956 tests, semantic integration 181,
authorized syntax integration 162, xsht 99, interactive application 19, API 65,
registry 10, and interactive embedded preparation one. The no-default library
build also passes. The existing ignored cold-start profiling test and baseline
package formatting exclusion remain explicit; no Linux check ran.
`gate-c-verification.json` retains commands, logs, product identities, exact source
hashes and the final source audit. Earlier development failures remain archived
and are superseded by the coherent verification rather than deleted.

The source audit reconciles all frozen 730 authority rows: 480 registry callables,
73 schemas, 17 error variants, 35 stages and 125 language entries. Source-owned
schemes preserve callable protocols, independent error joins, nullable payload
relationships, effect/default timing and original producer paths. Both concrete
and residual source obligations retain their original identity after frontend
disposal; query reads do not add inference work. The separate private return
walker, cloned default repair and whole-program effect scan have been removed.

D has started. `measurements/gate-c/indexed-execution-remaining.json` partitions
the frozen inventory into exact remaining execution obligations; its counts are
inventory rows, not failing-program counts. Full generalized operations, callable
and effect evidence, schemas, stages, resources, module reuse and removal of
semantic lowerer reconstruction remain required. Unsupported evidence currently
fails preparation explicitly. Annotation reduction and performance acceptance
remain pending; C does not establish campaign completion.

## Implementation ownership and remaining adapters

The following contracts guided B and remain the boundaries for consolidation:

- Keep the existing `Checker` traversal. Its bindings, signatures, expectations,
  expression facts and refinements will use IDs in one shared graph. Annotated
  and omitted declarations use the same constraint and operation handlers;
  omission introduces variables. Interning already-decided tree types afterward
  would leave the old semantic reconstruction authoritative.
- Freeze declaration completion and statement-use decisions before
  generalization. `FullFunctionView::decode_header` currently derives Result
  wrapping from a concrete return tree; it must consume the fixed decision so a
  generic Result payload keeps its nesting.
- Publish solved calls with one argument/default/spread binding plan and
  evidence plan. Signature precision and optional unique declaration identity
  remain separate. Compact consumers project these facts; body probes and
  `lower.rs` signature/binding reconstruction must stop deciding semantics.
- Prepared physical record layouts stay distinct from sorted semantic shapes.
  Both executors sort completed record vectors by `Name`; Stats, StatsBlob and
  Map backing require their own certified access plans. A generic projection
  consumes a scoped field-slot witness, with no per-item field-name search.
- Generic operation bodies receive canonical prepared evidence, including
  forwarding maps. Concrete paths keep their direct operations. Both indexed
  routes must execute the early identity/projection/operation/forwarding slice
  after frontend disposal, sharing each generic body.
- Quantifier scopes, layouts, evidence owners, forwarding maps and pool rewind
  are verifier contracts. Ground display/schema-input views can be bounded
  adapters; they cannot resolve assignability or erase open/quantified facts.
- `RecordConstructors::begin_constructor_inference` and
  `finish_constructor_inference` still use the bounded legacy `TypeConstraints`
  constructor-group adapter. Original field expressions are checked once; null,
  empty, and nested groups publish only after their independent supplied fields
  contribute, then retain exact core assignment endpoints. Gate D must consume
  the canonical application/default plans for execution, and Gate G must remove
  this group adapter in favor of graph-owned occurrence variables while keeping
  null/empty anchoring, universal defaults, phantom arguments, and lexical owners.

The 38 semantic cases freeze statement/Result decisions, not all 28 required
witness families. B's source-to-runtime and reference/verifier slice has passed.
C's computed callable flows, value restrictions, inferred exports and source
operation-family contracts are verified. D needs complete indexed boundary,
resource/refinement and counted module-reuse coverage. Existing annotated native
tests are compatibility owners; inferred counterparts still need execution.
