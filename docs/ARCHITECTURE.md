# Architecture

XSH is a compiler-style pipeline around one verified executable form. Source is
parsed once into a compact arena and a lossless CST, checked once, and the
checker publishes facts that every later consumer reads: lowering, the
formatter, lint, and the other `xsht` tools. Lowering commits a verified indexed
program, and a heap-frame executor runs it. There is no second frontend, no
alternate executable representation, and no fallback interpreter.

`docs/SPEC.md` is the language contract. This document is the contributor map:
where each stage lives, the invariants that hold the stages together, and the
checklist for changing them. Search for the exact symbol names below; each is a
retrieval handle for its owner file. Testing lives in `docs/TESTING.md`,
tooling in `docs/XSHT.md`, formatter policy in `docs/XSHT-FMT.md`.

## Packages

| Package | Owns |
|---|---|
| `xsh` (root) | the `libxsh` library, the `xsh` binary, and the `xsh-test-helper` test child process (`tests/helpers/`, one subcommand per mode) |
| `crates/xsht` | the `xsht` tooling binary: check, fmt, lint, test, api, trace, grep, refactor, grammar |
| `crates/xshi` | the `xshi` interactive shell |
| `crates/xsh-registry` | standard-module signatures, records, API docs, examples, language reference items, and runtime operation IDs |
| `crates/xsh-net` | DNS, the resolved TCP dialer, TLS, redirects, body limits, and network error classification |
| `crates/xsh-root` | kernel-enforced rooted file opening (Linux and macOS) |
| `crates/xsh-fuzz` | the soundness fuzzer: a seeded generator of well-typed programs, an independent reference evaluator, registry-derived call probes, corpus mutation, a sandboxed runner, and failure shrinking (`make fuzz`) |
| `crates/xsh-applets` | native applet support (`mdev`) |

A subsystem gets its own crate only when it has a stable Rust boundary that does
not depend on XSH source spans, runtime values, diagnostics, or evaluator state.
`xsh-net` is the model: `src/modules/net.rs` and `src/runtime/eval/modules/net.rs`
translate XSH records into plain Rust requests and translate results back into
`Value`/`RuntimeError`, keeping spans, test mocks, and evaluator state on the
XSH side.

### The `libxsh` façade

First-party consumers import the root library only through these modules:

| Concern | Path | Owner |
|---|---|---|
| loading, syntax, checking | `xsh::frontend::{load, syntax, check, source}` | `src/frontend.rs` over `src/loader.rs`, `src/syntax`, `src/sema`, `src/source.rs` |
| project module roots | `xsh::frontend::load::project_module_roots` and the config pieces beside it | `src/project.rs` |
| diagnostics | `xsh::diagnostic` | `src/diagnostic.rs` |
| script execution | `xsh::execution::script` | `src/execution.rs` over `src/runner.rs` |
| evaluator and values | `xsh::execution::{evaluator, value}` | `src/runtime/eval.rs`, `src/runtime/value.rs` |
| process lifecycle | `xsh::process` | `src/process.rs` over `src/runtime/process.rs` |
| structured traces | `xsh::trace::model` | `src/trace.rs` |
| reusable host adapters | `xsh::host` | `src/lib.rs` |

Script execution, source/diagnostic data, and structured traces are the
supported tier. Frontend, evaluator, value, and process types are first-party
tooling APIs whose representation is still coupled to the compiler. The
implementation roots (`runtime`, `sema`, `syntax`, `modules`, `runner`) are
private. Trace data belongs to `libxsh`; trace presentation belongs to
`xsht`. The library stays a static Rust library: no `cdylib`, and no split into
a separate core crate without a concrete consumer. `tests/libxsh_api.rs` guards
the façade.

`RunOptions.args` carries `OsString` words so the execution façade does not
discard bytes at the OS boundary. Checking selects the entry argument domain:
ordinary entries use UTF-8 `List[Str]`, while a sole `main` rest parameter of
`List[Bytes]` selects byte arguments. The same context reaches imported module
checking and runtime bindings; an import cannot be checked with text arguments
and then receive byte arguments. Launcher operands remain UTF-8.

Command and shared domain behavior belongs in XSH libraries under `core/lib/`.
This includes awk and sed parsers and interpreters, FAT geometry and repair
decisions, and human date parsing. Rust provides reusable syscall, descriptor,
codec, numerical and byte operations. A native performance exception requires
an XSH implementation and measurements that identify the smallest operation
needing acceleration.

Module resolution has one owner. `loader::resolve_module_path_candidates`
fixes the search order (beside the importing file, `XSH_MODULE_PATH`, project
module roots), and `src/project.rs` finds a project's `xsht-config.ini` and
reads its `module_path`. The runner, `xshi`, and `xsht` all get project roots
there; `xsht` reads the rest of the file itself, and `xsh` never learns the
tool configuration.

## Pipeline

| Stage | Primary objects | Owner |
|---|---|---|
| grammar | `grammar()`, `BINARY_OPERATORS`, `STATEMENT_KEYWORDS`, `QUOTED_LITERALS`, `STREAM_STAGES`, `RUN_FORMS`, `line_continuation`, `grammar_tokens` | `src/syntax/grammar.rs` (productions in `src/syntax/grammar/productions.rs`) |
| lex | `Lexer::lex_compact`, `TokenTable` | `src/syntax/lexer.rs`, `src/syntax/token.rs` |
| source structure | `SyntaxTree::from_token_table` (CST), `Parser::parse_source_arena_only`, `ArenaProgram`, `AstArena` | `src/syntax/cst.rs`, `src/syntax/parser.rs`, `src/syntax/arena.rs` |
| load | `CompactFileUnit`, `CompactModuleGraph`, `CheckedEntry` | `src/loader.rs` |
| check | `Checker`, `CheckOptions`, `CheckOutput`, `Checker::compact_declarations`, `CompactDeclOutput` | `src/sema/check.rs`, `src/sema/check/compact.rs` |
| publish facts | `CompactBodyFacts`, `CheckedApiCall`, `CheckedArguments`, `PreparedConstants` | `src/sema/check/compact.rs`, `src/sema/constants.rs` |
| lower | `FullBuilder::build_compact`, `BuildScratch` | `src/runtime/eval/lower.rs` |
| verify | `FullVerifier::verify`, `FullStore`, `FullProgram` | `src/runtime/eval/indexed/full.rs` |
| execute | `Evaluator::prepare_compact_indexed_only`, `indexed_run`, `CallFrame`, `FrameWork` | `src/runtime/eval.rs`, `src/runtime/eval/lowered_run/indexed_run.rs`, `src/runtime/eval/lowered_run/indexed_run/explicit_run.rs` |

**Grammar.** `src/syntax/grammar.rs` is the one definition of the syntax:
productions over the lexer's tokens plus the tables the lexer and parser
dispatch on (operators with precedence, associativity, and line continuation;
statement and primary keywords; quoted-literal prefixes; stream stages; run
forms and options; builder APIs). The parser keeps its own recursive descent,
recovery, and diagnostics but reads every such table from the grammar.
`make docs` renders the productions as `docs/reference/grammar.md` through
`xsht grammar --format json`. `grammar::earley` recognizes token streams
against the productions and `grammar::generate` produces sentences from them;
the grammar tests use both to hold the productions and the parser to the same
language (`docs/TESTING.md`).

**Syntax.** The lexer produces columnar token tags and starts; source text stays
the authority for token ends and spelling. The parser writes typed rows
(`StmtId`, `ExprId`, `PatternId`, `BlockId`) directly into `ArenaProgramBuilder`;
variable payloads live in side tables and ranges. The CST serves formatting and
source-preserving edits and is never executed. Do not add a recursive AST or a
CST-to-AST bridge for convenience. The parser decides language shape; later
stages do not recover from ambiguous trees the parser could have represented.

A surface form that is sugar (`docs/DESIGN.md`) is one statement row,
`ArenaStmtKind::Sugar { form, operands, expansion }`, that carries two views of
the same nodes. `operands` are the parts the user wrote, as `ArenaSugarOperand`
ids into the ordinary tables, in the order they run (source order, except
that a postfix `when`/`unless` condition comes before the statement it
guards). `expansion` is a core statement
that the form's function in `src/syntax/parser/sugar.rs` builds, once, from
those same ids plus the nodes it adds; that function is the only definition of
the form's meaning, and no later pass rewrites anything.
`ArenaProgramBuilder::push_sugar` records the rows an expansion adds, so
`AstArena::expr_is_synthetic` (and the statement and block equivalents) lets a
whole-table scan leave them out. Semantic code reads the expansion: a match
that recurses has one arm that delegates to it, and code that classifies a
statement by kind resolves it with `AstArena::core_stmt_id` first. Source
tools read the operands: a walker that only recurses has one arm over
`AstArena::sugar_operands`, and only a consumer that prints or matches a
particular form switches on `AstArena::sugar`. Lint has both kinds of walker:
a rule that looks for a spelling walks operands, and an analysis of meaning
(`stmt_flow`) follows the expansion. The mechanism is statement-level; an
expression-level form would use the same shape on `ArenaExprKind` when the
first one is scheduled.

**Checking.** `Checker` owns lexical scopes, signatures, imports, return and
purity context, and stream item context. Focused rules live beside it:
`src/sema/constraints.rs::TypeConstraints` (bounded monomorphic inference),
`src/sema/check/infer_return.rs` (private return inference),
`src/sema/check/infer_effects.rs` (effect inference over the whole module bundle),
`src/sema/check/effect_bounds.rs` (`without` regions: a block's bound lives in
`AstArena::block_effect_bound`, the block itself stays an ordinary lexical
block for every other stage), `src/sema/check/proof.rs` (narrowing provenance),
`src/sema/check/stream.rs` (pipeline stage facts), and `src/sema/arguments.rs`
(static argument binding). Registry signatures from `crates/xsh-registry` are
adapted to semantic types in `src/modules/signature.rs`. The checker reports a
diagnostic and continues with an internal recovery type; public dynamic data is
`Type::Any`, and recovery types never leak into signatures or docs.

**Facts.** The checker publishes everything later stages need:
expression and binding types, the selected overload and argument slots for every
registered call (`CheckedApiCall`), argument bindings for user calls and stages
(`CheckedArguments`), statement positions, function return and effect facts,
and prepared constants. `CompactBodyFacts` re-keys them by
arena identity. Lowering, lint, and annotation all consume these facts instead
of re-deriving them. A program is checked once: the runner, `xsht check`, and
`xsht test` check with `CheckOptions::embedded_bodies`, render that check's
diagnostics, and pass its output to `Checker::compact_declarations`.
`Checker::check_compact_declarations` runs the same check for programs prepared
without an entry check (embedded modules, loaded modules, tests).

**Lowering and verification.** `FullBuilder::build_compact` reserves function
identities, lowers each checked body into short-lived construction scratch,
encodes it into indexed columns, and builds the root driver only when the whole
program is representable. Checkpoints rewind every column on failure, so
unsupported behavior becomes a diagnostic and never a runnable placeholder.
`FullVerifier::verify` checks tag/data schemas, ranges, ownership, termination,
slot bounds, IDs, locations, patterns, stages, and literal and semantic pools
before a `FullProgram` exists. Runtime decoders rely on that contract.

Specialized `Path` calls use the receiver in `CheckedApiCall`, including
null-safe calls. A shared method name alone cannot select a filesystem
instruction: imported procedures may export those names with their own
parameters and defaults. `tests/xsh/lowering-coverage.xsh` covers those calls.

`SlotScope` assigns separate slots to pipeline callback parameters even when
they shadow enclosing bindings. Lowering restores the enclosing slot and
checked type after the callback; fold initializers use the enclosing scope.

**Execution.** `indexed_run` executes function blocks and driver ranges from
borrowed `FullProgram` views. Calls, work, and continuations live in heap-backed
frames (`CallFrame`, `FrameWork`), so XSH call depth never becomes native
stack depth; statement blocks nested in expressions run on lent block frames
over the caller's slots. After installation, parser, checker, and builder state
are dropped: only `FullProgram`, its `SourceMap`, and its `SymbolOwner` survive
execution. The runner, native tests, direct calls, module loading, auto-main,
and signal-hook setup all prepare through
`Evaluator::prepare_compact_indexed_only`.

### Program representation

`src/runtime/eval/indexed.rs` defines one-based `u32` identities (`IrFunctionId`,
`IrBlockId`, `IrStringId`, `TypeId`, `SignatureId`, `ShapeId`) with `IR_NONE` as
absence. Instructions are a one-byte tag plus eight-byte `IrData`; variable
payloads are ranges into shared tables. Hot rows hold no machine-width indexes,
recursive children, strings, or `Type` values. A finalized `FullProgram` holds no
CST, arena, or checker references. `src/runtime/eval/indexed/semantic.rs`
assigns program-owned type, signature, and shape identities and drops its
canonicalization maps at finalization.

Dynamic name spellings are owned by `SymbolOwner` in `src/symbol.rs`;
`Name::as_str()` returns `NameText`. Never claim a process-lifetime `&'static str`
for dynamic input. Dropping the last owner releases the spellings.

### Embedded standard library

A registry entry carries an `ImplBinding`: `Native` (a `RuntimeOp` body, the
default) or `Script` (a function in an embedded XSH module under `stdlib/`).
`src/stdlib.rs` embeds a fixed catalog with `include_str!`; nothing is read from
disk at run time. `stdlib::required_modules` selects modules syntactically from
the spellings a program mentions; selected modules are parsed into the same
arena under the reserved `<xsh-stdlib:IDENTITY>` namespace, checked with the
program, and called through ordinary `Call` instructions.
`every_catalog_module_parses_checks_and_lowers` validates every catalog module.
Whether an entry is native or script-backed is decided by measurement;
`bench/stdlib-port/README.md` records each disposition.

### Generated documentation

`dev/docs.xsh` renders each `docs/templates/REL` into `docs/REL` with the
`template` module (`make docs`, `cargo dev docs`), so the SPEC, the tour, and
the references come from code instead of copies of it. SPEC and tour code
blocks are the files in `docs/snippets/spec/` and `docs/snippets/tour/`
(`NN-name.xsh`; the tour also has the `project/` example with its own
`xsht-config.ini`). A snippet shows either the whole file or its
`# begin example` ... `# end example` regions, dedented, so a fragment is
checked inside a wrapper program the document leaves out. Snippets under
`rejected/` show code that must not check and name each expected diagnostic
with a `# error: CODE` comment on its line; other files in a snippet directory
are support modules the snippets import. A snippet runs, sandboxed in an empty
directory with only `PATH`, exactly when a template shows its `.output`, and a
`# platform: linux` snippet never runs, so generation is host-independent.
`docs/reference/stdlib.md`, `cli.md`, and `lints.md` are read from
`xsht api --format jsonl`, the binaries' help, and `xsht lint --list`. The
SPEC facet table is read from the `language:facet` API items, which come from
`xsh_registry::errors::ErrorFacet`, the one facet vocabulary the checker,
runtime, and built-in error families share.
`dev/docs.xsh::check` (the `check-docs` stage of `cargo dev check` and
`make check`, and `make docs-check`) re-renders and fails on any difference,
then runs `xsht check` on each snippet, requiring no diagnostics or exactly the
annotated ones, and `xsht test` in the project.

## Invariants

1. **Verify before execute.** Only a `FullProgram` that passed
   `FullVerifier::verify` is installed. A clean construction gap is a diagnostic;
   it cannot select another evaluator.
2. **One decision pipeline.** Each semantic decision (overload selection,
   argument binding, narrowing, inference, effects, constants) is made once, by
   the checker. Two routes that must agree (full and compact checking, the
   recursive and frame evaluators) share one implementation or are pinned by a
   parity test. Which member of a `Union[...]` a value belongs to is one such
   decision: the checker, the runtime type test, and schema decoding all ask
   `sema::types::first_accepting_union_member`, and `union_member_error` is
   the one definition of a well-formed union for both type resolvers.
3. **Lowering consumes checker facts.** Lowering never checks a body again,
   selects an overload, or binds arguments. A call without a checked plan lowers
   only its positional arguments.
4. **Behavior-bearing data survives every stage.** An indexed program that drops
   a format spec, stream error, trace event, method argument, or run option is
   wrong even if it lowers.
5. **Effects stay explicit.** Processes, cwd/env, defers, signal hooks, streams,
   and host operations remain explicit instructions or driver steps. Fast paths
   may remove dispatch after verification but must preserve tracebacks, traces,
   and exact error spans.
6. **Formatter equivalence is a safety net.** `verify_formatted_output` compares
   canonical syntax walks (`crates/xsht/src/format_equivalence.rs`) of the input
   and output and refuses to write a regrouped program, for both `fmt` and
   `lint --fix`.
7. **Lint is invariant under formatting.** A file and its `xsht fmt` output
   produce the same diagnostics. Layout may decide only whether a fix is offered.
   `crates/xsht/tests/lint_format_invariance.rs` enforces this.
8. **No speculative machinery.** No JIT, green threads, async task runtime, or
   bytecode VM. Reconsider only with measured bottlenecks and only if every
   observability and OS contract stays exact.
9. **Sugar has one meaning and two readers.** A sugar form reaches the checker,
   lowering, and execution only as its expansion; syntax tools read only its
   operands. The expansion references each operand exactly once and in the
   order listed, its root carries the surface statement's span and is never a
   declaration or binding, and every other node it adds has a span of its own
   inside the surface statement, because checker facts are keyed by span. The
   one static rule a form may add to its expansion is that a block must leave
   the enclosing continuation (`ArenaProgramBuilder::require_block_exit`, read
   by the checker as `AstArena::block_must_exit` on whichever `if` owns the
   block); `guard cond else` uses it, and no checker code names a form.
   `every_form_keeps_the_expansion_rules` checks this for every `SugarForm`,
   and `every_form_expands_to_its_stated_core_program` holds each expansion to
   a hand-written core program and to the text `xsht desugar` prints, which
   is what the SPEC shows (`crates/xsht/src/sugar_expansion_tests.rs`). A
   semantic walker that matches statement kinds with a wildcard arm can still
   skip a sugar node; `the_desugared_corpus_checks_and_tests_like_the_corpus`
   (`crates/xsht/tests/desugar.rs`) catches that by requiring the native test
   corpus to check and run the same once desugared.
10. **A set is not an instruction.** `Set[T]` is a value kind and a type-pool
    row, but a set literal, a set comprehension, and `set.empty()` lower to
    the list instruction they resemble under the `to_set` method, and a set
    read as a source lowers under `to_list`, so the executor has no set form
    of its own. Braces of bare names are a `Record` node in the arena; the
    checker's type for that expression (`Set[T]` where one was expected) is
    the fact lowering reads, as it reads `Map` for a brace literal that is a
    map. A set's elements are `MapKey`s, so its order, equality, and JSON
    array are the ones a map's keys have (`src/runtime/eval/set.rs`).

## Adding a language feature

1. Specify it in `docs/templates/SPEC.md`, with examples in
   `docs/snippets/spec/` (first, or in the same change).
2. Add its productions, and any keyword, operator, or stage table rows, to
   `src/syntax/grammar.rs`; add arena storage and accessors in
   `src/syntax/arena.rs`, and parse it in `src/syntax/parser/` from those
   tables. Keep CST round-tripping exact, and run `make docs`.
3. Check it in `src/sema/check/` and publish whatever later stages need as a
   checked fact. Keep full and compact checking in agreement.
4. Lower it in `src/runtime/eval/lower.rs` from those facts. Add instruction
   tags and verifier rules in `src/runtime/eval/indexed/full.rs`; execute in
   `src/runtime/eval/lowered_run/indexed_run.rs` or the focused runtime owner.
5. Format it in `crates/xsht/src/format.rs`, give its operands contexts in
   `src/syntax/grouping.rs` (`child_context`, `needs_parens`, shared by the
   printer and `check.redundant-parens`), and extend
   `crates/xsht/src/format_equivalence.rs` if the canonical walk needs it.
6. There is no generic AST visitor. Update every arena/CST consumer that owns
   behavior for the surface, typically `crates/xsht/src/lint.rs`,
   `crates/xsht/src/grep.rs`, and `xsht check --annotate`.
7. Add native tests (see `docs/TESTING.md`) plus a verifier unit test for new
   instruction shapes, and update `tools/xsh-ir-coverage.xsh` if coverage
   accounting changes.

## Adding a sugar form

A form qualifies when `docs/DESIGN.md` says it desugars trivially. It then
needs nothing in the checker, lowering, the verifier, or the executor. If the
core form does not already give the behavior the surface form needs (a
narrowing, a tail rule), improve the core rule so both spellings get it; do
not special-case the form. A form that binds a name in the enclosing block
cannot be sugar, because declaration scans do not look inside a surface form:
`guard let` is a core statement for that reason.

1. Specify it in `docs/templates/SPEC.md` by its expansion: one snippet in
   `docs/snippets/spec/` shown as written (`{{.spec.NAME.source}}`) and as
   `xsht desugar` prints it (`{{.spec.NAME.desugared}}`).
2. Add its productions and any keyword or contextual-word rows to
   `src/syntax/grammar.rs`, a `SugarForm` variant with its `ArenaSugar` view in
   `src/syntax/arena.rs`, and one function in `src/syntax/parser/sugar.rs` that
   parses the operands and builds the expansion inside
   `ArenaProgramBuilder::push_sugar`. Give each node the expansion adds a span
   on the keyword or operand a diagnostic about it should point at. Bind a
   value the expansion needs twice to one local whose name no identifier can
   spell, as the embedded standard library does for its namespaces.
3. Print it in `crates/xsht/src/format.rs` and paint its contextual words in
   `src/syntax/highlight.rs`. A form whose operands bind names also states
   their scope in the `Sugar` arm of `Linter::lint_stmt`.
4. Add its snippet and at least one hand-written core program to `cases` in
   `crates/xsht/src/sugar_expansion_tests.rs`, native tests under `tests/xsh/`,
   and the migration lint with its autofix in its own `lint_*.rs` file.
   `xsht desugar` needs nothing: it prints any form's expansion, and gives a
   local bound under an unspellable name a fresh legal one.
5. Read the diagnostics a user sees for a wrong operand and a wrong body. They
   carry the core form's wording; change a synthetic span, not the checker, if
   one lands in the wrong place.

## Adding a validated type

A validated type is a base type plus a property the checker tracks
(`docs/SPEC.md` 4.13): `Type::Validated` holds a `ValidatedType`, a
`Validation` and the base it narrows. There is one mechanism, in
`src/sema/validated.rs`; `NonEmpty[T]` over `List[T]`, `RelPath` over
`Path`, a `nominal type` over its record schema, and a bounded integer
(`Validation::Range`, whose payload is its bounds) over `Int` or `UInt` are
its instances. The rules
that make a validated type sound are written once, for every instance:

- **Assignability.** `Type::matches_expected` lets a validated type fit its
  base and nothing fit a validated type except the same validation. Inside a
  list, map, stream, optional, result, record, or union the same widening
  holds at any depth (`Type::matches_stored`), and only that: the position is
  otherwise invariant. It needs no conversion because of the next two rules
  and because a structure is a value.
- **Erasure.** An operation reads its operand through `Type::unvalidated`, so
  it sees the base and returns what the base returns. Only the places named
  below produce a validated type.
- **Representation.** Lowering stores a validated value as its base
  (`lowered_type_from_type`). The type pool row `TypeTag::Validated` holds the
  base type id and the validation's code; `SemanticPools::verify` rejects an
  unknown code and a base the validation does not accept. A validation with
  a payload has a row of its own: `TypeTag::Bounded` holds the base type id
  and its bounds, and the verifier requires an integer base and bounds that
  hold a value the base can hold.
- **The one runtime test.** `value_matches_static_type` and its lowered and
  test twins test the base and then ask `runtime/eval/validated.rs`. That is
  what `.require(T)`, `is T`, a type pattern, and a dynamic call boundary
  run. `PreparedSchema::Validated` decodes the base and then tests the
  validation, and the verifier ties that schema to its type
  (`PreparedSchema::matches_type`).

An instance is one `Validation` variant. Adding it makes each `match` on
`Validation` fail to compile until the instance says:

1. in `src/sema/validated.rs`: its code, the base form it accepts
   (`base_error`), how it prints, what a failing value is called, the note
   that names the conversion, the registry receiver that lists its
   operations (`method_receiver`), and whether it survives concatenation and
   element-wise mapping;
2. in `src/sema/check/validated.rs`: which literals pass
   (`validated_list_literal` is the list case, `validated_static_path_literal`
   and `validated_interpolated_path_literal` the path cases, and
   `validated_int_literal` the integer case; another scalar instance adds its
   own literal case beside them and calls it from that literal's arm of
   `check_expr_arena_inner`);
3. in `src/sema/constants.rs::constant_passes_validation`: which constant
   values pass;
4. in `src/runtime/eval/validated.rs`: which runtime values pass, for `Value`
   and for `LoweredValue`.

A property that is a function of the value alone is one function read by the
literal case, the constant case, and the runtime case, so the three cannot
disagree: `validated::is_rel_path` is the whole definition of `RelPath`.

`Validation::Nominal(name)` is the one instance that is not a property of
the value. Its payload is the declaration's qualified name
(`RecordConstructors::nominal_names`), and `RecordConstructors::instantiate`
is the only place that produces the type, for a type definition marked
`nominal`; the constructor and `.require` get it from there. Its runtime test
is the base test and nothing more, so the checker never lets a program ask
it of a value whose static type does not already carry the identity:
`check_type_pattern_applicability` rejects a tested type that holds a nominal
type (`Type::held_nominal`) unless the subject is a union listing it, and
`union_member_error` rejects a nominal member beside a type whose values have
its fields. A site that asks "is this a record?" of a value asks it of
`ty.unvalidated()`, as a scalar instance's sites do. The type pool stores it
as its own row, `TypeTag::Nominal` (the record type id and the name's
symbol), not as a validation code: `Validation::code` is `None` for it, and
`SemanticPools::verify` requires its base to be a record row.

A validation over a scalar base is where erasure is easiest to miss, because
checker and lowering code compares scalar types by equality. An operator
that hands one operand's type to the other as its expectation erases first,
or a literal operand would be judged against the validation: `port < 70000`
compares integers. A site that asks
"is this a `Path`?" of an operand asks it of `ty.unvalidated()`; a site that
asks it of a declared slot (a parameter the argument must fit) compares the
declared type as written. A validated parameter whose storage kind is a
scalar carries the test of its declared type (`compact_type_check`), which is
what a dynamic call runs; a parameter row stores the storage kind, not the
declared type, so the verifier cannot tell that such a test was dropped.

The operations an instance guarantees or survives are ordinary registry
methods on its own `MethodReceiver` (`crates/xsh-registry`): `NonEmpty` lists
`first`, `last`, `push`, and `extend`, and `RelPath` lists `parent` and
`normalize`. `check_method_dispatch_arena` looks a
method up there first and, when it is absent, dispatches on the base type, so
"every other operation returns the base type" needs no list. A guaranteed
operation keeps a defended runtime failure for the value the checker ruled
out.

The type's spelling is separate from the mechanism: a type constructor gets an
`ArenaTypeExprTag` and an arm in each type-expression resolver
(`Checker::type_from_arena`, `RecordConstructors::resolve_instance_annotation`
and `expectation_annotation`, `Type::from_arena`, and the three in
`src/runtime/eval/lower.rs`), a builtin name gets a row in
`BuiltinTypeName`, and a declared type resolves through its alias.

Change frame layouts, token/arena storage, or instruction encodings only with
retained-memory, RSS, latency, or stack-depth evidence from
`xsht frontend-stats` (`src/frontend_stats.rs`) or `xsht runtime-stats`
(`src/runtime_stats.rs`). The `xsht` binary wraps its allocator in
`mem_track::CountingAllocator`, which counts only after one of those commands
(`crates/xsht/src/stats.rs`) turns it on; `xsh` and `xshi` never count.

## Runtime design

The OS runtime coordinates tree-shaped evaluation with the host's graph of
processes, process groups, terminals, signals, and waits. `docs/SPEC.md` defines
what a script observes; the runtime preserves those observations when the host
interrupts, reorders, or outlives evaluation.

**Three layers, no smuggling.** Evaluation owns scopes, values, `$?`, `Result`
propagation, defers, hooks, handles, and trace parentage. The process substrate
(`src/runtime/process.rs`, `src/runtime/run.rs`) owns argv/env/cwd conversion,
redirections, process-group setup, terminal handoff, `waitpid` decoding,
timeouts, cancellation escalation, and detached reaping. The signal substrate
(`src/runtime/signal.rs`) owns handler installation, async-signal-safe recording,
and child disposition reset. The process substrate never decides XSH control
flow, handlers never inspect evaluator state, and the evaluator never calls
`waitpid` or `tcsetpgrp` directly.

**Ownership plus checkpoints.** Every process XSH starts is owned by a scope,
owned by an active wait, released to a background reaper on explicit detach, or
deliberately outside XSH's process group. Ownership answers who must reap or
cancel; checkpoints answer when a recorded signal may become XSH behavior. XSH
code never runs inside a signal handler, and shutdown is never deferred until
the script returns.

**The process group is the unit of cancellation.** A simple command gets a new
group; a byte pipeline shares one; a managed `spawn` handle owns one until
`wait`, `cancel`, lexical cleanup, or detach consumes it. Children that
double-fork, `setsid`, or create their own groups have left XSH's control.

**Signals are recorded globally and interpreted locally.** The handler records
the first primary signal and one escalation. At checkpoints the evaluator runs a
matching hook (once, with its own defers and its own process work), forwards to
active groups, cancels live handles, or skips cleanup after escalation.

**Status is data; failure is control.** The substrate returns structured
outcomes; the evaluator decides whether to set `$?`, wrap `Ok`, propagate `Err`,
or build a traceback. Cancellation policy returns `Forward`/`Escalate` decisions
rather than throwing from the wait loop.

**Lexical cleanup is ownership.** A live non-detached handle is canceled and
reaped when its owning scope exits unless it moved into a surviving value;
cleanup runs before user defers observe the completed scope. `NetJob`, stream
producers, and `par-map` workers follow the same rule. `cd`/`env` value scopes
select evaluator state without touching process-global cwd or environment and
restore it only after scope cleanup.

**Fork safety.** Once networking starts, XSH is multithreaded before it forks.
Post-fork child setup does only descriptor setup, signal reset, group/session
setup, and `exec`, with no allocation or locking. Internal descriptors are
close-on-exec.

**Networking.** Each evaluator owns at most one lazy `NetRuntimeOwner`: one
executor on a parked driver thread, bounded transport admission, and bounded
file workers. It receives only plain Rust data, never `Evaluator`, `Value`,
spans, or trace buffers, and cannot run XSH code. `NetJob` identity, ownership,
trace events, and signal decisions stay in `src/runtime/eval/net_job.rs`.

**Streams.** A `StreamValue` holds a materialized prefix plus an optional live
source or suspended script producer. Supported serial stages
(`src/runtime/eval/lowered_run/indexed_run/serial_pipeline.rs`) pull one source
item through every stage before the next, stop at bounded terminals, and
materialize before an unsupported stage. `yield @source` delegation and
cancellation are iterative
(`src/runtime/eval/lowered_run/indexed_run/producer.rs`), so delegation depth
never becomes native stack depth; child cleanup runs before parent cleanup.
Stages that need a complete input (`sort`, `collect`, `batch`) keep an explicit
materialization boundary, and `par-map` keeps its worker boundary.

**Errors and traces.** `RuntimeError` carries immutable typed causes
(`src/runtime/value/error_cause.rs`); traces snapshot them into a bounded flat
sequence (`src/trace/error_causes.rs`). `TraceEvent` and `TracePayload`
(`src/trace.rs`) are the runtime-graph projection: spans anchor nodes to source,
parent IDs preserve dynamic containment, and payloads carry structured argv,
cwd, env, handle IDs, signals, statuses, and errors rather than reconstructed
shell strings. Network events never carry bodies, headers, credentials, or URL
queries.

**Interactive.** `xshi` (`crates/xshi/src/interactive/`) uses the same process
substrate; its observable behavior matches the `ish` shell, held by differential
PTY tests. History (`crates/xshi/src/interactive/history.rs`) is its only
cross-process state.
Its session policy is part of the language reference.

A new host integration names its owner, checkpoint behavior, cleanup
responsibility, signal interaction, status/error shape, and trace evidence
before adding API surface.
