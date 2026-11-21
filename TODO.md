Prefix: XSH
Next: 148

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

### XSH-147 · P2 · xsht lint misses unused constants and unreferenced exports

- **Area:** Tooling
- **Where:** `crates/xsht/src/cli/lint.rs` (`lint.unused-callable`, `lint.unused-type`, `lint.unused-local`), Laputa `guest/asterinas/device/entry.xsh:2` (`const case_dir`), Laputa `xinfra/lib/infra/mutate.xsh:15` (`const nul`), Laputa `pm/types.xsh:177,414,669` and 15 other exports with no reference
- **Problem:** `xsht lint` flags unused private callables, types and locals, but not an unused private `const`: `case_dir` and `nul` in Laputa produce no diagnostic. It has no project-wide check for exports: an exported proc, type or const that no module in the project imports and uses (Laputa has 18 with no reference anywhere, and 102 used only inside their own file) is invisible to it, because `lint.unused-callable` stops at unexported callables. A script found these instead. The owner wants xsht's dead-code detection to be correct and thorough.
- **Recommendation:** Add `lint.unused-const` for private constants. Add a workspace-level unused-export lint that resolves `use ... as` aliases across every module under the project's module and test roots. It reports exports referenced nowhere, separately notes exports referenced only by tests or only inside their own module, and exempts modules declared as a public API in `xsht-config.ini`, such as stdlib and published libraries. Prove it against fixtures that reproduce Laputa's 18 dead exports, aliased imports, same-named exports in two modules, and re-export chains.
- **Fix with:** design-cli, dead-code-archaeology
- **Effort:** M

