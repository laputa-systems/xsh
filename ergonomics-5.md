## 1. Signature-driven CLI entrypoints

```xsh
cli main(root: Path, jobs: Int = 4, verbose: Bool = false) [fs, process, error] {
  guard jobs > 0 else {
    return error.fail("jobs must be positive")
  }
  build(root:, jobs:, verbose:)?
}
```

Introduce the contextual entry declaration `cli main(parameters) [effects]? [-> return_type] { ... }`. It establishes a typed script entrypoint without a separately maintained argument-descriptor record. Retain ordinary proc return/effect rules and the annotation-free Result[Unit] default. It is a script entry declaration, not an exported/callable proc, and cannot coexist with an ordinary auto-invoked main. Permit one in the entry module, not imported modules or nested scopes.

Specify a small deterministic mapping: required non-defaulted scalar parameters are required positionals; defaulted parameters are named options; a final rest List[Str] or List[Path] collects remaining operands. Require required positionals before defaulted options. Use existing supported CLI scalar parsers. Defaulted Bool parameters are Boolean options, accepting bare --verbose as true and explicit --verbose=false through existing Boolean parsing. Other options accept --jobs 4 or --jobs=4. Snake_case maps to kebab-case. Defaulted List parameters use existing repeated-option semantics and concrete supported scalar element types; preserve whether explicit occurrences replace or extend defaults according to that contract. Preserve --, unknown-option checks, duplicate-scalar rejection, and exact operand order.

Defaults must be validated preparation-time data, not arbitrary code. Derive one existing CLI schema/checked binding plan from the signature; reuse parsing, help, conversions, usage errors, and exit codes rather than writing another argv parser. Reserve -h/--help and conflicting parameter spellings consistently. Generate usage from parameter names/types/defaults and applicable existing declaration/module documentation.

After ordinary parsing/static validation, parse and validate arguments before executable entry/module initializers or the body run, so help and invalid input cannot trigger script side effects. Reading signature defaults must not execute initializer code. Do not add filesystem existence checks implicitly. Apply normal effects to the body and preserve entry exit/status behavior.

Keep cli.parse/parse_full/commands for explicit advanced policies: aliases, subcommands, provenance, custom constraints, and dynamic schemas. Do not guess short aliases, derive subcommands, add decorators, or infer CLI policy from callers. Ordinary proc main semantics remain unchanged.

Autofix only simple literal-schema entrypoints with exactly matching bindings, defaults, help/usage behavior, and initialization order. Retain sophisticated parsers. Test help/invalid argv with a marker proving the body and executable initializers did not run.

## 2. Bytes as a native stdin-redirection source

```xsh
let echoed = run.bytes cat < b"hello\n" ?
let digest_output = run.text shasum -a 256 < (payload) ?
```

Extend the existing input-redirection operator to accept Bytes as in-memory input. Path and existing path-like Str operands remain file redirections; a Str does not silently become content. Arbitrary text must cross the existing explicit UTF-8 encoding boundary to Bytes. Require Result operands to be handled explicitly. Add no here-string operator or stdin helper API.

Evaluate the input expression once at its ordinary redirection position. Send exactly those bytes, including NUL, with no newline insertion, decoding, trimming, or temporary file. Empty Bytes means an explicitly closed empty input, not inherited stdin. Limit this extension to stdin, retaining existing file/fd behavior elsewhere; reject Bytes output targets, competing stdin sources, and incompatible pipeline wiring explicitly.

Use the existing process I/O ownership/polling machinery to feed input while draining captured output/error. Never synchronously fill a pipe before beginning output reads. Preserve timeouts, signal checkpoints, process groups, capture limits, descriptor cleanup, and child reaping. Early child closure of stdin must not turn a successful bounded consumer into a spurious broken-pipe failure; genuine process/setup/I/O errors retain existing classification.

Support existing immediate/capture/stream invocation, supported owned spawn, the existing Command construction/execution routes, and the first segment of an otherwise valid byte pipeline. Extend the existing stdin payload representation and accepted Bytes input only as needed; do not introduce command literals, a new plan API, or new run-target behavior. Retain the payload until it is sent or the operation terminates; do not add a public async runtime, unbounded queued copying, stream-to-Bytes collection, or body data in traces. For spawn, the existing process owner must keep driving/owning its input delivery through wait/cancel/cleanup.

Use Rust integration fixtures for a large echoing child that would deadlock a write-then-read implementation, zero bytes, arbitrary bytes, early-close consumers, timeout, cancellation, and spawn cleanup. Do not autofix temporary-file handoffs unless file lifetime, seekability, reuse, metadata, error timing, and observability are all equivalent; most are not safe automatic rewrites.

## 3. Lexical error-context blocks with `ctx`

```xsh
# Before
let source = read_source(pkg).context("build", f"building $name")?
compile(source).context("build", f"building $name")?
install().context("build", f"building $name")?

# After
ctx f"building $name" {
  let source = read_source(pkg)?
  compile(source)?
  install()?
}
```

Add contextual `ctx message_expression { ... }`: one Str description attached to failures propagating out of this lexical region. It is not a catch, retry, rollback, new error type, or Result constructor. Preserve normal statement/value block classification and lexical return/loop/propagation destinations.

Evaluate the description once on entry. If that evaluation fails, only already enclosing contexts apply. Checking the description/body must preserve their ordinary effects; entering ctx itself performs no host operation. On successful execution, emit no output and do not eagerly format operand/error diagnostics. When a propagated failure crosses the boundary, append one frame through the existing ErrorContext machinery using a stable context kind, the description, and source span. Preserve nominal family/variant, payload, existing contexts, and the primary error.

Apply context to outbound propagated errors and cleanup failures under existing unwinding rules. Finish the region's defers before finalizing its failure context. An inner handled failure that never crosses the boundary is not annotated by that boundary. Direct Err values returned/stored as data are not automatically inspected or changed; explicit Result.context remains available for those cases. Do not swallow evaluator failures, convert cancellation/abort into recoverable errors, or make diagnostics change control flow.

Nested regions attach inner-to-outer context exactly once as failures cross them. Reusing a handled error must not mutate another alias's diagnostic chain. Check label/body effects normally. Recognize `ctx` only in this block-introducer shape. Ordinary bindings and parameters named ctx, ctx.field access, callable names, and imported modules remain legal. In particular, existing TestContext parameters named ctx must not require renaming. Add no executable `context` alias and do not rename `Result.context`.

The example intentionally adds context to every outbound body failure. A migration moving repeated Result.context calls to a region is safe only when its set of annotated failures and evaluation timing also match; otherwise present it as a deliberate reviewed improvement, not an equivalence-preserving autofix. Test these boundaries explicitly.

## 4. Value-producing cwd/environment scopes and typed overlays

```xsh
# Before
var revision = ""
cd $repo {
  revision = run.text git rev-parse HEAD ?
}

# After
let revision = cd (repo) {
  run.text git rev-parse HEAD ?
}?

let output = env ({CC: "clang", BUILD_MODE: "release"}) {
  run.text ./configure ?
}?
```

Complete cd/env as value-producing scoped expressions, while preserving their statement forms. Successful entry/body/restore yields Result[T] for the consumed body value T; use normal compatible error types. A Result-valued tail remains nested unless explicitly unwrapped. Do not reinterpret a plain statement-position run as a capture or make false predicate tails assert in genuine value contexts.

These scopes manage context, not local Result capture: body `?` retains its enclosing try/retry/function destination, with restoration during exit. Entry/restore failures follow the expression's Result contract; document the distinction and use try when local capture of the whole operation is intended. Preserve normal statement propagation, return/break/continue, cleanup ordering, and primary/secondary failure precedence. Defers registered inside execute under the scoped context before restoration.

Add the explicit `env (record_or_map_expression) { ... }` overlay form. Known records and string-keyed maps with supported value types use the existing environment scalar conversion and name/NUL/encoding checks, applied once before entering the body. A null field is not an implicit unset or omission; follow existing accepted values and reject unsupported input. Retain unmodified inherited native environment bytes. Do not mutate the embedding host process's global cwd/environment or introduce shared cross-evaluator context.

Use ordinary record literals/spreads to construct overlays. Migrate and remove the special expression-assignment double-block form `env { NAME = expression } { ... }`, with narrow migration diagnostics. Keep ordinary command-word env NAME=value scopes for their existing command-syntax purpose. Parenthesized input makes overlay-record and body braces unambiguous.

Autofix outer-var scoped-result scaffolding only when the temporary is fresh, does not escape, and mutation/result/error/cleanup timing is equivalent. Reject escaping live producers or handles where current lifetime rules cannot preserve scoped context; do not claim that a deferred stream continues executing in a cwd/env that has already been restored. Test successful, failed, nested, and early-exit restoration.

## 5. Lossless Path interpolation at filesystem and process boundaries

```xsh
let target = fp"${root}/${relative_name}"
run installer "--target=$target"
```

Make these ordinary idioms preserve native Path bytes, including non-UTF-8 Unix names. In formatted Path literals, append Path interpolands as native path bytes; append text/displayable non-Path interpolands through their established explicit conversion encoded as UTF-8. In command words, including compound words such as --target=$target, retain interpolated Path bytes rather than going through a lossy display string.

Keep f-strings and print as human-facing text: a Path explicitly converted to display text remains text, not a recoverable lossless path. Preserve argument boundaries, @ splicing, no word splitting, and NUL rejection. Do not introduce implicit Bytes-to-Path conversion, filesystem access, normalization, tilde expansion, globbing, canonicalization, or encoding guesses.

This is byte-preserving concatenation, not semantic path joining or confinement. A slash or .. inside an interpoland stays present. Do not reinterpret existing Path membership or claim that interpolation establishes a secure root; rooted filesystem APIs remain the confinement boundary.

Use byte-oriented construction paths in lowering/runtime and preserve the existing readable diagnostic escaping. Do not decode and re-encode native Path operands. Ensure stored command plans, direct argv, redirection targets, and Path literals use the same conversion rule. Interpolated expressions still execute once in source order.

Treat changed behavior for previously lossy paths as an intentional correctness change, not a universally semantics-preserving rewrite. Only remove explicit .display() where the authoring boundary intends native Path data; leave it in human text, serialized text, matching patterns, and deliberate display conversions. Test invalid UTF-8 through real macOS filenames and a byte-level argv helper, plus interpolation with separators, spaces, quotes, empty pieces, and rejected NUL.

## 6. Typed map keys instead of serialization into strings

```xsh
# Before
var by_pid: Map[ProcessEntry] = {}
by_pid = by_pid.set(f"$pid", entry)
let owner = by_pid.get(f"$pid")?

# After
var by_pid: Map[Int, ProcessEntry] = {}
by_pid[pid] = entry
let owner = by_pid.get(pid)?
```

Generalize the builtin to Map[K,V]. Preserve Map[V] as its established shorthand for Map[Str,V], not a runtime compatibility type. Carry K through literals, membership, key iteration, indexed assignment, lookup/default checking, and keys(); carry V through values() and entry iteration. All public map operations must have consistent signatures; do not leave type-erasing adapters.

Allow homogeneous keys from Str, Int, UInt, Bool, Bytes, Path, and Duration with their resolved aliases. Define canonical ordering/equality: numeric order for numeric/duration keys, false before true, existing string order, and raw-byte order for Bytes/Path. Preserve runtime Int/UInt representation invariants while enforcing declared key types. Exclude Any, Float, collections, callable/handle values, and arbitrary user records; add no hashing/ordering trait or coercion protocol.

Use existing ordered-map machinery with an appropriate compact scalar-key representation. Preserve deterministic order, value semantics, source evaluation order, and efficient borrowed lookup. Do not stringify keys internally. Computed-key map literals infer compatible K/V or take their types from context; mixed concrete key domains require an error, not an Any key. Bare/quoted field labels remain string keys, and ordinary record-literal classification is unchanged. Extend the existing computed-key map path rather than adding a second literal syntax. Map-entry iteration's record has key: K and value: V.

JSON objects and environment overlays remain string-keyed. Reject non-string map serialization at those boundaries instead of silently formatting keys, inventing tagged JSON, or hiding collisions. Explicit application-owned conversion stays available through existing comprehensions. Leave the string-set representation alone.

Numeric keys change ordering relative to decimal string keys, and Path keys differ from display-text identity. Consequently broad string-key migration is not a safe autofix. Migrate reviewed domain maps only where strings are merely internal encoding, proving no sentinel/prefix/format/serialization/order dependency. Keep externally defined textual keys textual. Test equality, ordering, aliases, empty maps, absent-versus-null values, updates, and boundary rejection for each supported domain.

## 7. Dimensionally correct Duration arithmetic

```xsh
# Before
let base_ms = 250
let pause = time.millis(base_ms * attempt)

# After
let pause = 250ms * attempt
let budget = connect_timeout + transfer_timeout
pause <= budget
```

Extend existing operators, not the grammar: Duration +/- Duration -> Duration; Duration * Int and Int * Duration -> Duration; Duration / positive Int -> Duration at millisecond resolution; Duration / positive Duration -> Int interval count; and ordinary ordering between Duration values. Retain equality. Add no implicit integer/float conversion, Float scaling, timestamp arithmetic, general units framework, or modulo in this feature.

Use Duration's existing non-negative representable millisecond domain. Addition/scaling overflow and subtraction below zero are checked failures; reject negative multipliers, zero/negative divisors, and overflowing interval counts. Integer division truncates consistently toward zero, including a possible zero-duration result. Reuse canonical typed runtime-error machinery and source attribution rather than host panics or silently wrapping values.

Evaluate operands once, left to right. Share the same checked rules across constant preparation, runtime, inference, assertions, and optimization. Do not implicitly sleep, read a clock, or add the time effect to pure duration arithmetic.

Keep time.millis/time.seconds where they intentionally convert runtime numbers with their existing clamping/saturation behavior. Their special boundary behavior is not equivalent to checked arithmetic. Autofix conversions only with proved range/sign/overflow equivalence; ordinary nonnegative bounded cases can become scalar multiplication by 1ms or 1s. Do not replace measurements in nanoseconds or reinterpret bare integers as durations.

Test exact unit preservation, zero, quantization, overflow/underflow, constants, comparison diagnostics, retry delays, and process/network timeout inputs without requiring real sleeps.

## 8. Statically resolved function references in stream stages

```xsh
# Before
let names = lines |> map { |line| normalize_name(line) }
let valid = rows |> where { |row| is_valid(row) }

# After
let names = lines |> map(normalize_name)
let valid = rows |> where(is_valid)
```

Allow a statically resolved named unary callable as the transformation/predicate argument of map, where, flat-map, each, tee, sort-by, group-by, unique-by, any, and all. Reuse the established named-argument stage syntax and each stage's own body contract. Do not add stages or change their streaming/materialization/parallelism policy. Preserve non-reference expression/projection forms. Keep other multi-argument/accumulator stages outside this feature.

Require a unique checked function/proc/standard-call signature for a normal one-item call, including any already supported default parameters. Qualified imported names are allowed. Reject erased Any/Pure/Proc dynamic dispatch, arbitrary callable-producing expressions, ambiguous overloads, bound-method partial application, and placeholder-lambda invention in this shorthand. Existing explicit blocks remain the general form.

Lower the reference to the same checked per-item call as its transparent wrapper block, preserving parameter conversions, defaults evaluated per actual call, body effects, trace spans, and output types. No closure allocation, synthetic public function, global lookup per element, or general function-type system is needed. The enclosing stage still controls pull timing and cleanup.

Returning Bool from a predicate remains data. A Result-returning map remains a stream/list of Results; it does not acquire implicit per-item ?. Side-effect stage auto-propagation follows its existing Unit-consuming block contract. A wrapper ending in f(item)? is not equivalent to a reference to f and must remain explicit when it changes error policy.

Autofix only single-call transparent wrappers with the exact bound item, stable symbol resolution, and no additional arguments, comments, cleanup, propagation operator, or control flow. Preserve useful explicit blocks. Test empty sources, defaults, imported overload resolution, effects, Result mapping, short-circuit terminals, and late producer failures.

## 9. Indentation-aware multiline block strings

```xsh
# Before
let unit = f"[service]\nname=$name\nexec=$executable"

# After
let unit = f"""
  [service]
  name=$name
  exec=$executable
  """
```

Define block-string layout for supported multiline Str literals whose opening triple delimiter is immediately followed by a line break and whose closing triple delimiter is alone after indentation on its line. Use the closing delimiter's exact space/tab prefix as the margin. Remove the structural opening and closing line breaks (counting a shared break in an empty block only once) and that prefix from each nonblank content line. The example has no implicit trailing newline; an extra blank content line expresses one. Preserve internal line endings and remaining whitespace exactly. On whitespace-only content lines, remove only the longest matching initial part of the margin, not arbitrary whitespace.

Reject a nonblank source line that does not contain the required prefix. Do not compute a changing minimum indentation from content or normalize tabs into spaces. Apply layout processing before string escape decoding and before interpolation; multiline text inserted by an expression is not reindented, trimmed, or rescanned. Preserve evaluation count and exact source maps into original literal text/interpolation expressions. Use interpolation-aware scanning: layout processing must not rewrite code or nested literals inside a ${...} interpolation.

Apply the same block layout to ordinary, raw, and formatted multiline Str literals; rawness still controls escape/interpolation rules, not source indentation. Do not change Bytes, Path, glob, or prepared-regex literal layout in this feature. Triple literals not meeting the block layout shape retain their existing exact behavior. Add no dedent method, new prefix, marker character, interpolation DSL, or implicit trailing newline.

This intentionally changes one existing syntactic shape and requires a byte-exact migration. Inventory affected maintained literals and capture their old literal pieces/interpolation boundaries before modifying behavior. Preserve existing values by rewriting legacy exact fixtures to an equivalent non-block form/escaped literals/explicit concatenation where necessary. Never silently dedent embedded test programs, snapshots, regular expressions, or generated files. No executable legacy mode or dual runtime parser.

Autofix escaped newline concatenations into block strings only when decoded content, interpolation order, line endings, and trailing-newline behavior are identical. The formatter may move the margin and content together but must not change the value. Test empty strings, blank lines, indentation errors, tabs, CRLF, explicit final newlines, raw backslashes, interpolation with newlines, and diagnostic span fidelity.

