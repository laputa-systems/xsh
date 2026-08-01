# XSHT API Registry Plan

## Goal

Replace generated API Markdown with a single authoritative API registry that
contains signatures, mandatory public documentation, runtime identities, and
executable coverage pointers. `xsht api` is a stateless query interface over
that registry; it does not own duplicate API prose or a second metadata table.

An agent writing XSH must be able to make one request for several related facts
and receive narrow, stable, machine-readable answers without scanning
`docs/STDLIB.md`, `docs/REFERENCE.md`, or an LSP index.

## Scope

This plan covers:

- standard modules, methods, and standard record schemas;
- core language reference data currently emitted into `docs/REFERENCE.md`;
- mandatory documentation for exported XSH declarations;
- the `xsht api` text and JSONL query interface;
- migration away from generated `docs/STDLIB.md` and `docs/REFERENCE.md`.

This plan does not add an LSP server, editor protocol, natural-language search
service, external index, or a second API-description format.

## Current Ownership

The implementation already has the structural API source of truth:

- `xsh_registry::signature::ApiSpec` in
  `crates/xsh-registry/src/signature/mod.rs` owns module and method signatures.
- `ModuleFnSig` owns parameter names, parameter types, default markers, return
  types, purity, command eligibility, argument-check policy, and `RuntimeOp`.
- `xsh_registry::RuntimeOp` in `crates/xsh-registry/src/runtime_op.rs` is the
  stable public-operation identity used by evaluator dispatch.
- `xsh::modules::signature::api_spec()` in `src/modules/signature.rs` adapts
  the registry for checking and lowering; its
  `api_spec_adapter_exactly_mirrors_registry` test prevents signature drift.
- `xsh_registry::reference::{EFFECT_REFERENCES, RUN_FORM_REFERENCES}` already
  owns part of the non-stdlib reference surface.

The duplicate layer is `crates/xsht/src/docs.rs`: it renders generated Markdown
and carries ad hoc module summaries, function summaries, parameter prose, and
return prose. That ownership must move into the registry before the generator
can be removed.

## Design Principles

- A public API declaration cannot be constructed without documentation.
- Signatures and structural facts are rendered, never repeated in prose.
- Documentation records constraints that types cannot express: trust
  boundaries, ownership, platform limits, ordering, safety, and intentional
  absences.
- Native tests are the default executable examples. A registry entry points to
  the nearest focused test rather than embedding tutorial snippets.
- `xsht api` may format or filter registry data, but it must not add API-specific
  facts, summaries, or fallback documentation.
- Exact API IDs, registry types, `RuntimeOp` variants, paths, and test names are
  stable grep targets.

## Terminology

| Term | Meaning |
|---|---|
| API item | A public module function, value method, standard record schema, or core language reference item. |
| API ID | A stable identifier such as `module.json.read`, `method.Path.read_text`, `record.FsEntry`, or `language.run.status`. |
| declaration docs | Required docs attached directly to an API declaration in the registry or parsed from an exported XSH declaration. |
| navigation | Structured implementation and native-test pointers that help an agent find proof and ownership. |
| query result | One response for one requested selector, including zero, one, or many matching API items. |

## Canonical Data Model

Extend the registry types in `crates/xsh-registry/src/signature/mod.rs`; do not
define equivalent structures under `crates/xsht/`.

```rust
pub struct ApiDocs {
    pub summary: &'static str,
    pub contract: &'static str,
    pub tags: &'static [&'static str],
    pub navigation: ApiNavigation,
}

pub struct ApiNavigation {
    pub implementation: &'static [&'static str],
    pub tests: &'static [&'static str],
    pub showcase: Option<&'static str>,
}
```

`summary` is a concise statement of purpose. `contract` is a concise statement
of the non-obvious rule, or an empty string only when the type and name fully
describe behavior. `tags` supply explicit search vocabulary such as `json`,
`dynamic`, `rooted`, `archive`, or `dry-run`; they are not inferred from prose.

`navigation.implementation` and `navigation.tests` contain repository-relative
paths, optionally ending in a `::symbol` test pointer. They are grep targets,
not prose. `showcase` is present only for one of the curated multi-module
programs under `examples/`.

The registry makes documentation mandatory at these levels:

- every `ModuleEntry` has `ApiDocs`;
- every `NamedModuleFns` has `ApiDocs` shared by all ordinary overloads;
- every `NamedMethodSigs` has `ApiDocs`;
- every standard record schema has `ApiDocs` and field documentation where the
  field name and type are insufficient;
- every language reference item has `ApiDocs`.

Overload-specific documentation is added only when one overload changes a
behavioral contract, not merely argument types. Model that with an optional
`overload_contract` on `ModuleFnSig` or `MethodSig`; do not duplicate the parent
summary.

Builder functions such as `module_sig` and method-signature constructors must
require documentation arguments. There must be no `ApiDocs::default()`, empty
builder fallback, or optional `docs` field for public items.

## Registry Families

The unified registry must expose four queryable families.

### Standard Modules And Methods

Migrate `ApiSpec`, `ModuleEntry`, `NamedModuleFns`, `MethodReceiverSig`, and
`NamedMethodSigs` first. Existing `Type`, `ParamSig`, `MethodReturn`,
`ApiArgCheck`, `pure`, `command`, and `RuntimeOp` fields remain structural
source data and are rendered directly by `xsht api`.

### Standard Records

Wrap the current `record_schemas()` data in registry entries with stable IDs
such as `record.FsEntry`. Include record field names and types, field docs only
where needed, and navigation to the module or native-test owner.

### Core Language Reference

Move the useful machine data from generated `REFERENCE.md` into a registry
family with stable IDs. Initial entries include:

- `language.effect.process`, `language.effect.fs`, and other effect entries;
- `language.run`, `language.run.status`, `language.run.capture`, and other run
  forms;
- `language.stream.map`, `language.stream.par-map`, and terminal stages;
- `language.trace.<event>` entries;
- `language.cli.xsh`, `language.cli.xsht`, and `language.cli.xshi` forms.

Normative multi-paragraph semantics stay in `docs/SPEC.md` and
`docs/SPEC-OS.md`. The registry entries point to the owning spec section and
nearest native tests; they do not restate the specification.

### Exported XSH Modules

Public declarations written in XSH need parser-visible documentation rather
than ordinary comments that can silently drift or disappear. Add two retained
doc-comment forms to the XSH grammar:

```xsh
##! Package metadata helpers that validate dynamic JSON at the boundary.

## Parses a manifest; callers must validate fields that remain `Any`.
export proc parse_manifest(path: Path) [fs, error] -> Result[Manifest] {
  ...
}
```

- `##!` is a single file/module documentation block before executable module
  content.
- contiguous `##` lines immediately before an `export` declaration form that
  declaration's docstring;
- ordinary `#` comments remain non-semantic;
- the parser retains the text and source span in the module interface;
- the checker rejects undocumented exported `proc`, `pure`, `type`, and
  exported record declarations with `check.missing-public-doc`;
- orphaned doc comments, duplicate module docstrings, and doc comments
  separated from an export are checker errors.

This phase applies to source modules that expose declarations through static
imports or `module.load`. It does not require public documentation for local
test helpers, private declarations, scripts, or core applet `main` functions.

## `xsht api` Interface

`xsht api` is the sole user-facing renderer. It has no persistent process and
reads the in-process registry for each invocation.

```text
xsht api [OPTIONS] QUERY...

QUERY := module:NAME
       | api:MODULE.FUNCTION
       | method:RECEIVER.METHOD
       | record:NAME
       | language:ID
       | search:TERMS
```

Selectors are explicit so a single command can mix exact lookups and searches:

```sh
xsht api \
  module:json \
  api:json.read \
  method:Path.read_text \
  record:FsEntry \
  language:run.status \
  search:"rooted extraction"
```

Do not use separate `show`, `module`, or `search` subcommands as the primary
interface. They prevent a mixed one-shot request or require an additional batch
language. A selector is the complete query unit; the query kind remains visible
in both argv and every response.

Required options:

```text
--format text|jsonl       Default: text.
--strict                  Exit nonzero when any selector has no match.
--details basic|full      Default: full for exact selectors, basic for search.
--query-file PATH         Read one selector per UTF-8 line; combine with argv selectors.
--stdin                   Read one selector per UTF-8 line from stdin; combine with argv selectors.
```

`QUERY...`, `--query-file`, and `--stdin` are intentionally additive. An agent
can ask every question it needs in one process without shell loops, repeated
startup, or undocumented batching conventions.

### Text Output

Text output preserves request order. Each selector prints a compact heading,
status, and matching items. Exact results include the full contract; searches
show summary and API ID unless `--details full` is requested.

```text
query: api:json.read
status: exact

api: module.json.read
signature: json.read(path: Path) -> Result[Any]
runtime-op: JsonRead
contract: Valid JSON is dynamic; require a schema before trusting fields.
implementation: src/modules/json.rs
tests: tests/xsh/stdlib/json.xsh::test_json_read_write_lines_and_paths
showcase: examples/json.xsh
```

Each result starts with `query:` and `status:` so agents can split a combined
text response without heuristic parsing.

### JSONL Output

`--format jsonl` emits exactly one JSON object for each requested selector, in
request order. A selector that finds multiple items returns one result object
with a `matches` array; a selector with no match returns an empty `matches`
array and a `status` of `missing`.

```json
{"schema_version":1,"query":"api:json.read","status":"exact","matches":[{"id":"module.json.read","kind":"module-function","summary":"Read JSON from a path.","contract":"Valid JSON is dynamic; require a schema before trusting fields.","signatures":[...],"runtime_ops":["JsonRead"],"implementation":["src/modules/json.rs"],"tests":["tests/xsh/stdlib/json.xsh::test_json_read_write_lines_and_paths"],"showcase":"examples/json.xsh"}]}
```

The JSON schema is versioned with a top-level `schema_version` field. Additive
fields are allowed within a version; removal, renamed fields, or changed scalar
types require a version increment.

### Search Semantics

`search:` is deterministic and local:

1. normalize terms by ASCII lowercase and split on whitespace;
2. match exact API-ID segments, module names, method receivers, record names,
   and explicit `tags` first;
3. then match words in `summary` and `contract`;
4. order results by exact ID match, exact tag match, prefix match, then stable
   API ID;
5. never call a remote service, embed an opaque ranking model, or infer API
   behavior from source text at query time.

`search:` returns all matches by default. A future `--limit N` is acceptable
only with stable truncation and a `truncated: true` response field.

## Query Implementation

Add an `api` arm beside the existing `docs`, `fmt`, `lint`, `grep`, and `test`
CLI commands under `crates/xsht/src/cli/`. Keep parsing, selectors, result
types, rendering, and registry adaptation in a dedicated module such as
`crates/xsht/src/api.rs`.

The module boundary is:

- `crates/xsh-registry`: canonical public structure and required docs;
- `src/modules/signature.rs`: runtime/checker adapter preserving the registry
  data and documentation;
- `crates/xsht/src/api.rs`: selector parsing, batch execution, and text/JSONL
  rendering only;
- `crates/xsht/src/cli/api.rs`: CLI argument handling and exit statuses;
- `crates/xsht/tests/api.rs`: black-box CLI contract tests;
- registry unit tests: documentation coverage and stable-ID validation.

Do not put a registry mirror, API summary match table, documentation fallback,
or test-path fallback in `xsht`.

## Exit Statuses

| Status | Meaning |
|---|---|
| `0` | All selectors were processed; missing selectors are represented in output unless `--strict` is set. |
| `1` | `--strict` was set and at least one selector was missing, or a query file/stdin selector was invalid. |
| `2` | CLI usage error, unknown option, invalid format, or malformed selector syntax. |

Batch execution never stops at the first missing selector. It renders every
well-formed selector, then computes the process status. This is required for
one-shot agent queries.

## Validation Invariants

Add registry-level tests that fail when:

- a public module, function, method, record, or language item lacks `ApiDocs`;
- `summary` is blank, a tag is blank, or a public navigation list has a blank
  entry;
- a navigation implementation or test path does not exist;
- a `tests/...::test_name` pointer names no exported native test proc;
- a showcase pointer is not listed in `examples/catalog.json`;
- two public items have the same API ID;
- a `RuntimeOp` is not represented by its intended public API entry, except for
  explicit internal-only operation allowlists;
- the `src/modules/signature.rs` adapter drops or changes registry docs.

Add parser/checker tests for `##!` and `##` documentation attachment,
documentation spans, missing public docs, orphaned docs, and dynamic module
interfaces.

Add `xsht api` integration tests for:

- exact module, function, method, record, and language queries;
- multiple mixed selectors in one command, preserving order;
- multiple matches from `search:`;
- text headings and JSONL one-result-per-selector framing;
- `--strict` after mixed found/missing selectors;
- `--query-file` plus argv selectors;
- stdin queries;
- stable JSON schema version and required keys;
- a representative navigation pointer into `tests/xsh/stdlib/*.xsh`.

## Migration Plan

### 1. Freeze The Existing Surface

Inventory generated `STDLIB.md` and `REFERENCE.md` sections against
`ApiSpec`, record schemas, `EFFECT_REFERENCES`, `RUN_FORM_REFERENCES`, stream
stage metadata, and CLI form metadata. Record every generated section's new
registry family or intentional deletion before modifying output.

The existing `crates/xsht/src/docs.rs` tests are the baseline inventory; do not
delete them before equivalent registry and `xsht api` tests exist.

### 2. Add Required Registry Documentation

Introduce `ApiDocs`, `ApiNavigation`, stable API-ID constructors, and required
builder arguments in `crates/xsh-registry`. Migrate module, function, method,
and record documentation from `crates/xsht/src/docs.rs` into the registry.

Keep prose concise during migration. Delete generated-doc wording that merely
restates signatures, parameter names, defaults, or return types.

### 3. Unify Core Language Reference Data

Move effects, run forms, stream stages, trace events, and CLI forms into the
same queryable registry family. Preserve `docs/SPEC.md` as the normative owner
for semantics and add spec-section navigation references where useful.

### 4. Implement Read-Only Exact Queries

Implement `xsht api module:`, `api:`, `method:`, `record:`, and `language:`
selectors with text and JSONL rendering. Add batch processing from argv before
adding search or input files.

### 5. Implement Search And Batch Inputs

Implement deterministic `search:`, then `--query-file` and `--stdin`. Test
mixed query sources and partial failures. Keep output ordering tied to request
order, not registry iteration order.

### 6. Enforce Exported XSH Documentation

Add retained `##!` and `##` doc-comment syntax, parser storage, checker errors,
and module-interface propagation. Update existing exported XSH modules in
`core/lib/`, `showcase/`, and other module roots only where they actually
export public declarations.

### 7. Migrate Agent Routing

Replace generated-reference links in `AGENTS.md`, `docs/AGENT-ROUTING.md`,
`docs/DOCS-STYLE.md`, `docs/TEST-MAP.md`, and `examples/README.md` with exact
`xsht api` command examples and the nearest native-test module paths.

Keep `docs/SPEC.md`, `docs/SPEC-OS.md`, `docs/STREAMS.md`, and `docs/JSON.md`
for durable contracts; they should link to `xsht api` only for API discovery,
not delegate normative behavior to the CLI.

### 8. Remove Generated Documentation

After query-parity and migration gates pass:

1. delete `docs/REFERENCE.md` and its generator path first;
2. delete `docs/STDLIB.md` and its generator path;
3. delete `crates/xsht/src/docs.rs`, `crates/xsht/src/cli/docs.rs`, and docs
   generation tests that have no remaining purpose;
4. remove `xsht docs build` and `xsht docs check` from CLI help, test maps, and
   agent routing;
5. retain only the registry coverage, API CLI, parser/checker, native-test,
   and showcase gates described here.

Do not remove generated docs merely because `xsht api` exists. Remove them only
after every generated API section has a registry representation, an API-query
test, and a routing replacement.

## Verification Gates

During implementation, use the narrowest gate first:

```sh
cargo test -p xsh-registry signature
cargo test -p xsh --lib modules::signature
cargo test -p xsht --test api
cargo test --test integration runtime::coverage::runnable_xsh_corpus_is_formatted_and_lints_without_warnings
target/debug/xsht test tests/xsh/stdlib
```

Before removing generated docs, require:

```sh
cargo test -p xsh-registry
cargo test -p xsh --lib
cargo test -p xsht
cargo test --test integration runtime::
target/debug/xsht test
target/debug/xsht test --examples
```

Run the corpus gate after adding `##!` or `##` documentation to XSH source so
the documentation syntax itself remains formatter- and lint-clean.

## Acceptance Criteria

- Every public registry item has mandatory canonical documentation and stable
  navigation pointers.
- Every exported XSH declaration has a parser-retained docstring or is rejected
  by the checker.
- `xsht api` can answer multiple exact and search selectors in one invocation.
- Text output remains readable; JSONL output is stable, versioned, and contains
  one response per requested selector.
- An agent can obtain signatures, contracts, `RuntimeOp`, ownership, native
  tests, and optional showcase pointers without reading generated Markdown.
- No API-specific prose tables remain under `crates/xsht/`.
- `docs/REFERENCE.md`, `docs/STDLIB.md`, and the docs generator are removed
  only after registry and CLI parity tests prove replacement coverage.
- The normal XSH corpus, native tests, retained showcases, and API CLI tests
  all pass without formatter or lint rewrites.
