# Joint corpus checker result

The corpus worktree starts at `ac4e04219fc8a822d379463cd14f67671e6002a2`.
The eligibility freeze is `a8d5bd910`. Compiler source and embedded standard
library are unchanged. This result reuses the qualified typing successor's
verification products from `d6a70a2374f01dfad1017c3ba54306e0fb121956`:

| Product | SHA-256 |
|---|---|
| xsh | `f19e7d500ec480f17312e3d242a40f76e74e11365e7a86e4ff848f21a32f7b78` |
| xsht | `a333f3452d76b92b0c88a8eee2ff3e8411ca20af8dd3aaf30a4b8562cd98a213` |

The pinned Linux musl image is
`sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`.
Checks run with four CPUs, 8 GiB memory, no swap, no network and UID 1000.
All scratch and subprocess temporary files stay under the corpus worktree's
ignored `.work` directory. No build, production edit or standard-library edit
is part of this result.

## Frozen counts

| Category | Frozen sites | Jointly removed | Restored eligible |
|---|---:|---:|---:|
| Eligible locals | 693 | 236 | 457 |
| Eligible internal parameters, returns and effects | 2,190 | 377 | 1,813 |
| Schema definitions | 4,364 | 0 | Not eligible |
| Exported interfaces and module contracts | 1,920 | 0 | Not eligible |
| Required entry decoding and implicit validation targets | 23 | 0 | Not eligible |
| Explicit entry effect bounds | 4 | 0 | Not eligible |
| All inventory sites | 9,194 | 613 | 2,270 |

The local removal rate is 34.05%. The internal rate is 17.21%. The targets of
95% and 85% remain unmet. A failing declaration group is restored together.
This conservative result does not establish that every restored site is
individually necessary or that the passing subset is maximal. No restored site
is removed from the denominator.

## Checker observations

`original-*` records all ten annotated XSH roots and the annotated Laputa root
passing. `all-removed-*` records both joint candidates rejecting after all
2,883 eligible sites are removed. Required Proc parameter syntax is the first
failure. No removal percentage is assigned to that failed candidate.

`joint-*` records all eleven roots passing ordinary checking and indexed
lowering with the 613-site joint candidate. `annotation-validation.tsv` records
per-file removal counts and verifies the exact retained AST edit spans across
all 48 loaded sources. Every schema and exported-interface span is protected.
No algorithm, cast, constructor, call, inserted type or runtime check is changed.

The intermediate restoration steps encounter ambiguous receiver types, empty
collections, narrowing and widening contexts, and inferred-call lowering
exclusions. They determine restoration choices, not the frozen eligibility
classes. The retained-site table identifies those declaration groups.

## Minimal capability witnesses

The source pairs are in
[capability-witnesses](../../typing-corpus/capability-witnesses/).
Their stderr files here are the actual qualified-product results.

| Pair | Annotated | Reduced | Interpretation |
|---|---|---|---|
| bytes | Pass | `check.result-fallback` | Str and Bytes both provide `byte_at(Int) -> Int?`; receiver ambiguity is real |
| slice | Pass | Pass | The retained Str return context selects the receiver |
| record | Pass | Pass | A typed return anchors the projected list field |
| issue | Pass | `check.local-inference` | Existing direct generic composition restriction excludes this body |
| replace | Pass | `compact.indexed-build` | A known Str helper anchors the input, but an earlier replace call lacks lowering evidence |
| split | Pass | `compact.indexed-build` | A known Str helper anchors the input, but a nested split lacks lowering evidence |

The last two pairs are compiler coverage gaps for parent review. This lane
makes no compiler repair and does not convert an ambiguous method into a
caller-trained signature. Neither pair uses a required Proc parameter or adds
an effect contract.

## Remaining acceptance

The named 26 executions of the retained thirteen behavior witnesses are prepared
but have not run. Their original expected-output, failure, operand-order,
selection and cleanup assertions must pass in both source views. Checker success
alone does not establish behavior parity. No main merge, general inference
completion, resource acceptance or performance acceptance is claimed here.
