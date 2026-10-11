# Design: one check session

Status: approved by the owner 2026-10-11 (drafted 2026-10-10). Part of workstream 4 of
`../CAMPAIGN.md`.

## Rule

The check diagnostics of a program are a function of the program and its
module roots, and never of the tool that asked. Tools differ only in which
stages they run, which severities they print, and which make them fail.

## Today

| Entry point | Loader | Options set | Fails on | Prepares for execution |
|---|---|---|---|---|
| `xsh` | `src/loader.rs` | `embedded_bodies` | an error; warnings are dropped unless an error prints them all | yes |
| `xsht check` | `src/loader.rs` | `reveal_types`, `migration_diagnostics`, `embedded_bodies` | any diagnostic | yes |
| `xsht test` | `src/loader.rs` | `embedded_bodies` | any diagnostic in the file | yes |
| `xsht lint` | its own `WorkspaceLoader` | none | a check error that is not a migration or spelling finding | no |
| `Checker::check_arena` in process | none | none | caller decides | no |

Three disagreements follow, all listed in `TODO.md`:

1. `migration_diagnostics` turns on three warnings (two for a flattened
   error handler or translation, one for `ProcessError(...)` used as a
   constructor). Only `xsht check` sets it and `xsht check` fails on any
   diagnostic, so it rejects a program every other entry point accepts.
2. `compact.main-missing-spread` is raised while preparing the program for
   execution, so `xsht lint`, which does not prepare, never reports it.
3. A `use NAME` that resolves to nothing is `check.unknown-module` from the
   in-process check and `parse.module-read` from a tool, because the loader
   fails first and its parse diagnostics end the run before any check.

## Contract

**One set of check diagnostics.**

- `CheckOptions::migration_diagnostics` is removed and the three warnings are
  always computed. `xsh` still drops warnings, so no script prints anything
  new. `xsht test` and `xsht lint` now report them, as `xsht check` does.
- `reveal_types` and `embedded_bodies` decide which facts a check publishes.
  They must not change the diagnostics a check reports for user source. A
  corpus test holds this: every repository source is checked under each
  combination and the diagnostics are equal.
- `interactive_commands` belongs to `xshi` and is not part of this rule.

**A program property is reported by the checker.**

- `compact.main-missing-spread` is decided from the source alone (a
  `proc main` with a fixed parameter that cannot bind script arguments), so
  the checker reports it and every tool sees it. The code keeps its name:
  renaming a published code is not worth a migration.
- `compact.cli-default` moves the same way if its test reads only the source
  program. The lane confirms that from the code; if the test reads anything
  produced by lowering, it stays where it is.
- The other `compact.*` codes stay with preparation. `compact.cli-args` and
  `compact.main-args` depend on the arguments of one run. The rest report a
  construct the indexed form cannot encode, which is a limit of the
  implementation and not a property of the program. `xsht lint` does not
  prepare, and `docs/XSHT.md` says so.

**An import that resolves to nothing has one code.**

- When no candidate file exists for `use NAME` and `NAME` is not a standard
  module, every entry point reports `check.unknown-module`, with the
  candidate paths the loader searched as a label. The loader records the
  unresolved import and the check continues with the checker's recovery
  type, as the in-process check does today.
- `parse.module-read` remains for a candidate that exists and cannot be
  read, or is not UTF-8.
- Existing tests that assert `parse.module-read` for a missing module are
  rewritten to assert `check.unknown-module`. This is the one sanctioned
  change to existing tests in this design; the integrator lists each in the
  handoff log.

**What each tool does** is stated once, in `docs/XSHT.md`:

| Tool | Stages | Prints | Fails on |
|---|---|---|---|
| `xsh` | load, check, prepare, run | errors, and warnings only beside an error | an error |
| `xsht check` | load, check, prepare | everything | any diagnostic |
| `xsht test` | load, check, prepare, run tests | everything | any diagnostic in the file |
| `xsht lint` | load, check, lint | check errors and warnings beside lints; a migration finding under its lint code | as today |

## Internal shape

Not contract; the lane chooses names.

- One constructor builds a check from a loaded program, used by all four
  tools and by the library entry the Rust tests call. It takes the facts to
  publish, not a set of behavior switches.
- One loader. The workspace loader in `xsht lint` and the loader in
  `src/loader.rs` become one, with a single root as the case of one root. The
  campaign's module-check design depends on this.
- `CompactDeclCollector` stops reporting declaration diagnostics the checker
  already reported, and reads the checked declarations.

## Tests

- The option-equality corpus test above.
- For each of the three disagreements, one native test in
  `tests/xsh/tooling-check.xsh` or `tooling-lint.xsh` that runs `xsht check`,
  `xsht lint`, and `xsh` on the same file and asserts the same code from each
  tool that runs the stage.
- The Rust tests listed under "Tests still in Rust" in `TODO.md` as kept
  only because of these disagreements are re-examined; one that no longer
  needs a Rust boundary moves to a native test.

## Relies on

Checked by the integrator at the start commit; if one is false this design
is parked whole.

- `CheckOptions` has exactly the fields `interactive_commands`,
  `reveal_types`, `migration_diagnostics`, `embedded_bodies`.
- `migration_diagnostics` is read at three sites, all in
  `src/sema/check/call.rs`, and all three emit warnings.
- `xsh` reports check diagnostics only when one has error severity.
- `compact.main-missing-spread` is raised from evaluator preparation, from a
  test of the root `proc main`'s parameters.
- `xsht check` on this repository and on Laputa reports nothing, so no
  source here carries one of the three migration warnings.
- The loader reports a module it cannot find as `parse.module-read` and
  parse diagnostics end `xsht check` before the check runs.

## Out of scope

Checking a module once (`module-check.md`); any change to severities; new
diagnostic codes.
