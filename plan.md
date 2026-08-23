`dev/` currently has 13 production modules—`build`, `test_workflows`, `coverage`, `dist`, `docker`, `install`, `release`, `verify`, and supporting modules—and none has grown into a giant monolith. The decomposition broadly follows real operational domains rather than arbitrary “utilities.” ([GitHub][1])

I would not try to cut the corpus to 1,000 lines. Much of it is legitimate policy:

* exact Cargo and Docker invocations;
* target-specific linker and CPU configuration;
* artifact naming and verification;
* platform-specific installation;
* cleanup and failure handling;
* CI and release orchestration.

Hiding that behind a generic task runner would make XSH shorter and worse. The explicitness is the point.

The real opportunity is narrower:

> **The module loader is fairly mature. The module composition model is not.**

XSH already has explicit exports, declaration-only imported modules, canonical module loading, load-once behavior, cycle rejection, and a statically represented module graph. That is much more disciplined than the dynamic-module failure mode you are worried about. ([GitHub][2])

But modules are still mostly **files used as namespaces**. They are not yet a fully coherent typed abstraction boundary.

---

## What is already elegant

### The module boundaries are mostly correct

The production code has recognizable ownership:

* `targets.xsh` owns target policy;
* `context.xsh` owns repository and host context;
* `docker.xsh` owns container invocation;
* `dist.xsh` owns distribution builds;
* `verify.xsh` owns artifact proofs;
* `release.xsh` owns release assembly;
* `main.xsh` remains the composition root.

That is the right shape. It is substantially better than a monolithic `dev.xsh`, and it avoids an elaborate framework.

The long explicit dispatch in `main.xsh` is also not inherently clunky. CLI input is an external string boundary. A visible exhaustive dispatch is preferable to a dynamic table of callable objects or a command-registration framework. The problem is only that some of those strings continue deeper into the program after they have already been validated. ([GitHub][3])

### Process behavior remains visible

The corpus generally constructs explicit executables, argument arrays, environment records, and working directories. It does not dissolve orchestration into quoted shell programs or a new task DSL. That aligns closely with XSH’s stated preference for source-visible behavior, explicit process boundaries, effects, and types at module boundaries. ([GitHub][4])

### It is not currently a dynamic mess

Imported modules cannot freely execute arbitrary top-level orchestration during import. Exports are explicit. The loader detects cycles rather than exposing partially initialized module objects. Module dependencies are known to the checker. ([GitHub][2])

Those restrictions are extremely valuable. Preserve them.

The danger is not that XSH has already become Python. The danger is that future ergonomic pressure could be answered with Python-like mechanisms:

* wildcard namespace injection;
* mutable module globals;
* runtime import hooks;
* service-locator records;
* generic callable objects;
* implicit re-exports;
* path-sensitive module shadowing.

Some early pressure toward the first and last of those is visible now.

---

## Where it is genuinely clunky

### 1. `use` is too permissive by default

An unaliased import currently does two things:

```xsh
use context
```

It binds the module namespace, but also injects the module’s exported values, procedures, streams, and types into the importing scope as bare names. An aliased import instead provides qualified access. Internally, the checker explicitly populates both the importing scope and separate qualified-call maps. ([GitHub][5])

That is the most Python-like part of the current design.

It creates several problems:

* provenance becomes less obvious;
* modules can accidentally conflict as they grow;
* adding an export can alter consumers without their import declarations changing;
* reviewers cannot determine ownership from a bare symbol;
* qualification becomes a style convention rather than a semantic guarantee.

For a small systems language, qualification is cheap and highly valuable.

### 2. XSH currently has two module type systems

This is the deepest architectural issue.

A statically imported module alias is represented by the checker as a `Record` containing exported data fields. Its exported procedures, pure functions, and streams are tracked separately in qualified signature maps. By contrast, an explicitly contracted runtime module is represented as `Type::Module(exports)`, and field checking has separate logic for records and modules. ([GitHub][5])

That means, architecturally:

```text
static use import
    ≈ record namespace + side tables

module.load(...).require(Contract)
    ≈ typed Module value
```

This is an inference from the checker representation, but it explains much of the compositional awkwardness: **the useful module-contract mechanism does not naturally describe ordinary static modules.**

As a result, the corpus cannot cleanly say:

> “`build.xsh` needs something satisfying this small process-runner module contract.”

Instead it tends to depend directly on `context.run_stage`, construct generated source programs, or fake executables at process boundaries.

XSH already has the beginning of a much better answer. It just has not joined the two halves.

### 3. Closed states remain strings after parsing

`Target` carries fields such as operating system, architecture, executable format, and target triple as strings. `main.xsh` similarly carries test kind, coverage backend, Docker policy, and release operation as strings. ([GitHub][6])

At the CLI boundary, that is correct. Internally, it leads to repeated checks such as:

```xsh
if target.os == "linux" ...
if options.backend == "docker" ...
if operation == "dist-container" ...
```

XSH already has tagged unions and exhaustive matching specifically to eliminate stringly closed-state logic. Its own specification treats string matching over closed state as something the type system and linter should supersede. ([GitHub][4])

This is not primarily a module defect, but it weakens module interfaces because modules exchange partially validated string records instead of closed domain types.

### 4. `context.xsh` is beginning to become a service module

`Context` itself is reasonable. But `context.xsh` also owns:

* repository-root resolution;
* path construction;
* tool lookup;
* command formatting;
* process execution;
* stage errors;
* directory creation.

Its `run_stage` interface takes an executable, argv, cwd, and generic environment record separately. ([GitHub][7])

That is an early “god context” smell. It is not severe yet, but it points toward a future where anything operational gets placed in `context` because every module already imports it.

The remedy is not a richer `Context` object. That would create a service locator. The remedy is to separate:

* immutable context data;
* command/process execution capability;
* tool resolution;
* filesystem preparation.

### 5. The test corpus is noticeably clunkier than production

The strongest evidence is in `dev/tests/`.

Tests repeatedly construct large anonymous `Context` and `Target` records, sometimes return them merely as `Record`, generate XSH source strings, write temporary scripts, and manipulate `XSH_MODULE_PATH` to substitute behavior. ([GitHub][8])

Some generated-source tests are legitimate because they verify real process and module-loading boundaries. But the frequency of this technique indicates that XSH lacks a clean, statically typed substitution seam for ordinary modules.

That is where a stronger module system would pay for itself first.

---

# The three module-system changes I would make

## 1. Make imports namespace-only

Change the fundamental rule to:

```xsh
use context
```

binds exactly one name:

```xsh
context
```

And:

```xsh
use context as ctx
```

binds exactly:

```xsh
ctx
```

It should no longer inject all exports as bare names.

Usage becomes consistently qualified:

```xsh
export proc build(ctx: context.Context)
    [process, fs, env, io, error]
    -> Result[Unit] {
    context.run_stage(...)
}
```

This includes:

* procedures;
* pure functions;
* streams;
* values;
* record types;
* error types;
* tagged-union constructors.

That last part matters. Namespace qualification must work uniformly; otherwise users will immediately ask for bare imports to recover ergonomics.

I would **not** initially add:

```xsh
use context::{Context, run_stage}
```

Selective imports are defensible in a large general-purpose language, but XSH modules are small and qualification carries useful systems-level provenance. Start with one mechanism.

A migration path can be aggressive because the language is deliberately unstable:

1. Add a lint that reports every bare name originating from an unaliased `use`.
2. Provide a mechanical fixer that qualifies those names.
3. Convert the repository.
4. Change `use` semantics.
5. Delete the compatibility behavior.

This removes the most ambient and collision-prone part of the module system while making the language **smaller**, not larger.

---

## 2. Unify static modules and module contracts

Every statically loaded module should have one inferred module signature.

Conceptually:

```text
ModuleSignature {
    exported_values
    exported_types
    exported_errors
    exported_procs
    exported_pures
    exported_streams
}
```

A static alias such as `context` should be represented by the checker as a proper module value or module namespace with that signature—not as a data `Record` plus several side tables.

Then existing module contracts can become structural requirements over those signatures.

Conceptually:

```xsh
type StageRunner = module {
    export proc run(
        stage: Str,
        command: CommandSpec,
    ) [process, env, io, error] -> Result[Unit]
}
```

A workflow module could accept it:

```xsh
export proc build(
    ctx: context.Context,
    runner: StageRunner,
) [process, fs, env, io, error] -> Result[Unit] {
    runner.run("build", command)?
}
```

The composition root passes the real module:

```xsh
build.run(ctx, stage)
```

A test passes a tiny fake module:

```xsh
build.run(ctx, fake_stage)
```

This gives XSH a restrained, statically checked form of dependency substitution without introducing:

* classes;
* traits;
* closures;
* monkey-patching;
* mutable service containers;
* generic `Proc.call`;
* a test mocking framework.

The contract rules should initially stay simple:

* required exports are checked structurally;
* additional exports are allowed;
* parameter and return types must match;
* declared effects must be compatible;
* static modules remain immutable;
* no module initialization;
* no arbitrary module introspection;
* no constructing modules dynamically from records.

Runtime loading remains explicitly dynamic:

```xsh
let plugin = module.load(path)?
let checked = plugin.require(StageRunner)?
```

The key is that `StageRunner` should describe both a statically imported module and a runtime-loaded module after refinement.

You do not need OCaml-style functors or abstract associated types. The useful synthesis is:

> **Take Rust’s namespace discipline and OCaml’s idea that a module signature is a type, then stop before functors.**

Rust’s import model emphasizes explicit namespace bindings and renaming, while OCaml treats module signatures as types and supports parameterization at the module layer. XSH only needs the first half of the OCaml idea: small structural signatures, not a full higher-order module calculus. ([Rust Documentation][9])

This is the one substantial module-system investment I think the corpus now justifies.

---

## 3. Introduce strict project boundaries

XSH currently exposes `Any`, permits compatibility-oriented flows at concrete boundaries in normal checking, and treats an empty `Record` as deliberately dynamic. Strict checking exists, but `dev` currently invokes ordinary `xsht check`, not `xsht check --strict`. ([GitHub][10])

That is acceptable for shell-edge scripts. It is not the correct default for a canonical multi-module corpus.

I would make `dev/` and Laputa’s package-manager modules strict projects.

Strict module boundaries should require:

* no exported parameter or return type silently degrading to `Any`;
* no empty `Record` at an exported boundary unless explicitly annotated as dynamic;
* no unrefined result of `module.load` escaping its loading module;
* named record types for stable cross-module structures;
* exhaustive handling of tagged unions;
* exact effect checking across module contracts;
* explicit decoding and validation at JSON or process-output boundaries.

This need not force every tiny `core/` script into strict mode. The useful distinction is:

```text
edge script:
    permissive inside, validate where useful

canonical multi-module system:
    strict exported boundaries, localized dynamic edges
```

Module resolution should also become deterministic in strict project mode. The loader currently searches sibling locations and configured/module-path roots and accepts the first matching candidate. `XSH_MODULE_PATH` is useful for tests and ad hoc execution, but ambient path precedence can eventually create shadowing that is hard to inspect. ([GitHub][2])

For strict projects:

* project-configured roots should be authoritative;
* ambiguous matches should be errors;
* diagnostics should show the resolved canonical path;
* ambient `XSH_MODULE_PATH` should be disabled or lowest-priority;
* the resolved graph should be inspectable.

The checker already constructs a compact module graph containing imports, aliases, signatures, types, and diagnostics, so a command such as this would mostly expose existing information:

```text
xsht modules dev/main.xsh
xsht modules dev/main.xsh --graph
```

That is tooling, not language expansion. ([GitHub][2])

---

# How I would refactor `dev/` immediately

These changes do not need to wait for the full module-contract work.

## Replace closed strings with tags

For example:

```xsh
type HostOS =
    | Linux
    | Darwin

type Arch =
    | X86_64
    | AArch64

type TargetId =
    | LinuxX86_64Musl
    | LinuxAArch64Musl
    | DarwinAArch64

type DockerPolicy =
    | Auto
    | Always
    | Never

type CoverageBackend =
    | Native
    | Docker
```

Parse strings once in `main.xsh`, then pass closed types internally.

`Target` can still contain the concrete string values needed by Cargo, Docker, and artifact naming, but policy should match on `TargetId`, `HostOS`, or `Arch`, not repeatedly reinterpret strings.

## Split `context.xsh`

A cleaner division would be:

```text
context.xsh
    Context
    repository paths
    immutable host configuration
    context construction

stage.xsh or exec.xsh
    StageError
    tool resolution
    command execution
    rendering failures

fs_support.xsh
    only if repeated filesystem setup justifies it
```

Do not put a runner field inside `Context`. Keep context as data.

## Replace long scalar argument lists with request records

`docker.internal_argv` currently carries an operation string and numerous separate values. ([GitHub][11])

Prefer something like:

```xsh
type InternalOperation =
    | TestLinux
    | Dist
    | Coverage

type DockerInvocation = {
    operation: InternalOperation,
    target: targets.Target,
    image: Str,
    repository: Path,
    target_dir: Path,
    cargo_home: Path,
    privileged: Bool,
}
```

Similarly, `run_stage` should accept a typed command request—or the existing `Command` value where it supplies the necessary inspection—rather than executable, argv, cwd, and environment as unrelated parameters.

This does not reduce type safety. It makes invalid combinations harder to construct.

## Add typed test fixtures

Create something like:

```text
dev/tests/fixtures.xsh
```

with constructors returning the actual named types:

```xsh
export pure fn linux_x86_context(root: Path) -> context.Context
export pure fn darwin_context(root: Path) -> context.Context
export pure fn target(id: targets.TargetId) -> targets.Target
```

Do not return `Record` when the value is known to be `context.Context`.

After static module contracts work, add one or two fake capability modules rather than generating source for every substitution. Retain generated scripts only where the test is intentionally checking process isolation, import resolution, or executable behavior.

The largest reduction in clunkiness will probably occur in tests, not in the production workflow modules.

---

# What I would explicitly refuse to add

To keep XSH from drifting toward a dynamic general-purpose language, establish these module invariants:

1. **A module is immutable after loading.**
2. **Importing a module performs no arbitrary effects.**
3. **Imports are statically resolvable unless `module.load` is visibly used.**
4. **Every imported name has an obvious qualified owner.**
5. **Cycles remain errors.**
6. **Runtime modules must be refined through a declared contract before typed use.**
7. **Modules are not generic mutable objects.**

And avoid:

* module-level mutable variables;
* import hooks;
* package initializer files;
* implicit directory modules;
* wildcard imports;
* barrel modules that recursively re-export namespaces;
* dynamic method fallback;
* a global module registry;
* dependency injection containers;
* automatically searching arbitrary parent directories;
* first-class functors;
* abstract associated types;
* per-module magic metadata;
* replacing the explicit `main.xsh` match with a registry of dynamic `Proc` values.

“One file is one module” remains entirely adequate at the current scale. The answer is not packages-within-packages or a more elaborate filesystem convention.

## Bottom line

The 2,000-line self-hosting corpus proves that XSH is already viable for substantial systems orchestration. It does **not** reveal a bloated language. It reveals three precise seams:

1. imports inject too much;
2. static module namespaces and contracted modules use different type models;
3. strictness is not yet enforced at canonical project boundaries.

Fix those, refactor the stringly domain states, and clean up test construction. Do not introduce a task framework or a general object/module runtime.

The ideal end state is exceptionally compact conceptually:

> **Immutable file-shaped modules, namespace-only imports, inferred static signatures, small structural module contracts, deterministic resolution, and strict exported boundaries.**

That would make the current `dev/` code materially cleaner—especially its tests—while making XSH **less dynamic and more resistant to entropy**, not more general-purpose.

[1]: https://github.com/laputa-systems/xsh/tree/master/dev "https://github.com/laputa-systems/xsh/tree/master/dev"
[2]: https://raw.githubusercontent.com/laputa-systems/xsh/master/src/loader.rs "https://raw.githubusercontent.com/laputa-systems/xsh/master/src/loader.rs"
[3]: https://raw.githubusercontent.com/laputa-systems/xsh/master/dev/main.xsh "https://raw.githubusercontent.com/laputa-systems/xsh/master/dev/main.xsh"
[4]: https://raw.githubusercontent.com/laputa-systems/xsh/master/docs/SPEC.md "https://raw.githubusercontent.com/laputa-systems/xsh/master/docs/SPEC.md"
[5]: https://raw.githubusercontent.com/laputa-systems/xsh/master/src/sema/check/decl.rs "https://raw.githubusercontent.com/laputa-systems/xsh/master/src/sema/check/decl.rs"
[6]: https://raw.githubusercontent.com/laputa-systems/xsh/master/dev/targets.xsh "https://raw.githubusercontent.com/laputa-systems/xsh/master/dev/targets.xsh"
[7]: https://raw.githubusercontent.com/laputa-systems/xsh/master/dev/context.xsh "https://raw.githubusercontent.com/laputa-systems/xsh/master/dev/context.xsh"
[8]: https://raw.githubusercontent.com/laputa-systems/xsh/master/dev/tests/test-lifecycle.xsh "https://raw.githubusercontent.com/laputa-systems/xsh/master/dev/tests/test-lifecycle.xsh"
[9]: https://doc.rust-lang.org/reference/items/use-declarations.html "https://doc.rust-lang.org/reference/items/use-declarations.html"
[10]: https://raw.githubusercontent.com/laputa-systems/xsh/master/docs/SPEC-TYPING.md "https://raw.githubusercontent.com/laputa-systems/xsh/master/docs/SPEC-TYPING.md"
[11]: https://raw.githubusercontent.com/laputa-systems/xsh/master/dev/docker.xsh "https://raw.githubusercontent.com/laputa-systems/xsh/master/dev/docker.xsh"
