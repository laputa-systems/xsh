# Isolated generic module export acceptance

The approved [module export slice](../../typing-first-slice.md) passes on
`8475984a9ce1e8dbc987a8cf409f0747583c4878`, tree
`2d2dfebdb892fa6dbae70db5fd38199489321fdf`, in the clean
`campaign/typing-pipeline-integration` worktree. The slice remains isolated.
Main integration waits for consolidation acceptance.

The public `ModuleExportType::GenericPure { optional }` marker uses export
kind 3, optional bit 2, and `IR_NONE`. The verifier requires an exported,
child-owned Pure scheme with quantified binders. Generic function values
remain rejected, including optional member access, `get`, and indexing.
Qualified generic calls retain their existing target authority.

Value exports carry lexical binding identities. The verifier follows the
unique writer for that identity within the module owner, including imports
whose shared initialization was already expanded. It checks visibility, name,
Let ownership, initializer ownership, and the registered value type. Runtime
retention uses the existing program-namespaced binding store. No new retention
map or counter was added.

Independent review found no remaining defect in the bounded export repair.
The joint-import regression now passes both import orders with a private
same-named marker. Negative tests retain exact export, scheme, payload,
visibility, and forged-Any rejection. The prepared-program frontend-disposal
fixture reads the exported marker on both indexed execution routes.

## Verification

All checks use pinned x86_64 Linux musl image
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
Builds use kache and mold. No formatter or CI workflow ran.

- [Rust selectors](rust-selectors.txt): 289 selections, 288 distinct tests,
  all passing. One repeated selection comes from the retained baseline.
  [The runner](run-rust.sh) rejects empty matches and requires one passing
  test for every selector. [Raw results](rust-results.log) contain 289 passing
  results. Its stale failure text says 288, but the executed count check is 289.
- [Production warning denial](warning-denied.stderr): exit 0 for
  `focused-check.sh final-prod-warning-denied-8475984a9 cargo rustc --locked --offline --profile verification --target x86_64-unknown-linux-musl -p xsh --lib -- -D warnings`.
  [The wrapper](focused-check.sh) supplies the pinned build environment.
  `CARGO_BUILD_WARNINGS` alone is not treated as warning denial.
- [Verification products](products.stderr): xsh, xsht, and xshi built
  successfully. [The build command](build-products.sh) records the environment.
- [Four native invocations](run-native.sh): capability 7, rank-1 12, and two
  exact size-literal controls pass, for 21 tests total. Their
  [summary](native/summary.tsv) and four stdout files retain the results.
  The runtime uses four CPUs, 8 GiB, no swap, no network, and the shared
  native-suite lock.

The product and native source snapshots agree before and after execution.
Native product hashes also agree. The warning command has no separate source
snapshot. Its logs, subsequent product snapshots, and clean final checkout
bind this result to the same committed source. In the retained snapshots,
the empty Git status is followed immediately by the Cargo.lock hash on the
`status=` line.

The change from `e2cf1fec2` touches 32 paths, adding 1,239 lines and deleting
263. Twenty-five non-test paths add 1,169 and delete 259 lines. Seven test
paths add 70 and delete four lines. Source budgets remain advisory.

## Limits

This acceptance covers the first module-export slice. It does not establish
broader rank-1, row, recursion, effect, annotation-removal, corpus, or performance
goals. Production uses `generalize_callable_body` and `instantiate_callable`.
Test-only rigid and skolem coverage does not establish production support for
those features. Retained semantic byte estimates are not RSS or one live graph.
No speedup is claimed. Lean and Phase F remain paused by the owner.
