# Operationally Self-Host XSH’s Development Lifecycle

Replace the repository’s Make- and shell-based development orchestration with one coherent, typed, multi-module XSH program.

Implement the complete migration. Do not stop after adding a partial wrapper around the Makefile.

## Outcome

After this work, XSH must own the orchestration of its own development lifecycle:

* development builds;
* non-mutating checks;
* formatting and autofix workflows;
* Rust tests;
* native XSH tests;
* privileged Linux container tests;
* CI-target tests;
* combined coverage;
* benchmarks and syscall diagnostics;
* native and Docker distribution builds;
* installation;
* binary verification;
* release artifact preparation;
* core-script packaging.

Cargo, rustc, Docker, LLVM tools, `readelf`, codesign, `xattr`, Git, GitHub CLI, and rustybench remain external programs because they provide substantive capability.

XSH must own the typed policy around those programs:

* target selection;
* argv construction;
* environment composition;
* working directories;
* process ordering;
* bounded concurrency;
* cancellation;
* temporary resources;
* cleanup;
* artifact naming;
* validation;
* error classification;
* reporting.

The final repository must not maintain parallel Make and XSH implementations of the same lifecycle.

## Architectural Constraint

This task is a proof of the existing language.

Do not add:

* XSH syntax;
* new language semantics;
* new standard-library APIs;
* a task-runner DSL;
* a build-description format;
* a plugin framework;
* a general process abstraction layer;
* a Rust dependency;
* an application scheduler or async model.

Do not change the compiler or runtime merely because an orchestration operation is awkward.

When the existing XSH API cannot express something conveniently, use a direct, explicit external-process boundary. If a true language or host-API blocker is found, reduce it to a minimal reproduction and record it in the final report; do not expand this task into a language change.

## Read Before Editing

Read only the material relevant to this orchestration boundary:

* `AGENTS.md`
* `docs/CHAPTER-01-why-xsh.md`
* `docs/TEST-MAP.md`
* `Makefile`
* `.cargo/config.toml`
* `Dockerfile.test`
* `.github/workflows/release.yml`
* `.github/scripts/release-verify-static.xsh`
* `.github/scripts/release-package-linux.xsh`
* `.github/scripts/release-package-darwin.xsh`
* `tools/cov-linux.xsh`
* `tools/xsh-cov.xsh`
* `docs/COVERAGE.md`
* `docs/BENCHMARKING.md`
* `xsht-config.ini`

Then search for exact call sites of:

* `make`;
* every current Make target;
* the three `.github/scripts/release-*.xsh` entry points;
* `scripts/check-libxsh-imports.sh`;
* current coverage, benchmark, distribution, installation, and release commands;

Do not map the parser, checker, indexed runtime, or compiler internals. They are out of scope unless an existing repository test exposes an actual regression.

## Behavioral Contract

Treat the current Makefile, documentation, and release workflow as the behavior to preserve unless this prompt explicitly authorizes removal or consolidation.

The lifecycle to preserve includes:

* a default target of `x86_64-unknown-linux-musl`;
* Linux musl targets for x86_64 and aarch64;
* Darwin target support, including `aarch64-apple-darwin`;
* a macOS deployment target default of `26.0`;
* distribution binaries `xsh`, `xsht`, and `xshi`;
* native and Docker distribution paths;
* Linux static-link validation;
* target-specific CPU and linker flags;
* install workflows for Darwin and Linux;
* native and Docker coverage;
* Rust and native XSH test paths;
* privileged Linux tests;
* rustybench latency, allocation, and syscall workflows;
* release artifact verification and checksumming;
* a three-target GitHub release matrix;
* packaging `core/*.xsh` scripts without the `.xsh` suffix.

Do not silently discard any other current behavior because its Make implementation is awkward. Either preserve it, deliberately remove a proven-dead compatibility alias, or document why it no longer has a caller.

The current `docs` phony target should not be recreated unless an active call site proves that it still has value.

## Final Command Surface

Add a Cargo alias in `.cargo/config.toml`:

```toml
[alias]
dev = "run --quiet -p xsh --bin xsh -- dev/main.xsh --"
```

The ordinary interface becomes:

```text
cargo dev help
cargo dev build
cargo dev check
cargo dev lint --fix
cargo dev test
cargo dev test xsh
cargo dev test linux
cargo dev test linux --ci
cargo dev test macos --ci
cargo dev coverage
cargo dev coverage --backend native
cargo dev coverage --backend docker
cargo dev bench
cargo dev bench --fast
cargo dev bench --syscalls
cargo dev dist
cargo dev dist --target aarch64-unknown-linux-musl
cargo dev dist --target x86_64-unknown-linux-musl --docker always
cargo dev dist --ci
cargo dev install
```

The direct bootstrap-equivalent form remains:

```text
cargo run --quiet -p xsh --bin xsh -- dev/main.xsh -- COMMAND
```

Once a development binary exists, the same program must also run as:

```text
target/debug/xsh dev/main.xsh -- COMMAND
```

Use the exact final command names consistently in documentation and CI. Small adjustments are acceptable when XSH’s existing `cli.parse` shape makes another layout materially cleaner, but retain one obvious entry point and coherent command families.

Do not create one executable script per former Make target.


## Migration Mapping

Use this as the minimum behavior map:

| Existing entry                  | New entry                              |
| ------------------------------- | -------------------------------------- |
| `make build`                    | `cargo dev build`                      |
| `make lint`                     | `cargo dev lint --fix`                 |
| `make install`                  | `cargo dev install`                    |
| `make install-darwin`           | internal host dispatch from `install`  |
| `make install-linux`            | internal host dispatch from `install`  |
| `make dist`                     | `cargo dev dist`                       |
| `make dist-native`              | internal native distribution operation |
| `make dist-Linux-docker`        | `cargo dev dist --docker always`       |
| `make dist-ci`                  | `cargo dev dist --ci`                  |
| `make cov`                      | `cargo dev coverage`                   |
| `make cov-native`               | `cargo dev coverage --backend native`  |
| `make cov-docker`               | `cargo dev coverage --backend docker`  |
| `make test`                     | `cargo dev test`                       |
| `make test-xsh-native-only`     | `cargo dev test xsh`                   |
| `make test-linux`               | `cargo dev test linux`                 |
| `make test-linux-ci`            | `cargo dev test linux --ci`            |
| `make test-macos-ci`            | `cargo dev test macos --ci`            |
| `make bench`                    | `cargo dev bench`                      |
| `make bench-fast`               | `cargo dev bench --fast`               |
| `make bench-syscalls`           | `cargo dev bench --syscalls`           |

Internal container entry points may be exposed under a hidden or clearly internal command namespace. They must not become a second public interface.

## Recommended Module Structure

Use a small set of cohesive modules. A reasonable structure is:

```text
dev/
  main.xsh
  context.xsh
  targets.xsh
  build.xsh
  test.xsh
  coverage.xsh
  bench.xsh
  dist.xsh
  install.xsh
  release.xsh
  docker.xsh
  verify.xsh
  internal.xsh
  tests/
    test-context.xsh
    test-targets.xsh
    test-build.xsh
    test-test.xsh
    test-dist.xsh
    test-docker.xsh
    test-release.xsh
    test-failures.xsh
```

Adapt the split when the code demonstrates a better boundary. Do not create empty modules or split short procedures merely to match this tree.

Responsibilities should remain clear:

* `main.xsh` parses and dispatches commands.
* `context.xsh` resolves repository root, host information, environment overrides, paths, and tool availability.
* `targets.xsh` is the single source of truth for supported target properties.
* command-family modules own policy for their operations.
* `docker.xsh` owns Docker argv, mounts, platform selection, and container invocation.
* `internal.xsh` owns operations executed inside the test container.
* `verify.xsh` owns reusable artifact proofs.
* `release.xsh` owns release naming, binary packaging, core packaging, checksums, and artifact-set validation.

Do not create a generic task graph, dependency engine, executor framework, or string-based command language.

## Typed Configuration Model

Represent closed configuration with closed types.

At minimum, model:

* host OS;
* host architecture;
* target OS;
* target architecture;
* target triple;
* Docker platform;
* ELF machine name where applicable;
* distribution profile and output directory;
* binary product set;
* coverage backend;
* Docker selection policy;
* paths to repository, target, coverage, and artifact directories.

A target description should centralize:

* triple;
* OS;
* architecture;
* Docker platform;
* expected executable format;
* expected ELF machine;
* target CPU flags;
* C flags;
* Rust flags;
* static-musl behavior;
* whether native execution is possible on the current host.

Do not scatter string comparisons such as:

```text
target.starts_with("aarch64-")
```

throughout command modules.

Use `Record` or `Any` only at genuinely dynamic boundaries. Internally, prefer named records, enums, tagged unions, `Path`, and `Result`.


## Environment Contract

Preserve active environment overrides where they are used by CI, documentation, or local workflows, including as applicable:

* `TARGET`;
* `DARWIN_DEPLOYMENT_TARGET`;
* `DARWIN_CODESIGN_ENTITLEMENTS`;
* `DARWIN_CODESIGN_FLAGS`;
* `RUSTFLAGS`;
* `CFLAGS_x86_64_unknown_linux_musl`;
* `CFLAGS_aarch64_unknown_linux_musl`;
* `CFLAGS_aarch64_apple_darwin`;
* target-specific Cargo Rust flags;
* `DIST_PROFILE`;
* `DIST_BUILD_STD_FLAGS`;
* `DIST_DOCKER_BUILD_STD_FLAGS`;
* `XSH_TEST_IMAGE`;
* `XSH_TEST_IMAGE_BUILD`;
* `DOCKER_PLATFORM`;
* `CARGO_BUILD_WARNINGS`;
* `CARGO_TARGET_DIR`;
* `COV_BACKEND`;
* `COV_CARGO`;
* `COV_NATIVE_LINKER`;
* `XSH_COV_CARGO_BIN`;
* `RUSTYBENCH`;
* `XSH_OS_STRESS_REPEAT`.

Internal Make-only variables may disappear when their values become typed XSH data.

Environment rules:

* use scoped XSH environment blocks or command-specific environment records;
* do not construct `KEY=value command` strings;
* do not mutate the parent process environment permanently;
* make target-specific environment visible at the process call site or in a narrowly named builder procedure.

## Process Rules

Every external command must use an explicit executable and argv.

Do not use:

* `sh -c`;
* `bash -c`;
* shell pipelines embedded in strings;
* command substitution;
* shell loops;
* Make recursion;
* `eval`;
* generated shell fragments.

Use XSH for:

* loops;
* conditions;
* environment scopes;
* redirection;
* process capture;
* bounded parallelism;
* status checks;
* cleanup.

Capture output only when the program needs to inspect it. Otherwise inherit stdout and stderr so Cargo, tests, Docker, and benchmark tools remain directly observable.

On failure, report:

* the lifecycle stage;
* target or backend;
* executable and useful argv context;
* exit status or structured host error;
* the artifact or state transition that did not complete.

Do not create a helper that turns every process back into an opaque command string. Process boundaries should remain visible in source and traces.

## Repository Root and Bootstrap

The program should run from the repository root.

Validate the root using stable repository files such as:

* `Cargo.toml`;
* `rust-toolchain.toml`;
* `xsht-config.ini`.

Fail clearly when launched from the wrong directory. Do not add an elaborate workspace-discovery framework unless a real call site needs execution from a subdirectory.

The initial `cargo dev` invocation is the bootstrap boundary: Cargo builds the current XSH binary and then runs the XSH development program.

Do not attempt to self-host the Rust compiler or eliminate Cargo from this boundary.

## Build and Check Workflows

### `build`

Preserve the current native-musl build behavior where the Makefile links the required libc and `libgcc_s` files into the target sysroot before `cargo build`.

Apply that workaround only on the host and target combinations that require it. Do not perform Linux-musl sysroot mutation unconditionally on Darwin or unrelated targets.

### `check`

Add a non-mutating developer gate that checks the repository without rewriting it.

It should use the repository’s existing tools and contracts, such as:

* an exact Cargo build for required products;
* non-mutating Rust formatting checks;
* non-mutating Clippy where appropriate;
* `xsht check`;
* `xsht fmt --check`;
* `xsht lint`;
* the runnable XSH corpus gate;
* `git diff --check`.

Do not broaden this into every expensive test or distribution build.

### `lint --fix`

Preserve the intent of the current mutating `make lint` workflow:

* `cargo fmt --all`;
* `cargo clippy --fix --allow-dirty --all-targets --all-features --quiet`;
* build `xsht`;
* `xsht lint --fix`;
* `xsht fmt`.

This command exists for the repository owner. Do not run it while implementing this task, because `AGENTS.md` prohibits agent-driven formatting and autofix churn.

## Test Workflows

### Ordinary Rust test

Preserve:

```text
cargo test --release -- -Zunstable-options --report-time
```

unless a current repository contract has deliberately changed by implementation time.

### Native XSH test

Preserve the native-only path currently represented by:

```text
cargo run --release -p xsht -- test
```

Use exact package and binary ownership rather than relying on whichever `xsht` happens to be on `PATH`.

### Linux developer test

Replace the current Docker-plus-`sh -c` sequence with:

1. an outer XSH Docker invocation;
2. a direct container command that bootstraps or invokes XSH;
3. an internal XSH command that:

   * configures Git safe-directory state if required;
   * builds `xsh`, `xsht`, and `xsh-test-sleeper`;
   * exposes the built `xsh` at `/bin/xsh` where tests require it;
   * runs Rust tests with `linux-priv-tests`;
   * runs native `xsht` tests with the correct sleeper path;
   * performs cleanup through `defer`.

The container command must invoke Cargo or XSH directly. It must not invoke a shell interpreter.

### CI tests

Preserve the current target-specific contracts:

* Linux CI uses the selected target and distribution profile with `linux-priv-tests`, `net`, and `tools`;
* macOS CI uses the selected target with `net` and `tools`;
* test output remains uncaptured where the workflow currently requests it;
* Linux container target ownership is repaired to the host UID/GID even when a test fails.

Implement ownership repair through XSH cleanup, not a shell `trap`.


## Coverage

Retain `tools/cov-linux.xsh` and `tools/xsh-cov.xsh` behavior rather than rewriting working XSH logic unnecessarily.

Integrate them behind:

```text
cargo dev coverage
```

The automatic backend policy should preserve the current behavior:

* native on x86_64 Alpine Linux when the required tools are available;
* Docker otherwise.

Preserve explicit `native` and `docker` selection.

The Docker path must:

* build or reuse `Dockerfile.test`;
* use the privileged boundary required by the current coverage workflow;
* preserve the dedicated coverage target volume;
* mount the final coverage output into the repository;
* invoke XSH directly in the container;
* leave output ownership usable by the host.

The native path must preserve:

* Cargo binary-directory resolution;
* native linker selection;
* `XSH_COV_CARGO_BIN`;
* target-specific linker environment;
* output under `target/cov`.

Do not merge source-coverage policy, LLVM coverage, and standard-API coverage into one monolithic procedure. Reuse the existing separation.

## Benchmarks

Preserve the existing rustybench boundary and override.

The default remains conceptually equivalent to:

```text
cargo run --quiet --manifest-path ../../rustybench/Cargo.toml --
```

Support:

* baseline latency and allocation workflow;
* fast allocation iteration;
* syscall diagnostics.

Preserve the existing baseline files:

```text
crates/xshi/benches/baseline.json
crates/xshi/benches/fast-baseline.json
```

Do not implement benchmarking calculations in XSH. XSH owns invocation, repository paths, mode selection, and failure reporting.

Do not run broad benchmarks while implementing ordinary orchestration code. Use only the minimum smoke invocation necessary to prove dispatch.

## Distribution Builds

Centralize the distribution product list:

```text
xsh
xsht
xshi
```

Preserve the current distribution features:

```text
xsh/net
xsh/tools
xsht/native-tests
```

Preserve the current distribution optimization policy unless the repository has already intentionally changed it:

```text
-Zlocation-detail=none
-Zunstable-options
-Cpanic=immediate-abort
```

Do not add profile-use flags or consume optimization profiles.

Preserve target-specific behavior.

### x86_64 Linux musl

* `-C target-cpu=x86-64-v3`;
* C `-march=x86-64-v3`;
* `+crt-static`;
* the existing `__isoc23_sscanf` and `__isoc23_strtol` symbol aliases.

### aarch64 Linux musl

* `-C target-cpu=neoverse-n2`;
* disable SVE and SVE2 so binaries run on both Graviton and Apple Silicon Linux VMs;
* matching C CPU flags;
* `+crt-static`;
* the existing symbol aliases.

### aarch64 Darwin

* `-C target-cpu=apple-m1`;
* matching C CPU flag;
* the current rust-lld/ld64.lld configuration;
* safe ICF;
* the configured macOS deployment target.

Preserve `-Z build-std=std` where the current distribution profile requires it.

Build the three product binaries from their owning packages. Do not reintroduce duplicate root binary ownership.

Normalize final artifacts under:

```text
target/<target>/dist/xsh
target/<target>/dist/xsht
target/<target>/dist/xshi
```

when the selected Cargo profile emits them elsewhere.

## Distribution Verification

Every final product must be checked for:

* existence;
* a plausible size of at least 1024 bytes;
* executable mode;
* successful `--help`;
* target executable format;
* target architecture.

For Linux products, additionally verify:

* ELF magic;
* expected ELF machine;
* no dynamic `NEEDED` entries when static linkage is required.

Reuse or absorb the current logic in:

```text
.github/scripts/release-verify-static.xsh
```

Do not keep two separate implementations of the same checks.

The verifier should return structured failure information internally and render a concise command-facing error at the entry point.

## Docker Boundary

The Docker layer is one of the most important proofs in this task.

Outer XSH must own:

* image name;
* image build/reuse policy;
* target platform;
* repository and target mounts;
* Cargo registry volume;
* working directory;
* environment;
* privilege flag;
* host UID/GID;
* direct container argv.

Inside the container, XSH must own:

* target sysroot preparation;
* Git safe-directory setup;
* Cargo invocation;
* artifact normalization;
* target ownership repair;
* cleanup;
* error propagation.

Do not pass a multiline program to `sh -c`.

A valid shape is conceptually:

```text
docker run ... cargo run --quiet -p xsh --bin xsh -- \
  dev/main.xsh -- internal dist-container ...
```

or, where a built container-local XSH binary is already available:

```text
docker run ... target/debug/xsh dev/main.xsh -- internal ...
```

Choose the form that avoids recursion bugs and preserves target-directory reuse.

Container-internal commands must not appear in ordinary help output.


## Installation

### Darwin

Preserve:

* release builds of `xsh`, `xsht`, and `xshi`;
* current feature selection;
* deployment-target environment;
* distribution linker flags;
* copy into `$HOME/usr/bin`;
* executable mode;
* ad-hoc codesigning;
* optional entitlements;
* removal of `com.apple.quarantine` where present.

Use direct `codesign` and `xattr` process calls. A missing quarantine attribute must remain a tolerated condition; signing failure must not be ignored.

### Linux

Preserve:

* staging stripped CRT objects under `target/llvm-crt`;
* `clang`, `llvm-ar`, and lld-based linking;
* existing `-B` search paths;
* current release feature selection;
* installation into `$HOME/usr/bin`.

Use XSH filesystem APIs for copies and directory creation where possible. Continue using LLVM tools for object transformation.

## Release Workflow

Keep GitHub Actions as the remote runner, cache, artifact-transfer, and publication control plane.

XSH should own all repository-specific release policy after the Rust bootstrap.

Preserve the release matrix:

```text
x86_64-unknown-linux-musl on ubuntu-24.04
aarch64-unknown-linux-musl on ubuntu-24.04-arm
aarch64-apple-darwin on macos-26
```

Preserve:

* target installation;
* product smoke tests;
* target-specific test execution;
* static-link verification for Linux;
* artifact naming;
* checksums;
* artifact-set validation;
* core-script packaging;
* pre-release naming as `release-<git SHA>`.

After toolchain setup, replace Make and multiline shell orchestration with `cargo dev` or the built distribution XSH binary.

Minimal bootstrap shell in GitHub Actions is permitted before XSH can run. Checkout, Rust installation, caching, upload/download actions, permissions, and the final GitHub publication boundary may remain platform-native.

Do not leave multiline shell blocks for repository-specific build, test, verification, or packaging logic after bootstrap.


### Binary artifact names

Preserve the existing forms:

```text
xsh-<release-tag>-x86_64-linux-musl
xsht-<release-tag>-x86_64-linux-musl
xshi-<release-tag>-x86_64-linux-musl

xsh-<release-tag>-aarch64-linux-musl
xsht-<release-tag>-aarch64-linux-musl
xshi-<release-tag>-aarch64-linux-musl

xsh-<release-tag>-aarch64-apple-darwin
xsht-<release-tag>-aarch64-apple-darwin
xshi-<release-tag>-aarch64-apple-darwin
```

Each binary artifact must have its matching `.sha256` file.

### Core package

Move the current shell implementation into XSH.

The core package must:

1. discover `core/**/*.xsh`;
2. exclude `core/tests`;
3. sort source paths deterministically;
4. stage them under `core/`;
5. remove the `.xsh` suffix from installed command names;
6. preserve subdirectories;
7. mark installed files `0755`;
8. create `dist/core-<release-tag>.tar.xz`;
9. create `dist/core-<release-tag>.sha256`;
10. reject unexpected compressed artifacts where the existing release workflow does so.

Use XSH filesystem, archive, and hash APIs when they satisfy the exact format. Use `tar` or another explicit native tool when the required xz archive contract cannot be expressed exactly without expanding the language.

### Existing release scripts

Absorb or reuse the logic from:

```text
.github/scripts/release-verify-static.xsh
.github/scripts/release-package-linux.xsh
.github/scripts/release-package-darwin.xsh
```

Do not retain those scripts as independent policy implementations after the shared `dev/` modules exist. Thin compatibility entry points are acceptable only when an external caller still requires them, and they must delegate immediately without duplicating behavior.

## Migrate the Remaining Shell Check

Replace:

```text
scripts/check-libxsh-imports.sh
```

with an XSH-owned check or an integrated `cargo dev check` stage.

Preserve its exact policy against deprecated imports.

Do not rewrite `scripts/ir-layout.py` merely because it is Python. It performs a specialized analysis and is not generic lifecycle glue. Keep it as an explicit tool boundary unless corpus evidence independently justifies replacing it.

## Testing Strategy

Add native XSH tests for the development program.

Register the new test root in `xsht-config.ini`, and ensure all `dev/**/*.xsh` files participate in check, formatting, lint, and runnable-corpus coverage.

Tests must not:

* run real release builds;
* publish releases;
* mutate `$HOME/usr/bin`;
* require Docker merely to validate command construction.

Use existing process mocks where suitable. Otherwise create temporary fake tools that record:

* executable name;
* argv;
* selected environment variables;
* working directory;
* invocation order;
* configured exit status.

Prefer testing the actual effectful XSH code against fake executables over building a large parallel “planned command” abstraction solely for tests.

### Required unit and native-test coverage

Test at least:

* host OS and architecture classification;
* every supported target record;
* default target selection;
* explicit target override;
* Docker platform selection;
* native-versus-Docker distribution selection;
* coverage backend auto-selection;
* distribution output paths;
* feature and flag composition;
* preservation of inherited `RUSTFLAGS`;
* Darwin deployment-target propagation;
* Docker mount and environment construction;
* release artifact naming;
* core source-to-installed-path mapping;
* deterministic artifact ordering;
* Linux ELF-machine expectation;
* unsupported-target diagnostics;
* wrong-directory diagnostics;
* missing-tool diagnostics.


### Required failure tests

Inject failures at representative stages:

* Cargo build failure;
* Docker image-build failure;
* Docker container failure;
* test failure before ownership repair;
* binary missing after a successful Cargo status;
* wrong ELF magic;
* wrong architecture;
* dynamic Linux binary where static is required;
* checksum or package-stage failure;
* codesign failure;
* cleanup failure after an earlier error;
* cancellation while a child process is active.

Assert:

* later stages do not run after a required predecessor fails;
* cleanup still runs;
* the original error remains identifiable;
* no success artifact is reported;
* temporary paths are not leaked where cleanup succeeds;
* child processes are not left running;
* error output names the stage and target.

### Trace coverage

For representative build, Docker, and failure fixtures, assert that XSH traces preserve:

* parent/child process relationships;
* scoped cwd and environment changes;
* process status;
* cancellation where tested;
* propagated failure;
* cleanup work where traceable.

Do not turn every test into a trace golden. Cover the important orchestration relationships.

## Documentation Migration

Update all active documentation and agent instructions from Make commands to the new `cargo dev` interface, including:

* `AGENTS.md`;
* `docs/TEST-MAP.md`;
* `docs/COVERAGE.md`;
* `docs/BENCHMARKING.md`;
* comments in `Dockerfile.test` or workflow files;
* any other exact call sites found during the initial search.

Update `AGENTS.md` so agents remain forbidden from running mutating formatter or autofix commands. Replace references to `make lint` with the new mutating entry point.

Keep documentation focused on command contracts and non-obvious constraints. Do not add a second long guide that duplicates `dev/main.xsh help`.

## Makefile End State

The preferred final state is to remove `Makefile`.

A tiny compatibility shim is permitted only if an active external caller cannot be migrated in this repository. Such a shim must:

* contain no variables;
* contain no target logic;
* contain no conditionals;
* contain no loops;
* contain no shell fragments;
* contain no platform policy;
* delegate directly to `cargo dev`;
* be marked temporary;
* have a test proving it contains no behavior.

Do not keep the existing Makefile “for safety.” Two implementations are worse than one incomplete migration.


## Prohibited Shortcuts

Do not:

* make XSH call `make`;
* move Make recipes verbatim into quoted shell strings;
* retain `sh -c` inside Docker;
* build a generic task-runner framework;
* invent a declarative manifest for tasks;
* encode targets as unvalidated strings everywhere;
* suppress child output globally;
* ignore process statuses;
* turn expected failures into unstructured `abort` calls deep inside modules;
* duplicate target flags across modules and workflow YAML;
* duplicate release verification in CI and XSH;
* use broad `Any` records for closed configuration;
* add Rust host APIs for operations already expressible with XSH plus an external tool;
* rewrite Cargo, Docker, LLVM, tar, codesign, or rustybench functionality;
* stabilize unrelated APIs;
* perform unrelated repository cleanup;
* run `cargo fmt`, `cargo clippy --fix`, `xsht fmt`, `xsht lint --fix`, or the new `cargo dev lint --fix` while implementing the change.

## Implementation Sequence

Use this sequence so the repository remains testable throughout:

1. <redacted>
2. Add the Cargo alias, `dev/main.xsh`, typed target/context modules, help, and native tests.
3. Implement `build`, `check`, and ordinary test dispatch.
4. Implement benchmark dispatch.
5. Integrate the existing XSH coverage programs.
6. Implement native distribution build and verification.
7. Implement Docker execution and container-internal XSH commands.
8. Implement Linux and macOS CI test paths.
9. Implement installation.
10. Consolidate release verification and binary packaging.
11. Implement core-script packaging and artifact-set validation.
12. Migrate `.github/workflows/release.yml`.
13. Migrate documentation and the deprecated-import shell check.
14. Remove obsolete release entry scripts and Make behavior.
15. Run focused tests, the runnable-corpus gate, broad Rust tests, and environment-supported smoke commands.

Do not leave a half-migrated state where CI uses XSH but local development still depends on Make, or vice versa.

## Verification

Follow `docs/TEST-MAP.md` and `AGENTS.md`.

At minimum, run the applicable forms of:

```text
cargo build -p xsh --bin xsh
cargo build -p xsht --bin xsht
target/debug/xsht test --jobs 1 dev/tests
cargo test --test integration runtime::coverage::runnable_xsh_corpus_is_formatted_and_lints_without_warnings
cargo test
git diff --check
```

Then smoke-test the new entry point with inexpensive commands:

```text
cargo dev help
cargo dev check
```

Run real build, test, coverage, distribution, Docker, or benchmark commands only where the current environment supports them and where their cost is justified.

For platform paths unavailable in the implementation environment:

* require deterministic fake-tool tests;
* preserve the real CI invocation;
* report exactly which real command could not be exercised and why.

Do not claim a platform workflow was executed when only its command construction was tested.

## Acceptance Gates

The work is complete only when all of these hold:

1. `cargo dev` is the single documented development entry point.
2. The Makefile is removed or is a logic-free compatibility shim.
3. No lifecycle path invokes Make.
4. No post-bootstrap lifecycle path uses `sh -c` or `bash -c`.
5. Docker container work is implemented by XSH internal commands.
6. Target flags and artifact naming have one typed source of truth.
7. Local and CI workflows use the same XSH modules.
8. Existing coverage XSH code is reused rather than duplicated.
9. Release verification and packaging have one implementation.
10. Core packaging is performed by XSH.
11. Active documentation contains no stale Make commands.
12. Native tests cover target policy, argv and environment construction, failure propagation, and cleanup.
13. The runnable XSH corpus gate passes.
14. Broad Rust tests pass.
15. No language feature, standard API, Rust dependency, or generic task DSL was added.
16. External native tools remain visible, explicit process boundaries.
17. Coverage remains functional after PGO removal.
18. The final diff contains no unrelated formatting or generated-documentation churn.

## Final Report

At completion, report only:

* the new command surface;
* the final module structure;
* which Make and shell paths were removed;
* important design decisions;
* tests and real smoke commands executed;
* platform paths verified only through mocks;
* any genuine existing-XSH pressure discovered;
* remaining limitations.

Do not paste routine command output. Include the exact failing command for any gate that remains unsuccessful.
