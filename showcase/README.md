`showcase/` is XSH's production-like systems corpus.

Its purpose is not to prove that XSH can imitate every familiar Unix utility,
data tool, or application language. It exists to prove that XSH is unusually
good at the work it was designed for:

* orchestrating processes;
* manipulating files, paths, archives, and system state;
* crossing text, bytes, JSON, and native-tool boundaries deliberately;
* coordinating bounded parallel work;
* making expected failures explicit;
* preserving cleanup, cancellation, and traceability under failure;
* expressing system policy in ordinary typed XSH.

A difficult port is not automatically a valuable showcase. The best showcase is
one whose complexity becomes smaller and more legible because it is written in
XSH.

## The Selection Test

A canonical showcase candidate should satisfy all of these:

1. It solves a recurring operational problem rather than existing only as a
   demonstration.
2. Host interaction is intrinsic to the program. If processes, files,
   environment, network access, archives, and system state were removed, most of
   the interesting program would disappear.
3. It crosses at least three meaningful effect domains, such as `fs`, `process`,
   `env`, `net`, `time`, `archive`, `hash`, `linux`, or `io`.
4. It is large enough to require durable internal types and multiple modules.
5. It has meaningful partial-failure, cancellation, rollback, or cleanup
   behavior.
6. It uses external programs where they provide substantive capability rather
   than reimplementing them merely to remain "pure XSH."
7. It can be tested with deterministic fixtures, controlled failures, and
   observable state transitions.
8. Its design pressure is likely to recur in other systems programs.

The fast rejection question is:

> Remove every process, filesystem, environment, network, and host-state
> interaction. If most of the interesting program remains, is this really an
> XSH showcase?

Programs that fail this test may still be useful compiler or runtime stress
tests. They do not define the language's intended domain.

## Corpus Discipline

### Use the existing language first

The next corpus cycle should operate under a language-feature moratorium.

Corpus work must not begin by adding syntax, new semantic categories, generic
application-runtime facilities, or convenience APIs. A program should first be
implemented using the existing language, standard modules, process model, and
ordinary XSH libraries.

When friction appears, resolve it in this order:

1. simplify the program;
2. improve the program's data model;
3. extract a local helper;
4. extract a reusable XSH module;
5. improve diagnostics or tooling;
6. add a narrow host capability that cannot safely or efficiently be expressed
   in XSH;
7. only then consider a language change.

Policy belongs in XSH. Rust should provide irreducible host capabilities, not
encode package, release, service, or distribution policy.

### Apply the three-strike rule

No new semantic category should enter XSH because one ambitious program wants
it.

A language change requires:

* the same irreducible problem in at least three independent canonical programs;
* evidence that an ordinary XSH module cannot solve it cleanly;
* evidence that a narrow host primitive cannot solve it;
* a design that preserves explicit effects and source-visible work;
* a clear account of which existing complexity the feature removes.

A single program may discover a problem. It does not get to define the solution.

### Keep capability boundaries honest

External programs are not a failure of self-hosting when they provide real
capability.

Cargo should compile Rust. Docker should create containers. LLVM tools should
inspect binaries. A compiler should compile. A signer should sign.

XSH should own:

* argv construction;
* target and policy selection;
* environment and working-directory control;
* process ordering and concurrency;
* retries, cancellation, and timeouts;
* temporary resources and cleanup;
* result classification;
* artifact validation;
* reporting and traceability.

Do not replace a mature native tool with thousands of lines of general-purpose
algorithmic XSH merely to reduce the process count.

### Incubate reusable code gradually

Use this promotion path:

```text
program-local helper
    -> program module
    -> shared corpus module
    -> standard XSH module
    -> narrow native host primitive
```

Promotion requires multiple real consumers. Do not construct a speculative
framework before the corpus demonstrates a stable common shape.

## Completion Standard

A canonical corpus program is complete only when it has:

* a useful, documented command-line interface;
* a multi-module implementation with explicit typed boundaries;
* no unnecessary `Any` or unvalidated dynamic-record propagation;
* deterministic fixtures;
* native XSH tests for policy and behavior;
* platform or privilege tests only where the boundary requires them;
* failure tests for partial work and cleanup;
* cancellation or timeout tests where child processes may outlive the caller;
* trace assertions for important process and resource relationships;
* explicit external-tool boundaries;
* no embedded shell command strings;
* formatting and lint coverage through the runnable-corpus gate;
* a short findings section recording reusable pressure without immediately
  proposing language features.

The success path alone is insufficient. The program should remain intelligible
when a download is truncated, a child hangs, a file is replaced concurrently, a
permission check fails, a disk fills, or cleanup itself encounters an error.

## Priority 1: Operationally Self-Host XSH Development

This program belongs under `dev/`, not `showcase/`, because it is live repository
infrastructure. It is nevertheless the first flagship member of the canonical
corpus.

Replace Make and shell orchestration with one typed XSH entry point that owns:

* development builds;
* checking, linting, and formatting workflows;
* native and Rust tests;
* Linux container tests;
* coverage;
* benchmarks and syscall diagnostics;
* target-specific distribution builds;
* binary verification;
* installation;
* release artifact preparation;
* core-script packaging.

Cargo, Docker, rustc, LLVM, codesign, `readelf`, and GitHub remain explicit
capability boundaries.

The implementation must exercise:

* typed target and host descriptions;
* environment composition;
* exact argv construction;
* scoped working directories;
* nested container execution without `sh -c`;
* temporary-resource ownership;
* cleanup on every exit path;
* target matrices;
* binary and archive proofs;
* failure propagation across long process chains.

Completion means that the repository has one coherent XSH development command,
the Makefile contains no behavior, CI invokes the same XSH modules used locally,
and no second orchestration implementation remains.

## Priority 2: Root Filesystem Composer and Auditor

Build a root filesystem from package trees and typed manifests, then prove that
the resulting filesystem satisfies its contract.

The program should:

1. create a staged root;
2. merge package manifests and payloads;
3. detect path and ownership collisions;
4. preserve modes, symlinks, and directory metadata;
5. reject unsafe or escaping links;
6. inspect executable formats, interpreters, architectures, and shared-library
   dependencies;
7. identify missing or unexpected runtime dependencies;
8. enforce forbidden-file and forbidden-path rules;
9. produce a deterministic manifest and proof report;
10. emit an archive or image only after validation succeeds.

Use `readelf`, `otool`, or other native inspection tools where appropriate.
Do not implement a general ELF or Mach-O parser merely to avoid a subprocess.

Required failure cases include:

* two packages claiming the same path incompatibly;
* a symlink escaping the staged root;
* a binary for the wrong architecture;
* a missing interpreter or library;
* malformed package metadata;
* interruption during staging;
* archive creation failure after successful composition;
* cleanup failure after an earlier error.

The former `ldd-tree.xsh` idea is absorbed into this program as one subsystem,
where dependency inspection contributes to a larger system proof.

## Priority 3: Package Update and Verification Orchestrator

Automate the complete path from discovering an upstream release to producing a
reviewable, verified package update.

The program should:

1. discover candidate upstream versions;
2. fetch and classify release metadata;
3. select the correct source artifact;
4. download into a temporary or content-addressed staging area;
5. verify checksums, signatures, and expected archive structure;
6. update package metadata through a structured edit;
7. refresh checksums;
8. apply or rebase package patches;
9. build the affected package and dependency closure;
10. run package proofs;
11. produce a concise report and reviewable patch.

The implementation may live in the Laputa package repository, with deterministic
fixtures retained here where useful. It must use package definitions as ordinary
XSH values rather than inventing another package-description DSL.

Required failure cases include:

* ambiguous upstream versions;
* missing or renamed release assets;
* checksum mismatch;
* truncated download;
* patch rejection;
* unexpected archive root;
* failed build after metadata has been staged;
* one failed update in a bounded parallel batch;
* cancellation while downloads or builds are active.

## Priority 4: Service Activation Planner

Implement the finite, host-facing portion of service management without turning
XSH into a long-lived application runtime.

The program should:

1. load typed service definitions;
2. validate names and dependencies;
3. reject dependency cycles;
4. calculate start, stop, restart, and rollback plans;
5. execute one-shot setup and teardown actions;
6. launch or signal external service processes through explicit boundaries;
7. run bounded health checks;
8. record activation results and failure chains;
9. roll back successfully activated dependencies when policy requires it;
10. emit a structured trace of the activation transaction.

This is not a mandate to add a hidden scheduler, green threads, callbacks,
futures, or a process-wide event loop. A dedicated supervisor may remain the
long-lived runtime. XSH owns service policy, activation, host preparation, and
observable process control around it.

Required failure cases include:

* dependency cycles;
* setup failure before process launch;
* child process exiting during activation;
* health-check timeout;
* cancellation during a dependency fan-out;
* rollback where one teardown action also fails;
* stale PID or process identity;
* conflicting concurrent activation requests.

## Priority 5: Incident Evidence Bundle

Collect a useful diagnostic bundle from a live system while tolerating missing
tools, permissions, timeouts, and individual probe failures.

The program should collect a bounded set of evidence such as:

* process and thread state;
* listeners and network interfaces;
* mounts and filesystem capacity;
* service status;
* selected logs;
* kernel and platform metadata;
* package or build metadata;
* outputs from explicitly configured diagnostic commands.

Every probe should have:

* a typed identity;
* an explicit timeout;
* bounded output;
* a success, skipped, unavailable, denied, timed-out, or failed result;
* a recorded command and environment boundary;
* deterministic archive placement.

The final bundle should contain a manifest describing what was attempted and
what failed. One unavailable probe must not destroy the useful evidence produced
by the others.

Required failure cases include:

* missing diagnostic tools;
* permission denial;
* a hanging child;
* output exceeding its limit;
* cancellation during parallel collection;
* archive failure;
* redaction failure;
* temporary-directory cleanup after partial success.

This is a strong test of XSH's ability to make partial failure explicit without
collapsing into exception-driven or stringly orchestration.

## Priority 6: Host State Reconciler

Reconcile a deliberately narrow host policy through explicit
`inspect -> plan -> apply -> verify` phases.

Begin with a bounded policy surface such as:

* directories, files, symlinks, ownership, and modes;
* selected environment or configuration files;
* enabled service links or activation state;
* mounted or prepared filesystem resources.

Represent policy as ordinary typed XSH records and procedures. Do not introduce
YAML, TOML, a templating language, or a second declarative DSL.

The program must:

1. inspect current state;
2. produce a deterministic plan;
3. distinguish no-op, create, update, remove, conflict, and unsupported actions;
4. require an explicit transition into mutation;
5. stage replacements safely;
6. verify the resulting state;
7. report drift that appeared during application;
8. preserve enough information for rollback where the operation permits it.

Required failure cases include:

* host state changing between inspection and application;
* permission loss after part of the plan succeeds;
* a path changing type;
* a symlink substitution attack;
* verification failure after mutation;
* rollback failure;
* cancellation during application.

This should remain a focused systems program, not grow into a general remote
configuration-management platform.

## Secondary Corpus, Not Language Drivers

The following may still be useful when there is a concrete user need, but they
are not priorities for the canonical systems corpus:

* Chrome trace critical-path analysis;
* `strace` summarization;
* HAR waterfall rendering;
* Terraform plan summarization;
* lockfile graph analysis;
* Git conflict forecasting;
* JUnit flake aggregation;
* Kubernetes triage;
* Massif parsing;
* repository risk scoring.

Most of these are dominated by parsing, schema traversal, graph analysis,
aggregation, or report layout. They can expose implementation defects and
performance pathologies, but they should not pull XSH toward a general-purpose
data-processing language.

`jq.xsh` remains a valuable negative control and forcing function. It
deliberately demonstrates a workload that wants closures, lazy generators,
interpreter machinery, and persistent collections. Findings from it may reveal
compiler or runtime defects, but jq-shaped pressure alone does not justify
changing XSH's domain.

Small existing standalone tools may remain in `showcase/`. Their presence does
not make them roadmap priorities.

## Explicit Non-Goals

Do not prioritize:

* reimplementing every core utility for parity;
* embedded language runtimes or interpreters;
* generic parser libraries solely to ingest every configuration format;
* fine-grained concurrent application servers;
* TUIs or interactive application frameworks;
* a plugin framework;
* a package registry;
* first-class package, release, service, or build syntax;
* generic classes, traits, macros, closures, or application-runtime abstractions;
* compatibility work whose only purpose is to stabilize the current API;
* ports chosen mainly by recognizability or line count.

The corpus should sharpen XSH's identity, not expand its territorial claims.

## Corpus R1 Milestone

Corpus R1 is complete when:

1. XSH's own development lifecycle is operationally self-hosted.
2. At least two additional canonical multi-module systems programs are complete.
3. Each program has deterministic fixtures, fault injection, cleanup checks, and
   trace assertions.
4. Repeated helpers have been cataloged without premature promotion.
5. A corpus pressure report distinguishes:

   * program-design problems;
   * missing reusable XSH modules;
   * tooling or diagnostic shortcomings;
   * narrow missing host capabilities;
   * genuine repeated language pressure.
6. No new semantic category has been added without satisfying the three-strike
   rule.
7. At least one existing abstraction has been simplified, demoted, or removed
   based on corpus evidence.

The preferred outcome is not a larger language.

The preferred outcome is stronger evidence that XSH can remain small while
owning an unusually large portion of the systems layer.
