# Consolidate XSH syntax without removing features

**Status:** complete. Phase A produced [the audit](SYNTAX-AUDIT.md); the owner
approved it and decided every item in its
[Owner decisions](SYNTAX-AUDIT.md#owner-decisions). Phase B landed each decided
removal and reservation in its own commit, and the audit's
[Phase B status](SYNTAX-AUDIT.md#phase-b-status) and
[final metrics](SYNTAX-AUDIT.md#final-metrics) record them. Items the owner kept
(C13, D01, D02, D04, D05, D08, D09, D11, D14) changed nothing. Any further
consolidation starts a new audit.

## Goal

Make XSH smaller to read and learn by cutting duplicate spellings, contextual
keywords, and grammar special cases. Keep every capability. A script that works
today must still be expressible afterwards, in no more than one extra line per
site. Features and semantics are out of scope: this campaign changes how things
are written, never what they do.

## Why

The [design principles](DESIGN.md#give-each-concept-one-spelling) already say
"Give each concept one spelling." The language has drifted from that:

- [The lint reference](reference/lints.md) has 74 `prefer-*` lints. Each
  marks a concept with two legal spellings. Adding a lint and keeping the old
  shape has collected duplicates instead of removing them.
- Contextual words such as `until`, `within`, `every`, `backoff`, and `ctx` are
  keywords in one position only.
- Some rules exist only to work around grammar collisions: a handle named
  `until` must be written `wait (until)`, and the `..` of a backoff must be two
  dots with nothing between them.
- Conditional control has several spellings: `if`, `unless`, `guard ... else`,
  and postfix `when` and `unless`.

Every extra spelling is one more thing for a reader, a reviewer, the formatter,
the formal model, and any model writing XSH to learn.

## Rules

1. **One spelling per concept.** Where two spellings coexist, choose the
   survivor and make the other a check-time error whose diagnostic shows the
   replacement. Do not leave it as a lint.
2. **Sugar earns its place against today's corpus**, meaning this repository
   and Laputa. Keep a sugar form only if it removes many real sites and the
   core form would cost more than one extra line or obscure the intent. Report
   the site count for every decision.
3. **Prefer a method over syntax.** If an API can express a form at the same
   clarity, the syntax goes.
4. **No contextual keywords.** A word is reserved everywhere or nowhere. If a
   form needs a word that cannot be reserved, redesign the form.
5. **No rule whose only purpose is a grammar collision.** Remove one side of
   the collision.
6. **Add no new syntax**, including replacements. The survivor must be a
   spelling that already exists. A consolidation that invents a unified third
   spelling adds a concept instead of removing one.
7. **Do not change evaluation order, failures, or results.** Each removal must
   rewrite mechanically.

## Phase A: audit, then stop

Write `docs/SYNTAX-AUDIT.md` with:

- Every keyword, contextual keyword, operator, and statement or expression
  form. Classify each as core, sugar, or duplicate, list its spellings, and give
  corpus counts per spelling.
- Every `prefer-*` lint: its two spellings, the proposed survivor, the
  site counts, and whether the rewrite to the survivor is mechanical.
- Every disambiguation or special-case rule in the [SPEC](SPEC.md) and the
  [grammar reference](reference/grammar.md), with the collision it resolves.
- A ranked removal list that weighs lines and rules removed against migration
  risk. Mark each item "remove", "keep" (with the reason), or "owner decision".
- Starting metrics: reserved and contextual keyword counts, grammar
  productions, `prefer-*` lints, and SPEC line count.

Then stop and wait for the owner to approve the list. Choosing which spelling
survives is partly taste, and that call belongs to the owner. Delete nothing
before approval.

## Phase B: removal

For each approved item, in its own commit:

1. Migrate the corpus. AGENTS.md forbids running autofixers, so make ordinary
   reviewed edits. If a migration is too large to do by hand, stop and ask the
   owner to run a one-off rewrite.
2. Make the old spelling a check-time error with a diagnostic that names the
   survivor and shows the rewritten line.
3. Delete its `prefer-*` lint.
4. Update the SPEC, the grammar reference, and any generated references. Update
   `formal/` only where it models the removed form.
5. Verify with `cargo dev gate` on the touched paths, then the native suite for
   the language area.

Finish by updating the "Give each concept one spelling" principle in
[DESIGN.md](DESIGN.md) to record which old spellings were removed, and record
the final metrics beside the starting ones.

## Constraints

Follow AGENTS.md and the topic owners; the SPEC owns language behavior. Use
narrow gates and do not run formatters. If removing a form would lose a
capability, keep that form, record why in the audit, and move on.
