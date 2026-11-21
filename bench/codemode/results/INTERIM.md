# Code mode experiment: interim development report

Phase A only: development tasks, guide development and selection. No held-out or
session task exists yet, so nothing here answers the held-out questions. These are
empirical outcomes for one model on a pinned toolchain, not coverage or correctness
claims about XSH.

## Setup

- Toolchain: `xsh` and `xsht` built once with `--profile verification` from the lane base
  `c68ac9594`; SHA-256 `08d7fe15…5074` (`xsh`) and `a4a5c9be…4e94` (`xsht`), checked by
  every run.
- Boundary: the pinned `xsh-test` image (`fed05abf…`) and `node@sha256:6bd9ab43…` with
  TypeScript 7.0.2 and `@types/node` 26.6.6; `--network none`, read-only root, 4 CPUs;
  shared tasks reach only their fixture service. See `DESIGN.md`.
- Model: `opencode-go/hy4-flash-free` through pi 1.5.0 with SX's isolation flags,
  `--thinking medium` requested; every reply reports `thinkingLevel: max`. Costs are
  pi's estimates (`totalFromProvider: false`), not provider-reported costs.
- Contexts: current documentation 22,766 tokens (`baseline.md`), candidate 2,107,
  revision 2,451, measured as input tokens over a one-line probe.
- Runs: `dev-initial` (12 tasks × {baseline, candidate, TypeScript} × 2 = 72 cells,
  shuffled, concurrency 2) and `dev-revision` (12 × revised × 2 = 24). Manifests are in
  this directory; `dev-report.md` is the generated report.

## Qualification

- All 24 references (12 tasks × XSH and TypeScript) pass every variant (40 variants per
  arm), rerun on the final harness.
- All 44 plausible wrong programs (missing output, omitted pages, duplicate or retried
  non-idempotent writes, wrong order, swallowed errors, shell-joined argv, in-place
  writes, leftover temporary files, unbounded or sequential concurrency, wrong
  traversal policy) complete on the public variant and are rejected by their judges.
- Isolation, fixture reset, hidden-answer separation, usage accounting and
  interruption/resume are proven by the probes in `probes/`; see the proof record in the
  lane handoff.

## Results

Rates average samples within a task, then tasks; brackets are 95% intervals from a
bootstrap over tasks. Tokens per solved task count every token of the condition's
trials, failed ones included.

| Condition | First-attempt correct | Correct within 3 programs | Tokens per solved task | Static rejections | Runtime failures | Completed but wrong |
|---|---|---|---|---|---|---|
| XSH, current docs | 70.8% [54.2, 87.5] | 95.8% [87.5, 100] | 50,538 [43,565, 58,813] | 7/31 | 0/31 | 1/31 |
| XSH, candidate guide | 54.2% [41.7, 66.7] | 91.7% [79.2, 100] | 29,107 [22,530, 36,545] | 14/38 | 1/38 | 1/38 |
| XSH, revised guide (selected) | 70.8% [50.0, 87.5] | 95.8% [87.5, 100] | 26,156 [19,401, 34,679] | 6/32 | 2/32 | 1/32 |
| TypeScript | 100% [100, 100] | 100% [100, 100] | 11,970 [8,737, 15,936] | 0/24 | 0/24 | 0/24 |

By track (first-attempt / final / tokens per solved):

| Condition | Shared tool/data (6 tasks) | Native scripting (6 tasks) |
|---|---|---|
| XSH, current docs | 66.7% / 91.7% / 52,283 | 75.0% / 100% / 48,938 |
| XSH, candidate | 66.7% / 91.7% / 20,227 | 41.7% / 91.7% / 37,988 |
| XSH, revised | 91.7% / 100% / 18,134 | 50.0% / 91.7% / 34,907 |
| TypeScript | 100% / 100% / 9,143 | 100% / 100% / 14,796 |

Matched differences (task-clustered):

- Revised − current docs: first attempt 0.0 [−29.2, 29.2], final 0.0 [−12.5, 12.5],
  tokens per solved ×0.52 [0.35, 0.73].
- Revised − candidate: first attempt +16.7 [0.0, 33.3], tokens per solved ×0.90.
- TypeScript − current docs: first attempt +29.2 [12.5, 45.8], tokens ×0.24.

Per task and per sample, attempts as ✓ correct, R rejected, F failed, W wrong:

| Task | Track | Current docs | Candidate | Revised | TypeScript |
|---|---|---|---|---|---|
| dev-audit-errors | shared | W · R/✓ | ✓ · W | ✓ · ✓ | ✓ · ✓ |
| dev-ci-retry | shared | ✓ · R/R/✓ | ✓ · ✓ | ✓ · ✓ | ✓ · ✓ |
| dev-config-set | native | ✓ · ✓ | R/R/✓ · ✓ | ✓ · R/F/✓ | ✓ · ✓ |
| dev-config-sync | shared | ✓ · ✓ | ✓ · ✓ | ✓ · ✓ | ✓ · ✓ |
| dev-file-inventory | native | ✓ · ✓ | R/✓ · ✓ | R/✓ · ✓ | ✓ · ✓ |
| dev-host-probes | shared | ✓ · ✓ | ✓ · R/✓ | R/✓ · ✓ | ✓ · ✓ |
| dev-log-bundle | native | R/✓ · ✓ | ✓ · R/R/F | ✓ · ✓ | ✓ · ✓ |
| dev-parallel-sums | native | R/✓ · ✓ | R/✓ · R/✓ | ✓ · W | ✓ · ✓ |
| dev-prune-backups | native | ✓ · ✓ | ✓ · R/R/✓ | R/✓ · R/R/✓ | ✓ · ✓ |
| dev-restock | shared | ✓ · ✓ | R/✓ · ✓ | ✓ · ✓ | ✓ · ✓ |
| dev-run-jobs | native | ✓ · R/✓ | R/R/✓ · ✓ | F/✓ · ✓ | ✓ · ✓ |
| dev-ticket-import | shared | R/✓ · ✓ | ✓ · R/✓ | ✓ · ✓ | ✓ · ✓ |

## Accounting

| Condition | Model calls | Checker runs | Program runs | Uncached input | Output (of which reasoning) | Model s | Program s |
|---|---|---|---|---|---|---|---|
| XSH, current docs | 31 | 31 | 80 | 756,605 | 405,762 (393,224) | 2,575 | 227 |
| XSH, candidate | 38 | 38 | 78 | 151,930 | 488,434 (469,381) | 3,000 | 374 |
| XSH, revised | 32 | 32 | 82 | 137,620 | 463,968 (447,609) | 2,856 | 75 |
| TypeScript | 24 | 24 | 80 | 17,470 | 269,804 (248,045) | 1,650 | 139 |

The provider reported no cache reads or writes and every usage field on all 135
attempts; reasoning is a part of output, and totals never add it twice. Output, mostly
reasoning at the reported `max` level, dominates every condition: the current docs
cost 24k input tokens per call but the guides save mainly by needing fewer repairs and
less output per solved task. Ten cells hit a Docker start failure under host load (the
checker timed out) and were rerun with the start-retry boundary; their 10 calls and
224,030 tokens are listed separately and are not in the tables. There were no provider
interruptions and no rate limits; throughput at concurrency 2 was about one cell a
minute.

## Failures and the revision

Every program that was not correct is classified with a note in
`dev-initial-taxonomy.json` and `dev-revision-taxonomy.json`:

| Category | Current docs | Candidate | Revised | TypeScript |
|---|---|---|---|---|
| syntax/binding | 6 | 8 | 4 | 0 |
| missing/misunderstood API | 0 | 3 | 0 | 0 |
| result/optional/error handling | 1 | 2 | 1 | 0 |
| process/argv/path | 0 | 1 | 1 | 0 |
| task logic | 1 | 1 | 2 | 0 |
| compiler/runtime defect | 0 | 0 | 1 | 0 |
| harness/provider | 0 | 1 | 0 | 0 |

The candidate's static rejections came from facts it did not state; the one revision
added exactly those, and each is tied to failed programs:

| Fact added | Failed programs before (current docs + candidate) | Same code after revision |
|---|---|---|
| Redundant parentheses are a check error | 8 | 0 |
| Stage options are named (`par-map(jobs: 3)`) | 4 | 0 |
| Enum values take a leading dot (`.Tar`) | 2 | 0 |
| Host predicates return `Result` | 1 | 0 |
| The catch-all arm is `else =>` | 1 | 0 |
| `Str` has no `len()`; ranges are `range(n)` | 2 | 0 |
| A value-position `cd`/`env` block yields a `Result` | 1 | 0 |
| Retry patterns join variants with a vertical bar | 1 | 0 |

Diagnostics disappearing is not the same as repairs becoming correct: every rejected
program in every XSH condition was repaired to a correct program within the budget
except the candidate's `dev-log-bundle` sample, which ran out on a BusyBox `tar` option.
After the revision a new mistake appeared: `not` as negation (3 programs), plausibly
prompted by the revision's own `id not in seen` example; the guide never shows `!`. One
revision was allowed, so this is recorded for the published guide's follow-up.

Selection (`selection-rule.md`, fixed before the revision results were read): all three
XSH contexts kept final correctness within 5 points of the best (95.8%), and the
revision had the lowest tokens per solved task, so `revised.md` is frozen as
`contexts/selected.md` (SHA-256 `b000c759…d357`, 8,178 bytes, 2,451 tokens).

## Defects found

- D1 (P2): an importer's `is mod.Facet` never matches an imported error's facet, so
  `retry ... on (is mod.Transient)` silently runs once. Found while writing references;
  the bindings declare no facets (policy set before any scored call).
- D3 (P2): a top-level `Str +=` statement checks but fails at run time. Hit by one
  revised-guide program during scoring.
- D2 (P3): `Path.relative_to` returns a path outside its base unchanged, contrary to its
  contract.

Reproducers are in `../defects/`; TODO.md tracks them as XSH-160, XSH-164 and XSH-161 (renumbered after merging master).

## Ranked follow-ups

1. Fix D1 and D3; both silently change behavior of programs that check.
2. Decide whether `check.redundant-parens` should stay a hard error (XSH-163): it alone
   caused a third of the failed XSH programs under both the current docs and the
   candidate.
3. Add negation (`!`, and that `not` exists only in `not in`) to the published agent
   context's next version; the getting-started page already states it.
4. Correct `xsht api` contract text on named arguments and print defaults, starting with
   `fs.walk`'s `gitignore` (XSH-162).
5. Resolve D2's contract.

## What development evidence says, and what it does not

- Observed: with current documentation the model writes correct XSH for 96% of trials
  within three programs, 71% on the first, against 100% for TypeScript, at about four
  times TypeScript's tokens per solved task.
- Observed: the selected 2,451-token guide matched the current documentation's
  correctness at about half the tokens per solved task; the candidate matched final
  correctness only within the 5-point band and lost 17 points on first attempts.
- Observed: XSH's native track cost more than its shared track for every guide, and the
  compact guides lost first-attempt correctness on native tasks against the current
  docs (−25 points for the revision), while gaining it on shared tasks (+25).
- Hypothesis, not tested: that the native-track gap comes from facts the tour teaches
  and the guide omits (scopes, enums, archives). The held-out evaluation, run once with
  the frozen guide, is what can establish whether any of this generalizes.
