# TODO

Prefix: XSH
Next: 35

### XSH-1 · P2 · Complete private and resource baseline acceptance

- **Area:** Performance
- **Where:** `dev/util-linux/resources.py:1632`, `dev/util-linux/acceptance.py:926`
- **Problem:** Final private/resource qualification remains unrun on the current frozen source. The scheduler comparison failed, so composed baseline close cannot proceed.
- **Recommendation:** After scheduler qualification, run the private and resource gates on the same source. Complete baseline close with its one-use independent review and readiness checks.
- **Fix with:** benchmark-checklist
- **Effort:** S

### XSH-3 · P3 · Finish the deferred consolidation acceptance matrix

- **Area:** Tests
- **Where:** `docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md`, `dev/performance/check.py`
- **Problem:** Canonical, supplemental, profile, observer, forced and comparative checks passed on R13. Scheduler pairs failed. Focused repairs need qualification, and UDP failures and full-corpus timeouts remain unexplained. Private/resource close remains unrun.
- **Recommendation:** Complete G1's remaining queue on one qualified source, then publish the accepted baseline and retire obsolete infrastructure under XSH-19.
- **Blocked:** XSH-29 retains an unresolved full-corpus timeout mismatch. The replacement image is available and its qualification launcher is repaired; historical passes do not qualify it. XSH-25's stream-effect decision and focused implementation are complete, but combined qualification remains pending.
- **Fix with:** orchestrate
- **Effort:** M

### XSH-4 · P3 · Check use after consumption for runtime handles

- **Area:** Types
- **Where:** `src/sema`, `dev/consolidation/designs/resources.md:1`
- **Problem:** The checker accepts visible reuse after consuming operations such as `wait child`, `NetJob.wait`, and `FsRoot.close`. Programs discover the invalid state only at runtime.
- **Recommendation:** Implement the straight-line ownership rules in the linked draft before extending them across control flow.
- **Fix with:** make-invalid-states-unrepresentable
- **Effort:** L
- **Design:** [XSH-4 affine runtime handles](docs/rfc/XSH-4-affine-runtime-handles.md)

### XSH-7 · P3 · Define record and JSON schema exactness

- **Area:** Types
- **Where:** `docs/SPEC.md:1990`, `src/sema`
- **Problem:** Width-compatible records cannot express when dynamic validation must reject, strip, or preserve unknown fields. Adding exact record syntax would bind static typing to one boundary policy.
- **Recommendation:** Choose the validation-boundary policy in the linked draft, then specify its interaction with defaults, encoding, and schema evolution.
- **Fix with:** api-surface-and-module-boundaries
- **Effort:** L
- **Design:** [XSH-7 record and JSON exactness](docs/rfc/XSH-7-record-json-exactness.md)

### XSH-10 · P3 · Replace Laputa's manual command parser

- **Area:** Navigability
- **Where:** `../laputa/pm/cli.xsh`, `../laputa/pm.xsh`
- **Problem:** Laputa parses subcommands with eleven placeholder-and-match blocks across about 850 lines. The parser duplicates behavior provided by `cli main` entries.
- **Recommendation:** Move the entries to `pm.xsh`, preserve the command contract, and delete the manual parser.
- **Fix with:** none
- **Effort:** M

### XSH-14 · P3 · Reuse import-graph checks across lint candidates

- **Area:** Performance
- **Where:** `crates/xsht/src/lint.rs`, `crates/xsht/src/cli`
- **Problem:** Each lint candidate probe parses, loads, and checks the unchanged import graph again. Large fix runs repeat the same work for every candidate.
- **Recommendation:** Measure shared checking, grouped candidates, and clean-corpus probe skipping. Keep the smallest design that preserves diagnostics and fix safety.
- **Blocked:** Guarded timing needs a quiet host. Candidate `cb8c23d6d` is preserved on `backlog-lane-diagnostics`, outside integration; `.work/wt/backlog-diagnostics` retains the comparison products and runner. Memory and diagnostic controls pass, but no valid timing samples exist.
- **Fix with:** make-it-fast
- **Effort:** M

### XSH-19 · P3 · Retire consolidation-only gate infrastructure after acceptance

- **Area:** Build
- **Where:** `dev/consolidation/winddown/gate-tools/`, `dev/performance/check.py`, `dev/util-linux/acceptance.py`, `docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md`
- **Problem:** Baseline qualification currently depends on campaign-specific controllers, receipt adapters and historical pins. Leaving those dependencies in the everyday gate path makes subsequent changes require campaign history. The accepted baseline is still pending, so deletion must follow G1.
- **Recommendation:** Execute G5 immediately after G1. Trace current callers, move reusable checks behind ordinary developer commands, and delete obsolete adapters, duplicate entry points and historical-pin dependencies. Preserve required gates and their rejection controls. Update testing and baseline docs. Keep historical evidence outside the everyday command path.
- **Fix with:** dead-code-archaeology
- **Effort:** M

### XSH-28 · P3 · Bound the narrowing proof cost

- **Area:** Performance
- **Where:** `src/runtime/eval/indexed/full/verify_schemes/producers.rs` (`checked_instruction_is_in_roots`)
- **Problem:** The producer work budget is now 32 units per instruction because dense programs need about 26. The ancestor walk in `checked_instruction_is_in_roots` is quadratic in nesting depth. No timing was measured.
- **Recommendation:** Measure verification time on the deepest corpus programs after the baseline exists, then cut the walk cost rather than raising the factor again.
- **Fix with:** make-it-fast
- **Effort:** M

### XSH-29 · P3 · Finish the XSHT integration failures

- **Area:** Tests
- **Where:** `crates/xsht/tests/cli.rs`, `crates/xsht/tests/lint_format_invariance.rs`, `crates/xsht/tests/desugar.rs`, branch `campaign/xsht-integration` (`e23c43739`, unreviewed)
- **Problem:** On `de42880c8` five XSHT tests fail: `test_roots_lower_contextual_nominal_imports_in_the_selected_scope` (verifier rejection), three `lint_format_invariance` tests (the corpus includes inference-erased `*.erased.xsh` witnesses; a redundant-parentheses fixture loses lints after formatting), and the desugared `core/tests/test-gnu-cp.xsh` run. Later merges may have fixed some.
- **Recommendation:** Rerun the five on current `master`, then fix the owners; reuse the WIP commit only where it still applies.
- **Blocked:** Candidate `6bc0df51d` on `backlog-park-xsht-29` remains outside integration. The four original non-desugar checks and a new collection-return lint control pass, as do isolated written/desugared GNU cp and fold files. Two full parity runs retain different timeout mismatches on the busy host. The existing eight-case per-file limit disproves the proposed oversubscription fix; resume with a quiet-host comparison or a demonstrated deadline cause, not another unchanged full run.
- **Fix with:** root-cause
- **Effort:** M

### XSH-30 · P3 · Bound the alias-DAG proof witness

- **Area:** Performance
- **Where:** `tests/xsh/proof-provenance.xsh:155`, `src/runtime/eval/indexed/full/verify_schemes/producers.rs`
- **Problem:** `test_record_projection_alias_dag_is_bounded_without_expansion` builds 256 aliases, each combining the previous alias with itself. Bounded baseline and candidate runs do not finish. The failing stage and cause remain unproven; this is not evidence of a new campaign regression.
- **Recommendation:** Reproduce the existing selector with a bounded run, separate checker and verifier cost, then preserve DAG sharing at the responsible owner. Keep the witness and do not raise its proof budget to hide the cost. Coordinate with XSH-28 if the owners coincide.
- **Fix with:** make-it-fast
- **Effort:** M

### XSH-23 · P4 · Evaluate bbtree as the ordered-map implementation

- **Deferred:** The owner is not interested in this evaluation at present. Exclude it from the active backlog run until the owner reopens it, even after a baseline becomes available.
- **Area:** Performance
- **Where:** `src/runtime/value.rs:172`, `src/runtime/value.rs:958`, `src/sema/schemes.rs`, `src/sema/constraints.rs`
- **Problem:** `laputa-systems/bbtree` claims about 5x faster single-thread lookups than std `BTreeMap` with `unsync::BBTreeMap` as a drop-in. XSH uses `BTreeMap` throughout the checker, verifier and runtime values, but most maps are probably tiny, where the gain may vanish. The measured campaign growth is generalization-map copying, which depends on clone cost. `unproven` that ordered-map operations matter at all.
- **Recommendation:** After the post-inference performance baseline, profile the 512-unit generalization witness and script startup to rank `BTreeMap` time. Only if it is a meaningful share, swap `unsync::BBTreeMap` in behind one alias at the hottest owner and compare interleaved CPU time. Vet the single maintainer, its `unsafe` and allocation code, `InlinkeKey` on our key types, and the build-time, binary-size and startup gates before widening.
- **Fix with:** make-it-fast
- **Effort:** M
