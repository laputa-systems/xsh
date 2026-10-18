# TODO

Prefix: XSH
Next: 21

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
- **Fix with:** xsh-consolidation-campaign
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

### XSH-12 · P3 · Align CLI and in-process checker diagnostics

- **Area:** Errors
- **Where:** `dev/consolidation/designs/check-session.md:28`, `crates/xsht/src/cli/lint.rs:3217`
- **Problem:** `xsht check`, `xsht lint`, and the in-process checker can report different primary codes for migration diagnostics, `proc main` parameters, and unresolved modules.
- **Recommendation:** Define one diagnostic contract for loaded and source-only checks, then make each entry point preserve it.
- **Fix with:** error-model
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

### XSH-18 · P3 · Restore the ambient filesystem policy guard

- **Area:** Tests
- **Where:** `tests/ambient_fs_policy.rs:590`, `src/modules/linux/real/netlink.rs:1490`
- **Problem:** Canonical verification ignores `ambient_filesystem_use_is_allowlisted` pending a netlink allowlist decision. Source review finds the netlink filesystem operations inside its test module, including opt-in capture and replay. The disabled guard supplies no policy-test coverage.
- **Recommendation:** Scope the permitted fixture I/O to test code without allowing production filesystem access throughout the file. Remove the ignore and run the exact guard plus a rejection control for an unapproved production access.
- **Fix with:** os-boundaries
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

### XSH-13 · P4 · Add the example lint corpus to the standard test plan

- **Area:** Tests
- **Where:** `crates/xsht/tests/examples.rs:190`, `dev/tests/test-gates.xsh:240`
- **Problem:** The standard XSHT test plan skips `examples::example_corpus_lints_without_warnings`. Contributors must know and run the target separately.
- **Recommendation:** Decide which local gate owns the corpus pass, include it once, and keep the timing-line normalization inside the test.
- **Fix with:** renovate-tests
- **Effort:** S

### XSH-17 · P4 · Diagnose two inherited Laputa package-manager failures

- **Area:** Tests
- **Where:** `../laputa/tests/pm/pm_execute.xsh`
- **Problem:** Two `pm_execute` tests lose the proof's error text. They fail on both old and current toolchains, so the XSH migration did not establish their cause.
- **Recommendation:** Reproduce each test in Laputa, trace the missing diagnostic, and fix the owner only after the failing boundary is known.
- **Fix with:** root-cause
- **Effort:** S
