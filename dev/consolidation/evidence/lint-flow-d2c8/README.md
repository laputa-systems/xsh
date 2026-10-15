# Shared lint flow checks

Merged implementation: `d2c842812f87ddc13a84ab77b151b2a3ef0bd2ea`.
The executable reference is `5366d950a`. Flow transfers now read completed
child summaries from the common lint walk. The separate recursive flow walk
is removed. Source-form sugar transfers retain the reference's expansion
behavior and diagnostic order. Facts belong to one source and exist only
when `lint.dead-code` is enabled.

The five-file diff adds 490 lines and removes 230. Physical non-test Rust
adds 412 and removes 230. The private invariant unit adds 32 lines, and the
native fixture adds 46. These are source counts, not performance results.

Focused replay uses the restored lane products whose source matches the
merged Rust changes. The pinned Linux musl runtime has eight CPUs, 16 GiB,
no swap, disk-backed scratch, and no runtime preload. Each complete test log
retains the command, output, and status:

- [Dead-code native file](native-dead-code.log): 25 passed, zero failed.
- [Guard native file](native-guards.log): three passed, zero failed.
- [Exact private invariant](private-flow-invariant.log): one passed,
  411 filtered. Duplicate expression summary insertion must panic.
- [Products](products.log): exact hashes and byte sizes. Candidate `xsht`
  is 38,773,704 bytes; reference `xsht` is 38,740,888 bytes. Both arms use
  the same `xsh` product.

The lane's pinned verification build and debug unit compilation passed with
warnings denied. Their output remains in the tool transcript. The lane also
mutated `pattern_condition_flow` to return `FlowSummary::terminating()`.
The exact pattern-fallback oracle failed because line five became unreachable.
After restoration and rebuilding, that exact oracle passed. A prior mutation
that only removed the `terminates` bit survived this oracle and supplies no
negative-test evidence. These mutation outputs remain in the tool transcript;
the logs above are fresh passing replays.

The sixteen `lint-flow-final-*` files retain complete paired stdout and stderr
for repository and Laputa `check .` and `lint .`. The lane reports all eight
statuses as zero. The integrator independently checks all four pairs after
validating and removing only the final timing footer. Stdout and diagnostic
bodies match byte for byte. Repository lint checks 1,524 files and reports
three findings and 113 notes. Laputa checks 316 files and reports two findings
and 78 notes. Laputa remains at `ca658ce1da9d89c55209c25268dfdf687ff4fdc6`.

Independent static review finds no flow-equation, child-publication, source
gating, or diagnostic-order blocker. Final canonical, route, platform, and
performance acceptance remain pending on a fresh merged-source freeze.
