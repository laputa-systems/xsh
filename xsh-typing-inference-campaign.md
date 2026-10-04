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

### In flight: provable formatter and grouping rules (lane HH)

1. **Line continuation:** continuation tokens must not overlap the tokens that
   can start an expression, so a line starting with `-` is a new statement. A
   test computes the overlap from the parser's own tables.
2. **Token separation:** an exhaustive token-pair separation table drives the
   printer.
3. **Parentheses:** one proven `needs_parens` function drives the printer and a
   new `check.redundant-parens` error, which has a fix.
4. **Grouping:** mixing `and` with `or`, or `??` with either, requires explicit
   grouping.
5. **Round trip:** a property test generates syntax trees and checks that
   printing and reparsing gives back the same tree.
6. **Migration:** this repo is migrated in the lane. `../packages` and
   `../laputa` follow as local commits.

### 1. Disagreeing value tails are an error

Today, `proc pick(flag: Bool) { if flag { 1 } else { "one" } }` silently falls
back to `Result[Unit]`. The user then sees an unrelated error at the use site.

- The `Result[Unit]` fallback applies only when every completion is Unit-like.
- Value tails whose types disagree report `check.type-mismatch` at the proc,
  naming both types.
- Owner: `src/sema/check/infer_return.rs`.
- Pin the behavior in `tests/xsh/private-proc-returns.xsh`.

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
- Migrate this repo, `../packages` and `../laputa` before the error lands.

### 3. Soundness property test

Every typing bug found in this campaign had the same shape: the checker
accepted a program that then failed at runtime. Make "well-typed programs do
not go wrong" a tested property.

- A grammar- and type-directed generator produces well-typed programs, with no
  `Any` and no host effects.
- The test runs each one through the normal preparation and execution path.
- Any runtime type error, internal `indexed IR could not encode` error, or
  checker/runtime disagreement fails the test.
- Fixed seeds, bounded size, and no new dependencies.
- Fold in lane BB's registry-signature probes as a deterministic corpus.

### 4. Diagnostics for common mistakes

A mistake corpus: each snippet is a common mistake, paired with its expected
diagnostic code and a message fragment that names the real cause.

Known gaps:
- `else` on its own line reports "expected expression".
- Interpolating a Unit value reports "cannot convert to one command word".
- A value `match` that misses variants is reported twice, as a warning and as
  an error.

Fix messages until the corpus passes. Seed it from these gaps and from the
mistakes found during this campaign.

### 5. Lowering drift test

Lowering still computes representation-level types (37 `infer_*` uses in
`src/runtime/eval/lower.rs`) and expands spreads itself.

- Add a differential test over the corpus. Every type that lowering computes
  must agree with the checker's published facts for the same expression.
- Delete any remaining computation the checker already publishes.

## Diagnostics and grouping (remaining)

- **Typed diagnostic codes.** Replace string codes (`LINT_CODES`,
  `FIXABLE_CHECK_CODES`, and about 390 scattered `parse.*`/`check.*`/
  `lint.*`/`run.*` literals) with one `DiagnosticCode` enum. Each variant
  carries its summary, severity and fix kind as properties.
  - Diagnostics carry the enum.
  - `--only` parses into it.
  - `lint --list` and `make docs` iterate it.
- **Required grouping for two more forms.** An `if`/`match` expression used as
  an operand, and a pipeline followed by a suffix or operator, must be
  parenthesized. The diagnostic carries a fix, and this repo and the siblings
  are migrated.

## Tooling (remaining)

- **Test speed (running).** `cargo test -p xsht` takes 828 s, mostly in the
  corpus invariance tests. Target under ~90 s. Tests never spawn debug
  binaries; every spawned `xsh`/`xsht` is the release build, with a loud
  failure when it is missing or stale.

- **Small fixes found by the fuzz lane.**
  - The formatter is not idempotent on `fn2(...{p0: if … })`.
  - `unix` module errors lack the NotFound/PermissionDenied facets that fs
    errors now carry.

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

## Docs refresh (remaining)

Landed so far: `README.md`, `docs/user-tour.md` (it absorbs CHAPTER-01), one
merged `docs/SPEC.md`, a slim `ARCHITECTURE.md`, `TESTING.md` (which replaced
TEST-MAP), `core/SYSTEM-REPORT.md`, and the retired docs.

- **Stdlib `template` module (lane D1).** Then `make docs` / `make docs-check`,
  written in XSH with `template`:
  - generate `docs/reference/stdlib.md`, a compact index from
    `xsht api --format jsonl`;
  - generate the CLI reference from `xsht --help`;
  - generate a lint catalog, after adding a one-line summary per code in
    `crates/xsht/src/lint.rs`;
  - render the tour from `docs/templates/user-tour.md`. Each example is a real
    file, `docs/snippets/tour/NN-name.xsh`, and the multi-file example is a
    real project, `docs/snippets/tour/project/`. Portable snippets are run
    during rendering and their stdout is spliced in, so the tour never shows
    stale output; Linux-only snippets are checked but not run;
  - the snippets go through the normal `xsht check`, lint, `fmt --check` and
    `xsht test`, so the markdown block convention and its extraction checker
    are not needed;
  - `make check` fails when re-rendering differs from the committed docs.
- **Formatter small heuristics (after lane HH).** Keep short single-line
  blocks and records that fit the line width. Then make the tour `fmt`-clean
  and require that in `docs-check`.
- **Regex `$`.** Document that it means end of text, in SPEC and the tour, with
  `(?m)` or `.trim()` for line-oriented matching.
- **Parser bugs (after lane HH).** `run ./tool` and `cd /tmp { … }` do not parse.
- **Decided: module names stay reserved as binding names.**

## Final gates

These hold at every landing:
- full native suite on release;
- Rust gates on debug;
- `make check`: release lint, 15 s budget;
- `xsht fmt --check`;
- clippy;
- both sibling repositories;
- one Linux run in the `Dockerfile.test` image at the end.

Then delete this file.
