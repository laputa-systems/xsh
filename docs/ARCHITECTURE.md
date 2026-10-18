# Architecture

XSH checks a complete program before execution. Parsing produces an arena
program and a lossless concrete syntax tree (CST). The checker publishes
semantic facts, lowering builds indexed IR, and verification gates execution.
The CST serves formatting and tools that preserve source. The evaluator runs
only the verified indexed program.

[`SPEC.md`](SPEC.md) owns language behavior. This page maps the implementation
and its invariants. [`DESIGN.md`](DESIGN.md) explains syntax choices;
[`TESTING.md`](TESTING.md) owns build and test procedures; [`XSHT.md`](XSHT.md)
and [`XSHT-FMT.md`](XSHT-FMT.md) own tool behavior.

## Packages

The Cargo workspace and its members are listed in `Cargo.toml`. Put a Rust
subsystem in a separate crate when its stable boundary does not depend on XSH
spans, values, diagnostics, or evaluator state. `xsh-net` is the example: its
adapter converts XSH records to Rust requests and turns results back into XSH
data.

## The `libxsh` facade

`src/lib.rs` defines the public library surface. `tests/libxsh_api.rs` protects
it. The supported tier is script execution, diagnostics, traces, and the
narrow host adapters in `xsh::host`. Frontend, IR, and process types also serve
first party tools and remain coupled to the implementation. The compiler,
runtime, syntax, checker, and standard modules stay private behind the facade.
A separate core crate or foreign-function interface needs a concrete consumer.

Core command behavior lives in XSH under `core/`; shared XSH libraries live in
`core/lib/`. Standard-module signatures live in `crates/xsh-registry`, host
operations live in `src/modules/`, and operations that need evaluator state
live under `src/runtime/eval/`. Keep command policy and behavior in XSH. Rust
implements host operations and representation; command-specific performance
shortcuts do not move policy into Rust. A native performance exception needs
an XSH implementation and measurements that identify the smallest operation
needing acceleration.

Module resolution is owned by `src/loader.rs` and `src/project.rs`. The loader
searches beside the importing file, then `XSH_MODULE_PATH`, then project module
roots. The executable does not read the rest of the `xsht` configuration.

## Pipeline

| Stage | Main owner |
|---|---|
| Grammar and syntax tables | `src/syntax/grammar.rs` |
| Lexing, CST, and arena program | `src/syntax/lexer.rs`, `src/syntax/cst.rs`, `src/syntax/parser.rs`, `src/syntax/arena.rs` |
| Module loading and project roots | `src/loader.rs`, `src/project.rs` |
| Semantic checking | `src/sema/check/` |
| Lowering | `src/runtime/eval/lower.rs` |
| Indexed schema and verification | `src/runtime/eval/indexed/full/` |
| Execution and stage scheduling | `src/runtime/eval/lowered_run/indexed_run/` |

The parser keeps source structure for tools. Semantic passes use the arena
program and checker facts. `Checker` owns type, signature, and import-binding
decisions; it publishes selected overloads, argument bindings, types, and
effects for lowering and tools. Later stages consume these facts instead of
repeating semantic decisions.

`FullBuilder` encodes the checked program as indexed rows and pools. A
construction gap is a diagnostic. The builder runs `FullVerifier::verify`
before it returns a program. Verification checks schemas, ranges, ownership,
IDs, and references, then derives the marks used by the in-place evaluator.
Other instructions use the explicit frame scheduler. Runtime decoders rely on
the verified representation.

Indexed IDs refer to program-owned tables. `SymbolOwner` owns dynamic names.
The installed program retains its source map, but not the CST, arena program,
or checker output.

`src/stdlib.rs` embeds script-backed standard modules at build time, so an
installed XSH does not read a stdlib tree at runtime. The loader selects modules
required by source use and sends them through ordinary checking and lowering.
The registry can also bind an entry directly to a native runtime operation.

## Invariants

- Verify the indexed program before installation or execution. Malformed or
  unsupported data fails preparation.
- Make each semantic decision once. Routes that must agree share an
  implementation or have parity coverage. Lowering consumes checker facts.
- Preserve order visible to XSH programs, error kinds and spans, cleanup,
  status, and trace events through lowering and runtime routes.
- Give each construct one core meaning. A sugar form has one core expansion.
- Keep host effects at their boundary. The evaluator interprets structured
  host outcomes as XSH control flow.

## Adding a language feature

1. Define observable behavior in the SPEC template and its snippets. Follow
   [`TESTING.md`](TESTING.md) to regenerate derived documentation.
2. Add syntax in `src/syntax/`. Preserve the CST, update the exhaustive
   `AstArena` child enumerator, and update traversals that depend on binding
   order, callable edges, command boundaries, or scope transitions.
3. Add checker behavior in `src/sema/check/` and publish the facts later stages
   need.
4. Add indexed rows in `src/runtime/eval/indexed/full/schema.rs` when the
   feature needs a new representation. Implement lowering, verifier checks,
   and evaluator behavior.
5. Update `crates/xsht/` tools that handle the new form. Add focused language
   and verifier coverage, then run the narrow gates in [`TESTING.md`](TESTING.md).

## Adding a sugar form

A form is sugar only when existing core forms provide its complete meaning,
including evaluation order, effects, and failures. The parser retains the
written operands and a core expansion. Semantic passes use the expansion;
source-aware tools use the operands. A form that binds in the enclosing scope
or owns resource cleanup stays core. See [`DESIGN.md`](DESIGN.md) for the
syntax rationale.

## Adding a validated type

`src/sema/validated.rs` owns validation identity and base-type rules.
Value-based validators such as `IntRange::contains` and `is_rel_path` are
shared by literal, constant, and runtime checks. `src/runtime/eval/validated.rs`
uses the runtime value view. `FullVerifier` checks the indexed type and schema.
The SPEC owns the observable behavior.

## Runtime design

The evaluator owns values, scopes, result propagation, cleanup, resource
handles, and trace parentage. `src/runtime/process.rs` and
`src/runtime/run.rs` own process creation, process groups, terminal handoff,
waits, cancellation, and detached reaping. `src/runtime/signal.rs` records
signals in the handler; the evaluator interprets them at checkpoints. Process
groups are the unit of cancellation.

Linux post-fork setup applies a plan prepared in the parent. Child setup uses
async-signal-safe operations and performs no allocation or locking. Keep XSH
code out of the signal handler and the post-fork child path.

Resources belong to their creating scope until transferred to an enclosing
scope. Scope exit stops owned activities before user defers; handles remain
usable during defers and are released afterward. Explicit `close` and `unlock`
results flow to the script. Automatic cleanup does not replace the script
result with a release error.

Network workers receive plain Rust requests. They do not own evaluator values,
source spans, or trace buffers. The evaluator owns job lifetimes and trace
events.

The serial pipeline pulls one item at a time. Materializing stages and
`par-map` cross into the evaluator's stage engine. Stages resume through
continuations, and yield delegation and cancellation stay iterative so producer
depth does not become native stack depth.

Before adding a host integration, name its owner, checkpoint behavior, cleanup
responsibility, signal interaction, result shape, and trace evidence.
