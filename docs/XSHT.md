# XSHT Tooling Architecture

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
| checked command pipeline | `check_script`, `format_files`, `lint_files` | `crates/xsht/src/cli/check.rs`, `fmt.rs`, `lint.rs`; CLI and lint integration tests |
| structural search and refactoring | `find_matches_in_program`, `PatternExpr`, `Match`, `apply_replacement` | `crates/xsht/src/grep.rs`; `crates/xsht/tests/grep.rs` |
| command adapters | `api_command`, `grep_scripts`, `refactor_scripts`, `ast_script` | `crates/xsht/src/cli/api.rs`, `grep.rs`, `refactor.rs`, `syntax_tree.rs`; `crates/xsht/tests/api.rs`, `grep.rs`, and `cli.rs` |
| source-preserving edits | `SyntaxTree`, `apply_cst_guarded_edits`, `Formatter` | `crates/xsht/src/edit.rs`, `format.rs`, `cli/fmt.rs`; formatter coverage in `crates/xsht/tests/cli.rs` |

`CliOutput` is the shared `xsht` command result, not an XSH runtime output
type. `Match` is the structural result produced by `xsht::grep`; its module
qualification is intentional and sufficient.

## Ownership

Command dispatch starts in `xsht::app::main` in `crates/xsht/src/app.rs` and
the `xsht::cli` module in `crates/xsht/src/cli/mod.rs`. Each command has a focused module under
`crates/xsht/src/cli/`:

- `check.rs` runs parser, module loading, checker, and optional source
  annotation.
- `fmt.rs` checks the resolved program bundle through the shared program
  pipeline, then applies the formatter from `crates/xsht/src/format.rs`.
- `lint.rs` runs lint analysis over checked programs and applies safe autofixes.
- `grep.rs` and `refactor.rs` use AST-aware structural matching.
- `api.rs` renders the canonical registry for batch API queries.
- `files.rs` owns configured file discovery and `xsht-config.ini` parsing.

The shared language pipeline stays in the main `xsh` crate. `src/syntax`
lexes, parses, and builds both the semantic AST and the lossless CST.
`src/sema` and `src/sema/check.rs` own checking. Runtime evaluation
stays in `src/runtime`.

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

`xsht api` is the standalone first-contact reference for XSH. With no selector
it prints a tiny valid script and the basic `xsht check`, `xsht fmt`, `xsht
lint`, and `xsh SCRIPT` loop. It reads the canonical API registry; it does not
parse generated Markdown or maintain a second documentation table. Batch
selectors preserve request order and may mix exact lookups with deterministic
search:

```sh
xsht api
xsht api api:json.read method:Path.read_text record:FsEntry language:run.status
xsht api --format jsonl --strict api:archive.tar_extract search:"rooted extraction"
xsht api summary
```

`module:NAME` prints the module overview and its member index. `method:NAME` prints the receiver overview and its member index; `method:NAME.MEMBER` reads one exact item. Exact API
queries print purpose, contract, derived effects, signatures, tags, and a short
example when one is useful. `xsht api summary` prints the complete sorted
module/function tree, method receiver tree, record list, and language-reference
groups after a compact count header. `--format jsonl` returns the same
inventory as structured arrays.

API examples are maintained as XSH snippets under docs/snippets/api/; the
registry maps them to API IDs and xsht api returns their contents.

Use `--query-file PATH` or `--stdin` to add one selector per line to the same
request. `crates/xsht/src/api.rs::query` renders and derives the registry;
`crates/xsht/src/cli/api.rs::api_command` owns CLI result conversion; and
`crates/xsht/tests/api.rs` covers onboarding, text, JSONL, strict, file, stdin,
module, and exact-item queries.

## Configuration

Tooling configuration is read from `xsht-config.ini`. The current working
directory config controls no-argument discovery. File-oriented commands that
operate on explicit or discovered files use the nearest `xsht-config.ini` in
each file's ancestor directories when command behavior is file-specific.

Relative paths from a config file are resolved from that config file's
directory. Invalid config is a command error, not a silent fallback, except that
a missing config file means defaults.

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

- the original file loads, parses, resolves imports, and checks;
  effect-annotation fixes may also run when the only checker diagnostics are
  effect violations that the fixer can remove;
- the fix is represented as a source replacement over an original span;
- overlapping fixes are resolved before application;
- a replacement span containing comments is skipped unless the fixer explicitly
  handles those comments;
- the edited source parses, resolves imports, checks, and formats before it is
  written.

When a lint can report a real issue but cannot safely preserve nearby comments,
it should report the diagnostic without a fix hint. This is better than
silently moving comments or relying on final formatting to reconstruct intent.

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
