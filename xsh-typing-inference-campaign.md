# XSH typing: final quality plan

Goal: make it easy to write correct XSH. Delete this file when every item
below has landed and the final gates pass. The campaign history (the reverted
first campaign, items 1–6 of the replanned campaign, and verification records)
is in git; see `git log -- xsh-typing-inference-campaign.md`.

## Settled design

- **Annotations:**
  - Signatures are written at declaration boundaries; everything inside a body
    is inferred, locally and monomorphically.
  - Required parameters, public contracts, schema boundaries and recursive
    declarations stay annotated.
- **Assertions:** `assert` is the only assertion form, and a bare Bool
  statement is `check.bool-statement`.
- **Proc returns:**
  - Every callable tail is data.
  - An unannotated private proc infers `Result[T]` from its value tails.
- **Single source of truth:**
  - Lowering consumes the checker's published types, API call resolutions and
    argument bindings.
  - One executor implementation per operation, on the heap frames.
- **Formatting:** `xsht fmt` refuses output that does not reparse to an
  equivalent tree. Lint diagnostics are identical before and after formatting.

## Remaining items

Done and removed from this list: `Any` validation (`.require`), discarded values are errors everywhere, the mistake corpus, `{expr}` f-strings, the xsht CLI command table, tail mismatch, the soundness fuzzer (`make fuzz`; fuzzing is wound down), lowering drift, the provable formatter and grouping rules, the docs pipeline (`make docs`, the `template` module, the templated tour), regex anchors, parser bugs, test speed and release-only test binaries.

## Diagnostics and grouping (remaining)

- **Typed diagnostic codes.** Replace string codes (`LINT_CODES`,
  `FIXABLE_CHECK_CODES`, and about 390 scattered `parse.*`/`check.*`/
  `lint.*`/`run.*` literals) with one `DiagnosticCode` enum. Each variant
  carries its summary, severity and fix kind as properties.
  - Diagnostics carry the enum.
  - `--only` parses into it.
  - `lint --list` and `make docs` iterate it.

## Tooling (remaining)

- **Small fixes found by the fuzz lane.**
  - The formatter is not idempotent on `fn2(...{p0: if … })`.

## Grammar derivation (running)

There is one typed grammar definition, `src/syntax/grammar.rs`, and nothing
else is maintained by hand. Today the EBNF in SPEC Appendix A is
documentation only; no code reads it.

- **Grammar as data:** productions, terminals, keyword and operator sets,
  precedence and associativity, continuation tokens, literal prefixes and
  stage kinds.
- **The parser reads those tables** (binding powers, keyword dispatch,
  continuation, literal kinds) from the grammar data. It stops keeping its
  own copies.
- **Generated docs:** `make docs` renders `docs/reference/grammar.md`, SPEC
  Appendix A becomes a link to it, and `docs-check` fails on drift.
- **Structural productions are proven equal in both directions on every
  test run:**
  - every sentence generated from the grammar parses without diagnostics;
  - every corpus file and every fuzz-generated program, lexed with the real
    lexer, is recognized by an Earley recognizer over the same grammar data.
- **The hand-written parser stays,** with its error recovery and its
  diagnostics.

## Single sources of truth (queued)

The same kind of fix as the grammar work: each fact is defined once, and
everything else is generated from it or checked against it.

- **SPEC examples (done):**
  - SPEC's 42 `xsh` blocks move to `docs/snippets/spec/*.xsh`. Fragments are
    checked inside a wrapper.
  - The snippets go through check, lint and `fmt --check`, and portable ones
    run.
  - `make docs` renders SPEC.md from a template, as it does the tour, and
    `docs-check` fails on drift.
- **Error facets:** a typed facet enum in the registry replaces the copies in
  `xsh-registry/src/errors.rs`, `value.rs::host_facet` and the SPEC table.
  - The runtime maps OS errors onto it.
  - The checker validates `is Facet` against it.
  - The SPEC facet table is generated from it.
- **Diagnostic codes:** the `DiagnosticCode` work also checks that every code
  SPEC prose mentions exists in the enum.

## Formatter (remaining)

- Keep short single-line blocks and records that fit the line width.
- The formatter moves a comment that ends a block past the closing `}`, and
  adds blank lines around comments. Both break snippet region markers.
- Not idempotent on `fn2(...{p0: if … })`.
- Then hold `docs/snippets/` (tour and spec) to `fmt --check`.

## Small contract fixes

- SPEC §10.2 lists `(expr)` as a command argument, while lint calls
  `(name)` stale command-value syntax. Settle it: SPEC says `$name` for a
  name and `(expr)` only for compound expressions, and lint flags only the
  redundant form.

## Final gates

These hold at every landing:
- full native suite on release;
- Rust gates on debug;
- `make check`: release lint, 15 s budget;
- `xsht fmt --check`;
- clippy;
- the Laputa monorepo (`../laputa`);
- one Linux run in the `Dockerfile.test` image at the end.

Then delete this file.
