# XSH design notes

[`SPEC.md`](SPEC.md) owns the language contract. This page records the reasons
behind syntax choices. When this page disagrees with the SPEC, follow the SPEC.

## Choose forms readers can infer

Prefer a short keyword form when it has one obvious meaning.

```xsh
return cached when cached != null
continue unless entry.kind == "file"
guard ready else { return }
```

A reader should be able to guess what the form does and search for it later.
A sugar form must expand mechanically to existing forms with the same
evaluation order, effects, and failures. A form that needs a new runtime
concept is core.

`guard let NAME = EXPR else { ... }` stays core because it declares a binding
in the enclosing block. `tempdir NAME at PATH { ... }` also stays core: it
extends the existing `tempdir` scope and preserves its result behavior.

`with NAME = VALUE { ... }` is a core resource scope. It releases the resource
on every exit and returns a failed release as the scope's `Result`. The
language has no user-defined resource protocol; adding a trait system for this
form would be broader than the feature needs.

## Give each concept one spelling

Two spellings for one operation split the corpus and make code reviews debate
style. When a better spelling replaces an old one, add a lint and an autofix
when the rewrite preserves behavior.

Keep sigils' meanings fixed: `$` and `${...}` interpolate command words, `@`
splices argv, and `?` propagates a result. Do not reuse a sigil or add delimiter
noise to work around a conflict. If no clear spelling exists, record the
problem in `TODO.md` and wait.

## Make syntax searchable

Use a shared stem for related forms, such as `defer` and `errdefer`. Keep API
and stage names such as `par-map` and `sort-by` as their only spellings. Prefer
a keyword or method name over punctuation or word order that gives search no
stable term.

## Require corpus evidence

Measure repetition in this repository and Laputa before proposing syntax. Show
a real before-and-after example and estimate how many sites the form removes.
A form that saves only a few lines or renames an existing idiom does not earn a
keyword.

## Ship migration with syntax

A new form ships with a lint for the old shape. Add an autofix when the rewrite
provably preserves behavior. Otherwise, have the lint explain the manual
rewrite.
