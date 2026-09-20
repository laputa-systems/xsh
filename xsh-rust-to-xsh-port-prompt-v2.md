# XSH standard-library self-hosting: implementation specification

## 0. Mandate and fixed decisions

Implement this task in `laputa-systems/xsh`. This document replaces earlier versions of the Rust-to-XSH port prompt. It is a complete specification, not a request to prepare another proposal.

Move the library algorithms explicitly assigned below from Rust into ordinary XSH source. Delete their superseded native implementations. Keep Rust responsible for compilation, verification, execution, runtime representations, efficient kernels, and necessary host-operation guarantees. The objective is a net reduction in maintained production Rust and more dogfooding, not a smaller line count achieved by moving Rust between crates or hiding it behind script wrappers.

This is an implementation-language migration. Do not redesign public APIs, broaden supported syntax, change effects or inference, fix unrelated behavior, replace native algorithms with external commands, or weaken the existing tests.

The owner has made these decisions:

| Decision | Contract |
| --- | --- |
| Library packaging | Embed ordinary `.xsh` source in the binary through an explicit build-time manifest. No installed stdlib directory is required. |
| Preparation | Parse/check/lower required embedded modules during normal program preparation, before executing the program. No first-call compilation, runtime `eval`, JIT, bytecode-image project, or persistent compiled cache. |
| Execution | Use the existing verified indexed runtime and its normal function frames. An ordinary stdlib invocation does no parsing, checking, lowering, import search, or name-to-function lookup. |
| Namespace integrity | Public stdlib bindings are sealed. Private implementation identities and primitives are inaccessible to user source. Trust is assigned by internal provenance, never a filename or search path. |
| Scope | Complete all Required groups. Resolve all Gated groups under the specified rules. Do not attempt the Deferred groups, including `mdev`, account mutation, and privileged boot/orchestration extraction. |
| Performance | Cold-start regression allowance: the larger of 5% or 1 ms. Designated non-hot end-to-end workloads: the larger of 10% or 2 ms. Existing native fast-path regression gates remain in force. |
| Verification host | Native aarch64 macOS. Docker is available for Linux verification; use native `linux/arm64` and the repository's Linux/musl tooling. |

**Meaning of “no runtime parsing.”** A fresh process still parses its input program and any required embedded stdlib source during preparation. That cost counts toward cold startup; embedding source does not erase it. Once execution starts, no embedded stdlib source may be parsed, checked, or lowered. The first stdlib call and all later calls execute already prepared instructions. Existing explicit loading of *user* source through `module.load` retains its behavior, but must not become a back door for late stdlib preparation; section 3 specifies the conservative preparation rule.

Source-audit baseline: `master` at `a2f3fd4f12a3fd99e9ccbf6787d7bc5fc72dcda4`, commit `lint now builds one shared parsed/import graph and source map`, timestamp `2026-08-23T17:17:38Z`. That was also the branch tip returned by GitHub when rechecked on September 20, 2026. The audit was static: it is not evidence that builds, tests, or proposed performance gates have passed.

Record the actual starting revision and dirty state. Inspect newer changes when applicable; do not reset a newer checkout to the audit revision, overwrite unrelated work, or change the pinned toolchain/dependency policy simply to make this task easier. Build the reference and candidate under equivalent conditions.

## 1. Completion is a finite contract

There are three dispositions, with different completion rules:

**Required (`R01`–`R12`).** These algorithms must execute in XSH, with the superseded native algorithm removed. Small native boundaries explicitly permitted below may remain. A required group cannot be relabeled optional, gated, or deferred by the agent. A genuine failure of a required group means the task is incomplete; finish independent work and report the exact remaining failure rather than requesting another architecture decision.

**Gated (`G01`–`G07`).** Each group must end as `ported`, `retained-boundary`, or `retained-performance`. Port it when the existing contract can be preserved with the permitted mechanisms and performance gates pass. A boundary rejection requires a concrete call graph, representation/lifecycle obstacle, or specifically unavailable qualification requirement under section 11; not “OS code” or “too complex.” A performance rejection requires a behaviorally valid prototype and repeated paired measurements. Unattempted work or an uninvestigated uncertainty is not a completed gate.

**Deferred.** Leave these production algorithms native in this task. Record why they remain; do not spend the implementation budget prototyping their extraction. They remain regression coverage targets when the common execution/binding layer changes.

A task ledger, `STDLIB-PORT.md`, must identify every R/G group, implementation destinations, remaining native boundaries, deletion status, tests, and benchmark results. This is a migration ledger, not a second language specification. Put lasting architecture changes in the existing canonical docs.

Completion requires all R groups, resolved G groups, the architecture tests, the applicable macOS/Linux/feature matrix, unchanged public API behavior, packaging checks, and performance acceptance. Merely adding a loader, writing source wrappers, or declaring a large number of candidates unsuitable does not satisfy this contract.

## 2. Read and baseline before editing

Read `AGENTS.md`, `docs/CHAPTER-01-why-xsh.md`, `docs/ARCHITECTURE.md`, `docs/SPEC.md`, `docs/SPEC-TYPING.md`, `docs/SPEC-OS.md`, `docs/STREAMS.md`, `docs/JSON.md`, and `docs/TEST-MAP.md`, then the owner code and nearest tests. Follow current routing when paths have changed.

The audited repository prohibits agent formatter/autofix runs: no `cargo fmt`, `cargo dev lint --fix`, `cargo clippy --fix`, `xsht fmt`, or `xsht lint --fix`. Use manual scoped edits and existing non-mutating checks. Do not regenerate generated documentation. Ordinary verification uses debug builds; performance comparisons use release builds. Do not use the `dist` profile. Add no dependencies, public CLI switches, or new CI workflows for this task.

Inventory the complete public registry, not just `src/modules/*.rs`. Relevant owners include:

- `crates/xsh-registry/src/signature/`, `crates/xsh-registry/src/runtime_op.rs`, `crates/xsh-registry/src/records.rs`, and `src/modules/signature.rs`.
- `src/loader.rs`, `src/sema/check/`, `src/runtime/eval/lower.rs`, `lowered_run.rs`, `lowered_ops.rs`, `indexed/`, and `lowered_run/indexed_run/`.
- `src/modules/`, including `linux/`, and `src/runtime/eval/modules/`.
- Rust facade callers in `src/lib.rs`, `src/runtime/eval/modules/host.rs`, `crates/xsht`, and `crates/xshi`.
- Existing corpus, native tests, runtime integration tests, `core/`, and `dev/` consumers.

For R/G groups record public function/method/overload spellings, current native and specialized lowering owners, argument/default handling, effect and return contracts, direct Rust callers, platform/features, tests, script destination, and retained primitives. Every other registry area gets a coherent retention row. Do not inventory nonexistent APIs by treating a `RuntimeOp` variant as a public symbol. Some operations are method-only or stage-only; do not recreate removed module functions. In particular, `text.rs` is not a public `text` module.

Preserve an immutable reference revision/build outside the active candidate target directory. Save public API snapshots and test-discovery results. Run baseline tests and the fixed benchmark workloads before replacing implementations. Record pre-existing failures and environmental skips individually. Reproduce uncertain failures on the reference before attributing them to the port.

Bootstrap development tools with explicit Cargo package/binary commands rather than relying exclusively on `cargo dev`, because the CLI/env/stdlib being migrated is used by the self-hosted developer tooling. Do not change unrelated `dev/` programs to compensate for a broken library port.

## 3. Fixed architecture: embedded sources, preparation-time linking

### 3.1 One public registry, implementation bindings per entry

Keep the current registry authoritative for public names, overloads, parameter order/names, defaultedness, special argument checks, receiver behavior, schema-driven inference, effects, records, errors, docs, and API coverage identity.

Associate each public callable entry/overload with a closed implementation binding:

```text
Native(existing native operation)
Script(embedded implementation module identity, implementation function identity)
```

These names are conceptual types, not mandated Rust identifiers. Bind by the public entry/overload, not solely by a `RuntimeOp` that may serve multiple public spellings. Preserve the registry's public operation identity where needed for compatibility and observability; it need not be deleted just because a native body disappears.

The descriptor contains implementation routing, not a duplicate independently maintained public signature. Internal implementation function annotations are checked against the public contract, after any explicitly documented ABI adaptation. Preserve existing precisely inferred types at the caller; a script body with a dynamic `Record` result must not erase the schema-dependent type inferred for `cli.parse`.

Mixed modules are mandatory. For example, TUI formatting is XSH while secret input remains native. Retained operations must continue down their current native fast paths rather than being wrapped in unnecessary script calls.

### 3.2 Explicit, immutable embedded-source catalog

Store maintained implementations under `stdlib/`. Use a small compile-time catalog with explicit module identities and `include_str!`-style source inclusion. An existing build step may generate a deterministic inclusion table, but it must not execute XSH to produce executable code. Do not require a previously installed XSH binary, serialize IR, or create a bootstrap compiler cycle.

The catalog maps known internal module identities to embedded UTF-8 bytes. It does not perform directory scanning or filesystem/environment lookup when the executable runs. Cargo rebuild tracking must include every embedded file. Retained sources and diagnostics ship in normal binaries without an external package layout.

Standard implementation imports resolve only through this catalog and existing native public/private descriptors. They cannot import user/project source. User imports remain on the existing user-module resolution path and cannot substitute an embedded module.

Give sources stable diagnostic labels, for example `<xsh-stdlib:cli>`. Labels are presentation only, never identity or authority. Do not use canonicalized filesystem paths as builtin identities.

### 3.3 Preparation algorithm

Use one normal frontend and one normal executable representation. Extend the existing preparation pipeline as follows:

1. Parse the entry and its statically loaded user-module graph; collect/check declarations and references using the existing public registry.
2. From resolved API references in the program, choose the embedded implementation modules needed for script-backed entries. Include methods, supported callable references, and command forms. Do not infer requirements from textual matching or `use` statements alone: standard APIs are available without `use`.
3. Parse each selected embedded module at most once in this preparation ownership context. Resolve its standard/private dependencies, check all its bodies, and continue to the finite transitive closure. Reuse the existing dependency-cycle machinery; report a cycle in the internal module import graph as an implementation error. Ordinary function recursion is not an import cycle.
4. Check binding/implementation compatibility and lower/link the prepared bodies into the existing program store. Bind script-backed public calls to concrete program-owned function identities.
5. Run the existing whole-store verifier, extended with the provenance/private-reference rules. Only then execute entry/user-module initialization and program statements. Release construction-only state as the current execution architecture requires.

Use module-granularity dependency loading. Checking all bodies in a selected module and conservatively including their referenced dependencies is acceptable. Do not add a function-level optimizer/tree-shaker, interpreter reentry mechanism, or another IR. A trivial static program not using these APIs must prepare zero script implementation modules. A referenced module is not recompiled per call or per import of the same user module.

Do not add a separately hand-maintained dependency graph that can silently disagree with the checked source. Derive dependencies from resolution; any cached/catalog dependency information must be mechanically verified against the source. Do not repeatedly rebuild the public type registry to grow the implementation closure.

### 3.4 Dynamic user modules and opaque references

No late stdlib parsing is allowed, including on existing `module.load` paths.

During initial preparation, if any checked user body references `module.load`, a supported indirect callable reference to it, or another existing user-code-loading route whose future API needs cannot be bounded, conservatively prepare **all script-backed stdlib implementation modules applicable to that build** before entering execution. Dead-code elimination is not necessary: a reference in a checked but uncalled body is sufficient. Similarly, supported opaque public namespace/callable flows must include all possible stdlib targets when a narrower set cannot be established.

This is a small conservative rule, not a whole-program proof system. It preserves dynamic user loading while moving every stdlib preparation cost before execution. Later `module.load` may still parse/check user source as it already does, but it links standard references to the prepared builtin implementations and may not reparse their source. Do not remove or restrict existing user-module behavior to make the invariant easy.

For `xshi`, each newly submitted input has a preparation boundary before its execution. Reuse previously prepared modules within the existing compatible session/program owner; prepare newly needed modules before executing that input. Before an input may dynamically load user code, expand its prepared stdlib set conservatively as above. Do not mutate or renumber function identities still referenced by live session values.

Embedding APIs that can execute arbitrary standard operations without a statically bounded script must prepare the complete applicable implementation set during their existing preparation/session-initialization boundary. Do not defer this to the first native dispatch. Keep existing public Rust facade signatures intact.

A missing script implementation during execution is an internal preparation/verifier defect, not a cue to compile source or fall back to the deleted Rust algorithm. The release path must fail clearly rather than silently selecting another implementation. Ordinary code must never reach this state.

### 3.5 Calls use resolved targets and the existing frame engine

After linking, an ordinary script-backed API call carries a resolved target in the normal executable representation. Execute it through the existing function/continuation frame machinery, preserving stack-depth, cancellation, `defer`, result propagation, and tail behavior. No per-call source processing, string-based dispatch, secondary evaluator, or recursive native-to-interpreter trampoline.

Keep public-call metadata where needed for arguments, traces, error attribution, and API coverage. The public-boundary adapter may normalize invocation arguments and enter the resolved script function. It may not implement the library algorithm.

Inspect and migrate every specialized lowering path serving a selected entry, not only the generic module dispatcher. A specialized node such as file verification must not keep executing the removed native body while the generic route calls XSH.

### 3.6 Ownership and reuse

Embedded source bytes may be process-global and immutable. Prepared source maps, symbol identities, type pools, functions, and executable IDs must obey existing program/session ownership.

The audited `SymbolOwner` releases dynamic symbols and permits IDs to be reused after owners die. Do not store owner-local IDs in a global cache. Do not serialize closures, retain parser/lowering scratch indefinitely, or leak one program's state into another. Existing compatible workspace/session sharing is the reuse boundary; there is no new persistent/on-disk cache in this task.

Preserve the shared parsed/import-graph behavior of project-wide `xsht` checks/linting. Reuse one checked implementation module within a compatible workspace context; do not instantiate an evaluator for every file just to check standard APIs. Isolated programs with different owners may prepare separately. Measure rather than hide that distinction.

### 3.7 No effectful library initialization

Embedded modules contain ordinary functions/types and only supported immutable literal constants. No module-level host calls, mutable singleton maps, dynamic imports, clock reads, environment/cwd snapshots, resources, cache initialization, or arbitrary initializer execution. Constant data such as MIME fallback entries may be constructed locally during calls or represented as supported immutable literals.

Do not use a `pure` annotation as permission to execute a body during preparation: some legacy APIs have contextual/native behavior. Invocation-sensitive work remains at its baseline invocation point. Preparation must not touch `/etc`, `/proc`, `/sys`, network resources, caller cwd/env, or command context to initialize these modules.

## 4. Namespace integrity and private primitives

### 4.1 Separate identities; preserve existing source rules

Represent builtin/public implementation identities separately from user module identities. Preserve the existing reserved-standard-name rules and documented exceptions, including record destructuring and conventional `error` bindings. Do not globally reserve every internal helper spelling or retroactively forbid unrelated user identifiers.

A user file called `cli.xsh`, a directory called `stdlib`, an alias named like a helper, or a forged diagnostic filename cannot replace or become an internal module. Qualified standard calls retain their existing resolution rules. Internal helper calls are lexically bound to their defining implementation modules and cannot capture similarly named user declarations.

Standard implementations must not be replaceable through exports, record writes, `Any`, module contracts, user search roots, dynamic modules, or interactive rebinding. Do not add a stdlib override environment variable or public testing switch.

### 4.2 Authority comes from unforgeable internal provenance

An embedded source unit is designated trusted only by the crate-private code that loads a known catalog entry. User input APIs always produce user provenance, regardless of their source name or where bytes were read.

Carry this identity through checked declarations and executable function ownership. Do not expose a public constructor/flag that grants embedded-stdlib authority. The verifier must check private call targets against the owning implementation identity, rather than trusting a callsite span, source label, or the current runtime caller. Passing a user callback through a trusted frame must not elevate it.

Private primitive references are direct-call-only and non-first-class. Reject attempts to store them as function values, return them, export them, or pass them through `Any`/records/callables. Apply the same non-escape rule to private implementation helpers where leakage would expose those operations or bypass a public boundary. No private entry appears in public `xsht api`, user module introspection, or module contracts. Ordinary library data can still be returned normally.

A supported public callable value, where one exists today, must still enter its public argument/context boundary; it must not expose the implementation function as an unrestricted private callable. A public `Stream` result may retain its private producer internally: consuming the stream executes that already-verified producer under its defining owner, without exposing the producer function as user-callable data. Ordinary user callable values already present in input data remain ordinary user values; do not reject or elevate them merely because a private record operation preserves them.

Tests requiring trusted helper access must use a crate-private harness limited to the actual compiled-in catalog and predeclared test companions, not a filename-based trust exception. Production executables must not acquire a new trust-granting option.

Wire catalog validation into the ordinary Cargo/corpus gates, including strict type/effect checking and the repository's non-mutating format/lint expectations. Validate the actual embedded bytes through the trusted internal preparation entrypoint. Do not grant trust to `xsht check stdlib/something.xsh` just because it names a repository path; direct public source loading remains untrusted. Adjust corpus routing for the explicit catalog-owned sources and their test companions so it uses their dedicated validation instead of misclassifying them as user programs. A blanket stdlib exclusion without equivalent checked coverage, or accepting arbitrary disk bytes through an internal test flag, is not allowed.

### 4.3 Allowed native bridge categories

Prefer existing operations. New private bridges are limited to these categories, with explicit types and narrow per-implementation access:

| Category | Allowed work | Not allowed |
| --- | --- | --- |
| Runtime representation | Construct/persistently update an existing `Record` from named values, or the smallest missing existing-container operation needed by required ports. Preserve types/order/value semantics. | JSON round trips, unsafe casts, arbitrary memory access, complete CLI/INI/JSON-path algorithms. |
| Existing builtin errors | Construct an existing error family/payload, preserve context, or reproduce the existing return-versus-propagate boundary and caller attribution. | Generic source execution or inventing replacement error semantics. |
| Invocation context | Supply the existing contextual command-name/default information at a public call boundary. | Reading caller lexical variables by name or snapshotting ambient context at import time. |
| Legacy primitive host behavior | The specific CLI path checks and host acquisition operations required to preserve baseline cwd/env, error order, read timing, or stream capture. | General raw syscall/FFI dispatch, renamed whole algorithms, adding filesystem policy not already present. |
| Existing shared kernels | Typed adapters to retained numeric conversion, ASCII/Unicode operations, IP rendering, and small validators that remain genuinely shared with native bootstrap callers. | Keeping a second full policy implementation or porting already-native fast loops into another native helper. |

Use a closed, statically typed set of private operation descriptors integrated with the existing lowering/dispatch infrastructure. No `internal.call(name, args)`, public FFI, arbitrary syscall number, source-text evaluation, general unchecked-effects mode, or public new runtime type. A diagnostic name can describe an operation, but cannot select arbitrary native functionality at execution time.

Each new descriptor gets one ledger row with exact inputs/outputs, effect/context behavior, permitted implementation owner(s), why an existing operation was insufficient, tests, and native code retained. If a G group requires a bridge outside these categories, retain that group. If a required group does, investigate a smaller implementation; do not silently expand the permitted surface.

### 4.4 CLI's existing purity inconsistency has one bounded treatment

At the audited revision, CLI entrypoints are marked pure even though path constraints inspect host filesystem state. Preserve those public declarations and the existing check timing. Do not make the APIs effectful, remove validation, or exempt all stdlib code from effect checking.

Preserve this through specifically identified legacy CLI probe operations usable only by the embedded CLI implementation. Their compatibility classification is an exception for those existing probes, not general permission for pure XSH to do IO. Do not const-fold, pre-execute, memoize, reorder, or hoist them because the public signature is pure. All ordinary XSH effect rules still apply everywhere else, including other embedded modules.

Keep CLI's low-level signed integer and Duration conversion faithful. An existing broader `parse_int` grammar is not a drop-in replacement for Rust's scalar parsing. A narrow adapter to a retained conversion kernel is allowed; a second CLI parser is not.

### 4.5 Public boundary context, errors, and observability

Use existing callsite metadata to preserve public argument normalization, contextual defaults, runtime errors, trace identity, API coverage, and mock interception. Omitted arguments are normalized exactly as before at each invocation, not when the module is prepared. Explicit empty/default values remain distinguishable where the baseline distinguishes them.

A small scoped internal call-boundary context is permitted when required for command naming, error construction, or propagation. It is interpreter-owned, not forgeable script data, and must unwind correctly through recursion, errors, `defer`, and callbacks. Do not add a global mutable current-caller variable or allocate a fresh evaluator per operation.

Default public diagnostics and traces must preserve the existing observable public boundary rather than exposing every internal source frame as a new user call. Internal implementation statements/calls must not manufacture extra public API counts or mock calls. Retain source locations for implementation diagnostics and test instrumentation. Do not suppress user callbacks or genuine user-origin nested activity merely because a stdlib frame appears lower on the stack. Add no new public trace mode for this task.

## 5. Required algorithm ports

For each group, exercise the original public spellings; helper names below identify implementation owners, not permission to add new public functions. Split implementation files only along coherent existing concerns. Do not require one huge module or create an excessive collection of one-function modules.

### R01 — Shell quoting

Owner: `src/modules/shlex.rs`. Port `quote` and `join` into embedded XSH and remove their native algorithm bodies and dead dispatch.

Preserve the exact ASCII safe set: alphanumeric plus `_@%+=:,./-`. Empty input produces `''`. Preserve apostrophe escaping, Unicode, embedded newlines, and joining each independently quoted argument with one space. This is string quoting only, not shell execution or a new command builder. Include all existing Rust unit cases in native XSH coverage before deleting their original helper tests.

### R02 — Complete CLI policy

Owner: `src/modules/cli.rs`, relevant checker inference and runtime/lowering adapters. Port the policies behind `cli.parse`, `parse_full`, `applet`, both `commands` overloads, `tokens`, and `usage`.

Move schema/descriptor parsing, form parsing, normalization, argument scanning/state transitions, aliases, option/positional/repeated values, defaults and env precedence, warnings, constraints, command routing/fallback/rootless behavior, and usage rendering into XSH. Native code may retain only the approved representation/context/conversion/probe boundaries. This group is not complete with only `tokens` or `usage` ported.

Preserve sorted-map iteration where it affects positional ordering, help output, alias collision handling, or error priority. Preserve `kind` versus `type`, `form` versus `use`, optional-value rules, duplicate detection, required groups, `requires`, conflicts, deprecated warnings, hidden options, reserved help, and the applet overwrite/reset policy. Implement only baseline-supported features; do not infer a new flag-counting API from a generic CLI feature checklist.

Distinguish repeated defaults, absent/null values, environment sources, and argv sources exactly. Preserve when path checks happen and whether they use host cwd rather than evaluator cwd. Preserve command-name defaults, the `cli_usage` payload, `cli-help`, `cli-parse`, `cli-commands`, and result/abort presentation at the outer CLI boundary. `usage` must retain its plain return type and existing failure behavior, not become a public `Result` API.

Retain schema-dependent checker inference and all existing literal-path/typed argument rules. Preserve type-sensitive `Record` results; do not return maps instead. Add invalid schemas and arguments with multiple plausible errors to pin validation order. Do not “correct” legacy inconsistencies in this migration.

### R03 — Argument-word parser

Owner: `src/modules/process.rs::argv_words` and `ArgvWordsParser`. Port the parser/state machine, not native process creation or inspection.

Preserve empty quoted arguments, quote concatenation, escapes, whitespace, error messages/positions, and every rejection of unsupported shell syntax. Quoted literal metacharacters retain their meaning. No shell tokenizer dependency, command substitution, permissive glob expansion, or subprocess. Keep the `argv-words` error behavior and original unit cases.

### R04 — MIME table and parser policy

Owner: `src/modules/mime.rs`. Port builtin extension data, host-overlay interpretation, extension/path suffix selection, media-type and parameter parsing, and result construction.

Retain native file acquisition where needed. `/etc/mime.types` is consulted at the same invocation points as the baseline; ignore its read failures as before. Do not add import-time or process-lifetime caching. For path lookup, preserve the existing number/order of overlay lookups across candidate suffixes rather than quietly snapshotting once per process.

Preserve host override ordering, extension-list order, leading dots, ASCII case normalization, multi-extension precedence, duplicate parameter semantics, and the existing restricted token grammar. The baseline splits on semicolons before interpreting quoted values; do not replace it with a more permissive standards-complete parser. Test mutations between calls and invalid/quoted edge cases.

### R05 — TUI formatting, not terminal IO

Owner: `src/modules/tui.rs`. Port every ANSI sequence producer and left/right padding with their visible-width policy. Keep `read_secret`, raw stdin access, echo changes/restoration, EOF handling, and descriptor lifetime native.

Visible width counts Unicode scalar values, not display cells or graphemes. A CSI beginning ESC `[` is skipped through its first character in `@`–`~`; an incomplete sequence consumes the remainder for this calculation. CR/LF do not count. Negative widths clamp to zero. Preserve exact bytes for escapes, spaces, Unicode, and already-wide text. Do not add a terminal-width dependency.

### R06 — Small numeric presentation

Owners: `src/modules/bytes.rs::human`, `src/modules/time.rs::duration_compact`. Port both formatters.

Preserve negative size `-`, binary unit progression, precision thresholds, and floating formatting/rounding. Preserve negative-duration clamping, day/hour/minute/second selection, and exact space/zero padding. Reuse the existing numeric formatting kernel where required for exact output; do not implement binary64 formatting in XSH.

Keep clocks, sleeping, duration representation, measurement, digest encoding, and shared UTC civil-date formatting native. `format_epoch_ms_utc` has native process-record callers and is explicitly not part of this group.

### R07 — Higher-level string policy

Owner: `src/modules/text.rs::wrap_text`, `wrap_line`, `wrap_word`, and `fields_text`, through their current method bindings. Port wrapping and field-selection policy.

Preserve whitespace classification, splitting of long words by Unicode scalar count, empty input versus empty lines, trailing newline behavior, non-positive-width errors, explicit delimiter filtering, and output list shape. Use efficient existing splitting/joining primitives; do not implement per-byte Unicode decoding in XSH or repeatedly copy growing output strings.

Retain primitive splitting, searching, replacement, Unicode case conversion, translation and its measured native fast path, byte indexing/views, and numeric parsing. Other transform/count helpers are Deferred, not a requirement to rewrite all of `text.rs`.

### R08 — Checksum-line interpretation

Owner: `src/modules/hash.rs::parse_check_line` and its language adapter. Port line validation, parsing, and returned record construction.

Preserve trailing-CR handling, the exact search precedence of double-space and space-star separators, optional leading star handling in the path, binary marker, lowercasing, incomplete-line errors, and hexadecimal validation. Do not convert this into a different checksum-file dialect. Preserve error kinds and validation priority.

Digest creation, MD5/SHA/CRC implementations, digest representation, encoding, and the baseline file-hashing IO strategy remain native. Full file-verification policy is G02.

### R09 — INI encoding only

Owner: `src/modules/ini.rs::encode` and encoder-only helpers. Port output selection/order, validation sequencing, section/global formatting, and multiline serialization.

Keep the single shared native decoder: `crates/xsht/src/cli/files.rs` calls `xsh::host::ini::decode` for tooling configuration before normal script execution. Do not add a Rust-to-XSH callback layer, load a project evaluator to read configuration, or keep duplicate INI parsers.

Preserve distinct normalization for global keys and section keys, ordered output, collision/overwrite behavior after normalization, valid/invalid characters, indentation, blank lines, and final newlines. Reject non-string/non-section values exactly as before. Small validators genuinely shared with the retained decoder may remain native under typed private bindings; do not duplicate their policy or pretend they were deleted. IO composition is G03.

### R10 — JSON path operations and in-memory JSON-lines composition

Owners: `src/modules/json.rs::json_path_get`, `json_path_set`, `json_path_remove`, path-segment/recursive helpers, and `encode_json_lines`.

Move path interpretation, whole-path validation before traversal, traversal/update/removal policy, and JSON-lines composition into XSH. Preserve `Record` versus `Map`, non-JSON runtime values accepted by the path operations, nonnegative list indexes, missing versus null, missing-intermediate behavior, empty-path get/set/remove, and value semantics. Do not create missing intermediate containers or round-trip through JSON.

Use the approved minimum record-construction/update bridge only for runtime representation. The sequence of path decisions must not remain native. Implement deep operations without introducing quadratic cloning or native stack growth where the existing explicit-frame engine should handle calls.

JSON codecs, number restrictions, runtime-value conversions, pretty formatting needed by native callers, schema checks, and streaming JSON decode remain native. File read/write wrappers are G03. `encode_lines` must preserve per-item compact encoding, failure order, and a newline for every successfully encoded output item in the final successful string; no partial external output is introduced.

### R11 — Environment convenience policy

Owner: the actual `EnvGetOr`, `EnvBool`, and `EnvInt` dispatch/lowering paths. Port their fallback and conversion logic into XSH on retained raw environment acquisition.

Preserve key validation, absent versus empty values, invalid UTF-8, default evaluation, accepted boolean spellings, numeric whitespace/range rules, and operation-specific errors. Do not reuse a similarly named parser without grammar equivalence. Check both scoped evaluator environment behavior and externally supplied process environments.

Retain raw OS-byte access, environment enumeration, `env.path`, `EnvPathList`, path-list mutation, scoped environment installation, and boundary validation. Do not expand this group to all environment functionality or treat evaluator env and real process env as interchangeable.

### R12 — Linux text-backed system policy

Owners: Linux branches of `src/modules/system.rs`, `src/modules/unix.rs::uptime_seconds_impl`, and `src/modules/linux/real/kernel.rs`.

Port these exact groups:

- Linux `system.os_release`: text parsing, unquoting, fallback/default record construction.
- Linux `system.memory` and `linux.meminfo`: text interpretation, required fields, and record selection.
- Linux `unix.uptime_seconds`: proc-text conversion policy.
- `linux.modules`: module-line interpretation and lazy record production, with a call-time snapshot wrapper.

Keep native acquisition as needed to preserve operation-specific failures and capture timing. Use target-aware implementation bindings so macOS retains its existing native APIs and unsupported Linux entrypoints produce the same errors before any `/proc` access. Do not add a public platform switch.

OS release falls back from `/etc/os-release` to `/usr/lib/os-release` after the same read failures as before, not just `NotFound`. Preserve the current quoting rules, key handling, and defaults. Memory parsing uses saturation; ordinary checked multiplication is not equivalent. Preserve the two schemas and their different required fields/error namespaces. Uptime keeps its current fractional-field truncation and malformed-value fallback.

For `linux.modules`, read/capture text and surface acquisition errors at call time; parse each retained line as consumed, preserving delayed malformed-line failures. Implement a normal wrapper returning an existing named lazy producer. Do not make everything eager, and do not defer the initial snapshot until iteration. Test zero consumption, one-item consumption, a bad later row, and early termination.

## 6. Gated extraction groups

Evaluate each G group separately after required infrastructure works. For a `retained-boundary` decision, identify the exact native caller or indivisible contract and explain why a smaller extraction cannot delete meaningful policy without duplication or prohibited machinery. A demonstrable static obstacle can settle that gate without a pointless prototype. “Probably slow,” “platform-specific,” “needs investigation,” and “would take time” cannot.

For a viable group, make a reversible behaviorally faithful prototype, run the relevant parity and fixed performance workloads, then either integrate it or remove the prototype and retain the native implementation with measurements. No shipped runtime backend selector or dual production implementation. Do not reject an entire G group because one listed suboperation has a boundary; record each coherent suboperation's disposition.

### G01 — Git-root discovery

Owner: `src/modules/fs.rs::gitroot` and `FsGitroot` dispatch. Try moving ancestor selection and `.git` existence policy into XSH while retaining filesystem/path primitives. Preserve starting evaluator cwd, stopping behavior, symlink/existence semantics, raw path bytes, and error kind. A `.git` file can satisfy the baseline; do not require a directory or run Git.

Check Rust facade callers first. If this public host helper is needed outside language execution, keep one shared native implementation rather than adding callbacks or a duplicate script algorithm. Shared mode-bit predicates used to construct native `FsEntry` records remain native; rewriting a few bit tests twice is not meaningful progress.

### G02 — File-checksum verification policy

Owner: file-verification dispatch/specialized IR and `src/modules/hash.rs::verify_hex` plus validation. Move algorithm selection, expected-digest policy, and comparison/error composition into XSH only if no native transport/archive/bootstrap caller requires the same policy to remain native.

Keep hash acquisition and digest representation native. Preserve IO before/after validation order, digest-length and hex checks, case-insensitive comparison, error messages/context, and Path/Bytes overload distinctions. Do not change how native file hashing buffers data. Verify both generic and specialized execution paths.

### G03 — JSON/INI file-IO composition

Owners: JSON read/write/write-lines and INI write adapters. Move only composition that deletes real code around the retained codecs and filesystem operations. R09 and R10 are independently mandatory.

Preserve encoding-before-opening versus opening-before-encoding, error remapping, truncation/atomicity, final newlines, whole-result behavior, and precise public effects. Do not add streaming writes when the baseline first encodes into memory. Keep the INI decoder/read/bootstrap path native. If the existing wrapper is already the smallest useful host adapter, retain it with exact accounting instead of producing a script shim and equal-sized Rust adapter.

### G04 — Read-only route interpretation

Owner: `src/modules/linux/real/net.rs` route-line parsing and record production. Try the IPv4/IPv6 text parser and destination/flag/metric policy in XSH, retaining exact IP parsing/rendering primitives as needed.

Preserve call-time acquisition of route sources, missing-file behavior, header skipping, row order, malformed-line filtering, numeric default/range policy, endian treatment, IPv6 formatting, prefix calculation, and flag order. The address rendering in route records is not necessarily the format used by open-file socket records. Do not standardize them.

Do not change route mutation APIs, link configuration, DHCP sockets, or interface ABI handling. Exercise fixture parsers on both platforms through internal test companions and real read-only acquisition on Linux.

### G05 — Read-only sysfs inventories

Owners: `linux.interfaces` in `real/net.rs`, `linux.rfkill_list` in `real/boot.rs`, and `linux.block_devices` in `block.rs`.

Evaluate each independently. Move selection, sorting, field interpretation/defaults, partition-path selection, and record construction into XSH while keeping directory traversal, byte-preserving paths, and any required platform address acquisition native.

Preserve eager directory snapshots versus lazy per-entry reads, host ordering versus sorting, disappearing-entry handling, propagation versus swallowing of errors, exact booleans, numeric parsing defaults, and saturation. Preserve existing block-size calculations even if a different interpretation seems more correct; this is a port, not a units correction. Test symlinked sysfs layouts.

Use fixture trees and read-only Linux probes. Do not port rfkill block/unblock, raw block-device reads/writes, or network mutation. Do not call a lossy display conversion merely to avoid a Path representation edge.

### G06 — Read-only disk-usage presentation

Owner: `src/modules/linux/real/fs.rs` mount selection and `linux.disk_usage` stream composition. Try moving selection and presentation around native statvfs and retained mount acquisition.

Keep canonicalization/fallback, component-aware longest-prefix matching, tie/order behavior, snapshot timing, lazy stat calls, partial consumption, and exact saturation/units. Preserve operation-specific errors. Shared mount parsers needed by native mount-success checks stay native; do not duplicate them or call into XSH from the native mount operation.

Do not replace the native `fs` traversal/du/mount/stat layers. This gate concerns only independently removable policy behind the Linux presentation API.

### G07 — Kernel-module query and index-output policy, not insertion

Owner: `src/modules/linux/kernel.rs`, limited to `modinfo` and `depmod` behavior unique to those entrypoints.

Try moving query selection, parameter-description shaping, result presentation, and `modules.dep` line generation into XSH around a retained native index/acquisition boundary. Use a fixture `XSH_MODULES_DIR`; never rewrite the host's real modules tree.

The native `modprobe` operation is Deferred, so retain shared `ModuleIndex::scan`, normalization, dependency/metadata mechanics, or any other helper it actually needs. Do not duplicate those shared algorithms or introduce Rust-to-XSH callbacks to delete them. A retained index adapter must expose existing plain data, not retain the whole query or output-formatting algorithm that this gate claims to port.

Preserve compressed suffixes, NUL-delimited metadata interpretation, sort/duplicate-name behavior, explicit-path selection, first metadata values, `parm` splitting, ignored missing dependencies, output order/newlines, and real-process environment overrides. The metadata baseline scans decompressed `key=value` strings; do not silently turn it into an ELF-section parser. Do not substitute public `linux.insmod` for any retained insertion helper: fallback behavior differs.

## 7. Deferred and native-retention map

These are deliberate scope decisions. Do not count them as missing required work or attempt their extraction in this task. They retain their public contracts and regression coverage.

| Area | Retain |
| --- | --- |
| Compiler/runtime | Syntax, type/effect system, loader semantics for user source, lowering/verification, normal frame execution, cancellation, value storage, and ownership. Change only what is needed for the specified implementation bindings and encapsulation. |
| Primitive values/collections | Arithmetic/bit operations, Float conversions/formatting, List/Map/Set storage and mutation, equality/ordering, type/record/module contracts, runtime record construction, and special values. No general collection self-hosting project. |
| `fs`, `path`, `io`, `xsh-root` | Fast walking/listing/ignore rules, metadata/cursors/views, file/byte streams, copying, install/remove-manifest, atomic and rooted operations, locks/temp resources, permissions/links, path-byte handling, and platform project/user-directory resolution. G01 is the only additional filesystem policy gate. |
| Strings/bytes | Search/split/replacement/case conversion/translation, byte scanning/comparison, packing/unpacking, base encodings, UTF-8, dump/string-extraction kernels, offset IO, zeroing, and block/file copy. Only R06/R07 presentation policy is required. |
| `archive`, compression, `elf` | Codecs, efficient binary parsing, extraction/creation and safety/metadata semantics. |
| `diff`, `patch` | Diffy matching/patch parsing/applying, filenames and IO/safety transactions. No new thin policy extraction in this task. |
| `net`, `dns`, `xsh-net` | Transport/TLS/framing, resolver behavior, pools, executor admission, jobs, resource ownership, cancellation, instrumentation, and feature-disabled behavior. No Tokio or network redesign. |
| `process`, `unix` | Process/PTY/session/credential/signal mechanics, PID1 handling, child/group polling/escalation, command construction, process/thread/port/open-file scanning, status and platform ABI handling. R03 and the Linux uptime subset of R12 are the explicit exceptions. |
| `user`, `group`, authentication | NSS/libc lookup, account-file add/remove/parsing/writing, UID/GID selection, password hashing/verification, login/su/sulogin and credential transitions. No account mutation extraction. |
| `linux` administrative/boot operations | Sysctl get/set/load-dirs extraction; bulk mount/unmount/swap; modprobe/insertion/removal; DHCP release packet/orchestration extraction; filesystem-check execution policy; signal-all and shutdown; chroot/pivot/switch-root; reboot/halt; clocks; raw devices and partition/format/swap operations. No privileged orchestration port. |
| `linux` other native machinery | Syscall/ABI structs, statvfs/metadata identities, uevent/netlink sockets, device probes, IP/tty mechanics, loop attach/detach/probing and rollback, mount checks, RTC conversion. Only the R/G read-only policy splits are candidates here. |
| `mdev` / `xsh-applets` | The complete applet, rule/config parser, POSIX regex/captures, daemon state/signals, sequencing, firmware transfer, and device operations. No new extraction. |
| `system`, `cpu`, `time`, `utils` | Platform syscalls, macOS sysctl, available parallelism, clocks/sleep/measurement/Duration, shared native UTC formatting, evaluator-owned cache/callable/key semantics. Only listed R subsets move. |
| INI/JSON native boundaries | The shared INI decoder and tooling configuration path; JSON codecs and native tooling adapters; native-shared validators. R09/R10 and G03 are the only assigned policy migrations. |
| `error`, `test`, tooling | Error representation/control flow, test assertions/discovery/mocks/skip/capture/coverage, interactive editor/history/completion, formatter/linter/checker algorithms. Do not migrate the test oracle while using it to validate this task. |

Account for current additional registry entries in the ledger without expanding required scope. Newly discovered policy opportunities become future notes, not unsolicited rewrites.

Why the larger candidates are Deferred rather than forgotten:

- Account mutation mixes configurable file parsing with real-process environment paths, NSS, partial passwd/shadow update ordering, and writer semantics. A port must not silently add transactions/locks or substitute evaluator-local environment behavior. That separate project needs isolated mutation qualification.
- Sysctl and bulk mount/swap policies are valid future XSH candidates, but they carry directory/file ordering, partial-success behavior, source-specific error kinds, and selective errno swallowing. Shared mount parsing is also used by retained native mount logic. They must not be replaced by approximately equivalent single-operation APIs.
- Module dependency policy is portable in principle, but retained native modprobe callers and insertion-fallback differences prevent assuming the shared index/resolver is removable now.
- DHCP RELEASE layout is policy around native socket lifetime, address checks, broadcast setup, timestamp conversion, and exact close/error ordering. Keep the existing operation until a separately qualified resource-boundary extraction.
- Mdev uses libc POSIX extended regex with full-match/capture-offset behavior. The ordinary XSH regex API is not a replacement dialect. Rule parsing alone can also create a new data-conversion layer without removing the native rule interpreter.
- Process/open-file decoding and byte dumps can be bulk workloads. Shared native UTC/signal/formatting helpers and direct host facades make indiscriminate script callbacks counterproductive.
- Test assertions, collection conveniences, and small duplicated bit predicates offer comparatively little deletion while endangering generic inference, throughput, or the migration's own oracle.

This is the bounded implementation scope the owner approved, not a claim that the Deferred algorithms can never be written in XSH.

## 8. Cross-cutting compatibility rules

### 8.1 Freeze behavior, not just types

The unchanged public API snapshot is necessary but insufficient. Preserve argument normalization/evaluation order, contextual defaults, runtime types, return/propagate distinction, error family/variant/facets/payload/context, exit behavior, stderr/stdout bytes, source attribution, trace/API identity, mock interception, and supported call forms.

No new required `use`; no namespace aliases or literal-path coercions lost; no removed methods resurrected; no APIs weakened to `Any`; no public native/script backend switch. Preserve compile-time diagnostics on unchanged user programs where the implementation language is irrelevant.

When the old implementation has deterministic oddities, preserve them and add a characterization test. Do not reproduce undefined behavior or a genuine unsafe defect. Record such a defect separately; do not silently fix it and advertise strict parity. If it materially prevents a required port, report an exact incomplete item while continuing independent groups.

### 8.2 Data representation and arithmetic

Keep `Record` and `Map` distinct. Keep byte-backed `Path` lossless where the baseline is lossless. Do not use `.display()` as a universal conversion or JSON as a universal copier. Keep typed values inside dynamic containers; JSON-path functions are not permission to coerce Path/Bytes/Duration/errors into JSON.

Audit every relevant Rust cast, saturating/checked operation, signed/unsigned conversion, UTF-8 boundary, and scalar-versus-byte length. Preserve integer bounds, negative values, numeric grammar, and formatting. A checked XSH multiplication is not equivalent to Rust saturation; a decimal parser accepting signs/whitespace is not interchangeable with a canonical-decimal parser.

Avoid quadratic repeated string/list/record rebuilding. Use existing efficient operations and bounded kernels. Do not add an optimizer, mutable identity model, or generic builder language merely to rescue a bad G prototype.

### 8.3 Streams and resources

Preserve call-time versus consumption-time work explicitly. The wrapper owns eager snapshots/errors; an ordinary named `stream` producer owns the baseline lazy work. Preserve source ordering, early stop, partial outputs, error timing, and cleanup on success/failure/cancellation.

Do not replace streams with collected lists, reread a snapshot on every item, reopen a path to imitate one captured descriptor, or change producer timing by wrapping all work in a lazy function. Keep native file/resource ownership where it carries guarantees unavailable to ordinary composition.

### 8.4 Shared native callers

Before deletion, search all supported-target and feature-gated callers. Keep one implementation of genuinely shared host kernels. Do not add a general host-to-XSH callback facility, spin up evaluators for native configuration parsing, or keep two independently maintained production policy algorithms.

It is acceptable to retain a small shared validator/kernel that the script calls through a typed private adapter. It is not acceptable to leave the entire CLI/MIME/JSON-path algorithm native and label the public wrapper “ported.” Count retained helpers honestly.

### 8.5 Test migration without weakening coverage

Keep existing public integration and native XSH tests unchanged unless an internal harness path necessarily changes. Do not adjust expected behavior to the candidate. Do not lower coverage thresholds or disable tests.

When deleting a Rust-private helper makes its unit tests obsolete, first port every semantic assertion/case into disk-backed native XSH tests exercising the migrated public API or a restricted internal test companion. Record old-to-new test mapping. Keep Rust tests for genuine compiler/verifier/ABI/host/process/byte/PTY boundaries. Removing dangling helper tests after preserving their full coverage is allowed; deleting the cases is not.

Do not add a permanent test-only copy of the old Rust algorithm. The immutable reference executable is the differential oracle during development. Store small deterministic characterization fixtures for ongoing regression testing.

## 9. Required architecture tests

Implement these with disk-backed XSH fixtures and Rust boundary tests as appropriate. Use test-only counters/hooks for preparation assertions; no public instrumentation flag is needed.

| ID | Required evidence |
| --- | --- |
| A01 | A static trivial program prepares zero script stdlib modules. A simple script-backed call prepares only its module-level dependency closure, not unrelated modules. |
| A02 | Parsing/checking/lowering counters for embedded sources do not increase after execution starts, including the first call, loops, callbacks, stream consumption, failures, and `defer`. |
| A03 | Repeated imports/references do not reparse the same embedded module within one preparation ownership context. |
| A04 | `xshi` submits new inputs through preparation and reuses compatible session-owned implementations without stale IDs or cross-session state. |
| A05 | A program that can `module.load` prepares all applicable implementation dependencies before execution. A dynamically loaded user module referencing a previously otherwise-unused stdlib entry causes no late stdlib preparation. Its source retains ordinary user privileges. |
| A06 | Copied/installed binaries work without loose stdlib files, repo cwd, or project config. `xsh`, `xshi`, and `xsht` exercise representative migrated APIs. |
| A07 | Local `cli.xsh`/`mime.xsh`, misleading `stdlib` directories, `XSH_MODULE_PATH`, project roots, symlinks, aliases, and dynamic exports cannot replace standard implementations. Existing legitimate user import behavior remains intact. |
| A08 | Forged source labels and copied embedded source do not grant private access through parser/loader/Rust facade entrypoints. Provenance is independent of paths and spans. |
| A09 | Private primitive/helper references cannot be obtained, stored, returned, reexported, inserted into records, passed through `Any`, or made available by a user module contract. Existing public callable forms still work. |
| A10 | User callback code cannot invoke private primitives merely because a trusted stdlib frame called it. Same-spelled user helpers cannot capture internal references. |
| A11 | The checker/verifier rejects unauthorized private call targets, missing implementation targets, wrong-owner function IDs, incompatible bindings, and unsupported features. A forged executable test cannot bypass the check by changing only a display span. |
| A12 | Existing reserved-name rules and their exceptions continue to work. No new global internal-name reservation breaks unrelated source. |
| A13 | Preparation executes no library initialization IO, snapshots no cwd/env/command state, and opens no host resources on behalf of these implementations. Context-sensitive values change between calls as before. |
| A14 | Pure/effect checking remains enabled for user and embedded bodies. Only the narrowly identified legacy CLI probes retain their existing compatibility exception; unrelated IO in a pure embedded function is rejected. |
| A15 | Module-level dependencies work across script/native mixed modules, and import cycles are diagnosed. Ordinary function recursion and stack/cancellation/defer behavior remain valid. |
| A16 | Public overloads, method/command forms, specialized IR paths, schema inference, errors, traces, API counts, and supported mocks match the reference. No unreachable native shadow implementation serves one route. |
| A17 | Newline/Unicode/path-byte/error cases behave identically under debug and release, supported native-test/net feature combinations, and both verification platforms. |
| A18 | Whole embedded-catalog validation covers unused bodies too, including target-specific variants. A bad unused embedded source cannot escape all tests just because a normal script does not load it. |
| A19 | Independent program/session creation and teardown do not accumulate stale symbol/function ownership or caller state. Tool workspace reuse does not reintroduce per-file repeated stdlib preparation. |
| A20 | Builtin/private entries remain absent from user-search resolution and public introspection except for the unchanged public API entries; native-only entries retain their direct execution route. |

The encapsulation threat model is XSH user source, imports, and ordinary exposed source-loading APIs. Do not claim protection against malicious Rust embedder code, unsafe memory corruption, or an attacker modifying the executable. This is not a new sandbox.

## 10. Behavior and differential qualification

For every R group and integrated G subgroup, build a deterministic parity corpus before removing the native body. Run it through both reference and candidate binaries with equivalent inputs, cwd/env, fixtures, feature settings, and target. Compare observable results, including error cases, not only exit success.

Use typed XSH assertions for runtime identity and value semantics. Do not flatten everything into JSON: it would lose distinctions this task must preserve. A test driver may emit a stable tagged test record for comparison, but do not add a public tagged serialization API. Exact stdout/stderr/exit tests remain byte comparisons.

Cover empty/minimal/normal/large inputs; Unicode and malformed byte boundaries; negative/zero/maximum integers; invalid schemas; conflicting invalid inputs; absent/null values; nested records/maps/lists; paths with non-UTF-8 bytes where supported; symlinks; missing/denied files; and partial stream consumption. Existing fixtures are the first source of cases.

Use deterministic generated cases with a fixed seed for CLI descriptors/argv, quoting/argv-word strings, MIME parameters/suffixes, INI encoding records, JSON paths, and relevant proc/sysfs text. Use baseline-defined valid/error behavior and keep minimized regression cases. Generated cases must be bounded and must not mutate real system state.

Live Linux observations vary across runs. Use fixed fixture snapshots to compare algorithm output, then separate read-only smoke tests for actual acquisition. Do not normalize away sorting errors, error timing, or missing-field behavior under the guise of handling nondeterminism.

Test preserved public errors at the user callsite as well as internal implementation diagnostics. For host failures compare within the same OS/image; do not demand byte-identical localized macOS and Linux OS error strings.

## 11. Platform and feature verification

### 11.1 Required environment

Run native qualification on `aarch64-apple-darwin`. Run Linux builds and tests inside Docker on `linux/arm64`, targeting `aarch64-unknown-linux-musl` with the repository's target flags. Do not substitute an emulated x86_64 benchmark for native ARM measurements or claim ARM tests prove x86 ABI behavior.

Read `Dockerfile.test`, `dev/docker.xsh`, `dev/test_workflows.xsh`, `dev/internal.xsh`, and `dev/targets.xsh` before running the Linux workflow. The audited test image has architecture-aware aarch64/x86_64 compiler setup, and `cargo dev test linux` routes to an existing privileged Docker workflow. Reuse that machinery rather than building a parallel CI system.

Keep reference/candidate target directories separate, and separate macOS artifacts from Linux artifacts. Match compiler, build profile, allocator, image, feature configuration, CPU allocation, and fixture paths within each comparison. Use container-local storage or Docker volumes for timing-sensitive Linux fixtures; do not compare one build on a macOS bind mount with the other on a native Linux filesystem.

Docker is a Linux verification environment, not proof that every kernel/device qualification is available. Read-only proc/sysfs fixtures and normal runtime tests are required. Probe privileges/capabilities explicitly before the existing privileged harness; record absent loop devices, sysctl restrictions, PID namespaces, or unavailable kernel features rather than labeling skipped tests as passed.

Do not run destructive commands against the macOS host, the Docker VM's real disks, actual account databases, or `/lib/modules`. No host PID namespace, Docker socket inside a test container, or unrestricted host filesystem mounts. Existing privilege tests must operate on their isolated test resources. Do not change Docker Desktop settings or disable security features to force a pass.

A new R/G path cannot rely only on an unavailable hardware test. The R set is chosen to be qualifiable through native/macOS tests and Docker fixtures/read-only Linux behavior. For a G group with a genuine unavailable qualification requirement, retain the native body and report that exact boundary; do not ship an unverified extraction.

### 11.2 Build/test commands

Resolve current command ownership first. At the audited revision these are applicable starting commands, not replacements for reading the current test map:

```sh
cargo build -p xsh -p xshi -p xsht --bin xsh --bin xshi --bin xsht
cargo metadata --no-deps --format-version 1
cargo test --test integration libxsh_api
cargo test -p xsh-registry --lib
cargo test -p xsh --lib modules::signature
cargo test -p xsht --test api
cargo test -p xsht --test integration

target/debug/xsht test --jobs 1 tests/xsh/stdlib
cargo test -p xsh runtime::eval::indexed::full::tests --lib --features native-tests
cargo test --test integration runtime::stack_depth -- --test-threads=1
cargo test -p xsh runner::tests --lib --features native-tests
cargo test -p xsh --test integration runtime:: --features native-tests -- --test-threads=1
cargo test -p xsh --test integration runtime::coverage::xsh_native_tests --features native-tests -- --exact
cargo test --test integration runtime::coverage::runnable_xsh_corpus_is_formatted_and_lints_without_warnings

cargo test
cargo test --workspace
cargo dev check
git diff --check
```

Run targeted checks first, then broad gates after integration. Run equivalent normal/API/native-corpus tests for Linux in the existing container workflow with the correct target/profile paths. Include syntax/checker/loader tests because binding resolution changes, not just module behavior tests. Include `xshi` integration/session coverage from its owning package and the existing runnable `core/`, `dev/`, examples, and showcase corpus.

Record before/after public API outputs:

```sh
target/debug/xsht api
target/debug/xsht api summary --format jsonl
```

Compare public metadata semantically and byte-for-byte where stable. Public signatures, effects, overloads, schemas, docs, and API identities must not change. Native/script binding metadata remains internal. Do not regenerate docs to conceal an API difference.

Run feature builds in separate target directories to avoid accidentally testing a default-feature executable:

```sh
cargo check -p xsh --lib --no-default-features
cargo build -p xsh --bin xsh --no-default-features
cargo build -p xsh --bin xsh --no-default-features --features net
cargo build -p xsh --bin xsh --no-default-features --features native-tests
```

Exercise copied minimal-feature binaries with disk-backed migrated-API smoke scripts; a check-only build is insufficient. Run supported library/registry tests for these combinations as the current Cargo manifests allow. Also cover default workspace features, which at the audit included `native-tests`, `net`, and `tools`. Preserve `net-disabled`, omitted test-module behavior, and unsupported-platform errors. Do not blindly select `--all-features` on macOS.

Use the dedicated Linux privilege workflow only after verifying its isolation/capabilities. Prefer the existing ARM target-selection interface. If the high-level runner cannot start because the current port broke developer stdlib calls, repair the port or use the existing underlying explicit Cargo commands for diagnosis; do not patch developer policy to accept a regression.

### 11.3 Packaging checks

Copy built binaries to a fresh temporary location outside the repository and exercise representative R groups from a working directory without project config. A Linux smoke image containing the executable and required OS runtime dependencies, but no loose stdlib/repository, is a strong packaging test. Do not accidentally rely on the source tree remaining readable by an absolute path.

Test hostile user module search roots at the same time. Verify both normal and no-default-feature executable variants. Build from a clean source checkout/container to prove no untracked local artifact or previous installed XSH is needed to embed the library.

## 12. Fixed performance acceptance

### 12.1 Measure total cost, including preparation

Use matched release builds and the existing `cargo dev bench --fast` regression workflow. Use explicit package/binary builds for additional benchmarks. No distribution-profile experiments or compiler-option changes between reference and candidate.

For each workload let `B` be the reference median duration and `C` the candidate median duration, in milliseconds, measured within one target/environment:

```text
Cold startup passes when:        C - B <= max(0.05 * B, 1.0 ms)
Non-hot end-to-end passes when:  C - B <= max(0.10 * B, 2.0 ms)
```

Cold startup means a fresh executable process through completion of a fixed tiny script, with no previous in-process preparation cache. Include loader/parser/checker/linker/runtime work; do not start the clock after preparation. Warm OS file caches are acceptable when both sides use the same sampling procedure; do not describe those results as disk-cache-cold measurements.

Non-hot end-to-end means the complete designated operation/workload, not an isolated helper chosen to make the relative slowdown look small. Do not amortize stdlib preparation over extra unrelated work, add sleeps, or omit first-call latency. A significant individual helper slowdown is diagnostic, but acceptance follows the fixed real workloads and retained fast-path gates.

Existing native throughput/memory regression gates must not be weakened. Also report first-call and repeated-call timings, prepared-module counts, peak/retained memory, binary size, and code-size accounting. No added O(n²) algorithms or proportional List[Int]/string blowups are acceptable simply because a small input passes the time gate.

### 12.2 Workloads fixed before implementation

Add only a small reproducible runner/fixture set using existing benchmark facilities or standard-library-only host tooling. Freeze the scripts/inputs before comparing candidate performance. Select equivalent existing corpus examples where available, but record their exact paths, arguments, and fingerprints in the baseline ledger.

| Class | Workloads to freeze |
| --- | --- |
| Cold startup | A minimal script using no migrated API; one tiny call to a small script helper; a representative `cli.parse` invocation; CLI help/error rendering; a tiny program with an existing dynamic module-load reference. |
| CLI | Small ordinary schema/argv; a 64-field schema with aliases/defaults/constraints; repeated parsing with long/repeated/positional operands; help generation and invalid-input paths. |
| Text and formatting | Wrapping and fields on small and large Unicode-containing text (including a 1 MiB fixture); padding strings with ANSI sequences; bounded batches of byte-size/duration/checksum-line formatting. |
| Quoting/tokenization | Typical argv, empty/quoted/escaped edge cases, and a fixed large argv-word input. |
| MIME/INI | Fixed MIME parse/lookup cases and representative host-overlay fixture; INI encoding of a fixed 1,000-key multi-section record. |
| JSON | Get/set/remove on nested records/maps/lists, path lengths including a deep case, and JSON-lines encoding of 10,000 small records. Preserve typed non-JSON path-test values outside codec workloads. |
| Environment | Missing/empty/valid/error typed lookups, including invocation changes across scoped environment use. |
| Linux required policy | Fixed os-release/meminfo/module-list/uptime snapshots, short and long lists, partial consumption, malformed later rows. |
| Gated Linux policy | Representative and large fixture trees/tables/index data for every viable G prototype, measured per group before integration. |
| Real commands/tooling | At least one small core command using CLI, one representative self-hosted developer command, and the same project-wide `xsht` check/lint workload on both revisions. |
| Native controls | Existing filesystem walk/copy/lines, bytes/hash, archive, process, and network workloads that should retain native fast paths. |

Use bounded fixtures; these sizes are workload anchors, not permission to add artificial unrelated work. Do not turn microsecond helpers into a gating benchmark with millions of needless repeated calls while ignoring their real use. Conversely, real batch workloads must not be reduced to one invocation to conceal cumulative overhead.

Use paired, interleaved reference/candidate runs after a small identical warmup, at least 30 pairs for short latency workloads, and a second independent round to reproduce a suspected regression. Keep raw samples and report medians and dispersion; use more samples only when needed to resolve measurement noise. A noisy result does not count as a pass or a proven regression. Prefer the existing runner's stronger sampling/regression method when available.

Do not benchmark through `cargo run`, include compilation time, or measure one revision on different CPU/container allocations. Include startup preparation in every cold-start sample. Compare macOS only to macOS and Linux only to Linux; a Linux image's performance cannot clear a macOS regression.

### 12.3 Failure handling

Performance acceptance is cumulative: every integrated change is compared with the same immutable starting baseline, not only the immediately preceding phase. Do not consume the allowed budget anew for each group.

A G prototype that fails a reproduced gate is removed from the shipped implementation and retained natively with its measurements. A mandatory R group or the common architecture failing a gate remains incomplete until corrected within the agreed architecture. Do not silently drop that group, invent a native fallback, increase the threshold, move work out of the measured region, or switch to build-time freezing without a separate owner decision.

Finish independent groups while investigating. If the constraints genuinely conflict, provide the exact failing workload, parity status, measured delta, and smallest outstanding issue. Do not repeat architecture questions already decided here or claim complete unattended success with an unmet mandatory gate.

## 13. Implementation order and review boundaries

**Phase A — Reference and scope ledger.** Record starting revision; establish exact public bindings/native callers; save API/test baselines and fixed workloads; verify macOS and Docker ARM environments. Create the R/G/deferred ledger. Do not stop at this report.

**Phase B — Minimal vertical slice.** Implement embedded-source identities, bindings, preparation/linking, private-access checks, and one small migrated group plus mixed TUI/native behavior. Prove copied-binary execution, zero late stdlib preparation, sealed names, and native-only fast-path preservation before expanding infrastructure.

**Phase C — Required high-value libraries.** Complete CLI, argv-word parsing, MIME, JSON path policy, INI encoding, and the remaining R general-library groups. Stabilize the few shared representation/error/context primitives once rather than adding per-module escape hatches. Differential-test and delete old bodies group by group.

**Phase D — Required Linux policy.** Complete R12 with target-aware bindings and correct eager/lazy behavior. Qualify fixed fixtures on both platforms and actual read-only host acquisition on Docker Linux. Leave privileged/daemon/account operations native.

**Phase E — Resolve G gates.** Evaluate each G group independently under the boundary and performance rules. Keep complete evidence for retention decisions. Do not use G work to postpone failures in required groups or to expand into Deferred projects.

**Phase F — Acceptance and deletion.** Remove dead native algorithms/dispatch and only truly unused dependencies; run the full API, architecture, feature/platform, corpus, packaging, and performance matrix. Inspect all generic/specialized routes, ownership teardown, and private-reference escapes. Finish net code accounting and canonical architecture docs.

Use small reviewable changes. Independent implementation groups may be delegated after the binding/private ABI contract is stable. Keep one owner for common loader/checker/IR changes and serialize integration; do not let multiple workers independently invent private operations or edit the central dispatcher inconsistently. Follow existing repository agent rules rather than adding orchestration infrastructure.

No production dual backends, compatibility interpreter, speculative caching framework, or unrelated cleanup. Tests may compare an immutable old executable, but shipped code must have one implementation of each migrated policy.

## 14. Deliverables and final status

Deliver these concrete artifacts in the repository:

1. Embedded XSH implementations for all R groups and every G subgroup that passes its gates, integrated behind the unchanged public APIs.
2. The minimal source catalog, implementation bindings, preparation/linking, provenance/private-operation verification, and call-boundary integration described here, using the existing runtime.
3. Focused native XSH behavior tests, compiler/verifier/host boundary tests, architecture checks A01–A20, and minimized deterministic parity fixtures. Preserve original coverage and map migrated helper unit cases.
4. A small repeatable baseline/candidate performance and packaging verification path, plus machine-readable raw results outside production source and concise checked-in summaries. Do not commit giant logs/binaries or add CI.
5. `STDLIB-PORT.md` with every R/G group resolved, every Deferred area accounted for, native caller/bridge justifications, removed symbols, source destinations, tests, timings, and status. Update `docs/ARCHITECTURE.md` and the nearest canonical docs for the new internal boundary without duplicating the public reference or regenerating documentation.

The final report must distinguish:

- Algorithms that now execute in XSH versus unchanged native operations with new adapters.
- Production Rust deleted, production Rust added for integration, net maintained Rust change, XSH implementation added, generated data/code, and test-only changes. Use the same counting method on both revisions and include all affected crates. There must be net production Rust deletion; moving Rust to a new crate or hiding it in generated output is not a reduction.
- Every private bridge's exact purpose and permitted caller(s); no whole-policy algorithm hidden inside one.
- R completion and every G subgroup's final `ported`, `retained-boundary`, or `retained-performance` evidence.
- Exact commands executed, platform/target/features/profile/image/toolchain, passed tests, unchanged pre-existing failures, environmental skips, and unexecuted qualification. Do not describe a capability check or compile-only result as a runtime test pass.
- Performance deltas against the fixed gates, including cold startup and dynamic-loading preparation, plus memory and native fast-path results.

Use `complete` only when all mandatory implementation and verification obligations pass and every G group is resolved. An unaffected pre-existing test/environment exclusion must be documented with matching reference evidence; it is not a new pass. Any new failure, required behavior/performance failure, unqualified changed path, unresolved gate, duplicated required policy, or late stdlib compilation makes the report `incomplete`, with exact remaining items. Finish all independent safe work even when one item is blocked.

Do not claim the Rust core is now theoretically minimal or guarantee that no future improvement exists. Report the achieved boundary: ordinary prepared XSH owns the listed library policy; Rust owns the execution substrate, existing efficient kernels, and retained host guarantees.

## 15. Source references and audit navigation

Implementation and test details in this specification come from the source audit at the pinned revision, not from measured prototype results. All references below are paths in `laputa-systems/xsh`; inspect the corresponding owners at the actual working revision before editing.

| Finding/contract | Primary source |
| --- | --- |
| Philosophy and work rules | `AGENTS.md`; `docs/CHAPTER-01-why-xsh.md` |
| Public APIs and effects | `docs/SPEC.md`; `crates/xsh-registry/src/signature/modules.rs`; `methods.rs`; `streams.rs`; `src/modules/signature.rs` |
| Standard modules bypass user source loading | `src/loader.rs::load_uses`; `resolve_user_module` |
| Current executable architecture and specialized call paths | `docs/ARCHITECTURE.md`; `src/runtime/eval/indexed/full.rs`; `src/runtime/eval/lower.rs`; `lowered_run.rs`; `lowered_run/indexed_run/` |
| Dynamic symbol lifetime | `src/symbol.rs::SymbolOwner` |
| Name reservation and exceptions | `docs/SPEC.md`, lexical rules and standard-module sections |
| Contextual CLI defaults and native dispatch | `src/runtime/eval/lowered_run.rs`; `src/modules/cli.rs` |
| Record/error representation gaps | `crates/xsh-registry/src/signature/methods.rs`; `src/runtime/value.rs`; `src/runtime/eval/modules.rs` |
| Required general policies | `src/modules/{shlex,cli,process,mime,tui,bytes,time,text,hash,ini,json}.rs`; relevant runtime dispatch |
| Shared INI bootstrap decoder | `crates/xsht/src/cli/files.rs::load_config_from`; `src/lib.rs` host facade |
| Linux required/gated policies | `src/modules/system.rs`; `src/modules/unix.rs`; `src/modules/linux/{kernel,block,process}.rs`; `src/modules/linux/real/{kernel,net,fs,boot,device,mount}.rs` |
| Deferred account and mdev boundaries | `src/modules/{user,group}.rs`; `crates/xsh-applets/src/mdev.rs`; `src/runtime/eval/modules/host.rs` |
| Test/feature ownership | `docs/TEST-MAP.md`; `Cargo.toml`; `tests/xsh/stdlib/`; `tests/runtime/`; package integration tests |
| ARM Linux verification route | `Dockerfile.test`; `dev/docker.xsh`; `dev/test_workflows.xsh`; `dev/internal.xsh`; `dev/targets.xsh` |

External architectural context is informative, not an instruction to copy another runtime: CPython distinguishes ordinary source modules from frozen compiled modules, and Docker Desktop runs Linux containers in a managed Linux VM. This task deliberately chooses source embedding plus XSH's own preparation/runtime, not CPython's mutable import system or frozen-code packaging.

Primary external references:

```text
https://docs.python.org/3.14/reference/import.html
https://docs.python.org/3.14/c-api/import.html
https://docs.docker.com/desktop/setup/install/mac-permission-requirements/
```
