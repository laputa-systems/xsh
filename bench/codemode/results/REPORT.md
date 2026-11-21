# XSH code mode experiment: report

One model, `opencode-go/hy4-flash-free`, wrote XSH and TypeScript programs for 12
development and 8 held-out tasks and worked 4 session tasks interactively, on the XSH
toolchain built from `c68ac9594` and pinned by hash. These are empirical outcomes for
that model and toolchain, not coverage, correctness or isolation claims about XSH; 8
held-out tasks support a pilot conclusion only.

## Answers

**Can the agent write useful XSH today?** Yes, mostly after repairs, and at a high cost
next to TypeScript. On held-out tasks with today's documentation, 70.8% of trials
reached a correct program within three programs but only 16.7% on the first; TypeScript
managed 95.8% and 91.7%. The checker stops most XSH mistakes before anything runs: 31 of
52 held-out programs were rejected by `xsht check` and 2 failed at run time, and no arm
ever repeated a non-idempotent write.

**Which context worked best, and how much?** The selected 2,451-token guide
(`contexts/selected.md`, published as `docs/agent-context.md`) against the 22,766-token
current-documentation baseline: on held-out, +20.8 points correct within three programs
[−4.2, 45.8], +8.3 points on the first program [0.0, 16.7], and 61% fewer tokens per
solved task [47, 75]. On development it matched the baseline's correctness (95.8%) at 52%
of its tokens per solved task.

**What did held-out establish?** The guide passes the test registered in
`thresholds.json` before any held-out task existed, through the token target (−60.9%
against −20% required) with the correctness guard met (+20.8 points, interval lower
bound −4.2, inside the −5 allowance); the first-attempt target was missed (+8.3 against
+10). Over 8 tasks this is a pilot indication that the compact guide is cheaper at no
loss of correctness, not a demonstrated first-attempt gain.

**Where does XSH save or cost effort against TypeScript?** XSH costs more on every
measure here. On held-out it needed 2.3 times TypeScript's tokens per solved task with
the guide (6.0 times with today's docs) and 2.2 to 3.1 times the authoring time per
solved task. The cost is almost entirely first-draft rejections that a repair fixes:
named-argument rules, redundant parentheses, unknown fields and methods, strict typing.
What XSH saved was native file and process work staying in the language: selected-guide
programs started external commands in 4 of 24 final held-out programs (`mkdir`, `rm`),
baseline programs in 5 (`rm`, `chmod`, `stat`, `cmp` and others), no TypeScript program
shelled out, and XSH failures were contained (completed-but-wrong 2 of 48 and 2 of 52;
runtime failures 0 and 2).

**Did the guide work with normal API discovery?** Partly. In 8 sessions with `xsht api`, file editing, checking and running inside the boundary, the selected guide ended with 2 of 4 correct programs against 1 of 4 for the current docs, at 42% of the tokens per session (216k against 519k). The model leaned on discovery under both contexts (29 to 36 `xsht api` queries per session), but interactive access did not make up for missing facts: two selected-guide sessions ended on a program that did not check. One sample per task, so this is an indication only.

**Single most valuable next improvement.** Make `xsht api` tell the truth about argument
labels (XSH-162). Most module contracts say arguments are "positional-only", while
`Path.rename` requires `to:` and `csv.encode` requires `headers:`, `delimiter:`,
`columns:` and `escape_formulas:`. Following the text produced the most frequent
held-out diagnostic, `check.named-arg`: 18 programs before a repair, in both XSH
conditions.

## Method in brief

- pi 1.5.0 with SX's isolation flags: a fresh session, a fixed system prompt, no tools,
  extensions, MCP, skills or context files. `--thinking medium` was requested; every
  reply reported `max`.
- A static trial is at most three programs. A repair sees only what a user would: the
  checker's diagnostics, or the public variant's exit status and stderr. A program that
  completes but is wrong ends the trial. Every program is judged on every variant,
  hidden ones included, by an independent judge over final state, effects and service
  ledgers.
- Programs run in the pinned `xsh-test` image or `node@sha256:6bd9ab43…`, with
  `--network none`, a read-only root and 4 CPUs; a shared task's only reachable address
  is its fixture service. The XSH conditions differ only in their context file; every
  condition gets the same task text and equivalent API facts (`DESIGN.md`).
- Held-out: 8 tasks × {current docs, selected guide, TypeScript} × 3 samples = 72 cells,
  shuffled with a recorded seed, at concurrency 2. A fresh author with no development
  history wrote the held-out and session tasks from the author pack; I requalified them
  on the frozen toolchain (16 of 16 held-out and 8 of 8 session references pass every
  variant; all 42 + 20 wrong programs are rejected). The guide was frozen before any
  held-out task existed (`selection.md`, `selection-rule.md`).

Rates average samples within a task, then tasks. Brackets are 95% percentile intervals
from a bootstrap over tasks. Tokens per solved task divide every token of a condition's
trials, failed ones included, by its solved trials. Costs are pi's estimates
(`cost.totalFromProvider: false`), not provider-reported costs; the model is free to
call, and a missing cost would be unknown, not zero.

## Held-out results

| Condition | First program correct | Correct within 3 | Tokens per solved task | Est. cost per solved (pi) | Authoring s per solved |
|---|---|---|---|---|---|
| XSH, current docs | 16.7% [0.0, 41.7] | 70.8% [50.0, 91.7] | 130,862 [85,223, 220,367] | $1.04 | 351 |
| XSH, selected guide | 25.0% [4.2, 50.0] | 91.7% [75.0, 100] | 51,126 [39,077, 63,142] | $0.64 | 258 |
| TypeScript | 91.7% [79.2, 100] | 95.8% [87.5, 100] | 21,817 [13,370, 36,477] | $0.31 | 115 |

Pre-registered test, selected guide minus current docs over 8 matched tasks:

| Test | Value [95% CI] | Threshold | Met |
|---|---|---|---|
| First program correct | +8.3 points [0.0, 16.7] | ≥ +10 | no |
| Tokens per solved task | −60.9% [−46.9, −74.7] | ≥ 20% reduction | yes |
| Correct within 3 programs (guard) | +20.8 points [−4.2, 45.8] | ≥ −5 | yes |

By track (first program / within 3 / tokens per solved):

| Condition | Shared tool/data (4 tasks) | Native scripting (4 tasks) |
|---|---|---|
| XSH, current docs | 33.3% / 75.0% / 106,642 | 0.0% / 66.7% / 158,110 |
| XSH, selected guide | 33.3% / 100% / 41,276 | 16.7% / 83.3% / 62,946 |
| TypeScript | 91.7% / 100% / 14,602 | 91.7% / 91.7% / 29,687 |

The guide's effect by track, selected minus current docs: shared, no first-program gain
(0.0 [0.0, 0.0]), +25.0 points within three [8.3, 33.3], 0.39 times the tokens per
solved task; native, +16.7 first-program [0.0, 33.3], +16.7 within three [−25.0, 75.0],
0.40 times the tokens. The development runs showed the opposite first-attempt pattern
(native −25, shared +25 over 6 tasks each), so neither track's first-attempt effect is
stable across the two sets; the token reduction is.

Per task, the attempts of each sample (✓ correct, R rejected by the checker, F runtime
failure, W completed but wrong, P no answer):

| Task | Track | Current docs | Selected guide | TypeScript |
|---|---|---|---|---|
| held-catalog-export | shared | R/W · R/✓ · R/✓ | R/✓ · R/✓ · R/R/✓ | ✓ · ✓ · ✓ |
| held-dedupe-tree | native | R/✓ · R/✓ · R/✓ | R/R/✓ · R/✓ · R/R/✓ | ✓ · ✓ · ✓ |
| held-deploy-waves | shared | ✓ · ✓ · ✓ | ✓ · ✓ · ✓ | ✓ · ✓ · ✓ |
| held-dns-plan | shared | R/R/✓ · R/✓ · R/R/R | R/✓ · R/✓ · R/✓ | ✓ · ✓ · R/✓ |
| held-hosts-block | native | W · R/F/✓ · R/✓ | R/W · ✓ · R/W | ✓ · ✓ · ✓ |
| held-log-rotate | native | R/R/✓ · R/F/✓ · R/✓ | R/R/✓ · R/✓ · R/R/✓ | ✓ · ✓ · P |
| held-migrate | native | R/R/R · R/R/R · R/R/R | R/R/✓ · R/✓ · ✓ | ✓ · ✓ · ✓ |
| held-payout-verify | shared | ✓ · R/✓ · R/R/R | R/✓ · R/✓ · ✓ | ✓ · ✓ · ✓ |

The one TypeScript "P" (`held-log-rotate`, sample 2) is the model spending its whole
output budget on reasoning and returning no text (`stopReason: length`). The provider
answered normally, so it counts as a TypeScript failure and was not rerun. The trial loop
now labels such a stop "no answer" (commit `d87dd1044`). There were no infrastructure
failures and no provider interruptions in the held-out run.

The largest gap is `held-migrate` under the current documentation, 0 of 3: all nine
programs were rejected, seven of them for reading an exit code off `Status` as
`.code`, `.code()` or `.exit_code` (the method is `exit_code()`), and the rest for
showing or encoding a `Status` directly. The guide shows `out.status.exit_code()?`; with
it, 3 of 3 were solved.

## Held-out accounting

| Condition | Model calls | Checker runs | Program runs | Uncached input | Output (of which reasoning) | Model s | Program s | Est. cost (pi) |
|---|---|---|---|---|---|---|---|---|
| XSH, current docs | 52 | 52 | 86 | 1,310,283 | 914,377 (869,969) | 5,880 | 70 | $17.65 |
| XSH, selected guide | 48 | 48 | 105 | 227,126 | 897,647 (845,693) | 5,591 | 68 | $14.15 |
| TypeScript | 25 | 24 | 101 | 24,773 | 477,007 (444,467) | 2,559 | 72 | $7.23 |

The provider reported no cache reads or writes and no unknown usage field; reasoning is a
part of output and totals never count it twice. Model time dominates every arm; program
time is about a minute per condition. Output tokens, almost all reasoning, are similar
for the two XSH conditions; the guide's saving is the 24k-token context it does not
repeat on every call, and fewer failed trials. No duplicate write or rejected service
input occurred in any arm.

## Sessions

Each session gave the model its condition's context, the public task, and four actions
it writes as fenced blocks (`xsht api` selectors, write a file, `xsht check`, run with
arguments), executed in the same boundary against one persistent public fixture and
service; at most 8 model turns and 10 minutes of model and tool time; the final
`main.xsh` was then judged on every variant. One sample per task and condition, so this
is a usage indication, not a rate.

| Session | Current docs | Selected guide |
|---|---|---|
| session-du-summary (native) | wrong; 8 turns, 12 queries, 5 checks, 8 runs | rejected (stopped by time); 6 turns, 39 queries, 16 checks, 42 runs |
| session-encode-batch (native) | correct; 8 turns, 77 queries, 2 checks, 2 runs | correct; 3 turns, 49 queries, 39 checks, 5 runs |
| session-mirror-sync (shared) | no program written; 8 turns, 9 queries | correct; 8 turns, 24 queries, 3 checks, 2 runs |
| session-oncall-report (shared) | rejected; 8 turns, 17 queries, 2 checks | rejected; 8 turns, 30 queries, 8 checks, 10 runs |

| Condition | Correct | Mean turns | Mean queries | Mean returned reference bytes | Mean checks | Mean runs | Mean tokens per session |
|---|---|---|---|---|---|---|---|
| Current docs | 1 of 4 | 8.0 | 28.8 | 68,537 | 2.8 | 4.0 | 519,336 |
| Selected guide | 2 of 4 | 6.2 | 35.5 | 294,846 | 16.5 | 14.8 | 216,058 |

With the selected guide the model queried, checked and ran far more and used 42% of the
tokens; with the current documentation it spent its turns re-reading a 24k-token
context (`session-mirror-sync` used 1.14 million input tokens and never wrote a
program). The model often packed many actions into one reply (89 in one turn of
`session-encode-batch`). Two selected-guide sessions still ended on a program that did
not check. Native tool calls through pi did not work with this model (the provider
returned the calls as text), so sessions used harness-mediated actions; `README.md`
describes the protocol. `sessions-report.md` has the per-session accounting.

## Development results (summary)

Full development report: `INTERIM.md` and `dev-report.md`. 12 tasks × 2 samples:

| Condition | First program correct | Correct within 3 | Tokens per solved task | Est. cost per solved (pi) |
|---|---|---|---|---|
| XSH, current docs | 70.8% | 95.8% | 50,538 | $0.36 |
| XSH, candidate guide | 54.2% | 91.7% | 29,107 | $0.35 |
| XSH, revised guide (selected) | 70.8% | 95.8% | 26,156 | $0.32 |
| TypeScript | 100% | 100% | 11,970 | $0.17 |

The one revision added the facts behind the candidate's static rejections (redundant
parentheses are errors, stage options are named, enum values take a leading dot, host
predicates return `Result`, the catch-all arm is `else =>`, `Str` has no `len()`, ranges
are `range(n)`, a value-position `cd`/`env` block yields a `Result`), each tied to failed
programs in `dev-initial-taxonomy.json`. After the revision `not` as negation appeared
three times: the guide never shows `!`. One revision was allowed, so
`docs/getting-started.md` states it and the frozen guide does not.

## Defects and capability gaps

No task was beyond XSH: every held-out and session task has a passing XSH reference.
Product defects, with reproducers in `../defects/`, all reproduced on the frozen binary:

| ID | Ticket | Found by | Defect |
|---|---|---|---|
| D1 | XSH-160 (P2) | development references | An importer's `is mod.Facet` never matches; `retry on (is mod.Facet)` silently runs once |
| D6 | XSH-167 (P2) | held-out programs, both XSH conditions | Narrowing a `Str?` field of a match-bound record and reading the result later fails IR verification with an internal code at 1:1 |
| D3 | XSH-164 (P2) | revised-guide development program | A top-level `Str +=` checks but fails at run time; master's XSH-158 records the same top-level path for `Duration` |
| D4 | XSH-165 (P2) | held-out author | `sort-by(desc: true)` reverses equal keys despite its "stable" contract |
| D2 | XSH-161 (P3) | development, then held-out author | `Path.relative_to` returns a path outside its base unchanged, against its contract |
| D5 | XSH-166 (P4) | held-out author | A user type named like a built-in record cascades into a misleading mismatch |

Documentation gaps: `xsht api` misstates labels and omits defaults (XSH-162: the
"positional-only" text, the required `to:` and `csv.encode` labels, `fs.walk` obeying
`.gitignore` by default), and `check.redundant-parens` is a hard error no guide mentioned
(XSH-163; a third of the failed development programs, and 5 held-out programs under the
current docs).

Judge limitations the author recorded: `held-catalog-export` and `held-hosts-block`
detect an in-place rewrite by an unchanged inode, so a program that removes the file and
writes a new one may get a different inode and pass, although it is not one atomic
replacement. Correct programs are never rejected by this. The development task
`dev-config-set` uses a hard-link witness instead, which has no such gap.

## Infrastructure and provider failures

- Development: ten cells failed when Docker left check containers in the created state
  under host load. Their calls (10, 224,030 tokens) are reported separately and the
  cells were rerun after the boundary learned to retry a container that never started.
- Held-out: none.
- Sessions: one session (`session-encode-batch`, selected guide) lost its run when Docker never started a check container after its 8 turns; its 8 calls and 62,141 tokens are reported separately and the session was rerun from scratch after the session tools learned to record such a failure as infrastructure.
- No call hit a rate limit or a provider error across the 135 development, 125 held-out
  and 57 session model calls.

## Observed, hypothesized, not done

- Observed: the selected guide met the pre-registered token target and the correctness
  guard on 8 held-out tasks and missed the first-attempt target; TypeScript was far ahead
  of XSH on first-program correctness (+67 to +75 points) and needed 2.3 to 6 times fewer
  tokens per solved task.
- Hypothesis, untested: fixing XSH-162 and adding the negation and label facts to the
  guide would close much of the first-attempt gap. The held-out set is consumed; testing
  it needs new tasks.
- Not done: no second held-out set, no other model (by ruling), no more than one sample
  per session task, no provider-reported cost (none exists for this model).

## Reproduction

```sh
bench/codemode/codemode references --split held
bench/codemode/codemode judges --split held
bench/codemode/codemode report .work/codemode-runs/held --thresholds bench/codemode/thresholds.json
bench/codemode/codemode report .work/codemode-runs/sessions
bench/codemode/codemode report .work/codemode-runs/dev-initial .work/codemode-runs/dev-revision \
  --taxonomy bench/codemode/results/dev-initial-taxonomy.json \
  --taxonomy bench/codemode/results/dev-revision-taxonomy.json
```

Bulk records stay under `.work/codemode-runs/`; this directory commits the manifests
(`*-manifest.json`), the generated reports (`dev-report.md`, `held-report.md`,
`sessions-report.md`), the taxonomies and the selection records.
