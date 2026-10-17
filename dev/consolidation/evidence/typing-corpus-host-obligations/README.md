# Joint corpus with finite host obligations

This checkpoint starts at isolated source `f86af382d42e1bd480b51575f992c2fcf443eda1`.
Compiler, crates, embedded standard library and build inputs match qualified
source `a972c8d139ecbb43174271842988eefb2c8638cc`. The
[host-obligation proof](../typing-host-obligations/README.md) owns the compiler
repair and its focused qualification. This migration makes no compiler edit or
build. The earlier 613 and 615 checker checkpoints remain unchanged.

## Source and products

The cohort retains the eleven roots, 48 loaded files and 9,194 AST sites frozen
at `a8d5bd910`. Program sources remain XSH
`ddc17d3d53c819c343f9ecaa738ac67d59e9033a` and Laputa
`ca658ce1da9d89c55209c25268dfdf687ff4fdc6`. Schema definitions, exported interfaces,
validation targets and entry decoding remain protected.

Both source views use the same read-only verification pair:

| Product | SHA-256 |
|---|---|
| xsh | `ee306b82df028f4b2d0bc83699bf41de27c5c7ac56862de10135461e6bbca154` |
| xsht | `c7c12a7af5e26edee94ae12bba7196a9a3bffd6bbe90129d29a5a923b73e831c` |

The pinned image is
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
Commands use four CPUs, 8 GiB memory, no extra swap, no network and UID 1000.
Scratch and temporary files stay under the isolated worktree's `.work`.
`original-products.*` and `joint-877-products.*` retain the before and after hash
guards. `views.ready.tsv` records content, modes and symlink identities for the
prepared immutable source views. No timing or performance claim is made.

## Joint migration

The retry removes all 2,125 sites compatible with current Proc syntax and default
return semantics together. It preserves 549 required Proc parameter types and
209 non-Unit Proc return annotations as eligible restorations. The broad trial
rejects. Its removal count has no success percentage.

Restoration first supplies private parameters for failing bodies, then explicit
return contexts and directly implicated locals. It avoids restoring every
parameter, return and local in a failing function together. The source excerpts
listed in `failure-excerpts.tsv` are actual diagnostic blocks from those joint
trials. They show real receiver ambiguity, the named generic-body restriction,
empty collection inference, missing enum context, recursive return requirements
and loss of a typed map. They are excerpts, not complete diagnostic logs or
independently executed standalone witnesses. Frozen AST edit spans reconstruct
the candidate stages without changing algorithms or inserting types.

The final candidate passes ordinary checking and indexed lowering for all ten
XSH roots and the Laputa root. The annotated originals also pass all eleven.
`annotation-validation.tsv` verifies the exact 48 source transformations and
protects every retained site, schema and exported-interface span.

| Eligible class | Frozen | Removed | Restored | Removal rate |
|---|---:|---:|---:|---:|
| Local | 693 | 348 | 345 | 50.22% |
| Internal parameter, return and effect | 2,190 | 529 | 1,661 | 24.16% |
| All eligible | 2,883 | 877 | 2,006 | No aggregate target |

The internal removals comprise 165 parameter and 364 return annotations. No
private effect clause contributes a removal. The 4,364 schema sites, 1,920
exported-interface sites, 23 validation or entry-decoding sites and four entry
effect bounds remain outside these eligible denominators and unchanged.
Together the categories still total 9,194 sites.

The 95% local and 85% internal targets remain unmet. All restored sites remain
eligible. Restoration is still conservative in some declaration groups. These
counts do not establish individual necessity or maximal removability.

## Private effect proof limit

The existing read-only `lint.prefer-inferred-private-effects` rule examines all
22 source files containing the 202 eligible private effect clauses. It reports
142 clauses whose declared effects equal the effects inferred when that clause
alone is removed. The XSH command exits 1 for those 142 warnings. The Laputa
command exits 0 with no findings. The raw outputs and matching AST site list are
retained here. No fix or formatter runs.

Those are individual proofs. `EffectGraph::redundant_clause` in
`src/sema/check/infer_effects.rs` substitutes one declaration's inferred summary
for its clause. Calls to other declared clauses retain their contracts. The
result alone does not establish equal contracts after simultaneous deletion
through mutually supporting declared calls. This checkpoint does not assert
that such a cycle occurs in this cohort.

All 202 private effect clauses are therefore retained, including the 142 with
individual proofs. They stay eligible and count as not removed. The final source
keeps the original explicit effect contracts. No new effect API, analysis,
budget or speculative language feature is added to improve the score.

## Remaining behavior acceptance

The strict 26-execution runner now binds the qualified pair above and the actual
877-removal `reduced-final` view. Each of thirteen existing behavior witnesses
must select and pass once in each view with zero failures or skips. Its source
and product guards, shared native lock and exact-container cleanup remain in
force. Native execution still awaits the assigned slot. This checkpoint claims
checker and lowering acceptance, not behavior parity or completion of the
annotation-removal workstream. The named generic-body seam, broader successor
acceptance and campaign targets remain open. Main remains untouched.

## First native attempt and fixture correction

On clean checkpoint `338b82238`, the original view selects thirteen tests:
twelve pass and the lifecycle cleanup test fails. The reduced view does not run.
`native-attempt-1` preserves every actual output and guard. No 26-pass claim is
made from that attempt.

The lifecycle witness restricts PATH to its fake Git, Cargo and chown tools.
Its production path calls `test_workflows.run_commands`, `cargo_steps.run_build`,
`cargo_steps.execute`, `stage.execute_with_flock` and `require_tool("flock")`.
The missing flock stops before fake Cargo can exit 23. Cleanup chown still runs.
This original-view fixture failure is independent of annotation removal.

The accepted fixture correction from `fff7fed23` supplies a helper that invokes
real `/usr/bin/flock`, then calls that helper once in the affected witness.
`apply_witness_fixture.py` reproduces only those two insertions in both views.
The Cargo-failure and chown assertions remain unchanged. All 48 program files,
source pins, schemas, exported interfaces and 877 removals remain unchanged.
`fixture-validation.tsv` repeats their protected-span check. The archived test
witness is explicitly ddc source plus this accepted fixture correction.
`views.fixture-ready.tsv` records the resulting whole-view digests.

The first attempt's source and product guards pass. Docker reports the owned
container absent with exit 1 and exact lowercase `error: no such object: CID`.
The runner's uppercase-only absence predicate conservatively rejects that
response. Its corrected predicate accepts exactly the two observed spellings,
with exit 1, one stderr line and the exact owned ID. Other inspect failures still
fail cleanup. The original attempt remains red and its raw receipts unchanged.
