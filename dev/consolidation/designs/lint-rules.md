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
