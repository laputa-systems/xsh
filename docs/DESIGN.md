# XSH design notes

[`SPEC.md`](SPEC.md) owns the language contract. This page records design principles
and the reasons behind syntax choices. When this page disagrees with the SPEC, follow the SPEC.

## Purpose

Make systems programming as immediate as shell scripting, with behavior explicit
and checkable enough to reason about before execution. The aim is not the fewest
characters or a replacement for every general-purpose language. It is fewer
independent assumptions that must hold for an operation to be correct.

Structured data and structured operating-system state belong in the same
programming model. Paths retain native bytes; processes, streams, resources and
host observations should not become text merely to pass between XSH components.
Keep presentation at command boundaries and reuse the underlying typed operations
in scripts, services and interactive tools. External programs remain legitimate
interoperability boundaries, not failures of the design.

## Keep three responsibility layers

These are responsibility boundaries, not a demand for new crates or a rewrite.
[Architecture](ARCHITECTURE.md) maps their current implementation.

| Layer | Owns | Must not absorb |
|---|---|---|
| Language core | Types, values, control flow, effects, errors and resource/ownership semantics | Per-command policy or a new type system for each data format. |
| Systems substrate | Minimal trustworthy process, filesystem, stream, authority and structured-data boundaries | Entire utilities disguised as host intrinsics. |
| XSH userspace | Commands, shared libraries, administration, services, diagnostics and applications | Duplicate host guarantees or render-and-reparse protocols between native components. |

Grow the userspace predominantly in XSH. Stabilize and simplify the layers
beneath it; a language capable of implementing coreutils need not put all of
coreutils into its runtime. A missing host primitive should serve a concrete
consumer and remain narrow enough to reuse. Existing architecture rules govern
measured native acceleration. Mature external engines can be the right boundary
for cryptography, clock discipline or storage repair; purity is not a substitute
for reliability.

## Prefer architectural subtraction

> The quality of a systems language is measured not by how much complexity it
> can express, but by how much complexity it makes unnecessary.

Preserve useful capability while removing duplicated responsibilities, mandatory
intermediate representations, hidden conversions and independently maintained
copies of the same invariant. Less code is not sufficient if it hides semantics,
weakens validation or creates a surprising fallback. Conversely, a small explicit
adapter can be better than a clever universal abstraction.

For a substantial addition, identify its real consumer, the machinery it removes,
the existing owner of each invariant, and the smallest boundary that must change.
Show a before/after workflow and the failure behavior, not just a feature count.
Prefer two consumers sharing an ordinary XSH library to two commands growing
special compiler/runtime paths. Keep checked facts authoritative across checking,
lowering and execution rather than making every stage solve the same question.
Keep independent verifier authentication and validation at trust boundaries;
those are not duplicate responsibilities to delete. Distinct representations
with distinct proof/execution contracts can be necessary. Remove gratuitous
translation, not the separation that makes a boundary checkable.

Structured-data codecs should project into the existing value/schema model and
report representation limits explicitly. They do not each earn a schema language,
validator or coercion rule. The current [Deser campaign](../campaign-deser.md)
is a planned example of this direction, not proof of completion: it owns a
seven-codec cutover, direct language values, limited host-tooling DOM use and
explicit lexical conversions. This philosophy neither expands that inventory
nor launches the campaign.

Do not add a framework, daemon, public abstraction or new syntax solely to unify
hypothetical future consumers. Retire superseded paths when a migration is
proved; retain necessary compatibility only as an explicit contract. Architectural
subtraction is a review criterion for each delivery, not authority for an
unbounded repository-wide rewrite.

## Earn trust at the host boundary

Conceptual coherence, expressive coverage, implementation economy and operational
trustworthiness are separate questions. Broad APIs or elegant types do not prove
the last two. Review dependency/build/startup cost, steady-state memory and
performance alongside the responsibilities and representations removed.

Effects describe host interaction; they are not an OS sandbox or a grant of
access to particular paths. A typed path is not race-free pathname confinement.
A process number is not stable ownership. Atomic visibility is not crash
durability. Specify and test the actual boundary rather than borrowing a stronger
claim from the language's surface.

Keep pure policy and narrow host operations separable so deterministic fixtures,
failure injection and reduced replays can explain a real failure. Pair these with
actual OS/VM tests for partial writes, filesystem races, process identity,
cancellation, clock discontinuities and resource cleanup. Mock agreement is not
host conformance; a formal model is not proof that the Rust/native adapter
implements it. State exact proof scope, unsupported behavior and unverified
platforms. Use existing gates and respect paused campaigns rather than creating
another verification framework.

The stable endpoint is an unsurprising core and substrate supporting an expansive
userspace. New capability earns its place when doing the right thing needs less
machinery without acquiring a new category of surprises.

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
