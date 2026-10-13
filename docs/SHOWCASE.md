# Showcase Corpus

`showcase/` holds larger systems programs; native tests live in
`showcase/tests/`. See [Examples](../examples/README.md) for smaller complete
programs. The corpus exercises deliberate text, bytes, JSON, and native-tool
boundaries, bounded parallel work, and visible failure, cleanup, cancellation,
and traces.

## Choosing programs

Prefer recurring operational problems whose design pressure is likely to recur,
with intrinsic host interaction, at least three host-facing areas (`fs`,
`process`, `env`, `net`, `time`, `archive`, `hash`, `linux`, `io`), and enough
complexity to benefit from typed internal types and multiple modules. Favor
real partial work, rollback, cleanup, or cancellation behavior that can be
tested with deterministic fixtures and controlled failures. Use external tools
where they provide real capability.

[`jq.xsh`](../showcase/jq.xsh) is the deliberate exception: its header calls it
a nonrepresentative interpreter and a forcing function for dynamic-data and
allocation pressure. It can expose design defects, but does not model XSH's
systems domain. Keep program-specific warnings in the script header.

## Resolving design friction

Simplify the program and its data model first, then consider local helpers, XSH
modules, diagnostics or tooling, and narrow host capabilities. A language
change needs the same irreducible problem in at least three independent
canonical programs, evidence that neither an XSH module nor a narrow host API
solves it, and an account of the complexity it removes. Promote in stages from
program helpers to shared modules, standard modules, or host APIs, requiring
multiple consumers at each step.

Let mature tools do their specialized work. XSH owns argument and environment
setup, policy, ordering and concurrency, retries and timeouts, temporary
resources and cleanup, result classification, and reporting.

A complete program has a documented CLI, typed multi-module boundaries without
unnecessary `Any`, deterministic fixtures, and native tests for policy, partial
work, and failure. Add trace assertions for important process relationships;
avoid embedded shell strings; keep `xsht fmt --check` and `xsht lint` clean.
Test cleanup and cancellation when children can outlive the caller. Utility
compatibility ports, embedded interpreters, application servers, TUIs, plugin
or package frameworks, and syntax for packaging, release, or services are out
of scope.
