# Lowered IR Guide

This is the focused map for interpreter-speed work. The lowered IR is an
acceleration layer for eligible pure functions and selected whole-script
regions. It is not a bytecode format, not a serialized program representation,
and not a replacement for the tree-walking evaluator.

## Architecture

XSH executes through an AST-first pipeline:

1. `src/syntax` reads source into the AST and applies syntax-level
   normalization.
2. `src/sema` checks names, types, purity, module APIs, records, errors, and
   stream contexts.
3. `src/runtime/eval.rs` registers checked `proc`, `pure`, type, and error
   definitions.
4. Function lowering first produces explicit `LoweredFunctionUnit` values keyed
   by `LoweredFunctionKey`. Each unit records its function kind, dependency
   edges, SCC metadata, and blocker reason before runnable bodies are committed
   into the lowered function maps.
5. `Evaluator::refresh_lowered_pures` repeatedly lowers eligible `pure`
   functions and restricted effect-free `proc` bodies after currently known
   definitions are registered, then refreshes the script-level lowered cache
   against the current lowered function set.
6. `eval_use`, `import_user_module`, and `loaded_module_record` register module
   exports, then retry lowering so newly importable helper functions can
   unblock callers and later script statements.
7. `src/runtime/eval/call.rs` attempts the lowered call path.
8. `Evaluator::eval_maybe_lowered_stmt` attempts the top-level lowered
   statement path.
9. Unsupported functions, unsupported statements, and gated runtime contexts
   fall back to AST evaluation.

The current shape is effectively:

```text
source -> parse/desugar AST -> semantic check -> register functions
       -> lower eligible functions to LoweredFunctionUnit records
       -> commit runnable per-function IR bodies
       -> evaluate `use` statements and retry lowering with module exports
       -> lower eligible restricted proc bodies to the same internal shape
       -> call lowered functions when runtime gates allow it
       -> execute eligible top-level script statements through lowered IR,
          including calls to lowered restricted procs
       -> otherwise evaluate checked AST
```

The script or module is already loaded as checked AST before lowering. The
first lowering unit is one `pure` function body. Restricted `proc` bodies with
no host effects other than `error` can lower to the same representation for use
from script-level IR. The script-level lowering unit is one eligible top-level
statement, synchronized against normal runtime bindings. This keeps OS effects,
cwd/env state, process forms, tracing, defers, signal hooks, and stream
callbacks on the normal source-order evaluator while letting pure,
effect-free-proc, and script-level glue code skip repeated AST dispatch where
semantics are already represented.

Think of the lowered IR as a checked-AST acceleration cache. XSH still reads the
whole script/module graph into memory as AST before execution. There is no
streaming compiler, bytecode image, instruction dispatch loop, or serialized
intermediate format.

## Symbol Identity And Registry Source Of Truth

XSH uses compact interned symbols for identifier-like names. Source text stays
human-readable, but checked AST, semantic maps, runtime binding maps, and
lowered IR paths compare `Name` values by integer identity instead of repeatedly
hashing or comparing UTF-8 strings. Qualified identities use
`QualifiedName { namespace, member }`; IR, checker, and runtime code should keep
that binary identity through semantic paths and reserve formatted
`module.member` strings for diagnostics, docs, and other display boundaries.

`src/symbol.rs` defines:

- `Symbol`: a compact `u32` raw id.
- `Name`: a copyable wrapper around `Symbol`.
- `QualifiedName`: a pair of `Name`s for `namespace.member` identities.

The first ids are the fixed core builtin prefix. Their raw values are stable and
must not be reordered because constants such as `Name::INT` depend on them.
`Name::is_builtin()` only checks that fixed prefix.

Additional standard names are preloaded at build time. `build.rs` asks the
language-facing registry in `crates/xsh-registry` for the ordered preloaded
symbol list, then writes `OUT_DIR/preloaded_symbols.rs` with:

- one 64-byte-aligned ASCII byte blob for all preloaded symbol text;
- one compact range table indexed by `Symbol::raw()`;
- counts for the fixed builtin prefix and the full preloaded table.

The runtime interner initializes an `FxHashMap<&'static str, Symbol>` from
slices into that static blob. Resolving a preloaded `Name` slices the blob by
range. The generated table is ASCII-validated in `build.rs`; ASCII is valid
UTF-8, so the resolver keeps the conversion from bytes to `&str` in one small
checked boundary.

Dynamic symbols still support user-defined names and names that are not known at
build time. They keep the original safe lifetime model: text is copied into a
boxed string and leaked for process lifetime. Dynamic symbol ids follow the
preloaded range, and the interner stores direct `&'static str` references for
those dynamic symbols. This is less compact than an arena, but it avoids custom
unsafe storage while preserving the `Name::as_str() -> &'static str` contract.

The north-star invariant is:

> Adding a language-facing API name happens in exactly one place, and the
> checker, runtime, generated docs, coverage data, and preloaded symbol table
> all observe it automatically.

`crates/xsh-registry` is the authoritative language-facing registry. It owns
typed metadata for fixed core builtin symbol names, fixed semantic names such as
`Ok`, `Err`, `args`, `ARGV`, and `main`, standard module/function/method
signatures, parameter definitions, standard record schemas, builtin error
families, builtin type names, and runtime operation ids. The registry lives
outside the main `xsh` crate so Cargo build scripts and runtime/checker/docs
code can consume the same definitions without creating a build cycle.

The registry should remain Rust-native unless a stronger reason appears. XSH's
API metadata includes type expressions, runtime operation ids,
receiver-specific method behavior, command-callability, and special argument
checks. Keeping those definitions typed gives normal Rust compile-time checking
and avoids replacing one fragile string layer with another.

The invariant is achieved only when drift is mechanically impossible or caught
by tests. A migration is not done just because data moved from `src/modules` to
`crates/xsh-registry`; it is done when the old copy no longer exists and the
remaining consumers either read the registry directly or are covered by exact
adapter-equivalence tests.

The registry proof must cover these surfaces:

- preloaded symbols are generated from the registry and include every registry
  API, record, error, and builtin type name;
- checker and runtime API adapters expose exactly the registry metadata after
  converting from registry types into local runtime/checker types;
- standard record values produced at runtime match the registry schemas they are
  advertised to implement, enforced by strict debug/test return validation at
  standard module and method boundaries;
- builtin type-name decisions are made through a registry-owned typed enum, so
  the type checker and runtime cannot silently diverge through string literals;
- build-time symbol generation does not parse source files or maintain parallel
  source lists.

String output for diagnostics, docs, parser tokens, and runtime record field
values is still a boundary concern. Stringly-typed semantic matching inside the
type system is not acceptable: matching `Int`, `Path`, `Result`, `Error`, or a
standard record by raw string in more than one place reintroduces the same class
of drift that the registry is meant to remove.

Current registry migration status:

- fixed core and semantic symbol names live in `crates/xsh-registry`;
- builtin process error family metadata lives in the registry and the checker
  registers error definitions from it;
- standard record schema definitions live in the registry and
  `record_schemas()` is derived from that data;
- standard module and method names, parameters, signatures, and `RuntimeOp`
  values live in the registry while `src/modules/signature.rs` remains the
  checker/runtime adapter;
- debug/test runtime module and method calls validate successful values against
  their selected registry return signatures, including exact field sets for
  non-empty record types;
- `build.rs` generates `preloaded_symbols.rs` from
  `xsh_registry::symbols::preloaded_symbol_names()`, with no source extraction;
- `docs/SPEC.md` names the registry as the authoritative source for
  language-facing API metadata.

Future work should keep reducing adapter code where it no longer carries useful
semantic boundaries, but it must not recreate parallel hand-maintained name
lists. Keep `docs/SPEC.md` in sync with the implemented registry contract
whenever language-facing behavior changes.

## Runtime Gates

A lowered body can exist and still not run. `call_lowered_pure` uses the fast
path only when:

- tracing is disabled;
- the call is not an entry call through module context that changes behavior;
- arguments convert into the `LoweredValue` set for the function signature;
- the function successfully lowered.

Lowered bodies may still call other lowerable functions, including qualified
helper pures such as `auth.invalid_option`, when the callee body lowered
successfully. That nested call uses the already-lowered callee body; it does not
bypass the entry gate for module-owned calls from normal runtime dispatch.

Restricted procs are lowerable only when their effect annotation is `[]` or
contains only `error`. Unrestricted procs and procs with `fs`, `io`, `net`,
`process`, `env`, or `time` remain on the AST path. Ordinary proc dispatch still
uses the AST evaluator; lowered proc bodies are currently used only by lowered
script regions, with tracing disabled and no module-context gate active.

The fallback contract is strict: unsupported syntax, types, methods, or runtime
conditions must return `None` from lowering or opt out at the call gate. They
must not partially interpret different semantics.

Script-level lowered statements use the same conservative contract. The current
cache lowers typed top-level bindings, assignments, expressions, and lowerable
`if`/`while`/`for`/`match` statements. Explicitly typed top-level bindings and
untyped bindings with unambiguous lowered initializer types become
synchronization slots for later lowered script statements. Each lowered
top-level statement reloads slot values from normal runtime bindings before
execution and writes back only tracked mutable slots after execution. Tracing
disables the script-level fast path, and lowered loops service the same signal
checkpoints as AST loops.

## Critical Files

`src/runtime/eval.rs`
: The hub: defines the `Lowered*` IR types (the shared vocabulary), the
  `Evaluator`, the lowering registry/bridge (`refresh_lowered_pures`,
  `call_lowered_pure`), the `LOWERED_METHOD_NAMES` whitelist, and AST-evaluator
  glue. The lowered-IR subsystem was split out of here into the sibling modules
  below; the IR types stay here so all of them can reach the types (and their
  private fields) via `super::`.

`src/runtime/eval/lower.rs`
: The AST→lowered-IR lowering pass: `LoweredPureFunction::lower`,
  `LoweredProgram::lower`, `lower_stmt`, `lower_expr`, the top-level lowering, and
  `SlotScope` (which owns the dense slot-allocation, scope enter/exit, and
  `\0`-sentinel high-water invariants).

`src/runtime/eval/lowered_run.rs`
: The lowered evaluator — `eval_lowered_function`/`_stmt`/`_expr`/`_typed_int`/…
  as a second `impl Evaluator` block. The lowered `Call` errors
  (`unresolved-lowered-call`) rather than falling back per-call, so callees must
  be lowered before callers (see mutual-recursion note under Open performance work).

`src/runtime/eval/lowered_ops.rs`
: Pure value operations and method dispatch over `LoweredValue`
  (`lowered_*_method_value`, `lowered_value_matches`, binary/conversion helpers) —
  no `Evaluator`/`self`.

`src/runtime/eval/call.rs`
: Contains `call_registered_function` and `call_lowered_pure`, the dispatch gate
  between lowered pure calls and AST function calls. Ordinary proc calls stay on
  the AST path; eligible restricted proc bodies are invoked only from lowered
  script regions.

`src/runtime/eval/stmt.rs`
: Imports user modules, evaluates module exports, registers qualified exported
  procs/pures, and retries lowering after those exports are known.

`src/runtime/value.rs`
: Defines public `Value`. Boundary conversion must preserve this representation
  exactly, including `Result`, `Error`, `Path`, records, lists, and maps.

`src/symbol.rs`
: Defines `Symbol`, `Name`, and `QualifiedName`, initializes the generated
  preloaded symbol table, and owns the dynamic symbol interner. IR/checker/runtime
  semantic paths should carry `Name` or `QualifiedName`, not reconstructed
  dotted strings.

`src/syntax/arena.rs` and `src/syntax/node.rs`
: Define the source surface. `arena.rs` stores statement, expression, pattern,
  command, run, and call forms as compact arena IDs and arena node kinds.
  `node.rs` holds shared leaf syntax such as stream stage kinds, operators, and
  type-expression leaves.

`build.rs`
: Generates `OUT_DIR/preloaded_symbols.rs` from
  `xsh_registry::symbols::preloaded_symbol_names()`. It must stay a registry
  consumer, not a source scraper or parallel semantic-name list.

`crates/xsh-registry/src/signature/*` and `crates/xsh-registry/src/runtime_op.rs`
: Define standard API signatures and `RuntimeOp` identifiers. Use these to find
  normal method semantics before adding a lowered method.

`crates/xsh-registry/src/symbols.rs` and `crates/xsh-registry/src/types.rs`
: Define registry-derived symbol collection and builtin type-name identities.
  Type-name decisions in checker/runtime code should route through these typed
  registry representations.

`src/runtime/eval/methods.rs` and `src/runtime/eval/modules.rs`
: Contain normal runtime dispatch for methods and stateful module operations.
  Lowered methods should mirror this behavior or stay unsupported.

`crates/xsh-registry/src/records.rs`
: Defines standard record schemas. The Rust lowerer treats standard records as
  lowered record values when their fields are representable.

`tools/xsh-ir-coverage.xsh`
: Programmatic expansion map. It scans pure functions, restricted proc bodies,
  and top-level script statements in this repository, `../packages`, and
  `../laputa`, extracts the lowered surface from Rust source, and reports real
  fallback reasons.

`perf/interpreter/`
: End-to-end hyperfine scenarios for CLI startup, parse/check, and evaluation
  together. Add or update a scenario whenever IR coverage expands.

## Frontend Benchmarking

Use the Criterion frontend group when changing parse, desugar, semantic check,
lowering, or evaluator setup before top-level execution:

```sh
cargo bench --bench bench frontend -- --sample-size 10 --warm-up-time 0.5 --measurement-time 1
```

The benchmark prints an allocation audit before Criterion timing. Treat
allocation count and allocated bytes as the stable signal; local Criterion means
are useful directionally but move enough between nearby runs that they should
not gate small changes by themselves.

The scenarios cover:

- `empty`: fixed parser/checker/evaluator setup.
- `loop_10k`: small top-level loop shape.
- `pure_call_chain_20k`: lowered pure registry and call-chain setup.
- `stream_callback_pure_5k`: large literal/list front-end pressure plus lowered
  pure callbacks.
- `mixed_glue_2k`: record/map/string glue with a lowerable pure.
- `import_disk`: file reads, module parsing, and module resolution.
- `small_corpus_le200_lines_16k`: checked-in standalone `.xsh` files with at
  most 200 lines and 16 KiB, filtered to sources that parse and check cleanly as
  individual scripts. This is the floor-overhead lens for ordinary small-script
  front-end work. The parse/check/lower helper uses an explicit cwd so the
  corpus does not measure host `current_dir()` syscall latency once per file.

To run only the small-script floor lens:

```sh
cargo bench -p xshi --bench bench small_corpus -- --sample-size 10 --warm-up-time 0.5 --measurement-time 1
```

As of the 2026-06-15 baseline after the string-churn cleanup, empty
`parse/check/lower` is close to the fixed floor at about 68 allocations. The
remaining interesting `+lower` allocation counts are concentrated in
`pure_call_chain_20k`, `mixed_glue_2k`, and `stream_callback_pure_5k`; small
bookkeeping cleanups are unlikely to move the suite much.

Good remaining ideas:

- Reduce boxed lowered expression/statement representation where it duplicates
  the checked AST. This is the most likely IR-local win.
- Audit lowered vectors and maps for empty or single-item cases before adding a
  dependency; prefer existing local structures unless the benchmark shows a
  clear win.
- Consider compact token payload storage only if identifier/string-heavy
  scenarios still allocate meaningfully after IR-local work.
- Treat arena-backed AST storage as a larger parser/checker project. It can pay
  off, but it crosses formatter, diagnostics, checker, and runtime ownership.

Lazy cwd remains a runtime-startup question rather than a front-end lowering
question: the small-corpus frontend lens now excludes per-file cwd lookup, while
cwd/path/process semantics still need a concrete current directory when a script
actually executes.

## Core Structures

`LoweredPureFunction`
: The lowered unit used for pure functions and eligible restricted proc bodies.
  Stores parameter names and kinds, return kind, slot count, and lowered body.

`LoweredProgram`
: The script-level lowered cache. Stores one optional lowered representation per
  top-level statement, so unsupported statements can fall back independently.

`LoweredTopLevelStmt`
: A lowered top-level statement plus the runtime binding slots it needs to sync
  before and after execution.

`LoweredType`
: Small type-kind set accepted at IR boundaries: unit, `Int`, `Bool`, `Str`,
  `Regex`, `Status`, path, error, record, list, map, and tag-union shapes.
  Named user types resolve through the runtime type-definition table; qualified
  type expressions resolve through `QualifiedName` keys in
  `qualified_type_defs`, not by synthesizing flattened `module.Type` names.

`LoweredReturnKind`
: Distinguishes plain returns from `Result[T]` returns so implicit result
  wrapping and error propagation match AST evaluation.

`LoweredStmt`
: Slot-oriented statement IR for `let`, `var`, assignment, `if`, `while`,
  `for`, `match`, `return`, `break`, and `continue`.

`LoweredStmtFlow`
: Internal statement-control result used by the lowered evaluator. Extend it
  only when the matching source control flow is implemented end to end.

`LoweredExpr`
: Expression IR for scalars, slots, binary operations, `if`, `match`, records,
  lists, tag constructors, simple list comprehensions, selected list
  pipelines, field/index access, selected methods, `Ok`/`Err`, `?`, `??`, and
  lowerable function calls. Dotted calls lower as qualified function calls only
  when the qualified name is in the lowered function set; otherwise dotted
  calls remain receiver methods and must pass `lowered_method_name`.
  Qualified type/tag lookup uses the same binary `QualifiedName` identity as
  function calls when the source type definition is qualified.

`LoweredPattern`
: Pattern subset for lowered matches. It supports wildcard and literal patterns,
  plus tag-union constructor patterns with simple binding or tuple-of-binding
  payload fields.

`LoweredPipelineStage`
: List-pipeline IR. Current stages are the `text.lines` adapter, `where`,
  `map`, `group-by`, `enumerate`, `take`, `drop`, `sort`, and `sort-by`;
  item-expression stages use a synthetic item slot for `.`, and `map` can also
  lower a simple one-parameter block with local `let`/assignment setup before
  its tail value.

`LoweredValue`
: Runtime value set used by the lowered evaluator. It intentionally avoids the
  full `Value` enum in hot paths. Boundary conversion is centralized in
  `lowered_value_from_runtime`, `lowered_return_value`, and
  `LoweredValue::into_value`.

## Extension Points

`LoweredPureFunction::lower`
: Entry point for one checked pure definition or eligible restricted proc body.
  Rejects unsupported signatures or bodies by returning `None`.

`LoweredProgram::lower`
: Entry point for the top-level script cache. Keep it statement-granular and
  fallback-friendly; do not require the whole script to lower before using
  lowered regions.

`lower_stmts`, `lower_stmt`, `lower_expr`
: The lowering pass. Keep these conservative and structural. If a construct is
  not completely represented, return `None`.

`lower_type_expr`
: Converts checked source type syntax into `LoweredType`. Standard records and
  user records are represented as `LoweredType::Record` when field values are
  lowerable. Qualified source types must be resolved through the qualified
  type-definition table, with text formatting reserved for diagnostics and
  user-visible output.

`lowered_method_name`
: Whitelist used by the lowerer and coverage tool. Add a method here only after
  `lowered_method_value` or `lowered_method_ref` implements exact behavior.

`lowered_method_value`, `lowered_method_ref`
: Receiver-method dispatch. Use `_ref` when slot-backed receivers can avoid
  cloning large lists, maps, or records.

`eval_lowered_function`, `eval_lowered_stmts`, `eval_lowered_expr`
: General lowered evaluator. Keep spans, `Result` propagation, type errors, and
  error values aligned with the AST evaluator.

`eval_lowered_fast_plain_return`, `eval_lowered_fast_return`
: Specialized fast paths for simple returns. If a new expression cannot be
  safely handled here, return `None` and let the general lowered evaluator run.

## Implemented Behavior

The lowered path currently covers the main pure glue shapes: scalar arithmetic
and comparisons, local variables, loops, branches, matches, records, lists,
maps, tag unions, simple list comprehensions, selected list pipelines including
`text.lines`, `group-by`, and filter/map block normalization, selected string/
regex/status/path/record/list/map/result methods, `regex.compile`, error
construction, result propagation including `Result[Unit]`, implicit pure tail
expressions, expression-only pure tail `match` statements, and calls between
lowerable pure functions. Recent value-method coverage includes string
`reverse()` and map `values()` in addition to existing collection helper
methods such as `extend()` and `keys()`. Tight text scanners also have
specialized lowered expressions for `Str.byte_len()` and positional
`Str.byte_at(...)`, plus a typed integer node for slot-backed
`Str.count_lines()`, so byte-offset and line-count loops do not pay the generic
lowered method dispatch cost on every hot call. Unary `-` and `!` lower into equivalent primitive
expressions, which keeps scanner initializers such as `var delim = -1` from
blocking the whole function. Scanner-shaped `Int` and `Bool` lets, assignments,
branches, and while conditions can use typed lowered statement variants when
the value shape proves the primitive type; ambiguous bare-slot `=` assignments
fall back to the generic lowered statement path. Typed boolean lowering also
uses direct slot-backed string predicates for `s.starts_with(...)`,
`s.contains(...)`, `s.trim() == ""`, and `s.trim().starts_with(...)` /
`s.trim().ends_with(...)` with literal needles. Repeated lowered pure/helper
calls recycle slot-frame allocations to reduce allocation pressure in helper
pures called inside loops. Qualified helper calls and qualified source type
annotations keep their binary identities through lowering and runtime
validation; the lowerer should not rebuild `module.member` names except at
display boundaries. `for line in text.lines()` can lower to a streaming
statement form that evaluates the receiver once and iterates Rust string lines
without first materializing the full `List[Str]`; ordinary `.lines()` calls and
pipeline stages still return lists when the source program asks for a list.
Lowered string slots can also hold internal string views, so line-loop items and
`trim()` results can borrow ranges from the original text and materialize owned
`Str` values only when they escape the lowered string path.

Restricted `proc` bodies with `[]` or `[error]` effects can lower when their
signatures and bodies fit the same value/control-flow subset. These lowered proc
bodies are callable from lowered script regions and can return plain values or
`Result[T]`, including result propagation with `?`. They are not used for normal
proc dispatch, tracing, module-context entry calls, unrestricted procs, or procs
with host effects.

At script scope, the lowered path covers top-level scalar/list/map/record
bindings with explicit lowerable types or obvious inferred types, assignments,
expressions, and lowerable branch/loop/match statements. Untyped bindings are
exposed as slots only when their initializer has an unambiguous lowered type,
such as a literal, simple collection, compatible branch expression, or a plain
return from an already-lowered pure or restricted proc.

It intentionally does not cover process forms, effectful module calls, tracing
behavior, stream callbacks with block parameters, arbitrary lambdas, ambient
cwd/env mutation, broad untyped script binding inference, `Bytes`, or host
objects such as `Command` and `Duration`. Current `Bytes` fallback counts come
from effectful `fs`/`io` procs, so adding a lowered byte value alone would not
make those bodies eligible for the fast path.

Run the coverage tool for exact current numbers:

```sh
target/release/xsh tools/xsh-ir-coverage.xsh -- --json target/perf/ir-coverage.json
```

The pure-function percentage remains separate from the top-level script
lowerability percentage, and restricted proc-body coverage is a third number.
All three are expansion maps, not whole-language coverage.

## Whole-Script Coverage

For XSH, "whole-script IR coverage" does not mean every language feature runs
through IR. It means every executable script region is either lowered or has an
explicit, measured, documented fallback reason.

The coverage boundary is:

1. Executable top-level script regions: top-level bindings, assignments,
   expressions, and control statements are counted independently.
2. Pure/effect-free bodies: pure functions and restricted proc bodies are
   reported separately because their call gates and source contracts differ.
3. Imports and modules: `use` statements are runtime boundaries that load AST
   modules and refresh lowering after exports are registered; they are measured
   as script fallback points, not hidden IR work.
4. Runtime gates: tracing, module-context entry calls, unsupported argument or
   return values, and unsupported body shapes keep the AST evaluator in charge.
5. Benchmarks: IR expansion requires a corpus hit and a hyperfine scenario that
   exercises the new surface without measuring unrelated subprocess cost.

The coverage tool is conservative. It scans source shape and lowered capability
lists; it does not prove that a specific dynamic execution used IR, and it does
not model every checker fact. Top-level script scanning groups obvious
continuation regions such as multiline structured pipelines, list literals, and
argument lists, and it skips triple-quoted string contents and proc/pure
definition bodies before assigning fallback reasons. Treat false negatives as
acceptable prompts for manual inspection, not as language failures.

Each pure, restricted-proc, and top-level script report includes both raw
fallback reasons and grouped summaries. The groups separate import boundaries,
explicit runtime boundaries, proc effect boundaries, methods, types,
expressions, statements, and uncategorized leftovers so intentional AST-first
regions are not mixed with actionable lowering candidates.

## Active Roadmap

The lowered IR should remain serious, lean, and inspectable before considering a
bytecode VM.

1. Keep the AST as the semantic source of truth. Lowered IR must be a runtime
   acceleration cache with precise fallback.
2. Broaden typed value, method, and statement coverage only when there is a
   corpus hit, benchmark case, and exact parity with the AST evaluator.
3. *(Done.)* `src/runtime/eval.rs` was split into focused modules — `lower.rs`
   (lowering + `SlotScope`), `lowered_run.rs` (lowered evaluator), `lowered_ops.rs`
   (value ops + method dispatch), and `tests.rs` — dropping it from ~13.8k to
   ~5.0k lines, with the IR types kept in the hub. See Critical Files.
4. Improve programmatic coverage precision for top-level regions and proc
   bodies, especially around imported module exports, effect annotations, and
   method false positives from line-based scanning.
5. Continue building mixed glue benchmarks around package imports, JSON/records,
   maps, path and regex helpers, result-heavy validation, stream callback setup,
   and command-adjacent orchestration with minimal actual process work.

## Performance Methodology

The lowered IR exists to make ordinary XSH shapes — local scalar loops, string
and byte predicates, `for line in text.lines()`, small pure helpers, and
`par-map |> reduce-by` aggregation — cheap enough for file-oriented and
text-scanning workloads, without growing the standard library with
benchmark-shaped primitives. The AST stays the semantic source of truth; lowered
IR is an acceleration cache with precise fallback.

Decisions are made on release measurements, never debug timing. Debug timing is
directional — enough to find a likely bottleneck, never enough to keep or reject
a change. Every accepted change should name the exact cost it reduces, preserve
output parity (and computed-value parity against any reference implementation),
carry direct lowered/runtime tests with exact AST parity, and report a release
A/B delta for the affected command or fixture. Flat or regressing changes should
be reverted, or kept only with a note explaining why the representation is still
worthwhile (coverage or consistency rather than speed). See **Benchmark Loop**
for the mechanical loop.

### Performance tooling

Decisions are release A/B; the harnesses below find and attribute the cost. The
forcing benchmark is `showcase/tokei.xsh` over a large corpus (`/Users/josh/dev/sentry`)
versus native release tokei. It is an interpreter/lowered-IR goal, **not** a
license to grow the standard library with benchmark-shaped primitives.

**Release A/B vs native tokei** (the decision currency — release only):

```sh
hyperfine --warmup 2 --runs 7 \
  'target/release/xsh showcase/tokei.xsh -- /Users/josh/dev/sentry > /dev/null' \
  '/Users/josh/d/tokei/target/release/tokei /Users/josh/dev/sentry > /dev/null' \
  'target/release/xsh showcase/tokei.xsh -- --json /Users/josh/dev/sentry > /dev/null' \
  '/Users/josh/d/tokei/target/release/tokei /Users/josh/dev/sentry -o json > /dev/null'
```

**Per-function instruction attribution** (Linux/Docker; deterministic Callgrind):

```sh
make prof-callgrind SCENARIO=extension-count
# annotated per-function Ir in target/prof/callgrind.extension-count.txt
```

Callgrind instruction counts (`Ir`) are deterministic, so they double as the
bytecode-VM decision gate and the before/after regression signal
(`make prof-compare`). For per-call-stack allocation attribution use
`make prof-dhat` (in-process dhat). See `perf/README.md`. **Caveat:** these
profile the chosen scenario, so profile a `--json` run separately for report
cost, and thin-LTO inlining can smear small hot functions into their callers.

**Allocation volume** (instrumented mimalloc; the value-movement axis):

```sh
target/release/xsh perf/run.xsh                    # builds --features perf-metrics
target/release/xsh perf/allocation-compare.xsh -- perf/allocation-baseline.json \
  target/perf/<stamp>/allocation.json
```

**Which functions lower / which fall back.** `tools/xsh-ir-coverage.xsh --root .
--json /tmp/ir-cov.json` is a heuristic map — method coverage is reliable, but
some *type* reasons are stale (e.g. it reports `type.param.Bytes` as unlowerable,
which is false). To prove a specific function lowered, verify empirically:
env-gate an `eprintln!` in `refresh_lowered_pures` listing `self.lowered_pures`
keys vs the rejected `self.pures`, debug-build, and run on a small directory.
(This is how the `map.empty()`, mutual-recursion, and `TailBareIdent` blockers
were each found.)

**Verification gate** for any lowered-IR or stream change, at least:

```sh
cargo check
cargo test --lib            # + 2 KNOWN pre-existing failures:
                            #   interactive::app::tests::expands_shell_arithmetic
                            #   modules::tests::standard_module_contract_snapshot_is_stable
target/release/xsht check showcase/tokei.xsh
target/release/xsht test tokei --nocapture        # JSON shape/counts/ignores parity
target/release/xsht fmt --check showcase/tokei.xsh showcase/tests/test-tokei.xsh
```

Format Rust with `rustfmt --edition 2024 <changed files>`, **not** crate-wide —
the repo has pre-existing fmt drift, and a crate-wide `cargo fmt` churns unrelated
files. New lowered behavior needs direct lowered tests with exact AST parity
(mirror the `RuntimeOp` implementations in `src/runtime/eval/methods.rs` and
`src/modules/`), and `--json`/totals output parity must be preserved
(canonicalized; totals-vs-native-tokei differences are pre-existing
language-detection/ignore semantics, compared separately — do not "fix" them).

### Current baseline and gap

Release `showcase/tokei.xsh` over `/Users/josh/dev/sentry` vs native release
tokei (warm, hyperfine, 7 runs):

| path | XSH wall | native wall | gap | XSH user CPU |
| --- | --- | --- | --- | --- |
| default table | ~0.59 s | ~0.77 s | **~1.31× faster** | ~1.23 s |
| `--json` | ~0.83 s | ~0.83 s | **~parity** | ~1.31 s |

Trajectory of the default path: pre-lowering it was ~1.65 s wall / ~8.6 s user
(~2.3× native); after the `map.empty()`/block-scope scanner-lowering fixes it was
~1.03 s / ~2.34 s (~1.35× slower). After **co-lowering the mutually-recursive
scanner cluster** (SCC co-lowering + `bytes.concat` + match-arm block-scoping +
bare-ident match arms; item 3 below) plus memoizing `record_schemas` (item 5), it
is ~0.59 s wall / ~1.23 s user — **now faster than native release tokei** on the
default path. Aggregate counts still differ from native tokei
(language-detection/ignore differences, compared separately); XSH-vs-previous-XSH
JSON parity is preserved (canonicalized).

The `--json` gap closed at the same time: the heavy TS/JS/HTML/Markdown scanners
run on both paths and were the dominant `--json` cost too, so lowering them (with
cheaper per-file record materialization from the `record_schemas` memoization)
brought `--json` from ~3.83× to **roughly at parity** with native — without
touching the report-assembly path itself (the planned parallel report-list reducer
is therefore no longer justified by the benchmark; re-measure before pursuing it,
see item 4 below). The remaining XSH self-time is now value movement (alloc/free +
value-drop/clone + btree/record), not scanning or scope hashing.

### Open performance work

CPU self-time attribution of a serial `showcase/tokei.xsh` scan over the Sentry
corpus (historically via macOS `sample`; now reproduce per-function with
`make prof-callgrind`) reshaped the priorities below. The *pre-fix* compute-only
split was roughly:
allocation/free ~27%, semantic type/schema work ~27%, generic AST eval ~15%,
scope management ~6%, string/byte scan ~7%, and **lowered IR eval ~0%** — the hot
scanner path was *not lowered at all*, so the documented lowered predicate/`for
line` fast paths never fired. Item 1 addressed that; item 3 then co-lowered the
remaining mutually-recursive scanners. The current split (regenerated baseline):
`other` ~22%, value-drop ~12%, alloc/free ~12%, ast-eval ~12%, stream/pipeline
~12%, **lowered-eval ~11%**, str/byte-scan ~7%, btree/record ~6%, value-clone ~4%,
**sema/type-intern ~1%** (was ~27%), scope-mgmt ~0.4% (was ~6%). The sema/SipHash
and scope buckets effectively vanished once the scanners lowered (they used
integer slots instead of `HashMap<String, Binding>`), and `record_schemas` is now
memoized. The dominant remaining cost is value movement (item 6 below). In
priority order:

1. **(Largely done — `map.empty()` lowering.)** The root cause of `lowered-eval`
   ~0% was that **every `count_*` scanner was rejected from lowering by its single
   `blobs: map.empty()` call**: that nil-ary builtin constructor was not lowerable,
   so `LoweredPureFunction::lower` rejected the whole function and the AST↔lowered
   bridge (`call_lowered_pure`, wired from `call.rs`) fell back to
   `eval_call`/`eval_stmt`/`eval_expr` with full `push_scope`/`pop_scope`. Bisection
   confirmed `Bytes` params/returns and Map-field record params all lower fine (the
   IR-coverage tool's `type.param.Bytes`/`type.return.Bytes` reasons are stale);
   only `map.empty()` blocked. Fixed by lowering `map.empty()` to `LoweredExpr::EmptyMap`
   (empty list literals already lower), plus a latent bug: `lowered_value_matches`
   had no `Bytes` arm at all, so any Bytes param/return through a lowered call
   errored "expected Bytes, found Bytes" once scanners started lowering — added
   `Bytes`/`BytesView` arms mirroring `Str`. Release A/B on the parallel Sentry
   scan, byte-for-byte identical totals: **user CPU ~8.6 s → ~2.4 s (~3.5×), wall
   ~1.65 s → ~1.0 s (~1.65×)**; native release tokei is ~0.75 s, so XSH went from
   ~2.3× to ~1.3× slower. Remaining: 6 scanners (`count_json`, `count_html`,
   `count_language`, `count_markdown`, `count_slash_language`, `join_lines`) still
   fall back — see item 2.
2. **(Done — block-scoped re-binding.)** `lower_stmt` rejected any `let`/`var`
   whose name was already in the slot map (`if slots.contains_key(name) { return
   None }`), banning the scanners' sibling-scope re-`let` (early-return `if` body
   declares `let stats`, the function body declares it again). Fixed by scoping
   nested blocks: `lower_block_stmts` snapshots the name→slot map on block entry
   and restores it on exit (so block-local bindings drop and a sibling can re-`let`
   the name), keeping every slot index reserved via a `\0scope.N` placeholder so
   `slots.len()` stays the high-water count. Applied to `if`/`else`/`while` bodies
   and the `for` loop var + body; the function-body statement list stays unscoped
   because its tail expression depends on its prefix bindings. The `let` rejection
   is *kept* on purpose: nested shadowing of a still-in-scope name (verified
   block-scoped at runtime) conservatively falls back to AST rather than risk a
   miscompile, since the scanners only need sibling scopes. `count_json` now
   lowers; `lowered-eval` self-time rose to ~9–10%. (NB: a parallel rebuild
   replaced the pre-fix release binary, so a clean isolated A/B for this lever
   alone wasn't possible; correctness is covered by unit tests + tokei parity.)
3. **(Done — co-lowered the mutually-recursive scanner cluster.)** Five scanners
   ran on the AST evaluator because they form a **mutual-recursion cycle**
   (`count_markdown` ↔ `count_slash_language`, both call `count_html`, and
   `count_language` dispatches to all of them). The single-candidate fixpoint
   (`refresh_lowered_pures`) could not bootstrap a cycle. Fixed with **SCC
   co-lowering** in `colower_pure_sccs` (`src/runtime/eval.rs`): when the
   single-candidate sweep stalls, build the un-lowered pure call graph, Tarjan-SCC
   it, and co-lower each component atomically (all members added to the candidate
   set via `LowerableFunctions::pures_with_candidates`; commit only if every member
   lowers). Three more leaf blockers fell out during bring-up: (a) `bytes.concat`
   (`join_lines`'s tail) now lowers to `LoweredExpr::BytesConcat`; (b) statement-
   and tag-match **arm bodies are now block-scoped** (`lower_block_stmts`, like
   if/else) so sibling arms can re-`let` the same name (`let scan`/`let body` per
   `count_markdown` fence arm) — they previously used un-scoped `lower_stmts` and
   collided; (c) a **bare-identifier match arm** (`_ => empty`, parsed as
   `ArenaStmtKind::TailBareIdent`, not `ArenaStmtKind::Expr`) now lowers via
   `lower_arm_value_expr`/`lower_bare_ident`, which `count_language`'s `_ => empty`
   needs. All five (`count_html`, `count_markdown`, `count_slash_language`,
   `count_language`, `join_lines`) now lower; only `lang_for_name_ext` still falls
   back, on **guard patterns** (`e if e == "ts" or …`), which the lowerer does not
   yet support and which are out of scope here (a cold per-file dispatch helper).
   Release A/B: default path went ~1.03 s → ~0.59 s wall (now faster than native);
   `--json` ~3.07 s → ~0.83 s (parity). Covered by direct lowered tests
   (`bytes_concat_lowers_and_matches_ast`, `mutually_recursive_pures_colower_atomically`,
   `statement_match_arms_relet_sibling_names`, `match_expr_bare_ident_fallback_lowers`)
   and tokei JSON parity.
4. **(Resolved as a side effect of item 3 — re-measure before pursuing.)** The
   `--json` report path was the biggest headline gap (~3.83×). The scanners are
   identical on both paths and were the dominant `--json` cost; lowering them (item
   3) plus the `record_schemas` memoization (item 5) brought `--json` to ~parity
   with native (~0.83 s vs ~0.83 s) **without** touching report assembly. A
   parallel list-merging `reduce-by` reducer (to group per-language report lists in
   parallel, deleting the serial ~18k-file dispatch loop in `--json`) was the
   planned next lever; it is no longer justified by the benchmark. If a future
   change regresses `--json`, attribute a `--json` run specifically (the
   `make prof-callgrind` scenarios profile the *default* serial scan and miss
   report cost) before reaching for it.
5. **(Done — `record_schemas` memoized.)** `sema::records::standard_record_type`
   rebuilt the whole `record_schemas()` `BTreeMap` (every schema `Type`) on each
   call; it is now memoized via a `LazyLock` (`src/sema/records.rs`), so the hot
   per-record-construction lookup is a map read. The rest of the old
   "type/schema/SipHash" bucket was a *consequence* of item 3 — the AST evaluator
   hashes scope names via SipHash+RandomState, work the lowered scanners now avoid
   with integer slots — and dropped from ~27% to ~1% once they lowered.
6. Cut allocation and value movement (still the largest single bucket). The
   Gate-2 axis behind local record-accumulator mutation (see `LANG.md`); track
   volume with the allocation harness (`perf/run.xsh` + `allocation-compare.xsh`).
7. The string/byte scan itself (`memcmp`/memchr) is near-irreducible; do not chase
   it before 3–6.

See **Performance tooling** above for the attribution, allocation, and coverage
harnesses used to drive and verify the items above.

### Case study: tokei showcase

`showcase/tokei.xsh` was the forcing benchmark for most of the above and ruled
out several tempting directions. Durable lessons worth not relitigating:

- Byte-offset rewrites of line scanners (`byte_at`/`find`) regressed against
  idiomatic `for line in text.lines()`; the win was removing per-line helper
  calls, not abandoning `lines()`.
- Specialized standard-library summary APIs (e.g. a native `line_stats`) were
  removed in favor of internal lowered string views for idiomatic loops plus
  view-aware predicates. Do not add benchmark-shaped public text APIs.
- On large corpora, report accumulation can dominate scanner throughput. Avoid
  per-file `List.push` into per-language buckets; collect records once and derive
  reports in batches.
- Summary-only XSH-level scanners preserved totals but regressed badly.
- Worker-count tuning moved wall time only at noise level; scanner-loop runtime
  cost and intermediate `Value` movement are the higher-value targets.
- The biggest single win was getting the heavy mutually-recursive scanners
  (TS/JS, HTML, Markdown) onto the lowered path at all. The cluster is one SCC
  (`count_markdown` ↔ `count_slash_language`, both into `count_html`, dispatched by
  `count_language`), so it had to be co-lowered atomically — the one-candidate-at-a-time
  fixpoint cannot bootstrap a cycle (a lowered call to a not-yet-lowered callee
  errors rather than falling back). It also closed the `--json` gap as a side
  effect, because the same scanners are the bulk of report-record cost — i.e. lower
  the shared hot code before reworking a path-specific serial loop.
- When a whole function refuses to lower, the blocker is usually one leaf
  construct, not the obvious one. Bisect empirically (env-gated dump of
  lowered/rejected pures); here the scanner cluster was gated behind, in order,
  `bytes.concat`, un-block-scoped match arms colliding on sibling `let`s, and a
  bare-identifier match arm (`ArenaStmtKind::TailBareIdent`) — none of which is the
  recursion the cluster appears to be about.
- The default table reproduces tokei's output byte-for-byte, including the embedded
  ("`|- Child`") breakdown and per-language `(Total)` rows, the heavy/light rules
  (via `tui` glyphs), the fixed column right-edges (28/41/54/67/80, 80-wide rows /
  81-wide rules), and tokei's row order (sorted by tokei's *internal* `LanguageType`
  variant name — e.g. Plain Text sorts as `Text` — with child-bearing languages
  grouped last). The per-`(parent, child)` breakdown is aggregated in the stream by
  expanding each file into one parent record plus one child record per embedded
  language (`par-map |> flat-map |> reduce-by --sum`, children keyed `parent\tchild`);
  `|- Child` rows use the child's *deep* total (recursively including its own nested
  blobs, e.g. a TOML fence inside a Rust doc-comment Markdown blob). The `flat-map`
  breaks the `par-map |> reduce-by` fusion and the child expansion adds work (~9% on
  the default path), but the table still beats native tokei. Byte-identity is
  verified on controlled fixtures where language detection agrees; on real corpora
  the *counts* still differ from tokei (language-detection / ignore semantics — a
  separate axis from output format).
- The corpus count gap splits into two axes. **File selection** was closed to exact
  (Δfiles = 0 per language over the Sentry tree) cheaply: `.pyi`/`.pot` extensions,
  and `#!`-shebang detection for extension-less scripts (`lang_for_shebang`; the
  pipelines keep extension-less files past the language filter so par-map can read
  the first line, then drop the still-unknown ones). **Line classification** is the
  other axis: matching tokei's per-line code/comment/blank split byte-for-byte needs
  each language's own string/comment tokenizer (Python `"""`/`'''`, JS/TS backtick
  template literals, HTML `<style>`→CSS embedding, blank-inside-block-comment, …).
  Those were prototyped and reached ~0.12% of total lines, but they require
  char-level scanning of every string/comment-bearing line — and an interpreter
  (even lowered) doing per-character work over ~5M lines runs ~1.65× *slower* than
  native tokei, forfeiting the speed lead. Decision: **keep the cheap file-selection
  parity, revert the char-scanning tokenizers**, leaving the shared approximate
  counters (`count_hash_language`, `count_slash_plain`, single-pass `count_html`)
  for line classification. So the showcase matches tokei on file selection and output
  format, and stays faster than native, while line-classification counts remain a
  deliberate ~0.18% approximation rather than a byte-for-byte tokenizer port.

The `par-map |> flat-map |> reduce-by` default-table path and the borrowed
`for line in text.lines()` representation are in place, and `Bytes` is a
first-class zero-allocation byte-scanning surface.

## Deferred VM Considerations

A bytecode VM is not justified by the current architecture. Reconsider only if
the lowered IR becomes broad enough that a compact instruction set is obvious,
benchmark results show AST dispatch remains the bottleneck after lowered-region
coverage is high, and the VM can preserve exact tracing, process boundaries,
cwd/env mutation, defers, signal hooks, stream behavior, and fallback
independence. If that happens, this guide becomes the boundary spec for bytecode
work rather than disposable scaffolding.

## Benchmark Loop

Use hyperfine for IR changes because it measures the real CLI path:

```sh
hyperfine --warmup 3 'target/release/xsh perf/interpreter/path-ir-glue-5k.xsh >/dev/null'
```

For each meaningful IR expansion:

1. Run `tools/xsh-ir-coverage.xsh` and pick a frequent real fallback.
2. Add the smallest IR representation that preserves normal semantics.
3. Add direct lowered tests in `src/runtime/eval/tests.rs`.
4. Add or update a mixed glue scenario in `perf/interpreter/*.xsh`.
5. Run focused hyperfine for that scenario.
6. Run the full interpreter hyperfine set from `perf/interpreter/README.md`
   when the expansion is broad enough to affect unrelated scenarios.
7. Record clearly labeled focused or full-suite measurements in
   `perf/interpreter-hyperfine-baseline.json`.
8. Update this guide when pass shape, gates, structures, or covered surfaces
   change.

The useful priority order is corpus frequency, semantic safety, benchmark
coverage, then implementation size. Easy-to-lower constructs are not worth
adding unless they show up in real code or make a benchmark more representative.
