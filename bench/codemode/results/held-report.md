# Code mode report

- `held`: phase held, split held, model `opencode-go/hy4-flash-free`, thinking requested `medium`, pi 1.5.0, 72 planned cells, seed 20261121, created 2026-11-21T10:46:07Z

Recorded cells: 72. Missing or incomplete cells: 0.

Infrastructure or provider failures (kept in the counts as not correct):

- held-log-rotate/ts/2: ['provider-error']

## All tasks

| Condition | Trials | First-attempt correct [95% CI] | Correct within 3 [95% CI] | Tokens per solved task [95% CI] | Est. cost per solved (pi, not provider) | Mean authoring s/trial | Static rejections | Runtime failures | Completed but wrong | Mean programs/trial |
|---|---|---|---|---|---|---|---|---|---|---|
| xsh-baseline | 24 | 16.7% [0.0, 41.7] | 70.8% [50.0, 91.7] | 130,862 [85,223, 220,367] | $1.0380 | 249 | 31/52 | 2/52 | 2/52 | 2.17 |
| xsh-selected | 24 | 25.0% [4.2, 50.0] | 91.7% [75.0, 100.0] | 51,126 [39,077, 63,142] | $0.6430 | 237 | 24/48 | 0/48 | 2/48 | 2.00 |
| ts | 24 | 91.7% [79.2, 100.0] | 95.8% [87.5, 100.0] | 21,817 [13,370, 36,477] | $0.3143 | 111 | 1/24 | 0/24 | 0/24 | 1.04 |

| Comparison | Matched tasks | First-attempt difference [95% CI] | Final difference [95% CI] | Tokens per solved, ratio |
|---|---|---|---|---|
| xsh-selected − xsh-baseline | 8 | 8.3% [0.0, 16.7] | 20.8% [-4.2, 45.8] | 0.39 [0.25, 0.53] |
| ts − xsh-baseline | 8 | 75.0% [50.0, 95.8] | 25.0% [0.0, 50.0] | 0.17 [0.09, 0.30] |
| ts − xsh-selected | 8 | 66.7% [45.8, 83.3] | 4.2% [-12.5, 25.0] | 0.43 [0.31, 0.63] |

## Shared-capability track

| Condition | Trials | First-attempt correct [95% CI] | Correct within 3 [95% CI] | Tokens per solved task [95% CI] | Est. cost per solved (pi, not provider) | Mean authoring s/trial | Static rejections | Runtime failures | Completed but wrong | Mean programs/trial |
|---|---|---|---|---|---|---|---|---|---|---|
| xsh-baseline | 12 | 33.3% [0.0, 75.0] | 75.0% [66.7, 91.7] | 106,642 [58,134, 156,750] | $0.8243 | 207 | 13/23 | 0/23 | 1/23 | 1.92 |
| xsh-selected | 12 | 33.3% [0.0, 75.0] | 100.0% [100.0, 100.0] | 41,276 [27,645, 54,908] | $0.5187 | 206 | 9/21 | 0/21 | 0/21 | 1.75 |
| ts | 12 | 91.7% [75.0, 100.0] | 100.0% [100.0, 100.0] | 14,602 [9,046, 20,158] | $0.2020 | 86 | 1/13 | 0/13 | 0/13 | 1.08 |

| Comparison | Matched tasks | First-attempt difference [95% CI] | Final difference [95% CI] | Tokens per solved, ratio |
|---|---|---|---|---|
| xsh-selected − xsh-baseline | 4 | 0.0% [0.0, 0.0] | 25.0% [8.3, 33.3] | 0.39 [0.31, 0.49] |
| ts − xsh-baseline | 4 | 58.3% [16.7, 91.7] | 25.0% [8.3, 33.3] | 0.14 [0.10, 0.19] |
| ts − xsh-selected | 4 | 58.3% [16.7, 91.7] | 0.0% [0.0, 0.0] | 0.35 [0.29, 0.41] |

## Native track

| Condition | Trials | First-attempt correct [95% CI] | Correct within 3 [95% CI] | Tokens per solved task [95% CI] | Est. cost per solved (pi, not provider) | Mean authoring s/trial | Static rejections | Runtime failures | Completed but wrong | Mean programs/trial |
|---|---|---|---|---|---|---|---|---|---|---|
| xsh-baseline | 12 | 0.0% [0.0, 0.0] | 66.7% [25.0, 100.0] | 158,110 [87,270, 531,062] | $1.2785 | 291 | 18/29 | 2/29 | 1/29 | 2.42 |
| xsh-selected | 12 | 16.7% [0.0, 33.3] | 83.3% [50.0, 100.0] | 62,946 [49,879, 76,462] | $0.7922 | 267 | 15/27 | 0/27 | 2/27 | 2.25 |
| ts | 12 | 91.7% [75.0, 100.0] | 91.7% [75.0, 100.0] | 29,687 [16,110, 62,497] | $0.4369 | 135 | 0/11 | 0/11 | 0/11 | 1.00 |

| Comparison | Matched tasks | First-attempt difference [95% CI] | Final difference [95% CI] | Tokens per solved, ratio |
|---|---|---|---|---|
| xsh-selected − xsh-baseline | 4 | 16.7% [0.0, 33.3] | 16.7% [-25.0, 75.0] | 0.40 [0.12, 0.60] |
| ts − xsh-baseline | 4 | 91.7% [75.0, 100.0] | 25.0% [-16.7, 75.0] | 0.19 [0.04, 0.49] |
| ts − xsh-selected | 4 | 75.0% [66.7, 91.7] | 8.3% [-25.0, 50.0] | 0.47 [0.28, 0.86] |

Rates average samples within each task, then tasks; intervals bootstrap over tasks. Tokens per solved task divide every recorded token of the condition's trials, failed ones included, by its solved trials; uncached input + cache read + cache write + output (reasoning is inside output). Cost is pi's estimate (`totalFromProvider: false`), not a provider-reported cost.

## Per task

Each cell lists the attempts of each sample: ✓ correct, W completed but wrong, F runtime failure, R rejected by the checker, ∅ no answer, P provider error, I infrastructure error.

| Task | Track | xsh-baseline | xsh-selected | ts |
|---|---|---|---|---|
| held-catalog-export | shared | R/W R/✓ R/✓ | R/✓ R/✓ R/R/✓ | ✓ ✓ ✓ |
| held-dedupe-tree | native | R/✓ R/✓ R/✓ | R/R/✓ R/✓ R/R/✓ | ✓ ✓ ✓ |
| held-deploy-waves | shared | ✓ ✓ ✓ | ✓ ✓ ✓ | ✓ ✓ ✓ |
| held-dns-plan | shared | R/R/✓ R/✓ R/R/R | R/✓ R/✓ R/✓ | ✓ ✓ R/✓ |
| held-hosts-block | native | W R/F/✓ R/✓ | R/W ✓ R/W | ✓ ✓ ✓ |
| held-log-rotate | native | R/R/✓ R/F/✓ R/✓ | R/R/✓ R/✓ R/R/✓ | ✓ ✓ P |
| held-migrate | native | R/R/R R/R/R R/R/R | R/R/✓ R/✓ ✓ | ✓ ✓ ✓ |
| held-payout-verify | shared | ✓ R/✓ R/R/R | R/✓ R/✓ ✓ | ✓ ✓ ✓ |

## Accounting

| Condition | Model calls | Checks | Program runs | Uncached input | Cache read | Cache write | Output | of which reasoning | Unknown usage fields | Mean first-prompt input | Model s | Program s | Provider interruptions | Duplicate writes | Rejected service inputs | Reported thinking |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| xsh-baseline | 52 | 52 | 86 | 1,310,283 | 0 | 0 | 914,377 | 869,969 | 0 | 24,376 | 5,880 | 70 | 0 | 0 | 0 | max |
| xsh-selected | 48 | 48 | 105 | 227,126 | 0 | 0 | 897,647 | 845,693 | 0 | 4,059 | 5,591 | 68 | 0 | 0 | 0 | max |
| ts | 25 | 24 | 101 | 24,773 | 0 | 0 | 477,007 | 444,467 | 0 | 906 | 2,559 | 72 | 0 | 0 | 0 | max |

## Repairs

| Condition | Trials repaired | Correct after repair | Rejections then correct | Failures then correct |
|---|---|---|---|---|
| xsh-baseline | 19 | 13 | 13 | 0 |
| xsh-selected | 18 | 16 | 16 | 0 |
| ts | 1 | 1 | 1 | 0 |

| Condition | Diagnostic | Seen before a repair | Gone in the next program |
|---|---|---|---|
| xsh-baseline | `check.named-arg` | 9 | 9 |
| xsh-selected | `check.named-arg` | 9 | 9 |
| xsh-baseline | `check.unknown-field` | 7 | 5 |
| xsh-selected | `check.type-mismatch` | 6 | 6 |
| xsh-baseline | `check.redundant-parens` | 5 | 5 |
| xsh-baseline | `compact.indexed-build` | 4 | 3 |
| xsh-selected | `check.field-access` | 4 | 4 |
| xsh-selected | `check.index-type` | 4 | 4 |
| xsh-selected | `parse.expected-expression` | 4 | 4 |
| xsh-selected | `check.destructure-field` | 3 | 3 |
| xsh-baseline | `check.standard-module-shadow` | 2 | 2 |
| xsh-baseline | `check.type-mismatch` | 2 | 2 |
| xsh-baseline | `parse.expected-expression` | 2 | 1 |
| xsh-baseline | `parse.expected-terminator` | 2 | 1 |
| xsh-selected | `check.standard-module-shadow` | 2 | 2 |
| xsh-selected | `check.stream-sort` | 2 | 2 |
| xsh-selected | `compact.indexed-build` | 2 | 2 |
| ts | `TS2339` | 1 | 1 |
| xsh-baseline | `check.bare-print-ident` | 1 | 1 |
| xsh-baseline | `check.display-conversion` | 1 | 1 |
| xsh-baseline | `check.duplicate-name` | 1 | 1 |
| xsh-baseline | `check.schema-field` | 1 | 1 |
| xsh-baseline | `check.stream-sort` | 1 | 1 |
| xsh-baseline | `lex.invalid-escape` | 1 | 1 |
| xsh-baseline | `parse.expected-ident` | 1 | 1 |
| xsh-baseline | `parse.expected-token` | 1 | 1 |
| xsh-baseline | `parse.match-else-arm` | 1 | 1 |
| xsh-selected | `check.conversion` | 1 | 1 |
| xsh-selected | `check.display-conversion` | 1 | 1 |
| xsh-selected | `check.operator-type` | 1 | 1 |
| xsh-selected | `check.try-context` | 1 | 1 |
| xsh-selected | `check.unknown-field` | 1 | 1 |
| xsh-selected | `check.unknown-method` | 1 | 1 |
| xsh-selected | `parse.bare-path-literal` | 1 | 1 |
| xsh-selected | `parse.expected-terminator` | 1 | 1 |

## External commands

Literal command heads in each trial's last program (`run CMD` in XSH, `spawn`/`execFile` with a literal program in TypeScript).

| Condition | Final programs that start external commands | Commands (programs using them) |
|---|---|---|
| xsh-baseline | 5/24 | `rm` 4, `chmod` 2, `stat` 2, `cat` 1, `cmp` 1, `grep` 1, `head` 1, `mktemp` 1, `mv` 1, `printf` 1, `tail` 1 |
| xsh-selected | 4/24 | `mkdir` 3, `rm` 1 |
| ts | 0/24 | none |

## Pre-registered held-out test

| Pre-registered test (thresholds.json) | Value, selected − current docs [95% CI] | Threshold | Met |
|---|---|---|---|
| First-attempt correct | +8.3 [+0.0, +16.7] points | ≥ +10 | no |
| Reduction in tokens per solved task | +60.9 [+46.9, +74.7] % | ≥ 20% | yes |
| Correct within 3 programs (guard) | +20.8 [-4.2, +45.8] points | ≥ −5 | yes |

Point estimates over 8 matched tasks: the selected guide PASSES the pre-registered test. An interval that spans the threshold makes the result a pilot indication, not a conclusion.
