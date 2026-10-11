# Design: one resource rule

Status: approved by the owner 2026-10-11 (drafted 2026-10-10). Part of workstream 8 of
`../CAMPAIGN.md`. Carries decisions D3, D5, and D12.

## The rule

A handle is a value that names one live host resource. Every handle type
follows one rule:

1. **Owner.** A handle is owned by the scope that created it.
2. **Moving out.** Ownership moves to an outer scope when the value that
   holds the handle becomes reachable from that scope: it is returned, it is
   the value of a block, it leaves a loop through `break`, it is assigned to
   a binding of an outer scope, or it is yielded to a consumer. Passing a
   handle as an argument lends it; the caller still owns it.
3. **Scope exit, in two steps around the scope's defers.**
   - Before the defers, the scope's *activities* are stopped: process
     handles are cancelled and reaped, network jobs cancelled and drained,
     open streams cancelled. A defer therefore observes a quiet scope.
   - After the defers, the scope's *capabilities* are released: roots are
     closed and locks unlocked, newest first. A defer may still use them.
     A value already released is skipped.
4. **After release.** Any operation on a released handle returns `Err` with
   that type's existing kind. `ProcessHandle.cancel()` alone is idempotent,
   so that `defer handle.cancel()` is safe; that is the stated exception.
5. **Evaluators.** A handle belongs to the evaluator that created it. Used
   from another one (a `par-map` worker), it is not live.

Steps 1, 2, and the first half of 3 are what `ProcessHandle` and `NetJob` do
today. The second half of 3 is what a `with` scope does today. The change is
that roots and locks are owned at all, and that streams are owned in the
code as the SPEC already says.

A release that fails at scope exit is dropped, as a failed cancellation is
today. `with NAME = VALUE { ... }` is unchanged and becomes the way to
observe that failure: it returns the failed release as the scope's `Err`.

## The table

One table in the registry, extending `ManagedResource`. The checker, the
runtime, and SPEC 11.8 read it.

| Type | Kind | Consumed by | Allowed after release | Scope exit |
|---|---|---|---|---|
| `ProcessHandle` | activity | `wait`, `cancel()`, `process.wait_any`, `wait_ready`, `wait_timeout` when they return it | `cancel()` | cancel and reap; a detached handle goes to the reaper |
| `NetJob` | activity | `wait()`, `cancel()` | none | cancel and drain |
| `Stream[T]` | activity | draining it | reading it yields nothing | cancel |
| `FsRoot` | capability | `close()` | none | close; a temporary root removes its directory |
| `FsLock` | capability | `fs.unlock(lock)` | none | unlock |

Outside the rule, each with its reason stated in the same SPEC section:

- **Raw descriptors** from `unix` and `linux` are `Int` (D5). They are a
  system-call interface, and the script closes them.
- **HTTP pools** are named state of the evaluator, opened and closed by
  name. A pool is not a value, so it has no owner scope.
- **`process.spawn(plan)`** returns a pid record and releases the child to
  the reaper. Its registry text, which claims lexical ownership, is
  corrected.
- **`unix.exec`** replaces the process, so nothing is released and no defer
  runs. Root descriptors are close-on-exec; a temporary directory stays.

## Changes

### `FsLock` is a type

`fs.lock` returns an opaque `FsLock`, not the record
`{id: Int, path: Path, shared: Bool}`. Its `path` and `shared` stay readable
as fields. It cannot be written as a record literal, is not JSON-compatible,
and carries the owner token a root carries. A record with those three fields
is no longer accepted by `fs.unlock`.

### Roots and locks are scope-owned (D12)

The count D12 asks for was taken on 2026-10-10 by classifying all 731 sites
in this repository and Laputa that create a root or take a lock:

| How the site releases | Roots | Locks |
|---|---|---|
| `defer NAME.close()` or `defer fs.unlock(NAME)` in the block that binds it | 593 | 7 |
| explicit release later in the block | 14 | 7 |
| a `with` scope or a `tempdir` scope | 83 | 12 |
| the handle itself leaves the block (returned, or assigned outward) | 8 | 0 |
| never released, nothing leaves | 5 | 0 |
| released by nothing, while a value derived from it leaves | **0** | **0** |

The last row is the hazard: a root that used to stay open by accident and
would now close under a caller that still uses a path from it. There are
none. The five leaks (one outside tests, `core/mktemp.xsh`) become correct.

Rule for the start commit: the integrator reads every site that creates a
root or lock with no release in its block and no enclosing scope form. If
more than five are in the hazard row, this change is parked and the campaign
builds the alternative in D12 instead. Otherwise each hazard site is changed
to return the root.

### Ownership is moved by type, not by walking every value

Today every successful function return copies the returned value into the
host representation and walks it for handles (`finish_call`), whatever its
type and whether or not any handle exists. The code's own comment says the
copy duplicates every container the value holds.

- The checker publishes, for each return, block value, `break` value, outer
  assignment, and `yield`, whether the static type may hold a handle. The
  predicate has the shape of `Type::can_escape_context_scope`: true for the
  five types and for a list, map, optional, result, record, union, or
  validated type that contains one, and for `Any`, an erased `Record`, and an
  error payload, whose contents are not known.
- Lowering records it on the instruction. The executor moves ownership only
  there, and only when a handle table is not empty.
- A scope exit that owns nothing does no table scan.

### Ownership moves from wherever the handle lives

Three places lose a handle today while the program can still reach it:

- A transfer moves a handle only when its owner is exactly the scope being
  left. A handle created in a middle block and assigned to an outer binding
  from a deeper block is not moved, and is released when the middle block
  ends.
- A yielded handle stays with the producer's scope.
- The walker over executor values skips one container shape (`StatsBlob`).

The rule in "Moving out" replaces the test: a handle reachable from the
moved value moves when its owner is any scope inside the target scope.

### Streams are in the table

A live stream (a suspended producer, or one backed by a process) is
registered with its owner scope and cancelled in the first step of scope
exit, which runs the producer's own defers. The reachability sweep for
suspended producers is removed.

If the existing stream tests cannot be kept green without the sweep inside
the item's budget, the item is parked and SPEC 11.8 is corrected to describe
the sweep instead. That is the other branch of D3 and needs no further
decision.

### Handles do not cross evaluators

Process handles, network jobs, and locks get the owner token roots have, so
a handle used in a `par-map` worker is not live there instead of indexing
the worker's own table. A worker that ends releases what it owns by the same
rule.

### Use after release is a check error

A new diagnostic, `check.use-after-release`.

- **Tracked:** a `let` or `var` local whose static type is exactly one of
  the four handle types. Not a field, an element, or an alias.
- **Release:** an operation in the "Consumed by" column, applied to that
  local, in a statement of the local's own block, evaluated unconditionally:
  not under `if`, `match`, a loop, a stage block, `and`, `or`, or `??`.
- **Error:** any later mention of the local in that block, including inside
  nested blocks, other than an operation in "Allowed after release".
  Assigning the local a new value ends the tracking.
- **Not tracked:** a release inside a conditional or a loop, a `defer`, a
  handle passed to a function. Those keep the run-time error.

No parameter syntax is needed, because a call never counts as a release.

### Lints

- `lint.redundant-close` (new): `defer NAME.close()` where `NAME` is a root
  bound in the same block and no `defer` precedes the binding in that block.
  Under that condition the deferred close was already the last action of the
  scope, which is where the scope's own close now runs, so removing it
  preserves behavior, and the lint has an autofix. 590 of the 593 sites have
  the defer on the line after the binding.
- No lint for `defer fs.unlock(NAME)`: a failed unlock raises from a defer
  and is dropped at scope exit, so the two are not the same.
- `lint.prefer-with-scope` is removed. Its premise was that a resource has
  to be released by hand.

## Migration

1. Checker and runtime lanes land the rule. Existing `defer NAME.close()`
   keeps working throughout, because the scope's release skips a value that
   is already released.
2. Routine lanes run `xsht lint --only lint.redundant-close --fix`, one
   directory per lane, in this repository and Laputa.

Existing tests change in three classes, and only these:

- A test that builds a lock record by hand, or passes a look-alike record to
  `fs.unlock`, is replaced by a rejected-program test.
- A test that releases a handle and then uses it in the same block, to
  assert the run-time error, moves the use into a helper function so the
  run-time error is still covered.
- A test of `lint.prefer-with-scope` is deleted with the lint.

The integrator lists each in the handoff log.

## SPEC text that changes

4.11 (the handle list), 8.7 (what is cleaned up before a block's defers, and
the note on `with`), 10.4 (the `with` section: what it is for, which handles
are released by their block, early release, a value leaving the scope, the
lint), 11.8 (rewritten around the rule and the table, and without the claim
that there is no wait-any, which `process.wait_any` contradicts), and 18.
The registry text for `fs.lock`, `fs.unlock`, the root constructors,
`FsRoot.close`, and `process.spawn`.

## Tests

- `tests/xsh/with-resource.xsh` keeps its coverage of the `with` scope.
- A new `tests/xsh/handle-ownership.xsh`: for each of the five types, the
  release at scope exit; each way of moving out; the order against defers
  (an activity stopped before a defer runs, a capability usable inside one);
  the middle-block assignment and the yielded handle, which fail today.
- Rejected programs for `check.use-after-release`, one per handle type and
  one per tracked position, and accepted programs for each untracked case.
- The existing micro-workloads under `bench/` for call and return cost,
  paired before and after: a return of a value that cannot hold a handle
  must not become slower.

## Relies on

Checked by the integrator at the start commit; if one is false this design
is parked whole.

- `process_handles` and `net_jobs` carry an `owner_scope`; `fs_roots` and
  `fs_locks` do not; streams have none.
- Scope exit releases owned handles before running the scope's defers
  (`exit_block_scope`), and a `with` scope releases after its body's defers.
- `finish_call` converts the returned value with `into_value()` and walks it
  on every successful return.
- A transfer moves a handle only when `owner_scope` equals the scope being
  left.
- `FsRootValue` has an owner token and a lock value does not.
- `ManagedResource::ALL` lists `FsRoot` and `FsLock`.
- `FsRoot.close()` on a live root returns `Ok` and does not report a failed
  directory removal.
- `xsht check` on this repository and on Laputa reports nothing.

## Out of scope

A typed descriptor (recorded in `TODO.md`); ownership across a call
boundary, and with it any way for a parameter to say it takes a handle; a
protocol by which a program defines its own resource type; a link between a
root and the roots opened from it.
