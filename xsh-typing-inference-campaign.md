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
   new `parse.redundant-parens` error, which has a fix.
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
- Spec first, in `docs/SPEC-TYPING.md`. The current spec text says "Bare Any
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
