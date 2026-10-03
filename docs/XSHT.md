# XSHT Tooling Architecture

XSH tooling treats quality recommendations as guidance rather than hidden
language restrictions. In particular, `lint.path-constructor` recommends
path-string syntax for `Path(str)` while allowing the documented direct cast to
remain a valid typed-`Path` boundary. The recommendation is non-fatal so a
contract that names `Path(...)` can satisfy both lint and its own restriction.

`xsht` is the tooling frontend for XSH source files. It owns checks, linting,
formatting, source annotation, structural search, refactoring, API queries,
native tests, and coverage reports. Script execution remains in `xsh`; `xsht`
may parse, check, and evaluate only when a tooling command explicitly requires
that behavior.

## Greppable Tooling Vocabulary

Use these symbols as the retrieval handles for tooling work. The `xsht::cli`
module path supplies the needed context for names such as `CliOutput`; do not
add redundant product prefixes to already-qualified symbols.

| Concern | Canonical symbols | Owner and coverage |
|---|---|---|
| command entry and result contract | `xsht::app::main`, `xsht::app::finish`, `xsht::cli::CliOutput` | `crates/xsht/src/app.rs`, `crates/xsht/src/cli/mod.rs`; `crates/xsht/tests/cli.rs` |
| generated command help | `root_help`, `command_help` | `crates/xsht/src/help.rs`; generated-output coverage in `crates/xsht/tests/cli.rs` |
| checked command pipeline and reachability diagnostics | `check_script`, `format_files`, `lint_files`, `lint.dead-code`, `lint.unused-callable` | `crates/xsht/src/cli/check.rs`, `fmt.rs`, `lint.rs`; CLI and lint integration tests |
| structural search and refactoring | `find_matches_in_program`, `PatternExpr`, `Match`, `apply_replacement` | `crates/xsht/src/grep.rs`; `crates/xsht/tests/grep.rs` |
| command adapters | `api_command`, `grep_scripts`, `refactor_scripts`, `ast_script` | `crates/xsht/src/cli/api.rs`, `grep.rs`, `refactor.rs`, `syntax_tree.rs`; `crates/xsht/tests/api.rs`, `grep.rs`, and `cli.rs` |
| source-preserving edits | `SyntaxTree`, `apply_cst_guarded_edits`, `Formatter` | `crates/xsht/src/edit.rs`, `format.rs`, `cli/fmt.rs`; formatter coverage in `crates/xsht/tests/cli.rs` |

`CliOutput` is the shared `xsht` command result, not an XSH runtime output
type. `Match` is the structural result produced by `xsht::grep`; its module
qualification is intentional and sufficient.

## Ownership

Command dispatch starts in `xsht::app::main` in `crates/xsht/src/app.rs` and
the `xsht::cli` module in `crates/xsht/src/cli/mod.rs`. Generated command help is
owned by `crates/xsht/src/help.rs`; it renders the task-oriented root index and the
same metadata for individual command help. Each command has a focused module under
`crates/xsht/src/cli/`:

- `check.rs` runs parser, module loading, checker, and optional source
  annotation.
- `fmt.rs` checks the resolved program bundle through the shared program
  pipeline, then applies the formatter from `crates/xsht/src/format.rs`.
- `lint.rs` builds one shared arena-backed import graph, selects true entry
  roots for directory discovery (while preserving explicitly named files as
  roots), checks each reachable program bundle, and applies safe autofixes
  once per source. This keeps imported modules out of standalone lint passes
  and makes `lint.dead-code` reachability operate on the complete bundle.
- `grep.rs` and `refactor.rs` use AST-aware structural matching.
- `api.rs` renders the canonical registry for batch API queries.
- `files.rs` owns configured file discovery and `xsht-config.ini` parsing.

The shared language pipeline stays in the main `xsh` crate. `src/syntax`
lexes, parses, and builds both the semantic AST and the lossless CST.
`src/sema` and `src/sema/check.rs` own checking. Runtime evaluation
stays in `src/runtime`.

Tooling imports these representations through the `xsh::frontend` façade:
`frontend::load` owns loading/checking entry sources, `frontend::syntax` owns
AST/CST and parser types, and `frontend::check` owns checker facts and semantic
types. These are first-party tooling APIs rather than a general-purpose host
SDK. Structured trace events and traceback data come from
`xsh::trace::model`; text, JSONL, flamegraph, syscall, and terminal-table
presentation are owned by `xsht::trace`.

## Shared Substrate

Shared `xsht` infrastructure is split between the runtime crate and the CLI
crate by ownership:

- `config.rs` resolves file-specific `xsht-config.ini` state and derived paths.
- `src/loader.rs` in the main `xsh` crate builds a `CheckedEntry` from an
  entry source by parsing, module loading, desugaring, and checking the
  resolved program bundle. The bundle may contain multiple sources, but it
  preserves module boundaries in `Program.modules` rather than inlining modules
  into the entry source. `check`, `fmt`, and `lint` should consume this
  checked-program representation rather than constructing parallel
  parser/checker pipelines.
- `edit.rs` applies CST-guarded source edits and formats validated output.

`xsht lint`'s initial analysis resolves and parses each source path once per
command. Directory
roots are files with no inbound import edge; strongly connected import cycles
select one deterministic component root. An explicitly named file remains a
root even when another named file imports it. Imported modules are linted only
when reachable from one of those roots, and command-level diagnostic keys keep
shared-source diagnostics unique.

`xsht fmt` retains its per-file source-preserving formatting workflow, but
deduplicates shared parse/check diagnostics at command aggregation so a module
error is rendered once even when several formatted files import that module.

CLI command modules should call these helpers for common setup and rewrite
safety. Command-specific result aggregation, exit-code policy, and output text
stay in the individual `crates/xsht/src/cli/*.rs` modules.

## API Queries

`xsht api` is the standalone first-contact reference for XSH. It is a projection
of the canonical language and standard-library metadata, not a source or test
index and not a generated Markdown manual. With no selector it prints a compact
onboarding guide containing a valid script, the `xsht check`, `xsht fmt`, and
`xsht lint` loop, the `xsh SCRIPT` run command, and representative discovery
queries. The onboarding script is part of the executable contract: the API
tests extract it and run `xsht check` against it. JSONL mode emits one structured
guide object for this no-selector form.

Batch selectors preserve request order and may mix exact lookups with
deterministic search:

```sh
xsht api
xsht api api:json.read method:Path.read_text record:FsEntry language:run.status
xsht api --format jsonl --strict api:archive.tar_extract search:"rooted extraction"
xsht api summary
```

The query forms are `summary`, `module:NAME`, `api:MODULE.FUNCTION`,
`method:RECEIVER` or `method:RECEIVER.METHOD`, `record:NAME`, `language:ID`, and
`search:TERMS`. A bare module or receiver query returns its overview and member
index; an exact `api:` or member query returns the full item. `language:ID`
accepts an exact item or a prefix such as `language:core`. Search matches IDs,
purposes, contracts, and retrieval tags. Exact and language-reference queries
default to full details; module groups and search default to compact purposes.
`--details basic|full` overrides that choice. `--query-file`, `--stdin`,
`--strict`, and `--format jsonl` are available for batch and machine-readable
use. `summary` is exclusive of selectors and query inputs; its text and JSONL
forms contain the complete sorted module/function tree, method receiver tree,
record list, language-reference groups, and inventory counts.

Full API items expose a caller-facing purpose and, when applicable, a contract,
derived effects, signatures, retrieval tags, and a short XSH example. Contracts
carry only constraints needed to avoid a wrong program, such as ownership,
cleanup, rooted boundaries, ordering, platform limits, status-versus-error
distinctions, or text/byte boundaries. Effects come from the checked signature
metadata (`none` means no host capability); a fallible return does not itself
require the `error` effect. A contract may be empty when the purpose and
signature already cover the behavior. Results do not expose Rust operation names,
implementation paths, or test references.

Examples are maintained as XSH snippets under `docs/snippets/api/`; the registry
maps snippets to API IDs and embeds their contents in API output. Metadata stays
beside the language surface: module and method docs live in
`crates/xsh-registry/src/signature/`, record docs live with the record API
definitions, language rules live in `crates/xsh-registry/src/reference.rs`, and
`crates/xsht/src/api.rs` only selects, derives, and renders the registry. The
registry rejects missing or empty public documentation and unknown documentation
entries; it does not maintain a parallel table of implementation paths or tests.

Use `--query-file PATH` or `--stdin` to add one selector per line to the same
request. `crates/xsht/src/api.rs::query` renders and derives the registry;
`crates/xsht/src/cli/api.rs::api_command` owns CLI result conversion; and
`crates/xsht/tests/api.rs` covers onboarding, selectors, contracts, effects,
examples, JSONL, strict mode, query files, stdin, module and receiver indexes,
and the exhaustive summary. Registry tests verify that the public signature
surface and its documentation inventory agree.

## Configuration

Tooling configuration is read from `xsht-config.ini`. The current working
directory config controls no-argument discovery. File-oriented commands that
operate on explicit or discovered files use the nearest `xsht-config.ini` in
each file's ancestor directories when command behavior is file-specific.
For `check`, `fmt`, and `lint`, an explicit directory scans only that directory
with its nearest config's `exclude` patterns; `include` extends only the
no-argument scan.

Relative paths from a config file are resolved from that config file's
directory. Invalid config is a command error, not a silent fallback, except that
a missing config file means defaults.

The optional `[coverage]` section accepts `exclude` patterns for files omitted
from `xsht test --cov` source coverage. These patterns affect coverage
registration only; they do not change `xsht check`, `xsht fmt`, or `xsht lint`
discovery. The ordinary top-level `exclude` remains the shared discovery filter
for path-oriented commands.

The optional `[dead-code]` section accepts `exclude` patterns for files where
`lint.dead-code` and `lint.unused-callable` should not be reported. These files
still run through the other lint rules. For example, API documentation snippets
can opt out without becoming globally invisible to `xsht lint`:

```ini
[dead-code]
exclude = docs/snippets/**/*.xsh
```

Native tests capture `process.run` stdout and stderr per test by default. `xsht
test` shows that output for failed tests; `xsht test --nocapture` shows it while
tests run. Normal XSH execution continues to inherit child process streams.
Without `--jobs N`, `xsht test` runs half the logical CPUs' worth of tests
concurrently (at least 1, at most 8) so a default run leaves room for the
subprocesses tests start; an explicit `--jobs N` wins. Each `test.run_script`,
`test.run_xsh`, and `test.run_xsht_trace` child leads a runner-owned process
group. On SIGINT or SIGTERM the runner sends SIGTERM to those groups, which lets
an `xsh` child forward it to the process groups it created, waits up to one
second, sends SIGKILL to what remains, and exits with `128 + signal`. Covered by
`cli::test_runner_cancellation_stops_run_script_descendants`.
`crates/xsht/src/xsht/test.rs` reports indexed-lowering failures as failed
test files or selected test procedures, using the same source diagnostic as
`xsht check`; preparing a native test must not panic on unsupported source.

The default `module_path` is `.` (the current working directory). A config file
may set `module_path` explicitly to replace that default for projects whose
modules live in another directory. `xsht test` also gives those roots to
`module.load`, so a loaded module's `use` imports resolve like the test file's
(`stdlib/module.xsh::test_module_load_resolves_uses_with_configured_test_module_roots`).
A failed `module.load` names the module and its first diagnostics with their
source locations.

## Source Representations

The AST and CST have different jobs.

The AST is semantic. Checkers, lints, lowering, runtime evaluation, and broad
structural analysis should use it. AST nodes carry source spans, but the AST is
not source-faithful: comments, exact whitespace, delimiter trivia, and some
layout choices do not belong there.

The CST is source-faithful. It retains tokens, comments, whitespace, newlines,
skipped source gaps, delimiters, interpolation groups, and exact source spans.
Tooling that rewrites or formats source should use the CST to answer source
fidelity questions:

- Does this span contain comments?
- Which original tokens and trivia are inside this node or range?
- Can this edit be applied without moving undocumented source?
- Which comments are leading, trailing, or nested relative to a syntax range?

The intended direction is AST analysis plus CST-backed source edits. A tool may
use the AST to decide what change is correct, but the edit should be represented
as source syntax over a CST range and validated after application.

## Formatting

Formatter-specific design, layout policy, source-shape handling, configuration,
and corpus ownership live in `docs/XSHT-FMT.md`. The implementation entry points
are `Formatter` in `crates/xsht/src/format.rs` and `format_files` in
`crates/xsht/src/cli/fmt.rs`.

At the architecture level, formatting is AST-guided and CST-aware: the AST
supplies semantic shape and precedence, while the CST supplies comments,
source spans, and meaningful layout clues. `xsht fmt` validates the checked
program before writing source and preserves the rewrite invariants described in
`docs/XSHT-FMT.md`.

## Autofixes

`xsht lint --fix` is conservative. Lints are allowed to analyze the checked AST,
but fix application uses non-overlapping source edits guarded by the CST.

`xsht lint --only RULE[,RULE...]` keeps only diagnostics with the named codes,
so `--fix` applies only their fixes, including syntax migrations. Codes are
validated against `lint::LINT_CODES`; an unknown code is a usage error.

Safe fixes must satisfy all of these:

- the original file loads, parses, and resolves imports; safe fixes may run when
  checker diagnostics already exist, provided the edit does not introduce new
  checker diagnostics;
- the fix is represented as a source replacement over an original span;
- overlapping fixes are resolved before application;
- a replacement span containing comments is skipped unless the fixer explicitly
  handles those comments;
- the edited source parses, resolves imports, and formats; it may retain only
  checker diagnostics that were already present before the edit.

Type annotations that supply optional branch or record collection context stay
in place. `lint.needless-annotation` cannot use a contextually checked type as
proof that inference without the annotation will agree. Collection rewrites
retain record element annotations, and `lint.redundant-require` retains schema
checks on unconstrained `Record` values. Tail-return fixes keep explicit returns
when a conditional or fallback would be parsed as a statement or command.
`lint.prefer-guard` preserves the complete condition spelling, including the
closing parentheses around pipelines.

Workspace lint traversals copy only checked facts belonging to the current
source. Span-ordered fact ranges keep shared imports from multiplying the
expression and statement data cloned for each file; declaration contracts
remain available in the checked bundle.

Annotation-removal proofs cache the original checked type shapes for each
source. `Linter::local_annotation_removal_preserves_contract` still checks every
candidate against the complete expression contract; an unavailable original
contract prevents candidate checks. Standalone proof parses refuse unresolved
user imports before body checking because they cannot supply a valid baseline.
Named user types retain their explicit domain,
including exported types; these annotations do not enter removal probes.
`lint.unused-type` counts both the generic head and applied arguments as type
references, so a type used only in `CollectedDevices[Row]` remains used.

`lint.prefer-record-constructor` replaces checked schema-typed record literals
with static named-field construction while retaining the annotation. It preserves
field evaluation order and omits explicit defaults only for identical bounded
literal values. Comments and record spreads prevent an automatic replacement.

`lint.prefer-named-argument-pun` shortens checked `name: name` expression-call
arguments to `name:`. The value must be that lexical identifier; another binding
or a field selection does not qualify. Existing puns are stable, and an argument
with an internal comment receives a warning without a destructive fix.

`lint.unannotated-effects` and `lint.missing-effects` apply to exported procs,
streams, and other procs whose effects are not inferred. They skip native `test`
declarations, `cli main`, and an unexported top-level `proc main`: these entries
are already unrestricted and no restricted caller can reach them. A clause that
is present still bounds the body through the checker.

When a lint can report a real issue but cannot safely preserve nearby comments,
it should report the diagnostic without a fix hint. This is better than
silently moving comments or relying on final formatting to reconstruct intent.

`xsht lint --fix` retains diagnostic keys while validating rewritten files, so
an error reported by an imported module is emitted once per command even when
several entry files reach that module.
Each convergence round checks the rewritten import graph; the final round's
diagnostics remain attached to that exact source instead of loading and checking
the same graph again before writing it.

Workspace linting parses imported sources once and selects each entry's
dependency closure from a shared module index in
`LintWorkspace::configure_program_for`. Entry bundles preserve canonical module
identities and retain documentation only for reachable sources.
Linting uses at most four workers, bounded by available CPU parallelism and
entry-root count. Workers share one immutable arena for type references.
It carries no module membership: the
entry bundle determines whether an enum belongs to the root or an import.
Imported identities use canonical module keys, so matching file names in
different directories remain distinct nominal domains.

`lint.prefer-known-field-access` replaces a propagated literal-key get only
for an ordinary materialized record with an identical checked field type.
Literal records, local constructors, and immutable aliases establish that
provenance. Static record shape alone does not establish that a host-backed
receiver's get cannot fail. Nullable field values remain nullable.

`lint.lookup-fallback` accepts compatible literal data and checked immutable
binding reads, including parameters. These reads cannot observe a different
value when moved from an eager argument to a lazy fallback. Mutable reads,
calls, failing expressions, comments, and named argument order remain guarded.
Path command rewrites also require evidence that removing a text conversion
preserves bytes; an arbitrary Path can contain non-UTF-8 bytes that its text
display replaces.

## Reachability Diagnostics

`Linter` in `crates/xsht/src/lint.rs` owns two proof-oriented reachability
warnings. `lint.dead-code` reports a statement with no path from the preceding
statement in its enclosing body. `lint.unused-callable` reports an unexported
top-level `proc`, `pure`, or `stream` that has no path from a checked-program
bundle entry point. Neither warning has an automatic deletion fix.

Statement reachability uses a flow summary with normal fall-through, `return`,
`break`, `continue`, and guaranteed termination exits. Sequences issue one
warning at the start of a contiguous unreachable region, while still visiting
the remaining statements for independent diagnostics. Branches join their
possible exits; `while` and `for` retain a zero-iteration path, whereas `loop`
consumes its body's loop-control exits. `with`, `guard`, match arms, retry
attempts, pipeline/task blocks, and signal-hook bodies retain their own
control-flow boundaries.

Failure alone is not termination: fallible calls, `run`, `?`, `with`, and
dynamic dispatch retain a possible normal path. The exceptions are the
guaranteed `match-no-arm` runtime exit and a resolved core `abort(...)` call.
The checker records the latter in `CheckOutput::terminating_call_spans`; the
lint CLI passes that fact through `LintOptions` rather than inferring it from a
callee spelling or effect annotation.

Callable reachability is a separate graph over `ArenaProgram`, including loaded
modules. Resolved local calls and resolved imported calls are graph edges. A
resolved callable used as a value is a dynamic escape edge, activated only when
its enclosing root or callable is live. Roots are entry top-level execution,
the entry `proc main`, explicit `test NAME` native-test entry points, exports,
root signal hooks, and module initializers. Values, imports, and type
declarations are deliberately outside this warning: they have initialization,
API, or type-use contracts that reachability alone cannot prove dead.

Focused coverage belongs in `crates/xsht/tests/lint.rs`. It must cover both a
proven warning and a false-positive boundary for each flow rule, plus exports,
recursion, dynamic callable values, native tests, entry functions, and
cross-module calls. The relevant broader gate is `cargo test -p xsht --test
integration` from `docs/TEST-MAP.md`.

`lint.prefer-in` migrates removed standard membership methods and helpers;
`lint.prefer-bare-assertion` migrates statement-use `test.ok`, `test.eq`, and
`test.ne` to `assert` statements. The checker supplies statement consumers and resolved standard-call
identity even for the narrowly diagnosed removed APIs. Callable compatibility
aliases are not retained. Fixes use the CST for source spans and comment
protection, preserve custom messages and consumed Results, compose nested
membership edits, and are checked again before writing. Ambiguous dynamic
receivers and guarded operand-order hazards require explicit manual bindings;
whole statement fixes can introduce hygienic bindings at the original point.

`lint.explicit-assert` prefixes `assert ` to every statement in
`CheckOutput::assertion_spans` that is not already an `assert`, so the checker,
not syntax, decides which parenthesized, multi-line, Unit-tail, and
bare-identifier statements assert. An unbraced match arm becomes
`{ assert ... }` because `assert` would read the arm's comma as its message
separator. The rule runs only when selected with `--only lint.explicit-assert`
until the tree is migrated; a selection of only this rule applies its edits
without reformatting the file.

## Structural Search And Refactor

`xsht grep` and `xsht refactor` use AST-aware patterns so searches survive
whitespace and layout differences. Refactor replacements still operate on
captured source spans. They are not equivalent to formatting; users should run
`xsht fmt` after refactors.

Future refactor work should reuse the same CST-backed source-edit path as lint
autofixes so comment and overlap behavior is consistent across tooling.

## Verification

For parser, CST, formatter, and source-rewrite changes, start with targeted
tests in `tests/syntax.rs` or `crates/xsht/src/cli/lint.rs`, then run the
broader command from `docs/TEST-MAP.md`.

Important cases:

- exact CST reconstruction of original source;
- comment and whitespace classification;
- statement-leading, statement-trailing, nested, and `fmt: skip` comments;
- formatter idempotency;
- whitespace improvement for cramped but uncommented source;
- autofix rejection for comment-bearing spans;
- autofix success for comment-free spans;
- parse/check validation after rewritten source.
