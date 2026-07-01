# Showcase Ideas

Promising standalone tools that expert programmers actually use, chosen to push
XSH through recursive data, messy text formats, subprocess orchestration,
dynamic records, report layout, and host-system integration.

## `trace-critical-path.xsh`

Analyze Chrome Trace Event JSON and report the critical path, longest spans,
idle gaps, per-thread time, and optionally emit a compact SVG timeline.

Why it is useful: Chrome trace files are common across browsers, build systems,
profilers, and custom instrumentation. A small critical-path analyzer can turn a
large trace into a direct optimization target.

Why it stresses XSH: deep JSON traversal, dynamic records, timestamp math,
sorting, nesting, stateful stack reconstruction, and visual layout.

## `strace-summary.xsh`

Parse `strace -f -ttT` logs and summarize syscall counts, total/mean/max
latency, errno frequency, slowest calls, and per-PID activity.

Why it is useful: systems programmers use `strace` constantly, but raw output is
too noisy for comparing runs or finding the dominant failure/latency pattern.

Why it stresses XSH: irregular text parsing, optional fields, process state,
grouping, latency aggregation, and error classification.

## `har-waterfall.xsh`

Read a browser HAR file and summarize slow requests, cache misses, redirects,
DNS/connect/TLS/TTFB/download phases, and optionally emit an SVG waterfall.

Why it is useful: HAR files are the portable evidence bundle for web
performance debugging.

Why it stresses XSH: nested JSON records, timing phase arithmetic, grouped
tables, URL normalization, and generated graphics.

## `terraform-plan-digest.xsh`

Consume `terraform show -json plan.out` and print creates, updates, deletes,
replacements, destructive changes, changed attributes, and affected modules.

Why it is useful: expert infrastructure reviewers want a concise plan summary
before approving a large apply.

Why it stresses XSH: deeply nested schema-heavy JSON, dynamic record access,
recursive attribute diffs, and high-signal reporting.

## `lockfile-duplicates.xsh`

Inspect lockfiles such as `Cargo.lock`, `package-lock.json`, `pnpm-lock.yaml`,
or `go.sum`, then report duplicate dependency versions, largest dependency
subtrees, direct vs transitive packages, and suspicious version skew.

Why it is useful: dependency bloat and version skew are common maintenance
problems in real codebases.

Why it stresses XSH: mixed file formats, graph construction, cycle handling,
semantic version parsing, and awkward parser gaps for TOML/YAML-style inputs.

## `git-conflict-forecast.xsh`

Given two branches, use `git merge-tree`, `git diff --name-only`, and file
ownership/churn signals to estimate likely conflict hotspots before merging.

Why it is useful: this can save review and integration time on long-lived
branches.

Why it stresses XSH: subprocess orchestration, parsing Git output, joining
multiple signals by path, ranking, and explaining uncertainty.

## `junit-flake-triage.xsh`

Aggregate many JUnit XML files from CI, rank flaky tests, recurring failure
messages, slow suites, and new failures vs a baseline.

Why it is useful: test triage is a recurring expert workflow, especially in
large CI systems.

Why it stresses XSH: XML parsing is not native today, so this would expose where
regex parsing, subprocess helpers, or a future XML module becomes necessary.

## `kube-triage.xsh`

Run `kubectl get pods/events/deployments -A -o json` and summarize
CrashLoopBackOffs, pending pods, image pull failures, recent restarts,
unschedulable reasons, and related events.

Why it is useful: this is the first-pass diagnosis many Kubernetes operators do
by hand.

Why it stresses XSH: subprocess capture, large JSON records, cross-record joins,
time sorting, and concise incident-style output.

## `ldd-tree.xsh`

Recursively inspect shared-library dependencies with `ldd`, `otool -L`, or
`readelf`, detect missing libraries, duplicate SONAMEs, unexpected paths, and
emit a dependency tree or DOT graph.

Why it is useful: native dependency failures are painful, platform-specific,
and often solved with ad hoc scripts.

Why it stresses XSH: cycle detection, platform-specific parsers, path
normalization, tree rendering, and subprocess-heavy discovery.

## `massif-summary.xsh`

Parse Valgrind Massif output and report peak heap, allocation tree, top call
paths, and growth phases.

Why it is useful: Massif remains valuable for native memory profiling, but its
raw output is hard to scan.

Why it stresses XSH: nested tree parsing, stack aggregation, phase detection,
and report layout over a nontrivial text format.

## `repo-risk-map.xsh`

Combine `git log`, `git blame`, test paths, file size, TODO density, recent
churn, and ownership spread into a ranked risky-files report.

Why it is useful: reviewers and tech leads use this kind of signal to choose
where to add tests, split ownership, or slow down risky changes.

Why it stresses XSH: many subprocess calls, file scanning, joins across
heterogeneous records, scoring, and explainable ranking.
