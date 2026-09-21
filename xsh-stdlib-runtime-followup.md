# XSH: finish the stdlib port by repairing semantics and removing runtime overhead

## Mandate

Continue in `laputa-systems/xsh`. Implement this work; do not stop at another audit or proposal. Read `AGENTS.md`, `xsh-rust-to-xsh-port-prompt-v2.md`, `STDLIB-PORT.md`, the canonical architecture/specification/test documents, and the actual owner code before editing.

The reviewed revision is `d0bbc6e74fa2d48e90e174bb6a5f0f4281c0bea2`. Use the actual current checkout, preserve unrelated changes, and record any differences from that revision. The migration ledger identifies `37e1502` as the original pre-port starting revision; resolve and preserve its full identity rather than substituting the older source-audit revision.

The port has made real progress, but it is not complete. This follow-up prioritizes the implementations already shipped: repair their observable behavior, make the tests actually execute, remove avoidable container/frame/preparation overhead, and remeasure the original gates. Do not expand into new filesystem, archive, ELF, `xshi`, account, or boot-policy ports while these requirements remain unmet.

Keep R12 script-backed during this work. Do not revert required groups, add native whole-policy fallbacks, raise budgets, or declare the constraints impossible based on current interpreter calibration. A measurement of today's implementation is not a lower bound on all implementations.

### Fixed owner decisions

- Embed ordinary XSH source. Prepare required stdlib implementations before user execution; an ordinary call, including its first invocation, performs no stdlib parsing, checking, lowering, source lookup, or first-use compilation.
- Use the existing verified indexed runtime. No `eval`, JIT, frozen executable-image project, persistent compiled cache, second interpreter, runtime source rewriting, or subprocess execution of library functions.
- Public APIs, effects, inference, namespaces, and established native fast paths remain unchanged. Public stdlib bindings are sealed; private operations remain accessible only to the verified implementation identities that own them. No authority based on filenames or caller privilege.
- Host verification is native aarch64 macOS. All Linux verification uses `Dockerfile.test` / `xsh-test`, `aarch64-unknown-linux-musl`, and the repository's existing internal workflow and flags. Docker availability is not evidence that a particular privileged/kernel/device test ran.
- Cold-start budget: `C - B <= max(0.05 * B, 1 ms)`. Designated end-to-end budget: `C - B <= max(0.10 * B, 2 ms)`. These remain cumulative against the original pre-port baseline. Existing native throughput/memory gates also remain in force.
- No new public API or CLI switches, dependencies, CI workflows, or unrelated refactoring. Follow `AGENTS.md` on formatter/autofixer avoidance and exact package builds.

This prompt explicitly authorizes narrowly scoped runtime, lowering, verifier, and test-harness repairs needed to satisfy the existing contracts. Calling them “outside a source port” is no longer a reason to leave them broken. This is not authorization to redesign the language.

## 1. Findings to address, not merely repeat

At the reviewed revision:

1. `stdlib/linux_text.xsh` documents a delayed-error incompatibility for `linux.modules`. The ledger explains that `eval_indexed_stream_producer` materializes producer output with `StreamValue::from_values`. Matching the complete collected output does not preserve lazy error or side-effect timing.
2. The same module's `append_line` reads the existing log as UTF-8, falls back to empty text on any read error, and rewrites the file. This is not the original append operation: invalid bytes, unreadable-but-writable files, and concurrent writers can behave differently or lose content.
3. `STDLIB-PORT.md` records that important R12 fixture tests exist only in throwaway source-rewriting harnesses. It also records many integration tests failing before they reach the product because `tests/runtime/common.rs::build_workspace_binaries` derives a Cargo profile from the test executable's parent directories. Equal failure-name sets do not validate the skipped product behavior.
4. `src/stdlib.rs::required_modules` scans syntactic identifier/field spellings. `arena_uses_dynamic_module_load` treats any identifier `load` or `module` as sufficient, and module-name mentions select all script-backed entries of that module. This is more conservative than resolved dependency selection and can charge unrelated programs for stdlib preparation.
5. The current explicit frame runner owns `Vec<u32>` statement lists and boxed continuations, decodes statement lists on calls/blocks, and reconstructs function headers/call bookkeeping on recurring paths. Profile these costs; do not assume they are unavoidable costs of XSH semantics.
6. `stdlib/ini.xsh` and the ledger identify whole-container cloning on `Record`/`Map` reads. A linear number of reads can therefore perform quadratic copying. Freezing only large lists does not solve that.
7. `stdlib/cli.xsh` retains `CliText`, `CliCount`, and `CliValue`-style adapters explained as workarounds for the `Result[Any]` return bug that the ledger says was subsequently fixed. Revalidate and remove obsolete boxing/wrapping where it truly is redundant; do not remove a wrapper that still represents a meaningful absent/present distinction.
8. `bench/stdlib-port/run.py` has no active Linux-only workload despite a `--linux` switch; repeated rounds overwrite earlier round data; reference always runs first; fixtures are accepted based on existence/size rather than content identity; timed output is discarded. Separate parity checks exist, but the benchmark report itself does not establish parity or stable input identity.
9. G05 rfkill was labeled `retained-performance` without its own timing. G02's positive result covers a large file while the ledger anticipates failure on small files. The ledger's G07 aggregate budgets also need correction: a 94 ms baseline implies 9.4 ms, and a 115 ms baseline implies 11.5 ms, not 2 ms. Those examples still fail, but bookkeeping must implement the actual formula.
10. `src/modules/system.rs` still contains Linux-only native parsing bodies behind `#[cfg(target_os = "linux")]`. Keeping distinct macOS implementations does not by itself justify retaining superseded Linux algorithms. Audit actual callers before deleting them.

Reconcile the user's newer R12 measurements with committed evidence. Treat supplied numbers as reported measurements until the reproducible harness, inputs, build identities, and raw results are available. Do not invent missing results.

## 2. Establish valid baselines and executable verification

Maintain two comparisons:

- **B0:** the original pre-port revision, for public behavior of migrated APIs and cumulative acceptance budgets.
- **B1:** the starting revision of this follow-up, for attribution of improvements and repairs.

Candidate results must report both `C - B0` and `C - B1`. Never replace B0 with B1 when deciding whether the migration passes. Preserve separate builds and test target directories. Record source commit/dirty state, binary hashes, target, features, compiler, profile/flags, and Linux image identity.

### Repair the product-binary test harness first

Fix the demonstrated workspace-binary discovery problem so integration tests resolve and execute the intended `xsh`, `xshi`, and `xsht` products. Prefer Cargo's emitted artifact metadata and the existing explicit package/build configuration over guessing a profile from a directory name. Handle debug/release, a custom `CARGO_TARGET_DIR`, explicit target triples, and the Linux workflow correctly. Do not silently run a stale installed binary, guess a new profile, or change the product build flags.

Use a clearly recorded test-only harness patch against B0 where necessary to run its tests correctly, keeping B0 production code and performance binaries unchanged. Report the patch. Preserve explicit pre-existing behavior failures separately from harness/environment failures. Rerun the previously blocked runtime, traceback, PTY/process, and small-stack tests; matching their old inability to start is not acceptance.

The three Linux filesystem errors cannot be exonerated merely because `fs` stayed native: common execution machinery is touched. Reproduce the same test with the same setup on the reference, identify the cause, and demonstrate the relevant changed paths have executed.

Do not treat a “full suite” containing hundreds of harness failures as successful verification. Fix the local harness/configuration defect within this scope. Any genuinely unavailable device-level qualification remains explicitly unexecuted; never silently skip it.

## 3. Restore exact behavior before drawing performance conclusions

### 3.1 Resumable producers using the existing execution machinery

Make the existing declared `stream` contract actually lazy in the indexed runtime. Do not merely change the R12 comment or collect the whole stream before returning it.

Implement an evaluator-aware resumable producer using the existing frame/continuation model and live-stream consumption paths. It must own the prepared code identity, retained symbol/program ownership, argument/capture values, local slots, suspended control state, and cleanup state necessary to resume. It must not retain invalid Rust borrows, clone an entire evaluator as a semantic substitute, or use a thread/channel/process per producer. Resume against the appropriate active evaluator; do not introduce a scheduler or another expression evaluator.

Required behavior:

- Calling an ordinary producer does not execute its body. Pulling resumes through the next `yield`, end, or propagated error.
- R12's public `linux.modules` wrapper still reads and snapshots its source text at call time, exactly as its original native implementation did. It returns a producer that parses retained rows at consumption time. Eager acquisition and lazy interpretation are different boundaries.
- An unreadable source returns the original call-time error even when no item is consumed. A malformed later row is not reached by a consumer that stops earlier.
- `yield expr?` propagates a failing expression, not an `Err` element; `yield some_result` without `?` remains a valid in-band Result when the declared element type permits it.
- Zero-argument producers work. Do not retain dummy arguments as the solution to the reported lowering bug.
- `defer` and owned host-resource cleanup run exactly once under the specified semantics for exhaustion, `break`, `take`, `first`, errors, cancellation, abort, and an abandoned started producer. Do not run body defers for a producer whose body never started.
- Preserve single-use/alias behavior, argument/capture semantics, lexical scopes, span/error attribution, evaluation order, and existing pipeline materialization boundaries. Do not turn a `List` result into an escaping lazy pipeline.
- Direct `for`, consume-in-place pipelines, nested `flat-map`, calls from loaded user modules, and existing supported worker paths must share these semantics.
- A large or unbounded producer consumed with a bounded terminal must not allocate all remaining output or perform work beyond the stopping boundary.

Derive context/capture behavior from the existing specification and tests, not from an accidental eager implementation. This fixes an existing contract; do not change the spec to bless the bug. Record any intentional difference from B1's broken behavior separately from unchanged public API parity against B0.

### 3.2 Restore genuine append

Replace read-concatenate-rewrite logging with the retained host append mechanism. First reuse an existing equivalent internal operation. If none exists, a narrowly typed private `append_bytes(Path, Bytes) -> Result[Unit]` mechanism is authorized. Its sole job is the original create/open/append/write behavior and error transport; XSH continues to own logging decisions, line construction, and policy. No new public append API or general host-call dispatcher.

Match the original parent handling, append flags, creation behavior, write ordering, and errors. Never turn an arbitrary read failure into an empty prior file. Do not promise record-level atomicity stronger than the original implementation. Preserve the original append guarantees and test concurrent writes of bounded fixture records where appropriate.

Test existing non-UTF-8 bytes, empty/new logs, missing parents, permission/open failures, non-UTF-8 paths, a destination that is a directory, and two appenders. Assert the old prefix survives and prior data is not truncated. Check gate/environment lookup semantics against the actual B0 dispatcher, including dry-run precedence and scoped-versus-host environment behavior.

### 3.3 Close the regression-test gaps without making internals public

Commit disk-backed fixtures and tests for the R12 cases named in the ledger: os-release quoting/escaping, duplicate keys and fallback ordering, missing-key and malformed-value error precedence, saturation, signed integer boundaries, module record fields, partial consumption, and delayed malformed-row failure.

Use two test boundaries:

- A crate-private, test-only catalog companion may invoke a specifically enumerated embedded pure helper with fixture values. Its provenance comes from the compiled test catalog, not an arbitrary source filename or a user parameter. It must execute the real embedded body and normal verifier. Do not expose this route to production/user module loading.
- Exercise complete public OS-facing calls against fixed-path fixtures inside the existing isolated Linux container/mount namespace. Use container-owned fixture storage and private mount propagation; never bind a synthetic `/proc` or `/sys` over the host or a shared mount. No permanent path-override environment API, and no text-replacement copy of the implementation as the sole test.

Private-helper tests do not replace public wrapper, host-error, or eager/lazy boundary tests. Expected failures must assert structure and timing, not merely a substring in collected stdout.

### 3.4 Fix the relevant lexical binding defect

The ledger's nested-name reproduction is a real resolver/lowering issue, not a reason all library locals should have globally unique spellings. Add its minimized disk-backed test and repair declaration-to-slot identity for nested scopes. Cover loop/block shadowing, captures, and inner mutation not changing the outer binding. Preserve standard-module reservation exceptions; do not solve ordinary lexical shadowing by reserving more names or weakening resolution.

Apply the same regression discipline to any Result/optional lowering defect that prevents the above repairs. Keep unrelated language-feature work out of scope.

## 4. Remove container copying and algorithmic amplification

Trace ownership through actual production `LoweredValue` operations, not only the higher-level `Value` representation. Inspect slot reads/binding, `lowered_freeze_large_slot_list`, `lowered_method_value`, argument binding, continuations, `RecordWithField`/`RecordRemoveField`, collection builders, and the conversions between runtime representations.

### Required runtime properties

1. Reading a container's length, existence of a key, or one field must not clone the entire container. Reading all n fields must not incur n whole-container copies. Account separately for ordered lookup complexity, required result-value copying, and one-time representation conversion.
2. Passing a read-only Record/Map/list through a chain of helpers must not copy all entries at each call.
3. Preserve value semantics using borrowed reads where safe and shared immutable backing/copy-on-write where ownership requires it. Prefer extending the existing ownership model over adding a public persistent-collection subsystem.
4. Mutating one alias cannot mutate another. Updates to nested values preserve unaffected siblings and prior versions. Do not coerce `Record` into `Map`, `Module` into a mutable dictionary, or lazy filesystem metadata into eager full records just to simplify dispatch.
5. Avoid clone-then-immediately-mutate on uniquely owned values. A proven consuming/last-use path may reuse storage, but only with correct argument/receiver evaluation order and failure behavior. A live receiver must remain the value evaluated before an argument that mutates its original slot.
6. Repeated local accumulation should not thaw/refreeze a growing container on every iteration. Reuse existing builders/comprehensions, uniqueness, and storage reuse without introducing shared mutable identity.
7. Native FS entry/stat-specialized paths and small scalar/record performance must not regress to pay for large-container wins.

Implement these properties with minimal cohesive changes. Do not assume “freeze Record/Map” alone fixes repeated persistent updates; measure the update path and alias cases as well.

Do not add public `Record.values()` or `Record.entries()` as the first solution. If the current representation fundamentally needs a bulk view for this port, one private typed read-only `record_entries` mechanism is allowed, with deterministic ordering and caller whitelist, plus evidence why the ordinary read path cannot provide the needed complexity. It must not validate INI, parse CLI schemas, or render JSON.

Add deterministic test-only allocation/copy counters for focused cases where existing instrumentation is insufficient. Demonstrate that read-only whole-container copies do not grow with the number of field reads. Use geometric size sweeps (for example 16, 64, 256, 1024, 4096 fields) and separate construction from traversal in diagnostic measurements. Keep original end-to-end workloads unchanged and measured in full.

After the runtime fix, simplify INI/CLI/JSON code that worked around copying. Remove obsolete Result wrappers only after proving their absent/present/error distinctions are preserved. Test that `Err`, `null`, absent fields, and `Ok(null)` remain distinct where the public contract distinguishes them.

## 5. Make ordinary calls and loop bodies cheap without bypassing semantics

Profile the normal indexed runtime with the ported workloads and plain user-written equivalents. Investigate at least:

- `ExplicitFrames::push_call`, `push_call_with_header`, and recurring `call_header` work.
- `decode_statements`, `FrameWork::Statements`, block entry and branch/match list decoding.
- `Box<FrameContinuation>`, temporary argument vectors, slot/scoping vectors, and frame creation/recycling.
- Per-call function lookup, formatted display names, traceback construction, and inactive tracing work.
- Cloning between `Value` and `LoweredValue`, Result wrappers, captures, method receivers, and stage invocation.
- Repeated decoding/checking of immutable function metadata that preparation already knows.

Implement a shared fast path for verified simple calls and stage bodies, preferably by removing overhead in the existing frame engine. Borrow immutable instruction/block ranges with an index rather than allocate/reverse a `Vec` on every block iteration. Retain prepared metadata with its owning program where measurements justify it. Reuse bounded frame/argument scratch storage. Do not construct an independently maintained second executable graph or duplicate expression semantics in a separate “stdlib evaluator.”

A bounded leaf specialization or inlining optimization is allowed only when selected from verified body/actual effects and applicable equally to ordinary user functions. It must not recognize stdlib names, source text, fixture hashes, or known workload shapes. Do not infer safety from a public `pure` flag alone: CLI's legacy public contract hides host reads that its actual implementation declares.

Safety constraints:

- Preserve parameter/default/rest binding, return checks, Result propagation, `?` versus in-band Results, evaluation order, overflow/NaN/Unicode/path semantics, and public error/trace behavior.
- Preserve capture writes and lexical resource ownership. Skip host-scope machinery only with a proof that no transitive operation/capture/argument needs it.
- Retain signal/cancellation checkpoints in long loops. Making a benchmark faster by eliminating responsiveness is a regression.
- No unbounded Rust recursion, larger thread stacks, disabled depth tests, or a new class of native-stack overflow. Reuse the explicit-frame fallback before execution when a specialization cannot safely cover the body; never restart after partially executing effects.
- Function and metadata cache identities include their owning program and lifetime. Do not revive the dynamic-module stale-index bug or retain unbounded old programs in sessions.
- Private primitives remain authorized by the verified instruction owner. Do not inline a bridge into user-owned code and thereby bypass or invalidate its access check. User callbacks never inherit an internal caller's authority.
- Tracing-disabled savings must preserve error stack reconstruction; tracing-enabled execution preserves the established observable events. Test both.

Use a crate-private test mode to force the generic and optimized routes through the same public programs. This is test configuration, not a user-facing switch or production dual backend. Differential-test scalars, Result success/failure, defaults/rest, recursion/mutual recursion, nested compound expressions, captures, streams, dynamic modules, and small-stack execution.

Success is not a claim that loop steps became “10× faster.” Show fewer concrete allocations/decodes/copies per operation, a reproduced improvement on the affected complete workloads, and no native/control correctness or performance regression. Report remaining distance to each original gate.

## 6. Fix preparation selection and charge all preparation honestly

Restore the intended registry-resolved dependency model. Select script-backed implementations from resolved public entry/overload and method-receiver identities, including supported callable references and command/value-stage forms. A local variable/function called `load`, an ordinary record field called `module`, or a native function in a mixed standard module is not automatically a reference to every embedded implementation.

Use existing declaration/signature resolution against the public registry to discover dependencies, then attach and check the selected source closure. Avoid doing the entire frontend twice or repeatedly rescanning the whole growing arena after each module. A worklist over newly resolved module dependencies is sufficient; this is not a request for a whole-program optimizer.

Keep these rules:

- Native-only programs prepare no unrelated script-backed modules.
- A genuine resolved `module.load` reference, even in a branch not executed, or an existing opaque loading route whose needs cannot be bounded, still prepares the complete applicable target/feature set before execution. Do not redefine it as first-call preparation to pass `cold_dynamic_ref`.
- Type uncertainty is handled conservatively, with a documented finite over-approximation. Never drop a possible implementation merely to improve a timing.
- Match the current platform/features; an embedded source retained for other targets does not necessarily belong in the current target's dynamic-load closure.
- Registry signatures remain authoritative. Reject a missing/incompatible implementation during preparation. No filesystem fallback for builtin identities.
- `xshi` reuses compatible prepared modules within the proper session/program/symbol ownership; each submitted input is a preparation boundary. Dynamic user modules use already prepared standard implementations.
- No global executable cache with session-owned IDs, persistent compiled artifact, startup snapshot of host state, or runtime compilation.

Add negative-selection tests for misleading names/fields and native calls in mixed modules, plus positive tests for all supported resolved routes and genuine dynamic loading. Assert parse/check/lower preparation counters before and after successful, failing, callback, loaded-module, and stream execution—not only the happy-path parse count.

Measure lex/parse, declaration/body checking, dependency discovery, lowering, verification, and execution separately for diagnosis. Cold-start acceptance still measures the entire process including all preparation. Module splitting is allowed only at coherent source/dependency boundaries that reduce real required work; do not remove comments solely to game source-size numbers or build a new tree-shaker/freezer.

## 7. Use retained kernels intelligently; do not restore policy in Rust

After profiling and the common runtime work, reduce gratuitous interpreted work in the XSH implementations using existing bulk string/byte/search/format/collection operations. Preserve the exact grammar, order, saturation, and error behavior.

Examples to investigate:

- Replace a byte loop testing membership in a fixed forbidden-character set with equivalent existing bulk searches when that preserves behavior.
- Keep wrapping/padding policy in XSH while avoiding repeated scalar rescans, repeated string-prefix copies, and temporary tiny records for each character. Use existing correct Unicode iteration/splitting boundaries; do not treat bytes as characters.
- Remove compatibility wrappers whose motivating Result bug has been repaired and tested.
- Avoid rereading the same schema field or rebuilding option lookup tables inside one invocation. Preserve schema traversal and first-error order. Do not cache mutable/user-provided schemas across calls or cache host-dependent validation.

A small, private strict signed-decimal conversion primitive is explicitly authorized if no existing equivalent exists. It must implement one general scalar conversion corresponding to the baseline `str::parse::<i64>` behavior, including both signed bounds and its exact acceptance grammar. Return a typed optional/result and let XSH own each caller's trimming, validation policy, and domain error messages. This may replace duplicated digit loops in CLI, env, and Linux parsers. It must not parse complete `/proc` rows, CLI descriptors, INI, routes, or option lists, and must not silently change the public `Str.parse_int` grammar.

The only new private mechanism categories pre-authorized by this follow-up are genuine append, this scalar conversion, and a justified read-only bulk record view. Prefer existing equivalents. Use the existing typed descriptor/provenance/verifier mechanism with explicitly enumerated owners; no operation selected by an arbitrary user string, unchecked cast, generic syscall mechanism, or blanket internal effect exemption.

Do not add more bridges just to reach a benchmark number. If a further mechanism appears necessary, finish independent work and report its exact semantics and remaining workload rather than quietly implementing a native policy kernel. Preserve all deferred native boundaries.

## 8. Repair and integrate performance evidence

Improve the existing `bench/stdlib-port/` harness rather than creating another benchmarking framework.

### Required harness corrections

- Keep all existing 24 workload identities and semantics. Include the separately reported tooling measurements in a reproducible route rather than an unrepeatable prose note.
- Add all five R12 entries to the Linux manifest. Use exact reproducible fixed-path fixtures, including the reported 200-module case, and exercise real mode rather than accidentally benchmarking dry-run or `linux-unimplemented` errors.
- Preserve every round and every raw sample; report round-level and aggregate medians/dispersion. Do not overwrite previous rounds or conflate duplicate failure-list entries with independent workloads.
- Counterbalance AB/BA ordering or use a recorded deterministic shuffle. Keep matched builds and host/container allocation. Do not compare macOS timing to Linux timing or debug to release.
- Use monotonic high-resolution durations; do not round sub-millisecond reference totals to zero before arithmetic. For diagnostic repeated-call calibration choose enough iterations to resolve timing, but do not silently change the iterations or budget of an existing acceptance workload.
- Hash/verify deterministic fixture contents and regenerate mismatches. Existence or a generous size ceiling is not fixture identity.
- Establish output/status/side-effect parity before accepting timing, using the same fixture and script hashes. Keep exact byte comparison where appropriate; for changing live host data use documented invariants or deterministic fixtures, never an unexplained normalization that masks a regression.
- Timing may suppress stdout consistently, but the result must reference its successful parity run. Capture expected nonzero outcomes correctly and fail on unexpected errors/timeouts.
- Record binary/source/script/fixture hashes, features/toolchain/flags, environment, image, sample/iteration counts, units, and computation formula. Serialize actual skips and reasons; an inert `--linux` flag is not Linux coverage.
- Validate the harness with a tiny known synthetic result set, including multiple rounds and budgets above/below the absolute floor. G07's 94/115 ms examples must calculate 9.4/11.5 ms.

### Workload coverage

Retain fixed acceptance workloads unchanged. Add diagnostic size sweeps for container reads/updates, nested JSON paths, small/wide CLI schemas, Unicode wrapping, and simple calls/stages. Keep allocation and stage-timing instrumentation outside authoritative uninstrumented timing runs.

For R12, separate eager acquisition, row interpretation, stream creation, first-item/partial consumption, and full consumption. The full-consumption workload must actually observe parsed record content or errors; do not optimize `.count()` by bypassing fallible parsing. Test empty, small, and large inputs, malformed late rows, and changing fixtures between calls so no cache can manufacture a win.

Cover G02 verification over tiny files, large files, and many small files, including error cases. The accepted large-file measurement does not clear the full class. Resolve the existing group according to the original G policy once its full evidence is available: it may stay script-backed only if its applicable gates pass; otherwise the gated policy may be restored natively with the measured reason. This exception applies to G02, not to required R groups.

Correct G05 rfkill's historical status: mark the existing assertion unmeasured/unqualified rather than treating the block-device proxy as its measurement. Do not claim a new pass. Revisit that prototype only after the current required semantic repairs and primary performance round are finished; if evaluated, it requires its own correct fixture, timing, and deletion accounting. Do not reopen all deferred ports in this task.

Measure the original B0 and the follow-up B1 alongside the candidate. A speedup from B1 that still exceeds B0's budget remains a failing migration gate. Remove universal “10–30× native work” screening claims from authoritative conclusions; classify workloads by measured cost decomposition, scaling, and actual outcomes instead.

## 9. Delete superseded native policy and document the real boundary

After parity is established, remove unreachable required-policy bodies, obsolete dispatcher/lowering special cases, and obsolete temporary workarounds. Specifically audit Linux-only implementations in `src/modules/system.rs`, the relevant Linux text owners, and `unix` uptime. Keep genuinely shared native callers and macOS implementations; move language-level assertions to native XSH tests rather than retaining production algorithms solely for old helper tests.

Keep one production implementation of each migrated policy. Test-only reference builds are fine; a dormant production fallback or duplicate Linux parser is not. Prove the selected call paths execute embedded code even after optimization.

Report production Rust added and deleted, XSH changed, generated metadata, tests, and cumulative net change from B0 using one counting method. A general runtime improvement may add Rust in this phase; do not disguise it or suppress a necessary correctness repair to hit a local deletion quota. The overall migration's maintenance-reduction objective still stands, and restored hidden policy or relocation to another crate is not a reduction.

Update the existing canonical architecture/stream/test documentation and `STDLIB-PORT.md`. Remove stale explanations that say a fixed bug is still fundamental or that a cfg-Linux body is needed for macOS. Do not duplicate the specification in another sprawling guide.

## 10. Verification and completion

Run the narrow tests during development, then the applicable full gates from the current `docs/TEST-MAP.md`. At minimum this includes:

- Correctly staged root and `xshi`/`xsht` product integration suites, including previously blocked tests; native XSH stdlib/corpus tests; the API registry/signature/surface gates.
- Existing stdlib architecture A01–A20 coverage, strengthened where the implementation only tested a subset; public namespace sealing, private bridge leakage/provenance, dynamic loading and symbol/program teardown.
- Generic-versus-optimized cases; debug and release behavior parity; small-stack, recursion, Result propagation, lexical capture/shadowing, signal/cancellation, and stream early-close/defer tests.
- Native-only filesystem/hash/JSON/network and representative stream controls. Common-runtime changes require checking retained kernels' callers too.
- The supported features/platform matrix, including no-default-features and native-tests behavior, on native aarch64 macOS and the specified ARM Linux/musl container route. Use existing exact package commands and build flags.
- Copied-product execution from an unrelated directory with no stdlib checkout, hostile user module roots, and relevant first-use/loaded-module programs.
- Repeated uninstrumented release performance rounds against B0 and B1, including R12 and real commands, plus retained/peak memory and allocation/copy evidence.

Add tests that directly prove the complexity and control-state claims instead of relying solely on noisy timing: no n-sized clone per Record read; no repeated statement-list allocation per loop iteration; no producer-body execution before a pull; no late-row evaluation after an early stop; zero stdlib preparation after execution begins.

### Work order

1. Establish B0/B1, repair the blocked test harness, and make benchmark evidence reproducible.
2. Repair append and resumable stream semantics; close fixed-path/catalog fixture coverage and the directly relevant lowering/binding defects.
3. Remove container-copy amplification and simplify obsolete library workarounds.
4. Remove call/block/stage overhead in the existing runtime, with verified specialization only where justified.
5. Tighten resolved preparation dependency selection and profile remaining cold-start work; optimize XSH use of existing kernels and the one authorized scalar conversion as needed.
6. Rerun cumulative gates, qualify G02 and historical evidence honestly, remove dead native policy, and finish the ledger.

Parallelize independent fixture/harness/library work only after one owner fixes the shared ABI and runtime plan. Keep a single integration owner for loader/checker/IR/value/frame changes. Do not let workers introduce competing bridge or execution designs.

### Final deliverables

Deliver the implemented repairs and optimizations, committed regression fixtures/tests, the improved existing benchmark harness and reproducible results, deletion accounting, and updated canonical docs/ledger. The final report must contain:

- Exact repaired behavior gaps and the tests proving each boundary.
- Before/after allocation/copy/decoding evidence for the container and call-path work.
- Per-workload B0/B1/candidate medians, deltas, actual budgets, parity status, and remaining distance to acceptance on each platform.
- Whether every applicable test actually executed, with individual causes for any pre-existing failures or unavailable qualification.
- New private mechanisms and their whitelisted owners; proof that no public namespace, effect, or privilege boundary was broadened.
- Production code accounting and any still-unresolved R/G status.

Do not stop after a profiling report or the first speedup. Implement all independent safe work above. Do not claim the overall migration is complete until its required behavior, architecture, test, and cumulative performance gates pass. If one remains red, leave the ledger explicitly incomplete and report the exact measured blocker and smallest remaining change—not a blanket claim that interpreters cannot do better, an unapproved revert, or another request to decide the architecture already fixed here.
