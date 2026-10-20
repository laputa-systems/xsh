# XSHT Tooling

`xsht` checks, formats, lints, tests, and inspects XSH source. `xsht test` runs
native tests, `xsht trace` runs a traced script, and the private
`xsht runtime-stats` command runs one script with allocation and phase reports.
Use `xsht help` or `xsht help COMMAND` for the public commands and options; the
registry in `crates/xsht/src/commands.rs` owns that interface. The plain `xsh`
runner rejects tracing flags. Script stdout and stderr stay undecorated;
diagnostics go to stderr.

## Configuration

The nearest `xsht-config.ini` above a file supplies its per-file settings,
regardless of the command's working directory. Lookup uses the file's absolute
lexical path and does not follow symlinks. Without a config, tools use defaults
and the file has no project module roots. Relative paths resolve from the
config directory. Invalid configs fail the command.

Directory discovery applies `exclude` rules from configs between a file and the
walk root; `[format] exclude` adds a rule for `xsht fmt`. An outer config's
exclusion remains in a nested project, but discovery started there does not
apply configs above its root. A path named on the command line bypasses
discovery exclusions. The ignore precedence is `.fdignore`, `.ignore`,
`.gitignore`, shared Git `info/exclude`, then global Git rules. Linked
worktrees use the shared `info/exclude`. Discovery prunes ignored directories,
fails on unexcluded unreadable directories, and does not follow directory
symlinks.

`include`, `test_roots`, and `[coverage] exclude` come from the current
directory's config. `include` adds roots to no-argument discovery; `module_path`
adds module roots after file-relative lookup and `XSH_MODULE_PATH`. `xsht test`
searches `tests/` unless `test_roots` changes that default. The formatter uses
`[format] line-width` (default 120) and `[format] exclude`. Coverage exclusions
remove files from its source denominator. `[check] annotate`, `[lint]`,
`[dead-code]`, and `[coverage]` hold the other per-project policies. The parser
in `crates/xsht/src/cli/files.rs` owns accepted keys and defaults.

## Checking and annotation

`xsht check` prepares each script through the same pre-evaluation check path as
execution, including dynamic-boundary checks and lowerability validation; it
does not run the script. There is no permissive mode for dynamic values.
`--annotate` writes safe inferred annotations in place only after checking and
lowerability produce no diagnostics. Its default classes are defaulted
parameters, inferred returns, and exported bindings; `locals` is opt-in. A
bare `--annotate` uses `[check] annotate` when configured.

`xsht check --compare-with ORIGINAL_ROOT --candidate-root CANDIDATE_ROOT
CANDIDATE_PATH...` qualifies callable interfaces and CLI parsing contracts without writing either tree.
Candidate entries must be existing `.xsh` files inside the explicit candidate
root; directories are not entry selectors. Relative paths are relative to the
working directory. Candidate sources map by their relative path to existing
original files. Both complete programs use the original configuration and module
roots, with candidate entry and import bytes loaded at the original canonical
paths. This preserves nominal schema identity across the pair.

Qualification requires equal nonempty declaration sets keyed by logical source
path and name, preserving export visibility. Candidate imports must resolve to
the same corresponding files, including through symlinks and configured module roots. Original generic contracts must remain equivalent; original
concrete contracts must be instances of candidate contracts under the checker's
canonical comparison. Missing, extra, mismatched, unresolved, or unsupported
contracts fail with status 2 and a declaration diagnostic. CLI comparison covers
command paths, parameter parser domains, defaults, rest parameters, and declared
effect bounds. Help text and runtime behavior require separate paired tests.
Internal host declarations
are excluded from both sides. The command checks and validates lowerability of
both programs; it does not establish runtime behavior equivalence.

Both roots bound the source view: reachable user imports must stay inside the
original root and have candidate files. Every candidate `.xsh` file must have
an original counterpart; module additions and deletions are unsupported. Source
discovery skips symlinks, `.git`, `.work`, `target`, and `node_modules`.
Reachable module paths must resolve inside their respective roots.
Candidate import validation fails when the current `XSH_MODULE_PATH` resolves
a module outside `CANDIDATE_ROOT`. Embedded standard-library modules load
normally. Successful comparison prints `xsht check interface qualification: N callable, M CLI`,
counting unique declaration keys. At least one count must be nonzero.
Comparison requires explicit entries and cannot combine with
`--annotate` or `--summary`.

The frontend facade exposes `CheckedCallableContract` through
`CheckedFunctionDeclaration::callable_contract()` and
`CheckOutput::callable_contract_at(span)`. These immutable views share the
checker-selected signature and retain its symbols after source disposal.
At an invocation span, `callable_contract_at` returns the callee's declaration
contract. `CheckOutput::callable_value_contract_at(span)` returns a contract
only for a checked function value reference. Annotation consumers use this
query for initializer and argument values.
`is_generic()` and `kind()` describe the checked declaration; `Display` uses
stable bound parameter names, and `equivalent_to` compares structure modulo
bound type and row parameter names. `join_effects` combines finite effect
requirements only when the remaining checked shape agrees; an unrestricted
clause remains unrestricted. Per-call instantiation does not replace
the declaration contract. Failed or unresolved declarations have no view.
`CheckedCallableContract::kind()` returns the public `CallableKind` (`Pure`,
`Proc`, or `Stream`), excluding CLI entry declarations. `ModuleExportType`
uses `GenericCallable { kind, optional }` for quantified exports. Its
`field_type()` returns `Option<Type>`; an unsupported generic stream value
projection has no ground callable representation.

`source_annotation()` returns source text only for expressible callable types;
callers recheck it in the insertion scope. Displayed `forall` notation is
diagnostic text, not an annotation to insert.

## Lint

`xsht lint` analyzes checked facts. It skips files with check errors and
includes check warnings. Lint warnings fail with status 1; notes are advice,
exit 0 on their own, and are counted at the end of a report that contains them.
`--deny-notes` makes notes fail with status 1.

`--only` selects lint rules or other fixable diagnostic codes. Naming a rule
enables it even when its `[lint]` switch is off, but `[lint.RULE] exclude` still
applies; inferred-return rules remain off when `[check] annotate` writes
returns. A scoped fix edits only the selected spans and does not format other
bytes. Unscoped fixes format rewritten files. Fix candidates are reparsed and
rechecked; a fix is declined if it adds checker diagnostics or drops comments.
`--runless` reports external-command use unless `[lint] runless-except` permits
that command. `xsht lint --list` prints the catalog used by the generated
[lint reference](reference/lints.md).

`lint.prefer-atomically-replace` reports a temporary-file publication only
when its final rename explicitly uses `overwrite: true`; a rename that refuses
an existing destination does not have equivalent replacement behavior.

## Native tests

`xsht test` discovers native `test NAME { ... }` declarations and rejects
trace-only flags. See
[test ownership and commands](TESTING.md#native-tests-first) for repository
test selection and profiles. Use `xsht help test` for test-runner options.

## API queries

`xsht api` queries the language and standard-library registry. With no
selector, it prints a getting-started guide. Use `xsht help api` for query
syntax and output options.

`xsht ir-tags` and `xsh::ir::full_instruction_tag_names()` expose the
full-instruction tag names in schema order.

## Desugar

`xsht desugar SCRIPT` prints formatted source with sugar statements replaced
by their core expansion. It does not check or modify the input. It fails if the
input or expanded output does not parse back to the same program.

## Generated references

Generated reference sources and check commands are listed in
[documentation testing](TESTING.md#generated-docs).

## Source representations

`xsht grep` matches semantic AST expressions, respecting expression
boundaries and ignoring whitespace and comments. Uppercase identifiers are
expression metavariables; `ARGS..` matches zero or more arguments. A pattern
beginning with `for` is a loop head: it matches loops with the same number of
bindings, and a metavariable in a binding position matches any loop binding.
`xsht refactor` replaces each matched expression span and reuses captured
source text. Its `--dry-run` prints proposed replacements without writing
files. It does not format; run `xsht fmt` afterwards. See
[Architecture](ARCHITECTURE.md) for AST and CST invariants.
