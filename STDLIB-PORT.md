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
| R12 Linux text policy | ported (Linux), native on macOS | Linux `system.os_release`, `system.memory`, `linux.meminfo`, `linux.modules`, `unix.uptime_seconds` | `stdlib/system.xsh`, `stdlib/linux_text.xsh`, `stdlib/unix.xsh` | target-aware: the Linux binding is `script_sig`, macOS keeps `sig`. Native bodies are live on macOS and unreachable on Linux; they are not deleted because the file must still build and behave on macOS |
| G01 git-root discovery | retained-boundary | `fs.gitroot` | — | `xsh::host::fs::gitroot` is a public Rust façade helper consumed outside language execution by `crates/xshi/src/interactive/session.rs:328,416` for the git prompt. §6's own first check applies: keep one shared native implementation rather than adding callbacks or a duplicate script algorithm |
| G02 file-checksum policy | not evaluated | `hash.verify_file` | — | `verify_hex` has no native caller outside the two runtime dispatch arms, so no static obstacle was found; a behaviorally faithful prototype and paired measurements were not produced |
| G03 JSON/INI file-IO composition | retained-boundary (INI write ported) | `ini.write`, `json.read`, `json.write`, `json.write_lines` | `stdlib/ini.xsh` | the INI write composition moved with R09. The JSON file wrappers are the smallest useful host adapters: each is an open/read/decode or encode/open/write sequence whose only non-host step is a single call into the retained codec, so a script shim would add an interpreter boundary and an equal-sized Rust adapter instead of deleting policy |
| G04 read-only route interpretation | not evaluated | `linux.routes` | — | requires the Linux Docker verification route, which was not exercised |
| G05 read-only sysfs inventories | not evaluated | `linux.interfaces`, `linux.rfkill_list`, `linux.block_devices` | — | requires the Linux Docker verification route, which was not exercised |
| G06 read-only disk-usage presentation | not evaluated | `linux.disk_usage` | — | requires the Linux Docker verification route, which was not exercised |
| G07 kernel-module query and index output | not evaluated | `linux.modinfo`, `linux.depmod` | — | requires the Linux Docker verification route and a fixture modules tree, which were not exercised |

A placeholder source is a valid, empty embedded module: while a port is in
progress its entries keep their native route, so the tree stays green. Removing
the native body is what makes the implementation binding authoritative.

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

## Architecture tests

`tests/stdlib_port.rs` holds the boundary tests; `src/stdlib.rs` holds the
catalog tests.

| ID | Evidence |
| --- | --- |
| A01 | `preparation_is_proportional_to_referenced_standard_entries` — a trivial program prepares zero embedded modules; one script-backed call prepares one; an unrelated entry prepares none |
| A02 | `execution_does_not_prepare_embedded_modules` — 200 calls in a loop and a failing run each leave the preparation count at one |
| A03 | `repeated_references_parse_an_embedded_module_once` — two entries of one module, and a reference reached through a loaded user module, parse it once |
| A05 | `dynamic_loading_prepares_the_complete_set_before_execution` — a `module.load` reference prepares the whole applicable set before execution and prepares nothing more during the run |
| A06 | `copied_binary_runs_migrated_apis_without_repository_files` — a copied binary runs migrated APIs from a clean cwd |
| A07 | `standard_implementations_cannot_be_replaced` — hostile module roots and a module named after a standard module cannot replace an implementation, while an explicitly loaded hostile module keeps ordinary user semantics |
| A08 | `copied_embedded_source_grants_no_private_access` — a user file whose contents are a copy of an embedded module gains no private access |
| A09 | `private_implementation_helpers_are_not_nameable` — a user reference to an embedded private helper prepares nothing and fails; `user_functions_cannot_impersonate_a_representation_bridge` — a user function spelled like a bridge stays an ordinary user function |
| A10 | `same_spelled_user_helpers_cannot_capture_implementation_helpers` — user declarations spelled like embedded helpers stay user bindings |
| A04 | `crates/xshi/tests/stdlib_preparation.rs` — each submitted input crosses its own preparation boundary in one process, repeated submissions keep working, and a failed submission does not poison the next |
| A11, A12 | `implementation_namespace_is_unspellable_and_reserved_names_still_work` — the internal namespace is not parseable, standard module names stay reserved, and the documented `error` binding exception still works |
| A13 | `prepared_implementations_read_context_at_invocation_time` — a scoped environment overlay is observed at each call, never snapshotted at preparation |
| A18 | `every_catalog_module_parses_checks_and_lowers` — every embedded source, used or not, runs the full production preparation gate |
| A20 | `implementation_namespaces_never_appear_in_the_public_registry` — no internal namespace or label is a public module, function, method, or record name |

Not yet implemented as automated evidence: A14-A17, A19. Their status is recorded in the final report rather than claimed
here.

## Final status

Incomplete. Every completed group is ported, tested, and has its native body
removed, but the task is not finished:

- **R02 (CLI policy)** is not started. It is the largest required group and
  needs a runtime-representation bridge for schema-shaped record construction
  that does not exist yet.
- **R10 (JSON path policy)** is not started for the same reason on its
  `set`/`remove` paths.
- **R12 (Linux text policy)** has its implementations written and validated
  through the production preparation gate, but macOS runs the retained native
  bindings, so the ported behavior has been neither executed nor measured.
  Its native bodies are therefore not deleted.
- **G02 and G04-G07** are unresolved; **G01** is a documented boundary.
- Architecture test A17 (identical behavior under debug and release, across
  feature combinations and both platforms) has no dedicated test; it is
  covered for debug on macOS by every suite above and for Linux by the R12
  container runs, but no single test compares the same case across all of
  them.
- Performance acceptance fails for the dynamic-reference cold start and for
  every bounded helper batch; see `## Performance`.

## Performance

Measured with paired, interleaved reference/candidate runs on matched release
builds (`lto = "thin"`, `opt-level = 3`) on `aarch64-apple-darwin`. The
reference is an immutable worktree of the starting revision. Raw samples are in
`bench/stdlib-port/results-compact-index.json`; the runner is
`bench/stdlib-port/run.py`.

Cold startup (`C - B <= max(0.05 * B, 1.0 ms)`):

| Workload | Reference | Candidate | Delta | Result |
| --- | --- | --- | --- | --- |
| trivial script, no migrated API | 9.769 ms | 9.032 ms | -0.736 ms | pass |
| one `shlex.quote` call | 11.184 ms | 10.712 ms | -0.472 ms | pass |
| one `tui.left_pad` call | 11.254 ms | 10.815 ms | -0.439 ms | pass |
| script with a `module.load` reference | 11.922 ms | 23.753 ms | **+11.831 ms** | **fail** |

Non-hot end-to-end (`C - B <= max(0.10 * B, 2.0 ms)`):

| Workload | Reference | Candidate | Delta | Result |
| --- | --- | --- | --- | --- |
| native control (`fs.dirs` walk) | 28.241 ms | 27.306 ms | -0.935 ms | pass |
| 2,100 ANSI-aware pads | 29.036 ms | 73.739 ms | +44.703 ms | fail |
| `shlex.join` over a 1,000-word argv | 15.351 ms | 33.601 ms | +18.250 ms | fail |
| 4,000 size/duration formatter calls | 14.773 ms | 31.933 ms | +17.160 ms | fail |
| 500 `hash.parse_check_line` calls | 13.159 ms | 18.744 ms | +5.585 ms | fail |

### The dynamic-reference cold start

A program that merely *references* `module.load` must prepare the whole
applicable embedded set before execution (§3.4). That workload is the only
failing cold case, and its cost is the cost of preparing the complete catalog:
measuring the phases inside the candidate put parsing at 4.2 ms, declaration
and body checking at 2.2 ms, and indexed-IR construction at 10.7 ms for the
fifteen embedded modules.

Indexed-IR construction was quadratic when this row was first measured. Two
probes rebuilt `compact_function_defs` — a walk of every statement of every
module — once per function: the direct-pure-call candidate test and the
dependency/component lookup each unit needs. `CompactFunctionIndex` now builds
that walk once per lowering pass and answers all three, and the
per-function `top_level_known` prefix scan became one cursor per module. Those
two changes took this workload from +54.395 ms to +11.831 ms; the phase
breakdown above is from the same instrumented build.

The residue is the mandated preparation itself, not a defect that can be tuned
away inside the agreed architecture: 6,393 lines of embedded source are parsed,
checked, lowered, and verified in every process that can reach `module.load`.
Excluding the dynamic-loading rule, the same process prepares nothing
measurable — the three ordinary cold workloads are *faster* than the reference,
because their embedded implementations are prepared on demand by entry
reference rather than by breadth.

### The end-to-end batches

The four failing end-to-end workloads are per-call interpreter cost, and the
call count alone decides three of them. A calibration in the same build
measures about 0.67 µs per interpreted loop iteration and about 1.7 µs per
interpreted function call, against nanoseconds for the native helper each
replaces:

| Workload | Script-backed calls | Call cost alone | Budget |
| --- | --- | --- | --- |
| 2,100 ANSI-aware pads | 2,100 | ≈ 3.6 ms | 2.9 ms |
| `shlex.join` over 1,000 words | 1,000 | ≈ 1.7 ms + the join | 2.0 ms |
| 4,000 formatter calls | 4,000 | ≈ 6.8 ms | 2.0 ms |
| 500 checksum-line parses | 500 | ≈ 0.9 ms | 2.0 ms |

For the formatter batch the floor is more than three times its whole
allowance before a single line of implementation runs, so no faithful XSH body
can pass it. The checksum batch is the one case where the call count leaves
room, and its body is already kernel-shaped — `translate` for the hexadecimal
test, `find` for the separator, `byte_slice`/`lower` for the fields — but the
remaining ~18 kernel and helper calls per row still cost about 7 µs.

These deltas are therefore not implementation defects to be tuned away within
the agreed architecture; they are the cost of moving per-call algorithms from
compiled Rust into the interpreter. Retained native fast paths (the control
workload) and project tooling are unaffected.

One runtime property is recorded here because it bounds any XSH implementation
rather than this port's: evaluating a `Record` or `Map` as a method receiver
clones the whole container, and `lowered_freeze_large_slot_list` freezes `List`
only, so n reads over an n-key record cost Θ(n²) however the XSH source is
written. The embedded INI encoder is linear in XSH operations and still
measures ~271-375 ms for a 1,000-key multi-section record. A linear encoder
needs a runtime change (freeze large record/map slots, or a bulk
`Record.values()`), not an XSH change, so no INI workload is in the fixed set
and none is claimed.
## Tests

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
- `src/stdlib.rs` unit tests — catalog/registry agreement in both directions.
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

Counted with `git diff --numstat` against the starting revision, production Rust
only (`src/`, `crates/*/src`):

| | Lines |
| --- | --- |
| Production Rust added (tracked files) | 1,071 |
| Production Rust deleted (tracked files) | 1,280 |
| New production Rust file (`src/stdlib.rs`, untracked) | 413 |
| Net production Rust change | **+204** |
| XSH implementation added (`stdlib/*.xsh`) | 2,820 |
| Deleted native owner files | `src/modules/mime.rs` (283), `src/modules/shlex.rs` (77) |
| Test and doc churn (`tests/`, `docs/`) | +1,099 / -0 |

The net is still positive because the one-time preparation, binding, lowering,
and verification machinery lands in this change while three required groups
(`R02` CLI, `R10` JSON path, `R12` Linux) are unfinished and keep their native
bodies. Completing them is what turns the net negative; the infrastructure cost
does not recur per group.

## Linux verification

R12 was executed on Linux, not only prepared: built in `rust:alpine` and run
against `alpine:3.24.1` with the repository mounted read-only.

- `tests/xsh/stdlib/linux.xsh` 6 passed / 0 failed / 0 skipped
- `tests/xsh/stdlib/system.xsh` 3 passed / 0 failed
- `tests/xsh/stdlib/unix.xsh` 5 passed / 0 failed
- Full `tests/xsh/stdlib/` on Linux: 170 passed, 3 failed, 16 skipped

The three Linux failures are `fs.xsh` (`fs-root-mkdir`, `fs-root-symlink`,
`Bad file descriptor (os error 9)`), a running-as-root-in-a-container artifact.
`tests/xsh/stdlib/fs.xsh` exercises no script-backed entry — the whole `fs`
module is retained native — so the port cannot have produced them. They were
not reproduced on the reference, so they are recorded as environmental and
unattributed rather than as a pass.

## Defects found and fixed during the port

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

## Coverage limits recorded for R12

The Linux-live tests assert policy invariants against the real read-only
`/proc` and `/etc/os-release`; they deliberately do not assert exact values.
Fixture-driven cases that need a specific file body (os-release quoting and
last-wins rules, the `/usr/lib/os-release` fallback and its failure message,
missing-required-key orders and messages, saturation at both `Int` bounds,
`linux.modules` record fields and partial consumption, and the delayed
malformed-late-row failure) are **not covered by any committed test**. The
internal namespace is deliberately unrepresentable from XSH source, so those
private helpers cannot be called from a test, and no switch was added to make
them reachable — that was the point. Coverage for them lives only in a
throwaway harness that rewrites the fixed path literals in a copy of the
source; it is not part of the repository, so it is evidence for this report and
not a regression gate. Closing this gap needs an internal test companion
reachable through the crate-private catalog, which is future work.

## Open items for the repository owner

- Eight files need the repository formatter: `tests/xsh/stdlib/env.xsh`,
  `ini.xsh`, `linux.xsh`, `mime.xsh`, `process.xsh`, `text.xsh`, `tui.xsh`,
  `unix.xsh`. This change did not run any formatter. The repository's
  pre-existing unformatted file is `dev/tests/test-targets.xsh`, unchanged.
- `xsht check` and `xsht lint` over the whole repository match the reference
  exactly (exit 0 and exit 1 respectively, the latter pre-existing).

## Pre-existing failures and skips

- Baseline `xsht test --jobs 1 tests/xsh/stdlib`: 122 passed, 0 failed,
  17 skipped. Candidate: 165 passed, 0 failed, 24 skipped. Every added skip is
  a Linux-only case that runs in the Linux verification above.
- `cargo test --test integration`: reference 368 passed / 130 failed /
  27 ignored; candidate 379 passed / 130 failed / 27 ignored. The failure sets
  are byte-identical and were verified as such in both directions, so all 130
  are pre-existing and environmental: `tests/runtime/common.rs` cannot resolve
  the workspace profile directory in this checkout (`profile
  \`<hash>\` is not defined`), which fails every test that needs a product
  binary.

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
