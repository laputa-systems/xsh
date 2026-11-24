Prefix: XSH
Next: 191

Apply the [design principles](docs/DESIGN.md) to each slice: prefer reusable
host boundaries and ordinary XSH policy over command-specific runtime growth.
Existing distribution follow-ups remain in
[Laputa's queue](https://github.com/laputa-systems/laputa/blob/master/TODO.md);
its [roadmap](https://github.com/laputa-systems/laputa/blob/master/docs/ROADMAP.md)
owns VM milestones. Implement each item in its owning repository.

The joint review on 2026-11-13 inspected XSH `d9d1d7bcb78c` and Laputa
`bfe60a9ac40f`, with priorities assumed from impact and likelihood. It excluded
work already owned by the queues, roadmaps, and
[campaign handoff](dev/campaign/RESUME-2026-11-14.md). Runtime proof limits are recorded in each new item.

The systems-foundation items were reviewed against `9d893e0075d2`; findings
marked unproven need a reproducer, not an assumed defect. Existing acceptance
items remain open. The performance/deployment campaign retains its own
pause/launch rules; the owner resumed the Lean campaign on 2026-11-21. The Deser
codec cutover is adopted unconditionally. Native macOS qualification runs on
the m1-max host only. The [G0 release candidate](docs/ROADMAP.md#g0-release-candidate) decides
which items block a release. Do not make an unrelated full baseline an entry fee
for a bounded fix; use the narrow applicable gate. Real-hardware enablement is
out of scope.
### XSH-190 · P2 · Importing an empty module panics during checking

- **Area:** Module loading and semantic checking
- **Where:** src/sema/check/module_record.rs:324 (module_source_id)
- **Problem:** The verification binary panics with `module source identity missing` for a user module with no statements or module documentation, including an empty imported file. Reproduce with `mkdir -p /tmp/xsh-empty-import; : > /tmp/xsh-empty-import/shared.xsh; printf "use shared\n" > /tmp/xsh-empty-import/main.xsh; target/verification/xsh /tmp/xsh-empty-import/main.xsh`; the process exits 101 instead of checking the program. This also occurred on base e99cfcb8c before O4.
- **Recommendation:** Give an empty imported module its source identity and verify it through the native module-loading paths.
- **Fix with:** root-cause
- **Effort:** S

### XSH-189 · P3 · The comparative benchmarks pin a local test image that no longer exists

- **Area:** Performance tooling
- **Where:** `bench/consolidation/versions.lock.json:5` (`base_images.amd64`, pinned in `e3e1656d5` on 2026-11-16); `bench/consolidation/prepare.py` (`docker image inspect` of that ID)
- **Problem:** The comparative overlay is built from a local `Dockerfile.test` image ID, `sha256:0945d878…`. That image is no longer on the host, so `python3 -B bench/consolidation/prepare.py … --build` fails and `cargo dev bench --workload consolidation-directory` / `consolidation-json` stop at preflight (exit 3). B1 recorded both rows as inconclusive on 2026-11-24. A local image ID cannot be fetched again, so any prune breaks these benchmarks.
- **Recommendation:** Pin the published test image digest from `dev/images.lock` (as the gates already do) instead of a local image ID, pull it when absent, and keep the fixture hash check. Rebuild the overlay and rerun both workloads.
- **Effort:** S

### XSH-188 · P2 · Dynamic module loads re-lower their static import closure

- **Area:** Runtime modules, performance
- **Where:** `prepare_dynamic_module` in `src/runtime/eval/lowered_run.rs`
- **Problem:** Distinct `module.load` preparations reuse unchanged imports' parsed and checked forms, but still lower each import's top level and functions for every importing program.
- **Recommendation:** Reuse lowered imported modules where their binding namespaces and per-program initialization semantics can be preserved. Measure the change with the O4 load probe and M1 fetch workload.
- **Fix with:** make-it-fast
- **Effort:** M

### XSH-187 · P2 · A change to the HTTP client never runs the curl and wget command tests

- **Area:** Verification gates
- **Where:** `dev/gates.xsh:464` (`crates/xsh-net/` selects only the Runtime area); `core/tests/test-curl.xsh`, `core/tests/test-wget.xsh`
- **Problem:** `cargo dev gate crates/xsh-net/src/lib.rs` selects the Rust runtime and net unit tests but not the native curl and wget tests, the only tests that drive downloads end to end. XSH-181 (`c127a1b7b`) passed its gate while breaking `curl -o /dev/null` and `wget -O /dev/null`; only the full Linux run showed it.
- **Recommendation:** Select `core/tests/test-curl.xsh` and `core/tests/test-wget.xsh` for changes under `crates/xsh-net/`, within the unprivileged lane, and keep a gate-plan test for that mapping.
- **Fix with:** single-source-of-truth
- **Effort:** S

### XSH-186 · P4 · The byte scanner's nested flag is never read

- **Area:** Runtime lowering
- **Where:** `src/runtime/eval.rs:2029` (`ScanBytes.nested`), `src/runtime/eval/lower/scalar.rs` (`scan_exact_body` always returns `false`), `src/runtime/eval/indexed/full/decode.rs:1153`, the formal-bridge program image
- **Problem:** The fused `ScanBytes` runtime implements only the non-nesting comment scanner and never reads `nested`; the matcher now admits only exact non-nesting scanners and always sets it to `false`. The field is carried through lowering, decoding and the program image with no consumer.
- **Recommendation:** Remove the field and its encoding. Add nesting only with a runtime that implements it and a native test that compares fused and unfused results.
- **Effort:** S

### XSH-185 · P2 · par-map still loses writes made through calls and environment assignment

- **Area:** Runtime parallelism
- **Where:** `src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs:1306-1307` (route selection: `!evaluator.trace_enabled && jobs > 1 && item_count > 1`), `src/runtime/eval/lowered_run/indexed_run/pipeline.rs:102` (workers start from cloned `base_slots`); `src/sema/check/stmt.rs` (`check.par-map-captured-write`)
- **Problem:** On the parallel route each worker runs on a copy of the evaluator's state, so writes a callback makes outside its own locals are lost, while the sequential route (one item, `jobs: 1`, or `xsht trace`) keeps them. A program's result therefore depended on item count, `jobs:`, available CPUs and whether it was traced. `formal/tests/fixtures/l1-par-map.xsh` reproduces the called-procedure case: the serial stage records three writes, the parallel stage records none, and both return the first two ordered values. The checker now rejects direct writes in the callback to captured bindings, their fields and their elements (`check.par-map-captured-write`, SPEC 13.5). Three write paths still pass the checker and are lost on the parallel route: a called `proc` that assigns a top-level `var` (`via-proc=0` for three items in the verification binary); an `e"NAME" = ...` assignment in the callback, which leaves the variable unset after the stage; and a local `proc` declared inside the callback that assigns an outer binding.
- **Recommendation:** Reject the remaining paths at check time: a callback's call to a callable whose inferred writes reach state outside the callback, and environment assignment in a callback. Alternatively, specify one route-independent rule for them in SPEC 13.5. Keep a native test for each path.
- **Fix with:** lifecycle-concurrency, error-model
- **Effort:** M

### XSH-184 · P2 · A producer's environment is swapped wholesale around a suspension inside a scope

- **Area:** Runtime streams
- **Where:** `src/runtime/eval/lowered_run/indexed_run/producer.rs:301-325` (`pull_script_producer`); SPEC 10.4 ("A producer that yields from inside a `cd` or `env` scope keeps its own directory and environment between pulls")
- **Problem:** When a producer suspends inside a `cd` or `env` scope, `pull_script_producer` saves the producer's whole context and restores the consumer's context from the start of the pull. Plain `e"NAME" = ...` writes the producer made earlier in that pull, before entering the scope, are therefore lost to the consumer. When the producer later yields outside any scope, its saved context is dropped and the next pull runs it on the consumer's environment, so it loses its own earlier writes too. Reproducer (verification binary): a producer that sets `e"XSH_PA" = "before-scope"`, yields once inside `env ({XSH_PB: "scoped"}) { ... }`, then yields twice after it, over a loop that sets `e"XSH_PC" = "loop"`. It prints `before-scope/scoped`, then `before-scope/unset/unset`, then `unset/unset/loop`, and the loop sees `XSH_PA` unset throughout. Without a scope, the same writes are shared both ways. `formal/tests/fixtures/l1-producer-env.xsh` records this sequence and the consumer observations. The SPEC sentence does not say which of these is intended. unproven: whether this is a defect or intended isolation is an owner decision.
- **Recommendation:** Decide the rule and state it in SPEC 10.4. Either isolate only the scope's own overrides, sharing plain writes as in the unscoped case, or isolate the whole environment for the producer's lifetime. Make `pull_script_producer` follow it, and add the reproducer as a native test. The Lean model (`formal/README.md`, stream producers) shares the environment and records this as an open gap until the rule is decided.
- **Fix with:** lifecycle-concurrency, test-audit
- **Effort:** S

### XSH-180 · P2 · min and max have no specified order for NaN, ties and non-numeric items

- **Area:** Runtime ordering
- **Where:** `src/sema/sealed_operation.rs:25-40`; `src/sema/check/stream.rs:499-509`; `src/runtime/eval/lowered_ops.rs:833-848`; `src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs:1754-1762`; `crates/xsh-registry/src/reference.rs:601-608`; `tests/fixtures/runtime/generic-pipeline-aggregates.xsh:53-54`
- **Problem:** MinMaxItem admits Float and Duration, and before 2026-11-22 min/max compared them by type name, so `[9.0, 1.0] |> min()` gave `9.0`. Finite Float and Duration extrema now compare by value on the list, live, generic and reduce-by routes (`tests/xsh/numeric-aggregate-regressions.xsh`). NaN falls back to the sort comparator, and the SPEC still accepts List, Result and opaque items without defining their order; those are open decisions, not proved bugs.
- **Recommendation:** Decide NaN, tie and non-numeric ordering through the linked draft RFC, update the SPEC, and replace the remaining sort-comparator fallback in `compare_lowered_extrema` to match; do not silently turn the RFC into an approved language change.
- **Fix with:** typing-audit, make-invalid-states-unrepresentable, test-audit
- **Effort:** S
- **Design:** docs/rfc/XSH-180-min-max-ordering.md

### XSH-145 · P2 · Credential and module paths come from the caller's environment in shipped code

- **Area:** Security
- **Where:** `src/modules/user.rs:325-335`, `src/modules/group.rs:200-204`, `core/lib/auth.xsh:120-205` (`passwd_path`, `shadow_path`, `lookup_user`), `core/su.xsh`, `core/passwd.xsh`; current login guards at `core/lib/accounts.xsh:121-143`; `src/modules/linux/kernel.rs:350-352` (`XSH_MODULES_DIR`), `crates/xshi/src/interactive/builtin.rs:357` (`XSH_CORE_DIR`)
- **Problem:** Test seams remain in shipped code: `XSH_PASSWD_FILE`, `XSH_SHADOW_FILE` and `XSH_GROUP_FILE` redirect the native account mutations, while `su` and `passwd` still call the environment-sensitive auth helpers directly. Current `login` is partially hardened: `accounts.login_user` and `accounts.authenticate` force canonical passwd/shadow paths for effective UID 0, and unprivileged fixture identities must match the caller. `XSH_MODULES_DIR` still chooses kernel-module images and `XSH_CORE_DIR` chooses core scripts. These environment-selected authorities become a security boundary when a caller reaches a more privileged execution context. unproven: no current privileged path preserving an attacker's environment has been demonstrated; this review did not qualify external Laputa privilege configuration.
- **Recommendation:** Make production authority selection explicit and independent of an untrusted inherited environment at privileged boundaries. Move fixture roots behind a native test-host capability or a test-only feature excluded from shipped products, and audit the remaining auth, module and core-script callers. Retain the current login identity/canonical-path guards. Verify a shipped-feature build cannot redirect privileged credential reads or mutations through the fixture variables, without claiming a privilege escalation until a real privileged call path is demonstrated.
- **Fix with:** security, isolate-effects
- **Effort:** M

### XSH-146 · P2 · Shipped products compile native-tests seams through feature unification

- **Area:** Release
- **Where:** `Cargo.toml:67-69` (`default` includes `native-tests`), `dev/dist.xsh:88-100`, `docs/RELENG.md:19-21`, `src/runtime/eval/modules.rs:167-185`, `src/runtime/eval/lowered_run.rs:6633-6789` (`test` module dispatch), `src/runtime/eval.rs:259,8863`
- **Problem:** `cargo dev dist` builds `xsh`, `xsht` and `xshi` in one invocation, and `xsht/native-tests` enables `xsh/default`, which includes `native-tests`. Every shipped `xsh` therefore carries the native test machinery. The `test` module type-checks in ordinary scripts. `test.linux_fake` and `test.unix_fake` replace host `linux.*` and `unix.*` results. `test.mock` intercepts DNS and network results. Every DNS and network call appends a cloned request record to `evaluator.test_calls`, which is never cleared, for the evaluator's lifetime. `XSH_INTERNAL_TEST_FORCE_FRAMES` and `XSH_TEST_SMALL_EVAL_STACK` are honored. RELENG.md documents the unification but not these consequences.
- **Recommendation:** Build `xsh` for distribution without `native-tests` (a separate cargo invocation for `xsht`, or a `native-tests` feature that only `xsht` enables), and record only on an installed mock. Add a check that the dist `xsh` rejects a script using `test.`.
- **Fix with:** api-surface-and-module-boundaries
- **Effort:** M

### XSH-149 · P2 · The whole-project paranoid kernel recheck exceeds its time cap

- **Area:** Formal
- **Where:** `formal/scripts/check-fresh.py:19-20` (`WALL_LIMIT_SECONDS`, `PARANOID_WALL_LIMIT_SECONDS = 1620`), run log from the EFF-LATTICE integration on 2026-11-21
- **Problem:** After a clean build (298 jobs), the bundled lean4lean paranoid check of all 35,093 project declarations ran past its 1,620-second cap on one 4-CPU Lean slot and was terminated. Several decode certificates take 1-4.5 s each in lean4lean (`actual_scoped_facts_are_checked` 4.0 s, `scoped_fixture_accepted` 4.6 s). The plan requires every production proof root to pass a separate fresh kernel recheck, and G10 runs it over the whole root import, so the fixed gate cannot pass as the project grows. An exceeded cap is inconclusive evidence, not a pass.
- **Recommendation:** Recheck each closed root's import closure (`leanchecker --fresh <module>`) as the per-closure gate. Size the whole-project paranoid run from a measured baseline: give it its own budget within the plan's 45-minute clean-room cap, or run the paranoid kernel with more threads in a dedicated slot. Record the measured time per run, and treat certificate-heavy modules as the first optimization target.
- **Fix with:** make-it-fast
- **Effort:** M

### XSH-150 · P2 · `Path.with_ext` panics when the extension contains a separator

- **Area:** Runtime
- **Where:** `src/runtime/eval.rs:7865-7869` (`PathBuf::set_extension` called without validating the extension)
- **Problem:** `p"a".with_ext("/../..")` aborts the interpreter with exit 101 instead of returning a specified error. Rust's `set_extension` panics on an extension containing a path separator, and XSH passes the user string straight through.
- **Recommendation:** Reject an extension containing `/` (or NUL) with a SPEC-named error before calling `set_extension`, document it in the SPEC's Path methods, and add native tests for the separator case and for `p"...b".with_ext("")`.
- **Fix with:** none
- **Effort:** S

### XSH-151 · P2 · `.require` rejects every `stat: false` directory entry against a record schema

- **Area:** Runtime
- **Where:** Entry `.require`/`.require_exact`/`.require_stripped` for `fs.children(path, stat: false)`; define `type EntryName = {name: Str}` and call `entry.require(EntryName)` (and the other two policies)
- **Problem:** An unstat'd entry fails a non-empty named record schema under all three field policies with `expected Record, found Record`, while a stat'd entry exposes its record fields. Reproduced at 956a67a with the pinned-nightly verification-profile binary on native Linux GNU: keep, exact and stripped each returned status 3 for an available `name` field. SPEC 5.3 says the policy does not change which containers a slot admits. Use a named type alias in the reproducer; inline `.require({name: Str})` is not current type-argument syntax.
- **Recommendation:** Make an unstat'd entry expose its available fields as a record for schema checks (or reject with a message naming the missing fields), and add native tests for keep, exact and stripped against `stat: false` entries.
- **Fix with:** none
- **Effort:** S

### XSH-152 · P2 · Module values enter record schemas and later fail internal runtime guards

- **Area:** Runtime
- **Where:** `.require(T)` on a loaded module where `T` is a named record type: `type Fields = {x: Int}`; for the enum witness define `enum Kind: Str { One = "one" }` and `type Fields = {tag: Kind}`. Use `module.load(p"plug.xsh")?.require(Fields)?` with the corresponding export.
- **Problem:** At 956a67a, requiring a module with a plain Int export against a named record schema accepts the conversion, then using the result fails the driver's checked-type guard. A Str-backed enum field still fails internally with `lowered expression expected Record`. Both cases were reproduced with the pinned-nightly verification-profile binary on native Linux GNU. The SPEC provides module contracts (`module { ... }`) for checking module values, not a conversion into record representations. Neither an accepted schema conversion followed by an internal guard failure nor an internal enum-field diagnostic gives the caller an ordinary schema error. Use named record aliases; inline `.require({x: Int})` is invalid current syntax.
- **Recommendation:** Reject a module value against a record schema with a schema error naming the kinds, unless the SPEC is changed to allow it; add native tests for a plain field and an enum field.
- **Fix with:** none
- **Effort:** S

### XSH-160 · P2 · An importer's `is mod.Facet` never matches the facet of an imported error

- **Area:** Runtime
- **Where:** importer-side lowering of qualified facet patterns, `compact_qualified_pattern_facet` in `src/runtime/eval/lower/patterns.rs`; reproducers `bench/codemode/defects/d1-imported-facet/` and its `narrowed/`
- **Problem:** With `export error LibError = Down : Transient | Gone` in `lib.xsh`, an importer's `Err(is lib.Transient)` and `retry ... on (is lib.Transient)` never match `lib.LibError.Down`, while `error is Transient` inside `lib.xsh` is true for the same value wherever it was built. The checker accepts the pattern and even suggests `lib.Transient`, so a retry on a module's transient facet silently runs once. SPEC 4.10 says `is Facet` matches every variant that implements the facet. Found by the code mode experiment while writing service bindings; variant patterns (`lib.LibError.Down {..}`) do match.
- **Recommendation:** Match the facet name an imported pattern resolves to against the names the error value carries, and add a native test that an importer's facet pattern and `retry on (is mod.Facet)` match errors built in either module.
- **Fix with:** root-cause
- **Effort:** S

### XSH-164 · P2 · A top-level `Str +=` statement checks but fails at run time

- **Area:** Runtime
- **Where:** top-level compound assignment on `Str`; reproducer `bench/codemode/defects/d3-top-level-str-append/main.xsh`
- **Problem:** `var text = "a"` then `text += "b"` at the top level of a script passes `xsht check` and fails at run time with `+= assignment cannot combine Str and Str`; it takes the same top-level driver path as the `Duration` case that XSH-158 fixed by routing Int and Duration through the shared decisions; `Str` still has no arm there. The same statement inside a `for` body or a `proc` works, and `Int +=` works at top level. A model's program in the code mode revision run hit it.
- **Recommendation:** Find why the top-level lowering of `+=` rejects two `Str` operands, fix it, and add a native test for top-level `Str +=` beside the loop and proc forms.
- **Fix with:** root-cause
- **Effort:** S

### XSH-165 · P2 · `sort-by(desc: true)` reverses items with equal keys

- **Area:** Runtime
- **Where:** the `sort-by` stage with `desc: true`; reproducer `bench/codemode/defects/d4-descending-sort/main.xsh`
- **Problem:** `xsht api language:stream.sort-by` says the sort is stable, so equal keys keep their source order and the two-pass idiom gives a compound order. With `desc: true`, `[a:1, b:1, c:2, d:1] |> sort-by(desc: true) .n` yields `c d b a`: equal keys come out reversed, so sorting by a secondary key and then descending by a primary key silently gives the wrong secondary order. The held-out author of the code mode experiment found it.
- **Recommendation:** Make the descending order stable (reverse the comparison, not the result), or document that `desc: true` is not stable; add a native test with equal keys under `desc: true`.
- **Fix with:** root-cause
- **Effort:** S

### XSH-167 · P2 · Narrowing an optional field of a match-bound record fails IR verification

- **Area:** Lowering
- **Where:** indexed IR `full_ir_verification` (`compact.indexed-build`); reproducer `bench/codemode/defects/d6-narrowed-field-lowering/main.xsh`
- **Problem:** A record bound by `Ok(state) =>` whose `Str?` field is narrowed with `if state.transfer != null` and then used as `Str` (`sent += [state.transfer]`), followed by a later read of `sent`, type-checks and is then rejected with `err[compact.indexed-build]: ... checked optional field receiver has no non-null branch proof` at line 1, column 1. Without the later read, or with the narrowing on a `let` binding, it passes. Two held-out programs of the code mode experiment hit it.
- **Recommendation:** Carry the checker's narrowing proof for fields of pattern-bound records into the indexed IR, and add a native test of this shape; until then, report the source span of the field access instead of 1:1.
- **Fix with:** root-cause
- **Effort:** M

### XSH-169 · P2 · Explicit-server DNS timeouts become generic lookup failures on Unix

- **Area:** Networking
- **Where:** `crates/xsh-net/src/lib.rs:189-234,690-696` (`lookup_dns_server`, `dns_io_error`), `tests/runtime/modules.rs` (`LocalDnsServer`), `tests/xsh/stdlib/dns.xsh`
- **Problem:** Explicit-server `dns.lookup` sets a blocking UDP socket timeout, but `dns_io_error` recognizes only `ErrorKind::TimedOut`. [Rust documents](https://doc.rust-lang.org/stable/std/net/struct.UdpSocket.html#method.set_read_timeout) that Unix socket timeouts typically return `WouldBlock`, which this code maps to `dns-lookup`. Reproduced at 956a67a with the pinned-nightly verification-profile binary on native Linux GNU: a loopback UDP endpoint withholding its reply to `dns.lookup("fixture.test", "A", "127.0.0.1:<port>", 20ms)?` produced status 3 and `dns-lookup: Resource temporarily unavailable (os error 11)`. Callers cannot consistently identify deadline expiry. The test server handles both error kinds, but current transport tests exercise successful replies. Pinned Linux/musl and macOS qualification remain outstanding.
- **Recommendation:** Extend the existing loopback DNS fixture to receive a query and withhold its reply. Normalize `WouldBlock` and `TimedOut` to `dns-timeout` at the configured blocking-socket timeout boundary, without reclassifying unrelated resolver errors. Cover both error kinds directly and assert the native timeout kind and bounded completion; retain successful A/AAAA cases and bounded fixture cleanup. The regression needs no external DNS or new daemon. Qualify it through the applicable pinned Linux/musl gate and existing macOS path.
- **Fix with:** error-model, os-boundaries
- **Effort:** S

### XSH-173 · P2 · a hoisted call fails on any earlier top-level binding that has not run, read or not

- **Area:** Lowering and hoisting
- **Where:** src/runtime/eval/lower.rs:2874 (append_immutable_top_level_captures gets `reads: None` for every non-generic callable); src/runtime/eval/lowered_run.rs:7181; docs/templates/SPEC.md 3.1
- **Problem:** A non-generic `proc` or `pure` has no scheme body facts, so lowering captures every top-level binding declared before it, read or not. A hoisted call made before such a binding's statement has run then fails with `runtime.error` `unknown captured name` (exit 3): `print (first()); let b = 3; proc first() -> Int { 2 }` fails on `b`, and `let r = first(); proc first() -> Int { 2 }` fails on `r`. SPEC 3.1 makes a call fail only when it reads a binding before that binding's statement has run, and the Lean machine (Scripts.hoisted_scopes) follows the SPEC.
- **Recommendation:** Capture only the bindings `bodies.binding_reads.read_within` finds in the body and its default expressions (it includes assignment targets). A patch doing exactly that fixes both cases and passes declaration-hoisting, lowering-coverage, test-type-agreement and test-runtime, but changes slot counts in the exported IR: 13 test-lowering and test-runtime-unit cases fail, 10 of them the Lean-certified arithmetic and raw-IR documents (for example `excess` drops from 3 to 2 slots) and 3 verifier mutation tests that target a capture. Land it as a Lean-campaign slice that regenerates the documents with `write_lean_raw_ir_modules`, re-proves the artifact certificates, and updates those verifier tests. A runtime-only skip keyed on `use_location` is unsound: it misses tail and record-shorthand reads. Acceptance: tests/xsh/declaration-hoisting.xsh gains a case that prints 2, 2, 5 for `let r = first(); print (r); print (first()); let later = 3; print (later + first()); proc first() -> Int { 2 }`, the uninitialized-read and write-only capture cases still pass, and `lake build` plus check-inventory pass with the regenerated documents.
- **Fix with:** none
- **Effort:** M

### XSH-148 · P3 · The checker rejects validated callable returns inside collections

- **Area:** Checker
- **Where:** `src/sema/types.rs` (`matches_stored`, no callable arm; falls through to `matches_invariant`), SPEC `docs/templates/SPEC.md:1594-1602` (callable fit: a validated return may stand for its base at any depth), formal `Xsh.Validation.Core.Widen.Witness.callable_return_view_is_derived` (lane/lean-foundation)
- **Problem:** `let ys: List[pure() -> Path] = xs` with `xs: List[pure() -> RelPath]` fails with `check.type-mismatch`, while the direct binding `let f: pure() -> Path = name` for a `pure() -> RelPath` function is accepted. The SPEC lets a validated type stand where its base is written in a callable's return type at any depth, and the Lean model derives this view. Rust's stored-value matcher has no callable arm, so a callable nested in a collection, record or union loses the return widening the SPEC grants.
- **Recommendation:** Add a callable arm to `matches_stored` that applies the SPEC's callable fit (same kind, labels and parameter types; return compared by the stored view; effects may be fewer), with native tests for List, record and union nesting and a rejected case that changes a parameter type.
- **Fix with:** none
- **Effort:** S

### XSH-154 · P3 · A path write to a missing map key raises `missing-field` with a Debug-formatted key

- **Area:** Runtime
- **Where:** `src/runtime/eval/lowered_run/indexed_run/assignments.rs:149-165` (write path) versus `src/runtime/eval/lowered_ops.rs:2612` (read path, `map-missing`)
- **Problem:** Reading a missing key fails with `map-missing`, but a compound update or deeper path write through a missing key (`m["k"] += 1`, `m["k"].x = 1`) fails with `missing-field` and a message rendering the key as `Str("k")`. The two paths name the same condition differently and the write message leaks Rust Debug formatting.
- **Recommendation:** Raise the read path's kind and message for a missing key on the write path, state the kind in the SPEC's map-assignment rules, and add native tests for compound and nested writes through a missing key.
- **Fix with:** none
- **Effort:** S

### XSH-161 · P3 · `Path.relative_to` returns a path outside its base unchanged

- **Area:** Runtime
- **Where:** `Path.relative_to`; reproducer `bench/codemode/defects/d2-relative-to/main.xsh`
- **Problem:** `xsht api method:Path.relative_to` says "The result is an error when the path is not below the requested base", but the signature returns `Path` and `p"/etc/hosts".relative_to(p"/srv/app")` returns `/etc/hosts`, so a caller cannot tell a relative result from the unchanged input.
- **Recommendation:** Decide the contract: return `Result[Path]` and fail outside the base, or keep `Path` and state that an outside path is returned unchanged; align the registry text and add a native test for the outside case.
- **Fix with:** error-model
- **Effort:** S

### XSH-162 · P3 · `xsht api` misstates named arguments and omits the walk `.gitignore` default

- **Area:** Docs
- **Where:** `crates/xsht/src/api.rs:688` (`POSITIONAL_ONLY_NOTE` and its API-contract consumers); `fs.walk` and `fs.files` signatures/default rendering
- **Problem:** Many contracts say "Function arguments are positional-only; parameters marked `= default` may be omitted, but cannot be supplied as `name = value`", while `name: value` arguments such as `fs.walk(root, gitignore: false, hidden: true)` and `archive.create(..., collision: .Replace)` check and run, and some functions require them: `csv.encode(rows, true, ",", columns)` is `check.named-arg` for `headers`, `delimiter` and `columns`, and `path.rename(dest, true)` for `to`, so following the text literally is rejected. In the held-out runs this was the most frequent diagnostic, in 18 programs under both XSH contexts. `fs.walk` and `fs.files` obey `.gitignore` files by default, also outside a Git checkout, but the signature shows only `gitignore: Bool = default`. Agents in the code mode experiment had to learn both from failures.
- **Recommendation:** State which spellings are accepted (`name: value` yes, `name = value` no) and print each default's value in signatures, starting with `gitignore`.
- **Fix with:** none
- **Effort:** S

### XSH-163 · P3 · The tour and core API omit the redundant-parenthesis hard error

- **Area:** Checker
- **Where:** `check.redundant-parens`; `docs/templates/user-tour.md`; `xsht api language:core`; existing guidance in `docs/templates/agent-context.md:25` and `docs/templates/getting-started.md:65`
- **Problem:** Parentheses that do not change parsing, such as `x += (a + b)` or `and (id not in seen)`, are check errors. The code mode development runs recorded 8 of 24 failed programs from this rule. The compact agent context and getting-started guide now explain the hard error, but the tour and `xsht api language:core` still omit it. The earlier claim that no guide mentions the rule is no longer current.
- **Recommendation:** Document the current hard-error rule in the generated tour source and `language:core`, with accepted and rejected examples. Any later proposal to change it into a lint should be a separate language-policy decision; do not silently change checker behavior while filling this documentation gap.
- **Fix with:** none
- **Effort:** S

### XSH-168 · P3 · `cargo dev check` runs formatter checks the tree is not meant to pass

- **Area:** Tooling
- **Where:** `dev/build.xsh:166` (`check-rustfmt`: `cargo fmt --all -- --check`), the later `xsht fmt --check` stage of `cargo dev check`
- **Problem:** `cargo dev check` fails on every checkout: `cargo fmt --check` reports diffs in `build.rs`, `crates/` and `tests/`, and `xsht fmt --check` reports 1,045 unformatted files. The repository does not run formatters (AGENTS.md), and no gate or CI path runs these stages, so the command is permanently red and a newcomer following it cannot tell a real failure from the expected one. The cold Ubuntu 24.04 capsule run of 2026-11-21 stopped there after build and clippy passed.
- **Recommendation:** Decide whether formatting is checked. If not, drop the two formatter stages from `cargo dev check` and keep clippy, `xsht check`, `xsht lint` and the remaining contracts; if it is, format once and keep the stages. Until then docs/DEVELOPMENT.md leaves `cargo dev check` out of the newcomer path.
- **Fix with:** none
- **Effort:** S

### XSH-170 · P3 · named, spread and splice arguments to a callable value's .call are rejected although SPEC 6.3 allows them

- **Area:** Checker and calls
- **Where:** src/sema (call checking for callable values); docs/templates/SPEC.md 6.3
- **Problem:** SPEC 6.3 lets a record spread target a local that holds a callable value and allows named arguments in calls. For a callable value, the verification binary rejects f.call(...pair)? with check.named-spread (named argument spreading requires a statically checked callable signature), and the formal foundation lane found a named argument to a dynamic .call fails to compile (compact.indexed-build) and an @ splice there is check.call-splice. The Lean machine follows the SPEC (root call_arguments), so its named .call witness has no binary match.
- **Recommendation:** Decide whether the SPEC or the checker is right. Either type a callable value's .call against the referenced declaration's signature so named, spread and splice arguments work, or narrow SPEC 6.3 and 4.x to say those forms need a statically known callee. Acceptance: a native test pins the chosen behaviour for named, spread and splice arguments through .call, and the call_arguments root's boundary row matches it.
- **Fix with:** checker or SPEC
- **Effort:** M

### XSH-172 · P3 · lane/namespace-native always skips its 13 lsipc and lslocks tests

- **Area:** Gates
- **Where:** dev/gate_lane.xsh namespace_tests; core/tests/support/util_linux.xsh:187 (require_private_namespace); dev/util-linux/check.py:518
- **Problem:** namespace_tests lists 13 core/tests/test-lsipc.xsh and test-lslocks.xsh tests, but they run only when XSH_UTIL_LINUX_PRIVATE=1 and the parent IPC, mount and PID namespace identities are exported, and only dev/util-linux/check.py sets those. In lane/namespace-native each one reports "requires private util-linux IPC, mount and PID namespaces", so the lane ends incomplete (84 passed, 13 skipped, exit 2) on every run, including master 3155adf2a before the capsule merge, and the tests run under no gate.
- **Recommendation:** Have the namespace lane enter the private IPC, mount and PID namespaces the way dev/util-linux/check.py does and export the marker and parent identities, or move these tests to the group that names the util-linux harness as their owner. Acceptance: `cargo dev gate --case lane/namespace-native` exits 0 with the 13 tests passing, or the tests leave namespace_tests and the lane exits 0.
- **Fix with:** none
- **Effort:** S

### XSH-174 · P3 · "executable not found" does not name the program

- **Area:** Process API errors
- **Where:** src/runtime/process.rs:4202, 4339, 4750 (RunError "not-found"); core/env.xsh:368 matches the message text
- **Problem:** process.which, run and spawn report `not-found: executable not found` without the program name. A Laputa capsule run on a fresh Ubuntu host failed five PM tests with that message alone; finding the missing tools (cc, readelf, python3) took reading each test and recipe.
- **Recommendation:** Carry the requested program in the error, for example `executable not found: readelf`, and match on the error kind rather than the message in core/env.xsh. Acceptance: a native test shows process.which("no-such-tool") and run of a missing program both name it, and the env.xsh fallback still works.
- **Fix with:** error-model
- **Effort:** S

### XSH-175 · P3 · net.download and net.request send no User-Agent

- **Area:** Network API
- **Where:** `crates/xsh-net/src/lib.rs:1125-1141` (`h12_request`), `:1699-1706` (`request_builder`), `:1603-1636` (`run_download`); `src/runtime/eval/modules/net.rs:86-131,265-286`
- **Problem:** The native net API sends no User-Agent unless the caller supplies one. At 956a67a, two real loopback net.download requests from the pinned-nightly verification-profile binary on native Linux GNU contained Host but no User-Agent; request and batch download use the same header builder by source inspection. The existing report records git.kernel.org returning 403 without an agent and Laputa adding its own header in 601e4597; that external service behavior was not requalified in this review. The core curl/wget applets already add their own agents.
- **Recommendation:** Send a default `User-Agent: xsh/<version>` that a caller's header replaces, and document it in the net reference. Acceptance: a native test against a local server sees the default agent, and a caller-supplied User-Agent replaces it rather than duplicating it.
- **Fix with:** none
- **Effort:** S

### XSH-176 · P3 · prefer-item-shorthand suggests a rewrite that breaks inferred variants

- **Area:** Lints
- **Where:** crates/xsht lint.prefer-item-shorthand; Laputa xinfra/lib/infra/mutate.xsh:164, 169 and transport.xsh:227, 230, 233, 336
- **Problem:** In a `?? { |error| ... }` handler whose body builds inferred variants such as `.Stage(...)` or `.Refused(error.message)`, the lint reports that `error` is only read through its fields and suggests leaving the parameter implicit. Doing so makes the checker read `.Stage`, `.NotSubmitted(...)` and `.Refused(...)` as members of the item, failing with check.unknown-field and check.unknown-method. Laputa excludes the rule for those two files.
- **Recommendation:** Do not report a handler whose body uses a leading-dot inferred variant, or make the implicit-item form keep leading-dot variants as variants. Acceptance: a native lint test with such a handler reports nothing, and its suggested rewrite of an ordinary handler still checks.
- **Fix with:** none
- **Effort:** S

### XSH-166 · P4 · A user type named like a built-in record cascades into a misleading mismatch

- **Area:** Checker
- **Where:** `check.duplicate-name` for types; reproducer `bench/codemode/defects/d5-builtin-type-name/main.xsh`
- **Problem:** `type Group = {name: Str, size: Int}` is correctly rejected with `check.duplicate-name` ("type name conflicts with a built-in type"), but every later use is then checked against the built-in `Group` record and reports a second `check.type-mismatch` ("expected {gid: Int, members: List[Str], name: Str}, found {name: Str, size: Int}") that points away from the real cause.
- **Recommendation:** After a duplicate type name, check its uses against the user's declaration (or suppress follow-on mismatches) and name the built-in record in the first diagnostic.
- **Fix with:** none
- **Effort:** S

