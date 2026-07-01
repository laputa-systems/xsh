# XSHT Tooling Architecture

`xsht` is the tooling frontend for XSH source files. It owns checks, linting,
formatting, source annotation, structural search, refactoring, docs commands,
native tests, and coverage reports. Script execution remains in `xsh`; `xsht`
may parse, check, and evaluate only when a tooling command explicitly requires
that behavior.

## Ownership

Command dispatch starts in `crates/xsht/src/app.rs` and
`crates/xsht/src/cli/mod.rs`. Each command has a focused module under
`crates/xsht/src/cli/`:

- `check.rs` runs parser, module loading, checker, and optional source
  annotation.
- `fmt.rs` checks the resolved program bundle through the shared program
  pipeline, then applies the formatter from `crates/xsht/src/format.rs`.
- `lint.rs` runs lint analysis over checked programs and applies safe autofixes.
- `grep.rs` and `refactor.rs` use AST-aware structural matching.
- `files.rs` owns configured file discovery and `xsht-config.ini` parsing.
- `docs.rs` owns generated docs commands.

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

`xsht fmt` is a formatter, not a byte-preserver. It normalizes ordinary layout
toward the XSH style while preserving comments and meaningful blank lines.

The formatter uses the AST for semantic shape and precedence, and the CST for
comments and source-faithful trivia decisions. It targets
`format.line-width` from the nearest `xsht-config.ini`, defaulting to 120
columns. The line width is a layout target, not a guarantee; unbreakable
strings, paths, comments, and `fmt: skip` regions may exceed it.

Comments must stay attached to the construct they document. A leading comment
before a statement remains a leading comment. A same-line trailing comment after
a complete statement remains on that statement. A statement containing nested
comments should not be regenerated from the AST unless the formatter can
reattach those comments deliberately.

`# fmt: skip` applies to the next statement and preserves that statement's raw
source.

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
