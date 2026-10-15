# Final native-suite count audit

## Finding

The current acceptance owners specify **nine** full native-suite executions,
not eight. The eight-run subtotal is one canonical normal run, one forced-frame
run, and six scheduler-comparison runs. The ninth is the selected release
opt3 native suite run by the profile-cost gate. That run remains a concrete
correctness prerequisite for forced-frame acceptance; it is not an alternative
profile-selection experiment.

The owner waiver removes the test-profile selection measurement and any fastest-
profile claim. It does not waive the selected release opt3 correctness gate.
`CAMPAIGN.md` says final release evidence still uses opt3 with thin LTO and 16
codegen units (lines 866–869). `ITEMS.md` records that the profile-selection
measurement was waived while the final release opt3 gate remains pending
(line 164). `HANDOFF.md` makes the boundary explicit: the waiver was for the
alternative profile-selection measurement, not correctness or other gates
(lines 404–405).

## Concrete run owners

- **Canonical normal suite: one.** The final-correctness launcher records that
  the complete native suite is owned by `cargo dev gate --batch` and is not
  repeated inside that launcher (`winddown/gate-tools/final-correctness-gates/launch.py:1139`; its earlier note at line 155 says the same suite is intentionally absent).
- **Release opt3 suite: one.** `profile-cost.py` sets `PROFILE_OPT_LEVEL = 3`
  (line 42), iterates only that selected level (line 487), then invokes the
  release native suite (lines 519–528). This is not a multi-profile selection
  run. The forced-frame gate requires a complete profile-cost receipt whose
  selected opt3 result has `native_suite.status == 0`
  (`forced-frames.py:189–215`), so this correctness execution remains required.
- **Forced-frame suite: one.** The forced-frame owner runs the complete suite
  with `xsht test --force-frames` (`forced-frames.py:626–640`).
- **Scheduler comparison: six.** The comparator sets `SUITE_ROUNDS = 3`
  (`native-scheduler-comparison/compare.py:46`) and each round executes both
  reference and candidate suites (`compare.py:1258–1276`): three reference plus
  three candidate runs.

Total: `1 + 1 + 1 + (3 × 2) = 9` serial full native-suite runs. This count does
not include focused tests or partial suite attempts.

## Recommended minimal documentation change

Keep the nine-run count. Add one clarifying sentence to the current status at
`CAMPAIGN.md:1067` and the verification state at `ITEMS.md:292–293`: “The nine
runs are the canonical normal suite, the selected release opt3 correctness
suite required by forced-frame acceptance, the forced-frame suite, and three
reference/candidate scheduler pairs. The owner waived alternative
profile-selection measurement, not the selected release correctness run.” Keep
the existing waiver wording in `HANDOFF.md:404–405`.

If the intent instead is to waive the release opt3 native-suite run itself, the
present forced-frame gate contradicts that intent by requiring its successful
profile-cost receipt. That would require an explicit owner decision and a
reviewed change to the gate contract; it is not a documentation-only count
correction.
