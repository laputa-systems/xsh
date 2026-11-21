# Code mode experiment: design note

The experiment measures how well one model writes XSH from documentation alone, and
whether a compact guide beats the current documentation, against ordinary TypeScript
on the same tasks. The owner charter is outside the repository; this note records the
choices the harness makes.

## What comes from SX

SX revision `9e9e4e5bf3fc` (`joshuarli/sx`) supplied the method, adapted rather than
imported:

| SX piece | Reused as | Why |
|---|---|---|
| `bench/codemode/pilot.py` `ask` and `PI_ISOLATION` | `harness/model.py` | The same isolated `pi` call: fresh session, fixed system prompt, no tools, extensions, MCP, skills, context files or templates, offline startup, and a model guard that refuses every model but the pinned one |
| `pilot.extract` | `prompts.extract` | The same mechanical fence rule, with XSH and TypeScript tags |
| `probe.py` trial loop and `repair_prompt` | `trial.py`, `prompts.repair_prompt` | At most three programs; feedback only from the checker, a runtime failure, or a tool rejecting its input; a completed wrong program ends the trial |
| `host.mjs` response semantics | `harness/service.mjs` | Exact input matching, ordered results, a read repeating its last result, a write served once, kinds `Timeout` to `Failed` |
| `typescript.py` declaration generator | `bindings.py` | One declaration source generates both languages' bindings, so neither arm gets a stronger operation |
| `workflows/sandbox.py` and `definition.py` | `fixtures.py`, per-task `fixture.py` | Deterministic `setup` per attempt and a separate `judge` over final state, effects and service ledgers |
| `workflows/records.py`, `metrics.py`, `report.py`, `repairs.py` | `trial.py`, `report.py` | Per-attempt status, effects and usage records; per-diagnostic repair counts |
| `probe.bootstrap` | `report.bootstrap` | Task-clustered percentile intervals |
| SX tasks | 5 of the 12 development tasks | `dev-events-pages`, `dev-retry-ci`, `dev-low-stock-alerts`/`restock-orders` and `backup-rotation` shaped `dev-audit-errors`, `dev-ci-retry`, `dev-restock` and `dev-prune-backups` |

SX's virtual clock and in-process host do not carry over: XSH programs run as real
processes, so service delays and retries use real, short times with tolerant lower
bounds.

## Trial loop

For each scheduled cell (task, condition, sample) the runner:

1. builds the condition's prompt: its context file, the task description, the task's
   API facts, and the run instructions; saves it as `prompt-N.md`;
2. asks `pi` through standard input with the isolation flags and records the raw JSON
   events, the reply, usage, stop reason, reported model and thinking level;
3. extracts the program by the fence rule and saves it;
4. checks it inside the boundary (`xsht check main.xsh` or `tsc -p`); a rejection
   feeds the real diagnostics back;
5. otherwise resets and sets up each fixture variant in a fresh directory, starts the
   variant's fixture service when the task has one, runs the program, snapshots the
   tree before and after, collects the service ledger, and judges the variant;
6. feeds back only the public variant's runtime failure (an exit status the task does
   not allow, or the time limit): its exit status and standard error. A program that
   completes on the public variant is judged on every variant and ends the trial,
   correct only when every variant is.

At most three programs are asked per trial. A provider error or rate limit is an
interruption: the worker backs off (60 s doubling to 30 min, 2 h in total), retries the
same call, and never records it as a model failure. Each attempt is saved as it
finishes, so `trials --resume` continues a trial at its next attempt.

## Accounting

Each attempt records, from pi's final assistant message: uncached `input`,
`cache_read`, `cache_write`, `output`, `reasoning`, `total`, pi's `cost` estimate and
its `totalFromProvider` flag. A missing field is `null` and counts as unknown, never
zero. Reasoning is reported separately but is part of `output` (a probe returned
output 37 with reasoning 33 and total 90 = 53 + 37), so totals never add it again.
Model calls, checker invocations, and program runs (one per fixture variant) are
counted separately, as are model wall seconds and program seconds. Provider
interruptions keep their partial usage beside the attempt. Each trial also records its
prompt section sizes, so context, task, facts and feedback contributions can be told
apart.

## Isolation boundary

Model access stays on the host. Every program is checked and run in a container:

- XSH: the repository's pinned `xsh-test` image
  (`oci.17166969.xyz/xsh/test@sha256:fed05abf…`) with the frozen `xsh` and `xsht`
  mounted read-only at `/usr/local/bin`, and Python and Perl masked with `/dev/null`.
- TypeScript: `node@sha256:6bd9ab43…` (Node 26.13.1) with TypeScript 7.0.2 and
  `@types/node` 26.6.6 installed from `ts/package-lock.json`.
- Both: `--network none`, a read-only root, a private `/tmp`, uid and gid of the
  runner, no capabilities, `no-new-privileges`, 4 CPUs, 2 GiB, 512 pids. Only the
  program directory (`/task`, read-only: the program and, for shared tasks, the
  generated `svc` binding) and the variant's fixture (`/work`) are mounted. No
  checkout, reference, judge, hidden variant, expected answer or credential is.
- A shared task's fixture service runs in its own container of the TypeScript image,
  also `--network none`; the program container joins only its network namespace, so
  the one reachable address is the service on `127.0.0.1:8080`. The service's
  responses and ledger stay in the service container's mounts.

`codemode isolation` runs probe programs inside the boundary and prints what they see.

## Freeze

The toolchain is `xsh` and `xsht` built once with `--profile verification` from the
lane base `c68ac9594`, copied to `.work/codemode-runs/toolchain`, and pinned by
SHA-256 in `harness/config.py`; every run refuses a binary or image that differs.
A trial refuses a task or context whose digest changed since its manifest was written.

## Defect policy

A reference that hits an XSH product defect is recorded under `defects/` with a
minimal reproducer before any scored call, and the task keeps a correct path that
avoids the defect. Defect D1 (a facet declared in an imported module never matches at
run time) is handled this way: the generated bindings declare no facets, and both
languages receive the same list of transient kinds.
