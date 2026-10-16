# Acceptance follow-up on 8d

Canonical source `8d0d6362418a6ab1b58582cf5574a7e0f4ac3542`, tree
`08769a1d438d5d93f55bb21e7a4dbe8051f7b966`, exits 3 at the XSHT desugar
stage. Integration completed 223, auxiliary 23, symbol 1, PTY 58 passed with
three ignored, and debug 747 passed with three ignored. XSHT reports 115 passed,
one failed, and five filtered. The failure is original
`core/tests/test-df.xsh::test_df_reports_filesystem_capacity_counters`: observed
192512 instead of 65536. Its transformed counterpart passes. No later stage,
full native suite, or canonical suffix ran.

Source checks match before and after. There were no OOM or swap events, cleanup
resolved, and the canonical container is absent. The `8d` bootstrap produced
source-matched `xsh` prefix `d058`. The tracked driver enforces resource and
command bounds, swap, cleanup, read-only source, and shared lock. Raw canonical
receipts are under `.work/tmp/canonical-8d0d63624`.

The stable copied-artifact suffix rerun for `a22` passes `xsht check` with
1,526 files, lint with zero findings and 113 notes, metrics check, and docs/tour
with three passing tests. It uses immutable copied products because the shared
`xsh` product changed from prefix `a911` to `d058` during bootstrap. Detailed
receipts are under `.work/tmp/consolidation-main-suffix-a22-stable`.

The corrected `8d` driver passes 22 tests as root. The earlier `9a` replay
attempt raised `NameError` before Docker and ran no suites; it is separate from
the canonical result. No release run, performance timing, full native result,
or functional close is claimed.
