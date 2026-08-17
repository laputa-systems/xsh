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
- `lint.rs` runs lint analysis over checked programs, including the
  `lint.dead-code` reachability detector, and applies safe autofixes.
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

The default `module_path` is `.` (the current working directory). A config file
may set `module_path` explicitly to replace that default for projects whose
modules live in another directory.

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

When a lint can report a real issue but cannot safely preserve nearby comments,
it should report the diagnostic without a fix hint. This is better than
silently moving comments or relying on final formatting to reconstruct intent.

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
the entry `proc main`, root `proc test_*` native-test entry points, exports,
root signal hooks, and module initializers. Values, imports, and type
declarations are deliberately outside this warning: they have initialization,
API, or type-use contracts that reachability alone cannot prove dead.

Focused coverage belongs in `crates/xsht/tests/lint.rs`. It must cover both a
proven warning and a false-positive boundary for each flow rule, plus exports,
recursion, dynamic callable values, native tests, entry functions, and
cross-module calls. The relevant broader gate is `cargo test -p xsht --test
integration` from `docs/TEST-MAP.md`.

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
