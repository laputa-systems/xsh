# XSH gym: inner and outer loops

The gym is an experiment harness for improving an isolated coding agent on
small XSH tasks. It has two deliberately different loops:

```text
outer Pi on the host
  ├─ starts one isolated inner trial
  ├─ reads the task result, session, metrics, and review
  ├─ updates the runtime handbook when evidence supports it
  └─ starts the next trial with the resulting handbook

inner Pi in a task container
  ├─ sees only /work, xsh, xsht, and task-approved utilities
  ├─ receives gym/runtime/agents.md, handbook.md, review.md, and a task prompt
  └─ writes the requested artifact and review.md
```

## Inner loop

The inner loop is the controlled measurement boundary. The agent runs inside
the task image with `/work` mounted as its workspace. The oracle and evaluator
run in a second container so the candidate cannot inspect the oracle
implementation. `gym/runtime/` contains the files staged into that workspace:

- `agents.md` — task-neutral rules and the review contract;
- `handbook.md` — the current user-facing XSH guide;
- `review.md` — the session review template;
- task prompts such as `task-ecount.md` and `task-tags.md`.

The inner runner copies each trial's session and evaluator manifest to its
output directory. `session.jsonl` is the durable Pi record. The host-side
`gym/session-metrics.py` summarizes assistant turns, tool calls, tool errors,
token buckets, optional provider-reported reasoning tokens, cost, and the
candidate/oracle timing data emitted by an evaluator.

An inner result has separate concerns rather than one code-quality score:

- objective correctness: exact output, hidden cases, checks, and task-specific
  capability restrictions;
- agent effort: turns, tool calls, reasoning/output/input/cache tokens, cost,
  and wall time;
- program performance: candidate and oracle wall/CPU time and their ratio;
- protocol output: whether the requested `review.md` was completed.

The handbook and task prompt are mounted read-only inside the agent container.
The review file remains writable because it is an expected task deliverable.

## Outer loop

`gym/run-outer-loop.xsh` is the deterministic controller. It creates a fresh
experiment directory, records the handbook hash before each iteration, runs the
inner task, computes metrics, and invokes the host-installed `pi` with
`gym/outer-agent.md` as its system prompt.

The outer Pi is intentionally not containerized. It can inspect the complete
repository, source, tests, gym code, and experiment artifacts. Its role is to
judge the evidence and maintain the gym, not to solve the task artifact. For
the demonstration controller, it is asked to modify only
`gym/runtime/handbook.md` and write a `judge.md` report for each iteration.

Iteration 2 is accepted only when its staged handbook hash equals iteration
1's post-judge hash. This proves that the handbook change crossed the inner
container boundary rather than merely existing in the outer workspace.

The outer judge must distinguish these cases:

- agent failure — the task artifact is missing or incorrect;
- handbook friction — the agent encountered a reusable, correctable lesson;
- tooling/image mismatch — the documented command or runtime contract is not
  present, which should not be scored as an agent failure;
- evaluator failure — the judge itself could not establish a result.

A handbook patch is a proposal until a later replay validates it. Task-specific
lessons should not become general XSH guidance without evidence from another
task or an independent holdout.

## Timing contract

Evaluators may measure the exact candidate and oracle commands with
`time.measure`. For a task that opts into the strict envelope:

```text
runtime_ratio = median(candidate.wall_ns) / median(oracle.wall_ns)
strict pass   = 0.90 <= runtime_ratio <= 1.10
```

Measurements should use repeated runs, fixed images, identical inputs, and no
concurrent trials. CPU time is recorded alongside wall time so an artificial
sleep cannot masquerade as equivalent execution. The report may also retain
each case's ratio, but the contract uses the ratio of the candidate and oracle
wall-time medians; this reduces noise from very short process launches while
keeping the ±10% requirement explicit.

## Local image build

The gym does not download an XSH release. Its `base-image` prerequisite runs
the repository's local `make dist-Linux` on Linux, or
`make dist-Linux-docker` on macOS using `Dockerfile.test`, for the Docker
architecture. It stages the resulting `xsh` and `xsht` multicall entry points
in `gym/.dist/` and copies those files into the image. Cargo's Docker target
directory is persistent, so subsequent iterations reuse the local build.
Pi remains a separately pinned image dependency. Override `XSH_TARGET` when
the Docker platform is not the default Apple Silicon Linux target.

## Demonstration

Build the base task image and run the two-iteration handbook propagation demo:

```sh
make -C gym outer-loop OUTER_ITERATIONS=2
```

The command creates `gym/.outer/task-tags-<timestamp>/`. Its
`outer-summary.json` records each inner run, judge status, handbook hashes, and
the final result. `task-tags` is intentionally small but exercises argument
lists, string transformation, exact output, checks, and the inner review
protocol; it is a better propagation test than the one-file `hello` task and a
less difficult benchmark than `task-ecount`.
