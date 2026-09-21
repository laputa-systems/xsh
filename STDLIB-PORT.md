# Standard-library self-hosting ledger

Migration ledger for the Rust-to-XSH standard-library port specified in
`xsh-rust-to-xsh-port-prompt-v2.md`. This file tracks dispositions, destinations,
remaining native boundaries, and verification. It is not a second language
specification: lasting architecture belongs in `docs/ARCHITECTURE.md`.

## Starting revision

- Revision: `37e1502` (branch `master`), clean working tree at start.
- Source-audit baseline named by the specification: `a2f3fd4` (an ancestor of the
  working revision).
- Verification host: `aarch64-apple-darwin`. Debug builds for ordinary
  verification; release builds for performance comparison only.
- Linux host: the `Dockerfile.test` image (`xsh-test`), driven by
  `xsh dev internal test-linux` / `test-linux-ci`. Every Linux result in this
  ledger comes from that environment and no other; see `## Linux verification`.

## Architecture

Public standard-module entries are declared once in `crates/xsh-registry`. An
entry/overload carries an `ImplBinding`:

- `Native` — the existing `RuntimeOp` body (default).
- `Script { module, function }` — an embedded XSH implementation.

Embedded sources live under `stdlib/` and are embedded through the compile-time
catalog in `src/stdlib.rs`. `include_str!` embeds each file and makes Cargo
rebuild tracking cover it; the catalog never scans a directory or reads the
environment at run time.

Preparation (`src/loader.rs`):

1. Parse the entry and its statically loaded user-module graph.
2. `stdlib::required_modules` scans the parsed arena for public spellings that
   script-backed entries own and selects the embedded modules to prepare. A
   referenced user-code loading route (`module.load`) selects the complete set.
3. Each selected embedded module is parsed at most once, attached to the same
   arena as an *internal* module, and checked and lowered with the program.
4. Lowering (`lower_script_module_call` / `lower_script_method_call`) binds a
   script-backed public call to the prepared implementation function and emits an
   ordinary `Call`; execution uses the normal frame engine.

Two non-runner preparation paths had to be routed through the same loader:

- `crates/xshi/src/interactive/app.rs` had its own per-input parse, so a
  migrated call submitted at the prompt failed to lower. Each submitted input
  now crosses this preparation boundary through the loader.
- `src/loader.rs::parse_load_entry_source_shared_arena_only`, used by `xsht
  check` and `xsht lint`, did not attach embedded modules, so a script-backed
  call looked unlowerable to the checker while running fine. That path now
  attaches them too, which is what `xsht check`/`lint`/`fmt` need to agree with
  the runner.

Catalog-owned sources are routed away from the user corpus in
`xsht-config.ini`: `stdlib/**/*.xsh` is excluded because those files are
implementations, not user programs, and their helper bindings deliberately
shadow standard module names — a user-source rule that does not apply to an
implementation. Their equivalent coverage is the dedicated catalog gate, which
runs every embedded source through preparation whether or not anything
references it. `bench/**/*.xsh` is excluded as benchmark host tooling.

Namespace integrity: internal modules use the reserved namespace
`<xsh-stdlib:IDENTITY>`, a spelling no XSH identifier can produce, so user
source, `use` paths, module search roots, and dynamic modules cannot name them.
Their helpers are excluded from the unqualified declaration tables, from the
global top-level name set, and from user-module collection.

## Dispositions

| Group | Status | Public entries | Destination | Native removed |
| --- | --- | --- | --- | --- |
| R01 shell quoting | ported | `shlex.quote`, `shlex.join` | `stdlib/shlex.xsh` | `src/modules/shlex.rs` deleted; dispatch arms removed |
| R02 CLI policy | ported | `cli.*` | `stdlib/cli.xsh` | the argument parser, token splitter, usage renderer, and command router removed from `src/modules/cli.rs` (2,263 lines); the file is deleted and its dispatch arms and lowering special cases removed. Contextual command defaults come from the invocation-context bridge |
| R03 argument-word parser | ported | `process.argv_words` | `stdlib/process.xsh` | `argv_words`, `ArgvWordsParser` with its methods, `shell_syntax_char`, and their unit tests removed from `src/modules/process.rs`; the dispatch arm and its `lowered_module_op_supported` entry removed. Process creation, inspection, and the rest of `process.rs` stay native |
| R04 MIME table and parser | ported | `mime.lookup_ext`, `mime.lookup_path`, `mime.parse` | `stdlib/mime.xsh` | `src/modules/mime.rs` deleted (283 lines); three dispatch arms, their `lowered_module_op_supported` entries, and the two lowering special cases removed |
| R05 TUI formatting | ported | `tui.*` except `read_secret` | `stdlib/tui.xsh` | `sequence`, `pad`, `left_pad`, `right_pad`, `visible_width`, and the `Sequence` enum removed from `src/modules/tui.rs`; the nineteen `Tui*` dispatch arms removed. `read_secret` stays native |
| R06 numeric presentation | ported | `bytes.human`, `time.duration_compact` | `stdlib/bytes.xsh`, `stdlib/time.xsh` | bodies and dispatch arms removed |
| R07 string policy | ported | `Str.wrap`, `Str.fields` | `stdlib/text.xsh` | `fields_text`, `wrap_text`, `wrap_line`, `wrap_word` removed from `src/modules/text.rs`; the `fields`/`wrap` arms removed from `lowered_ops.rs`. Primitive splitting, searching, replacement, case conversion, translation, byte views, and numeric parsing stay native |
| R08 checksum-line parsing | ported | `hash.parse_check_line` | `stdlib/hash.xsh` | `parse_check_line` and `CheckLine` removed from `src/modules/hash.rs`; the `HashParseCheckLine` dispatch arm removed. Digests, encoders, and file hashing stay native |
| R09 INI encoding | ported | `ini.encode`, `ini.write` | `stdlib/ini.xsh` | `encode` and the encoder-only `write_key_value` removed from `src/modules/ini.rs`; the `IniEncode`/`IniWrite` dispatch arms removed. The single shared decoder and the `validate_key`/`validate_section` pair it genuinely shares stay native. `ini.write` is composition (G03) ported because it was the last caller of the native encoder |
| R10 JSON path policy | ported | `json.get` (both overloads), `json.set`, `json.remove`, `json.encode_lines` | `stdlib/json.xsh` | `json_path_get`, `json_path_set`, `json_path_remove`, `JsonPathSegment`, `json_path_segments`, `set_at_path`, `remove_at_path` removed from `src/modules/json.rs`; the `JsonGet`, `JsonSet`, `JsonRemove`, `JsonEncodeLines` dispatch arms removed. JSON codecs, number restrictions, runtime-value conversions, pretty formatting, schema checks, streaming decode, and the file wrappers stay native |
| R11 environment convenience | ported | `env.get_or`, `env.bool`, `env.int` | `stdlib/env.xsh` | the `EnvGetOr`, `EnvBool`, and `EnvInt` dispatch arms and their three `lowered_module_op_supported` entries removed. Raw acquisition, `env.path`, enumeration, path-list mutation, and scoped-environment installation stay native |
| R12 Linux text policy | ported (Linux), native on macOS; **fails its gate, 3×-98×** | Linux `system.os_release`, `system.memory`, `linux.meminfo`, `linux.modules`, `unix.uptime_seconds` | `stdlib/system.xsh`, `stdlib/linux_text.xsh`, `stdlib/unix.xsh` | target-aware: the Linux binding is `script_sig`, macOS keeps `sig`. The macOS bodies are live and stay; the Linux-only bodies were deleted in the follow-up phase, and the invariant that makes them unreachable is asserted by `linux_text_entries_bind_the_embedded_implementations` (see `### Superseded Linux bodies removed, and the route that replaces them pinned`). Measured per entry in `## Performance`; it stays ported because §12.3 keeps a mandatory group incomplete rather than reverting it to native |
| G01 git-root discovery | retained-boundary | `fs.gitroot` | — | `xsh::host::fs::gitroot` is a public Rust façade helper consumed outside language execution by `crates/xshi/src/interactive/session.rs:328,416` for the git prompt. §6's own first check applies: keep one shared native implementation rather than adding callbacks or a duplicate script algorithm |
| G02 file-checksum policy | ported | `hash.verify_file` | `stdlib/hash.xsh` | `verify_hex`, `validate_expected_hex`, the `HashVerifyFile` build row, the `ExprHashVerifyFile` tag and its decoder, and both native dispatch arms removed. What stays native is digest acquisition and representation (`hash.md5`/`sha1`/`sha256`/`sha512` and `Digest`), which the implementation selects by the algorithm name. The algorithm travels as a third argument, because the public form carries it in the checksum argument's *name* and a name is not a value |
| G03 JSON/INI file-IO composition | retained-boundary (INI write ported) | `ini.write`, `json.read`, `json.write`, `json.write_lines` | `stdlib/ini.xsh` | the INI write composition moved with R09. The JSON file wrappers are the smallest useful host adapters: each is an open/read/decode or encode/open/write sequence whose only non-host step is a single call into the retained codec, so a script shim would add an interpreter boundary and an equal-sized Rust adapter instead of deleting policy |
| G04 read-only route interpretation | retained-performance | `linux.routes` | — | ported, measured, and removed under §12.3: the embedded parser measured 11.9 ms per call against 0.02 ms across a real 6-row route table, 2379 ms against a 2 ms budget over 200 calls, and 15 ms against 5 ms for a single call in a fresh process. The native parser is restored and the prototype is deleted; `## Performance` records the measurement |
| G05 rfkill inventory | retained-performance | `linux.rfkill_list` | — | ported and removed with the block-device inventory beside it, which is the same code shape and measured 120 ms against a 2 ms budget. The implementation reads one attribute per device through the interpreter; it was not timed on its own before the group was settled, which is recorded as a limit of this evidence. The native body is restored |
| G05 block-device inventory | retained-performance | `linux.block_devices` | — | ported, measured, and removed under §12.3: 125 ms against 5 ms over 200 collections of a 3-device tree, a +120 ms delta against a 2 ms budget. The native body is restored and the prototype deleted |
| G05 interface inventory | retained-boundary | `linux.interfaces` | — | the record's `addresses` field comes from `libc::getifaddrs` (`src/modules/linux/real/net.rs:824`) and its `mtu` prefers the `SIOCGIFMTU` ioctl. Neither is reachable from script code and no public entry exposes interface addresses, so a port would have to add two acquisition bridges and a new record shape to move ~70 lines that sit beside them. §7 keeps platform address acquisition native |
| G06 read-only disk-usage presentation | retained-boundary | `linux.disk_usage` | — | the numbers the entry reports are not reachable from script code. `disk_usage_record` reads `f_bsize`, `f_blocks`, `f_bfree`, and `f_bavail` through `fs_module::statvfs` and reports bytes (`blocks * f_bsize`, saturating); the nearest public surfaces are lossy in two ways at once — `fs.filesystem_stats`, `fs.mount_for`, and `fs.mounts` use `f_frsize` falling back to `f_bsize` and truncate to 1K units, so `blocks_1k * 1024` is not `blocks * f_bsize`. A faithful port therefore needs a new private statvfs bridge, i.e. new Rust to move ~40 lines of policy that sit directly beside it, which the mandate's net-reduction objective does not support. the exact call graph is above. The design is available if the owner wants it |
| G07 kernel-module query and index output | retained-performance | `linux.modinfo`, `linux.depmod` | — | ported, measured, and removed under §12.3: `modinfo` measured +69 ms and `depmod` +120 ms over 200 calls against a 2 ms budget, roughly twice the native cost per call. The acquisition bridges, the embedded module, and the Linux policy's deletion are all reverted with it |

A placeholder source is a valid embedded module with no exports: while a port
is in progress its entries keep their native route, so the tree stays green.
Removing the native body is what makes the implementation binding
authoritative. No catalog module is a placeholder now: each of the fifteen
backs at least one public entry.

## Private bridge descriptors

Each new private operation, its exact inputs and outputs, its effect and
context behavior, the implementation modules permitted to use it, why an
existing operation was insufficient, its tests, and the native code it retains.

| Descriptor | Inputs → output | Effects / context | Permitted owners | Why an existing operation was insufficient | Tests | Native code retained |
| --- | --- | --- | --- | --- | --- | --- |
| `RecordWithField` (`RecordWithField`) | `(Record-family, Str, Any)` → the same record family | none; pure value construction, no context read | `<xsh-stdlib:cli>`, `<xsh-stdlib:json>` | `Record` exposes only `get`/`has`/`keys`. A record with a schema-driven field name cannot be built by a literal, and `json.decode` would be a JSON round trip, which the specification forbids | `stdlib/json.xsh`, `stdlib/cli.xsh` through their public entries; `tests/xsh/stdlib/json.xsh`, `tests/stdlib_port.rs` | none — the operation is new; it constructs the existing `Record` runtime representation |
| `RecordRemoveField` (`RecordRemoveField`) | `(Record-family, Str)` → the same record family | none; pure value construction | `<xsh-stdlib:json>` | same gap on the removal side | `tests/xsh/stdlib/json.xsh` | none |
| `BridgeCommandName` (`BridgeCommandName`) | `()` → `Str` | reads the evaluator's current invocation name; no host state, no ambient snapshot | `<xsh-stdlib:cli>` | The baseline's `cli.parse`, `cli.parse_full`, and `cli.applet` default their `command` parameter to the name the interpreter was invoked as. No XSH-visible source provides it, and the permitted "invocation context" category covers exactly this: supply the existing contextual command-name information at a public call boundary. It is read at each invocation, never captured at preparation | `tests/xsh/stdlib/cli.xsh`, `tests/xsh/stdlib/args.xsh`; the ownership rule is covered by `user_functions_cannot_impersonate_a_representation_bridge` | none |
| `BridgeTypeName` (`BridgeTypeName`) | `(Any)` → `Str` | none | `<xsh-stdlib:cli>`, `<xsh-stdlib:json>` | The baseline's error messages name the type they found (`expected object at key \`k\`, found {type}`). XSH has no type-name primitive, so a faithful port of the error payload needs one | `tests/xsh/stdlib/json.xsh`, `tests/xsh/stdlib/cli.xsh` | none |
| `append_bytes` (`BridgeAppendBytes`) | `(Path, Bytes)` → `Result[Unit]` | performs file I/O at the call: creates missing parents, opens the path create-or-append, writes every byte, and reports the failure as the call's `Err`; reads no ambient context | `<xsh-stdlib:linux_text>` | The ported dry-run log must append in place, the way the baseline's open file does; XSH's `fs.write` truncates, and its other file entries have no append mode. Read-concatenate-rewrite was the workaround, and it is observably wrong (non-UTF-8 bytes replaced, an unreadable-but-writable log truncated, concurrent writers losing each other's lines) | `tests/xsh/stdlib/linux.xsh::test_linux_dry_run_log_appends_in_place`; `src/modules/fs.rs::append_bytes_preserves_existing_bytes_and_appends_in_place` | the host half is `src/modules/fs.rs::append_bytes` — the same create-parents/open/write-all/transport-error shape the native dry-run path used; the logging decisions, line construction, and error kind stay in XSH |

A record reaches the runtime in more than one shape — a statically shaped
literal is a `RecordVec`, a dynamic one is a `Record`, an imported module is a
`Module`, and a filesystem entry materializes as a record — so the bridges
update whichever shape they are given and return that same shape. They never
convert a caller's container, which would change the representation a later
read observes.

Access is narrow at both ends. The lowering rewrites a call only when the
enclosing function's namespace is the very module that declares the bridge, and
an internal namespace is a spelling no XSH identifier can produce. The whole
store verifier then re-checks every `ExprModuleCall` that carries a private
operation: the instruction must sit inside a function whose recorded owner is an
internal namespace whose catalog entry declares that operation, and an
instruction outside every function (a driver statement) may never carry one.
The owner recorded on the function comes from the checked implementation
identity, so a forged callsite span or display label does not help. A user
function that happens to be named `record_with_field` or `type_name` stays an
ordinary user function, which
`user_functions_cannot_impersonate_a_representation_bridge` asserts.

**The follow-up phase added one descriptor** — `append_bytes`, whose row is
above: it is reachable only from `<xsh-stdlib:linux_text>` under the same
lowering rewrite and store-verifier checks as the four before it, and it is
declared by that module's catalog entry. Everything else the phase introduced is
internal to the runtime and has no XSH-visible spelling, so no owner list, effect
row, or privilege boundary changed. Those mechanisms, with their owners:

| Mechanism | Owner | New reach from XSH source |
| --- | --- | --- |
| Suspended producer frame — `ScriptProducer`, `ProducerFrameState`, `FrameWork::ForStream`, `FrameContinuation::Yield`, `FullTag::StmtYield`, `ScriptStreamState`/`StreamValue.script` | `src/runtime/eval/lowered_run/indexed_run/producer.rs`, `.../explicit_run.rs`, `src/runtime/value.rs` | none — it is the existing `yield`/`stream` contract run as a continuation; the body is the program's own checked body |
| Bounded frame scratch pools — `FrameScratch`, `recycle` | `.../indexed_run/explicit_run.rs` | none — allocation reuse; the values crossing a frame boundary are unchanged |
| Per-program function-header cache — `FullProgram::headers`, `header()`/`decode_header()` | `src/runtime/eval/indexed/full.rs` | none — the same decoded header the call path already used, retained with its owning program instead of re-decoded per call; nothing is reachable that was not |
| Lazy traceback names — `TracebackName` | `src/trace.rs` | none — traceback and trace-event rendering only |
| Shared container backing and the consuming call — `Arc` payloads, borrowed read-only receivers, the proven last-use transfer | `src/runtime/eval.rs`, `src/runtime/eval/lowered_run.rs`, `src/runtime/value.rs` | none — the same values and the same result, with the copies removed |
| Test-only catalog probe — `probe_embedded_call`, `probe_embedded_pure_call` | `src/runtime/eval.rs`, gated `#[cfg(test)]` | none — absent from every product build |

## Architecture tests

`tests/stdlib_port.rs` holds the boundary tests; `src/stdlib.rs` holds the
catalog tests.

| ID | Evidence |
| --- | --- |
| A01 | `preparation_is_proportional_to_referenced_standard_entries` — a trivial program prepares zero embedded modules; one script-backed call prepares one; an unrelated entry prepares none; `selection_ignores_names_that_are_not_script_backed_references` — a local binding and a record field named `load`, a bare `use module`, and a native entry of a mixed module (`env.get` beside script-backed `env.get_or`) each prepare nothing, while the script-backed entry beside them selects its module |
| A02 | `execution_does_not_prepare_embedded_modules` — 200 calls in a loop and a failing run each leave the preparation count at one |
| A03 | `repeated_references_parse_an_embedded_module_once` — two entries of one module, and a reference reached through a loaded user module, parse it once |
| A05 | `dynamic_loading_prepares_the_complete_set_before_execution` — a `module.load` reference prepares the whole applicable set before execution and prepares nothing more during the run; `a_resolved_dynamic_loading_route_prepares_the_complete_set` — the same through a `use module as …` alias, the route a tightened selector could otherwise miss; `a_loaded_module_calls_prepared_implementations` — a loaded module's standard calls reach those prepared implementations |
| A06 | `copied_binary_runs_migrated_apis_without_repository_files` — a copied binary runs migrated APIs from a clean cwd |
| A07 | `standard_implementations_cannot_be_replaced` — hostile module roots and a module named after a standard module cannot replace an implementation, while an explicitly loaded hostile module keeps ordinary user semantics |
| A08 | `copied_embedded_source_grants_no_private_access` — a user file whose contents are a copy of an embedded module gains no private access |
| A09 | `private_implementation_helpers_are_not_nameable` — a user reference to an embedded private helper prepares nothing and fails; `user_functions_cannot_impersonate_a_representation_bridge` — a user function spelled like a bridge stays an ordinary user function |
| A10 | `same_spelled_user_helpers_cannot_capture_implementation_helpers` — user declarations spelled like embedded helpers stay user bindings |
| A04 | `crates/xshi/tests/stdlib_preparation.rs` — each submitted input crosses its own preparation boundary in one process, repeated submissions keep working, and a failed submission does not poison the next |
| A11, A12 | `implementation_namespace_is_unspellable_and_reserved_names_still_work` — the internal namespace is not parseable, standard module names stay reserved, and the documented `error` binding exception still works |
| A13 | `prepared_implementations_read_context_at_invocation_time` — a scoped environment overlay is observed at each call, never snapshotted at preparation |
| A14 | `pure_user_functions_still_cannot_perform_io` — purity and effect checking stays on for user and embedded bodies; only the documented legacy CLI probes keep their exception |
| A15 | `module_dependencies_resolve_and_cycles_are_diagnosed` — module-level dependencies cross script/native mixed modules and an import cycle is diagnosed |
| A18 | `every_catalog_module_parses_checks_and_lowers` — every embedded source, used or not, runs the full production preparation gate |
| A19 | `tests/symbol_plateau.rs::repeated_preparation_returns_to_a_stable_symbol_plateau` — independent programs prepare, run, and teardown without accumulating symbol ownership, in its own test binary because the counter is process-global |
| A20 | `implementation_namespaces_never_appear_in_the_public_registry` — no internal namespace or label is a public module, function, method, or record name |

| A16 | `crates/xsht/tests/api.rs::api_surface_matches_the_recorded_reference` — the complete public surface, recorded from the reference build as `tests/fixtures/modules/standard-api-surface.jsonl` and compared byte for byte; the per-group parity suites cover the behavior behind it |

| A17 | `crates/xsht/tests/profile_parity.rs::one_case_set_behaves_identically_across_supported_builds` — newlines, non-ASCII text and escapes, path bytes, and composed error messages run through every `xsh` binary the tree has built: debug, release, and `--no-default-features`, with stdout, stderr, and exit status compared byte for byte. A missing binary is reported as a skip rather than passing quietly; the header records the two build commands |

Every architecture test above has dedicated evidence. A17's *platform*
dimension is the container runs recorded under `## Linux verification`; its
case, profile, and feature dimensions are the one test above.

## Final status

Incomplete. All twelve required groups are ported; the gated groups resolve as
one `ported` (G02), four `retained-boundary`, and three `retained-performance`
that were measured, failed, and reverted. R02 (CLI policy) and R10
(JSON path policy) use the four private representation bridges —
`RecordWithField` and `BridgeTypeName` in both, `BridgeCommandName` in the CLI
and `RecordRemoveField` in JSON — that the CLI's schema-shaped record
construction and the JSON `set`/`remove` paths need, and their native owners
are deleted. R12 (Linux text policy) owns the fifth, `append_bytes`, which the
follow-up added so its dry-run log appends in place; R12 executes on Linux and
is verified there;
macOS runs the retained native bindings by design, so its macOS bodies stay and
its Linux-only bodies are gone.

§10's completion rule is "Do not claim the overall migration is complete until
its required behavior, architecture, test, and cumulative performance gates
pass. If one remains red, leave the ledger explicitly incomplete and report the
exact measured blocker and smallest remaining change." Of those four gate
families, three pass on the final tree and are recorded above and below:

- **Behavior** — every §3 repair is implemented and pinned by a test: the
  producer contract at its boundaries, the worker paths' producer semantics,
  the nested-shadowing miscompilation, genuine append, the container-copy and
  consuming-call paths, the host-text fixture boundaries (helper level and the
  public entry's fixed-path two-path read), and the six defects the unblocked
  suite found.
- **Architecture** — the dispositions, the private-descriptor tables with their
  owners, the catalog/binding invariants, the preparation-selection tests, and
  the retired native bodies' unreachability are all recorded and asserted; no
  public namespace, effect, or privilege boundary was broadened and no
  dependency, switch, or workflow was added.
- **Tests** — every applicable test executes; the counts and every pre-existing
  or environment-caused failure are in `## Pre-existing failures and skips` and
  `## Linux verification`.

The **cumulative performance gate** is the family that remains red, and it is
the reason this ledger says Incomplete rather than Complete:

What is not finished:

- **Performance acceptance fails.** Eighteen of the twenty-four designated
  workloads exceed their gate, spanning the cold-start, CLI, text, quoting,
  MIME, INI, JSON, environment, checksum, and tooling classes, and R12's five
  required Linux entries fail a separately measured gate by 3× to 98×; see
  `## Performance` for the per-row numbers and the causes. §12.3's rule for a
  mandatory group that fails is that it "remains incomplete until corrected
  within the agreed architecture". One row — `json_lines_batch`, which was the
  port's only Θ(n²) workload — is inside its gate after the container-copy work;
  the rest are interpreted per-call cost, and this phase's profile of the
  runtime is reported under `### Call and stage overhead: profiled, with the
  copy and allocation removals measured`, which now includes the per-call
  header cache: it removed 27% of the three-million-call probe and 7–18% of
  twelve complete workloads without reaching their gates. The smallest
  remaining change is a bounded specialization over the bodies the workloads
  actually run, or an instruction cache. Both were tried in the form the fixed
  decisions permit: the single-return body shape was implemented and measured at
  -6.2% on the call-heavy probe and within noise on every designated workload
  (removed), and decoding statement lists once was measured identical (removed);
  the allocation route is closed by measurement too — the engine now allocates
  2.00 times per loop iteration and 14.00 per `Result`-returning call, and
  `xsh-runtime-stats` reports those figures directly. What remains for the gate
  is the per-step dispatch of the workloads' large bodies, which is what a
  second execution graph would have to replace and what §5 does not authorize.
  All of this is recorded under `### Call and stage overhead`. The four cold rows carry one
  additional charge that the others do not — lowering and verifying the
  embedded bodies, measured at two thirds of their preparation cost under
  `### What the failures are` — and that pass is cheaper work with a bounded
  target, so it is the second item on the list rather than part of the first.
- **The worker stages' dispatch is eager, though their semantics are not.**
  §3.1's requirement that the existing worker paths share the producer
  semantics is met and pinned by
  `worker_stages_consume_a_producer_through_the_producer_machinery`; what
  remains is that `par-map` and the fused path collect their input before
  dispatch, so a producer's rows are interpreted one `Vec` ahead of the
  workers. That is inherent to a stage whose own result is a `List` of every
  mapped row, and the difference is a memory profile rather than a behavior
  (`### Resumable producers`).
- **Gated groups.** **G02** is ported and measured inside its gate. **G01**,
  **G03**, **G06**, and G05's interface inventory are documented boundaries with
  their call graphs. **G04**, **G05**'s rfkill and block-device inventories, and
  **G07** were ported, measured per group, failed their gates, and were removed
  and retained natively under §12.3, with the measurements in `## Performance`.
- **Production Rust is net additive in this phase and net deleting overall.**
  The port deleted 5,089 lines and added 4,749 (net **-340**); this phase alone
  added 2,264 and deleted 717 (net +1,547), because it added the suspending
  producer engine and the container/pool runtime work while retiring the
  superseded Linux bodies and dispatch arms. The follow-up permits the addition,
  and `## Code accounting` breaks both numbers down.
- **Architecture test A17's platform dimension** is not automated: it is the
  container runs recorded under `## Linux verification`. Its case, profile, and
  feature dimensions are automated.
- **`xsht check` on a file that reaches a large embedded module** costs more
  than the reference; see `## Tooling cost recorded for §3.6`. This is the cost
  §3.6 asked to measure rather than hide, and it is recorded rather than
  smoothed.

## Tooling cost recorded for §3.6

`xsht check` and `xsht lint` over the whole repository are identical to the
reference — each compared as sorted output with the two checkout paths
normalized, exit 0 for `check` (no findings on either side) and 1 for `lint`
(the same six `lint.multiline-tag-union` warnings in `dev/*.xsh`, no more and no
fewer). A single file that reaches a large embedded module is not free, and
§3.6 asks for that to be measured rather than hidden:

| Command | Reference | Candidate | Delta | Allowance |
| --- | --- | --- | --- | --- |
| `xsht api summary` | 13.000 ms | 12.123 ms | -0.877 ms | 2.00 ms |
| `xsht check core/ls.xsh` | 11.985 ms | 18.613 ms | **+6.628 ms** | 2.00 ms |
| `xsht lint core/ls.xsh` | 12.105 ms | 11.519 ms | -0.586 ms | 2.00 ms |

Each row is the median of seven interleaved runs of matched release binaries,
both from the repository root; the allowance column is the specification's
non-hot gate for that row's reference median.

`core/ls.xsh` references `cli.applet`, so checking it prepares
`stdlib/cli.xsh` — the largest embedded module at 3,109 lines — because a check
that skipped preparation would report a script-backed call as unlowerable while
the same file runs fine. `xsht lint` on the same file is unaffected, which
locates the cost in preparation rather than in the tooling generally.

The whole-repository comparison is also what caught the only lint regression
this port introduced: the new test files carried twenty-two warnings
(`lint.redundant-string-interpolation`, `lint.unused-local`,
`lint.redundant-result-unit`, `lint.prefer-in`) that the corpus they join does
not have. They were removed by editing the five files — no formatter or
autofixer was run, per `AGENTS.md` — and the sorted whole-repository lint output
is back to the reference's six `dev/*.xsh` warnings. The ten files that
`xsht fmt --check` still reports are unchanged from before this work and stay in
`## Open items for the repository owner`.

## Feature matrix

Built on `aarch64-apple-darwin` with the default features,
`--features native-tests`, and `--no-default-features`; all three compile. The
`native-tests` build is what carries the preparation counters the architecture
tests use (`xsh::frontend::stdlib_preparation`), and the container runs the
`linux-priv-tests` feature. `cargo test -p xsh --lib` (180 passed, 1 ignored)
covers the catalog against the registry in both directions under the default
features.

## Performance

Measured with paired, interleaved reference/candidate runs on matched release
builds (`lto = "thin"`, `opt-level = 3`) on `aarch64-apple-darwin`. The
reference is an immutable worktree of the starting revision. Raw samples are in
`bench/stdlib-port/results-final.json`; the runner is `bench/stdlib-port/run.py`.

Gates, from the specification: cold startup passes when
`C - B <= max(0.05 * B, 1.0 ms)` and non-hot end-to-end when
`C - B <= max(0.10 * B, 2.0 ms)`.

### The fixed workload set

`bench/stdlib-port/run.py` covers the classes §12.2 fixes. Each row is one
frozen script; the fixtures it reads (a 1 MiB Unicode text) are generated
deterministically by the runner rather than committed. The runner records the
sha256 of both binaries, of the generated fixtures, and of every workload
script, so a reported row can be tied to the exact inputs that produced it.

The table below is the **final measurement of this set**, taken on the finished
tree. The reference column is the immutable starting revision (`37e1502`, sha256
`857697691be30efe120a857ef540eddd1a6c13aade9b69993d744694c2245651`); the
*candidate* column is the build the ledger carried before the follow-up phase,
read back from the committed `results-final.json` at revision `d0bbc6e`; and the
*after* column is the final binary, sha256
`da2df390247f50577b7e6721267c61b8ae93aa054f953514bf295fe32e46d99a` — the build
that carries the per-call header cache and the statement-list recycling recorded under
`### Call and stage overhead`, and the build
`bench/stdlib-port/results-final.json` describes. Both digests are the ones the
runner recorded for that file, so it describes the final tree rather than a
sibling of it. The after column was measured after each change in this phase;
the set was also measured twice in a row before the producer work landed, and
the final run is one interleaved pass of the runner's per-workload sample count
(60 samples a side on the cold rows, 30 on the rest) against the final pair of
binaries, so `--rounds 1` here is one interleaved round, not a single sample a
side. Against the measurement before it the pass/fail set is identical: the
call-heavy rows moved 7–18% (the header cache) and 1–7% more where statement
lists are rebuilt per iteration (`text_wrap_unicode` 1115.4 → 1097.1 ms,
`ini_large_record` 244.5 → 229.2 ms, `json_path_ops` 51.0 → 51.2 ms), which the
controlled A/Bs under `### Call and stage overhead` attribute. Rows neither
change reaches — the cold rows, `json_lines_batch`, and the two controls — are
within 1.5 ms. The reference column re-measured alongside it
agrees with the B0 column to within 1.1 ms on every row (the one 15 s row,
`json_lines_batch`, within 0.3%), so the round-to-round spread is the noise
scale here. The Linux half of this set
(`linux_uptime`, `linux_meminfo`,
`linux_memory`, `linux_os_release`, `linux_modules_full`,
`linux_modules_partial`) is skipped without `--linux`; its last measurement is
the per-entry table under `### R12`, taken in the container, and the harness
route for re-running it is in `bench/stdlib-port/README.md`.

| Workload | Class | Reference (B0) | Candidate (B1) | After | Delta vs B0 | Budget | Result |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `cold_trivial` | cold | 9.651 ms | 8.700 ms | 8.865 ms | -0.786 ms | 1.000 ms | pass |
| `cold_quote` | cold | 10.863 ms | 10.340 ms | 10.760 ms | -0.103 ms | 1.000 ms | pass |
| `cold_pad` | cold | 10.830 ms | 10.443 ms | 10.957 ms | +0.127 ms | 1.000 ms | pass |
| `cold_cli_parse` | cold | 11.359 ms | 19.502 ms | 19.814 ms | **+8.455 ms** | 1.000 ms | **fail** |
| `cold_cli_usage` | cold | 11.204 ms | 18.003 ms | 18.494 ms | **+7.290 ms** | 1.000 ms | **fail** |
| `cold_cli_error` | cold | 11.400 ms | 17.679 ms | 18.121 ms | **+6.721 ms** | 1.000 ms | **fail** |
| `cold_dynamic_ref` | cold | 11.721 ms | 23.321 ms | 23.857 ms | **+12.136 ms** | 1.000 ms | **fail** |
| `cli_small_schema` | CLI | 13.753 ms | 507.673 ms | 438.353 ms | **+424.600 ms** | 2.000 ms | **fail** |
| `cli_wide_schema` | CLI | 14.783 ms | 923.505 ms | 786.323 ms | **+771.540 ms** | 2.000 ms | **fail** |
| `cli_repeated_parse` | CLI | 13.012 ms | 574.309 ms | 497.571 ms | **+484.559 ms** | 2.000 ms | **fail** |
| `text_wrap_unicode` | text | 165.636 ms | 1246.919 ms | 1097.065 ms | **+931.429 ms** | 16.564 ms | **fail** |
| `text_pad_batch` | text | 29.947 ms | 70.305 ms | 59.769 ms | **+29.822 ms** | 2.995 ms | **fail** |
| `fmt_batch` | text | 13.769 ms | 32.112 ms | 28.743 ms | **+14.974 ms** | 2.000 ms | **fail** |
| `quote_batch` | quoting | 13.976 ms | 33.116 ms | 30.110 ms | **+16.134 ms** | 2.000 ms | **fail** |
| `quote_edge_cases` | quoting | 15.067 ms | 44.019 ms | 40.706 ms | **+25.639 ms** | 2.000 ms | **fail** |
| `mime_batch` | MIME | 37.363 ms | 116.593 ms | 111.740 ms | **+74.377 ms** | 3.736 ms | **fail** |
| `ini_large_record` | INI | 16.221 ms | 279.659 ms | 229.159 ms | **+212.938 ms** | 2.000 ms | **fail** |
| `json_path_ops` | JSON | 17.916 ms | 65.119 ms | 51.199 ms | **+33.283 ms** | 2.000 ms | **fail** |
| `json_lines_batch` | JSON | 15688.054 ms | 15648.233 ms | 1115.626 ms | -14572.428 ms | 1568.805 ms | pass |
| `env_typed_lookups` | env | 12.551 ms | 21.743 ms | 19.950 ms | **+7.399 ms** | 2.000 ms | **fail** |
| `checksum_batch` | checksum | 11.961 ms | 17.584 ms | 17.133 ms | **+5.172 ms** | 2.000 ms | **fail** |
| `core_command` | tooling | 12.318 ms | 77.703 ms | 69.762 ms | **+57.444 ms** | 2.000 ms | **fail** |
| `native_control` | control | 25.160 ms | 24.726 ms | 24.453 ms | -0.707 ms | 2.516 ms | pass |
| `native_hash_control` | control | 133.508 ms | 133.110 ms | 133.328 ms | -0.180 ms | 13.351 ms | pass |

**One row moved, by 14×.** `json_lines_batch` builds 10,000 small records in an
interpreted loop and encodes them once. The loop's `records = records.push(...)`
copied the growing list on every iteration, so the row was Θ(n²) in the
interpreter — visible in the B0 column as well, because the reference binary
runs the same script: 15.97 s there, 15.65 s for the candidate before the
container-copy work, 1.12 s after. A direct single run of the same script
against both binaries confirms it outside the harness: 17.34 s for the
reference, 1.13 s for the current candidate, identical output (`457780`). This
is the only row whose cost was algorithmic rather than per-operation, and it is
now *inside* budget against the native baseline.

**Twelve rows moved after it, none of them into budget.** The per-call header
decode recorded under `### Call and stage overhead` cut 7–18% from the
call-heavy rows (`cli_small_schema` 494.8 → 441.1 ms, `ini_large_record` 299.4 →
246.0 ms, `json_path_ops` 60.9 → 53.1 ms in the controlled A/B), which is why
their deltas here are smaller than the ones the previous measurement recorded;
none of them closes its 2 ms allowance, because the work each row does per item
is still interpreted step by step.

Plus three project-tooling workloads measured outside the runner, unchanged by
this work: `xsht api summary`, `xsht check`, and `xsht lint` (see
`## Tooling cost recorded for §3.6`).

**Parity status of the failing rows.** The runner times each workload and
discards its output, so it cannot say whether a failing row *behaves* the same.
Measured separately with `bench/stdlib-port/parity.py` against the same final
pair of binaries: each of the runner's thirty declared workloads runs under both,
from the runner's own working directory, and **twenty-seven of them produce
byte-identical stdout, stderr, and exit status**. Six of the thirty are the
Linux rows, and each of those prints a duration it measured itself, so the
harness compares their stdout with that one field masked — the duration *is* the
measurement, and a faster candidate reports a smaller number (`linux_uptime`,
1 ms against 0 ms, is this phase's work showing up in the row's own print). That
masking is why earlier runs of this same comparison listed different rows as
identical: those rows were agreeing or disagreeing on the printed millisecond.
The three that differ — `linux_meminfo`, `linux_modules_full`, and
`linux_modules_partial` — differ only in the `executable:` line of the refusal
traceback that names the binary which produced it: on macOS the `linux.*` boot
entries refuse with the same `linux-unimplemented` message and the same exit
status (3) under both binaries.
They are the entries whose Linux implementation the port moved, and their
behavior is established where they run, in the container under
`XSH_LINUX_REAL=1` (`linux_entries_answer_from_the_embedded_implementations`
plus the `## Linux verification` runs). The other Linux-class rows — `linux_uptime`,
`linux_memory`, and `linux_os_release` — run their retained native macOS
bindings, and their output other than the self-measured duration is identical. The failures are timing only — the exact failing workload, its reference and
after medians, its delta, and its budget are the table above, and the smallest
outstanding issues are named below.

### What the failures are

They are one cause in two shapes, and neither is a defect that tuning removes.

**Per-call interpreted cost.** `cli_small_schema` parses a four-field schema 200
times and costs 424 ms more than the reference: about 2.1 ms per parse against
0.08 ms for the native parser. (It was 482 ms before this phase's header cache,
which is why the cache is recorded as moving the batch rows and not only the
probe.) The ported `cli.parse` runs on the interpreter's
frame machinery, so every option, argv word, and descriptor field costs
microseconds where the native one cost nanoseconds. The same shape produces the
CLI cold-start rows (`cold_cli_parse` pays 8.3 ms to prepare and run one parse),
`text_pad_batch`, `fmt_batch`, `quote_batch`, `quote_edge_cases`,
`checksum_batch`, `env_typed_lookups`, `mime_batch`, `json_path_ops`, and
`core_command`. The calibration in this build measures about 0.7 µs per
interpreted loop step and about 1.4 µs per interpreted call — re-measured in
this phase with three 200,000-to-1,000,000-iteration probes, which the reference
binary runs at the same speed because it executes the same script through the
same interpreter; a workload whose per-item work is a native microsecond cannot
close a 10× gap.

**Mandated preparation.** `cold_dynamic_ref` is a program that merely
*references* `module.load`, so §3.4 prepares all fifteen embedded modules before
execution: 6,474 lines parsed, checked, lowered, and verified. That is the rule,
not an accident, and it is why the row reads +11.8 ms. `cold_cli_parse`,
`cold_cli_usage`, and `cold_cli_error` pay the same cost for the one module each
of them reaches — `stdlib/cli.xsh` is 3,109 lines, the largest in the catalog.

§6 asks for that preparation to be measured phase by phase rather than as one
number, and it now is:
`src/stdlib.rs::cold_start_phase_profile` (an `#[ignore]`d diagnostic, run with
`cargo test --release --lib cold_start_phase_profile -- --ignored --nocapture`)
times lex+parse and dependency discovery, declaration checking, body checking,
and lowering plus store verification, over a release build:

| Preparation | Total | lex + parse + deps | declarations | bodies | lower + verify |
| --- | --- | --- | --- | --- | --- |
| the `module.load` program (all fifteen modules, 6,474 lines) | 11.37 ms | 3.13 ms | 0.10 ms | 0.66 ms | **7.49 ms (66%)** |
| `cli` alone (3,109 lines) | 6.30 ms | 2.11 ms | 0.06 ms | 0.31 ms | **3.81 ms (60%)** |
| `json` alone | 0.51 ms | 0.22 ms | 0.01 ms | 0.03 ms | 0.27 ms |
| `text` alone | 0.20 ms | 0.07 ms | 0.00 ms | 0.01 ms | 0.11 ms |

Two thirds of the cold-start cost is lowering and verifying the embedded bodies,
a quarter is parsing them, and checking is a rounding error. That is what the
rows pay, and it is also why they cannot be closed within the fixed decisions:
the program still has to be parsed and has to call its entry, and a program that
names `module.load` must prepare the whole applicable set before it runs. Even
preparation *at zero* would leave `cold_cli_parse` about 2 ms above its
reference, because the ported entry it calls is interpreted: the native
`cli.parse` costs 0.08 ms and the embedded one about 2.5 ms, against a 1 ms
allowance for the whole row. The frozen-image and persistent-cache routes that
would change that are excluded by the fixed decisions, and the remaining lever
inside them is the same one named below — cheaper interpreted dispatch, not
preparation.

`text_wrap_unicode` and `ini_large_record` are the two largest remaining rows.
`Str.wrap` scans a 1 MiB text one scalar at a time (1,201 ms against 164 ms), and
the INI encoder validates and formats every key through interpreted helper
calls. A sweep over record sizes — literal 50/100/200/400/800-key sections,
five encodes each — shows the INI row is **linear** in the number of keys
(0.02 s at 50 keys, 0.14 s at 800, ~28 µs per key, about 40 interpreted steps
per field), so the earlier note that this row was Θ(n²) from a per-key record
copy was wrong: no n-sized copy per `Record` read survives in the current tree,
and the row's remaining distance is the same per-operation cost as the rest of
the table, not a copy amplifier.

### Gated groups: measured per group, and reverted

§12.2 requires every viable G prototype to be measured per group before
integration, and §12.3 removes one that fails. All four Linux G prototypes that
were built were measured with the container's matched release binaries, in
process, over the fixture trees recorded under `## Linux verification`:

| Group | Workload | Reference | Candidate | Delta | Budget | Result |
| --- | --- | --- | --- | --- | --- | --- |
| G04 routes | 200 × `linux.routes()?.collect()` over 6 rows | 4 ms | 2383 ms | **+2379 ms** | 2 ms | **fail** |
| G05 block devices | 200 × `linux.block_devices()?.collect()` over 3 devices | 5 ms | 125 ms | **+120 ms** | 2 ms | **fail** |
| G07 modinfo | 200 × `linux.modinfo("mod_a")` over a 3-module tree | 94 ms | 163 ms | **+69 ms** | 2 ms | **fail** |
| G07 depmod | 200 × `linux.depmod("")` over the same tree | 115 ms | 235 ms | **+120 ms** | 2 ms | **fail** |

The routes row is the clearest: 11.9 ms per call against 0.02 ms, because the
parser walks every hexadecimal character in the route table through the
interpreter. `linux.rfkill_list` was not timed before the others settled its
fate; its implementation has the same shape (a per-attribute interpreted read
per device) and the block-device row beside it is the same code pattern. Cold
start agrees: `linux_routes.xsh` as a whole measured 15 ms (release) against the
reference's 5 ms.

All four prototypes were therefore removed and their native bodies restored, and
`stdlib/linux_routes.xsh`, `stdlib/linux_rfkill.xsh`,
`stdlib/linux_block.xsh`, and `stdlib/linux_module.xsh` were deleted. The five
public entries are native again on every platform, and their dispatch arms are
no longer platform-gated. G02 is the one G prototype that stays: its policy
costs about 0.04 ms per call on top of a file hash that costs milliseconds, so
50 verifications of a 740 KiB file measured +2 ms against a 2.8 ms budget on
release builds (+9 ms against 20.6 ms in debug). That margin is thin and is
recorded as such: a batch over *small* files would fail for the same reason the
other batches do.

### R12: the required Linux entries, measured per entry

R12 is a *required* group, not a gated prototype, and it had parity evidence but
no §12 measurement. It was measured the same way the G prototypes were — 200
calls in process, matched release binaries in the `Dockerfile.test` container,
`XSH_LINUX_REAL=1`, three interleaved rounds, median reported. The follow-up
phase added the same six entries to `bench/stdlib-port/run.py` as `linux_only`
rows so the measurement has a reproducible harness route beside it; that route
is declared in `bench/stdlib-port/README.md` rather than transcribed here,
because the test image ships no Python and the numbers below were taken through
the in-process harness the image can run:

| Entry | Workload | Reference | Candidate | Delta | Budget | Result |
| --- | --- | --- | --- | --- | --- | --- |
| `linux.meminfo` | 200 × `linux.meminfo()?.total` | 2 ms | 196 ms | **+194 ms** | 2 ms | **fail** |
| `linux.modules` | 200 × `linux.modules()?.collect().len()` over a 200-module `/proc/modules` | 37 ms | 2007 ms | **+1970 ms** | 3.7 ms | **fail** |
| `system.memory` | 200 × `system.memory()?.total` | 2 ms | 165 ms | **+163 ms** | 2 ms | **fail** |
| `system.os_release` | 200 × `system.os_release()?.id.byte_len()` | 1 ms | 52 ms | **+51 ms** | 2 ms | **fail** |
| `unix.uptime_seconds` | 200 × `unix.uptime_seconds()?` | 0 ms | 6 ms | **+6 ms** | 2 ms | **fail** |

The container ships no modules, so `linux.modules` was measured against a
200-module fixture bind-mounted over `/proc/modules` inside the container's own
mount namespace; both revisions read the same text and produced the same 40,000
entries, and the reference row confirms the fixture is real work for it too
(37 ms against the 1 ms an empty file costs).

**Every entry fails, by 3× to 98×.** R12 is therefore *ported and incomplete*:
§12.3's rule for a mandatory group is that it "remains incomplete until
corrected within the agreed architecture", so unlike the gated prototypes it is
not reverted to native — reverting is the treatment §12.3 reserves for a G
prototype, and it explicitly forbids inventing a native fallback for an R group
to escape a gate. The same is true of the eighteen runner workloads above. What
this measurement adds is that the Linux text class fails for the same reason and
by the same magnitude as the rest: `/proc` text interpretation costs about a
microsecond per line through the interpreter, against tens of nanoseconds for
the native parser. The follow-up's producer work changes *when* that cost is paid
for `linux.modules` — the call is now the read, and the rows are interpreted as
they are consumed, so a consumer that stops early no longer pays for rows it
never sees — but a full consumption still pays the same per-row cost, which is
why this table stands as measured.

## Tests

- `src/stdlib/embedded_fixture_tests.rs` (11 tests) — the committed fixtures
  under `tests/fixtures/stdlib/`, driven through the crate-private catalog
  probe: `os-release` quoting/escaping/duplicates/defaults, the two-path read
  and which failure it reports, `/proc/meminfo` units/duplicates/ignored lines,
  malformed-value and missing-key precedence, scaling saturation at both `Int`
  bounds, the accepted decimal spelling against `Str.parse_int`'s grammar and
  both bounds, module-record fields with the four distinct row failures, and
  `/proc/uptime` whole-second reading. See `### Fixture coverage for the
  host-text policy`.
- `tests/stdlib_port.rs::linux_text_entries_bind_the_embedded_implementations`
  (Linux) — the five R12 entries bind `ImplBinding::Script`, which is what makes
  the deleted native bodies unreachable rather than merely unused, and its
  companion `linux_entries_answer_from_the_embedded_implementations` runs them
  through the ordinary binary under `XSH_LINUX_REAL=1`.
- `tests/runtime/frontend_indexed.rs::stream_producers_are_lazy_and_stop_where_the_consumer_stops`
  — the producer contract at its boundaries, over
  `tests/fixtures/runtime/lazy-stream-producers.xsh`: the body has not started
  when the call returns, the rows come from the text the call retained, an early
  stop runs exactly one row and the defer, a body that never started runs no
  defer, a `return` out of the consuming loop stops the producer where it
  stands, `take`/`break`/an abandoned producer each run the defer once, a
  malformed later row is not reached by an early stop and does abort a full
  consumption, and unreadable text fails the call before any row is interpreted.
- `tests/runtime/frontend_indexed.rs::zero_argument_stream_producers_run_from_every_call_position`
  and `tests/xsh/stdlib/streams.xsh::test_zero_argument_stream_producers_run_from_every_call_position`
  — the parameterless-producer defect, fixed in the frame engine.
- `tests/runtime/frontend_indexed.rs::worker_stages_consume_a_producer_through_the_producer_machinery`
  — the producer contract through the worker paths, over
  `tests/fixtures/runtime/worker-stage-producers.xsh`: `par-map` maps the rows
  the producer yields, the fused `par-map | flat-map | reduce-by` path consumes
  it the same way, and a mid-stream failure is the failure the row declared,
  with the producer stopping at that row and its `defer` running exactly once.
- `tests/stdlib_port.rs::os_release_entry_reads_the_fixed_paths` (Linux) — the
  public entry's own two-path read against container-owned fixtures staged at
  `/etc/os-release` and `/usr/lib/os-release`; the three scenarios and the
  container invocations are under `### Fixture coverage for the host-text
  policy` and in `bench/stdlib-port/README.md`.
- `tests/xsh/stdlib/streams.xsh::test_stream_producers_are_lazy_and_run_defers_on_stop`
  — the public-surface half of the producer contract: no row exists after the
  call, and exactly one row plus the defer exists after `first()`.
- `src/modules/system.rs::linux_native_system_bodies_report_the_retired_route`
  (Linux) — the retained Linux arms report the retirement instead of answering.
- `tests/xsh/stdlib/shlex.xsh` — R01 parity cases, including every case the
  removed `src/modules/shlex.rs` unit tests covered.
- `tests/xsh/stdlib/bytes.xsh`, `tests/xsh/stdlib/time.xsh` — R06 parity cases
  (pre-existing public coverage, unchanged).
- `tests/xsh/stdlib/hash.xsh` — R08 parity cases: both GNU separators, the
  binary marker and a leading star on the path, uppercase-hex lowercasing,
  trailing carriage returns, any-length and empty digest fields, the two
  incomplete-line rejections, the non-hexadecimal rejection, and double-space
  precedence when both separators appear. Messages are compared as well as
  kinds.
- `tests/xsh/stdlib/tui.xsh` — R05 parity cases: exact bytes for all sixteen
  escape sequences, padding that is already wide enough, zero and negative
  widths, ANSI sequences and CR/LF inside padded text, Unicode scalar counting,
  and lone or unterminated `ESC` handling. `read_secret` keeps its native route
  and its piped-input case.
- `tests/xsh/stdlib/linux.xsh` — the dry-run coverage of the Linux entries,
  which the G prototypes also passed through while they existed.
- `tests/xsh/stdlib/hash.xsh::test_hash_verify_file_policy` — G02 parity cases:
  a verifying checksum, an uppercase one, the length rejection for two
  algorithms with their exact messages, a right-length non-hexadecimal
  checksum, a mismatch with its exact message, each of the four algorithms
  selecting its own digest, and a missing file reporting its read failure ahead
  of a malformed checksum. The follow-up added the four input shapes §8 asks
  G02's evidence to carry: the empty file (its canonical digest and a
  verification of it), one byte, a multi-block file (32 KiB, verified through
  both spellings of its digest), a batch of 32 small files each verified against
  its own digest and rejected against its neighbour's, and a directory as the
  destination, which is the same read failure rather than a checksum one. The
  same file passes on the reference build, so the cases pin the native behavior
  rather than describing the port; the batch case also documents why G02's
  margin is thin — its policy cost is per call, so a batch over small files
  multiplies the interpreted overhead the ledger's `## Performance` section
  attributes to every other batch row.
- `tests/stdlib_port.rs::a_loaded_module_calls_prepared_implementations` — a
  dynamically loaded module reaches the prepared `hash.sha256` primitive and
  the prepared `verify_file` policy, and still prepares no module of its own.
- `crates/xsht/tests/profile_parity.rs` — A17's case and profile dimensions: the
  same newline, non-ASCII, path-byte, and error cases through the debug and
  release binaries, compared byte for byte.
- `tests/xsh/stdlib/linux.xsh::test_linux_module_policy_uses_the_configured_tree`
  — `linux.modinfo` and `linux.depmod` against a fixture tree selected by the
  existing `XSH_MODULES_DIR`, run in a nested process so the variable reaches
  the reader. It is written against the public entries, so it covered the G07
  policy while that prototype existed and covers the native body it was
  reverted to now; that is what makes it usable as the fixture route for
  either. The native body is what the current run exercises.
- `src/runtime/eval/indexed/full.rs::function_headers_are_decoded_once_per_program`
  — the per-program header cache: a second call reuses the same `Arc`, and a
  program built without cache slots still decodes.
- `src/runtime/eval/indexed/full.rs::loop_iterations_reuse_their_statement_list`
  — §5's control-state claim as a test rather than a timing: two hundred loop
  iterations take their body's statement list back from the pool instead of
  building one each time.
- `src/stdlib.rs` unit tests — catalog/registry agreement in both directions;
  `cold_start_phase_profile` (an `#[ignore]`d diagnostic, not part of the
  default run) is the §6 phase measurement recorded under
  `### What the failures are`.
- `tests/xsh/stdlib/env.xsh` — R11 parity cases: an unset name with and without
  a fallback, a present empty value (which is never replaced by the fallback),
  values passed through byte for byte, the `env-name` rejection and its exact
  message, all four accepted boolean spellings plus case and white-space
  variants, other spellings and the empty value as `false` rather than an
  error, the integer grammar (surrounding white space including a non-breaking
  space, `+`/`-`, leading zeros, and both `Int` bounds), the rejections
  (empty or blank text, a lone or repeated sign, underscores, radix prefixes,
  decimals, inner white space, trailing or leading junk, non-ASCII digits, and
  both out-of-range bounds) with their `env-int` kind and message, and lookups
  through nested scoped-environment overlays.
- `tests/xsh/stdlib/process.xsh` — R03 parity cases: every case the native
  `argv_words` unit tests covered, whitespace runs and leading/trailing
  whitespace, empty and only-whitespace input, empty quoted arguments (`''`,
  `""`, `''''`, `a '' b`), quote concatenation, escapes outside and inside
  double quotes, quoted and escaped shell syntax characters, every member of the
  rejected set both as its own word and inside a word, unterminated quotes and a
  trailing escape, the rejection kind and the exact message text (including the
  multi-byte word before the offending character), and Unicode words,
  multi-byte characters, and every Unicode whitespace character.

## Code accounting

Counted with `git diff --numstat` between the starting revision `37e1502` and
this work tree, production Rust only (`src/`, `crates/*/src`). The follow-up
phase's own numbers are the same count between `d0bbc6e` and this work tree,
because the port's earlier counts were taken before the last three commits
landed:

| | Lines |
| --- | --- |
| Production Rust added, `37e1502` → this tree | 4,749 |
| Production Rust deleted, `37e1502` → this tree | 5,089 |
| Net production Rust change | **-340** |
| Production Rust added, `d0bbc6e` → this tree (this phase) | 2,264 |
| Production Rust deleted, `d0bbc6e` → this tree (this phase) | 717 |
| Net production Rust change, this phase | **+1,547** |
| XSH implementation (`stdlib/*.xsh`, 16 modules) | 6,484 |
| Deleted native owner files | `src/modules/cli.rs` (2,263), `src/modules/mime.rs` (283), `src/modules/shlex.rs` (77), the Linux text bodies in `src/modules/system.rs`, `src/modules/unix.rs`, and `src/modules/linux/real/kernel.rs` |
| Test, doc, and bench churn (`tests/`, `docs/`, `bench/`, `AGENTS.md`, this ledger, `37e1502` → this tree) | +39,103 / -19 |

**The phase is net additive and the port as a whole is not, and both are
recorded rather than smoothed.** The follow-up permits the addition — "A general
runtime improvement may add Rust in this phase; do not disguise it or suppress a
necessary correctness repair to hit a local deletion quota" — so the accounting
says both numbers plainly: this phase added 2,264 lines and deleted 717 (net
+1,547), while the port overall has deleted 5,089 lines of native policy and
added 4,749 (net **-340**). The additions are

- the embedded catalog and its preparation gate (`src/stdlib.rs`, 559 lines),
  lowering and linkage (`src/runtime/eval/lower.rs`, 686), the loader's stdlib
  attachment (`src/loader.rs`, 137), the registry's implementation bindings
  (`crates/xsh-registry`, 346 + 107) are the one-time infrastructure the earlier
  ledger already accounted for, and
- the rest is this phase's runtime work: the suspending producer engine
  (`lowered_run/indexed_run/producer.rs` and `explicit_run.rs`, 550 + 349), the
  `Arc`-backed containers and their copy-on-write paths (`src/runtime/value.rs`,
  `src/runtime/eval.rs`, `lowered_run.rs`, `indexed/full.rs`), the frame scratch
  pools, and the private bridges.

The offsetting deletions are the four superseded Linux bodies, the obsolete
`mime` lowering special cases, the eager stream-item buffer with its recursive
`yield` arm, the native policy each ported group removed, and — this phase — the
retired dispatch arms, which is why the port's own number is now negative while
the phase's is not. The maintenance argument the migration rests on still holds
for the *stdlib* surface: every ported group moved policy out of Rust and into
the embedded library, and the remaining native boundary is documented group by
group. The whole-port line count now agrees with it; the phase's does not, and
the table above is the measure of both.

The required Linux entries (R12) bind per target: the Linux overload is the
script binding and the macOS overload is the native one, so a Linux build has
no path to a superseded implementation while macOS keeps the behavior it had.
The macOS bodies stay. The Linux-only bodies behind those bindings were removed
in the follow-up phase (see `### Superseded Linux bodies removed` above): the
same file still builds and behaves on macOS, and on Linux the retired arms fail
loudly if binding selection ever regresses. The gated Linux prototypes that
failed their gates were removed with the rest of their port — module, binding,
bridges, and dispatch — so their native bodies are the only implementations
again on every target.

## Linux verification

Every Linux build, test, and measurement in this port goes through the
`Dockerfile.test` environment, run as the `xsh-test` image and driven by
`xsh dev internal test-linux` (developer contract) or `xsh dev internal
test-linux-ci` (CI contract). The target is `aarch64-unknown-linux-musl` with
the flags in `dev/targets.xsh::docker_test_env`. That container owns the
compiler, the musl CRT objects, and the `__isoc23_*` symbol aliases this tree
links against, so no other Linux toolchain, image, or libc counts as evidence
about Linux support. The rule is recorded in `AGENTS.md` under Verification.

R12's entries and, while they existed, the G prototypes were executed on
Linux — not only prepared — in that environment, with the repository mounted
at `/work`:

- Full `tests/xsh/stdlib/` on Linux: **192 passed, 3 failed, 16 skipped**.
- The whole container corpus, `xsht test`: **441 passed, 4 failed, 18 skipped**.
  Three failures are the `fs.xsh` root cases below; the fourth is
  `dev/tests/test-lifecycle.xsh::test_build_failure_stops_at_the_cargo_boundary`
  (`test-fail: executable not found`), which drives a build tool the container
  does not stage.
- The three failures are `fs.xsh` (`fs-root-mkdir`, `fs-root-symlink`,
  `Bad file descriptor (os error 9)`), a running-as-root-in-a-container
  artifact. `tests/xsh/stdlib/fs.xsh` exercises no script-backed entry — the
  whole `fs` module is retained native — so the port cannot have produced them.
  They are recorded as environmental, not as a pass.
- Among the passes, `tests/xsh/stdlib/linux.xsh::test_linux_dry_run_covers_module_surface`
  reaches `linux.routes`, `linux.rfkill_list`, `linux.block_devices`,
  `linux.modinfo`, `linux.depmod`, and `linux.modules`, and
  `test_linux_module_policy_uses_the_configured_tree` drives the module query
  and index against a fixture tree selected by `XSH_MODULES_DIR`. While the G
  prototypes existed these entries reached their embedded implementations; they
  are native again after the reverts, so the current run exercises the native
  bodies through the same public surface. `tests/xsh/stdlib/unix.xsh` reaches
  `unix.uptime_seconds`, which stays script-backed.
- The container's own Rust suite, `cargo test --features linux-priv-tests
  --test integration`, was run for both revisions in that image with the
  repository at `/work`. Reference: **373 passed, 133 failed, 26 ignored**.
  Candidate: **388 passed, 133 failed, 26 ignored**. The two failure sets were
  compared as sorted name lists and are **identical**, so the port adds fifteen
  passing tests — the architecture tests and the prepared-implementation cases —
  and no Linux failure. The candidate's 133 differ from its macOS 130 by the two
  `*_on_non_linux` cases, which pass on Linux, and five Linux-only cases
  (syscall-trace, the container trust directory, and the signal-order case) that
  do not run on macOS.

#### Re-measured after the harness repair, this phase's changes, and the producer work

The figures above were taken before `tests/runtime/common.rs` could resolve the
workspace product binaries in the container, which is what produced the 133
identical failures on both revisions. Re-run in the same image
(`aarch64-unknown-linux-musl`, repository at `/work`, the same `docker run`
mounts and `CARGO_TARGET_DIR=/work/target`):

| Run | Result |
| --- | --- |
| `cargo test --features linux-priv-tests --test integration`, after this phase's deletions and fixtures | **526 passed, 1 failed, 26 ignored** |
| the same suite, after the suspending-producer work | **527 passed, 1 failed, 26 ignored** |
| the same suite, on the final tree (re-run after the trace-data change) | **527 passed, 1 failed, 26 ignored**, the same single failure |
| the same suite, after the fixed-path fixture test was added | **529 passed, 1 failed, 26 ignored**, the same single failure (the corpus runner and its four environment cases) |
| the same suite, after the per-call header cache | **529 passed, 1 failed, 26 ignored**, the same single failure |
| the same suite, after statement-list recycling | **529 passed, 1 failed, 26 ignored**, the same single failure |
| that test under the three staged scenarios (`XSH_OS_RELEASE_SCENARIO=etc`, `fallback`, `neither`) | **1 passed** in each, the assertions in the fixture table above |
| the corpus through that suite's runner (`runtime::coverage::xsh_native_tests`) | **442 passed, 4 failed, 19 skipped** |
| `cargo test --features linux-priv-tests` (every target) | **did not terminate**: ten minutes with no progress, the last observed state being an idle `xsh-test-sleeper` process holding no CPU time |

The single integration failure is the corpus runner, and its four failures are
the same environment cases as before: three `fs.xsh` root cases
(`Bad file descriptor (os error 9)`) and
`dev/tests/test-lifecycle.xsh` (`test-fail: executable not found`). Everything
else that used to fail in the container now passes, including the two
`stdlib_port` cases added in this phase, the producer contract cases, and
`hash.xsh`'s new G02 batch and boundary inputs. The every-target run is recorded
as a limit of this evidence rather than as a pass: `tests/linux_priv.rs` is
outside the integration target, and this container arrangement stalls somewhere
in it. The run was killed and the integration target used instead, which is the
same target the earlier revision comparison used. The stalled case was not
identified further, so no claim is made about which one it was.

The three tables that follow are the parity half of the §12.3 evidence for the
reverted prototypes: each shown an embedded implementation beside the native
one, matching byte for byte in every reachable case. The performance half, in
`## Performance`, is what removed them.

G04 was compared against the native baseline on the container's real `/proc`,
running the reference build from the starting revision and the candidate side
by side, both built in the same image:

| Case | Result |
| --- | --- |
| `XSH_LINUX_REAL=1`, whole stream (2 IPv4 rows, 3 IPv6 rows) | byte-identical |
| `XSH_LINUX_DRY_RUN=1`, record | byte-identical |
| `XSH_LINUX_DRY_RUN=1`, `XSH_LINUX_DRY_RUN_LOG` line | byte-identical (`{"op":"routes"}`) |
| neither gate open | identical but for the executable path in the traceback |

The IPv4 rows exercise the baseline's little-endian word rendering (a gateway
of `01D7A8C0` renders as `1.215.168.192`) and the mask population count; the
IPv6 rows exercise prefix rendering, the compressed form, a metric read as
hexadecimal, and a flag word that does not fit sixteen bits and so contributes
no flags.

G07 was executed and compared the same way, against a module tree in the
container's own mount namespace (`XSH_MODULES_DIR`, which the entries already
honour), with plain `.ko` files carrying NUL-delimited metadata:

| Case | Fixture | Result |
| --- | --- | --- |
| `linux.modinfo`, index lookup | `mod_a.ko` with description/license/version and two `parm` values | byte-identical |
| `linux.modinfo`, explicit path | `modinfo("mod_b.ko")` | byte-identical |
| `linux.modinfo`, unknown name | a name the index does not hold | byte-identical (`linux-modinfo: module not found`) |
| `linux.depmod`, whole file | five modules across two directories, one dependency resolved to a nested path, one unresolved, one case-mismatched (`MOD_A`), one module with no dependency | byte-identical |

The depmod fixture pins the line form (`mod_a.ko: mod_b.ko`, `mod_e.ko:`), the
dependency order inside a line, the nested relative path, the dropping of an
unresolved and a case-mismatched dependency, and the text ordering of the lines
(`mod_a.ko`, `mod_b.ko`, `mod_e.ko`, `nested/mod-c.ko`, `nested/mod-d.ko`).

G05 was executed and compared the same way, against trees built inside the
container's own mount namespace (a tmpfs over `/sys/class` or `/sys`, which
touches nothing outside the container):

| Case | Fixture | Result |
| --- | --- | --- |
| `linux.rfkill_list`, whole stream | 9 directory entries: ids 2, 7, 10, `notanid`, `rfkill+3`, `rfkill 4`, `rfkill0x5`, `rfkill-1`, `rfkill007` | byte-identical |
| `linux.rfkill_list`, attribute read fails | a device with no `name` | same kind and message (`linux-rfkill: No such file or directory (os error 2)`); the rendered traceback differs because the failure is raised inside the embedded frame |
| `linux.block_devices`, whole stream | `sda` with three partitions and a `slaves`/`queue` sibling, `sdb` with no `size`, `sdc` with `size` = `abc` and a 4096 block size | byte-identical |

The rfkill fixture pins the parsing policy: `rfkill+3` and `rfkill-1` are
accepted, `rfkill 4` and `rfkill0x5` are skipped, `rfkill007` and `rfkill7`
both report id 7, and the ids sort numerically (`-1, 2, 3, 7, 7, 10`). The
block fixture pins the defaults (a missing `size` is 0, a missing block size is
512, an unparseable `size` is 0), the 4096 block size, and the path-text
ordering of partitions (`sda1`, `sda10`, `sda2`).

## Follow-up phase: repaired verification

The follow-up work (`xsh-stdlib-runtime-followup.md`) started from B1 and is
recorded here as it lands.

### The integration suite executes again

`tests/runtime/common.rs::build_workspace_binaries` derived the Cargo profile
directory from the running test executable's own directory. This toolchain
emits test and helper targets into a per-unit
`target/<profile>/build/<package>/<hash>/out` directory, so the second parent
was that `out` directory and the "profile name" it passed to `cargo build
--profile …` was a 16-hex-digit unit hash. Every test that needed the `xshi` or
`xsht` product failed before it started: 130 of them, which the previous phase
recorded as pre-existing environmental failures because the two revisions
failed identically.

The harness now takes the profile directory from Cargo's own artifact
notification — `CARGO_BIN_EXE_xsh` names the `xsh` binary of this package, whose
parent is the directory the workspace products are built into — and takes each
product path from the `compiler-artifact` messages of the build it runs, with
the profile directory as the fallback. It also passes `--target <triple>` when
the profile directory sits under a triple, so an explicit-target build resolves
the same products the suite was built for.

`bench/stdlib-port/fixtures/harness-profile-discovery.patch` is that change as a
patch against either baseline; applying it to B0 makes its suite run too. With
it, B0 reports **497 passed, 1 failed, 27 ignored** and the candidate reports
**519 passed, 0 failed, 27 ignored** for `cargo test --test integration` on the
final tree. The whole workspace (`cargo test --workspace --no-fail-fast`,
including the `xsht`, `xshi`, `xsh-applets`, `xsh-net`, and `xsh-registry`
crates this suite does not cover) reports **1,039 passed, 0 failed, 29
ignored** on the same tree. Its previous reference comparison, and the
intermittent failures seen when it overlaps a build, are in
`## Pre-existing failures and skips`.

### Defects the suite then found

Every one of these was invisible while the suite could not start, and each is a
real defect rather than a harness artifact.

- **`xsht fmt` printed string-literal record keys unquoted**, producing source
  that does not parse: `{"a b": "1"}` came back as `{a b: "1"}` and `{"": "1"}`
  as `{: "1"}`. The arena stores an identifier key and a string-literal key as
  the same `Name`, so the printer could not tell them apart. `write_record_key`
  now reads the field's own source span and prints a quoted key as a string
  literal (`crates/xsht/src/format.rs`).
- **`xsht fmt` collapsed a short multi-line tag union**, while
  `lint.multiline-tag-union` demands exactly that multi-line shape for three or
  more variants. The two tools were unsatisfiable together: formatting a
  declaration the lint asked for produced a file `fmt --check` rejected. The
  printer now keeps a union the author wrote across lines multi-line, the same
  way it already preserves the author's line breaks for records.
- **The corpus had never been parsed.** Two test files carried syntax errors
  that no run had reached: `tests/xsh/stdlib/ini.xsh` used bare identifiers as
  record keys for keys that are not identifiers (``{c]d: "2"}``, ``{a b: "1"}``,
  a key spanning lines) and `tests/xsh/stdlib/cli.xsh` did the same for a
  dashed command name. They are now string keys, and the invalid-key cases they
  were written for — `[`, `]`, newline, and empty keys — actually execute.
- **Six `lint.multiline-tag-union` warnings in `dev/*.xsh` and one
  `lint.shadowing` error in `dev/targets.xsh`** stood between the corpus and its
  own gate. The unions are multi-line now; `native_execution`'s parameters no
  longer shadow the module's `host_os` and `host_arch` functions.
- **`tests/runtime/coverage.rs` listed the catalog and benchmark sources as
  user code.** The runnable-corpus gate asked `xsht fmt`/`xsht lint` about
  `stdlib/**` and `bench/**`, which `xsht-config.ini` already excludes from
  discovery for the reasons recorded there: embedded implementations are
  validated by the catalog gate under implementation rules, and the benchmark
  scripts are host tooling. The gate now applies the same policy.
- **`cargo test --workspace` did not compile** after the traceback-name change
  later in this phase. The change replaces `TracebackFrame.name`'s `String` with
  `TracebackName` (the lazy form recorded under `### Call and stage overhead`),
  which is a change to the supported `libxsh` trace-data tier, and it left two
  constructions behind in `crates/xsht/src/trace.rs`'s own tests. The same
  change also had to add `TracebackName` to the `xsh::trace::model` re-export:
  a public field whose type the model module does not name cannot be
  constructed by a consumer. Both are fixed, and the test expectations compare
  the same rendered text as before. This is the one contract change this phase
  made; `crates/xsht`'s trace tests are its consumer-side evidence.

### The nested-shadowing miscompilation is repaired

`lower_stmt` refused to lower a `let`/`var` whose name was visible in any
enclosing scope: it recorded a diagnostic blocker and substituted a `Unit`
statement, which committed, so the declaration silently did nothing and every
later read resolved to the outer binding. `shadowed("ab cd")` returned
`ab cd;` instead of `ab;cd;`.

`SlotScope` now tracks where the innermost scope's declarations begin, so the
lowerer distinguishes "this scope already declared that name" (still refused;
the checker reports it as `check.duplicate-name` before lowering runs) from "an
enclosing scope declared it" (accepted: the inner scope resolves a new slot and
the outer binding is restored on exit). `tests/xsh/basic.xsh` covers the loop
body and the nested block from the ledger's reproduction.

**What remains unsupported, and why it is recorded rather than fixed here:** a
declaration that shadows inside a *pipeline-stage block*, a *comprehension
target*, or a *match-pattern binding* still takes the old guard, which lowers it
as a no-op. Widening the acceptance at those sites changes which stage forms the
lowering produces, and the compact lowering then refuses functions that it used
to commit (`indexed IR could not encode full_ir_function_blocker`), so the
change is not confined to shadowing. The residual is recorded in
`## Open items for the repository owner` with its reproduction; it is a
pre-existing limitation of the compact lowering, not a regression.

### Genuine append replaces read-concatenate-rewrite

`stdlib/linux_text.xsh` wrote the dry-run log by reading the whole file, adding
a line, and rewriting it. That is not what the baseline does: it appends to the
open file. The difference is observable — bytes that are not valid UTF-8 in an
existing log were replaced, an unreadable-but-writable log lost its content, and
two writers could drop each other's lines.

The module now composes the line and appends it through the private
`append_bytes(Path, Bytes) -> Result[Unit]` operation, whose host half
(`src/modules/fs.rs::append_bytes`) performs exactly the baseline's
create-parents/open-create-append/write-all and transports the error. XSH keeps
the logging decisions, the line construction, and the error kind. The operation
is declared as a private bridge by the `linux_text` catalog entry, so the
lowering rewrite and the store verifier admit it only from that module.

Coverage: `tests/xsh/stdlib/linux.xsh::test_linux_dry_run_log_appends_in_place`
(invalid UTF-8 preserved, one record per call, missing parents created, a
directory destination reported as the call's `Err`) and
`src/modules/fs.rs::append_bytes_preserves_existing_bytes_and_appends_in_place`
(two appenders, invalid UTF-8, a non-UTF-8 path where the host filesystem
accepts one, and a directory destination).

### Container copies: shared backing and consuming calls

Every record, map, and record-vector payload is now shared (`Arc<BTreeMap<…>>`,
`Arc<Vec<…>>`) instead of inline, and four changes removed the copies that made
reads and updates quadratic:

- **Shared backing, copy on write.** Reading a container out of a slot is a
  pointer bump; a mutation copies only while the value is still shared
  (`Arc::make_mut`), which is what keeps value semantics. `List`/`SharedList`
  already worked this way; the record family now matches it.
- **Borrowed receivers for read-only methods.** `Record.len`/`has`/`get`/`keys`
  and `Map.len`/`has`/`get` take the receiver by reference. The updating methods
  (`Map.set`/`push`/`remove`) still take it by value, because they return a new
  container.
- **A consuming call for `x = x.set(…)`.** The lowering's statement shape proves
  which slot the result overwrites and that the receiver is a read of that very
  slot; the frame runtime then takes the value out of the slot once the
  arguments are evaluated, and only while the slot still holds the container the
  receiver came from. An argument that assigned to the slot wins: the slot keeps
  what it was given and the call keeps the value its receiver was read from.
  This is the "proven consuming/last-use path" the follow-up asks for, and it is
  structural — no name matching, no stdlib knowledge.
- **Host-resource transfer walks the lowered value.** Assigning a value
  transferred owned host resources by converting the whole value to a `Value`
  first, which deep-copies every container it holds; the walk now runs over
  `LoweredValue` directly.

Measured with a geometric sweep on the debug host build (`time.now()`, one
process per row, sizes 16/64/256/1024 keys):

| Workload | Before | After |
| --- | --- | --- |
| Build a map of `n` keys with `m = m.set(k, v)`, 1024 keys | 336 ms | **29 ms** |
| Read every key of a 1024-key map with `m.get(k)` | 118 ms | **9 ms** |
| 500 `m.set` calls over maps of 64 / 512 / 1024 keys | 96 / 248 / 431 ms | **10 / 19 / 28 ms** |

Both curves are linear in `n` afterwards; before, both were quadratic — the
build path because each assignment converted the map to a runtime `Value`, and
the read path because each lookup copied the map. The scaling, not the absolute
numbers, is the finding: the debug host build is not a benchmark.

`cargo test -p xsh --lib` (169 at that point in the phase, 180 on the final
tree), `xsht test tests/xsh/stdlib` (186 passed, 26 skipped), and the
integration suite are green after the change.

### Superseded Linux bodies removed, and the route that replaces them pinned

The follow-up's §9 asks for the Linux-only implementations that the embedded
library supersedes to be deleted once parity is established, keeping one
production implementation of each migrated policy. Four bodies were removed and
one lowering route was closed:

- **`system.memory` and `system.os_release`.** The Linux `memory_impl`,
  `os_release_impl`, `parse_os_release`, and `unquote_os_release_value` in
  `src/modules/system.rs` (about 120 lines, plus the `rustc_hash` import they
  were the only user of) are gone. The Linux arms of both functions remain, and
  deliberately report that the embedded implementation owns the entry: reaching
  them means binding selection regressed, so they fail loudly instead of
  answering from a retired parser.
- **`linux.meminfo` and `linux.modules`.** `src/modules/linux/real/kernel.rs`
  loses `meminfo`, `modules`, `ModuleStream`, and their five parsing helpers, and
  `src/modules/linux/real.rs` loses the two `/proc` path constants that only
  those bodies read. `dmesg` keeps its body and its stream; it has no embedded
  implementation.
- **`unix.uptime_seconds`.** The Linux `/proc/uptime` reader is gone; the macOS
  body and the unsupported-platform fallback stay.
- **The obsolete `mime` lowering special cases.** `lowered_module_call_args`
  recognized `mime.lookup_ext` and `mime.lookup_path` and lowered them to
  `RuntimeOp::MimeLookupExt` / `MimeLookupPath`. Both entries are script-backed
  on every platform now, neither operation has a runtime arm or any other
  lowering entry left, and the early returns bypassed the new guard below, so
  they are deleted; the two spellings reach the embedded implementations like
  every other embedded entry.

The deletion rests on an invariant rather than on reading the dispatcher:
`tests/stdlib_port.rs::linux_text_entries_bind_the_embedded_implementations`
asserts, on Linux, that `system.memory`, `system.os_release`,
`unix.uptime_seconds`, `linux.meminfo`, and `linux.modules` all bind
`ImplBinding::Script`. Its companion,
`linux_entries_answer_from_the_embedded_implementations`, runs the five public
entries through the ordinary binary under `XSH_LINUX_REAL=1` and requires them to
answer: a retired native body would fail those calls with a message that names
the retirement.

`lowered_module_call_args` also gained the guard that makes the deletions safe
rather than merely correct today: a signature whose binding is `Script` is no
longer lowered to its own operation. Before that, an embedded module that failed
to prepare would have fallen through to the native operation kept for the other
platforms — exactly the silent fallback §2 forbids — and on Linux that operation
is now the retirement stub.

### Fixture coverage for the host-text policy

The R12 entries read fixed paths, so their *reading* boundary is exercised by
the corpus on Linux; their *interpretation* had no fixture coverage, and the
follow-up's §3.3 asks for it without making internals public. There are two
authorized boundaries, and this phase used both:

- **A crate-private, test-only catalog companion.** `Evaluator::probe_embedded_call`
  (in `src/runtime/eval.rs`, behind `cfg(test)`) resolves a module identity in the
  compiled catalog, runs it through the same preparation and verification the
  runtime performs, and calls one named function through the same indexed entry
  point a prepared function uses. It is unreachable from production and from user
  module loading, and the arguments are ordinary values, so a helper that reads a
  path reads the path the test names. `src/stdlib/embedded_fixture_tests.rs`
  drives it with the committed fixtures under `tests/fixtures/stdlib/`:

  | Fixture | Policy it pins |
  | --- | --- |
  | `os_release/quoted_and_escaped.txt` | double and single quotes, escaped quote and backslash escapes, a key that is not trimmed, a line with no `=`, an indented comment, and last-line-wins for `ID` |
  | `os_release/defaults.txt`, `name_without_pretty.txt`, `empty.txt` | the `NAME`/`ID`/`VERSION` defaults and `PRETTY_NAME` defaulting to the resolved name |
  | `meminfo/valid.txt` | the `kB` unit rule, ignored short and non-`kB` lines, an unreported key that still parses, and last-line-wins |
  | `meminfo/malformed_reported.txt`, `malformed_unreported.txt` | the first malformed value decides, and a malformed *unreported* key still fails the call |
  | `meminfo/missing_available.txt`, `missing_total.txt` | the missing-key report is the first missing key in record order |
  | `meminfo/saturation.txt` | both saturation bounds, the largest and smallest counts that still fit, and a negative count |
  | `integers/field_spellings.txt` | the accepted decimal spelling against `Str.parse_int`'s wider grammar and both `Int` bounds, checked through both modules that carry the rule |
  | `modules/rows.txt`, `missing_size.txt`, `missing_use_count.txt`, `malformed_size.txt`, `short_row.txt` | record fields, the `-` placeholder, a trailing comma, ignored trailing fields, blank-line retention, and the four distinct row failures |
  | `uptime/spellings.txt` | whole seconds from `/proc/uptime` text, including the fraction, a negative value, and out-of-range text |

- **The complete public call against fixed-path fixtures inside the container's
  mount namespace.** `read_release_text` — the two-path read the public
  `system.os_release` routes through — is covered at the helper level
  (`release_text_prefers_the_first_path_and_reports_the_second_failure`):
  a readable first path wins, a readable second path answers when the first
  cannot be read, and the failure reported for the call is the *second* read's,
  distinguished by making the two failures different errors.

  The wrapper-level half is
  `tests/stdlib_port.rs::os_release_entry_reads_the_fixed_paths`, which runs the
  real entry — no path parameter, no override — against the two paths it reads.
  The route stages container-owned fixtures *at* those paths and names the
  scenario in `XSH_OS_RELEASE_SCENARIO`; the fixture contents are committed
  under `tests/fixtures/stdlib/os_release/fixed-path/`, and the three
  invocations are recorded in `bench/stdlib-port/README.md`. Because the image ships
  `/etc/os-release` as a symlink to `/usr/lib/os-release`, each scenario points
  the two paths into the committed sources inside the container's own writable
  layer, which is what lets the two be controlled independently:

  | Scenario | Staged at the two paths | What the run asserts |
  | --- | --- | --- |
  | `etc` | the `etc-os-release.txt` fixture at `/etc/os-release` | the first read answers: `Fixture Etc\|Fixture Etc "quoted"\|1.0\|7\|fixture-etc-last` — quoting, an escaped byte, and last-line-wins through the public entry |
  | `fallback` | `/etc/os-release` dangling, `usr-lib-os-release.txt` at `/usr/lib/os-release` | the second read answers: `Fixture Usr\|Fixture Usr\|2.0\|8\|fixture-usr` |
  | `neither` | `/etc/os-release` dangling, `not-utf8.txt` at `/usr/lib/os-release` | the call fails with `error: system-os-release: file is not valid UTF-8 at byte 5` — the *second* read's failure under the entry's own kind, which is what proves the first read failed, the second was attempted, and its failure is the one reported |

  An ordinary container run sets no scenario variable, so the test reports itself
  as skipped there rather than passing quietly, and the container's real release
  files are untouched in a run that stages no fixture. Nothing here binds a
  `/proc` or `/sys` path, and nothing is mounted over a shared mount.

### Resolved preparation selection tightened

The follow-up's §1 finding 4 names two over-selections in
`stdlib::required_modules` (§6): it treated any identifier spelled `load` or
`module` as a dynamic loading route, and it counted a field's base identifier as
a bare mention of the module the field is read from. Both are now resolved
rather than syntactic.

- **The loading route is `module.load`, and only that.** A field named `load`
  whose base is an identifier bound to the `module` standard module — directly,
  or through `use module as …` — is the route; a local variable called `load`, a
  record field called `load`, a field of anything else, and a bare
  `use module` are not. The route itself is unchanged: naming it still prepares
  the complete applicable catalog before execution, which is why
  `cold_dynamic_ref` stays a failing performance row rather than being redefined
  into first-call preparation.
- **A qualified field resolves through its base.** `env.get` is the native
  `get`, so it no longer selects `env`'s script-backed neighbours; `env.get_or`
  does. The same rule covers every mixed module in the catalog.

Both are asserted with preparation counters, before and after execution, in
`tests/stdlib_port.rs`: `selection_ignores_names_that_are_not_script_backed_references`
(a binding named `load` prepares zero modules, `env.get` prepares zero while
`env.get_or` prepares one, `use module` alone prepares zero) and
`a_resolved_dynamic_loading_route_prepares_the_complete_set` (an aliased
`module.load` prepares the whole catalog). They are the A01 and A05 rows in
`## Architecture tests`. No `## Performance` row is claimed for this change,
because none of the frozen workloads exhibits either over-selection — a program
that names `module.load` genuinely needs the catalog, and the rest reach their
modules through resolved calls — so the honest evidence is the counter assertions
above, not a timing row.

### Resumable producers: the producer body is a suspended frame

The follow-up's §3.1 asks for genuinely lazy `stream` producers in the indexed
runtime: calling a producer must not execute its body, a pull must resume to the
next `yield`, `linux.modules` must read at the call and parse at consumption, a
malformed later row must not be reached by a consumer that stops earlier, and
`defer`/cleanup must run exactly once for exhaustion, `break`, `take`, `first`,
errors, and an abandoned producer. **It is implemented** on the indexed runtime's
existing frame machine.

**What a producer is now.** A `stream` function's body is a continuation, and
the frame engine already models continuations: a frame's work stack *is*
everything the body still has to do, and `yield` is the only place it stops. A
producer call therefore binds its arguments and captures, decodes the body's
statements, and stores them as a suspended frame — a `ScriptProducer` in
`lowered_run/indexed_run/producer.rs` — holding the `Arc<FullProgram>` its code
identity belongs to (so the identity stays valid however far the producer
escapes), the function key and kind, the bound slots and the scopes those slots
belong to, and the frame's work stack, registered `defer` bodies, and open block
scopes.

Each pull installs the producer's program as the evaluator's active one, rebuilds
the machine around that frame (`CallFrame::from_state`), steps it until the body
yields or ends, and puts the frame back. Nothing else interprets a body: there is
no second evaluator, no thread or channel per producer, and no clone of an
evaluator.

**Where the evaluation happens.** Pulls go through the evaluator that is
consuming the stream (`Evaluator::stream_next`), so a producer resumes against
the active evaluator:

- `for item in producer` pulls one item per iteration and re-arms, so the loop
  never holds the whole stream (both the recursive evaluator and the frame
  engine's `FrameWork::ForStream`).
- `take(n)` and `first()` pull only what they keep and then stop the producer;
  `drop` still drains, because dropping has to read past what it drops.
- `.collect()`, `count`, and the remaining terminals drain through the same
  path, and the ported `linux.modules` wrapper now reads its text at the call
  and parses each row as it is consumed, exactly as §3.1 requires.

**Stopping, and `defer` exactly once.** A producer ends when its body runs out
(its defers then run through the frame engine's ordinary completion path), when
a consumer stops it early, or when the program can no longer reach it:

- An early stop (`take`, `first`, `break`, a `return` out of the consuming loop,
  a discarded loop) stops the producer where it stands: the frame engine clears
  the body's remaining work — so no later row runs — and then runs the defers the
  body had registered, exactly once.
- A producer whose body never started has registered no defers and opened no
  scope, so nothing runs.
- The evaluator also holds a registry of live producers. A producer that only
  the registry still reaches can never be resumed, so the evaluator stops it at
  its next sweep (function return, driver step, discarded work); a producer a
  pull currently owns is left alone, which is why the sweep probes with
  `try_lock` rather than blocking.

**Scopes across a suspension.** A suspended producer's scopes are *detached*
from the evaluator's scope stack while the consumer runs and *reattached* for
each pull, in the order they opened. That keeps the invariant the frame engine
relies on — a block is only closed while it is innermost — and keeps resources
the body creates tied to the body's scope rather than to whatever the consumer
was inside when the pull happened.

**What `yield expr?` does now.** The recursive evaluator used to collect a
`Break` from the yielded expression as an item, so `yield failing()?` handed the
consumer an `Err` element (recorded above as a port defect). The producer path
treats that `Break` as what it is — the propagated failure — so the failing
expression aborts the pull and the consumer sees the declared failure.

**Cost of the boundary.** The consumer's work is what it consumes. A release
build over a producer that yields 50,000 rows, one process per row
(`aarch64-apple-darwin`, `time -p`):

| Consumer | 1,000 rows | 8,000 rows | 50,000 rows |
| --- | --- | --- | --- |
| call it and drop the producer | 0.01 s | 0.01 s | 0.01 s |
| `\|> first()` | 0.01 s | 0.01 s | 0.01 s |
| consume every row with a `for` loop | 0.01 s | 0.02 s | 0.07 s |
| five `\|> first()` calls | — | — | 0.01 s |
| five full consumptions | — | — | 0.28 s |

Process startup is about 0.01 s, so the first two rows are the floor: calling a
producer and pulling one row cost the same at 50,000 rows as at 1,000, while
consuming the stream costs in proportion to the rows consumed. That is the
§3.1 requirement that a bounded terminal "must not allocate all remaining output
or perform work beyond the stopping boundary", measured rather than asserted.

**Evidence.** `tests/fixtures/runtime/lazy-stream-producers.xsh` is executed by
`tests/runtime/frontend_indexed.rs::stream_producers_are_lazy_and_stop_where_the_consumer_stops`
and asserts, from the marker files the program writes: the body has not started
when the call returns; the rows come from the text the call retained (the input
file is deleted before consumption); an early stop runs exactly one row and the
defer; a body that never started runs no defer; a `return` out of the consuming
loop stops the producer there; `take`, `break`, and an abandoned producer each
run the defer once; a malformed later row is not reached by the early stop and
does abort a full consumption; and unreadable text fails the call before any row
is interpreted. The public-surface half is
`tests/xsh/stdlib/streams.xsh::test_stream_producers_are_lazy_and_run_defers_on_stop`
(asserting no row exists after the call and exactly one row exists after
`first()`), which runs on both platforms.

**Worker stages.** §3.1 requires that the existing supported worker paths share
these semantics, and they do:
`tests/runtime/frontend_indexed.rs::worker_stages_consume_a_producer_through_the_producer_machinery`
drives `par-map` and the fused `par-map | flat-map | reduce-by` path over a
producer and pins the same contract as any other consumer — the body runs at
consumption, the stage maps the rows the body yielded, a mid-stream failure is
the failure the row declared, and the producer's `defer` runs exactly once
(`closed-map`, `closed-take`, `closed-fuse`, and `closed-check` in the fixture,
with `row-check-three` absent because the producer stopped at the failing row).

What is not shared is the *interleaving*: the parallel stages collect their
input before dispatch (`lowered_pipeline_input_items`), because each worker
takes a partitioned chunk and the stage's own result is a `List` that must hold
every mapped row. So the producer's rows are interpreted by the consuming
thread in stage order, one `Vec` ahead of the workers, and the remaining
difference is a memory profile rather than a behavior: full consumption is
inherent to a stage whose result is a `List`. Producers crossing into the
runtime `Value` representation keep their suspended state, so a stream that
reaches a native consumer which pulls without an evaluator reports that rather
than silently reporting an empty stream.

### Call and stage overhead: profiled, with the copy and allocation removals measured

§5 asks for fewer allocations, decodes, and copies per ordinary operation, with
any improvement shown on complete workloads rather than asserted. This phase
profiled the indexed runtime and removed four concrete costs; this section
records what moved and, just as importantly, what did not.

**How it is measured.** The allocation evidence below comes from
`xsh-runtime-stats`, the existing diagnostics binary, which installs the
counting allocator and reports a run's traffic by phase. Its *controller* stage
covers the calling thread, and the script's statements run on the engine's own
evaluation thread, so execution traffic was previously unattributed; the
evaluation wrapper now records that thread's stage and the report carries it as
`execution` (`src/mem_track.rs`, `src/runtime/eval.rs`, `src/runtime_stats.rs`).
That is a diagnostics change — both calls are inert unless a diagnostics binary
installed the counting allocator — and it is what makes the per-iteration
figures below deterministic instead of sampled.

**Where the time goes.** A release build running three million calls to a
one-line `Result`-returning function through the frame engine (7.4 s, `sample`
over the process) attributes 96% of samples to the frame engine's own step loop
— decoding instruction payloads, matching work items, dispatching expressions —
about 7% to `malloc`/`free` and their zones, 1.7% to `Name` interning, and 1.2%
to runtime type construction. The profile that mattered was finer: inside that
96%, the call path's own metadata decode — `FullFunctionView::header`,
`SemanticPools::to_type_inner`, and the re-interning of parameter names — stood
out as the largest single item, which is what the header cache below removes.

**Removed, with measurements.**

- **Container copies (previous phase).** Record, map, and record-vector payloads
  are `Arc`-backed, read-only receivers borrow, and the proven consuming path
  takes a container out of its slot instead of copying it. Reproduced on
  `json_lines_batch`: 15,648 ms → 1,115 ms (1,113–1,119 ms across this phase's
  measurements), now inside its gate against the native baseline.
- **Frame scratch (this phase).** Every call allocated its work stack, its
  slot-scope list, and its body's statement list, and freed them on return.
  Those three vectors now come from bounded pools on the evaluator and go back
  when the frame finishes (`FrameScratch`,
  `lowered_run/indexed_run/explicit_run.rs`). Measured on the same
  three-million-call probe: **6.93 s → 6.86 s (1.0%)**. On complete workloads
  (`cli_small_schema`, `json_path_ops`, `text_pad_batch`, seven runs each,
  medians) the change is **below run-to-run noise**: 0.480 s, 0.050 s, and
  0.060 s in both configurations.
- **Per-call function-header decoding (this phase).** Every call decoded its
  function's header from the store — parameter names (each one interned again),
  lowered parameter and capture types, validation and default lookups (a binary
  search per parameter), decoded default values, and the return kind — several
  allocations wide, for metadata preparation had already resolved and nothing
  mutates afterwards. `FullProgram` now keeps one `OnceLock<Arc<FunctionHeader>>`
  per function: the first call for a function decodes it, every later call —
  from any evaluator, including the worker evaluators that share the program —
  reads the same `Arc` (`src/runtime/eval/indexed/full.rs`). The cache is inside
  the program, so its identity and lifetime are the program's: it is indexed by
  function, it dies with the program, and the names it holds stay valid because
  the program owns the symbol owner that interned them. A program built without
  slots (the verifier fixtures construct one directly) decodes without caching
  rather than failing. Measured on the same three-million-call probe: **6.67 s →
  4.88 s (27%)**, and — because the probe is one function called in a loop — the
  complete workloads were measured against an otherwise identical binary with
  the cache disabled, one interleaved round of the runner's own sample counts:

  | Workload | Cache disabled | Cache enabled | Delta |
  | --- | --- | --- | --- |
  | `cli_small_schema` | 494.8 ms | **441.1 ms** | -10.9% |
  | `cli_wide_schema` | 905.9 ms | **793.2 ms** | -12.4% |
  | `cli_repeated_parse` | 544.1 ms | **503.1 ms** | -7.5% |
  | `text_wrap_unicode` | 1244.6 ms | **1115.0 ms** | -10.4% |
  | `ini_large_record` | 299.4 ms | **246.0 ms** | -17.8% |
  | `json_path_ops` | 60.9 ms | **53.1 ms** | -12.8% |
  | `text_pad_batch` | 67.7 ms | **58.4 ms** | -13.8% |
  | `core_command` | 79.8 ms | **73.0 ms** | -8.4% |
  | `env_typed_lookups` | 22.7 ms | **20.7 ms** | -8.8% |
  | `mime_batch` | 120.9 ms | **112.3 ms** | -7.2% |
  | `quote_edge_cases` | 49.1 ms | **44.3 ms** | -9.8% |
  | `fmt_batch`, `quote_batch`, `checksum_batch` | 32.8 / 34.0 / 18.5 ms | 31.7 / 33.2 / 17.7 ms | -2% to -4% |
  | `json_lines_batch`, the four cold rows | — | — | within noise (not call-bound) |
  | `native_control`, `native_hash_control` | 24.3 / 134.6 ms | 24.9 / 133.4 ms | no regression |

  This is the second change in this phase that reproduces an improvement on the
  affected *complete* workloads rather than on a probe, and the first that moves
  the batch rows rather than one algorithmic outlier.
- **Statement lists are recycled, not rebuilt (this phase).** A `Statements`
  work item owns its list, and when the list ran to its end the item dropped it
  — so every loop iteration allocated a fresh list for its body and freed it on
  the way out, and so did every call. The handler now returns an exhausted list
  to the bounded pool (`FrameScratch::recycle_statements`), which is what §5
  asks for as "no repeated statement-list allocation per loop iteration". The
  measurement is deterministic rather than statistical, because
  `xsh-runtime-stats` now attributes the *evaluation* thread as well as the
  caller (see below); allocations per iteration, and the same constructs after
  the change:

  | Construct | Before | After |
  | --- | --- | --- |
  | `while index < 300000 { index = index + 1 }` | 3.00 | **2.00** |
  | that loop plus one copying statement | 4.00 | **3.00** |
  | a no-argument call per iteration | 6.00 | **5.00** |
  | a one-argument `Int`-returning call | 10.00 | **9.00** |
  | a one-argument `Result`-returning call | 15.00 | **14.00** |

  Exactly one allocation per completed statement list, removed. Wall-clock on
  the three-million-call probe: **4.84 s → 4.74 s (~2%)**, and the claim is
  pinned by `loop_iterations_reuse_their_statement_list`, which reads the pool's
  own counters: two hundred iterations take their list from the pool one
  hundred and ninety-nine times.
- **The traceback frame's display name (this phase).** Every call rendered its
  function's display name into a `String` — for the traceback frame, and for the
  trace event it also pushed when tracing was off — and every return rendered it
  again for the exit event. The frame now keeps the function's name *text* (two
  interner reads, no allocation) and renders it only where a traceback or a trace
  event asks for it; the enter/exit events are built only when tracing is on.
  Measured on the same probe: **6.96 s → 6.67 s (4.2%)**, and the same
  below-noise result on the workload rows. `TracebackName` in `src/trace.rs` is
  the mechanism; the corpus traceback test pins the rendered text.
- **The eager stream buffer.** The recursive evaluator's `stream_items`
  accumulator and its `yield` arm are gone, and a producer's items are handed
  out one pull at a time (`### Resumable producers`). A consumer that stops
  early no longer interprets rows it never sees: over a 50,000-row producer,
  `first()` costs the same as calling the producer and not consuming it while a
  full consumption costs 0.07 s.

**Implemented, measured, and put back.**

- **A single-return body fast path.** §5 allows a bounded specialization
  "selected from verified body/actual effects and applicable equally to ordinary
  user functions". One was implemented: when the verified body is exactly one
  `return <expr>` — the body shape, read from the store, with no name or source
  text consulted — the frame pushes that expression with the return
  continuation, instead of decoding a one-element statement list and walking it
  through two work items. It was exercised on the probe and on the whole
  workload set against an otherwise identical binary: **4.87 s → 4.57 s on the
  three-million-call probe (-6.2%)**, and **every workload row within noise**
  (the largest apparent move was +7.8 ms on `text_wrap_unicode`, inside that
  row's own spread). The shape is real — it just does not occur in the calls the
  designated workloads spend their time in, which are large multi-statement
  bodies. It was removed by that measurement, for the same reason as the
  statement cache below: a second route through call setup that moves nothing
  the gates measure is not worth owning.

- **Decoding each block's statement list
once per program and handing out a shared slice — the plan's "borrow immutable
instruction/block ranges with an index rather than allocate/reverse a `Vec` on
every block iteration" — was implemented and then reverted: the two-level cache
lookup and the shared work item cost about what the vector it replaced cost, so a
controlled A/B over the workload set (`cli_small_schema` 0.490 s,
`json_path_ops` 0.060 s, `ini_large_record` 0.280 s, and three others, medians of
five runs) was **identical to two decimals in both configurations**. The
measurement is what removed the change; the decoder keeps its pop order and its
documentation says so.

**Cumulative.** The call-path removals measured together — container copies,
frame scratch, the traceback name, the per-call header decode, and statement-list
recycling — take the three-million-call probe from 7.42 s (the tree before this
phase's runtime work) to **4.74 s, about 36%**. Split by what they moved: container copies are what
made `json_lines_batch` (14×) and the `Map`/`Record` update paths linear; frame
scratch and the traceback name are 1.0% and 4.2% of the probe and below noise on
the workloads; the header cache is 27% of the probe and 7–18% on twelve complete
workloads; the recycling is 2% of the probe and 1–7% on the loop-heavy rows, with
one allocation per completed statement list removed. The plan's "reproduced improvement on the affected complete
workloads" is satisfied by two of the four — the container copies and the header
cache — and the measured null results for frame scratch and the traceback name
are recorded above rather than dressed up. The next paragraph is what remains.

**What is left.** The header cache removed the per-call metadata decode, so the
remaining distance is the dispatch loop itself: an interpreted step costs about
0.7 µs and a call about 1.4 µs, and the ported entries execute tens of steps per
item where a native helper executed a few instructions. Closing that needs a
different execution structure — an instruction cache, or a specialization over
the *bodies* the workloads actually run rather than over their call setup, which
is the only shape §5's constraints leave open (verified bodies and actual
effects, applicable equally to ordinary user functions, no stdlib-name or
workload-shape recognition, signals and cancellation preserved). The
specialization this phase implemented and measured — the single-return body
shape — closed no workload row, and the eighteen failing rows above are the
measure of what remains. No native or
control row regressed: `native_control` and `native_hash_control` are within
1.5 ms of the reference in every measurement.

The cold rows carry a second, separate charge, and the phase profile under
`### What the failures are` measures it: two thirds of their cost is lowering
and verifying the embedded bodies before execution, a quarter is parsing them.
Unlike dispatch, that is *one-time* work with a bounded target — the profile
shows it is 7.49 ms for the whole catalog and 3.81 ms for the largest module —
and nothing in the fixed decisions forbids making the lowering and verification
passes cheaper; what they forbid is replacing them with a frozen image or a
persistent cache. So the honest ordering of remaining work is: the interpreted
dispatch first (it dominates the eighteen rows), then the lowering and
verification passes for the four cold rows, and the measurement above is the
baseline either change starts from.

## Defects found and fixed during the port
- **The `no-default-features` build stopped compiling.** `lowered_bytes_arg_or_empty`
  in `src/runtime/eval/lowered_run.rs` was gated behind `native-tests`, and the
  `BridgeAppendBytes` arm added in this phase is production code that calls it,
  so `cargo check --no-default-features` failed with the helper "not found in
  this scope". The gate is removed: the helper is a production arm's argument
  reader. Verified by `cargo check --no-default-features` (which now compiles,
  with one pre-existing unused-dependency warning for `crossbeam-channel` when
  the `net` feature is off) and by the default-feature build and suite, which
  are green.
- **An optional value cannot be pattern-matched.** `T?` declares as an optional
  but its runtime value is a `Result`, so `Ok`/`Err` arms are rejected by the
  checker ("constructor patterns require a Result value") while a bare `null`
  arm is accepted by the checker and then rejected by the lowerer as a
  `full_ir_function_blocker`. Only `== null` and `??` observe an optional.
  `stdlib/linux_module.xsh` therefore reports an entry's position or `-1`
  rather than an entry-or-null, so the value it matches is a plain `Int`.
- **A method name the checker cannot resolve fails at run time, not at check
  time.** `Str.trim_end_matches` does not exist, but `described[i].trim_end_matches(")")`
  checked cleanly and then raised `missing-field: trim_end_matches` at run
  time. The embedded modules now strip trailing characters with `byte_at` and
  `byte_slice`.
- **`yield expr?` hands the consumer the `Err`, it does not fail the stream.**
  *(Fixed in the follow-up phase: the producer path treats the yielded
  expression's failure as the propagated failure it is, so a failing
  `yield ...?` aborts the pull and the consumer sees the declared error.)*
  A stream element is the yielded value, so a failing `?` in a `yield` position
  produced an element that is a `Result` — the consumer binds it and the next
  field access fails with `missing-field`. The same `?` in a `let` inside the
  stream body propagates and aborts the producer. Found by a fixture where a
  device's attribute file was missing: the baseline failed the call and the
  first version of the rfkill inventory's producer produced records with no
  fields.
  Every stream producer in this port binds first and yields second.


- **A script stream materializes its items when the producer runs, and a
  per-item failure has nowhere to live.** *(Fixed in the follow-up phase: a
  producer's body is a suspended frame now, so a call reads nothing and a pull
  runs one row at a time. See `### Resumable producers: the producer body is a
  suspended frame`.)* `eval_indexed_stream_producer` ran the producer body and
  handed `StreamValue::from_values` its items, so
  attribute or file reads inside a producer happen at the call rather than at
  consumption, and a failing `?` there returns the `Err` as the producer's
  *result*, which the caller sees as `stream producer returned Result`. The
  embedded implementations therefore read eagerly and report the baseline's own
  kind and message from the call. The residual difference — a consumer that
  stops early still pays for reads the baseline would not have reached, and a
  raised failure renders embedded frames in its traceback — is recorded under
  `## Coverage limits`.
- **The resolved-function index was cached without its program.** The
  evaluator's `indexed_function_cache` was keyed by `(function, kind)` alone.
  One evaluator resolves the same qualified key against more than one program
  once a dynamically loaded module links its standard calls to the loading
  program's prepared implementations — `<xsh-stdlib:hash> verify_file` names a
  function in both — so the second program's lookup returned the first
  program's index and `function_view_at` failed its `expect`, aborting the
  process. The entry now carries the program it was resolved in and compares by
  pointer identity, which also keeps that program alive so its identity cannot
  be reused while the entry is cached. Found by a loaded module calling
  `hash.verify_file`; `a_loaded_module_calls_prepared_implementations` is the
  regression test.

- **`Result[Any]` swallowed its own `Err`.** `lowered_return_value` matched the
  generic "value fits the declared ok type" arm before the error arm, and `Any`
  fits every value, so an embedded implementation returning `Err` had it wrapped
  back into `Ok` and the caller never saw the failure. A program could not
  `match` on a rejected `json.set`, and `json.get(...)?` continued instead of
  propagating. The error arm now precedes the generic match. This affected every
  `Result[Any]`-returning script-backed entry, not only JSON.
- **Record literals were `RecordVec`, not `Record`.** The representation bridges
  accepted only `LoweredValue::Record`, so `json.set({a: 1}, ["b"], 2)` failed
  even though the record was a record. The bridges now update whichever of
  `Record`, `RecordVec`, `Module`, or a materialized filesystem entry they are
  given and return that same shape.
- **The catalog gate validated embedded sources as user programs.** It parsed
  each source as an *entry*, so a module was checked twice — once as user
  source under user rules and once as an implementation — and a module whose
  exported function shares a name with a standard module failed the user-source
  shadow rule. `loader::prepare_stdlib_catalog_module` now prepares each module
  the way the runtime sees it: attached to an otherwise empty program as an
  internal implementation module.

## Coverage limits recorded for R12, G04, and G05

G04, G05, and G07 are native again, so nothing below limits the shipped code.
It limits the *parity* half of their §12.3 evidence: the measurements that
removed those prototypes were taken on implementations whose behavior had been
compared, and the comparison did not reach every branch. Where a case was not
reachable, the revert stands on the benchmark alone.

The Linux-live tests assert policy invariants against the real read-only
`/proc` and `/etc/os-release`; they deliberately do not assert exact values.
Fixture-driven cases that need a specific file body were **not covered by any
committed test** when this section was written: the internal namespace is
deliberately unrepresentable from XSH source, so the private helpers could not
be called from a test, and no switch was added to make them reachable — that was
the point. Coverage lived only in a throwaway harness that rewrote the fixed
path literals in a copy of the source, which is evidence for this report and not
a regression gate.

**Most of this gap is now closed** by the crate-private catalog companion
described under `### Fixture coverage for the host-text policy`: os-release
quoting, escaping, and last-wins, the record defaults, missing-key order and
malformed-value precedence, saturation at both `Int` bounds, the accepted
decimal spelling, `linux.modules` record fields and the four row failures, blank
line retention, and `/proc/uptime` reading all have committed, disk-backed
tests. The *wrapper* level is closed too:
`tests/stdlib_port.rs::os_release_entry_reads_the_fixed_paths` runs the public
entry against `/etc/os-release` and `/usr/lib/os-release` with container-owned
fixtures staged at those paths, covering which path is read first and how a
failed read of both is reported for the whole call (its three scenarios and
their invocations are in `bench/stdlib-port/README.md`). `linux.modules` partial
consumption and the delayed malformed-late-row failure are covered by the
producer contract tests, which exercise the same shape the Linux wrapper has: a
call-time read and consumption-time interpretation.

G05's two ported inventories are exercised through constructed trees in the
container rather than through committed tests, for the same reason: the paths
are fixed, so a test cannot point them at a fixture, and the container is the
only place a `/sys/class/rfkill` or `/sys/block` tree can be built. The runs
that were made are recorded in `## Linux verification`, and they are evidence
for this report rather than a regression gate. A committed test would need the
same bind-mount harness.

G04 has the same gap for the same reason. The route parsers read fixed paths
and are private to the embedded module, so the malformed-row, addressing,
prefix, default, and out-of-range cases cannot be driven from XSH source. The
Linux run recorded above compares them against the native baseline on the
container's real `/proc`, which covers the rows that container has — in practice
an IPv4 default and connected route and IPv6 local routes — but not a
constructed fixture. A fixture would need either the same throwaway rewrite
harness or a bind mount over `/proc/net/route` inside the test container.

- Calling a **parameterless** `stream` function failed at run time with
  `lowered function did not return`, from any caller: a `for` loop, a `let`
  binding, or a `return`. A stream function with at least one parameter was
  unaffected, which is why this survived the existing corpus — every stream
  producer in `core/` and `tests/` took one. The reference build reproduces it
  with plain user code:

  ```xsh
  stream items() [] -> Stream[Int] {
    yield 1
  }

  proc main() [io, error] {
    for item in items() {
      print f"got=${item}"
    }
  }
  ```

  This was first reached by the first embedded stream producer written for this
  port, which took no parameters: the Linux test failed with it in the container
  while passing on macOS, where the entry kept its native binding. Every stream
  producer in the catalog takes one today because that was the workaround.

  **Fixed in the follow-up phase.** The defect is in the explicit-frame engine,
  not in lowering: `ExplicitFrames` resolved a call with arguments by walking
  the argument list and checking whether the callee is a producer, but a call
  with *no* arguments reached its own arm and pushed an ordinary frame. The
  body then ran with nowhere for `yield` to report and `finish_call` reported a
  function that did not return. Both paths now share `push_resolved_call`, which
  makes that decision once. The minimized program is committed as
  `tests/fixtures/runtime/zero-argument-stream.xsh`, executed by the Rust case
  above and by the corpus case of the same name.

## Open items for the repository owner

- Ten files need the repository formatter, reported by `xsht fmt --check`:
  `tests/xsh/stdlib/cli.xsh`, `env.xsh`, `ini.xsh`, `json.xsh`, `linux.xsh`,
  `mime.xsh`, `process.xsh`, `text.xsh`, `tui.xsh`, and `unix.xsh`. This change
  ran no formatter on any of them; three earlier files were formatted by hand
  to the tool's own output, and the rest are left to the owner as
  `AGENTS.md` requires. The repository's pre-existing unformatted file is
  `dev/tests/test-targets.xsh`, unchanged. `stdlib/**/*.xsh` and
  `bench/**/*.xsh` are excluded in `xsht-config.ini`, which is why the
  embedded sources and the workload scripts are not in that list.
- `xsht check` and `xsht lint` over the whole repository produce output
  identical to the reference (compared as sorted line sets with the two
  checkout paths normalized) and exit 0 and 1 respectively; the lint exit is
  pre-existing and unchanged. The six warnings behind it are all
  `lint.multiline-tag-union` in `dev/*.xsh`. Getting there took removing
  twenty-two warnings these tests had introduced, described under
  `## Tooling cost recorded for §3.6`.

- **`tests/fixtures/modules/standard-modules.txt` is unused.** It is a
  human-readable surface listing from an earlier tooling generation; nothing in
  the tree reads it, and `crates/xsht/tests/api.rs` compares against the
  machine-readable `standard-api-surface.jsonl` instead. It is left in place
  because removing it is the owner's call.

## Pre-existing failures and skips

- Baseline `xsht test --jobs 1 tests/xsh/stdlib`: 122 passed, 0 failed,
  17 skipped. Candidate on macOS: 186 passed, 0 failed, 25 skipped. Every added
  skip is a Linux-only case that runs in the Linux verification above; the
  added passes are the R01-R11 parity cases and the G02 and module-tree policy
  cases, which pass on both revisions.
- `cargo test --test integration`: reference 368 passed / 130 failed /
  27 ignored; candidate 383 passed / 130 failed / 27 ignored. The two failure
  sets were compared as sorted name lists and are **identical**, so all 130 are
  pre-existing and environmental: `tests/runtime/common.rs` cannot resolve the
  workspace profile directory in this checkout (`profile
  \`<hash>\` is not defined`), which fails every test that needs a product
  binary. The 15 extra passes are this port's architecture tests and the
  prepared-implementation cases. The same comparison was run in release mode,
  which the developer suite's `test-rust` stage uses: reference 361 passed /
  137 failed / 27 ignored, candidate 376 passed / 137 failed / 27 ignored, sets
  **identical** again. The seven release-only failures are all
  `runtime::stack_depth::small_stack_*`, which run the same bodies on a small
  stack and fail on the reference too. The container's own run of the suite is
  compared the same way under `## Linux verification`.
- After the harness repair (`### The integration suite executes again`) the
  candidate's `cargo test --test integration` reports **519 passed, 0 failed,
  27 ignored**, and the whole workspace **1,039 passed, 0 failed, 29 ignored**.
  Nothing in either set is skipped for being unavailable: the ignored cases are
  the suite's own `#[ignore]` declarations — 27 of them in `tests/` and two in
  `src/` (this phase's cold-start phase-profile diagnostic is one), all
  documented in place as flaky, PTY-driven, long-running, or quality-only — and
  the Linux-gated ones run in the container.
- The two `net` timing tests (`native_xsh_net_job_progresses_while_synchronous_request_waits`
  and `net_module_request_many_refills_the_window_on_first_completion`) fail
  intermittently when the suite runs under load — they pass in isolation and
  passed in the unloaded full runs recorded here, and during this phase they were
  observed failing in roughly a third of full-suite runs that overlapped a build
  or a benchmark. They are the same class as the entry below: TCP barriers whose
  timing the machine's load decides, unrelated to the standard library. The
  final verification runs show both faces of it: the suite run that overlapped
  nothing reported 518/518 and 1,036/1,036, while an earlier run of the same tree
  that began while another test binary was still finishing reported
  `net_module_request_many_refills_the_window_on_first_completion` failing on a
  barrier read plus `native_xsh_net_batch_download_error_contract` failing with
  `net-io: connection establishment failed` — a second environmental case in the
  same file, where the local listener could not be reached at all.
- Two of the six debug captures of that suite reported one *extra* failure, a
  different test each time — `runtime::modules::net_module_request_many_refills
  _the_window_on_first_completion` once and
  `stdlib_port::a_loaded_module_calls_prepared_implementations` once. Neither
  reproduced: the first is a timing test over a refill window, and the second
  passed in twelve isolated runs of its file and in the two full-suite runs
  after it. The counter assertion that could report it now prints the exit
  status, stdout, and stderr of the run whose count it is comparing, so the next
  occurrence identifies itself instead of just failing.

## Known baseline defects recorded during the port

- `syntax::arena::doc_comments_for_statements` derives its lexer source id from
  the first statement of the range it is given. A range with no statements
  therefore lexes against `SourceId(0)`, attributing that source's leading
  comments to whatever file happens to be first in the map. The port avoids the
  defect for embedded sources by not attaching user doc comments to them; the
  underlying behaviour is otherwise unchanged.
- Rebinding a name that an enclosing scope already declared is miscompiled: the
  inner binding is initialized once, from the outer binding's current value, and
  every later assignment to it writes a binding nothing reads, so the name keeps
  the outer value. Two loops that both bind `byte` therefore never see the inner
  value, and the outer loop never sees the inner assignments. `check.duplicate-name`
  rejects two bindings in one scope but not this nested case, so the program
  checks and lowers cleanly. The port sidesteps the defect: every binding in
  `stdlib/process.xsh` has a unique name inside its function. Minimal
  reproduction (a plain user script; `shadowed` prints `ab cd;`, `distinct`
  prints `ab;cd;`):

  ```xsh
  pure shadowed(text: Str) -> Str {
    var out = ""
    var index = 0
    while index < text.byte_len() {
      let byte = text.byte_at(index, -1)
      if byte == 32 {
        index = index + 1
        continue
      }
      var word = ""
      while index < text.byte_len() {
        let byte = text.byte_at(index, -1)
        if byte == 32 {
          break
        }
        word = word + text.byte_slice(index, 1)
        index = index + 1
      }
      out = out + word + ";"
    }
    return out
  }
  ```

  Renaming the inner `byte` to `inner_byte` makes the same function print
  `ab;cd;`.
