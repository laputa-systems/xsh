Prefix: XSH
Next: 173

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
### XSH-144 · P1 · user and group mutations leave /etc/shadow readable under the umask

- **Area:** Security
- **Where:** `src/modules/user.rs:345` (`write_atomic`), `src/modules/user.rs:106-128,154-158` (`user.add`, `user.remove`), `src/modules/group.rs:214`; correct helper `src/modules/fs.rs:2849-2865`
- **Problem:** `user.add` and `user.remove` rewrite `/etc/passwd` and `/etc/shadow`, and `group.add` and `group.remove` rewrite the group file, through a private `write_atomic` that creates the replacement with `fs::write` and renames it. `fs::write` creates the file with mode 0666 minus the umask, so under root's usual umask 022 a rewritten `/etc/shadow` becomes 0644 and every local user can read the password hashes. The shared `fs::write_atomic` in `src/modules/fs.rs` already keeps the existing file's mode; the private copies duplicate it without that rule. The private temp name `<path>.<pid>.tmp` is also predictable.
- **Recommendation:** Delete the private `write_atomic` copies in user.rs and group.rs and call `fs::write_atomic` (or the rooted variant), which preserves the destination mode. Add a native test that `user.add` on a 0640 shadow fixture leaves it 0640 under umask 022.
- **Fix with:** deduplicate, os-boundaries
- **Effort:** S

### XSH-145 · P2 · Credential and module paths come from the caller's environment in shipped code

- **Area:** Security
- **Where:** `src/modules/user.rs:325-335`, `src/modules/group.rs:200-204`, `core/lib/auth.xsh:120-205` (`passwd_path`, `shadow_path`, `lookup_user`), `core/lib/accounts.xsh:87-88,141,157,182`, `src/modules/linux/kernel.rs:350-352` (`XSH_MODULES_DIR`), `crates/xshi/src/interactive/builtin.rs:357` (`XSH_CORE_DIR`)
- **Problem:** Test seams are compiled into shipped products without a gate. `XSH_PASSWD_FILE`, `XSH_SHADOW_FILE` and `XSH_GROUP_FILE` redirect the account files that `user.*`, `group.*` and the shipped `su`, `login` and `passwd` applets read and rewrite. `lookup_user` and `authenticate` then trust the redirected files, so any applet that runs with more privilege than its caller can be pointed at a caller-written passwd and shadow file. `XSH_MODULES_DIR` chooses which `.ko` image `modprobe` loads, and `XSH_CORE_DIR` chooses which core scripts `xshi` executes. Only tests set them, and none is documented. Today the escalation is latent: Laputa marks the interpreted `su` and `passwd` setuid, and Linux ignores that (LAP ticket filed alongside). Any future privileged path that honors it makes this exploitable. unproven: no current privileged path that preserves the caller's environment has been demonstrated.
- **Recommendation:** Remove the environment overrides from production code. Give tests a fixture root through the native test host, or compile the overrides only under `cfg(test)` and a test-only feature that shipped products never enable. Add a test that a shipped-feature build ignores `XSH_SHADOW_FILE`.
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
- **Where:** Entry `.require`/`.require_exact`/`.require_stripped` for entries from `fs.children(path, stat: false)`; reproduction: `fs.children(p"tree", stat: false)` then `entry.require({name: Str})`
- **Problem:** An unstat'd entry fails any non-empty record schema under all three field policies with `expected Record, found Record`, while a stat'd entry converts (`.require({kind: Kind})` succeeds). SPEC 5.3 says the policy does not change which containers a slot admits, and the message names the same kind on both sides.
- **Recommendation:** Make an unstat'd entry expose its available fields as a record for schema checks (or reject with a message naming the missing fields), and add native tests for keep, exact and stripped against `stat: false` entries.
- **Fix with:** none
- **Effort:** S

### XSH-152 · P2 · Module values are accepted into record schemas and fail internally on enum fields

- **Area:** Runtime
- **Where:** `.require(T)` on a loaded module value where `T` is a record type; e.g. `module.load(p"plug.xsh")?.require({tag: Kind})`
- **Problem:** A module value checked against a record schema is accepted (`.require({x: Int})` succeeds), which the SPEC does not provide for: module values are checked against `module { ... }` contracts. With a Str-backed enum field the check fails with the internal message `lowered expression expected Record` instead of a schema error.
- **Recommendation:** Reject a module value against a record schema with a schema error naming the kinds, unless the SPEC is changed to allow it; add native tests for a plain field and an enum field.
- **Fix with:** none
- **Effort:** S

### XSH-153 · P2 · A module contract with a `pure` entry fails with `compact.indexed-build`

- **Area:** Runtime
- **Where:** `type PureG = module { export pure g() -> Int }` then `module.load(p"plug3.xsh")?.require(PureG)` in the verification binary
- **Problem:** Requiring a module against a contract whose entry is a `pure` function fails with the internal diagnostic `compact.indexed-build` rather than accepting the module or reporting a contract violation.
- **Recommendation:** Find why the indexed build rejects the pure contract entry, fix it, and add a native test that a matching pure export is accepted and a proc export is reported as mismatched.
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
- **Problem:** Explicit-server `dns.lookup` sets a blocking UDP socket timeout, but `dns_io_error` recognizes only `ErrorKind::TimedOut`. [Rust documents](https://doc.rust-lang.org/stable/std/net/struct.UdpSocket.html#method.set_read_timeout) that Unix socket timeouts typically return `WouldBlock`, which this code maps to `dns-lookup`, so callers cannot consistently identify a DNS deadline expiry. The test server handles both error kinds, but the DNS transport tests only exercise successful replies. This is a source finding; a native XSH reproducer is still required.
- **Recommendation:** Extend the existing loopback DNS fixture to receive a query and withhold its reply, then reproduce the error through native `dns.lookup` on the pinned Linux/musl target. Normalize `WouldBlock` and `TimedOut` to `dns-timeout` at the configured blocking-socket timeout boundary, without reclassifying unrelated resolver errors. Cover both error kinds directly and assert the native timeout kind and a bounded completion time; retain the existing successful A/AAAA cases and bounded fixture cleanup. The regression must need no external DNS or new daemon. Run the applicable Linux gate and the existing macOS qualification path.
- **Fix with:** error-model, os-boundaries
- **Effort:** S

### XSH-172 · P3 · lane/namespace-native always skips its 13 lsipc and lslocks tests

- **Area:** Gates
- **Where:** dev/gate_lane.xsh namespace_tests; core/tests/support/util_linux.xsh:187 (require_private_namespace); dev/util-linux/check.py:518
- **Problem:** namespace_tests lists 13 core/tests/test-lsipc.xsh and test-lslocks.xsh tests, but they run only when XSH_UTIL_LINUX_PRIVATE=1 and the parent IPC, mount and PID namespace identities are exported, and only dev/util-linux/check.py sets those. In lane/namespace-native each one reports "requires private util-linux IPC, mount and PID namespaces", so the lane ends incomplete (84 passed, 13 skipped, exit 2) on every run, including master 3155adf2a before the capsule merge, and the tests run under no gate.
- **Recommendation:** Have the namespace lane enter the private IPC, mount and PID namespaces the way dev/util-linux/check.py does and export the marker and parent identities, or move these tests to the group that names the util-linux harness as their owner. Acceptance: `cargo dev gate --case lane/namespace-native` exits 0 with the 13 tests passing, or the tests leave namespace_tests and the lane exits 0.
- **Fix with:** none
- **Effort:** S

### XSH-148 · P3 · The checker rejects validated callable returns inside collections

- **Area:** Checker
- **Where:** `src/sema/types.rs` (`matches_stored`, no callable arm; falls through to `matches_invariant`), SPEC `docs/templates/SPEC.md:1594-1602` (callable fit: a validated return may stand for its base at any depth), formal `Xsh.Validation.Core.Widen.Witness.callable_return_view_is_derived` (lane/lean-foundation)
- **Problem:** `let ys: List[pure() -> Path] = xs` with `xs: List[pure() -> RelPath]` fails with `check.type-mismatch`, while the direct binding `let f: pure() -> Path = name` for a `pure() -> RelPath` function is accepted. The SPEC lets a validated type stand where its base is written in a callable's return type at any depth, and the Lean model derives this view. Rust's stored-value matcher has no callable arm, so a callable nested in a collection, record or union loses the return widening the SPEC grants.
- **Recommendation:** Add a callable arm to `matches_stored` that applies the SPEC's callable fit (same kind, labels and parameter types; return compared by the stored view; effects may be fewer), with native tests for List, record and union nesting and a rejected case that changes a parameter type.
- **Fix with:** none
- **Effort:** S

### XSH-154 · P3 · A path write to a missing map key raises `missing-field` with a Debug-formatted key

- **Area:** Runtime
- **Where:** `src/runtime/eval/assignments.rs:149,162` (write path) versus `src/runtime/eval/lowered_ops.rs:2612` (read path, `map-missing`)
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
- **Where:** registry contract text for module functions; `fs.walk`, `fs.files`
- **Problem:** Many contracts say "Function arguments are positional-only; parameters marked `= default` may be omitted, but cannot be supplied as `name = value`", while `name: value` arguments such as `fs.walk(root, gitignore: false, hidden: true)` and `archive.create(..., collision: .Replace)` check and run, and some functions require them: `csv.encode(rows, true, ",", columns)` is `check.named-arg` for `headers`, `delimiter` and `columns`, and `path.rename(dest, true)` for `to`, so following the text literally is rejected. In the held-out runs this was the most frequent diagnostic, in 18 programs under both XSH contexts. `fs.walk` and `fs.files` obey `.gitignore` files by default, also outside a Git checkout, but the signature shows only `gitignore: Bool = default`. Agents in the code mode experiment had to learn both from failures.
- **Recommendation:** State which spellings are accepted (`name: value` yes, `name = value` no) and print each default's value in signatures, starting with `gitignore`.
- **Fix with:** none
- **Effort:** S

### XSH-163 · P3 · `check.redundant-parens` is a hard error that no guide mentions

- **Area:** Checker
- **Where:** `check.redundant-parens`; evidence `bench/codemode/results/dev-initial-taxonomy.json`
- **Problem:** Parentheses that do not change parsing, such as `x += (a + b)` or `and (id not in seen)`, are check errors. In the code mode development runs this one rule caused 8 of the 24 failed programs under both the current documentation and the compact guide; the tour and `xsht api language:core` do not mention it.
- **Recommendation:** Decide whether the rule should stay an error or become a lint with an autofix; until then, state it in the tour and in `language:core`.
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

### XSH-166 · P4 · A user type named like a built-in record cascades into a misleading mismatch

- **Area:** Checker
- **Where:** `check.duplicate-name` for types; reproducer `bench/codemode/defects/d5-builtin-type-name/main.xsh`
- **Problem:** `type Group = {name: Str, size: Int}` is correctly rejected with `check.duplicate-name` ("type name conflicts with a built-in type"), but every later use is then checked against the built-in `Group` record and reports a second `check.type-mismatch` ("expected {gid: Int, members: List[Str], name: Str}, found {name: Str, size: Int}") that points away from the real cause.
- **Recommendation:** After a duplicate type name, check its uses against the user's declaration (or suppress follow-on mismatches) and name the built-in record in the first diagnostic.
- **Fix with:** none
- **Effort:** S

