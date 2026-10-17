# Reject unresolved native cleanup

The util-linux and consolidation campaigns share the original native lease.
Cleanup failures retain `cleanup-unresolved:` in that lease and the existing
`native-suite.cleanup-unresolved` marker. A reader that ignores either form
can start another gate while an owned container remains unresolved.

The source at `6d2ced336b6544b1d2912cc01a45f07c78fb41b5` did not consistently
reject those forms. The initial regression produced eleven failed rejection
subcases. Expanding it to the artifacts-only profile entry and the actual
scheduler and comparative defaults exposed nine further subcases after the
first repair. An intervening test-import error received no behavior credit.

The final regression covers normal profile locking, artifacts-only profile
execution, forced frames, the installed-observer source, canonical replay,
scheduler comparison, and comparative measurement locking. All twenty-one
quarantine rejection subcases and seven clear-lease controls pass. Rejections
preserve the lease inode and contents. No live cleanup marker or original
shared lease was changed by these tests.

The focused checks passed:

```sh
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/test_native_quarantine.py
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/test_repository_tmp.py
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/test_profile_cost_selection.py
python3 -B dev/consolidation/winddown/gate-tools/final-performance-gates/test_final_observers.py
python3 -B -m unittest dev.consolidation.test_final_canonical_replay
```

The canonical replay module reports twenty-two passing tests. These checks
used the shared performance guard and disposable synthetic native leases.
They ran no Docker workload, Rust build, native suite, or timing gate.

The repair changes gate admission. Applet source, Rust, Cargo inputs, and the
eleven semantic-review source scopes remain unchanged from the accepted D
focused checks and structural measurement. Full-source and timing gates remain
pending for the successor source.
