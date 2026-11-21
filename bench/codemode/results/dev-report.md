# Code mode report

- `dev-initial`: phase dev-initial, split dev, model `opencode-go/hy4-flash-free`, thinking requested `medium`, pi 1.5.0, 72 planned cells, seed 20261121, created 2026-11-21T06:56:30Z
- `dev-revision`: phase dev-revision, split dev, model `opencode-go/hy4-flash-free`, thinking requested `medium`, pi 1.5.0, 24 planned cells, seed 20261121, created 2026-11-21T08:23:09Z

Recorded cells: 96. Missing or incomplete cells: 0.

Discarded after an infrastructure failure and rerun: 10 trials, 10 model calls, 224,030 tokens (not in the tables below).

## All tasks

| Condition | Trials | First-attempt correct [95% CI] | Correct within 3 [95% CI] | Tokens per solved task [95% CI] | Est. cost per solved (pi, not provider) | Mean authoring s/trial | Static rejections | Runtime failures | Completed but wrong | Mean programs/trial |
|---|---|---|---|---|---|---|---|---|---|---|
| xsh-baseline | 24 | 70.8% [54.2, 87.5] | 95.8% [87.5, 100.0] | 50,538 [43,565, 58,813] | $0.3633 | 118 | 7/31 | 0/31 | 1/31 | 1.29 |
| xsh-candidate | 24 | 54.2% [41.7, 66.7] | 91.7% [79.2, 100.0] | 29,107 [22,530, 36,545] | $0.3537 | 142 | 14/38 | 1/38 | 1/38 | 1.58 |
| xsh-revised | 24 | 70.8% [50.0, 87.5] | 95.8% [87.5, 100.0] | 26,156 [19,401, 34,679] | $0.3205 | 123 | 6/32 | 2/32 | 1/32 | 1.33 |
| ts | 24 | 100.0% [100.0, 100.0] | 100.0% [100.0, 100.0] | 11,970 [8,737, 15,936] | $0.1708 | 75 | 0/24 | 0/24 | 0/24 | 1.00 |

| Comparison | Matched tasks | First-attempt difference [95% CI] | Final difference [95% CI] | Tokens per solved, ratio |
|---|---|---|---|---|
| xsh-candidate − xsh-baseline | 12 | -16.7% [-37.5, 4.2] | -4.2% [-12.5, 0.0] | 0.58 [0.42, 0.75] |
| xsh-revised − xsh-baseline | 12 | 0.0% [-29.2, 29.2] | 0.0% [-12.5, 12.5] | 0.52 [0.35, 0.73] |
| xsh-revised − xsh-candidate | 12 | 16.7% [0.0, 33.3] | 4.2% [-8.3, 16.7] | 0.90 [0.68, 1.13] |
| ts − xsh-baseline | 12 | 29.2% [12.5, 45.8] | 4.2% [0.0, 12.5] | 0.24 [0.16, 0.33] |
| ts − xsh-candidate | 12 | 45.8% [33.3, 58.3] | 8.3% [0.0, 20.8] | 0.41 [0.33, 0.49] |

## Shared-capability track

| Condition | Trials | First-attempt correct [95% CI] | Correct within 3 [95% CI] | Tokens per solved task [95% CI] | Est. cost per solved (pi, not provider) | Mean authoring s/trial | Static rejections | Runtime failures | Completed but wrong | Mean programs/trial |
|---|---|---|---|---|---|---|---|---|---|---|
| xsh-baseline | 12 | 66.7% [33.3, 91.7] | 91.7% [75.0, 100.0] | 52,283 [40,264, 68,260] | $0.3581 | 107 | 4/16 | 0/16 | 1/16 | 1.33 |
| xsh-candidate | 12 | 66.7% [50.0, 83.3] | 91.7% [75.0, 100.0] | 20,227 [15,594, 24,910] | $0.2415 | 105 | 3/15 | 0/15 | 1/15 | 1.25 |
| xsh-revised | 12 | 91.7% [75.0, 100.0] | 100.0% [100.0, 100.0] | 18,134 [14,059, 22,175] | $0.2199 | 91 | 1/13 | 0/13 | 0/13 | 1.08 |
| ts | 12 | 100.0% [100.0, 100.0] | 100.0% [100.0, 100.0] | 9,143 [6,936, 11,750] | $0.1261 | 56 | 0/12 | 0/12 | 0/12 | 1.00 |

| Comparison | Matched tasks | First-attempt difference [95% CI] | Final difference [95% CI] | Tokens per solved, ratio |
|---|---|---|---|---|
| xsh-candidate − xsh-baseline | 6 | 0.0% [-33.3, 33.3] | 0.0% [0.0, 0.0] | 0.39 [0.26, 0.54] |
| xsh-revised − xsh-baseline | 6 | 25.0% [-8.3, 58.3] | 8.3% [0.0, 25.0] | 0.35 [0.21, 0.55] |
| xsh-revised − xsh-candidate | 6 | 25.0% [8.3, 41.7] | 8.3% [0.0, 25.0] | 0.90 [0.67, 1.20] |
| ts − xsh-baseline | 6 | 33.3% [8.3, 66.7] | 8.3% [0.0, 25.0] | 0.17 [0.12, 0.26] |
| ts − xsh-candidate | 6 | 33.3% [16.7, 50.0] | 8.3% [0.0, 25.0] | 0.45 [0.38, 0.52] |

## Native track

| Condition | Trials | First-attempt correct [95% CI] | Correct within 3 [95% CI] | Tokens per solved task [95% CI] | Est. cost per solved (pi, not provider) | Mean authoring s/trial | Static rejections | Runtime failures | Completed but wrong | Mean programs/trial |
|---|---|---|---|---|---|---|---|---|---|---|
| xsh-baseline | 12 | 75.0% [58.3, 91.7] | 100.0% [100.0, 100.0] | 48,938 [42,337, 55,173] | $0.3681 | 129 | 3/15 | 0/15 | 0/15 | 1.25 |
| xsh-candidate | 12 | 41.7% [25.0, 50.0] | 91.7% [75.0, 100.0] | 37,988 [29,147, 48,103] | $0.4660 | 179 | 11/23 | 1/23 | 0/23 | 1.92 |
| xsh-revised | 12 | 50.0% [25.0, 75.0] | 91.7% [75.0, 100.0] | 34,907 [23,485, 47,795] | $0.4303 | 155 | 5/19 | 2/19 | 1/19 | 1.58 |
| ts | 12 | 100.0% [100.0, 100.0] | 100.0% [100.0, 100.0] | 14,796 [8,929, 21,562] | $0.2155 | 95 | 0/12 | 0/12 | 0/12 | 1.00 |

| Comparison | Matched tasks | First-attempt difference [95% CI] | Final difference [95% CI] | Tokens per solved, ratio |
|---|---|---|---|---|
| xsh-candidate − xsh-baseline | 6 | -33.3% [-50.0, -16.7] | -8.3% [-25.0, 0.0] | 0.78 [0.61, 1.01] |
| xsh-revised − xsh-baseline | 6 | -25.0% [-66.7, 16.7] | -8.3% [-25.0, 0.0] | 0.71 [0.48, 1.00] |
| xsh-revised − xsh-candidate | 6 | 8.3% [-25.0, 33.3] | 0.0% [-25.0, 25.0] | 0.92 [0.55, 1.23] |
| ts − xsh-baseline | 6 | 25.0% [8.3, 41.7] | 0.0% [0.0, 0.0] | 0.30 [0.18, 0.44] |
| ts − xsh-candidate | 6 | 58.3% [50.0, 75.0] | 8.3% [0.0, 25.0] | 0.39 [0.28, 0.48] |

Rates average samples within each task, then tasks; intervals bootstrap over tasks. Tokens per solved task divide every recorded token of the condition's trials, failed ones included, by its solved trials; uncached input + cache read + cache write + output (reasoning is inside output). Cost is pi's estimate (`totalFromProvider: false`), not a provider-reported cost.

## Per task

Each cell lists the attempts of each sample: ✓ correct, W completed but wrong, F runtime failure, R rejected by the checker, ∅ no answer, P provider error, I infrastructure error.

| Task | Track | xsh-baseline | xsh-candidate | xsh-revised | ts |
|---|---|---|---|---|---|
| dev-audit-errors | shared | W R/✓ | ✓ W | ✓ ✓ | ✓ ✓ |
| dev-ci-retry | shared | ✓ R/R/✓ | ✓ ✓ | ✓ ✓ | ✓ ✓ |
| dev-config-set | native | ✓ ✓ | R/R/✓ ✓ | ✓ R/F/✓ | ✓ ✓ |
| dev-config-sync | shared | ✓ ✓ | ✓ ✓ | ✓ ✓ | ✓ ✓ |
| dev-file-inventory | native | ✓ ✓ | R/✓ ✓ | R/✓ ✓ | ✓ ✓ |
| dev-host-probes | shared | ✓ ✓ | ✓ R/✓ | R/✓ ✓ | ✓ ✓ |
| dev-log-bundle | native | R/✓ ✓ | ✓ R/R/F | ✓ ✓ | ✓ ✓ |
| dev-parallel-sums | native | R/✓ ✓ | R/✓ R/✓ | ✓ W | ✓ ✓ |
| dev-prune-backups | native | ✓ ✓ | ✓ R/R/✓ | R/✓ R/R/✓ | ✓ ✓ |
| dev-restock | shared | ✓ ✓ | R/✓ ✓ | ✓ ✓ | ✓ ✓ |
| dev-run-jobs | native | ✓ R/✓ | R/R/✓ ✓ | F/✓ ✓ | ✓ ✓ |
| dev-ticket-import | shared | R/✓ ✓ | ✓ R/✓ | ✓ ✓ | ✓ ✓ |

## Accounting

| Condition | Model calls | Checks | Program runs | Uncached input | Cache read | Cache write | Output | of which reasoning | Unknown usage fields | Mean first-prompt input | Model s | Program s | Provider interruptions | Duplicate writes | Rejected service inputs | Reported thinking |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| xsh-baseline | 31 | 31 | 80 | 756,605 | 0 | 0 | 405,762 | 393,224 | 0 | 24,321 | 2,575 | 227 | 0 | 0 | 0 | max |
| xsh-candidate | 38 | 38 | 78 | 151,930 | 0 | 0 | 488,434 | 469,381 | 0 | 3,661 | 3,000 | 374 | 0 | 0 | 0 | max |
| xsh-revised | 32 | 32 | 82 | 137,620 | 0 | 0 | 463,968 | 447,609 | 0 | 4,006 | 2,856 | 75 | 0 | 0 | 0 | max |
| ts | 24 | 24 | 80 | 17,470 | 0 | 0 | 269,804 | 248,045 | 0 | 728 | 1,650 | 139 | 0 | 0 | 0 | max |

## Repairs

| Condition | Trials repaired | Correct after repair | Rejections then correct | Failures then correct |
|---|---|---|---|---|
| xsh-baseline | 6 | 6 | 6 | 0 |
| xsh-candidate | 10 | 9 | 9 | 0 |
| xsh-revised | 6 | 6 | 5 | 1 |
| ts | 0 | 0 | 0 | 0 |

| Condition | Diagnostic | Seen before a repair | Gone in the next program |
|---|---|---|---|
| xsh-baseline | `check.redundant-parens` | 4 | 4 |
| xsh-candidate | `check.redundant-parens` | 4 | 4 |
| xsh-revised | `parse.expected-expression` | 4 | 4 |
| xsh-candidate | `check.named-arg` | 3 | 3 |
| xsh-baseline | `check.named-arg` | 1 | 1 |
| xsh-baseline | `check.standard-module-shadow` | 1 | 1 |
| xsh-baseline | `check.type-mismatch` | 1 | 1 |
| xsh-baseline | `parse.expected-token` | 1 | 1 |
| xsh-candidate | `check.field-access` | 1 | 1 |
| xsh-candidate | `check.module-member` | 1 | 1 |
| xsh-candidate | `check.type-mismatch` | 1 | 1 |
| xsh-candidate | `check.unknown-method` | 1 | 1 |
| xsh-candidate | `check.unresolved-name` | 1 | 1 |
| xsh-candidate | `parse.expected-expression` | 1 | 1 |
| xsh-candidate | `parse.expected-token` | 1 | 1 |
| xsh-candidate | `parse.match-wildcard-arm` | 1 | 1 |
| xsh-revised | `check.field-access` | 1 | 1 |
| xsh-revised | `check.try-result` | 1 | 1 |
| xsh-revised | `parse.expected-ident` | 1 | 1 |

## External commands

Literal command heads in each trial's last program (`run CMD` in XSH, `spawn`/`execFile` with a literal program in TypeScript).

| Condition | Final programs that start external commands | Commands (programs using them) |
|---|---|---|
| xsh-baseline | 4/24 | `/work/bin/slowsum` 2, `mktemp` 1, `mv` 1, `sleep` 1 |
| xsh-candidate | 4/24 | `/work/bin/slowsum` 2, `mktemp` 2, `mv` 2, `rm` 2, `tar` 2 |
| xsh-revised | 2/24 | `/work/bin/slowsum` 2 |
| ts | 2/24 | `/work/bin/slowsum` 2 |

## Failure taxonomy

Every program that was not correct, classified by hand (`results/*-taxonomy.json` holds each one with a note).

| Category | xsh-baseline | xsh-candidate | xsh-revised | ts |
|---|---|---|---|---|
| syntax/binding | 6 | 8 | 4 | 0 |
| missing/misunderstood API | 0 | 3 | 0 | 0 |
| external-data/schema boundary | 0 | 0 | 0 | 0 |
| result/optional/error handling | 1 | 2 | 1 | 0 |
| process/argv/path | 0 | 1 | 1 | 0 |
| stream/concurrency | 0 | 0 | 0 | 0 |
| cleanup/partial effect | 0 | 0 | 0 | 0 |
| task logic | 1 | 1 | 2 | 0 |
| compiler/runtime defect | 0 | 0 | 1 | 0 |
| harness/provider | 0 | 1 | 0 | 0 |
