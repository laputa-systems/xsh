# Remaining XSH work

The formatter corpus and comment-fidelity repair are complete. The new cases
cover named and spliced arguments, pipeline stage blocks, nested control flow,
result propagation, comments at valid statement boundaries, and authored blank
lines. `cargo test --test integration syntax::` passes. A second `Doc` migration
is not warranted: these changes do not alter another construct family's layout
policy, and `DocRenderer` already owns call-argument layout. `docs/XSHT-FMT.md`
records the contract and limits.

Only the owner-run gates below remain. `AGENTS.md` reserves formatter, linter,
and coverage commands for the owner because coverage runs the unfiltered test
suite. Run each gate, keep its report, and fix a demonstrated behavior failure
before closing the item. No language expansion or downstream reintegration is
in this queue.

- **A02 · coverage.** Run `cargo dev coverage`. Use the report to select one
  interactive or Linux workflow gap with an observable state transition or
  invariant; add the smallest meaningful regression and rerun its focused gate.
  `docs/COVERAGE.md` owns the scope and known low-value remainders.
- **B01 · tooling acceptance.** Rebuild the candidate release `xsht` binaries,
  then run `bench/stdlib-port/tooling.py` against the existing B0 release on
  macOS and in the pinned `Dockerfile.test` Linux image. Complete the
  `xsht lint core/ls.xsh` row with exact status/stdout/stderr parity and the
  original end-to-end budget. Keep raw paired samples beside
  `bench/stdlib-port/results-b01-tooling.json`; its API and check rows already
  pass.
- **E04 · script-backed tooling parity.** Run
  `cargo test -p xsht --test integration cli::copied_xsht_formats_and_lints_script_backed_calls_in_static_and_loaded_modules -- --exact`
  and
  `cargo test --test integration runtime::coverage::runnable_xsh_corpus_is_formatted_and_lints_without_warnings -- --exact`.
  The first gate uses a copied `xsht` with static and dynamically loaded user
  modules that call embedded standard functions; the second covers the runnable
  source corpus. Both invoke owner-only formatter or linter commands.
