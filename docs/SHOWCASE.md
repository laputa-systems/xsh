# Showcase Corpus

`showcase/` holds production-like systems programs, with native tests in
`showcase/tests/`. It exists to show XSH doing the work it was designed for:
orchestrating processes; manipulating files, paths, archives, and system state;
crossing text, bytes, JSON, and native-tool boundaries deliberately; bounding
parallel work; and keeping failure, cleanup, cancellation, and traces explicit.
A difficult port is not automatically a good showcase. Durable warnings about a
particular program belong in that script's header comment.

## Selection test

A canonical candidate:

1. solves a recurring operational problem;
2. has intrinsic host interaction: remove processes, files, environment,
   network, and system state, and little of interest remains;
3. crosses at least three effect domains (`fs`, `process`, `env`, `net`, `time`,
   `archive`, `hash`, `linux`, `io`);
4. is large enough to need durable internal types and multiple modules;
5. has real partial-failure, cancellation, rollback, or cleanup behavior;
6. uses external programs where they provide real capability;
7. is testable with deterministic fixtures and controlled failures;
8. exerts design pressure likely to recur in other systems programs.

Programs dominated by parsing, graph analysis, aggregation, or report layout
(`showcase/jq.xsh` is the deliberate negative control) may expose defects but
do not define the language's domain.

## Corpus discipline

- **Existing language first.** Resolve friction in this order: simplify the
  program, improve its data model, extract a local helper, extract an XSH
  module, improve diagnostics or tooling, add a narrow host capability, and only
  then consider a language change.
- **Three strikes.** A language change needs the same irreducible problem in at
  least three independent canonical programs, evidence that neither an XSH
  module nor a narrow host primitive solves it, and an account of the
  complexity it removes.
- **Honest capability boundaries.** Cargo compiles, Docker builds containers, a
  signer signs. XSH owns argv construction, policy, environment and cwd,
  ordering and concurrency, retries and timeouts, temporary resources and
  cleanup, result classification, and reporting. Do not reimplement a mature
  tool to reduce the process count.
- **Gradual promotion.** program helper, program module, shared corpus module,
  standard XSH module, native host primitive; each step needs multiple real
  consumers.

A complete program has a documented CLI, typed multi-module boundaries without
unnecessary `Any`, deterministic fixtures, native tests for policy and
failure (including partial work, cleanup, and cancellation where children can
outlive the caller), trace assertions for important process relationships, no
embedded shell strings, and clean `xsht fmt --check`/`xsht lint`. Out of scope:
utility parity ports, embedded interpreters, application servers, TUIs, plugin
or package frameworks, and new syntax for packaging, release, or services.
