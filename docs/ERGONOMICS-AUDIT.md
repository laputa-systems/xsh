# Ergonomics completeness and autofix audit

The audit covers every numbered proposal in `ergonomics-0.md` through
`ergonomics-7.md` (removed after implementation; read them at `d6f09bc5`), plus
maintained XSH sources in this repository, `../packages`, and `../laputa`. The detailed matrices distinguish a supported exact example,
a narrower supported family, an intentional manual policy change, and a missing
safe migration:

- [Proposals 0–3](ERGONOMICS-AUDIT-0-3.md)
- [Proposals 4–7](ERGONOMICS-AUDIT-4-7.md)

Language feature support does not imply complete autofix support. Remaining
bounded gaps include forwarding aliases with imported/defaulted return
contracts and importing workspace context into standalone annotation/pipeline
proof checks. The reopened pass added bounded helper-to-try elimination, the
illustrated command scope initializer, and formatted block-string migration. Suffix slicing and indexed reconstruction
need stronger bounds evidence. Moving defaults, cleanup lifetimes, retry
selection, accepted exits, and serialization policy remains authored work.

## Repaired correctness boundaries

The completeness pass reproduced failures rather than assuming that a checker
accepting the original source made its replacement safe:

- Map entry iteration now requires an immutable map. Mutations hidden in value
  blocks changed live key lookup into an old entry snapshot.
- Path simplification preserves native bytes. An explicit text display can
  replace non-UTF-8 bytes; removing that conversion changed actual argv.
- Tail-return removal groups record expressions, including spread records,
  so a match arm does not reinterpret them as statement blocks.
- Annotation removal retains contextual collection element domains, nominal
  schemas, empty splices, Result success/error domains, and inferred require
  targets. Assignability alone does not prove context independence.
- Require removal preserves unsigned validation and requires an exact identity
  rather than treating a compatible numeric carrier as an equivalent schema.
- Assertion migrations retain context-dependent operands and source-order
  evaluation of eager custom messages.
- Guarded exits now preserve the opposite condition's continuation proof in
  both full and compact checking. Non-exiting yields and invalid lexical exits
  do not acquire that proof.
- Empty local returns resolve the same constraints with explicit and implicit
  tails. Bare statement blocks discard scalar tails; callable Unit tails retain
  their stricter return contract.
- Known-field get migration was added for ordinary materialized records with
  a guaranteed field and identical checked value type. Host-backed records do
  not receive a guessed infallibility proof.
- Lookup defaults now accept inert Paths and immutable typed binding reads,
  including parameter/alias reads. Mutable reads, failure, effects, comments,
  and named argument reordering remain guarded.

The focused regression owners are `crates/xsht/tests/lint.rs`,
`tests/xsh/local-inference.xsh`, `tests/xsh/guarded-control-proofs.xsh`,
`tests/xsh/stdlib/path.xsh`, and `tests/runtime/stack_depth.rs`.

## Performance and regression gate

`make check` delegates to `cargo dev check lint`. It runs the self-contained
Rust integration gate in `crates/xsht/tests/lint_performance.rs`, invoking
ordinary `xsht lint` from the repository root with canonical configured
discovery. It requires no diagnostics and a successful exit within 60 seconds.
Compilation is outside the clock; process startup, discovery, checking, and
linting are inside it. The gate never requests autofix. A timeout kills and
reaps the child; isolated cases verify imported diagnostics, fixture
exclusions, source preservation, and deadline cleanup. Inherited
`XSH_MODULE_PATH` cannot redirect its module discovery.

The final `make check` run passes all four integration cases. Whole-repository
lint is clean in **42.338 seconds**, leaving 17.662 seconds below the fixed
deadline. This unprofiled debug run used four workers on a MacBookPro18,3 with
10 logical CPUs and 32 GiB RAM, with competing native workloads stopped.

Measured implementation regressions pin actual work rather than only elapsed
time: one command-wide module index, one original proof preparation per source,
one statement-membership scan per linter, no completed checker histories copied
into speculative probes, source-local fact projection, and one shared immutable
type arena. Scope lookup uses an indexed interval sweep instead of repeated
whole-block scans. Standalone proof helpers refuse unresolved imports before
performing a guaranteed failing body check. Four workers are bounded by actual
CPU/root counts; serial/parallel diagnostics and source bytes agree. Frontend
workers reserve 16 MiB stacks for valid deep constructor sources.

Synthetic phase evidence is separate from end-to-end timing: 1,500 independent
root module configurations improved from roughly 610 ms to 5.07 ms; 64 call
expressions changed from 10,656 statement inspections to 66; repeated original
annotation probes changed from twelve to one. These are not claims about the
same speedup for an entire package workspace.

After rebuilding `target/release/xsht`, ordinary read-only lint of this
repository completed cleanly in 7.110 seconds. Whole-workspace packages lint
completed in 8.781 seconds, returning checker errors and 1,886 diagnostic
locations across 83 files. That corpus remains useful evidence of migration
gaps; it is not claimed to be lint-clean. These release measurements are
separate from the debug integration gate and do not establish a before/after
end-to-end speedup against the original binary.

## Corpus repairs and validation limits

The configured repository corpus was repaired manually. No formatter or
`xsht lint --fix` command performed the corpus cleanup. Meaningful syntax and
shadowing witnesses remain exercised in isolated scripts; negative and migration
fixture payloads remain intact. Required schemas and eager diagnostic evaluation
were retained instead of widening types or excluding troublesome sources.

Packages now preserve `MakeTask`, spawned handles, JSON boundaries, and recipe
hook capability contracts. All 69 production recipe entrypoints check cleanly;
the PM entry check completed in 19.52 seconds in a recorded debug run. Focused
native coverage includes seven publication cases, metadata field retention,
task invocation/cancellation, graph contracts, all declared hook variants,
certificate proof boundaries, and parser-generator sources. Authored Bison/Flex
source changes updated only checksum fields verified to match their previous
bytes. Existing heterogeneous native argv remains its explicit dynamic boundary.

Laputa has no `Any` annotations in its XSH corpus. Its environment overlay uses
concrete schemas and a nullable smoke configuration; absent variables are
omitted. All 38 top-level native tests, 26 selected read-only lint checks, owned
checker inputs, and package-dependent bootstrap/container/cross-consumer checks
pass. The Linux-only container fixture was checked but not executed on the host.
The final cross-consumer native test also passes. Workspace lint requires the
documented package module search path; without it, package imports are
unresolved. With it, remaining workspace diagnostics originate in imported PM
sources rather than Laputa-owned files.

The final tooling lint integration gate passes 219 tests; the semantic gate
passes 173 tests. Tooling library coverage passes 67 tests, and API coverage
passes 65 tests. Core native coverage passes 98
tests with three platform skips; host-safe standard-library coverage passes 312
tests with 17 fixture skips, plus eight Path tests. Dev system-report coverage
passes 217 distinct cases; its Linux trace case and one slow CLI incompatibility
loop were not verified in this run. The broad core system-report run stopped
after 16 passing cases; focused actual-model tests also passed.

The broader filtered Rust runtime gate passed 398 of 400 cases. Its unresolved
failures are an interactive completion golden with directory-order differences
and a process-group cancellation marker timeout. The syntax gate's remaining
failure is its recursive sibling-package formatting corpus check: the legacy
`../packages/tests/xsh/fixtures/linux-recipe/published-runner-kbuild-pool.xsh`
fixture still uses removed environment-scope syntax. No formatter,
Linux build, package build, network operation, or runner-pin update was used to
make these failures disappear.

Remaining package findings include Linux helper migrations, a Dropbear indexed
build blocker, and the existing certificate-source checksum mismatch. The
frontend stack regression covers the valid m4 checker-worker crash; frontend
workers now reserve enough stack for that constructor depth.
These limits are separate from the clean repository lint gate and the 69 checked
recipe entrypoints.

## Reopened removal and migration pass

The September 30 pass found maintained package call sites that the earlier
production-entry check did not cover. It migrated 87 standard membership calls,
95 removed lookup-default overloads, 119 harness-only test declarations, five
explicit find-sentinel policies, one filesystem alias, four stream option forms,
and six environment scopes. Two old lookup defaults in the producer-memory
benchmark also now use the Result fallback operator. Arbitrary caller-owned
`contains`/`has` fields and Rust membership primitives remain supported.

The public membership registry removal was already correct. Isolated failures
exposed two additional implementation bugs: snapshot-based fixes broke inline
match-arm syntax, and obsolete runtime method-name dispatch intercepted dynamic
caller-owned `contains`/`has` fields. Lexical snapshot blocks preserve source
order and scope, while removing stale dispatch restores those user fields.
Obsolete FsRoot alias lowering, expression tags, codecs and executor branches
were removed; the receiver operations and host operation identities remain.

The exact command scope example also exposed a missing value-tail behavior.
Captured text, bytes and records now retain their values in checked value tails
through full/compact checking and both indexed execution routes. Nested Results,
Unit discards, scoped defers, failures and context restoration are covered after
frontend disposal. Four additional literal migration classes now work: module
Path constants, exported literal keyword spans, Duration appends, and proven
literal-origin Bytes suffix/count bounds.

Package tests now validate producer-owned build-plan, generation-plan and
package-metadata DTOs instead of treating erased Records as typed JSON. Of the
119 original tests, 89 register with their original identities and effects;
30 remain blocked by errors in imported Kbuild/recipe code. Their test-owned
diagnostics are resolved. No Any annotation was added to bypass checking.
The package corpus is not claimed lint-clean or fully executable. Linux package
builds and proofs were not run. Laputa's stale `xsht check --strict` instruction
now names the sole ordinary checking contract.

Final verification for this reopened pass: 90 tooling library tests, 222 lint
integration tests, 65 API tests, 174 semantic tests, and 74 indexed tests pass.
The four requested read-only performance integration cases pass; configured
whole-repository lint is clean in 43.049 seconds against the unchanged
60-second wall budget. A first run exposed eleven diagnostics from newly
enabled safe literal/fallback eligibility, repaired through scoped source edits.
Eight changed native payload witnesses and the focused system-report controller
observation pass. The known package formatter-idempotence failure is retained;
no formatter CLI, repository autofix command, Linux package build, or proof ran.
