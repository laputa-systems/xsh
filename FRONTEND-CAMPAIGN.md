# Frontend and IR Design Campaign

## Campaign Status

- [x] Phase 0: establish evidence and freeze the comparison protocol
- [x] Phase 1: prove the compact indexed IR architecture with a hard vertical slice
- [x] Phase 2: replace permissive probing with transactional lowering
- [x] Phase 3: introduce interned semantic types, signatures, and shapes
- [x] Phase 4: migrate expressions, statements, patterns, functions, and slots
- [x] Phase 5: decide and implement the durable top-level/effect boundary
- [ ] Phase 6: cut production execution over and delete the recursive lowered IR
- [ ] Phase 7: redesign records and runtime value movement
- [ ] Phase 8: make dynamic interning reclaimable
- [ ] Phase 9: replace native-stack recursion with explicit execution frames
- [ ] Phase 10: compact remaining arena, span, CST, and builder storage
- [ ] Phase 11: remove migration machinery and publish the final evidence

Coding agents must update these checkboxes and the phase-local checkboxes as
work lands. A phase is complete only when every exit-gate checkbox is checked
and its evidence is recorded in the decision log. Do not mark a task complete
because code exists; mark it complete when the applicable behavior,
representation, allocation, latency, coverage, and documentation gates pass.

## Purpose

This campaign makes XSH's frontend, semantic representation, lowered IR, and
interpreter materially tighter without changing the language contract. It
follows the storage discipline of Zig's frontend closely while preserving the
parts of XSH that are different by design: a dynamic runtime, first-class Unix
process orchestration, structured streams, runtime tracing, and tooling that
needs a lossless CST.

The campaign is not a request to remove intermediate representations. Multiple
representations are desirable when each one has:

- [ ] one precise responsibility;
- [ ] a compact, asserted storage contract;
- [ ] an explicit owner and drop point;
- [ ] typed identities rather than object references;
- [ ] a verifier before downstream consumers trust it;
- [ ] a measured reason to exist.

The intended pipeline is:

```text
source
  -> token table and lossless CST
  -> compact arena AST
  -> interned semantic facts
  -> compact indexed executable IR
  -> explicit interpreter frames and runtime values
```

Each arrow must erase work or ownership from the next stage. A later
representation must not retain an earlier representation merely for
convenience except where diagnostics, formatting, tracing, or an explicitly
measured fallback boundary requires it.

This document owns the migration plan. `docs/FRONTEND.md` describes the
currently implemented frontend and lowered runtime. `docs/BENCHMARKING.md` owns
benchmark mechanics. `docs/TEST-MAP.md` owns behavior gates. If implementation
and this campaign disagree, record the decision and evidence here rather than
silently drifting from the plan.

`IR.md` records the architectural assessment that motivates this campaign.
This campaign operationalizes its recommended direction and deliberately
refines the order: the fixed-width store, verifier, transactional failure
model, and execution boundary are proved before localized representation work.

## Ordering Principle

The campaign deliberately puts the hardest and most structurally disruptive
work first.

The order is:

1. measure the current system;
2. prove the final instruction/storage model on difficult real semantics;
3. prove transactional lowering, recursion, failure, and verification;
4. establish semantic identities used by that format;
5. migrate broad syntax coverage;
6. decide the top-level and effect boundary;
7. cut over and delete the old representation;
8. optimize runtime values and remaining storage using the now-stable model;
9. perform localized compaction and cleanup last.

Do not begin with easy enum boxing, span shaving, builder header reduction,
isolated opcode specialization, or source-code cleanup. Those changes are
likely to be invalidated by an indexed IR. A small prerequisite may land early
only when it directly enables a foundational phase and has its own evidence.

### Phase Dependencies

| Phase | Requires | Why it cannot move earlier |
|---|---|---|
| 0 | none | later decisions need reconciled measurements |
| 1 | 0 | the final storage model must be proved before broad migration |
| 2 | 1 | transactional lowering needs the real builder/checkpoint model |
| 3 | 1-2 | semantic IDs must target the accepted IR and failure model |
| 4 | 1-3 | broad migration needs stable storage, construction, and identities |
| 5 | 1-4 | effect/fallback boundaries must be measured on representative coverage |
| 6 | 1-5 | production cutover requires complete semantics and a selected boundary |
| 7 | 6 | value movement must be measured on the final executable path |
| 8 | 3 and 6 | owned names depend on stable semantic/program ownership |
| 9 | 6-7 | explicit frames target the final instructions and values |
| 10 | 6 and 8 | localized frontend compaction must not optimize discarded structures |
| 11 | all prior phases | cleanup and final evidence are the completion gate |

Agents must not mark a later phase complete while an unmet prerequisite remains.
Parallel work is allowed only for tasks whose prerequisite rows are complete and
whose representation boundaries do not overlap.

## Outcome

The campaign is complete when:

- [ ] the compact AST remains the single syntax representation used by
  checking, tooling, and lowering;
- [ ] semantic types, callable signatures, and record/module shapes are compact
  interned IDs at persistent use sites;
- [ ] executable IR instructions are dense tag/data columns with variable
  payloads in shared extra storage;
- [ ] the completed executable program is self-contained for execution and does
  not read the compact AST, token table, or CST;
- [ ] persistent executable IR rows own no recursive child nodes, vectors,
  boxes, maps, strings, semantic `Type` values, full `Span` values, or
  machine-width indexes;
- [ ] lowerability analysis cannot manufacture valid-looking placeholder
  instructions;
- [ ] functions, parameters, captures, blocks, matches, and top-level regions
  are ranges into shared tables rather than independently allocated graphs;
- [ ] the old `LoweredExpr`, `LoweredStmt`, `LoweredPattern`, and wide
  `LoweredPureFunction` representations are deleted;
- [ ] runtime records use interned shapes and dense field storage;
- [ ] dynamic symbols have an explicit reclaimable owner;
- [ ] XSH call/control flow no longer relies on a 64 MiB native thread stack;
- [ ] every retained representation reports bytes, item counts, capacity, and
  bytes per source byte;
- [ ] the curated user-facing benchmark suite and PGO workload exercise the
  final representation without a benchmark-only path;
- [ ] arena-versus-IR value, output, error, source-span, and trace parity is
  covered by tests;
- [ ] the arena evaluator remains a test/migration oracle rather than a
  permanent equal production interpreter;
- [ ] current architecture documentation describes the final design without
  migration terminology.

## Non-Goals

This campaign does not:

- change XSH syntax or semantics to make implementation easier;
- remove the lossless CST required by formatting and source-preserving
  tooling;
- collapse all compiler stages into one representation;
- require serialized bytecode, a JIT, native code generation, a garbage
  collector, or a general-purpose optimizing compiler;
- intern arbitrary file contents, subprocess output, or user data;
- add benchmark-shaped standard-library primitives;
- preserve a fast path that does not improve a user-visible workflow;
- optimize solely for `size_of` while increasing allocations or latency;
- copy Zig mechanisms that solve compilation problems XSH does not have;
- use debug timing as acceptance evidence;
- use the `dist` profile for development or measurement;
- add dependencies when a small local typed representation is sufficient.

## Current Baseline

Refresh the structural and performance baseline after each completed phase:

```sh
scripts/ir-layout.py
make bench-fast
tools/xsh-ir-coverage.xsh
```

As of 2026-07-24 on a 64-bit target:

| Type | Bytes | Primary concern |
|---|---:|---|
| `ArenaProgramBuilder<'_>` | 2,384 | many staging vectors and maps |
| `ArenaProgram` | 1,584 | dominated by the empty arena header |
| `AstArena` | 1,552 | roughly sixty independently owned columns/tables |
| semantic `Type` | 32 | recursive boxes and `BTreeMap` payloads |
| `LoweredPureFunction` | 552 | five inline parameter arrays plus captures |
| `LoweredTopLevelStmt` | 240 | inline slot metadata and wide kind |
| `LoweredStmt` | 144 | largest recursive enum payload |
| `LoweredExpr` | 72 | recursive boxes, vectors, maps, strings, and spans |
| `LoweredPattern` | 56 | semantic types and inline slot metadata |
| `LoweredValue` | 32 | dynamic value movement |
| public runtime `Value` | 48 | composite ownership and cloning |
| `Evaluator` | 2,744 | unrelated runtime subsystems in one state object |
| construct probe | 1,816 | analysis, construction, and reporting combined |
| construct probe output | 1,672 | many inline counters and maps |

These are layout observations, not proof of memory impact. Heap traffic, peak
live bytes, node volume, and user-visible latency decide whether a
representation change is useful.

The checked-in `.xsh` and `.xsht` corpus was approximately 743 KiB across 337
files when this campaign was written. Recount it rather than treating these as
permanent figures. Bytes per source byte, bytes per instruction, and temporary
versus final retained bytes are first-class campaign metrics.

## Zig Reference Model

The reference checkout used to write this campaign was:

```text
~/d/zig
commit 783ad796565947e07430d0c73ab279020e5d9e74
```

Re-read the current versions before implementing a phase. Copy the discipline,
not incidental details from one revision.

### Required Zig Sources

| Source | What to study |
|---|---|
| `lib/std/zig/Ast.zig` | externally owned source, token/node `MultiArrayList` columns, `u32` indexes, optional-index sentinels, eight-byte node data |
| `lib/std/zig/AstGen.zig` | scratch allocation, scoped construction, extra-data reservation, body fixups, cleanup |
| `lib/std/zig/Zir.zig` | immutable file-level IR, one-byte tags, eight-byte data, string byte blob, shared `u32` extra data, self-contained lifetime |
| `src/Sema.zig` | separation between source IR and semantic analysis, temporary analysis state, interned facts |
| `src/InternPool.zig` | compact identities, canonical types/values, owned storage, explicit index types |
| `src/Air.zig` | per-function analyzed IR, fixed-width instructions, shared extra data, explicit deinitialization |
| `src/Air/Verify.zig` | structural verification before consumers trust indexed data |
| `src/Air/Liveness.zig` | analysis over instruction IDs and blocks rather than recursive ownership |

### Zig Principles To Adopt

- [ ] persistent compiler identities are typed `u32` indexes;
- [ ] hot tag/data columns are separate so alignment does not pad each row;
- [ ] uncommon and variable payloads live in shared `u32` extra storage;
- [ ] strings are stored once and referenced by offset or interned identity;
- [ ] representation sizes are asserted in code;
- [ ] each IR is self-contained for its consumers;
- [ ] scratch and final representations have explicit owners and drop points;
- [ ] completed IR is immutable and verified;
- [ ] source locations are compact and rich diagnostics are recovered lazily;
- [ ] optional IDs use reserved values rather than wide `Option<usize>`;
- [ ] instruction specialization does not widen common instruction storage;
- [ ] function bodies and variable payloads are ranges, not nested vectors;
- [ ] interning is an ownership and canonical-identity design;
- [ ] stage boundaries are visible in types and APIs.

### Zig Practices Not To Copy Blindly

- XSH does not need all of ZIR, AIR, LIR, machine IR, and incremental compiler
  state.
- Dynamic runtime values cannot all become compile-time intern-pool indexes.
- Runtime tracing and errors need source fidelity after checking.
- Formatting requires trivia that an execution-only frontend could discard.
- Process, stream, cwd, environment, signal, and module effects remain explicit
  runtime operations.

## XSH Reference Sources

| Concern | Documentation | Owner code |
|---|---|---|
| architectural assessment | `IR.md` | all frontend, sema, IR, and runtime owners below |
| language behavior | `docs/SPEC.md`, `docs/SPEC-TYPING.md`, `docs/SPEC-OS.md` | applicable syntax, sema, and runtime modules |
| current frontend | `docs/FRONTEND.md` | `src/syntax/arena.rs`, `src/syntax/parser/*` |
| measurement | `docs/BENCHMARKING.md`, `docs/TEST-MAP.md` | `crates/xsh-multicall/benches/bench.rs`, benchmark scripts |
| source and spans | `docs/ARCHITECTURE.md` | `src/source.rs`, `src/syntax/arena.rs` |
| semantic types | `docs/SPEC-TYPING.md` | `src/sema/types.rs`, `src/sema/check/*` |
| lowering | `docs/FRONTEND.md` | `src/runtime/eval/lower.rs` |
| lowered execution | `docs/FRONTEND.md` | `src/runtime/eval/lowered_run.rs`, `lowered_ops.rs` |
| normal execution | `docs/ARCHITECTURE.md` | `src/runtime/eval.rs`, `src/runtime/eval/*` |
| runtime values | `docs/SPEC.md` | `src/runtime/value.rs`, `src/runtime/eval.rs` |
| API identities | `docs/STDLIB.md` | `crates/xsh-registry`, `src/modules/signature.rs` |
| IR coverage | `docs/FRONTEND.md` | `tools/xsh-ir-coverage.xsh` |

## Architectural Invariants

These are campaign rules, not optional cleanup ideas.

### Identity And Width

- [ ] Persistent syntax, semantic, instruction, slot, block, function, literal,
  string, and shape identities are `u32` newtypes.
- [ ] `usize` exists only while indexing a Rust slice at an API boundary.
- [ ] Optional IDs use a reserved `u32` value and remain four bytes.
- [ ] Every `usize`-to-ID conversion is checked.
- [ ] Overflow is a deliberate construction error and never truncates.

### Instruction Storage

- [ ] Hot instruction tags use `#[repr(u8)]`.
- [ ] Common instruction data is exactly two `u32` words unless corpus evidence
  proves another fixed size wins broadly.
- [ ] Tags and data live in parallel vectors.
- [ ] The tag completely defines the meaning of its data and extra payload.
- [ ] Variable payloads use compact ranges or extra-payload indexes.
- [ ] Rare instructions may use more extra words but cannot widen every row.
- [ ] Completed instruction storage is immutable during execution.

### Persistent Ownership

The following are forbidden inside a hot persistent instruction row:

- `Vec`, `SmallVec`, `Box`, `Arc`, `String`, and `PathBuf`;
- `HashMap`, `FxHashMap`, and `BTreeMap`;
- semantic `Type`;
- runtime `Value`;
- full `Span`;
- `usize` and `Option<usize>`;
- recursively owned instructions or patterns.

These may exist in construction scratch state or separately owned pools when
the pool is measured and has the correct lifetime.

### Source Locations

- [ ] Source identity is stored once per program/source partition when possible.
- [ ] Hot instructions do not carry a full twelve-byte `Span`.
- [ ] Default locations are compact token/node offsets or `SpanId`s.
- [ ] Every instruction that can raise a user-visible error resolves a location.
- [ ] Cross-source and cooked-text spans use sparse side storage.
- [ ] Diagnostic quality does not regress to save bytes.

### Strings And Interning

- [ ] Identifiers, fields, tags, errors, modules, and callables use interned IDs.
- [ ] Runtime file content, process output, environment data, and arbitrary
  strings are not globally interned.
- [ ] Literal strings may use a per-program blob when measurements justify it.
- [ ] Dynamic interning has an explicit owner and drop point.
- [ ] No API demands `&'static str` only to avoid defining ownership.

### Semantic Facts

- [ ] Persistent IR refers to `TypeId`, `SignatureId`, and `ShapeId`.
- [ ] Structurally equal stable types/shapes have one identity per owner.
- [ ] Recovery types cannot become executable facts.
- [ ] Semantic scratch is discarded after required facts are committed.

### Construction And Failure

- [ ] Construction returns a valid ID or a structured blocker.
- [ ] Unsupported source never becomes placeholder `Unit` or another valid
  opcode.
- [ ] Failed construction rewinds scratch or drops an uncommitted builder.
- [ ] Blocker reporting observes failure without altering executable output.
- [ ] Dependency discovery is separate from emission.
- [ ] SCC/fixpoint handling operates on compact function IDs and a graph.

### Execution

- [ ] An executable store is verified before installation.
- [ ] The installed executable program contains every semantic pool, literal,
  location, function, and operation identity needed by execution.
- [ ] Indexed execution never consults `AstArena`, the token table, or the CST.
- [ ] Slot bounds are established by verification.
- [ ] Function metadata names exact parameter, capture, body, and slot ranges.
- [ ] Trace and error behavior matches the arena evaluator.
- [ ] Specialized instructions reduce measured dispatch/allocation or simplify
  the representation.

## Target Representation

Names are descriptive. Changing the shape is allowed only with recorded
evidence.

### Core IDs

```rust
#[repr(transparent)]
struct IrInstId(u32);

#[repr(transparent)]
struct IrBlockId(u32);

#[repr(transparent)]
struct IrFunctionId(u32);

#[repr(transparent)]
struct IrSlotId(u32);

#[repr(transparent)]
struct TypeId(u32);

#[repr(transparent)]
struct SignatureId(u32);

#[repr(transparent)]
struct ShapeId(u32);

#[repr(transparent)]
struct IrStringId(u32);
```

Every ID must have:

- [ ] a checked constructor from an index;
- [ ] an explicit slice-index conversion;
- [ ] a reserved optional sentinel where needed;
- [ ] no arithmetic outside the owning builder/accessor module;
- [ ] `Debug` output that identifies the ID kind.

### Instruction Columns

```rust
#[repr(u8)]
enum IrTag {
    // Semantic opcode vocabulary.
}

#[repr(C)]
struct IrData {
    lhs: u32,
    rhs: u32,
}

struct IrStore {
    tags: Vec<IrTag>,
    data: Vec<IrData>,
    extra: Vec<u32>,
    span_ids: CompactSpanColumn,
    blocks: Vec<IrBlock>,
    functions: Vec<IrFunction>,
    params: Vec<IrParam>,
    captures: Vec<IrCapture>,
    literals: IrLiteralPool,
}

struct ExecutableProgram {
    ir: IrStore,
    types: TypePool,
    signatures: SignaturePool,
    shapes: ShapePool,
    strings: IrStringPool,
    // SourceMap and compact locations are retained only for diagnostics,
    // tracebacks, and tracing; execution does not recover semantics from AST.
    sources: Arc<SourceMap>,
}
```

Initial budgets on 64-bit targets:

| Representation | Budget |
|---|---:|
| `IrTag` | 1 byte |
| `IrData` | 8 bytes |
| persistent/optional ID | 4 bytes |
| `IrBlock` | at most 24 bytes |
| `IrFunction` | at most 40 bytes |
| `IrParam` | at most 16 bytes |
| `IrCapture` | at most 12 bytes |
| common instruction including amortized location | at most 16 retained bytes |
| corpus-weighted instruction including extra data | initially at most 24 retained bytes |

The amortized budgets include every backing allocation. They cannot be met by
moving uncounted payloads behind pointers.

### Blocks And Bodies

```rust
#[repr(C)]
struct IrRange {
    start: u32,
    len: u32,
}

#[repr(C)]
struct IrBlock {
    instructions: IrRange,
    result: OptionalIrInstId,
    flags: IrBlockFlags,
}
```

- [ ] Function bodies, branches, loops, matches, defers, and stream-stage bodies
  refer to block IDs.
- [ ] Blocks never own vectors.
- [ ] Structured control flow remains structured until explicit-frame evidence
  favors flatter branches/jumps.

### Function And Parameter Tables

```rust
#[repr(C)]
struct IrFunction {
    key: FunctionKeyId,
    body: IrBlockId,
    params: IrRange,
    captures: IrRange,
    slot_count: u32,
    return_type: TypeId,
    flags: IrFunctionFlags,
}

#[repr(C)]
struct IrParam {
    name: Name,
    type_id: TypeId,
    default_value: OptionalIrValueId,
    flags: IrParamFlags,
}
```

Rare validation/default metadata belongs in optional side tables. The common
parameter does not pay inline for rare metadata. Top-level slots use the same
table discipline rather than another wide structure.

### Extra Data

`Vec<u32>` is the default variable-payload store. Candidate word schemas:

```text
call:            [callee, arg_count, arg_0, ..., arg_n]
match:           [arm_count, pattern_0, guard_0, block_0, ...]
record literal:  [shape_id, value_0, ..., value_n]
run:             [target, argv_range, env_range, redirect_range, flags]
module call:     [runtime_op, arg_count, arg_0, ..., arg_n]
```

- [ ] Every opcode has a typed payload structure or documented word schema.
- [ ] Append/decode logic is centralized.
- [ ] Consumers do not perform ad hoc word arithmetic.
- [ ] Every range and referenced ID is verified.
- [ ] Flags are packed only when decoding remains clear and tested.

### Match Representation

- [ ] General patterns use a flat source-ordered arm range.
- [ ] Guard-free exact matches may use compact sorted or small linear tables.
- [ ] Interned tag/error/string identities are keys.
- [ ] No match node owns an `FxHashMap`.
- [ ] Linear, binary, or compiled hash dispatch is selected from corpus arm
  counts and focused measurements.
- [ ] Ordering, binding, guard, error, and fallback behavior is identical.

### Type Pool

```rust
struct TypePool {
    tags: Vec<TypeTag>,
    data: Vec<TypeData>,
    extra: Vec<u32>,
}

struct TypePoolBuilder {
    pool: TypePool,
    canonical: FxHashMap<TypeKey, TypeId>,
}
```

Canonical categories include primitives, list/map/stream/optional/result,
records/modules, error identities, and callable signatures. The canonical map
belongs to construction/session state rather than the immutable executable
pool. Measurements decide whether the builder is dropped after checking or
retained beside the executable program for session/module reuse; it is always
accounted separately.

Do not start by interning every temporary checker value. Inventory allocation,
clone, equality, and hashing first. Migrate stable persistent facts, then
measure checker-local facts.

### String And Shape Pools

Use identities with deliberately different lifetimes:

- `Name` for language identifiers and structural names;
- `IrStringId` for program-owned literals/diagnostic strings;
- ordinary runtime strings for dynamic user data;
- `ShapeId` for ordered record/module fields;
- `SignatureId` for callable metadata.

`ShapeId` denotes one canonical ordered structural field identity. Semantic
record/module types and runtime records should share it when their field
sequence is the same. If dynamic records require a different owner or lifetime,
define a distinctly named runtime shape ID rather than silently maintaining two
unrelated pools both called `ShapeId`.

Candidate record shape:

```rust
struct ShapePool {
    shapes: Vec<IrRange>,
    fields: Vec<Name>,
}

struct ShapePoolBuilder {
    pool: ShapePool,
    canonical: FxHashMap<Box<[Name]>, ShapeId>,
}

struct RuntimeRecord {
    shape: ShapeId,
    values: Vec<Value>,
}
```

The canonicalization key must not remain duplicated after finalization unless
session reuse measurably repays it.

### Runtime Values

Runtime `Value` redesign is deliberately after the IR foundation and cutover.

- [ ] Preserve inline scalar values.
- [ ] Keep cold errors/process plans indirect.
- [ ] Represent records/modules by shape plus dense values.
- [ ] Avoid general tree maps for fixed-shape records.
- [ ] Use interned structural names.
- [ ] Enable unique-owner mutation where language aliasing permits it.
- [ ] Treat a 16-24 byte `Value` as a target, not permission to box common
  scalars.

Do not choose tagged pointers, NaN boxing, handles, reference counting, or an
arena without measuring size, allocation, clone/drop, mutation, thread
transfer, and FFI/process boundaries.

### Evaluator State And Frames

Target ownership:

```text
ProgramState
  source map
  semantic pools
  executable IR
  function/module registries

RuntimeState
  cwd/env
  process and signal state
  module caches
  stdout/stderr and stream values

ExecutionState
  explicit call frames
  explicit control frames
  slots
  trace stack
  pending error/return flow
```

The objective is not a cosmetic `Evaluator` size reduction. It is clear
ownership, smaller call frames, cheaper worker setup, and deletion of the
64 MiB stack workaround.

## Lifetime Model

Every implementation proposal must update the affected row:

| Representation | Owner | Created | Last required | Drop point |
|---|---|---|---|---|
| source bytes | `SourceMap`/loader | load | diagnostics/tracing complete | checked program/session drop |
| token table | parse output | lex | CST/AST complete; later only for tooling | after parse on runtime-only paths |
| lossless CST | tooling parse output | parse | formatting/source edit complete | before sema/runtime where possible |
| compact AST | checked program | parse | lowering and required fallback complete | after executable IR when fallback ends |
| semantic scratch | checker | check | pools/facts committed | immediately after checking |
| type/signature/shape pools | checked/executable program | check/lower | execution/diagnostics complete | program/session drop |
| lowering scratch | IR builder | lower | verify/commit complete | immediately after success/failure |
| executable IR | executable program | lower | execution complete | program/session drop |
| runtime values | runtime state/frames | execute | language ownership ends | deterministic drop |
| trace graph | trace state | execute | rendering/inspection complete | trace/session drop |

Runtime-only entry points should eventually release tokens, CST, construction
scratch, semantic scratch, and arena data not required by the selected fallback
boundary before execution.

## Measurement Model

### User-Facing Decision Workloads

The existing Divan suite remains the only performance and PGO workload.

| Concern | Primary workload | Secondary signals |
|---|---|---|
| lex/token/CST | `xsht_format_check_xsh_repository` | token/CST retained bytes |
| arena construction | `xsht_check_xsh_repository` | arena bytes/source byte |
| semantic pools | `xsht_check_xsh_repository` | entries, hits, allocation traffic |
| lint traversal | `xsht_lint_xsh_repository` | AST bytes and traversal latency |
| top-level IR | `xsh_short_script` | coverage and instruction count |
| process IR | `xsh_process_pipeline` | syscalls and trace parity |
| loops/records/strings | `xsh_extension_count_1000_files` | value/record allocations |
| maps/records/parsing | `xsh_json_log_rollup_10000_rows` | clone/drop and shape reuse |
| files/hash/JSON | `xsh_manifest_hash_1000_files` | retained report graph |
| unrelated regressions | every `xshi_*` workload | complete `make bench-fast` |

Do not add a microbenchmark to justify a representation. Add a deterministic
complete workflow only when the suite genuinely lacks a user-visible path.

### Required Structural Counters

Extend current `ArenaStats`-style reporting with:

```text
source_bytes
token_count
token_retained_bytes
cst_node_count
cst_retained_bytes
ast_stmt_count
ast_expr_count
ast_pattern_count
ast_type_count
ast_extra_words
ast_retained_bytes
ast_bytes_per_source_byte
type_count
signature_count
shape_count
semantic_extra_words
semantic_retained_bytes
lowered_function_count
lowered_block_count
lowered_instruction_count
lowered_extra_words
lowered_span_entries
lowered_literal_bytes
lowered_retained_bytes
lowered_bytes_per_instruction
lowered_bytes_per_source_byte
dynamic_symbol_count
dynamic_symbol_bytes
runtime_shape_hits
runtime_shape_misses
```

Accounting rules:

- [ ] report length-based payload and retained capacity;
- [ ] include owner headers, backing strings, side tables, and retained
  canonical maps;
- [ ] distinguish temporary peak construction from final retained bytes;
- [ ] never claim savings by moving bytes into an uncounted pool/thread;
- [ ] aggregate the real corpus while retaining per-file maxima;
- [ ] make output deterministic and diffable;
- [ ] keep detailed reports outside ordinary benchmark output.

### Layout Gates

Run:

```sh
scripts/ir-layout.py
```

Add every new hot persistent type to the default report. Assert hard contracts:

```rust
assert_eq!(size_of::<IrTag>(), 1);
assert_eq!(size_of::<IrData>(), 8);
assert_eq!(size_of::<IrInstId>(), 4);
assert_eq!(size_of::<OptionalIrInstId>(), 4);
```

Use exact assertions for format invariants and threshold tests only for
temporary migration structures.

### Allocation And Peak-Live Gates

Divan provides total allocated bytes, allocation count, and `max alloc`.

- [ ] Deterministic allocation increases are explained.
- [ ] A repeated increase over 1% blocks a phase unless it removes larger
  retained state or buys a clearly demonstrated latency improvement.
- [ ] A peak-live increase over 1% or 64 KiB, whichever is larger, blocks a
  phase unless it is a temporary migration cost absent from production.
- [ ] Work moved to threads is validated with process RSS.
- [ ] Allocation reduction is tied to an affected user-visible workflow.

### Latency Gates

This campaign uses `make bench-fast` as the default suite command: zero outer
warmup, one measured suite, Divan `--sample-count 1 --sample-size 1`, separate
`-fast` baseline, and a memory-only report (no per-benchmark time or run
spread). Allocation count, allocated bytes, and `max alloc` are the decision
signals. Whole-suite wall time is recorded only as iteration-cost telemetry.

Focused diagnosis may filter one operation:

```sh
cargo bench -p xsh-multicall --bench bench BENCHMARK -- \
  --sample-count 1 --sample-size 1
```

`make bench` (multi-sample latency baselines) remains available outside the
campaign when a timing claim needs multi-run evidence. Do not use it as the
ordinary phase gate.

- [ ] Benchmark processes run serially.
- [ ] Host, toolchain, allocator, profile, and command are recorded.
- [ ] Before/after use the same workload and fixture.
- [ ] The direction of a timing change repeats.
- [ ] A single sub-5% delta on a sub-millisecond workload is inconclusive.
- [ ] A 2-5% suite regression is measured again.
- [ ] A reproduced regression of 5% or more blocks the change.
- [ ] A memory-layout phase may land with flat latency only when it banks a
  substantial measured memory reduction.
- [ ] An execution specialization must improve user-visible latency or remove
  substantial complexity.

### Correctness Gates

Every phase uses `docs/TEST-MAP.md`. Executable IR additionally requires:

- [ ] exact value, status, stdout, stderr, and error parity;
- [ ] exact trace-event and source-span parity where applicable;
- [ ] force-arena versus force-IR tests;
- [ ] tests that execute a completed `ExecutableProgram` after its AST, tokens,
  CST, and lowering scratch have been dropped;
- [ ] tests for every opcode and flag combination;
- [ ] invalid ID/range/tag verifier tests;
- [ ] applicable import, recursion, mutual recursion, parameter, capture, defer,
  propagation, signal, process, stream, and top-level tests;
- [ ] corpus execution without silent in-region fallback;
- [ ] formatter/checker gates when their storage changes.

Never weaken an assertion or fixture to accommodate a representation.

### Coverage Gates

Run `tools/xsh-ir-coverage.xsh` before and after lowering phases.

- [ ] Function and top-level-region coverage does not silently decrease.
- [ ] Pure representation changes preserve the supported semantic set.
- [ ] Unsupported constructs have explicit blocker identities/sample spans.
- [ ] Coverage reporting builds no placeholder executable nodes.
- [ ] Real-corpus frequency chooses the next missing construct.

### PGO Gates

Do not run PGO during representation or cutover iteration. Its full
instrumented rebuild makes iteration slow, and its signal is low while ordinary
latency, allocation, behavior, or coverage gates are still moving or failing.

Run PGO only after the non-PGO gates pass and the implementation is a credible
release candidate whose instruction mix, dispatch, function layout, and runtime
values are stable:

```sh
make pgo-profile
make bench-pgo
```

The curated suite remains the entire PGO workload. There is no campaign-only
filter, and PGO is not a per-phase completion gate.

## Experiment Protocol

Every performance-affecting change follows this checklist.

### Before Coding

- [ ] State one falsifiable hypothesis.
- [ ] Name the complete user-facing workload containing the cost.
- [ ] Record XSH state, Rust toolchain/target, host, allocator, and profile.
- [ ] Preserve before measurements rather than overwriting them.
- [ ] Record latency/spread, allocations, bytes, peak live, layout, retained
  representation stats, coverage.

Example hypothesis:

```text
Replacing per-function parameter SmallVecs with one shared ParamRow table will
reduce retained function metadata by at least 40% on the repository corpus
without increasing xsh_* latency or allocation traffic.
```

### During Coding

- [ ] Change one representation boundary.
- [ ] Do not combine type interning with runtime `Value` redesign.
- [ ] Do not combine indexed IR with language semantics.
- [ ] Do not combine specialization with a new benchmark.
- [ ] Do not combine explicit frames with unrelated module optimization.
- [ ] Keep mechanical prerequisites separately verifiable.

### After Coding

- [ ] Run narrow behavior/verifier tests first.
- [ ] Capture after measurements with identical commands.
- [ ] Repeat close timing results.
- [ ] Check whether allocations moved outside accounting.
- [ ] Check whether setup moved outside the measured closure.
- [ ] Check whether coverage/corpus/cache/interner warmth changed.
- [ ] Check whether a rare payload made the common path indirect.
- [ ] Check regular and PGO builds separately when applicable.
- [ ] Keep, redesign, or revert based on the stated hypothesis.
- [ ] Record evidence in the decision log.

Evidence template:

```text
Hypothesis:
Affected workload:
Before commit/state:
After commit/state:
Host/toolchain/allocator:
Focused command:
Latency before/after and spread:
Allocated bytes before/after:
Allocations before/after:
Peak live before/after:
Layout before/after:
Retained bytes/source byte before/after:
Coverage before/after:
Correctness gates:
Decision:
Known follow-up:
```

## Migration Rules

- [ ] The arena evaluator is the semantic oracle during migration.
- [ ] Current lowered execution remains production until replacement parity.
- [ ] New IR may run in shadow mode only in tests/diagnostics.
- [ ] Shadow construction is disabled in benchmarks and PGO.
- [ ] Cutover is by complete function or coherent region.
- [ ] Migrated consumers delete the old representation promptly.
- [ ] Every adapter has an owner, removal phase, and test.
- [ ] No permanent `V2`, `New`, or migration naming remains.

## Phase 0: Evidence And Comparison Protocol

This is prerequisite work, not an easy optimization phase.

### Work Checklist

- [x] Extend retained-byte accounting to tokens/CST.
- [x] Account for semantic types, signatures, shapes, and canonical maps.
- [x] Account for current lowered functions, nodes, captures, parameters, maps,
  strings, vectors, and boxes.
- [x] Account for dynamic symbols and leaked shape/name storage.
- [x] Distinguish peak construction bytes from final retained bytes.
- [x] Add a deterministic stats diagnostic over benchmark scripts, `core/`,
  `examples/`, `showcase/`, and syntax/sema/runtime fixtures.
- [x] Add every hot type to `scripts/ir-layout.py`.
- [x] Add size assertions for existing compact AST IDs/tags/data.
- [x] Capture two `make bench-fast` baselines to prove allocation and peak-live
  signals are bit-stable (or exactly repeatable after mediation).
- [x] Capture focused repository-check and runtime memory measurements under
  the same fast protocol.
- [x] Capture coverage/blocker distributions.
- [x] Record lowerer, lowered runner, and evaluator line counts as descriptive
  complexity context.

### Exit Gate

- [x] Retained totals reconcile with components.
- [x] Capacity and backing strings are included.
- [x] No-change structural repeats are identical.
- [x] Allocation and peak-live signals are stable enough to distinguish
  representation work under `make bench-fast`.
- [x] Whole-suite wall time is recorded as iteration telemetry only.
- [x] Complete before evidence is archived locally.

## Phase 1: Hard Architectural Vertical Slice

This phase proves the final shape before broad migration. It intentionally
includes difficult semantics rather than only literals and arithmetic.

### Required Vertical Slice

The prototype must represent and execute:

- [x] scalar and string literals;
- [x] slots, parameters, captures, assignment, and return;
- [x] direct and recursive function calls;
- [x] at least one mutually recursive pair;
- [x] if, loop, break/continue, and propagation;
- [x] a guarded match with bindings;
- [x] a record value and field access;
- [x] one `RuntimeOp`;
- [x] one traceable/erroring operation with an exact source location;
- [x] one unsupported operation that fails transactionally without placeholder
  code.

### Representation Work

- [x] Define compact IDs and optional sentinels.
- [x] Define `IrTag`, eight-byte `IrData`, `IrRange`, blocks, functions,
  parameters, captures, literals, locations, and shared extra storage.
- [x] Assert all foundational layouts.
- [x] Implement typed payload append/decode APIs.
- [x] Implement checkpoint/rewind.
- [x] Implement immutable finalization.
- [x] Implement `IrVerifier`.
- [x] Verify tag/data agreement, bounds, payload schemas, block/function
  ownership, slots, source locations, and optional sentinels.
- [x] Add deterministic dumps.
- [x] Add complete retained-byte accounting.

### Execution Work

- [x] Execute only the vertical slice in a test-only path.
- [x] Compare exact values/errors/traces with the arena evaluator.
- [x] Exercise recursion and mutual recursion.
- [x] Exercise rollback after partially emitted difficult control flow.
- [x] Confirm no recursive owned IR node exists in the prototype.
- [x] Execute the finalized vertical slice after dropping its compact AST,
  tokens, CST, and construction scratch.

### Architecture Decision

- [x] Review the vertical slice against Zig AST/ZIR/AIR/InternPool principles.
- [x] Document any deliberate deviation.
- [x] Reject the design and repeat this phase if the difficult cases require
  wide rows or ad hoc owned payloads.

### Exit Gate

- [x] Common instruction storage is at most 16 amortized retained bytes.
- [x] Corpus-weighted extra payload estimate fits the initial 24-byte target.
- [x] Malformed stores are rejected before execution.
- [x] Failed lowering commits no instructions.
- [x] Difficult semantic parity is exact.
- [x] The vertical slice is self-contained for execution apart from the source
  map retained for diagnostics/tracing.
- [x] No production behavior or benchmark is changed.
- [x] The chosen representation is approved as the foundation for all later
  phases.

## Phase 2: Transactional Lowering And Dependency Architecture

This removes the hardest correctness liability before broad opcode migration.

### Work Checklist

- [x] Define structured blockers with construct identity and source location.
- [x] Separate dependency discovery from instruction emission.
- [x] Build compact function identities before bodies.
- [x] Compute a dependency graph once.
- [x] Compute SCCs over compact function IDs.
- [x] Emit a function/SCC into transactional scratch.
- [x] Commit only after verification.
- [x] Rewind or drop all failed output.
- [x] Accumulate coverage diagnostics from blocker results.
- [x] Remove `Unit` substitution from migrated paths.
- [x] Remove blocker-counter comparisons as correctness checks.
- [x] Preserve blocker labels, counts, callees, and sample spans.
- [x] Shrink/split construct probe state as responsibilities move.

### Exit Gate

- [x] Unsupported nodes create no executable instructions.
- [x] Counter state cannot decide whether code is safe.
- [x] Self/mutual recursion succeeds through the graph/SCC model.
- [x] Failed SCC construction leaves no committed state.
- [x] Coverage diagnostics retain current detail.
- [x] Probe/build allocations and retained state improve or remain flat.
- [x] Current production coverage does not regress.

## Phase 3: Interned Semantic Identities

This is foundational because final instruction payloads must point to stable
semantic identities rather than owned `Type` trees.

### Work Checklist

- [x] Inventory `Type::clone`, callable clone, record/module construction,
  equality, hashing, and allocation.
- [x] Define `TypeId`, `SignatureId`, and `ShapeId`.
- [x] Implement compact type tag/data/extra columns.
- [x] Intern primitives and unary/container types.
- [x] Intern result/optional types.
- [x] Intern callable signatures and parameter metadata.
- [x] Intern record/module shapes with deterministic field order.
- [x] Decide and document whether semantic and runtime records share `ShapeId`;
  prefer one canonical pool when their ordered fields are identical.
- [x] Keep recovery facts separate and non-executable.
- [x] Convert stored signatures and IR checks to IDs.
- [x] Measure dropping versus retaining canonical maps after checking.
- [x] Delete recursive clones from migrated persistent owners.

### Exit Gate

- [x] Equal stable types/shapes have equal IDs.
- [x] Diagnostics render identical type names and spans.
- [x] Repository checking allocation/peak live does not regress.
- [x] Semantic retained bytes/source byte materially decrease.
- [x] Executable IR stores no owned `Type`.
- [x] Semantic/module contract tests pass.

## Phase 4: Full Indexed IR Migration

### Expressions

- [x] literals and program-owned string/bytes/path pools;
- [x] slot/parameter/capture reads;
- [x] unary/binary operations;
- [x] field/index/slice operations;
- [x] list/map/record/tag construction;
- [x] result construction, fallback, and propagation;
- [x] formatting, comprehensions, and pipelines;
- [x] direct, dynamic, self, and module calls;
- [x] filesystem/path/archive/hash operations;
- [x] process command, run, spawn, wait, and abort forms.

### Statements And Control Flow

- [x] bindings, destructuring, assignment, discard, expression;
- [x] return, break, continue, propagation, and defer;
- [x] blocks, if, loops, while, for, and guard;
- [x] general match with source-ordered flat arms;
- [x] measured compact exact-match dispatch;
- [x] stream and parallel control bodies.

### Functions And Metadata

- [x] one compact parameter row;
- [x] cold optional parameter validation/default tables;
- [x] compact capture rows;
- [x] function body/parameter/capture ranges;
- [x] `u32` slot counts and IDs;
- [x] pure/proc, namespace, import, return, and mutability metadata;
- [x] compact function lookup by identity;
- [x] no five-`SmallVec` function header.

### Verification And Parity

- [x] payload verifier coverage for every opcode;
- [x] block terminator and ownership checks;
- [x] slot/type/function/pattern/location bounds;
- [x] exact arena-versus-IR tests for every construct;
- [x] corpus differential execution;
- [x] opcode frequency and extra-word reports;
- [x] coverage reports derived without placeholder construction.

### Exit Gate

- [x] All currently lowered semantics have indexed equivalents.
- [x] No indexed body owns nested node vectors.
- [x] Function/parameter metadata meets budgets.
- [x] Corpus-weighted IR is materially smaller including heap storage.
- [x] Coverage is at least current coverage.
- [x] Shadow mode remains disabled in production benchmarks.

## Phase 5: Top-Level, Effect, And Fallback Boundary

This difficult architectural decision occurs before production cutover.

### Strategies To Compare

- [x] whole-program lowering with honest whole-program fallback;
- [x] coherent top-level regions split at dynamic/effect boundaries;
- [x] arena top-level orchestration calling indexed functions.

The third strategy is an experiment and migration fallback, not permission to
retain a second general production interpreter. It may be selected only if the
arena-owned portion is a deliberately small effect-orchestration shell that can
be represented as an explicit driver plan without retaining general AST
evaluation. Otherwise choose whole-program or coherent-region lowering.

### Required Boundaries

- [x] imports and module installation;
- [x] cwd/environment mutation;
- [x] process execution/redirection;
- [x] signal hooks and cancellation;
- [x] tracing-sensitive operations;
- [x] dynamic modules/calls;
- [x] top-level bindings captured by functions;
- [x] defers and propagated failures.

### Work Checklist

- [x] Define explicit effect metadata.
- [x] Define exact region state synchronization.
- [x] Preserve process/syscall/trace boundaries.
- [x] Measure coverage, latency, allocation, retained duplication, and code
  complexity for all three strategies.
- [x] Choose the simplest strategy within measurement noise of the fastest.
- [x] Record the decision and rejected alternatives.
- [x] Delete machinery belonging only to rejected strategies.
- [x] Express any surviving orchestration shell as compact executable metadata,
  not general AST nodes.

### Exit Gate

- [x] The boundary has written benchmark/correctness evidence.
- [x] Committed regions have no silent internal fallback.
- [x] Effects, signals, defers, propagation, imports, and traces have parity.
- [x] `xsh_process_pipeline` syscalls do not regress.
- [x] Boundary synchronization is compact and documented.
- [x] Permanent statement-by-statement dual-representation synchronization is
  absent unless decisive evidence requires it.
- [x] The selected final boundary does not require a permanent equal arena
  interpreter.

## Phase 6: Production Cutover And Recursive IR Deletion

### Work Checklist

- [x] Install only verified indexed programs/functions/regions.
- [x] Add test-only force-arena and force-IR modes.
- [x] Differentially run the complete corpus.
- [x] Enable indexed production execution without shadow building.
- [x] Run focused allocation/latency comparisons.
- [x] Run complete `make bench-fast`.
- [x] Defer `make bench-pgo` until the non-PGO campaign gates pass and the
  implementation is a stable release candidate.
- [ ] Remove `LoweredExpr`.
- [ ] Remove `LoweredStmt`.
- [ ] Remove `LoweredPattern`.
- [ ] Remove wide `LoweredPureFunction`/top-level objects.
- [ ] Remove obsolete runners, probes, maps, and adapters.
- [x] Ensure every executable language construct is represented by indexed IR
  or an explicit host/runtime operation referenced by that IR.
- [x] Drop the compact AST, token table, CST, and lowering scratch before
  production execution when diagnostic/tracing ownership permits.

### Exit Gate

- [ ] One lowered executable representation remains.
- [ ] Migrated functions/regions retain no recursive lowered body.
- [ ] Production execution does not walk `AstArena`.
- [ ] User-visible latency improves or remains flat while memory materially
  improves.
- [ ] Full behavior, benchmark, and coverage gates pass.
- [ ] Old IR code is deleted rather than left dormant.

Current implementation note (2026-07-25): production retains only the verified
`FullProgram`. Borrowed indexed views and the first direct executor slice cover
ordinary scalar/list/record/format/result expressions, field and method access,
module and user calls, typed integer/boolean nodes, bindings, assignment,
branches, loops, iteration, printing, returns/yields, and matching top-level
driver forms. Common collection pipelines execute their indexed stage payloads
directly, including text/JSON lines, map/filter, sorting/grouping, predicates,
aggregates, collection, and range/count transforms. Selection is all-or-nothing
per function or driver step. Process, parallel/block pipeline stages, match,
defer, import, signal, and remaining cold opcode families still cross the
recursive decode compatibility boundary, and compact lowering still constructs
recursive scratch before encoding it. The non-PGO `make bench-fast` memory and
allocation columns remain flat after this executor cut, confirming that
construction-side scratch is the next performance target. The recursive
deletion and performance exit boxes therefore remain open.

## Phase 7: Record Shapes And Runtime Value Movement

This starts only after the IR foundation is stable so value work is measured
against the final execution model.

### Work Checklist

- [ ] Attribute record/module/map/list allocation, clone, and drop traffic on
  JSON rollup, extension counting, and manifest hashing.
- [ ] Introduce interned record/module shapes.
- [ ] Reuse semantic `ShapeId` for identical ordered structures, or use a
  distinctly named runtime identity when ownership genuinely differs.
- [ ] Store fixed-shape fields densely.
- [ ] Replace owned field strings with `Name`/shape identities.
- [ ] Add borrowed lookup and unique-owner mutation paths.
- [ ] Preserve deterministic rendering and field order.
- [ ] Re-measure public `Value` layout after shape migration.
- [ ] Prototype smaller `Value` representations only if remaining evidence
  justifies it.
- [ ] Measure scalar, record-heavy, clone/drop, and thread-transfer behavior.
- [ ] Remove obsolete fixed-record `BTreeMap<String, Value>` paths.

### Exit Gate

- [ ] Fixed records/modules do not use general tree maps.
- [ ] Ordering, mutation, equality, and rendering semantics pass.
- [ ] Record-heavy allocation or latency improves decisively.
- [ ] Common scalar behavior does not regress.
- [ ] `Value` size does not increase.

## Phase 8: Reclaimable Interning

### Work Checklist

- [ ] Inventory `Name::as_str() -> &'static str` assumptions.
- [ ] Separate preloaded static names from dynamic session/program names.
- [ ] Define owned dynamic symbol storage with stable IDs.
- [ ] Pass an owner where borrowed spelling is required.
- [ ] Remove dynamic `Box::leak` usage for names and record shapes.
- [ ] Add repeated load/check/drop plateau tests.
- [ ] Measure CLI and long-lived `xshi` behavior.
- [ ] Ensure caches do not retain dropped programs.

### Exit Gate

- [ ] Dynamic names are reclaimable.
- [ ] Preloaded names remain cheap/deterministic.
- [ ] Repeated sessions reach a stable memory plateau.
- [ ] CLI latency/allocation does not materially regress.
- [ ] No replacement global mutable leak exists.

## Phase 9: Explicit Execution Frames

### Work Checklist

- [ ] Measure current native recursion and frame stack use.
- [ ] Define compact call frames with function, cursor, slots, return
  destination, defer/control, and trace identity.
- [ ] Define explicit block/loop/match/propagation/defer control frames.
- [ ] Reuse slot/frame storage where safe.
- [ ] Migrate indexed execution.
- [ ] Preserve recursion limits, tracebacks, cancellation, and panic isolation.
- [ ] Reduce worker stack after stress tests.
- [ ] Delete the large-stack worker when ordinary stacks suffice.

### Exit Gate

- [ ] Deep language recursion cannot exhaust native stack before language limits.
- [ ] Traceback and defer order is exact.
- [ ] Frame bytes/allocations are reported.
- [ ] The 64 MiB stack reservation is deleted.
- [ ] Recursive and ordinary runtime benchmarks do not regress.

## Phase 10: Arena, Span, CST, And Builder Compaction

This localized work is intentionally late so it targets structures that
survived the foundational redesign.

### Work Checklist

- [ ] Rank every `AstArena` table by retained capacity/frequency.
- [ ] Move truly rare tables behind lazy cold storage or shared extra data.
- [ ] Consolidate builder staging only where nesting remains correct.
- [ ] Preserve isolated staging for recursively nestable constructs.
- [ ] Compare inline spans with main-token IDs, token-relative ranges, and
  sparse overrides.
- [ ] Keep CST only on tooling paths.
- [ ] Drop token/CST data before runtime where lifetimes allow.
- [ ] Pursue an at-most-512-byte empty arena header only if corpus evidence says
  per-file header cost is material.

### Exit Gate

- [ ] Arena bytes/source byte decrease.
- [ ] Parse/check/format latency does not regress.
- [ ] Nested construction tests pass.
- [ ] Diagnostics and formatter fidelity remain exact.
- [ ] Cold-table allocations do not erase header savings.

## Phase 11: Cleanup And Final Evidence

### Work Checklist

- [ ] Delete remaining migration adapters, flags, dumps, and shadow paths.
- [ ] Rename final types without `New`, `V2`, or `Indexed`.
- [ ] Split owner modules along stable IR/builder/verifier/execution/pool/frame
  boundaries.
- [ ] Update `docs/ARCHITECTURE.md`.
- [ ] Update `docs/FRONTEND.md`.
- [ ] Update `docs/BENCHMARKING.md`.
- [ ] Update `docs/AGENT-ROUTING.md`.
- [ ] Update `docs/TEST-MAP.md`.
- [ ] Update applicable specifications.
- [ ] Run full behavior tests.
- [ ] Run full benchmarks and focused repeats.
- [ ] Run coverage and layout reports.
- [ ] Run syscall diagnostics where applicable.
- [ ] Run regular-versus-PGO comparison.
- [ ] Replace historical baseline data with final measurements.

### Exit Gate

- [ ] One production path owns each supported behavior.
- [ ] Current documentation describes reality.
- [ ] No adapter lacks a removal condition.
- [ ] Every campaign completion criterion is satisfied.
- [ ] Remaining work is ordinary measured maintenance, not unfinished migration.

## Opcode Admission Policy

Zig uses many tags without making each instruction wide. XSH may also use a
rich opcode vocabulary, but every opcode must earn its place.

Add an opcode only when:

- [ ] it removes repeated runtime type/method dispatch on a represented path;
- [ ] it encodes a distinct semantic, error, or trace contract;
- [ ] it materially simplifies lowering/execution;
- [ ] it permits compact payload storage unavailable to a generic opcode;
- [ ] corpus frequency and benchmarks justify specialization where applicable.

Reject an opcode when:

- it bakes one benchmark fixture into runtime behavior;
- `RuntimeOp` plus compact flags expresses the same behavior;
- it duplicates another opcode for an unmeasured branch saving;
- it requires a wide inline payload;
- arena parity cannot be specified/tested;
- it hides an effect from tracing or analysis.

Every opcode documents:

- [ ] inputs and types;
- [ ] output type/value;
- [ ] extra payload schema;
- [ ] possible errors and source location;
- [ ] effect/trace behavior;
- [ ] verifier invariants;
- [ ] arena semantic owner;
- [ ] direct parity tests;
- [ ] corpus/benchmark justification for specialization.

## Review Checklist

### Representation

- [ ] What is the retained cost per item?
- [ ] Does a rare case enlarge the common case?
- [ ] Is every persistent index `u32`?
- [ ] Is optionality still four bytes?
- [ ] Are variable children in shared storage?
- [ ] Do strings/types use the correct lifetime identity?
- [ ] Are capacities/backing allocations counted?
- [ ] Can an earlier stage be dropped sooner?

### Correctness

- [ ] What is the semantic oracle?
- [ ] Can invalid data be constructed?
- [ ] Does the verifier reject malformed tag/data/range/ID combinations?
- [ ] Are errors, spans, traces, output, and status identical?
- [ ] Do failures rewind temporary storage?
- [ ] Are recursion, imports, captures, defaults, defers, and effects covered?

### Performance

- [ ] Which user-visible benchmark contains the operation?
- [ ] Which allocation, peak-live, retained, and latency cost should change?
- [ ] Did setup remain in the same measured location?
- [ ] Were benchmark processes serial?
- [ ] Did timing direction repeat?
- [ ] Did corpus/coverage/interner warmth change?
- [ ] Did generated code or syscalls change?

### Simplicity

- [ ] Does the new layer eliminate an old responsibility?
- [ ] Can an old type, adapter, map, or branch be deleted now?
- [ ] Is analysis separate from construction?
- [ ] Is there one owner for opcode semantics?
- [ ] Does every migration switch have a deletion phase?
- [ ] Was the simplest strategy within performance noise chosen?

## Risks And Countermeasures

### Compact Storage Becomes Untyped Word Soup

- [ ] Use typed IDs and payload accessors.
- [ ] Document one schema per tag.
- [ ] Forbid raw extra indexing outside the owner.
- [ ] Verify and deterministically dump stores.
- [ ] Preserve comments explaining non-obvious packing.

### Dual Execution Paths Drift

- [ ] Keep the arena evaluator as migration oracle.
- [ ] Differentially test each migrated construct.
- [ ] Cut over by complete function/region.
- [ ] Delete migrated old paths promptly.
- [ ] Never implement semantics in only one path.

### Size Wins Add Allocation Or Indirection

- [ ] Measure retained capacity and allocation traffic.
- [ ] Report corpus-weighted bytes per instruction.
- [ ] Separate common and rare payload costs.
- [ ] Reject `size_of`-only evidence.

### Type Interning Retains Too Much

- [ ] Report canonical map and compact pool bytes separately.
- [ ] Separate checker and session lifetimes.
- [ ] Drop construction maps when reuse does not repay them.
- [ ] Measure repeated sessions.

### Smaller Values Slow Scalars

- [ ] Retain inline scalar paths.
- [ ] Benchmark scalar and record-heavy operations.
- [ ] Treat the size target as subordinate to allocation/latency.
- [ ] Measure clone/drop/thread transfer.

### Top-Level Simplification Loses Coverage

- [ ] Compare whole-program, region, and arena-orchestration strategies.
- [ ] Preserve explicit effects.
- [ ] Use real coverage frequency.
- [ ] Prefer the simplest strategy within benchmark noise.

### Global Interners Contaminate Measurements

- [ ] Run benchmark processes serially.
- [ ] Discard warmup.
- [ ] Report interner growth.
- [ ] Compare cold and warm sessions when relevant.
- [ ] Complete the owned-interner phase.

### Migration Infrastructure Becomes Permanent

- [ ] Give each adapter/flag an owner and deletion phase.
- [ ] Require displaced structures to be deleted at phase exit.
- [ ] Never build shadow IR in production.
- [ ] Complete the cleanup phase before declaring success.

## Decision Log

Agents append entries here as decisions are made.

Template:

```text
Date:
Phase:
Decision:
Alternatives:
Evidence:
Affected workloads:
Revisit condition:
```

Initial decisions:

1. Multiple frontend/IR layers remain; layer count is not itself a complexity
   metric.
2. Executable IR becomes a compact indexed store rather than a recursive Rust
   enum graph.
3. Zig's tag/data/extra and interned-ID discipline is the primary reference.
4. The existing curated Divan suite remains the performance and PGO authority.
5. The arena evaluator remains the semantic oracle during migration.
6. Difficult representation, lowering, recursion, verification, and boundary
   decisions precede localized compaction.
7. Runtime `Value` redesign follows IR cutover so it targets measured remaining
   costs.
8. Placeholder executable instructions are forbidden in the final lowerer.

Date: 2026-07-25
Phase: 0
Decision: Freeze the stage-split allocator and retained-byte protocol, the
vertical-slice inputs, and `make bench-fast` memory columns as the Phase 1
comparison baseline. Treat whole-suite wall time as telemetry only.
Alternatives: Divan maximum allocation alone; timing-heavy multi-sample gates;
unreconciled per-structure estimates without an installed-state measurement.
Evidence: `target/frontend-campaign/phase-0/PROTOCOL.md`,
`frontend-stats.json`, `frontend-stats-vertical-slice.json`,
`bench-fast-1.tsv`, `bench-fast-2.tsv`, `ir-layout.txt`, `coverage.json`, and
`line-counts.txt` under the same directory. The two memory-column extracts and
the two full stats JSON files compare byte-for-byte.
Affected workloads: Full frontend corpus, frozen vertical slice and blocker,
repository check/format/lint, and curated runtime benchmark scripts.
Revisit condition: Phase 1 changes stage ownership or exposes an exact recursive
lowered retained-byte walker that can replace the labeled library fallback.

Date: 2026-07-25
Phase: 1
Decision: Approve the parallel tag/data/location columns, shared `u32` extra
storage, compact IDs, verified immutable finalization, and separate cold pools
as the representation foundation for later phases.
Alternatives: Recursive owned instruction enums; wide inline instruction rows;
unverified bytecode; direct production cutover during the prototype.
Evidence: `target/frontend-campaign/phase-1/PROTOCOL.md`, `tests.txt`,
`vertical-dump.txt`, `vertical-storage.txt`, `corpus-storage-summary.txt`,
`ir-layout.txt`, and `bench-fast-mediated-memory.txt`. The hard slice uses 123
instructions at 13 common-row bytes and 3.675 extra bytes/instruction. The
287-file corpus estimate covers 343 prototype instructions at 3.475 extra
bytes/instruction. The mediated benchmark memory table matches Phase 0 exactly.
Affected workloads: Frozen vertical slice and blocker, compact function units,
all Phase 0 corpus roots for storage weighting, and the curated fast benchmark
suite for production non-regression.
Revisit condition: Phase 2 cannot preserve the same verified format while
moving dependency discovery, SCC construction, and transactional emission to
compact semantic IDs.

Deliberate deviation: Phase 1 constructs from `LoweredFunctionUnit` so storage,
verification, failure, and execution can be proved independently. The finalized
program retains no lowered node. Phase 2 owns compact dependency/SCC planning,
verification-before-commit, and replacing blocker-counter correctness checks;
Phase 4 removes recursive bodies from the finalized broad store. The temporary
construction/decode compatibility boundary is deleted at the Phase 6 cutover.

Date: 2026-07-25
Phase: 2
Decision: Approve compact function identities, one dependency graph and Tarjan
SCC plan, dependency-first SCC emission beyond a commit watermark, partial
verification before commit, and structured blocker-derived coverage as the
construction architecture.
Alternatives: Retry/fixpoint emission driven by blocker counters; per-function
commit before recursive peers verify; placeholder `Unit` instructions; keeping
coverage state coupled to the oversized construct probe.
Evidence: `target/frontend-campaign/phase-2/PROTOCOL.md`, `tests.txt`,
`vertical-graph-summary.txt`, `blocker-detail-summary.txt`,
`corpus-graph-summary.txt`, `frontend-stats-comparison.txt`, `coverage.json`,
`ir-layout.txt`, and `bench-fast-mediated-memory.txt`. The hard slice contains
7 functions, 8 edges, 6 SCCs, and 2 recursive SCCs; all 123 instructions commit.
The corpus adapter measured 142 functions, 20 edges, 140 SCCs, and 6 recursive
SCCs. Production frontend retained columns and coverage match Phase 0 exactly;
mediated benchmark memory matches Phase 1 exactly. Splitting the result reduced
`IrBuildOutcome` from 544 to 176 bytes.
Affected workloads: Frozen vertical slice and blocker, all Phase 0 corpus roots,
the production lowerability coverage report, frontend retained diagnostics, and
the curated fast benchmark suite.
Revisit condition: Broad opcode migration cannot preserve SCC-level rollback,
structured blocker detail, or verification-before-commit without widening the
approved executable rows.

Remaining adapter: body opcode emission still reads a successfully committed
`LoweredFunctionUnit`; dependency discovery, SCC planning, commit safety,
verification, and blocker coverage no longer depend on construct-probe counters.
Phase 4 prevents this body adapter from escaping into the finalized program and
covers the full syntax vocabulary. Phase 6 moves the sink into the compact
lowerer and deletes the temporary adapter with the recursive IR.

Date: 2026-07-25
Phase: 3
Decision: Freeze the Phase 1 row/store format and Phase 2 dependency/SCC
transaction design. Approve checked one-based `u32` `TypeId`, `SignatureId`,
and `ShapeId` identities; compact finalized type tag/data/extra columns;
signature range/extra columns; one deterministic ordered shape pool; and
construction-only canonical maps. Semantic record and module types share the
same `ShapeId` when their ordered fields are identical, and Phase 7 reuses that
identity for dense runtime records.
Alternatives: Owned semantic `Type`/`CallableType` trees in executable rows;
wider instruction rows; separate record and module shape namespaces; retained
canonical maps beside the executable program; executable recovery identities;
continuing with the vertical slice's closed `IrValueType`/return-kind enums.
Evidence: `target/frontend-campaign/phase-3/PROTOCOL.md`,
`semantic-inventory.txt`, `tests.txt`, `sema-tests.txt`, `module-tests.txt`,
`corpus-semantic-summary.txt`, `ir-layout.txt`,
`frontend-stats-vertical-slice.json`, `coverage.json`, and
`bench-fast-mediated-memory.txt`. Across 287 checked corpus files, 2,908,060
bytes of recursively owned semantic facts compact to 112,736 finalized pool
bytes, from 5.333404 to 0.206759 bytes per source byte (96.12% lower).
Construction canonical maps account for 241,060 additional bytes and are
dropped. The corpus produces 3,296 type, 396 signature, and 414 shape
identities; 19,187 recovery-bearing facts are rejected from executable
interning. `TypeId`, `SignatureId`, and `ShapeId` are each 4 bytes; `TypeTag`
is 1 byte; `IrFunction`, `IrParam`, and `IrCapture` are 36, 16, and 12 bytes.
Common instruction storage remains 13 bytes. Production frontend retained
columns, coverage, and mediated benchmark allocation/peak-live columns match
the frozen baselines exactly.
Affected workloads: Compact declaration/body semantic facts over all Phase 0
corpus roots, the frozen indexed vertical slice and rollback fixture, checker
and module-contract integration tests, production frontend retained
diagnostics, coverage, and the curated fast benchmark suite.
Revisit condition: `TypeId`, `SignatureId`, or `ShapeId` cannot fit naturally
into the accepted `u32` ID/data/extra model during broad Phase 4 migration
without owned semantic trees or wider hot instruction rows.

Remaining adapter: Phase 3 constructs semantic identities from the committed
`LoweredFunctionUnit` metadata and record requirements used by the indexed
vertical slice. Phase 4 expands that freeze boundary to every committed body and
ensures no recursive node remains in the finalized store. Phase 6 moves the
indexed sink into direct production execution and deletes the temporary
construction/decode boundary. Runtime values retain their current record maps
until Phase 7.

Date: 2026-07-25
Phase: 4
Decision: Approve the exhaustive indexed function-body vocabulary, explicit
owned block rows, compact hot/cold function metadata split, program-owned
literal and location pools, and whole-store verification. Every current
recursive expression, statement, typed fast path, pattern, pipeline stage,
process/run payload, and persistable compile-time value has one exhaustive
encode/decode schema. Recovery-only checker types are converted at the commit
boundary to their existing executable wildcard meaning, `Any`; recovery
identities remain forbidden.
Alternatives: Keep widening the frozen Phase 1 vertical-slice store; store
recursive nodes behind IDs; retain hash maps and vectors inside indexed
instructions; add an unverified byte stream; shadow-build the store in
production.
Evidence: `target/frontend-campaign/phase-4/PROTOCOL.md`, `tests.txt`,
`corpus-ir-summary.txt`, `opcode-frequency.txt`, `blockers.txt`,
`ir-layout.txt`, `frontend-stats-vertical-slice.json`, `coverage.json`, and
`bench-fast-mediated-memory.txt`. The corpus encodes and verifies 837 committed
functions and 43,589 instructions across 204 wholly executable files. Final
store storage is 1,843,105 bytes, 57.11% below the conservative 4,297,136-byte
recursive-row lower bound, which excludes the recursive representation's
nested heap storage. It uses 13.560 extra bytes per instruction. `FullBlock`,
`FullFunction`, `FullParam`, and `FullCapture` are 20, 32, 12, and 12 bytes;
the hot instruction row is 9 bytes before amortized extra/location storage.
Exact differential tests preserve values, stdout, runtime errors and source
locations, and normalized traces. Production coverage and mediated benchmark
allocation traffic do not change because no product path builds shadow IR. The
local checkout did not retain Phase 0/3 target artifacts, so coverage compares
with the frozen local pre-change capture and the two serial Phase 4 benchmark
runs compare with each other; the protocol also compares historical artifacts
when they are available.
Affected workloads: Every fully committed function in the Phase 0 corpus,
the frozen full-construct vertical slice and malformed-store cases, production
frontend coverage, and the curated fast benchmark suite.
Revisit condition: Phase 5 selects a top-level boundary that requires different
function ownership, or Phase 6 direct indexed execution shows that a measured
hot payload needs a specialized layout.

Temporary execution boundary: the compact entry freezes committed function
units immediately and drops them with all frontend scratch; the finalized
program contains no recursive body. Tests decode into temporary
`LoweredPureFunction` values only to reuse the current runtime as the semantic
oracle. Phase 6 replaces that decoder with direct indexed execution and deletes
the construction/decode compatibility boundary. Product binaries neither build
nor install the Phase 4 store.

Date: 2026-07-25
Phase: 5
Decision: Select coherent effect regions inside an honestly admitted complete
driver program. Every executable top-level statement must lower before the
driver commits; declaration-only source rows become explicit compact skips.
Each source-ordered driver step owns an instruction range, location, exact
effect bitset, and typed read/write slot rows. Adjacent effect-free steps share
a region; imports, cwd/env changes, process work, signals/cancellation,
trace-sensitive calls, dynamic dispatch, defers, propagation, and host
operations are isolated boundaries. Region synchronization is a compact
verified union of step-local binding reads and mutation write-backs.
Alternatives: One whole-program region has the same honest admission but hides
effect scheduling and synchronization inside a broad body. Arena top-level
orchestration covers the remaining four corpus files today but retains
2,935,921 bytes of general arena state for the admitted corpus and requires a
permanent equal interpreter. Statement-granular arena/IR fallback was rejected:
the selected store has no fallback tag and verification rejects every
executable gap.
Evidence: `target/frontend-campaign/phase-5/PROTOCOL.md`, `tests.txt`,
`strategy-summary.txt`, `strategies.txt`, `effects.txt`, `blockers.txt`,
`ir-layout.txt`, `coverage.json`, `frontend-stats-vertical-slice.json`,
`bench-fast-1.tsv`, `bench-fast-2.tsv`, `bench-latency-1.tsv`,
`bench-latency-2.tsv`, and `bench-syscalls.txt`. The 287-file corpus admits 283
complete programs containing 1,647 steps, 705 coherent regions, and 2,919
synchronization rows. Driver metadata is 175,280 bytes. All four rejections are
unlowered function bodies rather than partial top-level commits. Exact decoded
driver tests preserve values, stdout/stderr, status, cwd/env behavior,
process execution, signal-hook registration, defers, errors, and normalized
trace payloads after the arena and construction scratch are dropped. Loaded
module tests also verify recursive whole-program admission, nested driver
execution, and cross-source locations. The Phase 5 store remains test-only, so
the candidate strategies are within production latency/allocation noise;
serial benchmark runs verify that no production path changed. Process/runtime
owner files are unchanged, so
`xsh_process_pipeline` uses the same syscall path; the protocol captures
`strace` evidence on supported Linux hosts.
Affected workloads: All Phase 0 corpus roots, the top-level env/cwd/process/
signal/defer/propagation boundary fixture, `xsh_short_script`,
`xsh_process_pipeline`, and the complete curated benchmark suite.
Revisit condition: Phase 6 direct indexed execution requires an effect not
represented by the driver bitset, or measured direct execution shows that
region-level synchronization should be made coarser without obscuring a
runtime boundary.

Temporary execution boundary: `FullDriverStep`, `FullDriverRegion`, and
`FullDriverProgram` are finalized compact metadata and retain no AST nodes.
Tests decode them into the recursive `LoweredProgram` solely to use the current
evaluator as the migration oracle. Phase 6 installs and executes the verified
rows directly, removes that decoder and the recursive top-level cache, and
eliminates whole-program arena fallback after the four remaining function
blockers are represented. No implementation machinery was added for either
rejected strategy.

## Completion Report

When the campaign finishes, replace this section with:

- [ ] before/after architecture diagrams;
- [ ] before/after type layouts;
- [ ] source/token/CST/AST/semantic/IR/runtime retained-byte tables;
- [ ] bytes per source byte and bytes per instruction for every corpus;
- [ ] allocation and peak-live deltas for every user-facing workload;
- [ ] latency and spread deltas;
- [ ] lowerability coverage and blocker changes;
- [ ] regular-versus-PGO comparison;
- [ ] syscall changes where applicable;
- [ ] deleted types, adapters, flags, and owner-module line counts;
- [ ] remaining limitations and their measured importance.

The campaign succeeds when XSH is compact because ownership and semantics are
clear, not because bytes were hidden, and when real user workflows become
faster or cheaper without making the language harder to understand or modify.
