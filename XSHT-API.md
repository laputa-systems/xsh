# `xsht api`: the first-contact XSH reference

`xsht api` is the language surface a new coding agent can use when it has no
repository source or example corpus to inspect. It must be useful as a small,
standalone manual: the agent should be able to write a first valid script,
understand the checker feedback loop, and discover the contract for an API it
needs without following internal source links.

The command is a projection of the canonical XSH signature and language
metadata. It is not a source index, test index, generated Markdown manual, or
catalog of larger programs.

## First invocation

`xsht api` with no selector prints a compact onboarding guide containing:

- one tiny valid XSH script;
- the basic `xsht check`, `xsht fmt`, and `xsht lint` loop;
- the `xsh SCRIPT` run command;
- representative queries for language rules, modules, exact operations,
  methods, records, and search.

The onboarding script is part of the executable API contract. The API test
suite extracts it and runs `xsht check` against it, so the first example cannot
quietly drift out of the language.

`xsht api --format jsonl` with no selector emits one machine-readable guide
object rather than a batch response with zero queries.

## Query surface

The text form is intentionally easy to discover and copy:

```text
summary
module:NAME
api:MODULE.FUNCTION
method:RECEIVER.METHOD
record:NAME
language:ID
search:TERMS
```

`module:NAME` returns the module overview followed by its member index. Use an
exact `api:` query for the full contract of one function. `language:ID` accepts
an exact language item or a prefix such as `language:core`; a prefix returns a
readable group of related rules. `search:` matches IDs, summaries, contracts,
and retrieval tags.

Exact queries and language-reference groups default to full details. Module
groups and search default to a compact list of purposes; `--details full`
expands them. `--details basic` keeps any query compact. Batch order,
`--query-file`, `--stdin`, `--strict`, and JSONL remain available for tools that
need them.

## What an exact result teaches

Every public module, module function, value method, record, and language item
must have a non-empty, domain-specific purpose. Full results may contain:

```text
purpose:
contract:
effects:
signature:
tags:
example:
```

The purpose answers “what is this for?” The contract records only behavior a
new caller needs to avoid a wrong program: ownership, cleanup, rooted
boundaries, ordering, platform limits, status-versus-error distinctions,
UTF-8 or byte boundaries, dynamic values, and similar constraints. A contract
may be empty when the signature and purpose genuinely cover the behavior.

Effects are derived from the same checked signature metadata used by the
language checker. They must not be maintained as a second string-keyed list in
`xsht`. `none` means the operation has no host capability requirement;
otherwise the output names the required XSH effect such as `fs`, `net`,
`process`, `env`, `time`, or `io`. Error propagation is explained by the
language reference and the return type; a fallible return does not itself mean
that the caller declared the `error` effect.

Examples are short XSH fragments showing the spelling and result boundary.
They should be included when they remove real ambiguity, especially for
commands, streams, paths, records, and fallible operations. They should not
turn every entry into a tutorial.

Example source is kept as .xsh files under docs/snippets/api/. The registry
associates those files with API IDs and embeds their contents into API output;
the Rust metadata files do not contain a second copy of the XSH source.

Results must not contain:

- implementation paths or Rust operation names;
- pointers to individual tests;
- fields whose only purpose is proving that prose was attached;
- generic templates such as “standard record”, “method for TYPE”, “see the
  signature”, or “no additional constraints”.

## Metadata ownership

Keep the contract beside the language surface it describes:

- module and method documentation belongs with the signature registry in
  `crates/xsh-registry/src/signature/`;
- record documentation belongs with the record API definitions in the module
  signature registry, while `crates/xsh-registry/src/records.rs` remains the
  schema/type definition source;
- language rules belong in `crates/xsh-registry/src/reference.rs`;
- `crates/xsht/src/api.rs` selects, derives, and renders metadata; it does not
  own a parallel documentation table.

The registry must reject missing or empty public documentation. It should not
require hand-maintained references to implementation files or test names.

## Evidence and verification

Coverage is established by running the behavior tests, not by attaching static
test strings to API records. Registry tests verify that the public signature
surface and documentation inventory agree. API integration tests verify the
onboarding guide, query selectors, contracts, effects, examples, JSONL shape,
batch ordering, and exhaustive summary. Language and host behavior remain
owned by their ordinary native XSH and Rust test suites.

The focused development loop is:

```sh
cargo check -p xsh-registry -p xsht --tests
cargo test -p xsh-registry --lib
cargo test -p xsht --test api
```

For a complete API change, also run the relevant `xsh` module/signature tests,
the `xsht` test suite, the integration gate, and `git diff --check`. Do not use
formatters or autofixers as part of this work; the user owns that final pass.
