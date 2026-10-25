# Coding Agent Campaign: Unify XSH's Serialization Architecture Around Deser

## Mission

Implement a complete, native structured-data serialization subsystem in:

**Primary upstream:** https://github.com/mitsuhiko/deser

**Architectural inspiration:** https://github.com/colinhacks/reylo

Replace the existing `miniserde`-based JSON implementation with Deser's serialization framework and integrate **all 17 currently supported Deser format crates** into XSH.

The goal is not merely to add formats. It is to establish a coherent, efficient, type-aware serialization architecture for a systems programming and glue language.

The final architecture must:

1. Use Deser as the common serialization/deserialization framework.
2. Decode directly into XSH-owned runtime values without an obligatory intermediate `deser_value::Value` tree.
3. Encode XSH values through a common adapter without unnecessarily materializing intermediate trees.
4. Integrate naturally with XSH's existing type declarations, inference, `.require(T)`, nominal types, and error propagation.
5. Preserve the existing public JSON API and its correctness guarantees.
6. Introduce a cohesive public API for every supported Deser format.
7. Provide proper support for text formats, binary formats, record-oriented formats, document sequences, and specialized formats.
8. Respect XSH's compile-time checking, effect system, resource ownership, and performance constraints.
9. Remove superseded JSON machinery and the `miniserde` dependency after parity is proven.
10. Remain maintainable, with no redundant type system, generated schema files, general-purpose codec plugin framework, or speculative abstraction layers.

**This is an implementation campaign. Do the work, run the tests, consolidate the result, and commit completed changes. Do not stop at a design document.**

### Repository boundaries

All production implementation belongs in `xsh`.

Do not modify Deser upstream or other repositories.

Use published, pinned Deser versions where available. The upstream source currently advertises version `0.10.1`; verify that the required versions have actually been published before selecting dependency pins.

The repository's default branch is currently `master`, not `main`. Fetch current state and confirm the target branch before making changes.

Do not overwrite concurrent work, force-push, or discard unrelated changes.

---

## 1. Read the architecture before editing

Read `AGENTS.md` in full and follow its requirements.

Inspect:

- `Cargo.toml` and `Cargo.lock`
- `docs/SPEC.md`, especially sections 4, 5, 13, 14, and 15
- `docs/ARCHITECTURE.md`
- `docs/DESIGN.md`
- `docs/TESTING.md`
- `docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md`
- `docs/user-tour.md`
- `src/runtime/value.rs`
- `src/runtime/eval/type_view.rs`
- `src/runtime/eval/json.rs`
- `src/modules/json.rs`
- `src/modules/ini.rs`
- `src/modules/mod.rs`
- `src/runtime/eval/lowered_run.rs`
- `src/runtime/eval/lowered_run/module_codecs.rs`
- `src/modules/signature.rs`
- `crates/xsh-registry/src/signature/modules.rs`
- `crates/xsh-registry/src/runtime_op.rs`
- `crates/xsh-registry/src/errors.rs`
- Existing JSON and INI unit/integration tests
- Existing JSON Lines and stream tests
- The native API golden fixtures
- Representative JSON workloads from `bench/consolidation/` and `bench/stdlib-port/`

XSH already has substantial runtime and serialization infrastructure.

In particular, `src/runtime/eval/json.rs` implements:

- `JsonInput`
- `JsonNode`
- `JsonSink`
- Specialized `Value` and `LoweredValue` traversal
- Compact JSON output
- Pretty JSON output
- Record/map serialization
- Wire enum handling
- Error handling for unsupported values

Much of this is intentional performance engineering.

Do not delete it blindly.

Understand what is generic value traversal, what is JSON-specific behavior, and what is necessary for optimized lowered-value execution.

Likewise, understand XSH's existing `Value`, `LoweredValue`, `RecordMap`, `MapKey`, `FloatValue`, nominal values, and schema-validation mechanisms.

**Do not redesign XSH's runtime value model to accommodate Deser. Deser is the codec engine; XSH remains the owner of its data model.**

---

## 2. Study Deser and its complete format family

Inspect the current upstream implementation, not only README examples.

### Core

Study:

- `deser-core/src/event.rs`
- `deser-core/src/de/mod.rs`
- `deser-core/src/ser/mod.rs`
- `deser-core/src/de/driver.rs`
- `deser-core/src/ser/driver.rs`
- `deser-core/src/de/limits.rs`
- `deser-core/src/ext/`
- `deser-value/src/`
- `deser-path/`
- `deser-location/`
- `deser-transcode/`

Study Deser's treatment of:

- Atoms and implicit lexical values.
- `Deserialize` sinks.
- `Serialize` emitters.
- Event-driven iteration.
- `Send` and `Sync` requirements.
- Recursive data.
- Borrowed versus owned values.
- Exact numeric representation.
- Unicode and byte buffers.
- Container shape and map ordering.
- Unknown extension values.
- Duplicate-key policies.
- Resource limits.
- Streaming and cancellation.
- Input source locations.
- Structured error categories.

Read `LIMITATIONS.md`, `SERDE.md`, `benchmark/README.md`, and `compile-times/README.md`.

In particular, Deser uses dynamic dispatch and an arena-based, nonrecursive model. Its benchmarks are promising, but this design has nontrivial fixed binary-size cost.

Do not confuse a compelling architecture with proven superiority on XSH's workloads.

### All supported formats

Integrate the following current Deser format crates.

| Crate | Format | Expected use |
|---|---|---|
| `deser-json` | JSON | Existing API replacement |
| `deser-jsonc` | JSON with comments | Tooling configuration |
| `deser-json5` | JSON5 | Permissive configuration |
| `deser-hj` | Hjson | Human-readable configuration |
| `deser-yaml` | YAML | Kubernetes, CI, infrastructure configuration |
| `deser-toml` | TOML | Application and build configuration |
| `deser-ini` | INI and Git config | Legacy configuration |
| `deser-cbor` | CBOR | Binary protocols |
| `deser-msgpack` | MessagePack | Binary structured data |
| `deser-xml` | XML | Documents and structured interfaces |
| `deser-plist` | Apple property lists | XML, binary, OpenStep |
| `deser-php` | PHP serialization | Interoperability |
| `deser-pickle` | Python pickle | Data interchange |
| `deser-csv` | CSV/TSV | Tabular files and streams |
| `deser-urlencoded` | URL-encoded forms | Query strings and form data |
| `deser-env` | Environment-variable mapping | Typed configuration from supplied variables |
| `deser-debug` | Debug formatting | Diagnostic rendering |

All 17 must receive an explicit integration disposition and tests.

However, these are not identical interfaces.

`deser-debug` is output-only.

`deser-env` operates on environment-variable mappings, not ordinary serialized documents.

CSV is a sequence of records.

XML has root-element, attribute, namespace, and mixed-content semantics.

JSONC, JSON5, and Hjson can be decoded with their extended syntax, but their serializers primarily emit JSON-compatible text. Do not claim they preserve input comments or formatting.

PHP and pickle have object/class/reference metadata that ordinary XSH values may not represent.

**Supporting a codec does not imply lossless representation of every possible value that codec can express. Explicitly diagnose unsupported representations rather than silently discarding important metadata.**

---

## 3. Architectural objective: direct mapping to XSH values

The intended architecture is:

```text
              XSH type declarations
                       │
                       ▼
             Existing XSH type system
                       │
                 .require(T)
                       │
                       ▼
        ┌──────────────────────────┐
        │  XSH native Value model  │
        │                          │
        │  Record / Map / List     │
        │  Str / Int / UInt        │
        │  Float / Bool / Null     │
        │  Bytes / supported enums │
        └──────────────────────────┘
                 ▲          │
                 │          ▼
        ┌──────────────────────────┐
        │ Shared XSH/Deser adapter │
        │ Deserialize + Serialize  │
        └──────────────────────────┘
                 ▲          │
                 │          ▼
        ┌──────────────────────────┐
        │ Deser format codecs      │
        │ JSON YAML TOML CBOR ...  │
        └──────────────────────────┘
```

### Nonnegotiable: no second runtime type system

Do not replace XSH's `Value` with `deser_value::Value`.

Do not store Deser values as opaque runtime handles.

Do not require user-defined XSH types to be recreated as Rust structs.

Do not generate Rust types for each XSH schema.

Do not introduce a JSON Schema or Zod-like runtime schema language.

Do not make scripts duplicate their record definitions in configuration files.

Deser's general-purpose dynamic `Value` is acceptable in a temporary prototype, reference test, or narrowly justified fallback, but not as the default production representation.

### Decoding

Implement an XSH-owned Deser deserialization adapter.

Conceptually:

```rust
struct XshDecodedValue(Value);
```

Implement the necessary Deser deserialization interface to construct XSH's existing values from events.

The decoder should create:

- XSH scalars directly.
- `Value::List` for sequences.
- `Value::Record` for appropriate string-keyed objects.
- `Value::Map` for mappings whose keys are representable by XSH.
- `Value::Bytes` for native binary values.
- Appropriate existing runtime representations for other supported values.

Avoid building a complete `deser_value::Value` tree and then walking it again.

Use bounded intermediate state only where required for constructing containers, dealing with map keys, or enforcing correct semantics.

The deserializer must handle nested values without unbounded stack recursion.

Respect XSH's record-shape interning and memory ownership.

Do not accidentally intern attacker-controlled strings without the existing appropriate safeguards.

### Encoding

Implement a borrowed XSH-value serialization adapter.

Conceptually:

```rust
struct XshEncodeView<'a> {
    value: &'a Value,
}
```

Prefer direct event production from XSH values.

Also investigate supporting `LoweredValue` directly. XSH's optimized execution representation must not needlessly materialize full runtime `Value` trees just to encode JSON or YAML.

**Important:** Deser's `Serialize` interface has `Sync` requirements, and its `Deserialize` interface has `Send` requirements.

Confirm that the proposed XSH adapters satisfy those requirements honestly.

Do not add unsound `unsafe impl Send` or `unsafe impl Sync` declarations.

If XSH's full runtime `Value` is incompatible because it contains capabilities, script streams, or other nonserializable resources, implement a constrained serialization view that contains only supported value kinds.

Do not copy arbitrary resource-bearing values into an ostensibly serializable representation.

If a direct borrowing strategy is impossible, isolate and benchmark the smallest correct projection.

### Keep serialization independent from runtime ownership

The adapter may inspect values, but it must never:

- Execute scripts.
- Resume streams unexpectedly.
- Open files.
- Invoke commands.
- Acquire privileged resources.
- Mutate environment variables.
- Change the current directory.
- Materialize opaque capabilities as serialized identifiers.

Nonserializable values must fail clearly.

---

## 4. Apply the useful idea from Reylo

Read:

https://github.com/colinhacks/reylo

Reylo gets types from the underlying SQL and database metadata rather than maintaining duplicate TypeScript types or generated schemas.

For XSH, the analogous source of truth is **the XSH type declaration**.

Consider:

```xsh
type Service = {
  name: Str,
  port: Int,
  enabled: Bool,
}
```

Users should be able to write:

```xsh
let service = yaml.read(p"service.yaml")?.require(Service)?
```

Or:

```xsh
let service = codec.read("toml", p"service.toml")?.require(Service)?
```

Without writing another schema.

### Preserve `.require(T)` semantics

Ordinary self-describing formats should decode directly into XSH runtime values, followed by XSH's existing validation.

Do not use arbitrary Deser coercions to make invalid JSON or YAML fit an XSH schema.

For example, a JSON string `"5432"` must not silently become an XSH `Int` simply because the destination field expects `Int`.

Existing nominal types, validated types, unions, defaults, and type errors must continue behaving according to `docs/SPEC.md`.

### Type-directed decoding for lexical formats

There is a significant opportunity here.

CSV, INI, URL-encoded forms, and environment variables contain textual fields whose meaning is defined by the target type.

For example:

```csv
name,port,enabled
api,8080,true
```

A decoder that first produces only strings cannot necessarily recover the intended integers and booleans using XSH's existing strict `.require(T)` validation.

Support type-directed decoding using XSH's existing type information.

The desired ergonomic form is:

```xsh
type Service = {
  name: Str,
  port: Int,
  enabled: Bool,
}

let services =
  codec.read("csv", p"services.csv")?.require(List[Service])?
```

Investigate how the compiler represents `.require(T)` and how much of its target schema can be passed into a Deser sink.

Prefer recognizing a statically known decode-then-require boundary and allowing the decoder to consume that schema during parsing.

Do not introduce a new XSH syntax merely to support type-directed codec calls.

Do not duplicate XSH's schema validator inside the codec implementation.

Preserve the existing visible error boundary and its effects.

**This optimization must be semantics-preserving.** It may use type-directed interpretation for formats whose lexical rules require it, but must not introduce coercions forbidden by the language for self-describing formats.

If direct fusion requires extensive compiler changes, implement it as a narrowly scoped subsequent lane with dedicated differential tests. Do not destabilize normal `.require(T)` operations globally.

No compile-time reads of arbitrary configuration files. Reylo's external source analysis is inspiration for avoiding duplicate schemas, not permission to add unexpected filesystem access during compilation.

---

## 5. Public API design

Design a small, cohesive API rather than introducing 17 unrelated modules with repetitive plumbing.

Preserve existing:

```xsh
json.decode(...)
json.encode(...)
json.read(...)
json.write(...)
json.get(...)
json.set(...)
json.remove(...)
json.encode_lines(...)
json.write_lines(...)
```

Preserve their existing overloads, effects, errors, and observable behavior.

### First-class common modules

Add the following particularly common formats with ergonomic APIs:

```xsh
yaml.decode(text)
yaml.encode(value)
yaml.read(path)
yaml.write(path, value)

toml.decode(text)
toml.encode(value)
toml.read(path)
toml.write(path, value)
```

Integrate the existing `ini` module with Deser while preserving its documented behavior.

Do not replace existing INI parsing semantics without compatibility tests.

### Generic `codec` module

Introduce a concise module for the full format family.

Suggested API:

```xsh
codec.decode(format: Str, text: Str)
codec.decode_bytes(format: Str, data: Bytes)

codec.encode(format: Str, value: Any)
codec.encode_bytes(format: Str, value: Any)

codec.read(format: Str, path: Path)
codec.write(format: Str, path: Path, value: Any, overwrite: Bool = false)

codec.decode_all(format: Str, text: Str)
codec.decode_all_bytes(format: Str, data: Bytes)

codec.formats()
```

Specify proper `Result` return types through the registry.

Where a function's behavior is partial, the result must be fallible.

Text encoding returns `Str`; binary encoding returns `Bytes`.

Never silently convert arbitrary binary output to UTF-8.

Never accept a text-only format in a binary-only operation without an explicitly documented contract.

The generic module should accept stable format identifiers, for example:

```text
json
jsonc
json5
hjson
yaml
toml
ini
cbor
msgpack
xml
plist
plist-binary
plist-openstep
php
pickle
csv
tsv
urlencoded
```

Aliases and dialects should be explicitly documented.

Do not derive format selection solely from filename extensions.

### Specialized formats

Expose Deser's remaining capabilities through interfaces that reflect their real semantics.

For `deser-env`, use caller-supplied environment mappings:

```xsh
codec.env_decode(vars, prefix: "APP_")
codec.env_encode(value, prefix: "APP_")
```

The precise signatures should follow XSH's actual type system.

Do not call `deser_env::from_env` from an ostensibly pure operation. XSH already controls environment effects and needs reproducible behavior in tests and isolated execution.

Use Deser's `from_vars`-style API over explicitly supplied values.

For `deser-debug`, provide a rendering function such as:

```xsh
codec.debug(value)
```

This is output-only.

Do not invent `debug.decode`.

### Format capabilities

Maintain a small static capability table specifying:

- Format identifier.
- Text or binary input.
- Text or binary output.
- Single-value support.
- Multi-document/record support.
- Type-directed lexical interpretation.
- Relevant dialect options.
- Relevant extension and metadata limitations.

Use this table to drive dispatch, format discovery, and diagnostics.

Do not build a plugin framework.

Avoid adding complicated generic format-registration abstractions. A static match or similarly straightforward dispatch is appropriate.

---

## 6. Streaming support

Deser's streaming facilities are a significant advantage for XSH.

Preserve XSH's existing JSON Lines functionality.

Support the appropriate stream semantics for:

- JSON Lines.
- YAML document streams.
- CBOR sequences.
- Consecutive MessagePack values.
- CSV/TSV records.
- Other upstream-supported multi-value formats where meaningful.

Prefer lazy XSH streams where the upstream codec supports incremental processing.

Respect XSH's existing `Stream` model and backpressure/cancellation semantics.

Do not create a second streaming runtime.

Important implementation constraints:

- No leaked input readers.
- No unbounded buffering.
- No accidental stream consumption when constructing a lazy stream.
- Cancellation releases resources.
- Errors appear at the correct stream item.
- JSON Lines recovery semantics remain compatible.
- YAML may buffer one document where required by its parser.
- Input budgets apply across the stream, not just independently to each item when that would permit unlimited total consumption.

Avoid building a general asynchronous codec subsystem. XSH already has its own synchronous and host-managed streaming models.

Use `deser-tokio` only if the actual implementation demonstrates that it is required. Do not introduce Tokio merely because an optional upstream integration exists.

---

## 7. Format-specific semantic policies

A generic codec adapter must not flatten meaningful differences between data formats.

### JSON

Preserve:

- XSH numeric interpretation.
- Existing key ordering.
- Compact output.
- Pretty output.
- JSON Lines behavior.
- String escaping.
- Existing error classes.
- Wire enum serialization.
- Rejection of nonfinite floats.
- Existing restrictions on non-string map keys.
- Existing special serialization behavior for supported runtime values.

Deser may use different defaults. Set explicit options rather than inheriting behavior accidentally.

### JSONC / JSON5 / Hjson

Use their respective decoders.

Document that encoding may produce ordinary JSON-compatible syntax rather than reconstructing comments or the original formatting.

For JSON5 and Hjson, define how nonfinite values and implicit lexical scalars behave.

Do not silently accept a nonfinite value into XSH if it cannot be represented according to the selected policy.

### YAML

Default to YAML 1.2.

Support:

- Anchors and aliases.
- Quoted and plain scalars.
- Nested mappings and sequences.
- Multiple documents.
- Explicit scalar tags where representable.
- Binary scalars.
- Strict duplicate-key handling.
- Configurable, bounded alias expansion.

Prevent alias bombs and unreasonable memory growth.

Do not turn arbitrary custom YAML tags into executable behavior or forged nominal XSH values.

### TOML

Support TOML 1.1 according to upstream behavior.

Recognize arrays of tables and native date/time forms.

Preserve meaningful types where XSH can represent them.

When a date/time or extension value cannot be represented exactly, define an explicit fallback or report a representability error. Avoid surprising lossy conversions.

### INI

Inspect the current XSH INI dialect.

Compare it against Deser's default, Python, and Git-config dialects.

Preserve existing public behavior. Expose additional dialects explicitly if useful.

Avoid a permanent pair of unrelated INI implementations unless there is a demonstrated compatibility reason.

### CBOR and MessagePack

Support native byte strings as XSH `Bytes`.

Support primitive scalar keys and mappings that XSH can represent.

Handle numeric overflow explicitly.

Reject unsupported complex keys, arbitrary extension values, or cyclic references rather than silently converting them into unrelated strings or records.

Support canonical encoding where upstream provides it.

Canonical encoding matters for deterministic artifacts and cryptographic signing; don't claim canonical output merely because XSH sorts its records.

### XML

Preserve Deser's handling of:

- Root element names.
- Attributes.
- Text content.
- Repeated children.
- Namespaces.
- Mixed content.

The `deser-xml` project currently identifies itself as an early version.

Do not paper over XML's structure by stringifying arbitrary nodes.

Provide explicit root-name configuration for encoding when required.

Where decoded XML cannot be represented without losing important structure, reject it or expose a clearly documented structured representation.

### Property lists

Support XML, binary, and OpenStep formats.

Use byte-oriented decoding where necessary.

Pay particular attention to:

- Dates.
- Binary data.
- UID values.
- Shared references.
- Cycles.
- UTF-16 input.
- macOS interoperability.

Verify compatibility using Apple's `plutil` where available, alongside portable fixtures.

### PHP serialization and pickle

These are data interchange formats, not permission to execute foreign code.

Use only Deser's Rust implementations.

Never invoke PHP's `unserialize` or Python's `pickle.loads` as part of XSH decoding.

Deser's implementations do not instantiate foreign classes or execute their constructors.

Even so, treat inputs as untrusted and enforce resource budgets.

Handle class identity and reference metadata explicitly.

When ordinary XSH values cannot preserve those semantics, reject or document the intentionally reduced data model.

Do not claim arbitrary round-trip fidelity for object graphs that contain cycles.

### CSV / TSV

Support:

- Headers.
- Headerless records.
- Field-count validation.
- Configurable delimiter and quoting where justified.
- CRLF and UTF-8.
- Repeated rows.
- Streaming.
- Type-directed field interpretation.
- Explicit output column ordering.
- Formula-injection-safe output as an opt-in policy.

Preserve default strictness. Do not silently repair malformed quotes or discard columns.

### URL-encoded forms

Support:

- Percent decoding.
- Repeated keys.
- Bracket syntax.
- Nested keys.
- Type-directed scalar interpretation.
- Explicit duplicate-key policy.

Use no hidden HTTP request context.

### Environment mappings

Input must be an explicitly supplied mapping.

Do not read or mutate the ambient environment inside a pure codec operation.

Preserve XSH's effect and environment-scoping model.

### Debug formatting

Use `deser-debug` to render compatible values where appropriate.

Preserve XSH's existing `Str`/debug display semantics unless deliberately introducing a separately named representation.

Do not treat debug output as a stable wire format.

---

## 8. Numeric and extension representation

Audit XSH's actual supported runtime numeric values and map keys.

Do not assume every Deser scalar maps directly to an XSH scalar.

Define conversion policies for:

- Signed integers.
- Unsigned integers.
- Values exceeding XSH's integer range.
- Floating-point values.
- NaN and infinity.
- Exact decimal literals.
- Big integers.
- Binary values.
- Date/time values.
- UUIDs.
- Tags and extensions.
- Map keys of unsupported types.
- Arbitrarily nested data.
- Shared or cyclic references.

Use the smallest policy that preserves correctness.

**No silent precision loss.**

Do not convert a large integer into an approximate float just to avoid an error.

Do not stringify non-string keys merely to fit `Record`.

Do not silently drop unknown tags when they are semantically significant.

If a representation is unsupported, return a structured codec error.

A later extension to XSH's value model should be justified by broad usefulness across the language, not a single obscure format edge case.

---

## 9. JSON migration requirements

The existing `miniserde` dependency must eventually be removed.

Treat the current implementation as the compatibility baseline.

Before replacing it, capture actual behavior through golden tests.

### Decode parity

Test:

- Null, booleans, strings.
- Integer boundaries.
- Unsigned numeric boundaries.
- Floating-point values.
- Negative zero.
- Exponent notation.
- Decimal rounding.
- Nested arrays/maps.
- Empty collections.
- Duplicate keys.
- Unicode.
- Escaped Unicode and surrogate pairs.
- Malformed input.
- Invalid UTF-8 for byte-based parsing.
- Deep nesting.
- JSON Lines.
- Error propagation.
- Resource exhaustion.

### Encode parity

Test:

- Stable ordering of record keys.
- Compact output.
- Pretty output.
- Float formatting.
- Escaping.
- Sets and supported enum values.
- Dynamic records.
- `Value` versus `LoweredValue`.
- Unsupported resource values.
- Existing error kinds.
- Large outputs.
- JSON Lines.
- Filesystem writes.

Build differential tests that run the old and new implementations on the same fixtures.

If Deser changes a behavior, classify it as:

- Required compatibility.
- Desirable intentional semantic tightening.
- Acceptable documented difference.
- Regression requiring repair.

Do not accidentally turn implementation changes into language-spec changes.

### Remove old machinery

Once parity and performance are acceptable:

- Remove `miniserde`.
- Remove superseded dynamic JSON tree conversion.
- Remove redundant JSON emitters/traversal implementations.
- Reuse shared codec logic for JSON and YAML where reasonable.
- Keep specialized fast paths only when measurements justify them.

The endpoint should be simpler overall, not two parallel JSON implementations connected by a compatibility switch.

---

## 10. Error handling and resource limits

Integrate with XSH's existing structured error system.

Do not create stringly typed errors that downstream scripts must parse.

Distinguish:

- Malformed syntax.
- Invalid encoding.
- Unsupported format.
- Unsupported value.
- Numeric overflow.
- Duplicate key.
- Resource limit.
- Invalid schema value.
- Invalid format options.
- Host I/O error.
- Unexpected upstream failure.

Retain XSH's call-site span.

Include source line, column, byte offset, and value path from Deser where available.

A YAML parser error should identify the YAML source location, not only the XSH function call.

Use the existing error-family infrastructure or the smallest coherent extension to it.

### Untrusted data limits

Enforce explicit bounds on:

- Input bytes.
- Nesting depth.
- Number of values.
- Number of mapping entries.
- String sizes.
- Alias expansion.
- Decoded output growth.
- Stream item count where configured.
- Parser allocations where practical.

Do not assume that nonrecursive parsing eliminates resource-exhaustion risks.

Do not catch panics and convert them into silent success.

Use ordinary structured error returns.

---

## 11. Preserve XSH's effects and ownership model

Codec operations fall into distinct effect categories.

Pure operations:

- Decode from supplied text or bytes.
- Encode in-memory values.
- Convert explicitly supplied environment mappings.
- Render debug text.

Effectful operations:

- Read files.
- Write files.
- Read the ambient environment if such an operation is explicitly exposed.
- Perform stream I/O.

Register those effects correctly in XSH's canonical module signatures.

Do not make `codec.read` pure because the parser itself is pure.

Do not bypass the language's checked `Path` and filesystem boundaries.

Preserve existing file-write and overwrite policies.

Do not introduce process-global mutable codec configuration.

Do not expose writable global registries.

---

## 12. Integration locations

Start with these ownership boundaries.

### Runtime value adapters

Implement the shared adapter near XSH's existing runtime serialization machinery.

Avoid putting format-specific code into `src/runtime/value.rs`.

Keep core value types independent of specific codec crates.

### Format backends

Implement a compact backend-dispatch layer under `src/modules/` or an appropriately scoped codec module.

Separate format-specific configuration from common XSH conversion logic.

### Public API

Register the new module APIs in:

`crates/xsh-registry/src/signature/modules.rs`

Add runtime operation IDs in:

`crates/xsh-registry/src/runtime_op.rs`

Extend lowerer/evaluator dispatch through the existing native operation mechanism.

### Existing JSON

Replace or consolidate:

- `src/modules/json.rs`
- `src/runtime/eval/json.rs`

Retain unrelated JSON path helpers, type validation, and other functionality that is genuinely format-independent.

### Existing INI

Integrate `deser-ini` using the existing public API where compatible.

### Errors

Use:

`crates/xsh-registry/src/errors.rs`

and the corresponding runtime error conversion.

### Documentation

Update authored documentation sources before regenerating references.

Follow repository instructions for generated files.

Do not manually edit generated docs as the source of truth.

---

## 13. Testing matrix

Use the Deser upstream examples and fixture suites as references, but test through the actual XSH language boundary.

At minimum, each format needs:

| Test | Requirement |
|---|---|
| Basic decode | Valid input constructs expected XSH value |
| Basic encode | XSH value produces valid output |
| Round trip | Values preserved where representable |
| Invalid input | Structured error |
| Numeric limits | No accidental overflow or precision loss |
| Nested values | Correct container semantics |
| Duplicate keys | Explicit documented policy |
| Unsupported values | Fail without silent coercion |
| Resource limits | Controlled failure |
| File operations | Correct XSH filesystem effects |
| Cross-platform | No platform-specific assumptions |
| Direct adapter | No required `deser-value` intermediate tree |

Add format-specific tests for XML namespaces, pickle references, CBOR tags, YAML aliases, CSV records, plist binary input, and so on.

### Cross-format tests

For values representable by both formats, verify workflows such as:

```xsh
let value = yaml.decode(source)?
let json_text = json.encode(value)?
let restored = json.decode(json_text)?
```

Likewise:

```xsh
let packet = codec.decode_bytes("cbor", binary)?
let yaml_text = yaml.encode(packet)?
```

These tests should verify data-model preservation, not necessarily byte-for-byte equality.

Where conversion is impossible, require explicit errors.

### Typed boundary tests

Verify that `.require(T)` works identically regardless of the self-describing source format.

Include:

- Nested records.
- Optional fields.
- Lists.
- Unions.
- Nominal types.
- Str-backed enums.
- Validated integers.
- Missing fields.
- Wrong types.
- Wrong enum values.
- Error paths.

Add tests for type-directed lexical formats to prove correct parsing into declared XSH schemas.

Also verify that JSON/YAML strings are not silently coerced into numbers in violation of XSH semantics.

### Streaming tests

Verify:

- Lazy consumption.
- Cancellation.
- Backpressure.
- Correct item indexing.
- Error attribution.
- Bounded memory.
- Repeated reads.
- Input cleanup.

### Compatibility tests

Preserve XSH's existing JSON and INI golden fixtures.

Run old-versus-new differential tests before retiring `miniserde`.

---

## 14. Performance acceptance

This migration affects XSH's foundational data paths and must be measured.

Follow the repository's performance/deployment campaign rather than inventing a new benchmark methodology.

### Benchmark at least

- Small JSON decoding.
- Small JSON encoding.
- Large JSON decoding.
- Large JSON encoding.
- JSON Lines.
- Pretty JSON.
- Nested values.
- Numeric-heavy JSON.
- YAML decoding.
- YAML encoding.
- TOML decoding.
- CSV record streams.
- CBOR encoding/decoding.
- MessagePack encoding/decoding.
- Cold startup with no codec usage.
- Cold startup with JSON usage.
- Cold startup with YAML usage.

Compare:

1. Current `miniserde` implementation.
2. Deser through `deser-value`.
3. Direct XSH/Deser adapter.
4. Optional specialized lowered-value adapter.

For JSON, the existing optimized implementation is the performance baseline.

**No speedup claims without matched measurements.**

### Binary-size gates

Measure `xsh`, `xsht`, and `xshi` using actual production build settings and targets.

Account for all newly linked codecs.

The current performance campaign documents a production binary-size gate of approximately 5% over its accepted baseline. Honor the exact active criterion and measure the complete products, not merely the size of the new crate.

If the full codec family exceeds the limit:

- Inspect which crates dominate binary growth.
- Disable unnecessary default features.
- Remove unused derive or auxiliary dependencies.
- Improve dead-code elimination.
- Investigate codec grouping and linking.
- Avoid duplicate parsers or generic value representations.

Do not silently drop required codecs.

Do not hide incomplete integration behind unadvertised features.

If XSH's documented acceptance thresholds remain blocked by unrelated campaign work, record that limitation and the independently verified results; do not fabricate a completed qualification.

### Resource safety

Measure peak RSS on representative workloads.

Preserve XSH's no-unbounded-recursion guarantees.

Investigate allocation patterns in the direct adapter.

Do not introduce an all-value copy merely for convenience when direct serialization is feasible.

---

## 15. Scope discipline

This is a large campaign. Do not broaden it into unrelated redesign.

Explicit non-goals:

- Rewriting XSH's parser.
- Replacing XSH's type system.
- Adding language-level reflection.
- Adding new generic syntax.
- Generating Rust or XSH schemas from files.
- General-purpose plugin loading.
- Introducing Tokio as a new runtime.
- Network configuration discovery.
- Schema registries.
- JSON Schema compilation.
- ORM features.
- YAML AST editing.
- Comment-preserving configuration rewriting.
- Formatters.
- CLI wrappers around Python/PHP/XML tools.
- New process execution mechanisms.
- Ambient environment access from pure functions.
- Replacing unrelated filesystem APIs.
- Replacing unrelated HTTP or networking infrastructure.
- Broad rewrites of deployment tooling.

The objective is a better structured-data boundary inside the existing language.

---

## 16. Execution plan

Complete the campaign in independently testable stages.

### Stage A — Characterize existing behavior

- Establish the exact current JSON API contract.
- Capture differential JSON fixtures.
- Characterize INI behavior.
- Baseline performance and binary size.
- Inspect Deser's published crate versions and feature sets.
- Produce a concise compatibility matrix.

Do not modify the public language API yet.

### Stage B — Prove direct integration

- Add pinned Deser dependencies.
- Implement a direct XSH value decoding sink.
- Implement the narrowest correct encoding adapter.
- Verify `Send`/`Sync` trait constraints.
- Prototype both JSON and YAML.
- Benchmark against the dynamic Deser-value route.
- Resolve numeric and map-key semantics.

This stage must establish that the architecture is sound before integrating all formats.

### Stage C — Replace JSON

- Move JSON decoding to Deser.
- Replace the old serialization path.
- Preserve special lowered-value behavior.
- Retain JSON Lines support.
- Pass differential tests.
- Remove `miniserde` and redundant code.
- Verify JSON benchmark regressions against baseline.

No permanent compatibility backend.

### Stage D — Common text formats

Integrate:

- YAML.
- TOML.
- INI.
- JSONC.
- JSON5.
- Hjson.

Add `yaml` and `toml` public modules and the generic codec dispatch.

Retain existing JSON and INI APIs.

### Stage E — Binary formats

Integrate:

- CBOR.
- MessagePack.
- Binary property lists.
- PHP serialization.
- Python pickle.

Establish the representability and security policies.

### Stage F — Specialized and record formats

Integrate:

- CSV/TSV.
- URL-encoded forms.
- XML.
- Remaining property-list dialects.
- Environment mappings.
- Debug formatting.

Implement type-directed lexical decoding where needed.

### Stage G — Streaming and transcode capabilities

Integrate supported sequence/record streams into XSH's existing `Stream` architecture.

Evaluate `deser-transcode` for direct format-to-format conversion without constructing XSH values where that functionality is genuinely valuable.

Do not let transcode bypass the safety and extension-representation policies.

### Stage H — Consolidation

- Remove dead adapters.
- Remove duplicated traversal.
- Simplify dispatch.
- Deduplicate format options.
- Audit dependencies.
- Run differential correctness tests.
- Run final performance gates.
- Update documentation and examples.
- Commit the resulting implementation.

The stages are logical checkpoints, not excuses to leave half-finished public APIs.

---

## 17. Quality review

Before finalizing, perform an adversarial review.

### Architecture

- Is any complete value tree unnecessarily materialized?
- Are codec-specific semantics leaking into XSH's core values?
- Is there duplicate serialization logic?
- Did the generic codec dispatcher become overengineered?
- Are `deser-value` and XSH `Value` both present in production paths unnecessarily?
- Are type conversion rules duplicated?
- Can more logic be implemented through Deser's existing event model?

### Correctness

- Can number conversion lose precision?
- Can tags or extension metadata disappear silently?
- Do source locations survive into errors?
- Can duplicate keys produce inconsistent results?
- Do input/output types match their declared signatures?
- Can lexical source values bypass XSH validation?
- Can nominal types be forged through input metadata?

### Security

- Can YAML aliases cause excessive resource use?
- Can pickle or PHP data execute foreign code? They must not.
- Can input trigger unbounded allocation?
- Can an environment codec mutate ambient state?
- Can a supposedly pure operation access the filesystem?
- Can unsupported extension objects impersonate XSH capabilities?

### Performance

- Are JSON hot paths slower for avoidable reasons?
- Did dynamic dispatch introduce unacceptable overhead?
- Did compiler/linker settings hide binary growth?
- Are large input values copied repeatedly?
- Can the adapter stream instead of buffering?

### Simplicity

- Are there unnecessary helper types?
- Could a custom converter be replaced by upstream Deser functionality?
- Do format-specific options duplicate upstream behavior?
- Is every new file necessary?
- Can tests be simplified without reducing their strength?

Fix issues discovered by this review, not merely document them.

---

## 18. Test and commit procedure

Follow XSH's instructions, especially:

```bash
cargo dev build
cargo dev gate PATH...
cargo dev gate --full
cargo dev test linux
```

Use the repository's documented `verification` and production profiles appropriately.

Run focused tests first.

Do not run Rust or XSH formatters or autofixers. The repository explicitly prohibits formatter-driven churn.

Avoid unrelated test/build commands that can alter the host.

Respect the existing pinned Linux container environment.

Update authored documentation sources, then run the documented docs-generation/check process.

Preserve existing test fixtures unless an intentional semantic change requires updating them with supporting evidence.

Commit coherent stages.

Only push validated commits to the confirmed target branch. Never force-push or overwrite concurrent work.

---

## 19. Definition of done

The campaign is complete when:

- [ ] Deser is XSH's unified codec framework.
- [ ] All 17 current Deser format crates have working, appropriate XSH integration surfaces.
- [ ] Existing JSON APIs remain compatible.
- [ ] `miniserde` is removed.
- [ ] Obsolete JSON decoding and encoding machinery is removed.
- [ ] A shared direct XSH value adapter is implemented.
- [ ] Production decoding does not require `deser-value`.
- [ ] Production encoding avoids unnecessary full-value materialization.
- [ ] XSH remains the owner of its types and runtime values.
- [ ] `.require(T)` works consistently with decoded self-describing values.
- [ ] Type-directed lexical-format decoding works without a separate schema language.
- [ ] YAML and TOML have ergonomic first-class APIs.
- [ ] Specialized codecs have accurate interfaces and documented limitations.
- [ ] Binary input and output are genuinely byte-safe.
- [ ] Document and record streams are supported where appropriate.
- [ ] Structured error semantics and source locations are preserved.
- [ ] No unsafe coercions or silent loss of meaningful data.
- [ ] Security and resource limits are enforced.
- [ ] No arbitrary foreign-code execution occurs during decoding.
- [ ] XSH's effect system remains correct.
- [ ] Existing JSON/INI test coverage passes.
- [ ] New cross-format and type-directed tests pass.
- [ ] Performance and binary-size measurements are documented.
- [ ] The relevant repository gates pass, or remaining external qualification blockers are explicitly identified.
- [ ] Documentation and generated references are current.
- [ ] Changes are committed without disturbing unrelated work.

Do not falsely claim support for a feature that an underlying codec or XSH's value model cannot represent. A codec may have explicit, tested representability restrictions; these must be visible in its API documentation and errors.

The full set of format crates must nonetheless be integrated, not merely listed as future work.

---

## 20. Final report

Report:

1. Exact commit SHAs.
2. Final module/API structure.
3. Deser crate versions and enabled features.
4. Public functions added or modified.
5. Complete 17-format support matrix.
6. Direct XSH value adapter design.
7. Type-directed decoding mechanism.
8. Remaining representability limitations.
9. JSON behavioral differences, if any.
10. `miniserde` removal confirmation.
11. Code and dependency reductions.
12. Binary-size changes for `xsh`, `xsht`, and `xshi`.
13. Cold-start and codec benchmark results.
14. Native and Rust test results.
15. Linux verification results.
16. Unresolved performance or compatibility risks.

## Final engineering directive

Build this as a **natural extension of XSH's type system and runtime**, not as a serialization framework bolted onto the side.

The central design principle is the same one that makes Reylo compelling: each fact should have one owner.

- XSH owns types.
- XSH owns runtime values.
- XSH owns validation.
- XSH owns effects and resources.
- Deser owns parsing and serialization.
- The shared adapter translates between them.

Do not duplicate those responsibilities.

Prefer direct value construction, bounded allocation, explicit errors, deterministic behavior, and excellent compatibility.

Do not preserve obsolete custom JSON code merely because it already exists. Equally, do not throw away optimized logic until its replacement is proven.

**Deliver a complete, coherent multi-format serialization capability worthy of a serious systems glue language.**
