# Showcase Corpus

`showcase/` holds larger systems programs; native tests live in
`showcase/tests/`. See [Examples](../examples/README.md) for smaller complete
programs.

## Choosing programs

Prefer recurring operational problems likely to recur in other systems
programs, with intrinsic host interaction, at least three host-facing areas
(`fs`, `process`, `env`, `net`, `time`, `archive`, `hash`, `linux`, `io`), and
enough complexity to benefit from typed internal boundaries and multiple
modules. Programs should expose real failure, cleanup, or cancellation behavior
and be testable with deterministic fixtures and controlled failures. Use
external tools where they provide real capability.

Programs focused on parsing, graphs, aggregation, or reports such as
[`jq.xsh`](../showcase/jq.xsh) can expose defects, but do not define XSH's
domain. Keep program-specific warnings in the script header.

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

A complete program has a documented CLI, typed module boundaries without
unnecessary `Any`, deterministic tests for policy and failure, trace assertions
for important process relationships, no embedded shell strings, and clean
`xsht fmt --check` and `xsht lint`. Test cleanup and cancellation when children
can outlive the caller. Utility compatibility ports, embedded interpreters,
application servers, TUIs, plugin or package frameworks, and syntax for
packaging, release, or services are out of scope.
