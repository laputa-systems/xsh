# Consolidation restart handoff

Status: active and incomplete, updated 2026-10-16. The owner authorized wider,
staggered inference work in isolated worktrees while consolidation acceptance
continues; this supersedes the earlier sequence that held lowerer work until
acceptance closed. No inference implementation is on main. Consolidation has no
functional close or performance credit. Lean and Phase F remain paused.

## Current resume

The owner resumed the typing consolidation campaign after the documentation
refresh at `bfffd3af5e0002668e685e1711d7e6d9ee90ad51`. Canonical source
`e89e35f29` includes the accepted `df` repair from `507`. Integrated
`xsht check` and lint pass on four files. Fake runner lane `5f21` passes 11
tests and its plan test passes 1/1. Shell syntax check and real workload test
`2ab` (1/1) pass.
The canonical batch on `e89e35f29` ended with exit 3 and 13 native failures.
It left source clean before and after. Cleanup resolved, with no OOM or swap;
the container is absent. No canonical suffix or performance close was reached.
Follow-up repairs on main address all 13 failure causes: hostname `63734367c`,
lifecycle `fff7fed23` and `610a22965`, and OS `14adcd5e6`. Their exact focused
tests pass (3, 8, and 2 respectively). The integrated `14ad` run passed 15/15
exact tests and `xsht check`. The corrected four-caller gate passed 45/45. Its
two earlier `docker.host_link_tools` errors came from missing mold and common
Git mounts; the corrected runner needed no source patch. Cache fix `f3afacb63`
is on main as `af7c55374`. Main is now `3bae6555c`; route-runner fixes `36d58`
and `70192` are integrated. Seven integrated runner tests and 63 route self-tests
pass. The private `gates` debug-default lint warning is removed without changing
its `Bool` contract. Source check, lint, and exact plan are running. The
canonical batch has not rerun and remains held for W9 source-closure tooling
review. The preceding canonical run at
`8d0d6362418a6ab1b58582cf5574a7e0f4ac3542` ended with the failure detailed
below. Historical gate helpers require fresh paths and source qualification.

Acceptance launch, structural/semantic review, loop selection, typing witness
readiness, and historical AWK evidence were inventoried independently. No Lean
work is authorized by this resume.

### Resume baseline: fixtures, historical AWK cause, and inference overlap

- Source `577bebfb9` integrates reviewed curl repair `07dd3bfbb`, AWK fixture
  `66945cbdd`, and tail fixture `11eac7c02`. Five exact checks on the integrated
  scripts pass with frozen `0d6` verification products: three curl cases,
  AWK shard 001, and tail wait. [Raw results](evidence/fixture-fixes-577b/driver.stdout)
  retain each status. The tail lane separately passes as root and uid 1000.
- The `0d6` canonical batch passed 305 process tests with four ignored and
  741 debug unit tests with three ignored. XSHT integration stopped at the
  AWK desugar difference: 115 passed, one failed. No normal full native suite
  ran. [The failed batch](evidence/canonical-0d6/canonical-replay.json) is retained.
  Its outer product lookup used the old target path; the
  [actual-product sidecar](evidence/canonical-0d6/actual-products.json) identifies
  the binaries from the mounted `.work` target. No timing credit is claimed.
- [Original-source release binaries reproduce the AWK cause](evidence/awk-cross-file-0ea/README.md):
  written AWK shard 001 passes, written curl creates `/nonexistent/dir`, and
  freshly desugared AWK then succeeds at a redirect expected to fail. The
  historical harness completes all written reports before transformed reports.
  The earlier twelve-shard replay omitted curl. Missing historical child bytes
  remain missing; this bounded reproducer establishes the cross-file mechanism.
- Curl now enforces its existing `--create-dirs` contract before the downloader
  can create parents. AWK's missing-parent case uses its private `@DIR@` tree.
  Tail's root unreadable fixture drops DAC override capabilities and gives the
  inner timeout time to return. No public API changes are introduced.
- Reviewed Clippy cleanup is integrated at `c1c6e4c87`. The required all-target,
  all-feature lint gate passes. Forty focused debug tests and 23 native tests
  pass. Eight paired repository/Laputa check and lint invocations exit zero.
  Their diagnostic bytes match after excluding only the final timing summary.
  [The retained batch](evidence/clippy-4d7b/README.md) includes the two failed
  lint iterations and the corrected fixture-mount run. These focused checks
  do not close canonical acceptance.
- Current structural measures retain all four hard zeros, 152 instruction
  variants, 17 patterns, 46 stages, one predicate, and two storage mappings.
  Production source is 268,504 lines, 25 above the qualified baseline.
  The current eight-root trace records eight requests, one prepare, seven
  reuses, one check, and seven check hits. These are cache events.
  The synthetic compiler probe requires 35 XSH edits and 19 XSHT arms.
  Its diagnostic commit `18c9a6c10` must never merge.
- Five W9 harness builds qualified on `0d6`. Fresh capture failed in the e240
  reference worker on index 1655, `tests/xsh/system-report.xsh`; candidate capture
  did not start. Both an exact range replay and a single-file replay hit the
  unchanged 1 GiB cgroup limit with one OOM kill and no swap. The
  [marker probe](evidence/w9-memory-format/README.md) localizes the kill to full
  pretty-Debug formatting after
  the checker returns. A compact-format diagnostic completes this input within
  the same cap. This earlier capture remains unqualified. The latest checkpoint
  below records the subsequent bounded equivalence proof and repair.
  [OOM evidence](evidence/w9-reference-137/single-1655/cgroup-oom-observed.txt)
  retains the bound and kill. Partial ranges have no parity credit.
- Capability branch `campaign/typing-capability-witnesses`, commit `b6d4a6d1a`,
  records six native witnesses: two existing rejection controls pass and four
  inferred capability cases fail at `parse.expected-param-type`. Joint and
  independent importing roots are included. [Raw baseline](evidence/typing-witnesses-b6d4/test.stdout).
- Solver baseline `6f53397b8` reproduces the 400-variable alias failure:
  `Err(TypeDepth)` instead of `Ok(Int)`. Candidate `3b7c033` fixes that case but
  misclassifies an existing foreign-variable test; its failed output is retained.
  Corrected `b556b5656` on `campaign/typing-unification` builds and passes the
  exact alias case and all eighteen constraint tests. Independent source review
  finds no remaining blocker. [Red baseline](evidence/typing-alias-red-6f53/constraints-module.stdout),
  [failed candidate](evidence/typing-alias-failed-3b7c/constraints-module.stdout),
  and [green candidate](evidence/typing-alias-green-b556/constraints-module.stdout)
  remain separate. This prerequisite is isolated, not merged or a speedup claim.
- The reviewed [first inference slice](designs/inference.md) is integrated as
  a design only. One implementation owner works in
  `campaign/typing-rank1-integration`, starting at `63add2b2d`. That branch
  combines the reviewed alias repair, Clippy changes, and red witnesses.
  It owns pure identity/projection schemes through parser, checker, imports,
  immutable body facts, lowering, and verification. Main has no inference
  implementation or red capability tests. Lean remains paused.

### Latest canonical and solver checkpoints

- The canonical batch on source `e89e35f29` ended with status
  `failed-or-incomplete` and exit 3. It passed integration 223, auxiliary 23,
  symbol 1, PTY 58 (3 ignored), debug 747 (3 ignored), XSHT 116 (5 filtered),
  soundness 14, privileged 82 (4 ignored and 4 filtered), and linuxpriv 10.
  Native tests finished with 13,891 passed, 13 failed, and 78 skipped out of
  13,982 selected. Source was clean before and after; cleanup resolved, with no
  OOM or swap, and the container is absent. No suffix stages or performance
  close were reached. Strict Clippy on root source `e89` passed with empty
  output; its receipt is under `.work/tmp/clippy-e89e35f295`. The retained
  [canonical receipt](evidence/canonical-e89/canonical-replay.json),
  [stdout](evidence/canonical-e89/stdout.log),
  [stderr](evidence/canonical-e89/stderr.log),
  [container resources](evidence/canonical-e89/container-resources.json), and
  [bootstrap receipt](evidence/canonical-e89/bootstrap-receipt.json) preserve
  the failure and source qualification.
- Follow-up repairs on main address all 13 native failure causes. Hostname
  commit `63734367c` passes three exact tests. Lifecycle commits `fff7fed23`
  and `610a22965` pass eight exact tests. OS commit `14adcd5e6` fixes cat's
  proc-fd handling and the XDG cache path; both exact tests pass. The integrated
  `14ad` run passed 15/15 exact tests and `xsht check`. The corrected four-caller
  gate passed 45/45 with no failures or skips. Its two earlier
  `docker.host_link_tools` errors came from missing mold and common Git mounts;
  the corrected runner passed without a source patch. No full canonical batch
  has rerun.
- Route-pin refresh `f19` preserves all 92 child-byte outputs and passes 62
  self-tests. Its 184 actual child outputs have not run. Gate map `7d087dd`
  scopes the native workflow to three callers, four native files, and the
  XSH/XSHT products. Cache fix `f3afacb63` is on main as `af7c55374`; its root
  and non-root cases, check, and lint pass. These checks claim no performance
  credit.
- Main is at `3bae6555c`. Route-runner fixes `36d58` and `70192` preserve
  explicit product attestations and use the canonical `xsht` path in the
  collector. Seven integrated runner tests and 63 route self-tests pass. The
  lint warning for the private `gates` debug default is removed; its `Bool`
  contract is unchanged. Source check, lint, and exact plan
  are running. The canonical batch remains held for W9 source-closure tooling
  review, not inference.
- The 13 native failures fall into three groups:
  - Hostname (3): `core/tests/test-bb-hostname.xsh::test_bb_hostname_hostname_i_works_89c47685`,
    `core/tests/test-hostname.xsh::test_hostname_ip_address_lists_distinct_addresses`,
    and `core/tests/test-uu-hostname.xsh::test_uu_hostname_hostname_ip`. The
    kernel hostname is absent from `/etc/hosts`; DNS returned `EAI_AGAIN`.
  - Lifecycle (8), all in `dev/tests/test-lifecycle.xsh`:
    `test_build_failure_stops_at_the_cargo_boundary`,
    `test_check_lint_guards_the_exact_budget_test_before_later_checks`,
    `test_check_lint_keeps_the_test_error_when_the_postcheck_fails`,
    `test_check_lint_rejects_a_successful_test_when_the_postcheck_fails`,
    `test_check_lint_skips_the_test_when_the_host_is_busy`,
    `test_codesign_failure_stops_darwin_installation`,
    `test_docker_container_failure_runs_target_ownership_cleanup`, and
    `test_lint_fix_rebuilds_the_debug_xsh_binary`. The build-boundary test
    attempted a real rustup download and failed on DNS; the other seven hit
    missing fake flock or runner tools.
  - Host filesystem/socket (2):
    `core/tests/test-uu-cat.xsh::test_uu_cat_domain_socket` and
    `tests/xsh/stdlib/fs.xsh::test_fs_tree_metadata_install_and_locking`.
    The socket reports `invalid-argument: unix socket name is too long or
    contains NUL`; the filesystem test reports `fs-dir: Permission denied`.
    Both were under investigation at `e89`. Full native used
    `/work/.work/tmp/native-tests-1000-1000/tmp`; the socket path may exceed
    108 bytes, so path length remained plausible before the later repair.
- The [8d acceptance follow-up](evidence/acceptance-follow-up-8d/README.md)
  records the terminal canonical batch on clean source
  `8d0d6362418a6ab1b58582cf5574a7e0f4ac3542`: exit 3 at XSHT after integration
  223, auxiliary 23, symbol 1, PTY 58 passed (3 ignored), debug 747 passed
  (3 ignored), and XSHT 115 passed, 1 failed, 5 filtered. The desugar failure
  is original `core/tests/test-df.xsh::test_df_reports_filesystem_capacity_counters`:
  observed 192512 instead of 65536; its transformed counterpart passes. No
  later stages, full native suite, or canonical suffix stages ran. Source
  checks match before and after; no OOM or swap event occurred. Cleanup
  resolved, and the container is absent. The `8d` bootstrap produced
  source-matched `xsh` prefix
  `d058`; the tracked driver enforces resource, command, swap, cleanup,
  read-only source, and shared-lock checks. No release, timing, or
  functional-close credit is claimed.
- Integrated queue `b6` passes 11 tests and its four-file check/lint. Stable
  copied-artifact rerun of lint changes `a22` passes check with 1,526 files,
  lint with zero findings and 113 notes, metrics, and docs/tour tests (3 pass).
  The original shared `xsh` changed from `a911` to `d058` during bootstrap;
  qualification is from the immutable copied artifacts, not the changing shared
  product. Detailed receipts are in disposable
  `.work/tmp/consolidation-main-suffix-a22-stable`.
  Replay launcher/bridge `73c`/`334` are integrated. The SGID private-scratch
  repair passes three Python regression scripts: `test_repository_tmp.py`,
  `test_profile_cost_selection.py`, and `test_profile_cleanup_signal.py`.
  Lint cleanup `f958` passes its focused gate with zero findings and notes and
  three negative preflight cases.
- Driver checkpoints `4c`/`a0`/`9a` pass 21 tests and bootstrap `9a`; its replay
  launch raised `NameError` before Docker and ran no suites. The separate `8d`
  driver fix passes 22 tests as root. This launcher failure is not part of the
  canonical batch.
- Cargo runner lock handoff `fcf` and selector fix `e89` are integrated. The
  integrated check and lint each pass four files. Fake runner lane `5f21` passes
  11 tests and its plan test passes 1/1. `sh -n` passes, and real workload test
  `2ab` passes 1/1. These focused results are separate from the failed
  canonical outcome above.
- W9 source repair `b8` is integrated and passes 213 Python tests. The full
  schema-3 comparison at controller `bef` processed 1,690 inputs and 6,760
  outcomes, then exited 1. It found seven exact-owner legacy-projection
  mismatches at indices 885, 887, 889, 897, 899, 1686, and 1688. A later review
  found all chain 01, 03, and 04 captures unqualified: 64 mutable main-import
  paths across 218 source entries make the seven mismatches only a subset.
  The closure builder passes 8 tests as root. Controller wiring is active on a
  separate e240 program snapshot; no new captures exist. W9 parity is not
  accepted. Schema-4 source checkpoint `191` passed 52 focused tests. Integrated
  checkpoint `9acd` passes 200 plus 52 Python tests. Review found that
  config-consumed roots were not compared with audit-config bytes. The helper
  closure repair is active; no native build, capture, or parity result exists.
- Isolated inference pipeline `7ef89df4e` has scalar/list proof commits `6135`
  and `7ef8`, which are not compiled or accepted. Source review found an
  empty-splice over-rejection and a proof gap in Int/UInt binder substitution.
  Corrective source work is active, and Cargo remains held. The prior `0ea`
  diagnostic passes 206/207 full tests and 44/46 lowerer tests outside the
  corpus. Capture-writer and runtime-identity repairs remain source-only with a
  known red witness. No inference implementation is on main.
- Earlier pure inference pipeline `360ed` carries the cohort record and raw
  9,194-site count. Its `0ea` diagnostic records 206/207 full tests, 44/46
  lowerer tests outside the corpus, semantic 23, schemes 8, declarations 9,
  and two module
  tests. Lower diagnostic fixes `0624` and docs `4c390` are integrated, with
  compilation pending. Red type-forgery capture `067` is confirmed; its origin
  fix and scalar/list proofs are active. Shape fix `9ad` and module fixture
  `4f1` are integrated but unverified. Checker `85e` remains under review, with
  BlockOwner and rest-parameter proofs in progress. No inference is on main.
- Review found a config-deletion bypass in the cohort scan. Its 9,194-site
  count and ten P2 repair units are not accepted while that repair is underway.
  No annotations have been removed.
- Earlier typing results include a fresh compile of the library, `xsh`, `xsht`,
  and helper, with four encoding, five semantic, and one module-ground tests
  passing. Prepared and three forged tests failed on invalid doc/`Result`
  fixtures; identity/projection failed in `FullBuilder`, and new generic
  list/`Result` tests were staged at the wrong phase. Verifier `539`→`root3f`,
  lowerer `f53`→`fb`, and fixture lint/docs `a0`→`3d` fixes were not tested on
  that pin. The 1,520-case native harness entered both modes but executed zero
  cases: `indexed_build_instruction` attempted pattern-forwarder lookup at the
  first test declaration. Source fixes `f71`/`86a` address that failure; the
  later `4b` native results are recorded above.
- W9 chain `03` passes 1,690 assignment proofs and 68 ranges. Its historical
  comparison stopped after 75 inputs/300 outcomes, with 12 mismatches and exit
  130. The first input `52b2` differs only in resolved-binding import rows for
  `system`/`unix`. Accepted bugfix `5366` removes the false ScopeChecker runtime
  `BindingID` rows from reference expectations for unaliased single-component
  API-registry imports; the actual checker does not publish these rows.
  Corrected reference worktree `w9-derived-api-imports` is commit
  `74f1551ee9e3066b653ae190779e16d511c90293`, tree
  `3162d2f33076a8c9261d0ffb7a89a9952662f03f`, parent `1fd9e5f2f`; it applies
  the exact nine-line source patch and leaves prior refs/captures unchanged.
  Schema 3's explicit combined-derivation controller at `aac` passes 85 Python
  tests. Corrected reference source `74f1551ee9e3066b653ae190779e16d511c90293`
  builds successfully. Chain `04` on that source and schema 3 recorded a
  capture with 6,760 outcomes across 68 ranges; both original exact-owner proofs
  cover 1,690 inputs. Historical source qualification fix `bef85594c` passes 71
  focused tests and revalidates all 1,690 chain-03 assignment proofs without
  changing the receipt. It checks the ancestor controller's tree and unchanged
  proof authorities. Later review found all chain 01, 03, and 04 captures
  unqualified because of mutable main-import paths. The controller `bef`
  comparison is recorded above; no new capture or rerun has started.
- The [915 acceptance follow-up](evidence/acceptance-follow-up-915/README.md)
  records the suffix checks and focused head rerun. `xsht check` passes 1,542
  files; `xsht lint` exits zero with 72 findings and 115 notes, so lint is not
  clean; `git diff --check`, approved current-metrics accounting, and docs
  checks pass, including three tour-project tests. The seven head file tests
  pass with the existing pinned products. These are suffix-stage results on
  `915e9d8bd`, not a complete canonical batch. The metrics report records
  previously approved integration growth. Products are unchanged. Main has
  since advanced to `e31732415`; at that checkpoint focused checks passed:
  `xsht check` 1,526, exact archive 1, workflow 3 plus one skip, non-root 1,
  curl 2, and route 2. Lint reported two warnings and 113 notes, with fixes
  assigned. The later 8d failure and current `e89` status are above.
- Earlier isolated successor checkpoints: checker `f734` passed a fresh build
  and 57 tests (body 3, focused 7, constraints 42, schemes 5); the earlier `1d`
  module-export, `FullNameOf`, and body-domain probes pass three tests. Draft
  pipeline `cbb` changes encoding, lowering, and runtime sources. A combined
  compile with verifier changes at `340ae7a7a` failed with eight Rust errors;
  encoding, lowerer, and verifier work was routed. Review also assigned
  duplicate-authority and ground-`Any` fixes. W9 parser repair `a8` passes 69
  tests and the single-input proof. Fresh chain `03` has 1,690 ownership proofs
  and 68 independently validated ranges. Its earlier comparison-pending state
  was superseded by the stopped comparison recorded above. The previously ready
  `c410`, `f2`, `41ac`, and `f446` checkpoints were isolated, not integrated;
  the newer driver status is above. No inference code is accepted on main. Lean
  remains paused.
- Isolated test-only `534db019e` removes invalid fixture diagnostics from
  the actual module-source export probe. Its fresh build passes, but the exact
  probe still reports `check.local-inference` on the generic body expression.
  [The retained failure](evidence/typing-intermediate-checkpoints/README.md#source-based-module-export-failure)
  blocks lowering until symbolic and ground expression facts have separate owners.
- Test-only `7d01429ba` repairs the generated ethtool namespace entry point
  by declaring its required `net` effect. [All 20 file tests pass](evidence/ethtool-fixture-7d014/README.md)
  both before and after integration, including every device-backed case.
  Canonical root and shared-device failures remain under investigation.
- Frozen `787ba37a4`, tree `31681c63f0`, integrates the second process-exit repair
  and builds matching verification products. [The bounded integrated replay](evidence/procps-exit-races/README.md)
  completes 500 iostat calls under eight-worker churn, passes written and freshly
  desugared procps 39/39 each, and passes the native malformed-counter rejection.
  [The canonical batch](evidence/canonical-787/README.md) passes all Rust stages,
  then finishes the native suite with 13,720 passed, 160 failed, and 92 skipped.
  Follow-up repairs addressed root-UID execution, `/dev/full` fixture pollution,
  and SGID setup; 159 of the 160 failure-census cases then passed. The remaining
  head memory case passes all seven `test-gnu-head.xsh` tests separately on
  `915e9d8bd`, including the huge excluded-tail-count regression. This focused
  pass is not a full native-suite rerun. Repository check, lint, structural
  checks, and docs were not reached in the original canonical run.
- [The source-bound structural scan](evidence/structural-787/README.md) retains
  all four hard zeros and the established 152/17/46 schema counts. Production
  source totals 268,539 lines, 60 above the original qualified baseline. The
  Linux test module is correctly excluded without a metric-policy change.
- Isolated inference `935d7da01` passes a recompiled library build, both new Any
  row regressions, all 42 constraints, all five schemes, and the synthetic export
  projection probe. Test-only `9ea2333e0` reproduces the two Any failures first.
  The real exported-identity handoff remains HOLD: visibility must persist on
  the declaration through finalization. Private pure identity inference also
  remains required. [Intermediate evidence](evidence/typing-intermediate-checkpoints/README.md)
  keeps the source-review findings, failed builds, and partial runtime results
  distinct from feature acceptance. No inference implementation is on main.
- Clean source `449320b18`, tree `b1f2632d9`, builds matching xsh, xsht, xshi,
  ordinary, and dedicated privileged products. All 184 expression route probes
  pass and [independent review accepts the exact set](evidence/routes-449/README.md).
  [Targeted Linux checks](evidence/linux-residual-449/README.md) pass: traceroute
  18 passes and two expected skips, root block-device two passes, and all 68
  integrated adapter-runner tests. These do not close canonical acceptance.
- The procps failure has two reproduced exit races. The first returns ESRCH
  from an open proc stat descriptor after its task exits; reviewed repair
  `ba14ae8e4` is integrated at `204467038`. A subsequent bounded run on `449`
  finds a readable dead-state X record with pgrp/session both -1. A read-only
  probe captures the exact row before the entry vanishes. The second repair
  `3646630bb` has independent source approval: the captured row fails before
  the fix, both live controls pass, and all six focused tests pass after a real
  recompilation. [Raw evidence](evidence/procps-exit-races/README.md) preserves
  the rejected stale-cache attempt. The fix is integrated at `787ba37a4`;
  the matching product replay above passes and fresh canonical acceptance
  remains pending.
- The private cp mount-worker selector repair is integrated at `449320b18`.
  Exact ordinary and wrapped name controls pass. On matching source and products,
  [QEMU passes three XSH date cases, three GNU date cases, and one reflink case](evidence/linux-qemu-449/README.md);
  every successful guest and QEMU exits zero. The reflink retry explicitly
  exports the guest PATH so Rust can find `/sbin/mkfs.btrfs`. Its earlier ENOENT
  receipt is retained separately; it is not a product failure or a pass.
- The full W9 comparison completes at controller `2b221766` against candidate
  `d20af40bc`: both exact-owner proofs cover all 1,690 inputs, but comparison
  exits one with 6,732 common checked-fact mismatches among 6,760 outcomes.
  [The complete failed comparison](evidence/w9-d20-final-failure/README.md) is
  retained. Its first mismatch is the candidate-only assignment summary absent
  from legacy publication. An independent legacy assignment-name oracle is required
  before projecting that field. Full publication captures remain immutable;
  no current W9 parity pass is claimed.
- [Isolated inference checkpoints](evidence/typing-intermediate-checkpoints/README.md)
  retain the accepted retained-byte probe at `aef756e1a` and intermediate scheme
  review at `81481a573`. The latter passes 41 constraints, three checker probes,
  and XSHT all-target compilation. One newly added scheme test has an incorrect
  Ground-map fixture; its gate stays open pending correction and rerun. Source
  review approves the intermediate canonical-scheme and Pending-to-Checked
  boundary. Generic body facts, lowering, and capability acceptance remain open.
- The clean `ed90857bd` replay passes 305 process tests and 741 library tests,
  then stops at the desugar comparison: 115 XSHT tests pass and one fails.
  The written procps file passes 39 tests; its transformed counterpart fails
  the iostat mebibyte-header assertion with empty stdout. The exit-race
  investigation above follows this failure. [Raw evidence](evidence/canonical-ed908/README.md)
  preserves the failure. Soundness, privileged checks, and the normal full
  native suite were not reached. No timing credit is claimed.
- All 184 mapped expression route probes pass on clean `ed90857bd` with its
  actual verification products, covering 92 tags in normal and forced modes.
  Each emits one witness artifact. [Independent review and the existing
  collector accept all 184 tuples](evidence/routes-ed908/README.md);
  this is not a full native suite or evidence for unsupported non-Expr routes.
- Isolated scheme checkpoint `121d39242` passes all 41 constraints and nine
  exact probes, with root-library and XSHT all-target compilation passing.
  The pre-fix union, Set, and Module probes reproduce their failures. The fix
  lowers inherited record descendants and shares the recovery predicate.
  [Evidence and independent source approval](evidence/typing-scheme-checkpoints/README.md)
  qualify this substrate checkpoint. Function schemes, immutable body facts,
  and end-to-end inferred capability acceptance remain under implementation.
- The root-package strict Clippy command and the changed fuzz target with
  `--no-deps` pass on `ed90857bd`. The initial fuzz command includes existing
  XSHT dependency diagnostics and fails. [Both scopes and outputs](evidence/clippy-ed908/README.md)
  are retained; these checks do not establish workspace-wide lint cleanliness.
- The clean `45b4c1cd8` canonical attempt passes 305 process tests, 741 debug
  unit tests, and all 116 selected XSHT integration tests. The complete
  desugar comparison passes. Soundness then stops at the replacement-label
  test: its accepted seed-4 program contains no `.replace(`. Thirteen other
  soundness tests pass. [Raw batch evidence](evidence/canonical-45b4/README.md)
  retains the failure and actual product hashes. Later canonical stages,
  including the normal full native suite, remain unexecuted.
- Reviewed compact W9 formatting is merged at `71f7cd689`. The bounded
  same-object comparison proves canonical equality of pretty and compact
  publication output. The affected Python modules pass 67 and 54 tests on the
  integrated tree. [Evidence](evidence/w9-compact/README.md) retains the
  1 GiB diagnostic, four Rust oracles, failed assertion correction, and review.
  Five fresh harness roles build and pass listing checks at isolated source
  `d20af40bc`. Its application source matches `45b4c1cd8`. Full capture and
  comparison now complete with the assignment-summary gap recorded above.
  Full cohort parity remains unaccepted.
- Isolated solver checkpoint `3545a21d6` passes all 29 constraint tests. Its
  corrected rollback oracle expects the established canonical alias, with
  the error and parent assertions retained. Independent review then finds
  flexible-to-rigid row rejection, compression outside rollback, and two
  operand-direction reversals. Test-only `d03fdc2b7` reproduces all four
  failures. The initial compression probe had a bad parent-chain fixture;
  the corrected probe reaches the failed constraint and observes compression.
  [Raw checkpoint evidence](evidence/typing-row-checkpoints/README.md) retains
  the red and green runs. Repairs pass all 36 module tests and thirteen exact
  regressions at `86c449d9d`. Independent source review returns GO. This
  isolated solver prerequisite is accepted. Levels, generalization, and
  schemes follow on the same branch. No inference code is accepted on main.
- The nine final native runs are the canonical normal suite, selected release
  opt3 correctness suite, forced-frame suite, and three reference/candidate
  scheduler pairs. The forced-frame gate requires the release suite receipt.
  The profile-selection waiver preserves this correctness prerequisite.
- The [ten-row review](evidence/acceptance-review-45b4/semantic.md) separates
  current source review, executed tests, and pending runtime evidence. The
  [defect review](evidence/acceptance-review-45b4/open-defects.md) reconciles
  TODO claims without closing deferred features or historical reports.
- The replacement-label test repair is merged at `8805a8ece`. A deterministic
  AST replaces the incorrect seed-shape assumption. The exact test and all
  fourteen soundness cases pass in the lane. The exact test also passes on
  main. [Raw evidence](evidence/fuzz-replacement-1d64/README.md) records the
  source-mount correction and separate build/runtime allocator settings. Fresh
  canonical verification remains required.

### Verified since resume

- Locked verification products and root process-test artifacts built in the
  pinned image with Kache and mold. Cargo reused the existing target tree.
- The `fff13082f` structural scan passed without an envelope: manual payload
  calls, optional raw calls, wide arena wildcards, and blanket dead-code lines
  are zero. Static type predicates are one, type-storage mappings two,
  instruction rows 152, patterns 17, and stages 46. Five factory-call sites
  still require their semantic bound review. Source totals are advisory.
- An eight-root lint fixture recorded eight requests, one prepare, seven
  reuses, one checked record, and seven check hits for its shared user module.
  This is a record-cache observation, not one body traversal. It is preliminary
  evidence and does not close the source-pinned semantic review row.
- The historically extra AWK shard passed in written and desugared forms with
  one worker in a disposable source copy. The historical individual case,
  transformed source, and child diagnostics remain unavailable. This passing
  pair does not explain or close the old mismatch.
- A [bounded replay of the original 0ea source](evidence/awk-replay-0ea/README.md)
  selected twelve stream/process shards under root with eight workers. Written
  and freshly desugared forms both passed shard 001 and failed only the separate
  root-sensitive shard 008. That replay did not include the cross-file curl mutation established above.
  It is not a full-suite acceptance pass.
- The first canonical batch at `fff13082f` passed 305 root integration cases,
  with four ignored. Its debug unit target passed 715, failed two, and ignored
  three. Both failures inherited `/work/xsht-config.ini` through disk-backed
  temporary paths. The batch stopped before the remaining canonical stages.
  Timing was discarded. [The failed output](evidence/resume-fff13082-canonical.log)
  and [resource capture](evidence/resume-fff13082-resources.json) retain the
  actual observations. No normal full native suite ran in this attempt.
- Reviewed repair `a86d4a7123bed5aafde302b170342dfd4fb4a2da` is integrated.
  Config selection now has a private controlled filesystem predicate for
  unit tests. Public discovery behavior is unchanged. Both original failures
  reproduced before the repair; all five project tests pass against the
  merged source, including the added relative-entry case. [Merged-tree output](evidence/resume-a86d4-project-tests.log)
  preserves that focused pass. No test coverage was dropped.
- Scheduler reference qualification is integrated through `6fc1478a1`.
  The comparison helper validates the retained aggregate overlay against
  the three original scheduler commits. Its derivation and resource-snapshot
  checks pass. No scheduler comparison or speedup is established.
- Route-evidence helper recovery is integrated through `fe4986b6e`.
  Reconstructed inventory and operand-policy inputs are source review data.
  The merged helpers pass 60 semantic-matrix and 45 eager-atom selftests.
  Mapping validation finds 114 uncredited candidates and one artifact-capability
  mapping. These checks credit zero executed route tuples. Source locks require
  an explicit command and remain under `.work/`. EmptyMap child bytes are pinned
  separately from its complete test file. The pipeline verifier oracle covers
  a present input with the wrong category. Missing or truncated input remains
  a separate evidence gap.
- Seven assignment parity fixtures pass against the old collector. They cover
  late writes, nested comparison candidates, target roots, and excluded regions.
  They also cover original scope visibility, error output, diagnostic order,
  and statement-pair movement. The approved design is in
  [lint rules](designs/lint-rules.md#assignment-safety-facts).
- The checked assignment producer is integrated at `266421477`.
  Existing lexical preparation collects source-and-range-owned spelling sets.
  Private storage preserves the public `ResolvedBindings` layout. Ten focused
  tests pass, including error output, raw sugar regions, and owner separation.
  The classifier repair at `7afa88e06` requires
  explicit decisions for future expression variants. Both existing latent
  stream tests pass. [Focused output](evidence/resume-assignment-producer.log)
  retains these checks. The frontend facade export is repaired at `f04cbfd6f`.
  No final campaign gate has run on this batch.
- The lint consumer is integrated at `e5bd34e50`. It borrows checked summaries
  and collects SourceOnly assignments during the existing descent. The separate
  recursive assignment prepass is deleted. The follow-ups at `1f7b795b6` and
  `52c0f5399` preserve exhaustive statement-region and dynamic-require decisions.
  At the clean `52c0f5399` freeze, all ten merged-source XSHT unit selections and
  four exact native lint regressions pass. Actual abort status-23 children retain
  one authenticated route record in each mode. Baseline and candidate check and
  lint diagnostics match on the same repository and Laputa source cohorts.
  [The focused evidence](evidence/assignment-52c0/README.md) retains full output.
  These checks leave canonical and performance acceptance open.
- Route-capture repair `84668c961` preserves valid artifacts from nonzero child
  exits. Its regression fails before the repair. Three capture and twelve
  authentication unit tests pass afterward. Nonzero children without artifacts
  retain their original result and supply no route evidence.
- Native expression oracles at `67dc22efd` add 37 missing canonical cases.
  The PreparedConstant oracle at `21f20fea6` checks its record-default value.
  Baseline probes pass for 36 new tags and all twelve scalar expression tags
  in both modes. Abort's candidate check passes as recorded above. These probes
  do not close the source-pinned route matrix.
- The original expression-oracle migration is integrated at `95488faf2`.
  Forty-one cases now capture direct children. Their original behavior
  assertions remain where a different child is needed for one target record.
  All 82 baseline probes pass with one authenticated record each. The existing
  Match child also passes in both modes. Final candidate replay remains open.
- Assignment identity repair `0957cb97d` selects the shared binding at the
  assignment site. Context ownership, latent-stream updates, and projection
  invalidation retain that identity. Three decoy-stack invariants fail before
  the repair and pass afterward. Two existing assignment and transfer tests
  also pass. The nearest active refinement remains the scope owner. Remaining
  AST, proof, call, and command consumers are under review.
- Route-matrix recovery is integrated through `088da08c5`. Mapping validation
  requires all 152 schema tags and reviewed child sources for all 92 expression
  tags. The child sources match the completed probe table at `95488faf2`.
  This mapping checkpoint credited zero receipts; the current `ed908` replay
  above credits all 184 supported tuples. The remaining
  60 tags retain unsupported artifact mappings. Main and eager helper
  selftests pass 61 and 46 cases on the merged source. The eager helper now
  selects Rust commands for Rust verifier oracles. Pipeline runtime behavior,
  wrong-category rejection, and the absent-or-truncated gap remain distinct.
- Pipeline payload tests at `382381547` reject an absent input word and a
  payload truncated after the input ID. Both begin with a verified fixture
  and assert the exact decoder rejection after changing its payload length.
  The two new tests and the existing wrong-category test each pass. Evidence
  metadata at `52fc6694a` records all three rejection oracles separately from
  the native child and removes the covered malformed-payload gap. Merged main
  and eager helper selftests pass 62 and 47 cases. Route receipts were zero at
  this earlier checkpoint.
- Disposable QEMU preparation passes its guard smoke. The guest uses the pinned
  image's userspace, matching Linux 7.2.9-2-stable kernel and modules, TCG/q35,
  two CPUs, 2 GiB, fixed RTC, and no network. Its temporary files stay on ext4.
  Guest-only BTRFS tools are installed. GNU 9.12 `date` and `cp` were rebuilt
  from the checksum-and-signature-verified official source for the clock and
  reflink probes. The pinned image has neither GNU tool, as corrected in
  `a7298ffa3`. Disposable assets remain under `.work/consolidation/qemu` and
  `.work/consolidation/gnu-reference`. Product tests await the final source and
  matching harnesses. The smoke establishes no privileged acceptance result.
- Binding-reader repair `2cd2baa86` is integrated. AST expressions and tails
  consume their published identities. Missing identities report existing
  unresolved-name errors before procedure or literal-word fallback. Command
  capture preserves its first selected ID. Proof creation retains the chosen
  binding. Stream classification checks that binding's scope owner. Recovery,
  constructor precedence, and the documented `error` module exception remain.
  All 18 focused unit selections pass. Fresh verification products pass four
  existing native item tests and match the constructor probe's baseline output.
  [Retained output](evidence/binding-reader-2cd/README.md) records the checks,
  source qualification, product hashes, and sizes. Canonical gates remain open.
- The fresh 2cd repository/Laputa comparison fails diagnostic parity.
  Imported standard modules receive lexical runtime IDs but no checker value
  bindings, so the identity lookup rejects valid namespace calls. Baseline
  2688 accepts the same source. [All eight command outputs](evidence/binding-reader-2cd-comparison/README.md)
  retain the failed observation. A minimal imported `system`/`unix` source
  reproduces it. Repair `5366d950a` omits only accepted namespace-only imports
  from value facts. The exact native regression is red before and green after;
  five existing namespace/import tests and four Missing-identity tests pass.
  Fresh merged verification products restore byte-identical repository and
  Laputa check/lint diagnostics on the same source. [Repair evidence](evidence/binding-import-5366/README.md)
  retains the focused checks and all eight paired outputs.
- Verification `xsh` and `xsht` products were rebuilt at `2688d52fc` for the
  assignment repair's output baseline. Repository and Laputa check and lint
  each exit zero. Preserve their complete outputs for candidate comparison.
  Compare diagnostics before the closing timing line, as the integration
  harness does. These runs do not establish performance acceptance.
- W9 recovery tooling is integrated at `b111cb3d8`. Explicit source-cohort
  schema 3 and fresh adapter/exact-owner schema 2 retain strict historical
  readers. The qualified builder fixes source, tools, profile, and resource
  identities. All 172 merged-source Python checks, shell syntax, and diff
  checks pass. [Tooling output](evidence/w9-recovery-b111/README.md) establishes
  no real harness build, capture, or parity result.
- Real W9 builder qualification found two integration failures. Repair
  `113a9ee7e` removes an unused generated Rust import; warnings stay denied and
  all 65 adapter tests pass. The next attempt compiles the reference harness
  but rejects its path outside the source checkout. The modern builder uses
  an exact shared target directory, which needs a separate strict artifact
  boundary. [Both failed attempts](evidence/w9-builder-preflight/README.md)
  remain retained. No build receipt or capture is accepted yet.
- Artifact-boundary repair `7cf20b6c1` is integrated. Modern builds derive the
  exact artifact directory from the pinned builder's plan for the same source,
  overlay, role, and environment. Historical source-root checks remain strict.
  All 178 merged-source Python checks, shell syntax, and diff checks pass.
  [Boundary evidence](evidence/w9-artifact-boundary/README.md) retains the five
  complete suite outputs. A fresh real build and capture remain pending.
- Shared lint-flow repair `d2c842812` is integrated. Transfer functions consume
  child summaries published by the common walk instead of recursively reading
  each subtree again. Source sugar preserves the reference's expansion behavior.
  The dead-code file passes 25 tests, guards pass three, and the exact duplicate
  summary invariant passes one. A deliberate fallthrough mutation fails its
  exact oracle, then passes after restoration. All eight repository/Laputa
  check/lint commands exit zero with matching output apart from timing footers.
  [Focused evidence](evidence/lint-flow-d2c8/README.md) retains fresh test output,
  paired diagnostics, source accounting, and the mutation evidence's limits.
- The feature-touch compiler replay at `d2c842812` finds 35 normalized XSH
  sites and 19 XSHT sites. Its combined compiler check passes after adding
  diagnostic placeholders. [The recount](evidence/feature-touch-d2c8/README.md)
  retains complete compressed Cargo output and source-site maps. Diagnostic
  commit `300d284d4` must never merge. The refreshed XSHT wildcard inventory
  has 114 rows across 98 owners. No synthetic expression executes.
- The clean `619810fb9` structural scan retains the hard zeros and counts
  268,479 production lines across 372 files. Five exact XSHT module/lint
  units each pass. The disk-backed nodev/mknod preflight also passes without
  changing host mount flags. [Source evidence](evidence/source-review-6198/README.md)
  distinguishes these checks from the pending module trace and product gates.
- Fresh W9 adapter builds at `619810fb9` pass artifact selection and listing.
  The exact-owner reference build fails on three unused helper imports with
  warnings denied. The adapter capture is then stopped with SIGINT, exits
  130, and cleans up its workers. It remains incomplete and grants no parity
  credit. [The complete attempt](evidence/w9-preflight-6198/README.md) is retained.
  Repair `5347f665a` projects only those imports from modern compiled helpers
  while preserving the original source and historical pins. All 181 merged
  Python checks pass. [Projection evidence](evidence/w9-exact-imports/README.md)
  leaves the repaired build and fresh captures pending.

### Current queue

The canonical batch on `e89e35f29` ended at native tests with exit 3, as
recorded above. All 13 failure causes have targeted fixes, and their exact tests
pass. The corrected four-caller gate passes 45/45. Its two earlier
`docker.host_link_tools` errors came from missing mold and common Git mounts;
the corrected runner needed no source patch. Main is now `3bae6555c`. The
`df` capacity repair from `507` is integrated. Focused check and lint, fake
runner 11/11, plan 1/1, `sh -n`, and workload test `2ab` 1/1 pass as detailed
above. The earlier `8d` canonical failure is historical and remains documented.

Route-runner fixes `36d58` and `70192` preserve explicit product attestations
and use the canonical `xsht` path in the collector. Seven integrated runner
tests and 63 route self-tests pass. The private `gates` debug-default lint
warning is removed without changing its `Bool` contract. Source check, lint,
and exact plan are running. Canonical preflight sees unchanged Rust since `e89`
and sufficient host headroom. The canonical batch has not rerun and remains
held for W9 source-closure tooling review, not inference.

Stable copied-artifact suffix rerun of `a22` passes check 1,526, lint zero
findings/113 notes, metrics, and docs/tour 3. The shared `xsh` drift from
`a911` to `d058` is handled by immutable copies, not by assuming the products
are interchangeable. Queue `b6` passes 11 tests and four-file check/lint;
SGID private-scratch passes `test_repository_tmp.py`,
`test_profile_cost_selection.py`, and `test_profile_cleanup_signal.py`; `f958`
passes focused lint with zero findings/notes and three negative preflight cases.
Replay launcher/bridge `73c`/`334` are integrated. Driver fix `8d` passes 22
tests as root; the earlier
`9a` replay failed before Docker and ran no suites. No release, timing result,
or consolidation close is available.

The checker audit's assignment and reader repairs and imported-standard-module
fact correction are integrated. Requalify the repaired facts and readers on the
final source. The invariants establish authority failures in
controlled checker states, not demonstrated user-visible failures. Existing
nearest-visible-name proof filters retain their identity checks. Changing
those filters could replace a real shadow and is outside this repair.

The diagnostic compiler replay at `d2c842812` finds 35 XSH and 19 XSHT source
sites requiring explicit decisions for a synthetic expression variant. The
two earlier classifier survivors now require compiler decisions. This recount
includes the checker and shared-flow repairs. Its synthetic source must never
merge and supplies no runtime reachability evidence.

The archived W9 corpus is recovered. Its 1,690 file paths, bytes, modes, and
manifest match the pinned e240 source. The original legacy capture is missing.
Fresh captures must bind the same source and cohort before exact-owner proof
and candidate comparison. Historical ownership receipts cannot replace the
missing outcomes. Five current harness builds qualified on `0d6`; fresh capture then hit the
reference serialization OOM recorded above. Recovery tools and proofs still
require complete fresh outcomes. No parity is claimed.

The migrated expression children require explicit matrix requalification.
Keep the scheduler comparison's original default workers and outcome checks.

The [shared flow integration](designs/lint-rules.md#flow-summaries-in-the-shared-walk)
is integrated with focused parity and an independent static review. Replay the
feature-touch probe and structural scan on the repaired source. Focused checks
do not replace the final canonical and full native gates.

Wider, staggered general-inference work is authorized in isolated worktrees
while consolidation acceptance continues. This supersedes the earlier
wait-before-lowerer sequence. Keep each lane's source and evidence separate;
there is no inference implementation on main. Shared integration, convergence,
and the accepted performance baseline remain gated on consolidation acceptance.
Lean and Phase F remain paused.

## Resume

Prompt: "Continue consolidation acceptance from this handoff. Keep Lean
paused. Continue authorized inference lanes in isolated worktrees; do not
integrate inference on main before consolidation acceptance closes."
The owner authorized wider, staggered isolated inference work alongside
acceptance, including lowerer work without a whole-acceptance barrier. Shared
integration, convergence, and the accepted performance baseline still wait for
consolidation acceptance. Judge Phases B–E independently of deferred Phase F.

First command, from the repository root:

```sh
git status --short
```

Read [the campaign](CAMPAIGN.md), [the successor requirements](CAMPAIGN.md#typing-and-formal-successor-requirements),
[the performance policy](CAMPAIGN.md#successor-performance-policy),
[the items](ITEMS.md), and [the testing owner](../../docs/TESTING.md).
All numerical typing targets and their current hard or aspirational status are
in the campaign. No deleted brief or old work directory is required.

## Winddown state

Integration remains on `master`. The executable source and test roots remain
unchanged from `633e6394df7e7492226a7c622cd3bad9f4eb2f86` during winddown.
The completed PTY-frame repair, root permission fixtures, desugar worker bound,
and six-digit PID separator repair were already integrated before this stop.
The owner steering is incorporated in the campaign; its standalone file is
removed. These documentation changes do not claim a new executable pass.

The 633 selected-profile native suite passed 13,793 cases, with zero failures
and 136 skips. Its observer completed 1,022 root Rust tests, then was
interrupted for winddown. Canonical acceptance did not complete. Isolated lint
exited zero but exceeded the existing 15-second budget. The historical extra
AWK desugar outcome was unexplained at winddown. The current resume section
records its reproduced cross-file mechanism. No scheduler or comparative speedup
is claimed. [Retained receipts and recovery instructions](winddown/README.md)
state the evidence limits.

## Winddown lanes

No lane or verification job remains in flight. The 824 original non-ancestor
branches and 92 additionally imported source-tip branches are parked. Their
source is recoverable through [the original branch index](winddown/branches.tsv),
[the detached recovery tags](winddown/detached-refs.tsv), and
[the imported source refs](winddown/standalone-source-refs.tsv).
[The historical lane review](winddown/lane-review.md) records changed scopes.

Automatic approval review rejected a mass archival merge because it would
mark competing, unintegrated alternatives as merged. Preserve the parked refs;
these histories are not accepted implementation. On resume, compare the
relevant source with current owners before choosing any unfinished patch.

All 50 dirty overlays, including staged alternatives and untracked fixtures,
are committed as recovery patches in the [linked index](winddown/unfinished/index.tsv)
and [standalone index](winddown/unfinished/standalone-index.tsv).
Each final patch was applied against its original tree in an isolated Git
index and the restored bytes compared with the original files. Diagnostic
probes and old scheduler baselines remain explicitly archival alternatives.

## Retained worktrees and disposable state

Only the primary repository checkout remains. All 557 linked worktrees,
79 standalone repositories, their 11 additional linked checkouts, and two
stale partial checkouts were removed. Both sibling consolidation directories
are gone. No worktree needs recreation until explicit resume.

The repository work directory is disposable, including caches, binaries,
old launch pins, and test fixtures. No required source or resume instruction
lives only there. Optional large historical captures are locally ignored in
`.campaign-evidence/consolidation`, outside that directory. They are not
checked into Git and are not required to recover source or resume work.
The committed small receipts retain historical path strings as observations,
not as files that must still exist.

## Remaining work

1. After explicit consolidation resume, qualify the source freeze and prepare
   current launch pins using the existing gate owners. Complete the unreached
   canonical, native, forced-route, privileged Linux, and soundness checks.
2. Retain the original AWK failure and the source-pinned cross-file reproducer;
   verify the integrated fixture and curl repairs in the fresh canonical batch.
3. Finish structural metrics, ten semantic review rows, docs, resource/size,
   scheduler, and comparative acceptance. Time-based gates require readiness
   before and after. Keep one full native suite at a time. The owner waived
   alternative profile-selection measurement, not correctness or other gates.
4. Continue the isolated identity/projection/import/rejection slice alongside
   consolidation. Retain initial red witnesses, the alias-chain repair, and
   solver counters. Freeze the performance baseline after consolidation closes.
   Then widen Phases B–E. No such new tests ran in winddown.
5. Phase F stays deferred until the owner explicitly resumes Lean. It does
   not block typing acceptance or authorize more disconnected proof lanes.

## Verified

- Git worktree pruning and listing retain only the primary checkout.
- All standalone source-ref histories and checkout states were checked before
  removal. Imported refs preserve 136 commits previously absent from the
  primary object database. Git connectivity checking passes.
- Recovery patches restore all 50 unfinished overlays. The two stale source
  trees have no unique source relative to their identified existing commits.
- Large historical captures were byte-verified, moved to local ignored
  storage, and removed from the unpublished commit that briefly included them.
- No campaign build/test processes or Docker containers remain. All team lanes
  are completed or interrupted. No user crontab or scheduling tool is present;
  the host has no `systemctl` for a systemd timer audit.
- The disposable work directory is on ext4 and excluded through local Git
  info/exclude. Optional historical capture storage is also locally excluded.
- The configured trusted Stop command was inspected and its closeout hook
  passed directly. The interactive hooks UI is unavailable in this session.
- Clean-environment resume and link checks passed. No new runtime tests,
  builds, performance measurements, or formal verification ran for winddown.

Historical detailed handoffs remain in Git history. Use this document for the
current stop; do not launch commands from obsolete chronological entries.
