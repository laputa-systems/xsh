# Design: lint declarations and one descent

Status: internal design established 2026-10-11. Implements workstream 6 of
`../CAMPAIGN.md`; approved lint contract migrations remain recorded in ITEMS.

## Declaration

One entry owns a rule's code, public listing, default state, source/core view,
required facts, root/statement/expression/pattern hooks, and fix policy. The
table generates listing and dispatch registrations. A disabled rule cannot
produce findings; shared prerequisite facts remain available to enabled
rules that need them. Preserve existing diagnostic ordering and project
configuration semantics.

One descent visits each node and invokes its registered hooks. Context is
the union needed by enabled rules, including callable and stream item scope,
imports, declarations, producer blocks, nested builders, and implicit
receivers. Root declarations and constants retain their established source
order. Scope-local rules run wherever that construct is legal, including
ordinary blocks; no rule maintains a private whole-tree recursive descent.

Use the immediate arena child API for recursive plumbing while retaining
explicit scope transitions and semantic classifiers. Rules consume checked
facts by reference instead of copying callable, type, or annotation maps.

## Probes and fixes

One transport performs checker-backed proof probes with source-owned arena
and declaration facts. Each rule retains its own proposition, acceptance
criteria, span mapping, diagnostic code, and replacement proof. A shared
transport is not permission to treat all failed probes as interchangeable or
to silently drop required facts.

Fixes preserve nominal errors and runtime behavior. Unsafe prefer-fail
rewrites become findings without an unsafe replacement. Propagation removal
handles initializers while retaining grammar-required propagation. Infer
return annotations with import-aware facts. Parenthesis minimization may
remove existing enclosing parentheses as well as those introduced by a
replacement, provided the contextual proof succeeds.

## Acceptance

Keep diagnostic/fix byte comparisons, format invariance, scope tests,
configured-rule behavior, and checked-fact publication oracles. Record every
approved expectation migration. The final table drives `xsht lint --list`;
registration lists, duplicate checked-fact copies, independent recursive
descents, and probe transports are removed. Run repository lint alone for
timing after correctness; defer a stricter threshold to the performance phase.

## Assignment safety facts

Remove lint's recursive assignment prepass. One assignment-name fact owns
root-name projection and selected-source ownership. Checked lint receives its
complete summary from existing checker work. Source-only lint records the same
names during its existing common descent, without invoking the checker or
adding a traversal. Both producers use the same target-root projection.

Produce checked summaries during the existing lexical preparation descent.
That descent visits rejected module statements as well as valid declarations.
Semantic checking alone skips some rejected subtrees and cannot publish the
complete source fact. Keep assignment eligibility separate from lexical
binding identity.

Preserve unresolved assignment names, field and index root names, late
assignments, and conservative suppression for the same spelling in another
scope. Preserve the old collector's statement-region eligibility, including
its exclusion of expression-owned blocks and defer descendants. Establish
parity for these boundaries and for partial or erroneous checked output.
Do not substitute binding identities for the existing spelling-based fact.

Source-only rules that need the completed summary retain conditional
candidates at their original diagnostic positions. Finalize them after the
common descent. Conditions retain the scope visibility observed at the
original hook and test only the completed assignment names. Drop a diagnostic
or remove its fix according to the original rule. Preserve surviving order,
fix arbitration, and behavior when no checked output exists.

Keep checker summaries scoped to their source and prepared entry. Importers
must not train another root's facts. Publish only the narrow checked-fact
interface needed by existing consumers. Do not add linter snapshots, another
rule walk, per-rule assignment scans, or a fallback that fabricates checker
facts. A remaining difference in diagnostics or fixes is a contract conflict,
not an implicit part of this structural repair.
