# Type-and-effect inference execution

**Incomplete — Gate A is paused for the reopened ergonomics prerequisite.** No inference changes are authoritative.

The isolated candidate is on `typing-inference-campaign`; the immutable baseline
is revision `877a114d6dc403db32c50f035b38aab7f193147a` in the adjacent
`xsh-typing-baseline` worktree. Their Cargo build directories are separate.
The shared `xsh` checkout is unchanged. Local checkpoint commits disable hooks;
merging, pushing, dependency additions, formatter/linter commands, and Linux
execution are outside this run.

## Current evidence

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
| A — semantic freeze and baseline | Frozen scope and observations ready; execution-boundary measurement and final baseline samples pending |
| B — shared inference and early indexed execution | Not run |
| C — declaration inference and solved consumers | Not run |
| D — full indexed integration and module reuse | Not run |
| E — joint annotation removal and adversarial witnesses | Not run |
| F — paired time, memory and scaling acceptance | Not run |
| G — consolidation and maintained-source migration | Not run |

The pilot and full noise verification completed successfully. Final baseline
collection was interrupted after the user found legacy membership calls and
test declarations in the sibling package corpus. Its raw observations remain
intact; `measurements/baseline-final-interruption.json` records the interruption.
Next: settle the reopened ergonomics handoff, establish an immutable baseline
for that revision without overwriting historical evidence, finish its final
sample sets, and commit the reproducible baseline before inference changes. No annotation reduction, new inference,
generic execution, measured performance improvement or campaign completion is
claimed yet.

## Next implementation ownership

These are reviewed interfaces for B, not implemented facts:

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

The 38 semantic cases freeze statement/Result decisions, not all 28 required
witness families. B still needs row/sealed-operation source-to-runtime and
reference/verifier fixtures. C needs computed callable flows, value restriction,
inferred exports and all operation families. D needs complete indexed boundary,
resource/refinement and counted module-reuse coverage. Existing annotated native
tests are compatibility owners; inferred counterparts still need execution.
