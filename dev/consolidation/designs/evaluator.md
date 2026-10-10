# Design: one evaluator, two ways to schedule operands

Status: proposed 2026-10-10, awaiting owner approval. Part of workstream 2 of
`../CAMPAIGN.md`. Carries decision D1. Internal: nothing a script observes
changes except the three corrections listed under "Observable".

## How it works today

Two evaluators run the same verified program.

- The **frame evaluator** (`ExplicitFrames`) schedules work on a heap stack.
  It has arms for 34 expression tags and 29 statement tags, and a wildcard
  arm that hands anything else to the recursive evaluator.
- The **recursive evaluator** (`eval_indexed_expr_inner`,
  `eval_indexed_stmt_inner`) has an arm for every expression tag and 16
  statement tags. It cannot push work onto the machine that called it. When
  it reaches a call, a block, or a stage callback, it builds a new frame
  machine and runs it to completion on the native stack.

So a script that recurses through anything the recursive evaluator owns uses
native stack per level. `xs |> map { ... f(x) ... }` inside `f` is the known
case: each level adds a pipeline activation and a new machine, and the run
aborts at 100 to 150 levels. The same shape exists through `loop` and
`retry` expressions, `cd` and `env` statements, string and tag matches, a
call that is an operand of any recursive-only instruction, `.call`,
`utils.cache`, a deferred call, and a pull from a script producer. The only
depth check counts open calls, with a limit of 100,000.

What the paired arms share, read from all of them:

- Most pairs already call one value-level helper once their operands are
  evaluated (`lowered_binary_value`, `finish_record_entries`,
  `indexed_field_value`, `lowered_index_value`, `require_value`, and so on).
  They differ only in how operands are scheduled.
- Five differ in substance: `ExprValueBlock`, `ExprTry` (an `Err` leaves by
  a different mechanism), `ExprMethod`, and the call tags (the recursive side
  starts a new machine).
- No stream stage shares its per-item code between the materializing
  pipeline arm and `serial_pipeline.rs`. Seventeen stages are written twice.
- The recursive arms for `StmtPatternIf` and `StmtPatternWhile` cannot be
  reached.
- No test forces one route or compares the two. The tests named
  `*_both_routes` call one entry point and assert a value.

## Design

**Apply functions.** Each instruction has one function that takes its
evaluated operands and its scalar fields and produces its value. The helpers
that exist become these; the instruction table (workstream 1) names the
function for each tag. An apply function may perform a host operation. It
never runs script code: it does not call a function, run a block, run a
stage callback, or pull from a script producer.

**The in-place mark.** The verifier computes one bit per instruction.
An instruction is *in place* when all of these hold:

- its tag is in the in-place class below;
- every operand instruction is in place;
- its height in in-place operands is at most 128, the parser's nesting
  limit, so that the bound does not rest on lowering preserving depth.

The in-place class is the tags whose own behavior runs no script code and
does not wait on anything that services signals: the constants and slot
reads, all `Int*` and `Bool*` tags, and the operations whose only work is on
their operand values (comparison, arithmetic, selection among operands by a
condition or a pattern, record, list, map, tag, range, and format
construction, field and index access, slices, string measures and
predicates, path construction, checked conversion, `.require`, `Ok`, `Err`,
`Error`, `?`, `fail`, and the fallback operator), plus the file-system and
path operations that take operands and wait on nothing. Excluded: every call
tag, `ExprMethod` and `ExprModuleCall` (what they run depends on the method
or operation), value blocks, captures, error contexts, context scopes,
`loop`, `retry`, pipelines, comprehensions, and everything that starts or
waits on a process.

**Two schedulers over the same apply functions.**

- The **in-place scheduler** is the recursive evaluator reduced to the
  in-place class. It evaluates operands by recursion and calls the apply
  function. It is entered only for an instruction the verifier marked, so it
  never meets a tag outside the class, never starts a frame machine, and its
  native depth is at most 128.
- The **frame scheduler** handles every other instruction. An instruction
  that only evaluates operands and applies uses one generic continuation
  (operands remaining, values so far, the apply function); it does not need
  an arm and continuation variants of its own. Instructions with control
  flow keep specific arms.

The frame scheduler's dispatch is exhaustive: a tag is scheduled, or it is
listed as always in place. There is no wildcard arm in either scheduler.
`leaf_operand` and the per-tag fast paths that read a slot or literal are
the six-tag special case of the mark and are replaced by it.

**Nested machines.** With the mark, an operand never causes a new machine.
The other places that start one are moved onto the calling machine in the
order given below. Whatever remains is bounded by a counter: starting a
machine while more than a fixed number are open is a `stack-overflow`
failure, as the open-call limit already is. A signal hook starts one machine
and is bounded by that.

**Pipelines.** One pull-based engine runs a pipeline over any source; a list
is a source that is already complete. A stage's per-item behavior is one
function. A stage that runs a block or a callback pushes it on the calling
machine and resumes when it returns. Stages that need their whole input
(`sort`, `collect`, `batch`) keep a materialization boundary, and `par-map`
keeps its worker boundary. The fast paths the materializing arm has today
(a field projection in `map`, a field-equals-literal test in `where`) become
checks the engine makes before it runs a stage.

## Observable

Three behaviors differ between the two routes today. Each is settled by the
SPEC, and the losing behavior could only be reached with one kind of input.

- `flat-map`: the materializing arm also flattens a set and unwraps a
  `Result`, and does not wrap an item error; the serial path accepts a list
  or a stream and reports the error as a `map` item error. SPEC 13.3 says a
  list or a stream. The serial behavior stands.
- The span given to a failing part of a format string differs: the part's
  own span on frames, the whole expression's on the recursive side. The
  part's span stands.
- `ExprRecordUpdate` with no updates is the base value on both; no change.

## Order

Each step is its own item, is useful if every later step is parked, and is
accepted on the full gate.

1. **Bound nested machines.** Add the counter and the `stack-overflow`
   failure, with a native test for recursion through a stage block. This
   closes the open defect as it is filed.
2. **Delete what is dead:** the unreachable recursive arms for the two
   pattern statements, `FullFunctionView::has_defers`, the stage decoder
   that nothing calls.
3. **A way to test both schedulers.** `xsht test` gains an unlisted option
   that treats no instruction as in place, so every expression is scheduled
   on frames. The full gate runs the native suite once each way. This is the
   parity test invariant 2 of `docs/ARCHITECTURE.md` asks for.
4. **The mark.** The verifier computes it; the frame scheduler enters the
   in-place scheduler by it; `leaf_operand` goes.
5. **Frame scheduling for everything not in place.** The generic
   operand-then-apply continuation; frame arms for the fourteen statement
   tags and the expression tags that exist only on the recursive side. The
   two wildcard arms and every recursive arm outside the in-place class are
   deleted.
6. **Calls that hide in value-level code.** `f.call(args)` on a `Pure` or
   `Proc` lowers to the dynamic call instruction; `utils.cache` and external
   calls push on the calling machine; deferred actions run as frame work.
7. **One pipeline engine.**

Steps 6 and 7 are the largest and the likeliest to be parked. Step 1 keeps
every case they would fix a diagnostic instead of a crash.

## Tests

- Step 1: `tests/xsh/` gains recursion through a stage block, a `loop`
  expression, and a deferred call, each asserting `stack-overflow`.
- Step 3 onward: the native suite under both schedulers.
- Verifier unit tests for the mark: an instruction of each class, an
  operand that removes the mark, and the height bound.
- Step 5: a test that the frame scheduler's dispatch names every tag, in
  the style of the existing instruction-count assertions.
- Steps 4 to 7: the loop and call micro-workloads under `bench/`, paired, and
  the allocation counts of `xsht runtime-stats`. An expression tree that is
  wholly in place must not allocate more than it does today.
- Step 7: for every stage, one native test run over a list source and over
  a stream source with the same expected output.

## Relies on

Checked by the integrator at the start commit; if one is false this design
is parked whole.

- The frame evaluator's `eval_expr` and `eval_statement` end in a wildcard
  arm that calls the recursive evaluator through `with_lent_context`.
- The recursive evaluator starts a new `ExplicitFrames` for every call,
  block, and stage callback it reaches; it has no way to push onto the
  caller's machine.
- An instruction's operand ids are indexes of earlier instructions, so one
  forward pass can compute a property of an instruction from its operands.
- The value-level helpers in `lowered_ops.rs` are free functions that take
  no evaluator, and `require_value` takes it by shared reference.
- Signals are serviced before each statement on frames, and by process
  waits, sleeps, stdin reads, and the two scan statements.
- The parser enforces a nesting limit of 128 and nothing later enforces
  one.
- The open-call limit is the only depth check in the executor.

## Out of scope

Running a script producer on its consumer's machine: a pull still starts a
machine, bounded by the counter. Any change to instruction encoding. Any
change to `par-map` workers.
