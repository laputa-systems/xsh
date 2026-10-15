# W9 compact Debug review

Reviewed `d3b2b2e95d7f650a6d11bf999246e8c53a897b46` in
`.work/wt/consolidation-w9-format`, read-only. No tests or builds were run by
this reviewer.

## Decision

**GO** for the guarded replacement in
`dev/consolidation/w9_publication_adapter_runner.py::derive_helper`: format the
checked publication with compact `Debug` before the existing canonicalization.
The replacement is against the exact pinned helper line and remains guarded by
`_replace_once`.

The three production `derive_helper` callers cover five build roles:

- `build_adapter_harnesses`: reference and candidate;
- `build_derived_reference_harness`: derived reference;
- `build_exact_owner_harnesses`: reference and candidate exact-owner harnesses.

The raw `build_harness` in `w9-publication-run.py:1332-1344` stages the pinned
raw helper directly and does not call `derive_helper`; that legacy raw path is
unchanged. The source-approved helper SHA `2ff5aaa…`, owner-helper SHA
`23fafe5…`, and exact-owner-helper SHA `33c7727…` remain pinned and unchanged.
Derived-helper SHA-256 is already recorded in build receipts. Source schema
continues to choose exact-owner suffix composition: schema 2 appends the raw
exact-owner helper, schema 3 appends its approved projection. Compact formatting
is common to the derived adapter base and does not change that policy or receipt
fields, so no schema/version branch is needed.

Build reuse remains strict: `_reuse_reference_build` requires the fresh helper
hash and bytes. A retained pretty-derived binary therefore needs rebuilding;
it is not silently accepted as the current compact artifact. Historical capture
readers continue to validate recorded helper identities.

## Evidence and limits

Retained evidence under `.work/tmp/w9-memory-profile` shows the pretty run for
input 1655 exited 137 at `memory.max=1073741824`, with no swap and cgroup
`oom_kill=1`. The compact diagnostic run for input 1655 passed at the same
bound with no OOM and cgroup peak 903,163,904 bytes. The same checked object
formatted both ways produced 104,463,580 normalized bytes with identical
BLAKE3. The four focused Rust oracle logs in `build-equivalence-final` pass;
the adapter and exact-owner source-composition tests pin the compact replacement
and preserve the raw/projected suffix checks.

This is bounded evidence for the high-water row and formatting equivalence, not
full 1,690-input W9 cohort acceptance. A fresh full cohort build/capture and
parity gate remain separate.
