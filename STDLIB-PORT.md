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
| R12 Linux text policy | ported (Linux), native on macOS | Linux `system.os_release`, `system.memory`, `linux.meminfo`, `linux.modules`, `unix.uptime_seconds` | `stdlib/system.xsh`, `stdlib/linux_text.xsh`, `stdlib/unix.xsh` | target-aware: the Linux binding is `script_sig`, macOS keeps `sig`. Native bodies are live on macOS and unreachable on Linux; they are not deleted because the file must still build and behave on macOS |
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
| A05 | `dynamic_loading_prepares_the_complete_set_before_execution` — a `module.load` reference prepares the whole applicable set before execution and prepares nothing more during the run; `a_loaded_module_calls_prepared_implementations` — a loaded module's standard calls reach those prepared implementations |
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
are deleted. R12 (Linux text policy) executes on Linux and is verified there;
macOS runs the retained native bindings by design, so its native bodies stay.

What is not finished:

- **Performance acceptance fails.** Eighteen of the twenty-four designated
  workloads exceed their gate, spanning the cold-start, CLI, text, quoting,
  MIME, INI, JSON, environment, checksum, and tooling classes; see
  `## Performance` for the per-row numbers and the two causes. §12.3's rule for
  a mandatory group that fails is that it "remains incomplete until corrected
  within the agreed architecture", and the smallest outstanding issues are
  named there: a runtime bulk accessor for `Record`/`Map` (the INI quadratic)
  and, for everything else, interpreted per-call cost that no XSH implementation
  of these algorithms can remove.
- **Gated groups.** **G02** is ported and measured inside its gate. **G01**,
  **G03**, **G06**, and G05's interface inventory are documented boundaries with
  their call graphs. **G04**, **G05**'s rfkill and block-device inventories, and
  **G07** were ported, measured per group, failed their gates, and were removed
  and retained natively under §12.3, with the measurements in `## Performance`.
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
`linux-priv-tests` feature. `cargo test -p xsh --lib` (168 passed) covers the
catalog against the registry in both directions under the default features.

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
deterministically by the runner rather than committed.

| Workload | Class | Reference | Candidate | Delta | Budget | Result |
| --- | --- | --- | --- | --- | --- | --- |
| `cold_trivial` | cold | 9.262 ms | 8.700 ms | -0.562 ms | 1.000 ms | pass |
| `cold_quote` | cold | 10.714 ms | 10.340 ms | -0.374 ms | 1.000 ms | pass |
| `cold_pad` | cold | 10.720 ms | 10.443 ms | -0.277 ms | 1.000 ms | pass |
| `cold_cli_parse` | cold | 11.084 ms | 19.502 ms | **+8.418 ms** | 1.000 ms | **fail** |
| `cold_cli_usage` | cold | 11.252 ms | 18.003 ms | **+6.751 ms** | 1.000 ms | **fail** |
| `cold_cli_error` | cold | 11.135 ms | 17.679 ms | **+6.544 ms** | 1.000 ms | **fail** |
| `cold_dynamic_ref` | cold | 11.874 ms | 23.321 ms | **+11.447 ms** | 1.000 ms | **fail** |
| `cli_small_schema` | CLI | 16.573 ms | 507.673 ms | **+491.101 ms** | 2.000 ms | **fail** |
| `cli_wide_schema` | CLI | 18.982 ms | 923.505 ms | **+904.523 ms** | 2.000 ms | **fail** |
| `cli_repeated_parse` | CLI | 16.099 ms | 574.309 ms | **+558.210 ms** | 2.000 ms | **fail** |
| `text_wrap_unicode` | text | 165.001 ms | 1246.919 ms | **+1081.917 ms** | 16.500 ms | **fail** |
| `text_pad_batch` | text | 31.042 ms | 70.305 ms | **+39.263 ms** | 3.104 ms | **fail** |
| `fmt_batch` | text | 15.582 ms | 32.112 ms | **+16.530 ms** | 2.000 ms | **fail** |
| `quote_batch` | quoting | 15.270 ms | 33.116 ms | **+17.846 ms** | 2.000 ms | **fail** |
| `quote_edge_cases` | quoting | 18.160 ms | 44.019 ms | **+25.859 ms** | 2.000 ms | **fail** |
| `mime_batch` | MIME | 36.695 ms | 116.593 ms | **+79.898 ms** | 3.670 ms | **fail** |
| `ini_large_record` | INI | 20.120 ms | 279.659 ms | **+259.539 ms** | 2.012 ms | **fail** |
| `json_path_ops` | JSON | 21.817 ms | 65.119 ms | **+43.302 ms** | 2.182 ms | **fail** |
| `json_lines_batch` | JSON | 15639.600 ms | 15648.233 ms | +8.633 ms | 1563.960 ms | pass |
| `env_typed_lookups` | env | 16.063 ms | 21.743 ms | **+5.680 ms** | 2.000 ms | **fail** |
| `checksum_batch` | checksum | 11.772 ms | 17.584 ms | **+5.811 ms** | 2.000 ms | **fail** |
| `core_command` | tooling | 11.666 ms | 77.703 ms | **+66.036 ms** | 2.000 ms | **fail** |
| `native_control` | control | 25.475 ms | 24.726 ms | -0.749 ms | 2.547 ms | pass |
| `native_hash_control` | control | 133.564 ms | 133.110 ms | -0.454 ms | 13.356 ms | pass |

Plus three project-tooling workloads measured outside the runner, unchanged by
this work: `xsht api summary`, `xsht check`, and `xsht lint` (see
`## Tooling cost recorded for §3.6`).

**Parity status of the failing rows.** The runner times each workload and
discards its output, so it cannot say whether a failing row *behaves* the same.
Measured separately with `bench/stdlib-port/parity.py`: all twenty-four
workloads, each run under both matched release binaries from the runner's own
working directory, produce **byte-identical stdout, stderr, and exit status**.
The failures are timing only — the exact
failing workload, its reference and candidate medians, its delta, and its budget
are the table above, and the smallest outstanding issues are named below.

### What the failures are

They are one cause in two shapes, and neither is a defect that tuning removes.

**Per-call interpreted cost.** `cli_small_schema` parses a four-field schema 200
times and costs 491 ms more than the reference: about 2.5 ms per parse against
0.08 ms for the native parser. The ported `cli.parse` runs on the interpreter's
frame machinery, so every option, argv word, and descriptor field costs
microseconds where the native one cost nanoseconds. The same shape produces the
CLI cold-start rows (`cold_cli_parse` pays 8.4 ms to prepare and run one parse),
`text_pad_batch`, `fmt_batch`, `quote_batch`, `quote_edge_cases`,
`checksum_batch`, `env_typed_lookups`, `mime_batch`, `json_path_ops`, and
`core_command`. The calibration in this build measures about 0.7 µs per
interpreted loop step and about 1.4 µs per interpreted call; a workload whose
per-item work is a native microsecond cannot close a 10× gap.

**Mandated preparation.** `cold_dynamic_ref` is a program that merely
*references* `module.load`, so §3.4 prepares all fifteen embedded modules before
execution: 6,474 lines parsed, checked, lowered, and verified. That is the rule,
not an accident, and it is why the row reads +11.4 ms. `cold_cli_parse`,
`cold_cli_usage`, and `cold_cli_error` pay the same cost for the one module each
of them reaches — `stdlib/cli.xsh` is 3,109 lines, the largest in the catalog.

`text_wrap_unicode` and `ini_large_record` are the two rows with a known
mechanical cause on top of that: `Str.wrap` scans a 1 MiB text one scalar at a
time (1,247 ms against 165 ms), and the INI encoder re-reads a 1,000-key record
once per key because a `Record` receiver is copied on every method call and
`lowered_freeze_large_slot_list` freezes `List` only (280 ms against 20 ms).
Both are linear in interpreted operations; the second is the Θ(n²) the
specification forbids, and it needs a runtime change — freezing large
record/map slots, or a bulk `Record.values()` — rather than an XSH change.

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
- `tests/xsh/stdlib/linux.xsh` — the dry-run coverage of the Linux entries,
  which the G prototypes also passed through while they existed.
- `tests/xsh/stdlib/hash.xsh::test_hash_verify_file_policy` — G02 parity cases:
  a verifying checksum, an uppercase one, the length rejection for two
  algorithms with their exact messages, a right-length non-hexadecimal
  checksum, a mismatch with its exact message, each of the four algorithms
  selecting its own digest, and a missing file reporting its read failure ahead
  of a malformed checksum. The same file passes on the reference build, so the
  cases pin the native behavior rather than describing the port.
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

Counted with `git diff --numstat` between the starting revision `37e1502` and
this work tree, production Rust only (`src/`, `crates/*/src`):

| | Lines |
| --- | --- |
| Production Rust added | 2,256 |
| Production Rust deleted | 4,369 |
| Net production Rust change | **-2,113** |
| XSH implementation (`stdlib/*.xsh`, 15 modules) | 6,474 |
| Deleted native owner files | `src/modules/cli.rs` (2,263), `src/modules/mime.rs` (283), `src/modules/shlex.rs` (77) |
| Test, doc, and bench churn (tracked `tests/`, `crates/*/tests/`, `docs/`, `bench/`, `AGENTS.md`, this ledger) | +7,625 |

The added Rust is one-time infrastructure: the embedded catalog and its
preparation gate (`src/stdlib.rs`), lowering, linkage and the specialized
`hash.verify_file` call (`src/runtime/eval/lower.rs`,
`indexed/full.rs`, `lowered_run.rs`,
`lowered_run/indexed_run.rs`), the loader's stdlib attachment
(`src/loader.rs`), the arena's internal-module support
(`src/syntax/arena.rs`), the registry's implementation bindings
(`crates/xsh-registry`), and the private bridge operations. That cost does not
recur per group; each additional ported group is nearly all deletion.

The required Linux entries (R12) bind per target: the Linux overload is the
script binding and the macOS overload is the native one, so a Linux build has
no path to a superseded implementation while macOS keeps the behavior it had.
The native bodies stay because the same file must still build and behave there.
The gated Linux prototypes that failed their gates were removed with the rest of
their port — module, binding, bridges, and dispatch — so their native bodies are
the only implementations again on every target.

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

## Defects found and fixed during the port
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
  A stream element is the yielded value, so a failing `?` in a `yield` position
  produces an element that is a `Result` — the consumer binds it and the next
  field access fails with `missing-field`. The same `?` in a `let` inside the
  stream body propagates and aborts the producer. Found by a fixture where a
  device's attribute file was missing: the baseline failed the call and the
  first version of the rfkill inventory's producer produced records with no
  fields.
  Every stream producer in this port binds first and yields second.


- **A script stream materializes its items when the producer runs, and a
  per-item failure has nowhere to live.** `eval_indexed_stream_producer` runs
  the producer body and hands `StreamValue::from_values` its items, so
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

- Calling a **parameterless** `stream` function fails at run time with
  `lowered function did not return`, from any caller: a `for` loop, a `let`
  binding, or a `return`. A stream function with at least one parameter is
  unaffected, which is why this survived the existing corpus — every stream
  producer in `core/` and `tests/` takes one. The reference build reproduces it
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

  This is the first Linux execution of a ported stream entry to reach the
  defect: the first embedded stream producer written for this port took no
  parameters, and the Linux test failed with it in the container while passing
  on macOS, where the entry kept its native binding. Every stream producer in
  the catalog now takes one. The underlying defect is
  left alone: fixing it is a change to lowering, not to this port.

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
