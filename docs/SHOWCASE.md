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

## Current Development Corpus

The repository's live development program is `dev/main.xsh`, with target,
build, test, Docker, coverage, release, and benchmark policy in neighboring
`dev/*.xsh` modules. Cargo, Docker, compilers, signers, and inspection tools
remain explicit process boundaries. The `Makefile` is a compatibility facade:
its 22 targets delegate to this program, through `cargo dev` by default.
`XSH_DEV` selects a specific prebuilt binary for a caller that needs one.

`.github/workflows/verify.yml` uses the same XSH test routes on macOS and
the pinned Linux image. `docs/TEST-MAP.md` owns the actual verification matrix.
The remaining behavior and evidence work belongs in
`IMPROVEMENT-BACKLOG.md`, especially the focused `dev/`, `core/`, and
`showcase/` items. Larger new corpus programs need a concrete consumer before
they are added to this document.

The byte-path review of `showcase/file-audit.xsh`, `showcase/path-audit.xsh`,
and `showcase/git-digest.xsh` found three concrete boundaries. `file-audit`
checks containment with native `Path.strip_prefix`, `path-audit` compares
native paths before reporting duplicate directories or shadowed commands,
and `git-digest` asks Git to quote filenames before consuming its text output.
Their paired `showcase/tests/test-*.xsh` modules cover non-UTF-8 names in the
pinned Linux filesystem; macOS skips only those filesystem cases. The
`file-audit` test also checks a symlink whose target is a distinct native path
with the same lossy display text.

`showcase/archive-unpack.xsh` stages mutations beside an absent destination,
publishes only after successful archive work, and cleans staging on ordinary
failure or SIGINT/SIGTERM after a blocking archive call returns.
`showcase/backup-rotate.xsh` is a best-effort deletion tool with dry run as its
default. It handles direct files only and reports a deletion after it succeeds;
run active rotation against a directory that is not changing, because the
script cannot guarantee file identity across a concurrent replacement.

`showcase/watch-run.xsh` and `showcase/run-retry.xsh` impose no default
deadline on arbitrary commands. SIGINT/SIGTERM stop the wrapper with status 3;
`process.run` cancels its owned child group, including descendants. Native
tests use a delayed marker to check that canceled descendants do not continue.

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
The byte-indexed parser once accepted misspelled `null`, `true`, and `false`
tokens because it advanced by their lengths without checking their bytes.
`showcase/tests/test-jq.xsh::test_jq_rejects_misspelled_json_literals` keeps
that program-local error covered; the 15-case native suite passes on macOS and
the pinned Linux image. No language or runtime change followed from it.

Small existing standalone tools may remain in `showcase/`. Their presence does
not make them roadmap priorities. When a tool exhausts retries, times out, or
runs a failing child in one-shot mode, its process status must report that
failure to the caller. `showcase/wait-for.xsh::main` applies `--timeout` to
both HTTP requests and polling sleeps.

`showcase/release-pack.xsh::main` builds beside an absent output directory and
publishes it by rename only after the archive is complete; the output parent
must already exist and the output cannot be inside the input tree.
`showcase/bump-version.xsh::main` edits only `[package].version`, writes through
`Path.write_atomic`, and leaves invalid manifests unchanged.
`showcase/archive-unpack.xsh::staged_output` publishes mutating results only to
an absent path after successful extraction or compression; failed work removes
its staging directory. `showcase/backup-rotate.xsh::main` considers only direct
files in its requested directory.

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
