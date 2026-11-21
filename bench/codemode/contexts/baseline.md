# XSH reference material

## Getting started (`xsht api`)

```text
XSH API getting started

Write this as hello.xsh:

proc main(...argv: List[Str]) {
  let name = argv.get(0) ?? "world"
  print "hello" $name
}

Basic development loop:
  xsht check hello.xsh
  xsht fmt hello.xsh
  xsht lint hello.xsh
  xsh hello.xsh

Ask for language rules, a module or receiver overview, or one exact API item:
  xsht api language:core
  xsht api module:fs
  xsht api method:Str
  xsht api api:fs.files
  xsht api method:Path.read_text
  xsht api record:FsEntry
  xsht api enum:archive.Format
  xsht api search:rooted extraction

`method:Str` lists every method on the Str receiver by purpose; append a member name (method:Str.lower) to read one exact item. Exact API items include purpose, contract, effects, signatures, tags, and a small example when one is useful. Use `xsht api summary` for the complete index and `--format jsonl` for machine-readable output.
```

## Language rules (`xsht api language:core`)

```text
query: language:core
status: matches

api: language.core.absence-lookups
kind: language
purpose: Represents ordinary search and byte absence as null.
contract: Str.find returns an Int byte offset or null; successful zero remains a hit. Str.byte_at and Bytes.byte_at return a byte Int or null for negative/out-of-range indices. List.get and Map.get retain their single-argument Result forms, including Ok(null); missing entries remain typed errors. Use ?? for explicit fallback. Removed fallback arguments may have evaluated eagerly: migration tooling fixes only proven inert alternatives, and effectful alternatives require ordered receiver/index/fallback snapshots before lookup. Sentinel-comparison rewrites require a checked lookup origin.
effects: none
tags: language, absence-lookups
example:
  let offset = "é: data".find(":")
  
  if offset != null {
    print ${offset}
  }
  
  let first_byte = "é".byte_at(0) ?? 0
  const entries: Map[Int?] = {present: null}
  let present = entries.get("present") ?? 7
  let missing = entries.get("missing") ?? 7
  
  if present == null {
    print "present null"
  }
  
  if missing != null {
    print ${first_byte} ${missing}
  }

api: language.core.assert
kind: language
purpose: Asserts a Bool condition with failure-only context.
contract: `assert condition, message` requires concrete Bool and Str and always produces Unit. Evaluate the condition once; evaluate the required message once only on false. Context supplements bounded reached-operand diagnostics without evaluating skipped operands. Messages retain ordinary static effects and propagation, and their failure takes precedence. Successful assertions skip messages. Failures retain the bare Bool assertion's Error family and assertion-failed kind, ordinary capture and cleanup, and work without native-test support. Use a bare Bool statement for a message-free assertion; Result-valued test APIs retain their separate result boundary.
effects: none
tags: language, assert
example:
  const name = "demo"
  let actual = 2 + 2
  const expected = 4
  assert actual == expected, f"package {name}"

api: language.core.assertions
kind: language
purpose: Asserts a checked Bool in statement position.
contract: A Bool statement evaluates once: true produces Unit; false propagates AssertionError.Failed(message: Str). Unit and Result[Unit] tails also assert. Bool, Any, Result[Bool], inferred retry tails, predicates, conditions, explicit returns, and `let _ = predicate()` remain value consumers. Assertions obey Result error-family compatibility, declared error effects, retry handling, and defer cleanup, and remain enabled without native tests.
effects: actual == expected, needle in output, let _ = predicate()
signature: actual == expected; needle in output
tags: language, assertion, Bool, AssertionError

api: language.core.bare-blocks
kind: language
purpose: Delimits lexical bindings and cleanup without a new call or Result boundary.
contract: Bare braces consume Unit in statement position and preserve their final value in value position. Bool statements assert; Bool values may be false. Defers run before an outgoing value becomes visible, and return, break, continue, and propagation retain enclosing targets. Record and Map literals keep syntactic field shapes: `{value}` is a record; `{ (value) }` is a block value. The checked lint.lexical-block rule replaces safe statement-only literal true conditionals while retaining the block scope.
effects: none
tags: language, bare-blocks
example:
  var count = 0
  {
    let increment = 1
    count += increment
  }
  let selected = {
    let answer = count + 41
    answer
  }
  let grouped = {
    (selected)
  }
  let shorthand = {selected}
  print $grouped ${shorthand.selected}

api: language.core.bindings
kind: language
purpose: Defines typed bindings and assignment scope.
contract: Bindings are immutable with `let`; declare a reassignable binding with `var` (`var x = 0; x = x + 1`). Record targets support shorthand, renaming, nesting, and `_` discards: `let {root, build: {jobs, target: target_name, ..}, ..} = config`. All required fields are selected before names become visible. Selected types are retained; Any requires an explicit schema check. `let mut` is not valid syntax. Reassignment cannot create an invalid inferred state.
effects: none
tags: language, bindings
example:
  const config = {root: "src", build: {jobs: 3, target: "native"}}
  let {root, build: {jobs, target: target_name, ..}, ..} = config
  print $root $jobs $target_name

api: language.core.block-parameters
kind: language
purpose: Defines one lexical block-header convention.
contract: Parameterized blocks use { |parameters| ... }, after optional whitespace and comments. Guard-let error handlers accept zero or one immutable error parameter; _ discards it, and the parameter retains the exact Result error type. Plain conditional, boolean-guard and defer blocks receive no input. Headers retain lexical return, loop and cleanup targets without a callable frame. Outside-brace handler headers are migration errors; lint.block-header moves safe headers and rechecks source.
effects: none
tags: language, block-parameters

api: language.core.block-strings
kind: language
purpose: Prepares indentation-aware ordinary, raw, and formatted multiline Str blocks.
contract: An opening triple delimiter followed immediately by LF, CRLF, or CR and a closing delimiter alone after spaces/tabs selects block layout. The closing delimiter's exact prefix is the margin; missing prefixes on nonblank source lines are errors. Structural opening and closing breaks are removed once, with no implicit trailing newline. Blank lines lose only the longest matching initial margin prefix. Internal line endings remain exact. Layout runs before escapes and interpolation, retaining original source spans and leaving interpolation code and inserted values untouched. Bytes, Path, glob, regex, and non-block triple literals retain exact layout.
effects: none
tags: language, block-strings
example:
  const name = "demo"
  let unit = f"""
    [service]
    name={name}
    """
  const expected = """[service]
  name=demo"""
  print ${unit == expected}

api: language.core.boolean-guards
kind: language
purpose: Continues only when an explicit Boolean or Status condition succeeds.
contract: guard condition else { ... } uses ordinary Bool/Status conditions, evaluates once, and requires every reachable failure path to leave the enclosing continuation. Return, applicable break/continue, and checked terminating operations qualify; an arbitrary fallible call does not. The failure block has no input parameter or new Result boundary. Success and failure retain checked refinements, mutation invalidation, lexical targets, effects, error identity, and defer cleanup. lint.boolean-guard preserves the complete exiting failure body; Float ordering retains negation for NaN safety. guard let remains the separate Result-binding form.
effects: none
tags: language, boolean-guards

api: language.core.captures
kind: language
purpose: Defines lexical captures for blocks and procedures.
contract: Captured bindings retain lexical ownership and cannot be used to smuggle ambient mutable state across a declared boundary.
effects: none
tags: language, captures

api: language.core.causes
kind: language
purpose: Retains typed causes when deliberately translating errors.
contract: `Err(error, cause: failure)` attaches an Error cause to an outer Error without changing its nominal family, facets, payload, or Result error type. Operands evaluate once in written order, including named spreads; construction remains Result data and propagation stays explicit. Shared immutable metadata leaves other aliases unchanged and retains the cause chain, spans, contexts, and process status. Matching inspects only the outer error; a payload field named cause remains ordinary data. Use ctx to contextualize the same failure and cause to record a translation. Human and structured traces render bounded chains and mark truncation; no script introspection API is introduced.
effects: none
tags: language, causes
example:
  error SourceCauseError = Missing(message: Str) : NotFound
  
  error BuildCauseError = CompileFailed(package: Str)
  
  pure translate_build_error(failure: SourceCauseError) -> Result[Str, BuildCauseError] {
    Err(BuildCauseError.CompileFailed(package: "demo"), cause: failure)
  }
  
  let translated = translate_build_error(SourceCauseError.Missing(message: "source missing"))
  
  if let Err(BuildCauseError.CompileFailed {package: package}) = translated {
    print $package
  } else {
    print "unexpected success"
  }

api: language.core.cli-main
kind: language
purpose: Declares typed command-line entries and group fallbacks.
contract: cli main PATH(parameters) [effects] -> Return with {name, help, parameters: {parameter: {form, min}}} declares an entry. Metadata is optional and must be compile-time literal data. Types and defaults come only from the signature; explicit forms supply operands, required named options, aliases and singular repeated names. Explicit forms show their declared spelling without inferred type/default descriptions. Unknown fields, invalid forms, alias collisions and invalid constant defaults fail checking. Parent entries take no parameters or one rest List[Str]; the longest matching path selects the entry and a nearest parent rest fallback receives unmatched raw words. Exact groups run their bodies. Custom group help accepts -h, --help or help; custom leaf help applies only to a sole help argument. Other leaf help uses strict generated usage. Parse failures exit 2; body failures retain ordinary error/status behavior. A command tree consumes one leading --; a flat entry retains ordinary end-of-options parsing. Help and argument parsing failures do not evaluate computed defaults. Minimum constraints also check all resolved scalar and list default values before the entry body; a violation prints usage and exits 2 after any computed-default effects.
effects: none
tags: language, cli-main

api: language.core.command-interpolation
kind: language
purpose: Defines explicit command and argv interpolation.
contract: Interpolated values remain typed argv boundaries; XSH does not perform implicit shell evaluation or word splitting. Compound process words retain interpolated Path bytes, including native non-UTF-8 names, without changing argument boundaries.
effects: none
tags: language, command-interpolation
example:
  const name = "world"
  run echo ${name}

api: language.core.comments
kind: language
purpose: Defines XSH comments and documentation comments.
contract: Documentation comments participate in exported-module contracts while ordinary comments remain source trivia.
effects: none
tags: language, comments

api: language.core.compatibility-vocabulary
kind: language
purpose: Exposes one canonical spelling for exact compatibility aliases.
contract: args, run forms, fs.children and Str.byte_len replace the removed predeclared ARGV, run.builtin qualifier, fs.ls and Str.count_bytes. Removed names reject preparation; lint.compatibility-vocabulary offers resolved CST edits and rechecks the complete import graph. User bindings, fields, strings, external argv, comments, wire keys and distinct filesystem/text operations retain their contracts. Historical operation-name attribution becomes canonical; error kinds remain unchanged.
effects: none
tags: language, compatibility-vocabulary
example:
  # Script arguments use the single predeclared binding; byte lengths are explicit.
  let argument_count = args.len()
  let byte_count = "é🍃".byte_len()
  let character_count = "é🍃".count_chars()
  print ${argument_count} ${byte_count} ${character_count}

api: language.core.comprehensions
kind: language
purpose: Builds lists and maps through ordered nested iteration and filtering.
contract: List and map comprehensions share interleaved for/if clauses. Direct Map[K, V] iteration supplies {key: K, value: V} entries in key order from retained snapshot storage; source Result[Map[K, V], E] failures propagate with E and the ordinary error effect. Str iteration supplies one-scalar Str values in Unicode scalar order; Bytes iteration supplies Int byte values from 0 through 255. Both retain the immutable source and have no line/word splitting or implicit decoding. Later clauses see earlier bindings; each inner iterable is evaluated anew for each reached outer item. False filters skip subsequent clauses and the projection. Streams are pulled lazily and closed on exhaustion, propagation, or early return. Maps evaluate each key before its value and later duplicate keys win.
effects: none
tags: language, comprehensions
example:
  type Package = {name: Str, sources: List[Path]}
  
  const packages: List[Package] = [
    {
      name: "core",
      sources: [
        p"main.xsh",
        p"README.md",
      ],
    },
    {
      name: "tools",
      sources: [
        p"check.xsh",
      ],
    },
  ]
  let sources = [
    source
    for package in packages
    for source in package.sources
    if source.ext() == "xsh"
  ]
  let by_package = {
    package.name: source
    for package in packages
    for source in package.sources
    if source.ext() == "xsh"
  }
  print sources.len()
  print by_package.len()
  
  const empty_counts: Map[Int] = {}
  let counts = empty_counts.set("beta", 2).set("alpha", 1)
  
  for {key, value: count} in counts {
    print f"{key}={count}"
  }
  
  let doubled = {key: value * 2 for {key, value} in counts}
  print ${doubled.get("alpha")?}

api: language.core.constants
kind: language
purpose: Defines prepared immutable data with a bounded lexical dependency graph.
contract: const values are checked and prepared without executing user code. Constants may use scalar, path, regex and homogeneous container data, checked primitive operations, other constants, and static record or tag constructors. Runtime bindings, parameters, ambient names, arbitrary calls and methods, blocks, comprehensions and propagation are rejected. Cycles, invalid arithmetic and preparation limits are diagnosed before execution. Empty collections require concrete type context; Any and resources are excluded. Indexed reads share immutable backing under ordinary value semantics, so mutating a derived var preserves the constant. let retains runtime initialization.
effects: none
tags: language, constants
example:
  const format_version = 1
  const retry_delays = [100ms, 500ms, 1s]
  const header = {version: format_version, delays: retry_delays}
  var configured = header
  configured.delays += [2s]
  print $format_version
  print header.delays.len()
  print configured.delays.len()

api: language.core.context-scopes
kind: language
purpose: Produces values while temporarily selecting cwd or environment.
contract: `cd (path) { tail }` and `env (overlay) { tail }` return Result[T, Error] after restoration. Result tails remain nested; body ? propagates to its enclosing boundary. Defers run inside the selected context, and lexical transfers and cancellation restore it. Environment overlays accept finite Records or string-keyed Maps with scalar argv-convertible values; null does not unset a name. Live producers and host handles cannot escape.
effects: none
tags: language, context-scopes
example:
  let compiler = env ({CC: "clang", BUILD_MODE: "release"}) {
    e"CC"?
  }?
  let directory = cd (p".") {
    fs.cwd()?
  }?
  let shown_directory = directory.display()
  print $compiler $shown_directory

api: language.core.defer
kind: language
purpose: Registers lexical cleanup actions without executing them.
contract: `defer { statements }` reads captures at cleanup time; use immutable let snapshots for earlier values. Actions run LIFO at their registering scope exit, including return, failure, loop control and cancellation. Bool statements assert and Result[Unit] failures stop that action; remaining actions still run. An existing failure stays primary and secondary cleanup failures are source-attributed. Cleanup cannot return, yield, or transfer to an outer loop; local loops and nested defers are allowed. Effects are checked normally. Expression and run forms remain supported; forced abort skips cleanup. `errdefer` registers the same way, in the same order, and its action runs only when the scope leaves with an error: a propagating failure, a function returning an Err, a non-forced abort or cancellation, or a failed later cleanup.
effects: none
tags: language, defer

api: language.core.display-strings
kind: language
purpose: Defines display-string interpolation.
contract: Display strings are presentation text: they interpolate with `{expr}` and do not become command argv or filesystem paths implicitly. `{{` and `}}` write literal braces, `{expr:>N}`, `{expr:<N}`, and `{expr:0N}` pad to a width, and `$` is ordinary text except that `${` and `$name` naming an in-scope binding are errors. Ordinary expression string literals never interpolate, so `$name` and `{name}` inside `"..."` are literal text; `lint.dollar-in-expression-string` and `lint.missing-f-prefix` warn when they name an in-scope binding, and raw strings keep `$` literal.
effects: none
tags: language, display-strings
example:
  const name = "world"
  let line = f"hello {name}"
  print $line

api: language.core.duration-arithmetic
kind: language
purpose: Combines Duration values with checked millisecond arithmetic.
contract: Duration +/- Duration and Duration * nonnegative Int produce Duration; multiplication works in either order. Duration / positive Int truncates sub-millisecond remainders; Duration / positive Duration produces an Int interval count. Ordering compares milliseconds. Overflow, underflow, negative multipliers, nonpositive divisors, and interval-count overflow are source-attributed runtime errors. Operands evaluate once left to right and arithmetic is pure. Float scaling and modulo are invalid. time.millis/time.seconds retain clamping and saturation rather than checked arithmetic.
effects: none
tags: language, duration-arithmetic
example:
  const budget = 1s + 500ms
  const quantized = 5ms / 2
  
  pure retry_delay(attempt: Int) -> Duration {
    250ms * attempt
  }
  
  let pause = retry_delay(3)
  assert pause <= budget
  let intervals = budget / 250ms
  assert intervals == 6
  assert quantized == 2ms

api: language.core.enums
kind: language
purpose: Declares nominal tagged unions with explicit enum bodies.
contract: An enum has at least one variant, including a single payload variant. Constructors retain their declaration module namespace, payload typing, value equality, and exhaustive constructor patterns; they are not qualified by the enum name. Where the expected type is the enum (an annotation, parameter, declared return, schema field, collection element, value branch, or the other `==`/`!=` operand), `.Variant` and `.Variant(args)` select its variant outside stream stage blocks, where `.name` reads the item; no or ambiguous expected type is check.inferred-variant. A type alias preserves nominal identity without adding constructors. Str-backed payload-free enums declare unique constant wire strings, including empty strings. Explicit require(Schema) converts nested enum slots atomically; type patterns and ordinary assignment never convert Str. JSON encoding and writing emit declared strings while raw decoding remains untyped. Exports, multiline bodies, and trailing commas are supported. Legacy type-union declarations produce parse.enum-migration and cannot execute; safe migration edits preserve variant order, payload annotations, and comments.
effects: none
tags: language, enums
example:
  enum Mode { Fast, Custom(Int) }
  
  type SelectedMode = Mode
  
  pure jobs(mode: SelectedMode) -> Int {
    match mode {
      Fast => 1
      Custom(count) => count
    }
  }
  
  enum Token { Present(Str) }
  
  pure token_text(token: Token) -> Str {
    match token {
      Present(text) => text
    }
  }
  
  print token_text(Present("ready"))
  print jobs(Custom(4))
  
  enum State: Str { Ready = "ready", Empty = "" }
  
  pure decode_state(text: Str) -> Result[State] {
    text.require()
  }
  
  pure encode_state(state: State) -> Result[Str] {
    json.encode(state)
  }

api: language.core.error-context
kind: language
purpose: Adds operation descriptions to outbound lexical failures.
contract: `ctx description { ... }` evaluates its Str description once on entry and preserves ordinary statement/value consumption. Outbound propagation, evaluator failures, and cleanup failures retain their identity, payload, and primary source span while contexts append inner to outer after defers. Stored or directly returned Err data stays unchanged, and handled failures do not cross the region. Return, loop control, suspension, and cancellation retain their lexical targets. Ordinary bindings, fields, modules, and attached calls named ctx remain legal.
effects: none
tags: language, error-context
example:
  let count = ctx "reading configured count" {
    "7" as Int
  }
  print $count

api: language.core.exit
kind: language
purpose: Terminates the script with an explicit exit status.
contract: `exit STATUS` is a deliberate process exit, not Result error propagation: it produces the requested status, an Int from 0 to 255, without a runtime traceback on stderr, and `try` does not capture it. Deferred cleanup runs while the script unwinds. The statement never completes, so a block that ends with it leaves its continuation, and it accepts a postfix `when` or `unless`. `exit` is a contextual word that begins the statement where a command would be read.
effects: none
signature: exit STATUS
tags: language, exit, exit-status, validation, statement
example:
  const invalid_input = true
  
  if invalid_input {
    print "validation failed"
    exit 1
  }

api: language.core.fail
kind: language
purpose: Returns an error that carries a message from the enclosing function.
contract: `fail MESSAGE` means `return Err(error.failure(MESSAGE))`: it leaves the function, not an enclosing `try`, runs deferred cleanup, ends its block like `return`, and accepts a postfix `when` or `unless`. The message is a Str and the error a plain Error, so the function returns a Result whose error type is Error. In a function that declares an error family, `fail .Variant(...)` means `return Err(.Variant(...))`; the variant comes only from the declared return type. `because CAUSE` after either form adds `cause: CAUSE` to the `Err`, keeping the replaced error as the cause. `fail` is a contextual word that begins the statement where a command would be read, and `because` is a word only after its first operand.
effects: none
signature: fail MESSAGE|.Variant(...) [because CAUSE]
tags: language, fail, error, result, cause, statement
example:
  pure percent(value: Int) -> Result[Int] {
    fail f"{value} is not a percentage" unless value >= 0 and value <= 100
    Ok(value)
  }
  
  match percent(140) {
    Ok(value) => print $value
    Err(problem) => print $problem.message
  }

api: language.core.fallback
kind: language
purpose: Defines fallback expressions for recoverable values.
contract: ?? lazily unwraps Result success or Optional presence and is right-associative. A Result may use { |failure| statements; tail_value } with exactly one immutable parameter containing the exact error. Handler tails match the success type and retain lexical return, loop, propagation, and cleanup targets. Optional fallback has no error parameter. lint.error-fallback-block preserves a complete unguarded Err handler when replacing a checked identity-Ok expression match.
effects: none
tags: language, fallback

api: language.core.field-labels
kind: language
purpose: Uses exact wire names as explicit labels without declaring variables.
contract: Keyword spellings such as type, in, and match are legal explicit labels in schemas, literals, member and update paths, patterns, renamed destructuring, and named record/error constructor arguments. Keywords remain reserved binding and import names; keyword labels cannot be shorthand or puns. Quoted literal keys retain arbitrary bytes, including a dot as one key. Known fields keep their checked types; dynamic values retain require validation.
effects: none
tags: language, field-labels
example:
  type Entry = {type: Str, in: Int}
  
  let entry = Entry(type: "file", in: 2)
  let {type: entry_kind, in: ordinal, ..} = entry
  let label = entry.type
  let dotted_key = {"wire.type": entry_kind}

api: language.core.glob-literals
kind: language
purpose: Defines filesystem glob literals.
contract: Glob expansion is an explicit filesystem operation with deterministic path values rather than shell word splitting.
effects: none
tags: language, glob-literals

api: language.core.guarded-control
kind: language
purpose: Defines condition-first guarded return, break, continue, and yield.
contract: `return value when condition`, `break value unless condition`, and `yield value when condition` evaluate the Bool/Status condition first and evaluate the payload only in the selected branch. Branch-local narrowing, existing lexical targets, effects, return wrapping, and deferred cleanup are preserved. Group command payloads, for example `return (run.status /usr/bin/true) when ready`; ungrouped run argv retain literal when/unless words. Valueless return/break/continue remain available, and guarded return can fall through.
effects: none
tags: language, guarded-control

api: language.core.list-concatenation
kind: language
purpose: Concatenates lists while preserving value semantics.
contract: `left + right` concatenates compatible List values in encounter order. `items += more` updates a mutable target; append one item with `items += [item]`. Empty lists use the expected element type, selectors and operands evaluate once, and earlier aliases retain their contents. The push and extend methods remain available for expression chains.
effects: none
tags: language, list-concatenation
example:
  var files: List[Str] = []
  files += ["main.xsh"]
  let earlier = files
  files += ["helpers.xsh", "tests.xsh"]
  let all_files = [@earlier, "README.md"]
  print files.join(",")
  print all_files.join(",")

api: language.core.list-element-assignment
kind: language
purpose: Updates existing List elements below mutable local bindings.
contract: `values[index] = replacement` and `rows[index].count += 1` retain contextual element types and collection value semantics. Paths can mix List elements, existing record fields, and Map keys. Int indices use ordinary indexing: negative and out-of-range indices fail without clipping, appending, or padding. Selectors run once in path order, then RHS once, then the current root and old selected value are read. Operand mutations survive; an invalid update exposes no partial ancestor rebuild. Immutable roots, temporary receivers, slice writes, and Str/Bytes mutation are rejected. Assignment produces Unit.
effects: none
tags: language, list-element-assignment
example:
  var rows = [{name: "first", count: 1}, {name: "second", count: 2}]
  let earlier = rows
  rows[1].count += 3
  rows[0] = {name: "updated", count: 7}
  print rows[1].count
  print earlier[1].count

api: language.core.list-splicing
kind: language
purpose: Builds lists from ordinary elements and explicit List splices.
contract: `[head, @middle, tail, @more]` inserts each spliced List in encounter order; an ordinary List-valued element remains nested. Elements and splice operands evaluate once from left to right, and propagation stops before later elements. Empty lists retain contextual element types and earlier aliases remain independent. Results require explicit handling such as `@(flags()?)`; Streams require explicit collect. Str, Bytes, Map, Any, comprehension clauses, and call argument unpacking are not splice operands.
effects: none
tags: language, list-splicing
example:
  const flags = ["-O2", "-g"]
  const sources = ["main.xsh", "helpers.xsh"]
  let argv = ["cc", @flags, @sources, "-o", "app"]
  let groups = [["head"], @[flags, sources]]
  print argv.join(" ")
  print groups.len()

api: language.core.local-inference
kind: language
purpose: Solves one fixed local collection or optional type from checked source constraints.
contract: Unannotated empty Lists, explicit empty Maps, and mutable local `set.empty()` initializers inside functions keep one monomorphic type hole. Writes and independently determined parameter or return expectations resolve the hole. Mutable locals initialized to null acquire one fixed Optional type. Immutable null remains Null, and an unannotated empty record remains a record. Every branch and loop body contributes static constraints, including zero-iteration loops. Aliases share one type identity. Incompatible contributions fail without widening to Any or a union. Material unresolved types need annotations before checked contracts or indexed execution can be published. Wholly discarded inert literals need no annotation. Runtime values never determine types.
effects: none
tags: language, local-inference
example:
  proc choose() -> Path? {
    var selected = null
    for destination in [p"release"] {
      selected = destination
    }
  
    selected
  }
  
  print ${choose()?.display() ?? ""}

api: language.core.map-literals
kind: language
purpose: Constructs typed Maps with explicit computed keys and Map spreads.
contract: `{[key]: value, fixed: other, ...more}` selects Map literal mode when any computed key exists or the expected type is Map[K, V]. Computed keys use one checked scalar domain: Str, Int, UInt, Bool, Bytes, Path, or Duration. Keys are never implicitly displayed or converted between domains; constant labels remain Str keys; constant-key braces otherwise remain Records, and spread-only Maps require context. Spread operands must be Maps. Every key runs before its value, entries run once in source order, and failure skips subsequent entries. Later duplicates replace earlier values; iteration keeps canonical key order and aliases retain earlier contents. Quoted dots are one key, and incompatible concrete values do not widen to Any.
effects: none
tags: language, map-literals
example:
  const name = "tasks"
  const seed: Map[Int] = {total: 3}
  let counts = {...seed, [name]: 2, total: 4}
  print counts.keys().join(",")
  print (counts.get(name) ?? 0)
  
  let attempts: Map[Int, Str] = {[2]: "retry", [0]: "initial"}
  
  for {key: attempt, value: label} in attempts {
    print f"{attempt}: {label}"
  }

api: language.core.membership
kind: language
purpose: Tests membership with in and not in.
contract: Membership evaluates the left operand before the right. It uses literal substrings for Str, contiguous subsequences for Bytes, value equality for List, and Str key/field presence for Map and Record, including present null values. Path retains display-text containment; env.PATH retains exact entry membership. Streams are not consumed. Bind operands in their original order when replacing a receiver-first method call whose evaluations cannot safely be reordered.
effects: "key" in mapping, "field" not in fields
signature: item in container; item not in container
tags: language, membership, in, not in

api: language.core.native-tests
kind: language
purpose: Defines native XSH test declarations and harness context.
contract: Top-level test NAME [effects]? { |ctx| ... } declarations check as Result[Unit], accept zero or one immutable TestContext parameter, and register without execution. The native-tests feature enables declarations and test-only host helpers.
effects: none
tags: language, native-tests

api: language.core.optional-postfix
kind: language
purpose: Guards Optional field, method, index, and slice operations.
contract: ?. and adjacent ?[ skip the whole operation on null, including arguments and bounds. Result receivers propagate one outer Result with ordinary error checks. Optional methods returning Result retain both layers; ?? removes only the checked outer Optional layer. An Any receiver must be validated before field, index, slice, or method access, including guarded access.
effects: none
tags: language, optional-postfix
example:
  const override_name: Str? = null
  let label = override_name?.trim() ?? "default"
  const samples: List[Int]? = [1, 2, 3]
  let first = samples?[0] ?? 0
  let remaining = samples?[1..] ?? []
  const text: Str? = "42"
  let number = (text?.parse_int() ?? Ok(0))?
  print $label $first $number
  print remaining.len()

api: language.core.path-literals
kind: language
purpose: Defines typed path literals.
contract: Formatted Path literals append Path fragments as native bytes and other displayable fragments as UTF-8. Concatenation does not join, normalize, access, or confine paths. NUL is rejected. F-strings, print, and explicit .display() remain human text.
effects: none
tags: language, path-literals
example:
  const config_path = p"config.txt"
  
  let sidecar = fp"{config_path}.sha256"

api: language.core.pattern-conditionals
kind: language
purpose: Selects branches and loop iterations with literal patterns.
contract: `if let Pattern = subject` evaluates the subject once; `while let Pattern = subject` reevaluates it once per check, including after continue. Ordinary constructor, record, literal, type, facet, and sequence patterns retain their matching rules. Successful captures are immutable and atomic branch or iteration locals; mismatch selects the else branch or ends the loop without unwrapping Result or Optional values. Explicit propagation and lexical control targets retain their meaning. Irrefutable binding conditions are rejected. Value-producing if-let requires an else and compatible branch values. Pattern scopes clean temporary resources and captures on every exit. `lint.pattern-conditional` preserves meaningful complements, guards, and comments.
effects: none
tags: language, pattern-conditionals

api: language.core.pattern-tests
kind: language
purpose: Tests a value against a non-binding pattern.
contract: `value is Pattern` evaluates the subject once and returns Bool without unwrapping or propagating a Result. RHS names resolve to types, nominal error facets, or constructors; unknown and ambiguous names are rejected. Constructor and record payloads may contain wildcards, literals, and nested non-binding patterns. Binding patterns and RHS alternation are rejected. Type tests retain the dynamic subject boundary; stable immutable subjects narrow the selected true branch. Use `!(value is Pattern)` for negation and `or` between complete tests. `is` shares equality precedence. `lint.boolean-pattern-test` safely replaces trivial boolean matches only when their complement, evaluation count, and comment retention are proved.
effects: none
tags: language, pattern-tests

api: language.core.postfix-question
kind: language
purpose: Defines postfix ? error propagation.
contract: ? unwraps a Result or returns its error and requires the declared error effect outside try/retry blocks.
effects: none
tags: language, postfix-question
example:
  let value = Ok("ready")?

api: language.core.print
kind: language
purpose: Prints values to standard output.
contract: `print` writes its arguments separated by a single space and appends a newline to stdout; `eprint` does the same on stderr. `--flush` is recognized only as the first argument and writes to the inherited stream immediately instead of the captured script-output buffer. Both return Unit and require no declared effect. Accepted values are human-facing scalars: Str, Int, Bool, and Path; Path uses display conversion without canonicalizing. Command-word position interpolates with `$name` or `${expr}`, while expression string literals such as `"$name"` never interpolate; use `f"{expr}"` for expression-string interpolation.
effects: none
signature: print [--flush] ARG...
tags: language, print, output, builtin
example:
  const name = "world"
  print "hello" $name
  print f"count {1 + 2}"

api: language.core.procs
kind: language
purpose: Defines procedure declarations and calls.
contract: Procedure calls preserve lexical scope, declared effects, return types, and runtime trace containment. In expression calls, `name:` before a comma or closing parenthesis passes the ordinary lexical value `name`, with the same named-argument checks as `name: name`. `...options` supplies exactly the checked finite Record fields to static parameter slots; hidden runtime fields are excluded. Unknown or duplicate names are errors, null is supplied, and entries evaluate once in written order before ordinary defaults fill omitted parameters.
effects: none
tags: language, procs
example:
  const defaults = {name: "world"}
  
  proc greet(name: Str) -> Str {
    name
  }
  
  let options = defaults
  let greeting = greet(...options)
  let name = options.name
  let punned = greet(name:)
  print $greeting
  print $punned

api: language.core.pure-functions
kind: language
purpose: Defines effect-free function declarations.
contract: Pure functions cannot cross host-effect boundaries. Defaulted parameters infer concrete checked types from their defaults, including constants, field projections and permitted calls; null and unconstrained empty collections require annotations. Defaults resolve outside the parameter scope and evaluate once only for omitted slots, in parameter order after supplied arguments. Private helpers and exported pure declarations may omit annotations when their definitions establish one closed type or rank-1 scheme. Recursive components share provisional types until every body is solved; independent calls then instantiate the published schemes freshly. Explicit annotations retain their checked bounds; empty or dynamic shapes without an independent type anchor require annotations. Inference never creates a Result boundary merely from ?. xsht check --annotate=returns can render source-representable inferred returns.
effects: none
tags: language, pure-functions
example:
  const build_defaults = {jobs: 4, timeout: 30s}
  
  pure add(left: Int, right: Int) {
    left + right
  }
  
  pure build_jobs(jobs = build_defaults.jobs + 1) -> Int {
    jobs
  }
  
  pure initial_jobs() -> Int {
    4
  }
  
  pure selected_jobs(jobs = initial_jobs()) -> Int {
    jobs
  }

api: language.core.records
kind: language
purpose: Defines structural and named record values.
contract: User record schemas and aliases support explicit lexical type applications, including qualified applications; substituted fields retain concrete types. Static constructors take named fields, or leading positional fields in declaration order when no single value fits two of the fields they fill (equal types after aliases, `T` and `T?`, `Int` and `UInt`, `Str` and `Path`, any two lists, records, or maps, `Any`, or a type parameter); overlapping fields must be named so swapped arguments are always type errors. They infer fresh arguments from supplied fields and independently annotated expected applications, including qualified imports. Arguments must agree exactly without numeric widening or guessed Any. Null, empty containers, and unused parameters alone leave inference ambiguous and require an annotation. Universal defaults must work for every argument type and never select inferred arguments. Concrete aliases remain available. Schema-owned defaults are bounded literal constants from the defining module; they apply only to constructors. Supplied arguments run once in source order, and each constructed value has independent value semantics. Named records are checked at their boundary; dynamic record access must be narrowed before typed field use. Functional updates use one leading record snapshot and disjoint existing field paths, such as `{...config, build.jobs: jobs}`. Replacements run once in source order before publishing the rebuilt value, retain the schema, and do not reapply defaults.
effects: none
tags: language, records
example:
  type Config = {name: Str, enabled: Bool = true}
  
  const name = "demo"
  let config = Config(name:)
  print $config.name
  
  const settings = {build: {jobs: 2, enabled: true}}
  let updated = {...settings, build.jobs: 4}
  print $updated.build.jobs
  
  type Observation[T] = {value: T? = null, samples: List[T] = []}
  
  type CountObservation = Observation[Int]
  
  let count = Observation(value: 7)
  let direct = Observation(value: "demo", samples: ["demo"])
  let absent: CountObservation = Observation(value: null)
  print ${count.value ?? 0}
  print direct.samples[0]
  print absent.samples.len()

api: language.core.regex-literals
kind: language
purpose: Prepares reusable regular expressions during checked program loading.
contract: `rx"..."` and `rx"""..."""` produce Regex with raw contents: no escapes or interpolation. The existing regex engine validates every occurrence, including unreachable code, before execution; repeated calls share the prepared engine. Dynamic `regex.compile(pattern)` retains Result-valued runtime errors.
effects: none
tags: language, regex-literals
example:
  const assignment = rx"^\s*[A-Z_]+=([0-9]+)$"
  let matched = assignment.matches("COUNT=42")
  let number = assignment.captures("COUNT=42")[1]
  const runtime_pattern = "[a-z]+"
  let dynamic = regex.compile(runtime_pattern)?

api: language.core.results
kind: language
purpose: Defines Result values and error families.
contract: Expected host failures remain Result data until ? or another explicit boundary propagates them.
effects: none
tags: language, results
example:
  let value = Ok("ready")

api: language.core.run
kind: language
purpose: Defines process run forms and status boundaries.
contract: Statement, value, status, capture, and stream forms differ in whether a nonzero child exit is asserted, returned, or wrapped as Result data.
effects: none
tags: language, run

api: language.core.scalar-iteration
kind: language
purpose: Iterates directly over retained Unicode scalars and raw bytes.
contract: Direct for loops and comprehension clauses accept Str and Bytes, including their supported outer Results. Str yields one-scalar Str values without normalization; Bytes yields Int values in 0..255 without decoding. The source evaluates once per reached clause and retains its snapshot and view bounds across reassignment. Empty sources have no items. Cursors construct only the next element, preserving immutable bindings, checkpoints, lexical transfers, suspension, cleanup, and error identity. This rule adds no pipeline, List splice, or yield-delegation source. lint.prefer-scalar-iteration removes equivalent direct empty splits or proved full stable byte ranges with unused offsets.
effects: none
tags: language, scalar-iteration
example:
  for character in "café" {
    print $character
  }
  
  for octet in b"\0\xff" {
    print $octet
  }
  
  let widths = [character.byte_len() for character in "éx"]
  print widths.len()

api: language.core.schema-validation
kind: language
purpose: Validates untrusted input against a concrete checked schema.
contract: value.require(Type) returns Result[Type, Error] after recursive validation or schema-directed conversion and preserves unknown record fields. value.require_exact(Type) rejects unknown fields with a schema error at their path; value.require_stripped(Type) removes them. Each policy recurses through declared structural record slots, lists, map values, optional slots, union alternatives in order, and validated bases. Any and erased Record slots remain opaque; maps retain their keys. Stripping precedes the final refinement check; failed validation never mutates the source. No policy supplies constructor defaults, and encoding observes actual retained fields. Static record typing stays width-compatible. All three operations support omitted targets. value.require() retains that operation when an explicit annotation or uniquely selected checked parameter or return contract independently supplies the concrete target. Expected types flow through value blocks, branches, Ok, and individual question propagation layers. Any, erased Record or callable contracts, unresolved type arguments, future field uses, fallback values, and ambiguous overloads cannot supply a target. An annotation alone never validates input. Safe argument omission compares concrete generic instances and conversion identities, preserving validation and Result handling.
effects: none
signature: value.require(Type) -> Result[Type, Error]; value.require_exact(Type) -> Result[Type, Error]; value.require_stripped(Type) -> Result[Type, Error]
tags: language, schema, record, unknown-fields

api: language.core.slicing
kind: language
purpose: Selects half-open List, Str, and Bytes ranges.
contract: `value[start..end]` evaluates receiver and explicit Int bounds once in order. Omitted bounds use zero/length, negative bounds count from the end, bounds clamp to length, and reversed ranges are empty. Str counts Unicode scalars; Bytes counts bytes. List results retain value semantics; immutable text/bytes may share backing storage. Offset/count methods retain their distinct error rules.
effects: none
tags: language, slicing
example:
  const data = b"abcdef"
  let prefix = data[..2]
  let suffix = data[2..]
  let scalars = "aé🦀"[1..3]
  let items = [10, 20, 30][1..]

api: language.core.source-files
kind: language
purpose: Defines source-file and module loading boundaries.
contract: Source files are parsed and checked as modules; imports retain their source boundaries for diagnostics and runtime loading.
effects: none
tags: language, source-files

api: language.core.statements
kind: language
purpose: Defines statement sequencing and result propagation.
contract: Checked statement position asserts Bool values and propagates Result[Unit] failures. Value position preserves Bool values; exhaustive if/match tails and their lexical multi-statement branches can supply function and callback values.
effects: none
tags: language, statements
example:
  let answer = 40 + 2
  print $answer

api: language.core.streams
kind: language
purpose: Defines lazy structured stream values.
contract: A stream owns source cleanup until a terminal, cancellation, or failure consumes that lifecycle. Inside stream producers, yield @source delegates List or Stream elements lazily, resumes after exhaustion, and closes children before parent cleanup on early termination. Results require explicit handling.
effects: none
tags: language, streams
example:
  stream rows() [] -> Stream[Int] {
    yield @[1, 2]
    yield 3
  }
  
  stream forwarded() [] -> Stream[Int] {
    yield @rows()
    yield @[4, 5]
  }
  
  for value in forwarded() {
    print $value
  }

api: language.core.try
kind: language
purpose: Captures local Result propagation in a value block.
contract: try executes once and wraps normal completion in Ok. Result tails remain nested data unless explicitly propagated. The nearest region captures question propagation and assertions after cleanup; lexical return and loop transfers retain their destinations. An expected Unit success consumes assertions as statements. Local capture requires host effects but no error effect solely for caught propagation, and emits no retry attempts.
effects: none
tags: language, try

api: language.core.value-pipelines
kind: language
purpose: Places an ordinary pipeline input in a checked call.
contract: A bare method call uses the input as its receiver; a qualified function call inserts it as the first argument. Exactly one whole positional or named argument may be _ to select the input position instead. Nested, embedded, spread and multiple holes are rejected. Input evaluates once before the callee or receiver and other arguments, which retain ordinary source order. Optional calls skip their other arguments when absent, after input evaluation. Explicit ? retains its Result propagation boundary. Structured stream stages retain their existing dispatch and do not accept holes as per-item mapping.
effects: none
tags: language, value-pipelines
example:
  pure pipeline_join(prefix: Str, value: Str, suffix: Str) -> Str {
    prefix + value + suffix
  }
  
  let wrapped = "middle" |> pipeline_join("[", _, "]")
  print $wrapped
```

## A tour of XSH

Write a checked script that runs commands, handles typed data, and reports
failures. This tour builds toward a service preflight check and a small
project with a library, a command, and tests.

`xsh` runs `.xsh` scripts, `xsht` provides the toolchain, and `xshi` is an
interactive shell. This tour covers `xsh` programs. See the
[interactive SPEC](SPEC-INTERACTIVE.md) for `xshi`.

Every `xsh` block below is a complete program that passes `xsht check`. Blocks
marked `# platform: linux` read `/proc` or other Linux-only interfaces.

### Running a Script

```xsh
#!/usr/bin/env -S xsh --
print "hello from xsh"
```

```text
hello from xsh
```

`xsh` runs `.xsh` scripts; `xsht` provides the toolchain:

```bash
xsh hello.xsh                 # run it
xsh deploy.xsh -- web-2 -f    # arguments after `--` go to the script
chmod +x hello.xsh && ./hello.xsh
xsht check hello.xsh          # parse, resolve, and type-check without running
```

`xsh` checks the whole program before executing anything. A type error on the
last line means the first line never runs, so a half-applied change caused by
a typo three screens down cannot happen. Check failures exit with status 2;
runtime failures exit with status 3.

Script arguments arrive as `args: List[Str]`. For anything beyond a couple of
positional words, declare the interface instead of parsing it. A `cli main`
signature is the argument parser, the usage text, and the type conversion:

```xsh
#!/usr/bin/env -S xsh --
# Show the largest files under ROOT.
cli main(root: Path, limit: UInt = 5, hidden = false) {
  let largest = fs.files(root, stat: true, hidden:)?
    |> sort-by(desc: true) .size
    |> take(limit)

  for entry in largest {
    print f"{entry.size:>12} {entry.path}"
  }
}
```

Required parameters are positional; defaulted ones become options
(`--limit 3`, `--hidden`); `-h` prints generated help. A bad `--limit` value
is rejected with usage status 2 before any of your code runs. A tool with
several commands declares one entry for each, named by its subcommand path:
`cli main repo check(repo = default_repo()) { ... }` runs for
`tool repo check`, and `tool --help` lists the commands.

### Values at a Glance

```xsh
const host = "db-01"
const port = 5432
const load = 0.75
const grace = 90s
const conf = p"/etc/postgresql/postgresql.conf"
const replicas = ["db-02", "db-03"]
const limits = {cpu: 2, memory_mb: 4096}
const owner: Str? = null
let data = fp"/srv/{host}/data"

print f"{host}:{port} load={load} grace={grace} doubled={grace * 2}"
print f"{conf.name()} in {conf.parent()} (.{conf.ext()})"
print f"data={data} replicas={replicas.join(",")}"
print f"cpu={limits.cpu} owner={owner ?? "nobody"}"
```

```text
db-01:5432 load=0.75 grace=90s doubled=3m
postgresql.conf in /etc/postgresql (.conf)
data=/srv/db-01/data replicas=db-02,db-03
cpu=2 owner=nobody
```

Every value has a type, and the checker infers most of them: `Str`, `Int`,
`Float`, `Duration`, `Path`, `List[Str]`, a record `{cpu: Int, memory_mb: Int}`,
and an optional `Str?`. `const` is data fixed when the script is checked, `let`
is an immutable runtime binding, and `var` is mutable. `f"..."`
interpolates `{expr}`; plain `"..."` never interpolates.

There are no implicit conversions. `"8080" + 1` is a check error, and
`"8080".parse_int()` returns `Result[Int]` because parsing can fail. You will
see that pattern everywhere: anything that can fail says so in its type.
`"8080" as Int` is the same parse with the failure propagated, for the usual
case where bad input should stop the function.

### Commands and argv

The bug every shell scripter has shipped at least once:

```bash
file="quarterly report.pdf"
rm $file                      # removes "quarterly" and "report.pdf"
flags="-l -a"
ls $flags "$dir"              # works only *because* of word splitting
for f in $(find . -name '*.log'); do gzip $f; done   # spaces, globs, newlines
```

In XSH a value is one argv item. Lists splice only where you write `@`. Words
are never split, globbed, or expanded:

```xsh
const file = "quarterly report.pdf"
const flags = ["-l", "-a"]
let argv = run.text printf "<%s>\n" $file @flags "*.log"
print argv.trim()
```

```text
<quarterly report.pdf>
<-l>
<-a>
<*.log>
```

`$name` and `${expr}` interpolate into a command word; `"*.log"` stays a
literal asterisk. When you actually want a glob, say so with a glob literal,
`g"*.log"`, which produces a `List[Path]` you can splice with `@`. There is no
shell underneath: external programs only run through `run`, and a bare word
like `make` in statement position is an unresolved name, not a `PATH` lookup.

The `run` family chooses what you get back:

| Form | Result |
|---|---|
| `run cmd ...` | statement: fails the script on nonzero exit; value: `Status` |
| `run.status cmd ...` | `Status`, never fails on exit code |
| `run.text cmd ...` | `Str`: captured stdout; fails on nonzero exit |
| `run.bytes cmd ...` | `Bytes`; fails on nonzero exit |
| `run.capture --text cmd ...` | `{status, stdout, stderr}`; fails only if the command cannot run |
| `run.stream --text cmd ...` | `Stream[Str]`: lines as they arrive |
| `try run.text cmd ...` | `Result[Str]`: the failure as a value, for any form above |

Byte pipelines and redirections look the way you expect:
`run tar -cf - $dir | run zstd -q > $archive`. Environment and working
directory changes are scoped to a block, so they cannot leak into the rest of
the script. `tempdir dir { ... }` is the same kind of scope for scratch space:
`dir` is a fresh temporary directory, removed when the block ends however it
ends:

```xsh
tempdir dir {
  cd (dir) {
    p"notes.txt".write("hi\n")
    let listing = run.text ls
    print f"inside: {listing.trim()}"
  }
}

env ({LC_ALL: "C", GREETING: "hello world"}) {
  let said = run.text printenv GREETING
  print f"child saw: {said.trim()}"
}

let outside = e"GREETING" ?? "(unset)"
print f"after the block: {outside}"
```

```text
inside: notes.txt
child saw: hello world
after the block: (unset)
```

`e"GREETING"` reads a variable; [Environment Variables](#environment-variables)
covers reading, setting, and what children inherit.

### Paths

Paths are a type, not strings that happen to contain slashes. They hold native
bytes, so a filename that is not valid UTF-8 still round-trips to `run`
untouched.

```xsh
const log_dir = p"/var/log/nginx"
const today = p"access.log"
let rotated = fp"{log_dir}/{today}.1"

print $rotated
print f"name={rotated.name()} ext={rotated.ext()} parent={rotated.parent()}"
print f"as gzip: {rotated.with_ext("gz")}"
```

```text
/var/log/nginx/access.log.1
name=access.log.1 ext=1 parent=/var/log/nginx
as gzip: /var/log/nginx/access.log.gz
```

Literals that start with `/`, `./`, or `../` are paths. `p"..."` makes a path
from any literal and `fp"..."` builds one with interpolation. There is no `/`
operator on paths and no implicit normalization: what you wrote is what the
kernel sees. An operation on one path is a method (`read_text`, `lines`,
`write`, `write_atomic`, `exists`, `mkdir`, `copy`, `rename`); the rest are
`fs.*` functions (`fs.files`, `fs.walk`, `fs.copy_tree`, `fs.mounts`).

### Errors, Results, and `?`

`set -euo pipefail` is a promise the shell does not keep:

```bash
set -euo pipefail
count=$(grep -c ERROR app.log)  # zero matches: grep exits 1, script dies silently
deploy() { cd /srv/app; git pull; systemctl restart app; }
if deploy; then echo ok; fi     # set -e is off inside deploy: every step runs
local version=$(get_version)    # `local` succeeds, so the failure is masked
```

XSH has no `set -e` to forget. A `run` in statement position that exits
nonzero fails the script, every time, in every context, with the command that
failed:

```xsh
proc backup(src: Path, dest: Path) {
  run tar -czf $dest $src
  print "backup written"
}

backup(p"/var/lib/app", p"/backups/app.tgz")
```

```text
tar: Failed to open '/backups/app.tgz'
err: `tar` exited 1
executable: /usr/local/bin/xsh
cwd: /home/ops
argv: tar -czf /backups/app.tgz /var/lib/app
at backup.xsh:2:3-2:26
call path:
  1. proc backup at backup.xsh:6:1-6:39
```

When a nonzero exit is an answer rather than a failure, say which codes are
acceptable. `grep` exits 1 for "no matches":

```xsh
let scratch = fs.tempdir()?
let log = fp"{scratch.host_path()?}/app.log"
log.write("ok\nERROR disk full\nok\nERROR link down\n")

let errors = run.text --accept=[0, 1] grep -c ERROR $log
let panics = run.text --accept=[0, 1] grep -c PANIC $log
print f"errors={errors.trim()} panics={panics.trim()}"
```

```text
errors=2 panics=0
```

Functions that can fail return `Result[T]`. Four tools handle it:

- `expr?` unwraps `Ok` or returns the `Err` to the caller.
- `expr ?? fallback` recovers with a default.
- `match` handles each outcome.
- `guard ... else { ... }` exits early when a precondition fails.

Expected failures get names. An `error` declaration is a small family of typed
variants that callers can match on, instead of grepping message strings:

```xsh
error PortError = Missing(file: Path) | Invalid(text: Str)

proc read_port(file: Path) -> Result[Int] {
  guard file.exists() else {
    return Err(PortError.Missing(file:))
  }

  let text = file.read_text()?.trim()
  text.parse_int() ?? { |_| Err(PortError.Invalid(text:))? }
}

let scratch = fs.tempdir()?
let dir = scratch.host_path()?
fp"{dir}/good".write("8080\n")
fp"{dir}/bad".write("eighty\n")

print f"good: {read_port(fp"{dir}/good")?}"
print f"missing, with default: {read_port(fp"{dir}/none") ?? 80}"

match read_port(fp"{dir}/bad") {
  Ok(port) => print f"port {port}"
  Err(PortError.Invalid {text}) => print f"not a port: {text}"
  Err(error) => print f"unreadable: {error.message}"
}
```

```text
good: 8080
missing, with default: 80
not a port: eighty
```

The checker also notices that `read_port` reads files and can fail, and every
caller inherits those effects; the [Effects](#effects) section covers them.

Two more tools round this out. `ctx "description" { ... }` attaches context to
anything that fails inside the block, so a bare "No such file or directory"
becomes "No such file or directory (ctx: loading /etc/app.json)". `try { ... }`
turns a block into a `Result` value when you want to collect failures as data
instead of propagating them.

### Environment Variables

In shell, `$DEPLOY_TARGET` is the empty string whether the variable is unset,
empty, or misspelled, and `export` changes every later command in the script.
XSH reads a variable with an e-string, `e"NAME"`, whose value is a
`Result[Str]`: a missing variable is an `Err` you handle like any other.

```xsh
# The first variable that is set wins; a missing one is an Err that `??`
# replaces.
let target = e"DEPLOY_TARGET" ?? e"DEFAULT_TARGET" ?? "staging"
print f"target: {target}"

env ({JOBS: "8", VERBOSE: "yes", PREFIX: "/opt/app"}) {
  let jobs = env.int("JOBS", 1)?
  let verbose = env.bool("VERBOSE")?
  let prefix = env.Path.PREFIX?
  print f"jobs={jobs} verbose={verbose} bin={fp"{prefix}/bin"}"

  # A value that is not what it claims is an error, not a silent default.
  match env.int("VERBOSE", 1) {
    Ok(n) => print f"verbose level {n}"
    Err(error) => print f"VERBOSE: {error.message}"
  }
}

# Text reads never decode bytes lossily; path reads keep them.
let data = b"/srv/caf\xe9" as Path
env ({DATA_DIR: data}) {
  match e"DATA_DIR" {
    Ok(text) => print f"text: {text}"
    Err(error) => print f"as text: {error.message}"
  }

  print f"as a path, the bytes survive: {env.Path.DATA_DIR? == data}"
}
```

```text
target: staging
jobs=8 verbose=true bin=/opt/app/bin
VERBOSE: environment value is not an integer
as text: environment value is not valid UTF-8
as a path, the bytes survive: true
```

`e"NAME"` is the same read as `env.Str.NAME` and `env.get("NAME")`, so `?`
propagates a missing variable, `??` supplies a default, and `match` tells the
cases apart. The name is a literal identifier and e-strings never interpolate;
read a computed name with `env.get(f"{prefix}_HOME")`. Typed reads do the
conversion and keep a malformed value an error: `env.int` and `env.bool` take a
fallback used only when the variable is unset, and `env.Path.NAME` and
`env.path` read native bytes.

Environment values are bytes, which is the non-UTF-8 rule's reason for being:
a text read of a value that is not valid UTF-8 fails rather than guessing,
while a `Path` read and every child process get the bytes unchanged.

Assigning to an e-string sets a variable:

```xsh
e"STAGE" = "build" # set for the rest of the script

env ({LC_ALL: "C"}) {
  e"STAGE" = "test" # undone when this scope ends
  e"RETRIES" = 3 # converted like an argv item
  let stage = run.text printenv STAGE
  let retries = run.text printenv RETRIES
  print f"child sees: {stage.trim()} {retries.trim()}"
}

print f"after the scope: STAGE={e"STAGE"?} RETRIES={e"RETRIES" ?? "(unset)"}"

# `NAME=value` before a command sets it for that one child.
let once = run.text STAGE=deploy printenv STAGE
print f"one command saw {once.trim()}; the script still has {e"STAGE"?}"

let scratch = fs.tempdir()?
let tools = scratch.host_path()?
let tool = fp"{tools}/hello-tool"
tool.write("#!/bin/sh\necho \"hello from $STAGE\"\n", mode: 0o755)

env ({STAGE: "release"}) {
  env.PATH.prepend(tools)
  run hello-tool
}

print f"tools on PATH afterwards: {tools in env.PATH}"
```

```text
child sees: test 3
after the scope: STAGE=build RETRIES=(unset)
one command saw deploy; the script still has build
hello from release
tools on PATH afterwards: false
```

`e"NAME" = value` takes any value that converts to one command argument, the
way `run` converts its arguments: a `Path` keeps its bytes and `3` becomes
`"3"`. The right-hand side is an ordinary expression, so `e"CC" = clang` names
a binding and the text is `"clang"`. There is no unset, and `null` is rejected
rather than meaning one.

An `env (record) { ... }` block is a scope: when it
ends, by finishing, failing, or returning early, the environment goes back to
what it was when the block started. An assignment therefore lasts until the
innermost enclosing `env` block ends, and outside any block for the rest of the
script. A proc that sets a variable sets it for its caller too, because the
environment belongs to the running script, not to a binding.

A child process inherits XSH's environment as it is when the child starts:
what the script started with, plus assignments, scope overlays, and `env.PATH`
edits. A `NAME=value` word before a command adds to that child alone. XSH never
changes its own process environment, so none of this leaks into the host.

`env.PATH` is a typed view of `PATH`: `prepend`, `append`, and `pop` take and
return `Path` values, and `dir in env.PATH` tests an exact entry, never a
substring. Like an assignment, the edit ends with its `env` scope.

Reading and setting are the `env` effect, so a `pure` function can do neither.

### Records, Lists, and Maps

Records are the unit of structure. A `type` names a schema; the checker then
knows every field and rejects typos.

```xsh
type Host = {name: Str, role: Str, cores: Int}

const fleet: List[Host] = [
  {name: "web-1", role: "web", cores: 4},
  {name: "web-2", role: "web", cores: 8},
  {name: "db-1", role: "db", cores: 16},
]

let web = [h.name for h in fleet if h.role == "web"]
let cores_by_name = {h.name: h.cores for h in fleet}
print f"web: {web.join(" ")}; db-1 has {cores_by_name.get("db-1") ?? 0} cores"

for {name, cores, ..} in fleet {
  print f"{name}={cores}"
}

let upgraded = {...fleet[0], cores: 32}
print f"{upgraded.name}: {fleet[0].cores} -> {upgraded.cores}"

for role in fleet |> group-by .role {
  print f"{role.key}: {[h.name for h in role.items].join(",")}"
}

match "deploy web-2 --force".fields() {
  ["deploy", target, ..flags] => print f"deploy {target} with {flags.len()} flag(s)"
  ["status"] => print "status"
  else => print "usage: deploy TARGET | status"
}
```

```text
web: web-1 web-2; db-1 has 16 cores
web-1=4
web-2=8
db-1=16
web-1: 4 -> 32
web: web-1,web-2
db: db-1
deploy web-2 with 1 flag(s)
```

Values have value semantics: `upgraded` is a new record and `fleet` is
unchanged. Maps iterate in key order and `group-by` keeps encounter order, so
output is deterministic. List patterns in `match` replace most hand-written
argument dispatch.

A schema's constructor also takes fields positionally, in declaration order,
when no single value could fit two of them. Where the checker knows which enum
or error family a value must be, `.Binary` names its variant:

```xsh
enum Kind { File, Binary, Symlink }

type Entry = {path: Path, kind: Kind, executable: Bool = false}

const manifest: List[Entry] = [
  Entry(p"usr/bin/xsh", .Binary, executable: true),
  Entry(p"etc/xsh.conf", .File),
  Entry(p"usr/bin/sh", .Symlink),
]

let binaries = [f"{e.path}" for e in manifest if e.kind == .Binary and e.executable]
print f"binaries: {binaries.join(", ")}"
```

```text
binaries: usr/bin/xsh
```

`Host("web-1", "web", 4)` is an error instead, because `name` and `role` are
both `Str` and a swap would go unnoticed; such fields are passed by name. A
leading-dot variant needs that expected type: `let kind = .Binary` is an
error, and inside a stream stage `.name` always reads the item.

### Streams and Pipelines

The classic log one-liner:

```bash
grep ' 500 ' access.log | awk '{print $7}' | sort | uniq -c | sort -rn | head
```

It works until a response size happens to be 500, or the log format gains a
field and `$7` silently becomes something else. Every stage re-parses text the
previous stage flattened.

XSH parses once, at the edge, into records. After that, `|>` passes typed
values between stages:

```xsh
const access_line = rx"""^(\S+) \S+ \S+ \[[^\]]+\] "(\S+) (\S+) [^"]*" (\d{3}) (\d+)"""

type Hit = {client: Str, method: Str, url: Str, status: Int, size: Int}

stream hits(file: Path) [fs, error] -> Stream[Hit] {
  for line in file.lines()? {
    let fields = access_line.captures(line)
    if fields.len() == 6 {
      yield Hit(
        client: fields[1],
        method: fields[2],
        url: fields[3],
        status: fields[4] as Int,
        size: fields[5] as Int,
      )
    }
  }
}

const sample = """
  10.0.0.5 - - [03/Oct/2026:10:00:01 +0000] "GET /api/users HTTP/1.1" 200 512
  10.0.0.7 - - [03/Oct/2026:10:00:02 +0000] "GET /api/orders HTTP/1.1" 500 31
  10.0.0.5 - - [03/Oct/2026:10:00:03 +0000] "POST /api/orders HTTP/1.1" 500 31
  10.0.0.9 - - [03/Oct/2026:10:00:04 +0000] "GET /healthz HTTP/1.1" 200 500
  a line that is not an access log entry
  10.0.0.7 - - [03/Oct/2026:10:00:05 +0000] "GET /api/users HTTP/1.1" 503 0
  """

let scratch = fs.tempdir()?
let log = fp"{scratch.host_path()?}/access.log"
log.write(sample)

let failures = hits(log)
  |> where .status >= 500
  |> count { .url }

for {key, value} in failures {
  print f"{value} {key}"
}

let heaviest = hits(log) |> sort-by(desc: true) .size |> take(2) |> map .client
print f"heaviest clients: {heaviest.join(" ")}"
```

```text
2 /api/orders
1 /api/users
heaviest clients: 10.0.0.5 10.0.0.9
```

`stream` declares a lazy producer: nothing is read until a pipeline pulls, and
a pipeline that stops early closes the file. `rx"..."` regexes are compiled
and validated by `xsht check`, so a broken pattern is a check error rather than
a runtime surprise. `$` means the end of the whole text, not the end of a line,
so trim what you read from files before anchoring (or use `(?m)`). The
`/healthz` line with a 500-byte body is not miscounted
as a server error, because `status` and `size` are different fields with
different meanings, not columns 9 and 10.

Pipelines evaluate to a `List`. The stage vocabulary is small and regular:
`where`, `map`, `flat-map`, `sort`, `sort-by`, `take`, `drop`, `unique-by`,
`enumerate`, `batch`, `par-map`, and terminals such as `count`, `sum`,
`first`, `group-by`, `fold`, and `each`. A stage block that names no
parameter reads the item as `.`: `count { .url }` is
`count { |hit| hit.url }`, and `where .status >= 500` is the same block
without its braces. Name the parameter when a nested block needs to see the
outer item, since `.` there means the inner one. `xsht api
language:stream.where` (and so on) documents each stage.

Keep the two pipe operators straight: `|` connects processes byte-to-byte,
exactly like the shell; `|>` connects XSH values.

### JSON Boundaries

`jq` is a second language you embed as strings inside the first. In XSH, JSON
is just data, with one rule: decoded JSON has type `Any`, and you must check
it against a schema before using its values. For an open shape, `json.get`
reads a path and returns an opaque value to validate or compare; `json.set`
and `json.remove` rebuild by path. Field and index navigation require a
validated receiver.

```xsh
type Service = {name: Str, port: Int, tags: List[Str]}

const raw = """
  [{"name": "api", "port": 8080, "tags": ["web", "public"]},
   {"name": "db", "port": 5432, "tags": []}]
  """

let services = json.decode(raw)?.require(List[Service])?

for svc in services {
  print f"{svc.name} -> {svc.port} [{svc.tags.join(",")}]"
}

let wrong = json.decode("""{"name": "cache", "port": "6379", "tags": []}""")?

match wrong.require(Service) {
  Ok(svc) => print f"unexpected: {svc.name}"
  Err(error) => print f"rejected: {error.message}"
}

let report = {count: services.len(), public: [s.name for s in services if "public" in s.tags]}
print (json.encode(report)?)
print (json.encode(report, pretty: true)?)
```

```text
api -> 8080 [web,public]
db -> 5432 []
rejected: schema check failed at port: expected Int, found Str
{"count":2,"public":["api"]}
{
  "count": 2,
  "public": [
    "api"
  ]
}
```

`.require(T)` validates the whole value, nested lists and records included,
and returns `Result[T]`. A missing or mistyped field fails at the boundary with
its path, not three functions later as a confusing `null`. Once validated,
every field access is checked statically.

When the shape is genuinely open, match on it with type patterns instead:

```xsh
let doc = json.decode("""{"version": 3, "features": ["ipv6"]}""")?
let version = if let {version: v is Int, ..} = doc { v } else { 0 }
print f"config version {version}"
```

```text
config version 3
```

`json.read(path)` and `json.write path (value)` do the same at file
boundaries. Records encode with sorted keys, so output is stable enough to
diff and commit. [`docs/SPEC.md`](SPEC.md) §14 has the full contract,
including JSON lines.

### Effects

Every proc has a set of effects: the kinds of host state it can touch. The
checker infers them from the body and everything it calls, so you rarely write
them down. `disk_used_kb` runs a process and can fail, so its effects are
`process` and `error`, and every caller inherits them:

```xsh
proc disk_used_kb(root: Path) -> Result[Int] {
  let out = run.text du -sk $root
  out.fields()[0] as Int
}

pure percent(part: Int, whole: Int) -> Int {
  if whole == 0 { 0 } else { part * 100 / whole }
}

# This script may touch files and run processes, and nothing else.
proc main() [fs, process, error] {
  let scratch = fs.tempdir()?
  let dir = scratch.host_path()?
  fp"{dir}/data".write("hello\n")

  print f"measured: {disk_used_kb(dir)? > 0}; 3 of 4 is {percent(3, 4)}%"
}
```

```text
measured: true; 3 of 4 is 75%
```

The effects are `fs`, `process`, `net`, `env`, `time`, `io`, and `error`
(permission to propagate with `?`). A clause between the parameters and the
return type turns the inferred set into a promise: an upper bound the checker
enforces through every call. `main` above may touch files and run processes,
nothing more, so if `disk_used_kb` someday also posted its result to a stats
server, the script would stop checking:

```text
err[check.effect-violation]: effect `net` required by `disk_used_kb` is not in caller's declared effects
  disk.xsh:18:22
    print f"measured: {disk_used_kb(dir)? > 0}; 3 of 4 is {percent(3, 4)}%"
                       ^^^^^^^^^^^^^^^^^ effect `net` required by `disk_used_kb` is not in caller's declared effects
```

Write a clause where it states a promise: on an exported proc whose callers in
other modules rely on its effects not growing as the implementation changes,
or on a surface you want kept narrow, such as an entry point that must never
reach the network. Without a clause, an export publishes whatever it infers. A
clause on a private proc that only repeats the inferred set is noise, and
`xsht lint` offers to delete it.

`pure` functions have no effects at all: no processes, no files, no clock.
They are where parsing and policy belong, and they are trivially testable.

### Functions and Inference

The rule of thumb: annotate parameters, let the checker infer the rest.

```xsh
type Disk = {mount: Str, used: Int, size: Int}

const inventory: List[Disk] = [
  {mount: "/", used: 45, size: 50},
  {mount: "/var", used: 99, size: 100},
  {mount: "/home", used: 10, size: 100},
]

pure usage(d: Disk) {
  d.used * 100 / d.size
}

pure hottest(disks: List[Disk], over = 80) {
  disks
    |> where { |d| usage(d) >= over }
    |> sort-by(desc: true, block: usage)
    |> first()
}

proc mount_of(target: Path) {
  let m = fs.mount_for(target)?
  Disk(mount: m.mounted_on.display(), used: m.used_1k, size: m.blocks_1k)
}

let worst = hottest(inventory)?
print f"{worst.mount} at {usage(worst)}%"
print f"anything at 100%: {hottest(inventory, over: 100) is Ok(_)}"
print f"root has capacity: {mount_of(p"/")?.size > 0}"
```

```text
/var at 99%
anything at 100%: false
root has capacity: true
```

Parameters carry types, or defaults that imply them (`over = 80` is an
`Int`). Local bindings rarely need annotations. A private `pure` function
infers its return type from its tail: `usage` returns `Int`, and `hottest`
returns `Result[Disk]` because `first()` fails on an empty list. A private
`proc` whose tail is a value returns `Result[T]`, so `mount_of` returns
`Result[Disk]` and callers use `?` or `??`. A proc whose body ends in a
statement returns `Result[Unit]`. Exported functions, `main`, and recursive
functions declare their return types, because those are promises other files
depend on. A proc that mixes early `return Err(...)` exits with a value tail
still infers `Result[T]`; its error type joins every failure path.

Arguments can be positional, named (`over: 100`), or punned (`over:` passes
the local `over`). Rest parameters (`...hosts: List[Str]`) collect the tail.
The last expression of a block is its value; `return` is for early exits.

### Concurrency: Fan Out, Collect

XSH exposes concurrency through processes and pipeline stages rather than
user-managed threads or futures. `par-map` runs its block on a bounded worker
pool and returns results in input order:

```xsh
let scratch = fs.tempdir()?
let root = scratch.host_path()?

for file in [
  "etc/hosts.conf",
  "etc/app.conf",
  "var/log/app.log",
  "var/log/app.log.gz",
  "srv/www/index.html",
  "srv/www/app.js",
  "srv/www/style.css",
  "srv/README",
] {
  let target = fp"{root}/{file}"
  target.parent().mkdir()
  target.write("x\n")
}

let per_dir = fs.children(root)?
  |> where .kind == "dir"
  |> par-map(jobs: 4) { |dir|
    {dir: dir.name, counts: fs.files(dir.path)? |> count { |f| f.ext }}
  }

let totals = per_dir |> fold(map.empty()) { |acc, part|
  var merged = acc
  for {key, value} in part.counts {
    merged[key] = (merged.get(key) ?? 0) + value
  }

  merged
}

for {key, value} in totals {
  let ext = if key == "" { "(none)" } else { key }
  print f"{value:>3} {ext}"
}
```

```text
  1 (none)
  2 conf
  1 css
  1 gz
  1 html
  1 js
  1 log
```

If a block uses `?` and one item fails, the stage stops scheduling new work
and the error propagates. Leave the `?` off and wrap the work in `try { ... }`
to get a `List[Result[T]]` instead, keeping every success and every failure.

For long-running processes that should overlap with other work, `spawn`
returns a handle and `wait` collects one or a list:

```xsh
const hosts = ["10.0.0.11", "10.0.0.12", "10.0.0.13"]

let probes = [spawn run --timeout=5s ping -c 1 $host > /dev/null? for host in hosts]
let statuses = wait probes?

for i in range(hosts.len()) {
  let state = if statuses[i].ok { "up" } else { "down" }
  print f"{hosts[i]} {state}"
}
```

Handles are owned by the scope that created them. If the script fails or is
interrupted before `wait`, XSH terminates and reaps the children; nothing is
left running in the background by accident.

### Cleanup with `defer`

The shell's cleanup story is `trap`:

```bash
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
lock=/run/rotate.lock
touch "$lock"
trap 'rm -f "$lock"' EXIT     # silently replaces the first trap: $tmp now leaks
```

`defer` registers cleanup for the enclosing block. Deferred actions run in
reverse order when the block exits for any reason: normal completion,
`return`, `?` propagation, or a runtime failure.

```xsh
proc rotate(dir: Path) {
  let lock = fp"{dir}/.rotate.lock"
  lock.write("locked\n")
  defer {
    lock.remove(missing_ok: false)
    print "released lock"
  }

  let staging = fp"{dir}/staging"
  staging.mkdir()
  defer {
    staging.remove_dir()
    print "removed staging"
  }

  print "rotating"
  let _ = fp"{dir}/missing.log".read_text()?
  print "never reached"
}

tempdir dir {
  match rotate(dir) {
    Ok(_) => print "rotated"
    Err(_) => print "rotate failed"
  }

  print f"left behind: {fs.children(dir)? |> count()}"
}
```

```text
rotating
removed staging
released lock
rotate failed
left behind: 0
```

`tempdir dir { ... }` is convenient when the code needs only a path. When it
needs rooted operations or must pass a directory capability, `fs.tempdir()`
returns an `FsRoot`; its owning scope closes it after its defers. Call
`scratch.close()` only for early release:

```xsh
let scratch = fs.tempdir()?

scratch.mkdir(p"logs")
scratch.write(p"logs/app.log", "ok\n")
print f"rooted read: {scratch.read_text(p"logs/app.log")?.trim()}"
print f"under the temp dir: {scratch.host_path()?.exists()?}"
```

```text
rooted read: ok
under the temp dir: true
```

#### Editing a config file safely

`sed -i` on a live config is a classic outage: no backup, no validation, and
nothing happens at all if the line you expected is missing. Do the edit in a
`pure` function, write atomically, and keep a backup:

```xsh
pure set_option(text: Str, key: Str, value: Str) -> Str {
  var out = []
  var done = false
  for line in text.lines() {
    let words = line.replace("#", with: " ").fields()
    if ! done and ! words.is_empty() and words[0] == key {
      out += [f"{key} {value}"]
      done = true
    } else {
      out += [line]
    }
  }

  if ! done {
    out += [f"{key} {value}"]
  }

  out.join("\n") + "\n"
}

proc edit_config(file: Path, key: Str, value: Str) {
  let before = file.read_text()?
  let after = set_option(before, key, value)
  if after == before {
    print f"{file.name()}: {key} is already {value}"
    return
  }

  let backup = fp"{file}.bak"
  file.copy(to: backup, overwrite: true)
  file.write_atomic(after)
  print diff.unified(backup, file)?.text.trim()
}

let scratch = fs.tempdir()?
let config = fp"{scratch.host_path()?}/sshd_config"
config.write("Port 22\n#PermitRootLogin prohibit-password\nPasswordAuthentication yes\n")

edit_config(config, "PermitRootLogin", "no")
edit_config(config, "PermitRootLogin", "no")
```

```text
--- sshd_config.bak
+++ sshd_config
@@ -1,3 +1,3 @@
 Port 22
-#PermitRootLogin prohibit-password
+PermitRootLogin no
 PasswordAuthentication yes
sshd_config: PermitRootLogin is already no
```

`write_atomic` writes a sibling file and renames it into place, so readers see
either the old config or the new one. In production, validate the candidate
before the rename, for example with `run sshd -t -f $candidate`, then
`run systemctl reload sshd`.

### Timeouts and Retries

Every `run` form accepts `--timeout`. A timeout is a typed error you can match,
not exit status 124 that you have to remember:

```xsh
let slow = try run.capture --text --timeout=200ms sleep 5

match slow {
  Ok(out) => print f"finished: {out.status.ok}"
  Err(ProcessError.Timeout {..}) => print "timed out; process group killed"
  Err(error) => print f"failed: {error.message}"
}
```

```text
timed out; process group killed
```

`retry` re-runs a block after each delay until it succeeds, and returns the
last error when the delays run out. `on (...)` restricts retries to errors
worth retrying:

```xsh
error FetchError = Transient(message: Str) : Transient | Fatal(message: Str)

var attempts = 0

proc fetch_index() [error] -> Result[Str] {
  attempts += 1
  if attempts < 3 {
    return Err(FetchError.Transient(message: f"attempt {attempts}: connection reset"))
  }

  "index-v42"
}

let index = retry [100ms, 200ms, 400ms] on (is Transient) {
  fetch_index()?
}?

print f"{index} after {attempts} attempts"
```

```text
index-v42 after 3 attempts
```

A `Fatal` error would stop immediately without consuming the remaining delays.
Combine the two for the common case of a flaky network command:

```xsh
const url = "https://mirror.example.org/releases/index.json"
let body = retry [1s, 2s, 4s] {
  run.text --timeout=10s curl -fsS $url
}

match body {
  Ok(text) => print f"fetched {text.byte_len()} bytes"
  Err(error) => print f"mirror unavailable: {error.message}"
}
```

### A Real Program

Here is a preflight check that decides whether a host is ready for traffic. It
reads a JSON config, probes service ports and health endpoints in parallel,
checks disk headroom and required files, and prints either a table or a JSON
report. It exits 1 if anything failed, so it can gate a deploy.

```xsh
#!/usr/bin/env -S xsh --
# preflight: verify a host is ready to take traffic.
#
#   xsh preflight.xsh -- /etc/preflight.json
#   xsh preflight.xsh -- /etc/preflight.json --emit-json
#
# {"services": [{"name": "api", "port": 8080, "health": "http://127.0.0.1:8080/healthz"}],
#  "disk_threshold": 90, "required_files": ["/etc/app/app.conf"]}

type Service = {name: Str, port: Int, health: Str?}

type Config = {services: List[Service], disk_threshold: Int, required_files: List[Str]}

type Check = {check: Str, ok: Bool, detail: Str}

const pseudo_filesystems = ["devfs", "devtmpfs", "tmpfs", "overlay", "squashfs", "proc", "sysfs"]

proc load_config(file: Path) -> Result[Config] {
  ctx f"loading {file}" {
    # The return type supplies the schema: this is `.require(Config)`.
    json.read(file)?.require()?
  }
}

proc listening(svc: Service) -> Result[Check] {
  let owners = process.port(svc.port)? |> where .state == "LISTEN" |> map .command
  let detail = if ! owners.is_empty() { owners[0] } else { "nothing listening" }
  Check(check: f"port {svc.port} ({svc.name})", ok: ! owners.is_empty(), detail:)
}

proc healthy(svc: Service, url: Str) -> Check {
  let label = f"health {svc.name}"
  match net.request({method: "GET", url, timeout: 3s, fail_status: false}) {
    Ok(response) => Check(check: label, ok: response.status == 200, detail: f"HTTP {response.status}")
    Err(error) => Check(check: label, ok: false, detail: error.message)
  }
}

proc disks(threshold: Int) -> Result[List[Check]] {
  fs.mounts()?
    |> where { |m| ! m.readonly and m.blocks_1k > 0 and m.fstype not in pseudo_filesystems }
    |> map { |m|
      Check(
        check: f"disk {m.mounted_on}",
        ok: m.capacity_percent < threshold,
        detail: f"{m.capacity_percent}% used",
      )
    }
}

proc files(names: List[Str]) -> Result[List[Check]] {
  let checks: List[Check] = collect {
    for name in names {
      let present = fp"{name}".exists()?
      yield Check(check: f"file {name}", ok: present, detail: if present { "present" } else { "missing" })
    }
  }

  checks
}

cli main(config: Path, emit_json = false) {
  let cfg = load_config(config)?

  let probes = cfg.services |> par-map { |svc|
    var found = [listening(svc)?]
    if svc.health != null {
      found += [healthy(svc, svc.health)]
    }

    found
  }

  let service_checks = probes |> flat-map { |found| found }
  let all = service_checks + disks(cfg.disk_threshold)? + files(cfg.required_files)?
  let failed = all |> where { |c| ! c.ok } |> count()

  if emit_json {
    print (json.encode({ok: failed == 0, checks: all}, pretty: true)?)
  } else {
    for c in all {
      let mark = if c.ok { "ok  " } else { "FAIL" }
      print f"{mark} {c.check:<32} {c.detail}"
    }
  }

  if failed > 0 {
    eprint f"{failed} check(s) failed"
    exit 1
  }
}
```

```text
ok   port 5432 (db)                   postgres
FAIL port 8080 (api)                  nothing listening
FAIL health api                       Connection refused (os error 111)
ok   disk /                           61% used
ok   disk /var                        38% used
ok   file /etc/app/app.conf           present
2 check(s) failed
```

A config with a wrong type stops before any probe runs, and the `ctx` block
says which file was being loaded:

```text
$ xsh preflight.xsh -- bad.json
err: schema: schema check failed at disk_threshold: expected Int, found Str (ctx: loading bad.json)
executable: /usr/local/bin/xsh
...
```

Things worth noticing:

- The only text parsing is `json.read`, and its result is validated once
  against `Config`. Everything after that is typed.
- `svc.health` is `Str?`; after the `!= null` test the checker knows it is a
  `Str`, so `healthy(svc, svc.health)` type-checks.
- `healthy` turns network failures into data (`Check` with `ok: false`) instead
  of propagating them, because an unreachable endpoint is a finding, not a
  crash. `listening` propagates, because failing to read the socket table means
  the check itself is broken.
- No proc declares its effects. The checker infers them, so `healthy` is
  known to be the one that reaches the network, and a caller restricted to
  `[fs, process, error]` could not call it.
- `exit 1` exits with a chosen status without a traceback. Deferred cleanup
  still runs.

### Project Layout

The layout above is the canonical one:

```text
ops/
  xsht-config.ini
  bin/          entry scripts (cli main), one per tool
  lib/          modules shared by the scripts
  tests/        test-*.xsh files with test declarations
```

#### Modules

Any `.xsh` file is a module. `export` makes a declaration visible; everything
else stays private. A module that exports anything starts with a `##!` doc
block, and every export has a `##` doc comment; `xsht check` enforces both.
Modules may contain declarations and `let`/`const` values, but no top-level
commands or control flow, so importing one never has side effects.

`use NAME` binds exactly one namespace, and nothing is injected into your
scope:

```text
use sshd                  # lib/sshd.xsh, used as sshd.set_option(...)
use checks.disk           # lib/checks/disk.xsh, used as disk.*
use checks.disk as usage  # the same module, bound as usage.*
```

Resolution is file-relative first, then each directory in `XSH_MODULE_PATH`
(colon-separated), then each `module_path` root of the project's
`xsht-config.ini`. That has two consequences worth knowing:

- `xsh` reads `module_path`, and only that, from the nearest
  `xsht-config.ini` above the entry script, exactly as `xsht` does:
  `xsh bin/harden.xsh` finds `lib/sshd.xsh` with nothing else set, and so
  does `cd bin && xsh harden.xsh`. A script with no config above it has no
  project roots in either tool, and the current directory is never searched;
  install such a script's modules next to it or set `XSH_MODULE_PATH` in the
  unit file or wrapper that launches it.
- File-relative lookup wins. The example test is `tests/test-sshd.xsh`, so
  `use sshd` resolves to the project `lib/sshd.xsh`; naming it
  `tests/sshd.xsh` would make it import itself. Prefix test files with `test-`.

#### `xsht-config.ini`

The nearest `xsht-config.ini` governs each file. Relative paths resolve from
that config's directory. `include` adds roots to no-argument discovery, and
`test_roots` selects the files `xsht test` discovers; both come from the config
in the current directory. `xsh` reads only `module_path` for its entry script.
The project example above shows the common `module_path` and `test_roots`
settings. See [XSHT Tooling](XSHT.md) for the full configuration rules.

### Where to Go Next

- [`docs/SPEC.md`](SPEC.md) is the language contract. When this tour and the
  spec disagree, the spec wins.
- `xsht api` answers "what is the signature of...?" without leaving the
  terminal:

  ```bash
  xsht api summary                    # every module, method, and record
  xsht api module:fs                  # one module
  xsht api api:fs.files               # one function, with its contract
  xsht api method:Path.write_atomic   # a method
  xsht api language:core.defer        # a language feature
  xsht api search:timeout             # full-text search
  ```

- In the spec, §5 covers inference, `Any`, and narrowing; §12 covers signals,
  cancellation, and process groups; §13 covers pipelines; and §14 covers JSON.
- `xsht highlight script.xsh` emits lexer classifications as JSON Lines;
  `xsht grammar` prints the language grammar as EBNF or JSON.
- `xsht trace script.xsh` shows process argv and proc durations. Add
  `--syscalls` on Linux x86_64 or aarch64 for native syscall totals;
  `--trace-top-syscalls N` sets the number of syscall rows shown (default 8).
- `showcase/` holds larger programs (`px.xsh`, `ecount.xsh`, `run-retry.xsh`,
  `wait-for.xsh`, and more), and `core/` holds coreutils-style tools written
  in XSH.
