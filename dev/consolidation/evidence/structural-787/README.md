# Structural measures at 787

The existing scanner exits zero on clean source
`787ba37a43e808c6b63b85b17cc912fe36352b22`, tree
`31681c63f02b825b4352017482230a33e78a819d`. Source identity and clean status
match before and after. Scanner sources are unchanged since the retained 4d7 scan.

The four required zeros remain zero: manual payload calls, optional raw calls,
wide arena wildcards, and blanket dead-code lines. There are 152 instructions,
17 patterns, 46 stages, one predicate, two storage mappings, and five production
factory calls. Semantic bound review remains separate from that last count.

Production source totals 268,539 lines: 215,360 under `src` and 53,179 under
`crates/xsht/src`. That is 35 more retained lines than 4d7, all in the process
sampler, and 60 above the original qualified baseline. The 372 files contain
315,753 physical lines. The scanner correctly excludes the 106-line Linux test
module guarded by `cfg(all(test, target_os = "linux"))`; no metric policy changes.

The command is the existing `metrics.xsh measure` with `--root` at the repository,
`--source-root src`, and `--xsht-source-root crates/xsht/src`. Its running ELF is
pinned through `/proc/20954/exe`, SHA-256
`a9117c2905afe522fe5eb7b78a21bf2bd6e9d1fa567473bdda548be7e1f24d0b`.
The target pathname is replaced during the scan by the product build, so its
later digest is not the executed digest. Build feature identity is unknown;
this report does not identify the runner as the final default product or give
timing credit. A prior scan overlapping the main source merge was stopped
without output and discarded.
