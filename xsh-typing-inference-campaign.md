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

Done and removed from this list: tail mismatch, the soundness fuzzer (`make fuzz`; fuzzing is wound down), lowering drift, the provable formatter and grouping rules, the docs pipeline (`make docs`, the `template` module, the templated tour), regex anchors, parser bugs, test speed and release-only test binaries.

### 2. Dynamic data is always validated before it becomes a concrete type

Today, `raw.get("a")` must use `.require`, but `raw.a` flows into typed bindings
and arithmetic and fails only at runtime.

- Rule: field access, indexing and method results on `Any` yield `Any`.
- `Any` establishes a concrete type only through `.require(T)` or a checked
  type pattern. This is `check.dynamic-boundary`, as `.get` is today.
- Spec first, in `docs/SPEC.md` (Typing). The current spec text says "Bare Any
  retains its established dynamic field behavior".
- The diagnostic carries a fix that inserts `.require(T)` when the target type
  is known from context, and the fix is selectable with `xsht lint --only`.
- Migrate this repo and `../laputa` before the error lands. `../laputa` becomes the monorepo, absorbing `../packages`.

### 4. Discarded values are errors everywhere (running)

- Enforce SPEC §8.1 at the top level too (user decision), with a safe
  `let _ = ` fix.
- The mistake corpus (`tests/xsh/mistakes.xsh`, 54 cases) landed.

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

## Syntax (running)

### f-strings interpolate with `{expr}`

The `f` prefix already says the string interpolates, so `f"..."`,
`f"""..."""`, and `fp"..."` use `{expr}`. Command words keep `${expr}` and
`$name`. Python's f-strings took years to fix (PEP 701 made the lexer
tokenize the expression), and today's `${...}` scanner shares their flaws: it
balances bytes, honors `#` comments, and guesses a format spec at the
rightmost `:`. The design below closes each of those edges.

1. **The parser decides where an interpolation ends.** The `{...}` content
   is lexed and parsed by the ordinary expression parser, which runs until
   the matching `}`. No byte scanner guesses. Nested strings, nested
   f-strings with the same quote, record and map literals, blocks, named
   arguments, and slices all work inside, because each is an ordinary
   expression.
2. **Escapes:** `{{` and `}}` in the text part. A lone `}` in text is an
   error whose fix writes `}}`, and an unclosed `{` is an error. `\{` is not
   an escape, so there is only one way to write a brace.
3. **Leading brace:** `{{` in text is always an escape, so an expression that
   begins with `{` needs a space: `f"{ {a: 1}.a }"`. The formatter keeps that
   space (a `join_tokens` rule) and the equivalence check covers it. Inside an
   expression, `}}` is just two closing braces.
4. **Format spec:** keep the existing widths, `{expr:>N}`, `{expr:<N}`, and
   `{expr:0N}` (116 uses, mostly tables), and document them in SPEC. The spec
   starts at a `:` the expression parser stops at. XSH expressions never
   contain a bare top-level `:`: named arguments, record and map entries are
   bracketed, and slices use `..`. That makes the spec unambiguous by
   construction, where today the rightmost-`:` guess decides it. Anything else
   after `:` is an error. No Python mini-language, no `!r` or `!s`
   conversions, and no `{x=}` debug form.
5. **`$` is plain text inside f-strings,** but `${` and `$ident` where
   `ident` names a binding in scope are errors with a fix (`{expr}`, or `\$`
   for a literal dollar). A shell user's `f"${x}"` or `f"$HOME"` would
   otherwise print `$` followed by the value, silently. `\$` stays a valid
   escape everywhere. Literal prices like `f"costs $5"` are fine.
6. **Restrictions inside `{...}`:** no comments, and no line breaks inside a
   single-line `f"..."`; block `f"""..."""` may break lines inside braces.
   An empty or whitespace-only `{}` is an error.
7. **Forgotten prefix:** new `lint.missing-f-prefix` (warning with a fix that
   adds `f`, or `fp` for `p"..."`). It fires when a plain `"..."` or
   `p"..."` contains `{name}` or `{name.field}` and `name` is in scope. This
   is Python's most common f-string bug.
8. **Diagnostics** inside interpolations point at exact source columns in
   both single-line and block strings, including after `{{` escapes and
   layout removal.
9. **Values:** display conversion is unchanged (§11.4 table). Path fragments
   in `fp"..."` keep their bytes.
10. **Migration:** mechanical and token-based, through the parser, never by
    regex.
    - Rewrite `${e}` to `{e}`, `$name` to `{name}`, and `\${` to `$`, and
      double literal braces.
    - Rewrite fix-hint emitters and lints that build f-strings.
    - Rewrite the corpus, docs snippets and templates, and Rust-embedded XSH.
    - Then `../packages` and `../laputa`.
    - Strings that generate XSH scripts (most of the 153 with literal braces)
      may move to plain `"""..."""` plus `+`, or `template.render`, when
      that reads better than `{{ }}`.
11. **Edge-case corpus,** in native tests plus a fuzz-generator update:
    - nested same-quote f-strings;
    - `{ {a: 1}.a }`;
    - `{m["k"]}`;
    - `{f(a: 1)}`;
    - `{xs[1..2]}`;
    - `{x:>4}` next to `{f(width: 4)}`;
    - `{if c { "a" } else { "b" }}`;
    - `{{`/`}}` at the start and end and adjacent to interpolations;
    - a lone `}`;
    - an unclosed `{`;
    - empty `{}`;
    - `${x}` and `$name` errors;
    - `f"costs $5"`;
    - `#` in text versus in an expression;
    - block strings with braces on indented lines;
    - multibyte text before an error column;
    - fp strings joining Path bytes.

    The formatter's round-trip and lint-invariance properties must hold on
    all of them.

## Grammar derivation (queued, after the f-string lane)

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

- **SPEC examples:**
  - SPEC's 42 `xsh` blocks move to `docs/snippets/spec/*.xsh`. Fragments are
    checked inside a wrapper.
  - The snippets go through check, lint and `fmt --check`, and portable ones
    run.
  - `make docs` renders SPEC.md from a template, as it does the tour, and
    `docs-check` fails on drift.
- **xsht CLI:** one typed command and option table drives both argument
  parsing (`app.rs`) and `--help` (`help.rs`). The generated CLI reference
  follows from it.
- **Error facets:** a typed facet enum in the registry replaces the copies in
  `xsh-registry/src/errors.rs`, `value.rs::host_facet` and the SPEC table.
  - The runtime maps OS errors onto it.
  - The checker validates `is Facet` against it.
  - The SPEC facet table is generated from it.
- **Diagnostic codes:** the `DiagnosticCode` work also checks that every code
  SPEC prose mentions exists in the enum.

## Formatter (remaining)

- Keep short single-line blocks and records that fit the line width.
- Then make the tour snippets `fmt`-clean and require that in `docs-check`.

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
