# XSH Design Notes

`docs/SPEC.md` §1 lists the properties the language guarantees. This file
records the taste behind them: how new syntax is judged before it reaches the
SPEC. It is not a contract; when it disagrees with the SPEC, the SPEC wins.

## Read like the sentence you would say

Prefer a keyword form that reads as plain English over a sigil, a method
chain, or a configuration argument, when the English form is short and has
one obvious meaning.

```xsh
return cached when cached != null
continue unless entry.kind == "file"
guard ready else { return }
```

`when`, `unless`, and `guard ... else` each read aloud the way the program
behaves; `tempdir scratch at PATH { ... }` follows the same rule, though it
is a core scope and not sugar.
A reader who has never seen the form can guess it, and a reader who has can
grep for it.

Such a form must desugar trivially: it has a short, local, mechanical
expansion into forms the language already has, with the same evaluation
order, effects, and failures. `return x when c` is `if c { return x }`;
`repeat n times { ... }` is `for _ in range(n) { ... }`.
If explaining a form needs a new runtime concept, it is not sugar and must
justify itself as a feature. The implementation takes this literally: the
parser builds a sugar form's expansion beside its operands, the checker and
runtime see only the expansion, and a test compares it with the expansion the
SPEC states (`docs/ARCHITECTURE.md`, "Adding a sugar form"). Postfix `when`
and `unless`, `guard cond else`, `repeat`, and `fail` are built this way.

`guard let NAME = EXPR else { ... }` reads like the others but is not sugar.
It puts a binding in the enclosing block, and the only core statements that
bind there are `let` and `var`, which have no failure branch; the nearest
spelling, `let NAME = EXPR else { ... }`, would be `guard let` under another
name. It stays a core form with its own checking and lowering.

`tempdir NAME at PATH { ... }` began as sugar for a block and was moved out
for another reason: a second spelling of an existing core form must behave as
that form does. `tempdir NAME { ... }` already was a scope that returns a
`Result`, usable as a value; a block that propagates its failures could not
be that, so the fixed-path form became a second head of the same scope.

## One name per concept, no overloaded sigils

Each concept gets one spelling. Two ways to say the same thing (`fs.write(p, x)`
and `p.write(x)`) split the corpus and make code review argue about style.
When a better spelling arrives, the old one gets a lint with an autofix rather
than living on as an alternative.

Sigils carry fixed meanings (`$`/`${...}` interpolate into command words, `@`
splices argv, `?` propagates). New syntax must not reuse a sigil with a second
meaning, and must not introduce delimiter noise (hash fences, doubled
prefixes) to dodge a conflict. When no clean spelling exists, record the
problem in `TODO.md` and wait.

## Greppable beats clever

A reader must be able to find every use of a concept with one search. Name
related forms so they share a searchable stem (`defer` and `errdefer`, not
`defer` and `on failure`), keep stage and API names (`par-map`, `sort-by`)
as the only spelling instead of adding English phrasings that hide them, and
prefer a keyword or method name over punctuation or word order that a search
cannot anchor on.

## Earn every form with the corpus

Changes start from repetition measured in real code: this repository and
Laputa (`../laputa`), which is written entirely in XSH. A proposal states the
before and after on a real site and roughly how many sites it removes. Forms
that would save a handful of lines, or that only rename an existing idiom,
are not worth a keyword.

## Migration is part of the feature

A new form ships with the lint that finds the old shape and, wherever the
rewrite provably preserves behavior, an autofix. An autofix never changes
behavior; when it cannot prove that, the lint explains the manual rewrite
instead.
