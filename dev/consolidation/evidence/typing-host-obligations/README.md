# Registered host receiver obligations

Indirect receiver inference now retains registered method relationships until
independent body constraints establish a concrete receiver. Actual ambiguity
remains an error. Main and frozen b235 remain untouched.

## Receiver constraints and publication

The old dispatcher returned `Unknown` immediately for an unresolved receiver.
That discarded the operation's result relationship. A later `anchor(flat)`
could not connect `flat = text.trim()` back to `text`.

The checker now indexes finite candidates by registered method name. Each
candidate retains its registry receiver domain. `BuiltinInstantiation`, the
existing static argument binder, and `TypeConstraints` own its relationships.
An unresolved occurrence retains receiver, argument, result, and expected
context identities. The existing cloned body precheck drains an index of
pending obligations by their unresolved dependency origins. Only affected
obligations wake after a unique candidate establishes substitutions.

Ordinary checking then publishes selected registered operations, argument
slots and types, effects, and loop item facts through the existing registry
checker. Lowering and execution gain no inference or AST fallback. There is
no repeated whole-body convergence loop or per-method inference rule.

A receiver domain with competing argument-compatible signatures retains all
those signatures. Return context never selects an overload within that domain.
The ordinary overload chooser is unchanged.

## Preserve all registered instance domains

The existing registry receiver-template table now explicitly supplies concrete
templates for EnvPathList, Path, Float, Str, Bytes, Status, Digest, Regex,
ProcessHandle, NetJob, and FsRoot. This changes their public `receiver_ty`
metadata from `None` to `Some` and makes `BuiltinInstantiation` validate the
registered concrete relationship. The registry adapter, method effects, and
required labels pass their existing parity checks.

Int and Record retain `None`: Int methods also accept UInt, and Record methods
accept structural records, erased records, and module contracts. These domains
remain blocking candidates. They cannot establish an inferred receiver by
being the only remaining signature. PathConstructor remains a static
constructor domain and is excluded from instance-method inference.

`Record.keys` and `Map.keys` both fit a `List[Str]` result and remain ambiguous.
A lone Int `.float()` requires a receiver annotation without choosing Int over
UInt. Explicit Record, Map, Int, and UInt controls check and execute unchanged.

## Distinguish genuine ambiguity

The initially supplied replacement-only reduction is not a valid positive.
`Str.replace(Str, with: Str)` and `Regex.replace(Str, with: Str)` both return
Str. Anchoring that result cannot distinguish their receivers. Both Str and
Regex caller controls now report `check.ambiguous-overload`.

The corrected positive uses `trim`: Str.trim returns Str and Bytes.trim
returns Bytes. A Str result anchor distinguishes those receivers inside the
body. The same proof covers two pending trim links, declaration reordering,
parameter renaming, and shared receiver constraints from replacement and trim.

The byte-access negative now explicitly expects `check.ambiguous-overload`.
Its earlier `check.result-fallback` expectation described an incidental error
after losing the receiver relationship. No error variant, parser rule,
Proc or Stream parameter requirement, or default Proc return rule changes.
The SPEC source and generated page document receiver-domain inference and its
diagnostic boundary.

## Candidate trials use the existing rollback journal

Candidate trials no longer clone the whole solver. The existing TypeConstraints
journal owns changes to touched cells, fresh type and row allocations,
provenance, levels, lacks sets, and journaled path compression. Shared resolution
does not perform unjournaled compression during a trial. An RAII owner restores
the trial on return or panic. An active per-constraint journal also restores
its changes if unification itself panics.

The private trial API returns only a Boolean match or a resolution category.
Conflict types carrying temporary inference identities are discarded before
rollback. A unique inferable winner is replayed with fresh owned identities.
Opaque identities are never reused. Work counters retain all trial work.
TypeConstraints remains Send and Sync. Foreign variables remain invariant
panics at the checker boundary. Depth exhaustion reports existing
`check.local-inference`; candidate pruning does not hide either category.

The deterministic work witness adds 0, 8, and 32 unrelated bound records with
64 fields each. For 1, 4, 16, and 64 touched cells, generated constraints,
unifications, occurs visits, and restored journal entries each equal the
number touched; row visits remain zero. Those counts do not change with the
unrelated records. The previous whole-solver copy would copy every solver
cell and each bound row's owned fields per candidate. The witness records
that former cell count separately from actual trial work. No timing or
overall scaling claim follows from this bounded test.

The precheck still clones a checker once per eligible body and other existing
probes remain. `function_body_check_entries` counts only final checker entries.
A focused count of two is not two total checks or one body traversal.
Phase E must measure that work after consolidation closes.

## Qualified sources and red checks

The lane starts at `1fccbae2284eff9086ae61c0a09216744f71f984`.

- `041e07527` corrects the initial invalid replacement positive to trim. Its
  exact Rust regression fails with `check.local-inference`; the annotated
  control passes first. The read-only qualified predecessor CLI pair also
  rejects the supplied-style trim reduction at `compact.indexed-build` for
  `line.split`. Inputs and raw results are retained in [red](red).
- `3e4280299` adds minimal failures for silent method-entry depth handling and
  false Record/Map narrowing. Both exact tests fail before their repair.
- `a972c8d139ecbb43174271842988eefb2c8638cc` is the qualified final compiler
  source, tree `6b329848a07a2e685c8f1a89797a3fa490ac6bbc`.

The [before](build-products.source.before) and
[after](build-products.source.after) seals match and contain no dirty paths.
The later proof and generated SPEC changes do not change compiler code.

## Focused checks and ordinary execution

[run-exact.sh](run-exact.sh) selects 36 Rust tests. Each result reports exactly
one pass and no failures or skips. The adjacent `*-final.log` files retain
those outputs. They cover facts and argument layouts, actual ambiguity,
receiver domains, error categories, trial cleanup, deterministic work,
frontend-disposed indexed execution on both invocation routes, previous late
anchors, registry parity, recursive independence from callers, and the current
generic-body restriction. [Compilation output](build-lib.cargo.stdout.jsonl)
and [compiler output](build-lib.cargo.stderr.log) retain the focused build.

[build-products.sh](build-products.sh) builds verification xsh and xsht in the
pinned Linux musl image, with four CPUs, 8 GiB, offline dependencies, kache,
mold, an exclusive target, and warnings denied. It builds no release product.
The [Cargo output](build-products.cargo.stdout.jsonl) and
[compiler output](build-products.cargo.stderr.log) retain the result.

The qualified pair is under this lane's
`.work/target-typing-host-obligations/x86_64-unknown-linux-musl/verification`:

```text
xsh  ee306b82df028f4b2d0bc83699bf41de27c5c7ac56862de10135461e6bbca154
xsht c7c12a7af5e26edee94ae12bba7196a9a3bffd6bbe90129d29a5a923b73e831c
```

[Build hashes](build-products.products.sha256) match
[post-proof hashes](products.after.sha256). The predecessor pair
[retains its hashes](predecessor-products.after.sha256). Product byte sizes
are recorded in [products.bytes](products.bytes); these are not release-size
gate results.

[run-cli.sh](run-cli.sh) runs inside the pinned image with `/products`, `/inputs`,
and `/scratch` mounted. All 12 positive scripts check and execute with status
zero. All nine negative scripts check and execute with status two and their
specified diagnostic codes. [Status output](ordinary-cli.status),
[inputs](inputs), and [raw output](ordinary-cli) retain the proof.

The qualified pair runs the existing [docs generator](render-docs.xsh).
[Its output](docs.stdout) records only `docs/SPEC.md` changing; its
[stderr](docs.stderr) is empty.

## Pending gates and limits

The all-unifier journal change warrants the targeted complete Rust solver
module. Discovery in [constraints-module.list](constraints-module.list)
selects 52 tests and zero benchmarks with `sema::constraints::tests::`.
The integrator authorized one invocation on the unchanged final compiler.
[The result](constraints-module.log) is 52 passed, zero failed, and zero ignored.
The [command](constraints-module.command), source
[before](constraints-module.source.before) and
[after](constraints-module.source.after) seals, and harness hashes
[before](constraints-module.harness.before.sha256) and
[after](constraints-module.harness.after.sha256) retain that qualification.
The harness is reused without a Cargo rebuild.

The six new native cases in `tests/xsh/typing-capability-witnesses.xsh` remain
queued behind the consolidation closure queue. Each exact selector must run
one test:

- `test_typing_host_result_anchor_and_nested_loops`
- `test_typing_host_constraints_follow_chains_and_body_order`
- `test_typing_host_replacement_result_keeps_receiver_ambiguity`
- `test_typing_host_byte_receiver_keeps_ambiguity_after_bytes_caller`
- `test_typing_host_inference_preserves_argument_and_effect_errors`
- `test_typing_host_untemplated_receiver_domains_remain_candidates`

The broader `items.push(item(row.value))` body-coverage gap remains named in
[the negative input](inputs/generic-row-body-gap.xsh). The ordinary checker
continues to reject it at the existing generic-body restriction. This repair
does not bypass verification or widen executable generic-body shapes.

Production Rust changes add 361 lines and remove 33, net 328, excluding test
modules. No dependencies, CI, formatting, full native suite, old 907 or 53
rerun, release, performance gate, complete Phase C/D/E acceptance, or main
promotion is claimed.
