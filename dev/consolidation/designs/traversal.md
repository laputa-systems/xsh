# Design: immediate arena children

Status: internal design established 2026-10-11. Implements workstream 5 of
`../CAMPAIGN.md`; traversal preserves each consumer's semantic policy.

## API

`ArenaChild` distinguishes statement, expression, pattern, block, binding
target, assignment target, type expression, and builder block IDs. Each arena
domain exposes a `for_each_*_child` function over a borrowed arena and a
closure. These functions enumerate immediate children in stored evaluation
or source order, recurse nowhere, and allocate no work list.

Every node-family match is exhaustive. Extra-table ranges, optional children,
pattern aliases and groups, tag-union field types and wire values, command
arguments, run options, and nested builders are explicit. A builder block is
an immediate child; its separate enumerator visits its entries. Comparison
chains enumerate each logical operand once. Value pipelines expose their
input and call without repeating an inserted hole operand.

`SugarView::Source` exposes surface operands once. `SugarView::Core` exposes
only the lowered expansion child. Each walker chooses its view explicitly;
it cannot accidentally process both forms.

## Consumer ownership

Replace only recursive plumbing with these enumerators. A semantic classifier
continues to match node kinds exhaustively: name declarations, constant
eligibility, effects, exit behavior, mutation, lazy boundaries, and type rules
cannot be derived from merely seeing all children. Consumers keep their own
entry/exit scope actions and intentional stopped descent.

Lexical preparation assigns binding identity while following declaration
order. Its child descent may use this API, but entering a callable, opening a
scope, and deciding which initializer can see which binding remain checker
rules. Do not add a generic visitor trait, a second AST, or an allocated
flattened child collection.

## Acceptance

Unit tests prove exact child identity/order for all domains, source/core
sugar, extra-table fields, builders, run options, and non-duplicated operands.
Every migrated walker retains its observable behavior tests. The wide-match
inventory includes aliases of Expr, Stmt, and Pattern families; behavior
classifiers lose their wildcards through explicit negative cases rather than
through indiscriminate recursive descent.
