# Showcase Corpus

`showcase/` holds larger systems programs with native tests in
`showcase/tests/`. See [Examples](../examples/README.md) for smaller programs.
The corpus exercises text, bytes, JSON, native tools, bounded work, cleanup,
cancellation, and traces.

## Choosing programs

Prefer recurring operational problems with host interaction across at least
three areas (`fs`, `process`, `env`, `net`, `time`, `archive`, `hash`, `linux`,
or `io`) and enough complexity to need typed internal types and multiple
modules. Favor partial work, rollback, cleanup, or cancellation that fixtures
and controlled failures can test. Use external tools when they provide real
capability.

[`jq.xsh`](../showcase/jq.xsh) is the deliberate exception: its header calls it
a nonrepresentative interpreter and a forcing function for dynamic-data and
allocation pressure. It can expose design defects, but does not model XSH's
systems domain. Keep program-specific warnings in the script header.

## Resolving design friction

Simplify the program and its data model before adding a helper, module,
diagnostic, tool, or host capability. A language change needs the same
irreducible problem in at least three independent canonical programs, evidence
that neither an XSH module nor a narrow host API solves it, and an account of
the complexity it removes. Promote helpers to shared modules, standard modules,
or host APIs as multiple programs need them.

Let mature tools do their specialized work. XSH owns argument and environment
setup, policy, ordering and concurrency, retries and timeouts, temporary
resources and cleanup, result classification, and reporting.

A complete program has a documented CLI, typed multi-module boundaries without
unnecessary `Any`, deterministic fixtures, and native tests for policy, partial
work, and failure. Add trace assertions for important process relationships.
Avoid embedded shell strings and keep `xsht fmt --check` and `xsht lint` clean.
Test cleanup and cancellation when children can outlive the caller. Utility
compatibility ports, embedded interpreters, application servers, TUIs, plugin
or package frameworks, and syntax for packaging, release, or services are out
of scope.
