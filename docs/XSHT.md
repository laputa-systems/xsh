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
The `[lint]` section rejects unknown keys, including retired rule switches.

## API inventory

`xsht api summary --format jsonl` emits one JSON object with `schema_version: 1`.
`method_receiver_count` is the number of method receivers; `method_receivers`
is their inventory array. The summary also includes module, record and language
inventories with their counts. Field names are unique, so strict JSON decoders
can consume the complete response without discarding a count or an inventory.

## Checking and annotation

`xsht check` prepares each script through the same pre-evaluation check path as
execution, including dynamic-boundary checks and lowerability validation; it
does not run the script. There is no permissive mode for dynamic values.
`--annotate` writes safe inferred annotations in place only after checking and
lowerability produce no diagnostics. Its default classes are defaulted
parameters, inferred returns, and exported bindings; `locals` is opt-in. A
bare `--annotate` uses `[check] annotate` when configured.

`xsht check --compare-with ORIGINAL_ROOT --candidate-root CANDIDATE_ROOT
CANDIDATE_PATH...` qualifies callable interfaces, CLI parsing contracts, exported
type-name availability, and checked body facts
without writing either tree.
Candidate entries must be existing `.xsh` files inside the explicit candidate
root; directories are not entry selectors. Relative paths are relative to the
working directory. Candidate sources map by their relative path to existing
original files. Both complete programs use the original configuration and module
roots, with candidate entry and import bytes loaded at the original canonical
paths. This preserves nominal schema identity across the pair.

Within each selected entry context, qualification requires equal declaration
sets keyed by logical source path and name, preserving export visibility. A
physical declaration can have different nominal schema identities when its file
is checked as an entry and when imported; each context is compared only with its
corresponding entry in the other tree. Candidate imports must resolve to the same
corresponding files, including through symlinks and configured module roots.
Original generic contracts must remain equivalent; original concrete contracts
must be instances of candidate contracts under the checker's canonical
comparison. Missing, extra, mismatched, unresolved, or unsupported contracts
fail with status 2 and a declaration diagnostic. CLI comparison covers
command paths, parameter parser domains, defaults, rest parameters, and declared
effect bounds. Help text and runtime behavior require separate paired tests.
Exported type names must form the same set in each selected entry, keyed by
relative source path and name. This covers aliases, bounded types, record schemas,
module contracts, enums, and error families; missing, renamed, newly private, or
additional public names fail qualification. Private helper names and generic
binder spellings do not enter this availability set. Ordinary structural alias
semantics stay unchanged. This check establishes name availability, not independent
equivalence of every unused type definition. Definitions used in callable, schema,
and checked body contracts retain those existing canonical comparisons.
Internal host declarations
are excluded from both sides. The command checks each program once and prepares
it once without executing initialization. It compares each selected program's
complete checked body owner set, keyed by selected entry, canonical source path,
module-resolution key, and entry or declaration name. Checked operation and stage
selections, effect summaries, and emitted runtime validation boundaries must be
preserved according to the canonical body contract. A body loaded in several
entry contexts is compared separately with its corresponding candidate context.
Within one selected entry, checked bodies with the same key must have mutually
preserved contracts.
Missing facts, unsupported capture, and preparation errors fail qualification.
These checks do not establish runtime behavior equivalence.

Both roots bound the source view: reachable user imports must stay inside the
original root and have candidate files. Every candidate `.xsh` file must have
an original counterpart; module additions and deletions are unsupported. Source
discovery skips symlinks, `.git`, `.work`, `target`, and `node_modules`.
Reachable module paths must resolve inside their respective roots.
Candidate import validation fails when the current `XSH_MODULE_PATH` resolves
a module outside `CANDIDATE_ROOT`. Embedded standard-library modules load
normally. Successful comparison prints `xsht check interface qualification: N callable, M CLI`,
counting unique physical source-path/name keys on the original side, so
declarations loaded by multiple selected entries count once. At least one count
must be nonzero. Comparison requires explicit entries and cannot combine with
`--annotate` or `--summary`.

The frontend facade exposes `CheckedCallableContract` through
`CheckOutput::callable_declaration_contract(definition)` and
`CheckOutput::callable_contract_at(span)`. These immutable views share the
checker-selected signature, symbols and published callable relationships after
source disposal. `CheckedFunctionDeclaration::callable_contract()` exposes the
declaration alone. Use the `CheckOutput` accessors when a signature contains
nested generic callable relationships.
At an invocation span, `callable_contract_at` returns the callee's declaration
contract. `CheckOutput::callable_value_contract_at(span)` returns a contract
only for a checked function value reference. Annotation consumers use this
query for initializer and argument values.
`is_generic()` and `kind()` describe the checked declaration. `Display` uses
stable bound parameter names and expands each nested callable relationship once.
For example, `callable#0{...}` defines a relationship and a later `callable#0`
refers to it. These local labels are diagnostic output, not annotation syntax.
`equivalent_to` compares structure modulo bound type, row and effect parameter
names, including complete nested callable target families. Missing relationship
context cannot establish equivalence. `join_effects` combines finite effect
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
diagnostic text, not an annotation to insert. A `T0: MapKey` binder reports the
existing finite key-domain requirement on that type parameter.

## Lint

`xsht lint` analyzes checked facts. It skips files with check errors and
includes check warnings. Lint warnings fail with status 1; notes are advice,
exit 0 on their own, and are counted at the end of a report that contains them.
`--deny-notes` makes notes fail with status 1.
For multiple files, the highest status wins: 1 for lint warnings, 2 for check
or read errors, and 4 for write errors. File order does not change the status.

`--only` selects lint rules or other fixable diagnostic codes. Naming a rule
enables it even when its `[lint]` switch is off, but `[lint.RULE] exclude` still
applies; inferred-return rules remain off when `[check] annotate` writes
returns. A scoped fix edits only the selected spans and does not format other
bytes. Unscoped fixes format rewritten files. Fix candidates are reparsed and
rechecked; a fix is declined if it adds checker diagnostics or drops comments.
`--runless` reports external-command use unless `[lint] runless-except` permits
that command. `xsht lint --list` prints the catalog used by the generated
[lint reference](reference/lints.md).

`lint.prefer-inferred-proc-return` remains opt-in. A private return annotation
can make a helper's result easier to read even when inference preserves its
type. The rule proves equality of checked facts before offering a deletion.
It preserves exported signatures, entry signatures, recursive annotations and
annotations needed for contextual typing. Enable it when you want private
helpers to omit redundant return types.

`lint.prefer-atomically-replace` reports a temporary-file publication only
when its final rename explicitly uses `overwrite: true`; a rename that refuses
an existing destination does not have equivalent replacement behavior.

## Native tests

`xsht test` discovers native `test NAME { ... }` declarations and rejects
trace-only flags. See
[test ownership and commands](TESTING.md#native-tests-first) for repository
test selection and profiles. Use `xsht help test` for test-runner options.

`FILTER` selects matching IDs; `--exact FILTER` selects one exact ID. Both
execution, `--list`, and `--list-json` reject an absent exact ID. Repeat
`--exact-id ID` to freeze a set across modules; it cannot combine with
`FILTER` or `--exact`.
Use `--exact-ids-file PATH` to read the set from a JSON array of strings. The
array and each ID must be nonempty. The file must be regular and at most 16 MiB;
symlinks to regular files are accepted. This option cannot combine with `FILTER`,
`--exact`, or `--exact-id`. Every requested ID must exist and occur once,
including during listing. `--list-json` emits the matching IDs as a JSON array,
including IDs with newlines. It cannot combine with `--list` or `--events-jsonl`.
Validation completes before a test starts. A selected module's compiler
diagnostics remain compiler diagnostics; they are not reported as missing IDs.
Invalid unselected modules do not block an exact set. Selected declarations in
the same source module share one prepared program.

`--events-jsonl PATH` creates a new file and refuses existing files or symlinks.
It cannot combine with `--list`. Each newline-terminated schema-1 object has
`schema`, `run_id`, and `event`. The stream contains:

- `run_started` with the sorted `selected` ID array.
- `case_started` with `case_id` when a test is admitted.
- `case_finished` with `case_id`, `outcome`, `reason`, and integer `duration_ns`.
  Outcomes are `passed`, `failed`, `skipped`, `timed_out`, `cancelled`, or
  `not_executed`. A reason is null for success, or an object with `kind` and
  `detail`; a timeout also includes its `limit` as a duration string.
- One `run_finished` with `status`, `selected` and `completed` counts, counts
  for every outcome, `reason`, and integer `duration_ns`.

Each selected ID has exactly one terminal outcome. A never-admitted test has
`not_executed` without `case_started`. Consumers must reject unknown schemas,
malformed or duplicate records, invalid order, missing terminal outcomes, and
streams without the final summary; a partial file cannot establish success.
The runner checks its own lifecycle and reports event-write and cleanup errors.
Consumers must also require the actual process status to agree with the final
row: a final write or flush failure can leave readable bytes while the runner
exits unsuccessfully.
Test durations describe execution and scheduling, not performance measurements.
Nested `test.run_*` results retain their existing raw child `status`; native
cases themselves execute in-process and have no invented child exit status.

Required exact-set skips leave the scope incomplete with exit 2. Existing
filter and single-exact skip behavior is unchanged. Failures and timeouts exit
1, retaining any cleanup or runner failure in the final reason when it can be
written. Invalid selection, event output, or incomplete cleanup without an
observed failure exits 2. SIGINT and
SIGTERM stop admission, cancel active tests, and drain owned workers before
exit 130. Pending tests receive terminal outcomes. A second signal aborts
immediately and can leave a truncated stream. An in-process evaluator that
cannot stop within its existing cleanup grace is reported as still running;
that scope cannot pass and requires process shutdown. `--fail-fast` records
remaining selected tests as not executed.

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
