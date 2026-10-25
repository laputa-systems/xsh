# TODO

Prefix: XSH
Next: 30

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

### XSH-8 · P3 · Require RelPath for rooted filesystem operations

- **Area:** Security
- **Where:** `docs/reference/stdlib.md:721`, `crates/xsh-registry/src/signature/modules.rs`
- **Problem:** `FsRoot` operations accept `Path`, so callers can pass absolute paths or paths that climb above their lexical start. The opt-in lint reports 637 current call sites.
- **Recommendation:** Migrate callers to `RelPath`, then change the public `FsRoot` signatures and generated reference together.
- **Fix with:** api-surface-and-module-boundaries
- **Effort:** L

### XSH-9 · P3 · Give Linux path sinks path-safe parameter types

- **Area:** Types
- **Where:** `src/runtime/eval/lowered_run/module_linux.rs:163`, `src/runtime/eval/lowered_run/module_linux.rs:335`, `src/runtime/eval/lowered_run/module_accounts.rs:10`
- **Problem:** `linux.mount`, `linux.modinfo`, and `applet.su_session` accept text for values that cross filesystem boundaries. The `su_session` command and shell parameters still need classification.
- **Recommendation:** Classify each parameter, migrate path values to `Path` or bytes, and update signatures, callers, tests, and docs together.
- **Fix with:** api-surface-and-module-boundaries
- **Effort:** M

### XSH-10 · P3 · Replace Laputa's manual command parser

- **Area:** Navigability
- **Where:** `../laputa/pm/cli.xsh`, `../laputa/pm.xsh`
- **Problem:** Laputa parses subcommands with eleven placeholder-and-match blocks across about 850 lines. The parser duplicates behavior provided by `cli main` entries.
- **Recommendation:** Move the entries to `pm.xsh`, preserve the command contract, and delete the manual parser.
- **Fix with:** none
- **Effort:** M

### XSH-11 · P3 · Isolate Linux CI products from the host target

- **Area:** Build
- **Where:** `dev/test_workflows.xsh:256`, `dev/docker.xsh:168`
- **Problem:** `cargo dev test linux --ci` passes the host target into the container and shares `target/`. It works only on Linux and overwrites host release products.
- **Recommendation:** Give the container an explicit Linux target and an isolated product directory while retaining shared reusable build inputs where safe.
- **Fix with:** isolate-effects
- **Effort:** M

### XSH-14 · P3 · Reuse import-graph checks across lint candidates

- **Area:** Performance
- **Where:** `crates/xsht/src/lint.rs`, `crates/xsht/src/cli`
- **Problem:** Each lint candidate probe parses, loads, and checks the unchanged import graph again. Large fix runs repeat the same work for every candidate.
- **Recommendation:** Measure shared checking, grouped candidates, and clean-corpus probe skipping. Keep the smallest design that preserves diagnostics and fix safety.
- **Fix with:** make-it-fast
- **Effort:** M

### XSH-15 · P3 · Qualify or remove the regex-lite fork

- **Area:** Dependencies
- **Where:** `third_party/regex-lite/Cargo.toml:1`, `third_party/regex-lite/src/nfa.rs`
- **Problem:** The vendored regex-lite 0.1.9 fork replaces a capture-name `HashMap` to avoid `getrandom`. Its build-time and binary-size costs remain unmeasured.
- **Recommendation:** Measure the fork against upstream. Drop it when upstream avoids the dependency, or retain it with current cost evidence.
- **Fix with:** benchmark-checklist
- **Effort:** S

### XSH-16 · P3 · Make Laputa Linux package tests hermetic

- **Area:** Tests
- **Where:** `../laputa/packages/linux/tests/main.xsh`
- **Problem:** Seven Laputa tests require writes to `/var/cache/laputa/linux-kbuild`. They fail under the unprivileged x86-64 Alpine test environment.
- **Recommendation:** Route the cache through a test-owned root or fixture and rerun the seven selectors without host privileges.
- **Fix with:** isolate-effects
- **Effort:** S

### XSH-19 · P3 · Retire consolidation-only gate infrastructure after acceptance

- **Area:** Build
- **Where:** `dev/consolidation/winddown/gate-tools/`, `dev/performance/check.py`, `dev/util-linux/acceptance.py`, `docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md`
- **Problem:** Baseline qualification currently depends on campaign-specific controllers, receipt adapters and historical pins. Leaving those dependencies in the everyday gate path makes subsequent changes require campaign history. The accepted baseline is still pending, so deletion must follow G1.
- **Recommendation:** Execute G5 immediately after G1. Trace current callers, move reusable checks behind ordinary developer commands, and delete obsolete adapters, duplicate entry points and historical-pin dependencies. Preserve required gates and their rejection controls. Update testing and baseline docs. Keep historical evidence outside the everyday command path.
- **Fix with:** dead-code-archaeology
- **Effort:** M

### XSH-20 · P3 · Move mdev command policy out of the native backend

- **Area:** Architecture
- **Where:** `core/mdev.xsh:2`, `crates/xsh-applets/src/mdev.rs:212`, `crates/xsh-applets/src/mdev.rs:379`
- **Problem:** The XSH command delegates argument parsing, configuration rules and event policy to Rust. This conflicts with the documented division between XSH command policy and Rust host operations.
- **Recommendation:** Preserve the existing behavior in XSH and retain only required device and sysfs operations in Rust. Require measurements and an XSH implementation for any native performance exception.
- **Fix with:** api-surface-and-module-boundaries
- **Effort:** L

### XSH-24 · P3 · Close the parked verifier narrowing and typing gaps

- **Area:** Types
- **Where:** `src/runtime/eval/indexed/full/verify_schemes/`, `tests/xsh/signature-cli.xsh`, `tests/xsh/proof-provenance.xsh`, `tests/xsh/guarded-control-proofs.xsh`, `tests/xsh/pattern-tests.xsh`, `tests/xsh/wait-until.xsh`, `tests/xsh/default-parameters.xsh`
- **Problem:** The checker accepts these programs and the indexed-IR verifier rejects them. Witnesses: `cli main(...operands: List[Str]) [] { print ${operands.join(",")} }` reaches the verifier as `List(Any)`; a type-test proof joined across if/else branches (`if c { assert a } else { assert a }` then a guarded read); top-level `for raw in xs { continue when raw == null; let s: Str = raw }`; top-level `let outcome = Err("failed")`; `try { wait until false within 50ms backoff 10ms..20ms }`; top-level `let shared = combine` then `shared.call(left: 9)`; `.call` on an erased `Pure` handle typed `Any` instead of `Result[Any]`; a generic body calling an `export let` alias (lowering blocker).
- **Recommendation:** Give each a verified link to the evidence the checker already holds, as the earlier narrowing repairs did; never admit unauthenticated IR.
- **Fix with:** root-cause
- **Effort:** M

### XSH-25 · P3 · Decide the error effect of a stream parameter

- **Area:** Errors
- **Where:** `tests/xsh/yield-delegation.xsh`, `tests/xsh/stdlib/rel_path.xsh`
- **Problem:** The checker rejects `for e in paths` over a `Stream[Path]` parameter in an `[io]` proc yet accepts `stream parent(s: Stream[Int]) [] -> Stream[Int] { yield @s }`, while the verifier requires `error` for the second. Both tests expect acceptance.
- **Recommendation:** Owner decision on whether pulling an opaque stream implies `error`, then align checker, verifier, SPEC and both tests.
- **Fix with:** error-model
- **Effort:** M

### XSH-26 · P3 · Type the pull error of a process stream

- **Area:** Errors
- **Where:** `tests/xsh/run-accept-stream-capture.xsh`, `src/sema/check`
- **Problem:** `Stream[T]` has no pull-error type, so a typed `ProcessError` is lost on stream iteration. Typing it in the checker made the verifier fail with "captured error producer disagrees with its nominal error contract".
- **Recommendation:** Add process-stream provenance to the checker and the verifier's captured error types together, or record that pull errors stay untyped.
- **Fix with:** error-model
- **Effort:** M

### XSH-27 · P3 · Decide typed-callable facts for non-exact module projections

- **Area:** Types
- **Where:** `tests/xsh/constant-key-projections.xsh`
- **Problem:** `plugin.get("increment")?` on a module required as a declared type is typed as a callable by the checker, but the module is non-exact, so admitting the fact would trust a signature the runtime `require` never validates.
- **Recommendation:** Decide whether non-exact modules may carry a callable scheme through `get`; if not, make the checker stop typing it.
- **Fix with:** architect
- **Effort:** S

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
- **Fix with:** root-cause
- **Effort:** M

### XSH-5 · P4 · Decide whether inferred proc returns become default

- **Area:** Config
- **Where:** `crates/xsht/src/lint_prefer_inferred_proc_return.rs:1`, `docs/XSHT.md:68`
- **Problem:** `lint.prefer-inferred-proc-return` is opt-in and reports about 160 current sites. The repository has no decision on making the rule default.
- **Recommendation:** Review the fixes for public readability and checker stability, then either enable the rule by default or record why it remains opt-in.
- **Fix with:** test-audit
- **Effort:** S

### XSH-6 · P4 · Add a named contract validation error

- **Area:** Errors
- **Where:** `docs/SPEC.md`, `src/runtime`
- **Problem:** A failed `.require(Contract)` raises plain `Error` facets. Callers cannot match contract validation failures by a stable family name.
- **Recommendation:** Add `ContractError` only if real callers need typed recovery, then update propagation, matching, tests, and the SPEC together.
- **Fix with:** error-model
- **Effort:** M

### XSH-17 · P4 · Diagnose two inherited Laputa package-manager failures

- **Area:** Tests
- **Where:** `../laputa/tests/pm/pm_execute.xsh`
- **Problem:** Two `pm_execute` tests lose the proof's error text. They fail on both old and current toolchains, so the XSH migration did not establish their cause.
- **Recommendation:** Reproduce each test in Laputa, trace the missing diagnostic, and fix the owner only after the failing boundary is known.
- **Fix with:** root-cause
- **Effort:** S

### XSH-23 · P4 · Evaluate bbtree as the ordered-map implementation

- **Area:** Performance
- **Where:** `src/runtime/value.rs:172`, `src/runtime/value.rs:958`, `src/sema/schemes.rs`, `src/sema/constraints.rs`
- **Problem:** `laputa-systems/bbtree` claims about 5x faster single-thread lookups than std `BTreeMap` with `unsync::BBTreeMap` as a drop-in. XSH uses `BTreeMap` throughout the checker, verifier and runtime values, but most maps are probably tiny, where the gain may vanish. The measured campaign growth is generalization-map copying, which depends on clone cost. `unproven` that ordered-map operations matter at all.
- **Recommendation:** After the post-inference performance baseline, profile the 512-unit generalization witness and script startup to rank `BTreeMap` time. Only if it is a meaningful share, swap `unsync::BBTreeMap` in behind one alias at the hottest owner and compare interleaved CPU time. Vet the single maintainer, its `unsafe` and allocation code, `InlinkeKey` on our key types, and the build-time, binary-size and startup gates before widening.
- **Fix with:** make-it-fast
- **Effort:** M
