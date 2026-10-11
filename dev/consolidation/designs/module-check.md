# Design: check a module once

Status: approved by the owner 2026-10-11 (drafted 2026-10-10). Part of workstream 9 of
`../CAMPAIGN.md`. Depends on the one loader in `check-session.md`.

## Problem, measured from source

Static count over this repository's lint configuration (2026-10-10): 1,141
files, 1,051 lint roots, 90 distinct modules. Summed over roots a module is
checked 778 times per pass, so 688 of those are repeats; `core/lib/gnu.xsh`
alone is checked under 234 roots. `xsht check` and `xsht test` repeat the
same work, each entry in its own arena.

Three costs stack:

1. **Every check is at least two whole-bundle passes.** A probe pass checks
   every body to build the effect graph and is thrown away; the final pass
   checks every body again. A call through a module value can force further
   probes.
2. **Bundle-wide preparation walks the whole arena, not the bundle.** In
   `xsht lint` the arena holds every workspace file, so each of 1,051 roots
   walks all 1,141 files several times per pass
   (`prepare_effect_declarations`, `prepare_local_inference`,
   `PreparedConstants::collect`, `prepare_regex_literals`,
   `ReturnInferenceIndex::collect`), and `prepare_local_inference` runs again
   for every module. 762 roots import no module at all and still pay this.
3. **A shared module's bodies are checked once per importing root.**

Which of the three dominates is not known; nothing was profiled. Setup
measures it before any item starts (see "Order").

## What a module's check depends on today

A module's result should be a function of the module and what it imports. It
is not, in three ways, and each is a defect in its own right:

| Dependency | Effect | Fix |
|---|---|---|
| The type of the builtin `args` is `List[Bytes]` when the entry's `main` takes a sole byte rest parameter, else `List[Str]` | intended; SPEC 3.2 | keep; it is the one legitimate input from the entry, with two values |
| A module's statements are checked against the entry's source text, which is sliced by span for fix-hint text and scanned to decide one diagnostic (`check_fmt_dollar_names`) | a module's fix hints, and possibly one diagnostic, depend on an unrelated file | check a module against its own text |
| `qualified_procs`, `qualified_pures`, `qualified_streams` are written by every `use` and never restored, so a call `ns.name(...)` resolves through an alias a sibling module bound | a module can call through an import it does not have, depending on check order | save and restore them per module, as the other eight tables already are |

Two more are in lint, not the checker: a module's lint input is gated on
whether any diagnostic exists anywhere in the bundle of whichever root
happened to claim the module first, and that root is chosen by worker
scheduling. So a module's lint result can depend on thread timing.

Everything else an importer reads goes one way, importer to importee: the
importee's `UserModuleSig`, and for effects the importee's solved summaries.
Imports are acyclic and modules are already checked dependency-first.

## Contract

Observable changes, all corrections:

- A module's diagnostics and fix hints no longer depend on which entry
  imports it, apart from the `args` type.
- A call through a namespace the module did not import is an error
  (`check.unknown-module` or the existing code for an unknown qualified
  name, whichever the checker reports for an unbound namespace today). A
  site in this repository or Laputa that relied on the leak gets its missing
  `use`; the integrator lists each in the handoff log.
- A module's lint result is gated on diagnostics in that module and its
  imports only, so it is the same under every root and every worker count.

Nothing else is observable. Output of `xsht check` and `xsht lint` over this
repository and Laputa is byte-identical before and after each item, except
for differences that fall under the three lines above, which the lane lists.

## Design

**A bundle view.** The arena is immutable and shared. What a check is asked
to check is a separate view: the root's statements, the reachable modules in
dependency order, and the documentation in range. Preparation walks the
view. `xsht lint` workers stop deep-copying the workspace arena, since the
copy existed only to hold a mutated view.

**A module record.** Checking a module produces one value:

- its `UserModuleSig`, with solved effects;
- its diagnostics;
- its entries of every fact table whose keys are sites in the module (the
  fields of `CheckOutput` other than `diagnostics`, `prepared_constants`,
  `function_effect_facts`, `callable_effects`, and
  `embedded_bodies_checked`);
- its effect nodes with their solved summaries and outgoing edges;
- its prepared constants, record constructors, and wire enums.

**A per-module driver.** The checker checks modules in dependency order and
then the root. For each, it prepares that file alone against its imports'
records, runs the effect probe over that file's bodies, solves, and runs the
final pass. An importee's summaries are solved before any importer reads
them, so the repetition for calls through a module value disappears; the
probe remains only because declarations in one file may be recursive or
refer forward. A bundle's `CheckOutput` is assembled from the records in
order, which keeps today's diagnostic order.

**The `args` type is decided first.** It is read from the annotation of the
root's `main` before any module is checked. If the lane finds a case where
the annotation alone does not decide it, modules are checked as `List[Str]`
and checked again as `List[Bytes]` only when the root selects bytes.

**Reuse.** A record is keyed by module and `args` type. Within one process,
`xsht lint`, `xsht check` with several entries, and `xsht test` discovery
load their files through the one workspace loader, so sites have the same
identities under every root, and a module is checked once per key. Records
are shared between workers when the types involved are `Sync`; if they are
not, each worker keeps its own, and a module is checked at most once per
worker.

**Embedded standard modules** are modules like any other and get a record.

## Not covered

- Fix rounds under `xsht lint --fix` parse the edited file into a fresh
  arena and recheck its bundle each round. They do not reuse records.
- A file checked as a root and also imported is checked in both roles; the
  two results differ by design (module-only rules, qualified enum names).
- `xsh` running one script gains nothing from reuse. It is unaffected.
- No cache outlives the process (decision D7).

## Order

1. **Measure** (Setup). Record `StageTimings` and a sampling profile of
   `xsht lint` on this repository at the start commit, split into arena-wide
   preparation, module bodies, and root bodies.
2. **Bundle view.** Preparation walks the bundle, once per check. No change
   in output.
3. **Isolation.** The three fixes in the table and the lint gating, each
   with its test.
4. **Per-module driver.** Records, dependency-order checking, no repeated
   probe across modules. No change in output.
5. **Reuse** in `xsht lint`, then `xsht check` and `xsht test`.

Each step is its own item and is accepted on the byte-identical output rule.
Steps 2 and 3 stand on their own if step 4 is parked.

## Tests

- One module checked under two roots that differ in everything but `args`
  type produces equal records. Added to `tests/sema.rs` beside the
  full-versus-compact comparisons, since it needs the facts and not a
  command line.
- A module that calls through a sibling's import is rejected.
- `xsht lint` over a fixture with a shared module gives identical output
  with one worker and with four, with the fixture ordered both ways. The
  existing four-worker test is the model.
- A check-count probe under `native-tests`, like the existing counter for
  stdlib parses: linting a fixture of eight roots sharing one module checks
  that module once.
- The corpus rule above, run by the integrator after each item.

## Relies on

Checked by the integrator at the start commit; if one is false this design
is parked whole.

- Imports are acyclic: the loader reports a cycle and the check is skipped.
- Modules are checked in dependency order and before the root's
  declarations (`collect_user_modules_arena`, `check_user_module_arena`).
- `check_user_module_arena` clones and restores eight tables and does not
  restore `qualified_procs`, `qualified_pures`, `qualified_streams`.
- The `source` argument passed while checking a module is the entry's text.
- Effect edges cross a module boundary only from importer to importee, and a
  call through a module value reads effects from the importee's signature.
- The lint workspace parses every file into one arena with one `SourceMap`
  and one `SymbolOwner`, so a module has the same spans, ids, and names under
  every root.
- No test today checks one module under two roots and compares its facts.
