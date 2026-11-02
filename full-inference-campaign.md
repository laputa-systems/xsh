# Close the remaining inference gaps

**Status:** active correctness work. Full inference qualification remains open.
All performance work and readiness checks are deferred.

The [completion contract and evidence](docs/PERFORMANCE-INFERENCE.md) owns the
lasting requirements, capability matrix, corpus accounting, and deferred
performance budgets. The [SPEC](docs/SPEC.md) owns language semantics.
[Architecture](docs/ARCHITECTURE.md) owns representation and verification
invariants. Earlier implementation history stays in Git.

## Active work

| Item | Current evidence | Close when |
|---|---|---|
| Guard composition | Closed on `ef2bf422d`: guarded values reach typed and untyped locals and returns (`e2c2e4736`), inferred `if let` payloads live under a non-null guard, and the `cpio.validate_options` erasure checks. The rewrite passes all 427 `full-inference` native tests, the 1,639 `runtime::eval` verifier tests and the `test-soundness` scenario. Inferred `while let` still reaches `guard_source_duplicated`; baseline rejects that fixture too. | Keep the forged-guard, capture and single-evaluation checks green; reopen for `while let` when a source view needs it. |
| Deferred domains | The named cases have landed: qualified error-family matches (`5c851b4fc`), target-typed enum variants (`0aa92f1ef`, `98423dbc0`, `d71d0f84a`), conversions through indexed iteration (`33322c4b6`) and through slices of an indexed list (`2cd7cedbe`), each with paired annotated and inferred native tests. Both execution routes are exercised only where Rust fixtures call `execute_both`. | Replay the corpus to find any remaining domain gap before reopening; keep genuine ambiguity, such as an unanchored slice conversion, rejected. |
| Corpus residuals | On the `51084cb9a` sources the private core parameter view (250 files, 4,677 sites, Btop excluded) passes 4,654 sites alone (4,651 on the same sources before the closure lane), up from 4,587 of 4,680; five per-file joint views fail with every passing site erased (`env`, `iw`, `lsipc`, `mdev`, `pstree`), and each passes once the one annotation named below is kept. The inventory was regenerated from a clean `51084cb9a` worktree after the syntax removals that rejected bare paths, ordering chains and the `cd`/`env` command spellings, so earlier site numbers do not carry over. Recurring classes: effect contracts that cannot bind to the checked declaration (4 single, joint `date`, `dd`, `setpriv`), generic record constructors that disagree with their producer (joint `curl`, `sed`, `wget`), and the closure replay below. Btop's 39 sites remain unmeasured. The frozen 1,040-removal baseline remains separate. | Reduce genuine inference failures, justify retained source contracts, and close joint views without changing historical denominators. Closure replay: the argument, assignment, match-arm, key-validation and comprehension repairs raise the single-site count to 4,636 and close the ten no-closed-type singles and two of four `RecoveryType` singles. Closure lane: a generic proc completion branch may be empty (`arp` `set_entry`, `route` `send_route`); a fallback handler may end in `break` or `continue`, which the generic verifier now accepts as a diverging value block after checking the statement against its lexical loop (`cpio` `copy_out`); `spawn run $word ?` in a generic body authenticates its argument words by the scheme conversion requirement instead of failing the ground producer proof (`xargs` `start`); a null-tested field read through a guarded view counts as a proven non-null fallback receiver (`ss`, `mdev` `rule_matches.event`); a union expectation unifies with the one member that shares the actual's constructor (`mdev` `run_rule_command`); an error variant test on an unannotated subject (`if let CopyError.Invalid {message} = failure`, `failure is CopyError.Invalid`) infers `Error` (`cp` `report.failure`). Every function blocker now carries a span and detail. Retained: `pstree` `print_user_roots.name` with `has_same_named_user_parent.name` (erased together the callee stays generic over an equality with `Str` and a generic body cannot capture the top-level `List` of process records; keep one annotation); `mdev` keeps the `apply_rule` `Captures` annotation because the `values: []` argument in `handle_event_rules` has no other anchor; with it kept, the joint `mdev` view passes. Closure lane 2 closes `unshare:41`, `env:174` and `lsipc:218`, and the `mdev` lowering gap: a whole-record read under a field guard (`if let file = entry.file { entry.flag }`) verifies against the slot record with the guarded field replaced by the guard view, through the same non-null proof a field read uses; a registered `Path.strip_prefix` plan is admitted in a generic body for that one signature; a bare local ending a `Result[Unit]` body records its tail type; a proc checked twice unifies the method family it registered the first time (`environment.get(name)` over an unresolved receiver); a map key operation whose key a later statement fixes to `Str` needs no validation wrapper; a null-guarded immutable local reads inside a loop; and a typed callable flows through a `try` capture. Each has paired inferred and annotated native tests, a rejection control, and forged-IR tests where the verifier accepts something new. A generic `item.mark != null` or `item.mark ?? d` over a plain `Int` field is not a gap: it is the specified optional injection and matches the `Int?` field twin exactly, while the `Int` annotation is the stricter contract (`test_full_inference_null_test_over_a_plain_field_is_the_optional_injection_twin`). Retained: `env` keeps `byte_prefix.arg` or `main.raw` (nothing but a generic slice and equality with byte literals reaches the rest list's element type, and equality does not choose a type); `lsipc` keeps any one of `make_sysv_entry.record`, `make_sysv_entry.selection`, `posix_entry.selection`, `posix_entries.selection`, `row_for.selection`, `selected_rows.selection` (erased together, `main` fails callable generalization with a recovery type); `date:8` (a `null` default establishes no parameter type). Open with leads: `env:300` `run_command.cwd` and `.argv0` (a tail `env (...) { match ... return / exit }` whose block never completes types the scope `Result[Unknown, Error]` and the match tail `Unknown`; generalization rejects both, so a never-completing context scope and a diverging tail match need the non-completing treatment resource scopes have in checker, lowering and verifier); `getty:113` (the join of `{}` and `{TERM: options.term}` is the first branch's exact empty record in the ground checker, which accepts the wider record, and the generic verifier rejects it: a language decision on record branch joins); `dd:342` (reduces to `let x: Path? = if let f = target { f } else if c { null } else { null }`: the pattern branch value has no assignment requirement against the annotation once an `else if` follows); `find:836` `deferred` and `split:906` `name` (`x as Path` over an unresolved operand has two table sources, `Str` and `Bytes`, and no generic conversion requirement is recorded); verifier or lowering classes not yet reduced: `ls:3290` (generic literal pattern), `wget:704` (generic interpolation producer cannot be displayed), `comm:48` (generic argument temporary not bound on a unique path), `getfacl:9` (outside the supported parameter projection path), `nc:560` (no supported runtime representation), `expr:547` (ground producer form has no executable type proof); check failures not yet reduced: `namei:205` (storage requires Record, found Record), `split:107` (inferred types disagree); `smartctl:447` and `:468` are the retained `Any` payload contract below. Retained: `smartctl` `finish.session` (an `Any` payload leaves its empty list unanchored) and the `iw` pair, now a check diagnostic. Recursion: a `null` argument to an unresolved shared parameter of a recursive call now makes it optional (`mkdir` `create_directory.mode`). Guard views: a guarded source read stored into an optional slot keeps its payload type (`syslogd` `check_sink.previous`, `udevadm`). Recursive guards: members of a recursive component now share guard identity (each member lists the component's guards in one order, peer rows are transported, and Full authenticates every peer binding by row correspondence under the call substitution); self-recursive guards keep guard-local view binders, and self calls forward callee guard calls. Joint `date` now passes (47 sites). Joint `du` and `prlimit` pass and `mdev` `build_alias` erases: an ordering, negation, sort or count key inside an `if let` body is recorded against the guard that owns its view binder, not the declaration (`generic pipeline key lacks its scoped canonical domain predicate`). Nested `if let` on independent sources now lowers with its guard membership and runs in self and mutual recursion and without recursion (the tail-conditional path had recorded none, so Full rejected the IR as an internal `compact.indexed-build` error); forged-IR tests pin each correspondence comparison and the runtime transport rejections. Retained: a peer call that would forward a guard call (the checker rejects it with a diagnostic); no accepted program yields a method or wrapper requirement in a guard row, so their forwarding has no forged-IR fixture. Null patterns on an unresolved generic subject now verify: an unused `if let` binding (its guard is pruned from the scheme, so the binding has no view) and a `match` with a `null` arm run in Full and match their `T?` annotated twins, and the generic verifier accepts a `null` literal against a scheme parameter or optional subject. Any other literal against a parameter subject still fails (`generic literal pattern has no ground subject`), which the checker never produces because a non-null literal fixes its subject type; forged-IR tests swap integer and boolean literals into both shapes. Joint `ls` now passes: the verifier validated a call's substitution once per instantiated type, so a callee with thousands of binders (`list_all`, 2,466) cost its binder count per operation and exhausted a bound that scales with the program; it validates once per call, and the wide-binder test pins that. |
| Practical source views | On `6a747c7b5` the applied view (405 private parameter sites in wc, sort, awk and Laputa `pm/generation.xsh`) passes every site alone and all four joint views, after guard calls with a known source started forwarding through generic callers. Joint-erased `sort.xsh` produces identical output to the annotated source and costs about 2.3 times its CPU on the faster sort build; the slow path is not the one the sort fixes removed. | Compare joint behavior and interfaces for wc, awk and `pm/generation.xsh`, rerun the final host-consumer guest drill, and find the erased-source CPU gap. Retain exported `write_generation_plan.path_value`. |
| Integrated acceptance | `ef2bf422d` passes all 427 `full-inference` native tests on the host (not yet replayed in the pinned container) and the 1,639 `runtime::eval` verifier tests. Debug-only type agreement remains open. | Run affected verification owners, soundness families and final corpus views on the integrated compiler; remove superseded inference machinery. |

## Constraints

Give each worktree its own Cargo target directory and freeze accepted products
before replay. Prepare independent repairs in isolated worktrees and integrate
each checker, lowering, and verifier slice
together. Keep independent producer, effect, ownership, and forged-IR checks.

Preserve rank-1 inference, monomorphic recursive components, container
invariance, nominal schemas, callable kinds, and explicit public contracts.
Never use `Any`, recovery types, caller training, runtime inference, or relaxed
verification to make a source view pass.

Keep intentional annotations separate from implementation gaps. Private proc
parameters are eligible for the applied view. Exported parameters, private
proc returns, effect clauses, and Map/require locals remain retained there.
The exported Laputa `write_generation_plan.path_value` contract remains.
The eight core assign-target annotations pass individually; their joint source
view still needs replay.
Do not change historical cohort denominators.

Keep inference changes in XSH and isolated Laputa source views. Registry and
h12tiny migration is integrated. The separately authorized
[Asterinas campaign](../laputa/docs/CAMPAIGN-ASTERINAS-COMPATIBILITY.md) owns its
local kernel execution boundary. Do not change live systems, CI, toolchain pins,
or releases. Git pushes remain unauthorized. New syntax, production dependencies, persistent cache
formats, higher-rank inference, user traits, new runtimes, and Lean expansion
remain excluded.

## Acceptance loop

Read `AGENTS.md`, the relevant code and tests, and the owning SPEC section.
Reproduce each gap in a native test before repairing its authority boundary.

Use current prepared verification products for the exact regression first:

```sh
"$XSHT_BIN" test --exact "$SELECTOR"
```

Run a build only when its code inputs changed. Observe successful completion
before launching dependent checks. Keep optimized milestone builds out of the
edit loop and avoid queuing test builds behind an unnecessary product rebuild.
Debug Rust units may cover non-process boundaries. Native and process tests
require verification or release products in the pinned Linux environment.

Inspect the affected gate selection and run its required checks:

```sh
cargo dev gate --plan PATH...
cargo dev gate PATH...
```

Add exact inference-native selectors when path ownership does not select them.
Require the intended nonempty set and zero unexplained skips. Reuse evidence
only while its code, products, dependencies, environment, and scope still match.
Group wider checks at coherent integration milestones. Full runs remain behind
the explicit request boundary documented in [Testing](docs/TESTING.md).

Use the existing `inventory.sh`, `erase_sites.py`, and paired corpus adapters
from [Inference evidence](docs/PERFORMANCE-INFERENCE.md) for annotation claims.
Keep scratch under ignored `.work/`. Commit each accepted product slice.
Rewrite this active status after a landing and keep the implementation log in
Git. Do not restart completed or parked campaigns to increase lane count.
