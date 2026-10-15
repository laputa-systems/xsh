# Compare a derived W9 reference

Use this workflow to compare the candidate with the corrected W9 reference
while keeping the original e240 reference intact. The correction runner pins
the e240 base, derived source commit and tree, allowed paths, and exact
base-to-source patch bytes in `w9_publication_reference_correction.py`. It
also pins the source parent and requires a clean worktree.

The derived source includes the retry correction described by SPEC 8.8. An
expected `Result[T, E]` now supplies `T` to the retry attempt body and `E` to
its failure boundary. `ProcessError` accepts its declared variants and family,
so a retry selector can match process failures without accepting another
error family. The annotated selectors in `tests/xsh/retry.xsh` retain their
nominal error expectations. Four separate typed positive programs and two
mismatched-family programs passed the focused `xsht check` checks. The exact
Rust type unit could not build because the musl host could not compile the
`mini-internal` proc-macro. No red run against parent
`bb29697265b29540f7d3a944a2454d2a4152dcc7` was performed.

Original e240 captures and exact-owner receipts remain unchanged. No fresh W9
capture or full comparison has run with the retry-derived source. Keep the
68-range and 6,760-outcome checks strict.

Run these commands from the XSH repository root. Set the paths to the clean
candidate, its source receipt, the pinned derived reference worktree, and the
original captures and exact-owner receipts:

```sh
RUNNER=dev/consolidation/w9_publication_adapter_runner.py
CANDIDATE_ROOT=/path/to/w9-next-controller-56580
CANDIDATE_SOURCE_RECEIPT=/path/to/candidate-source.receipt
ORIGINAL_REFERENCE_SOURCE_ROOT=/path/to/w9-adapter-ref-e240
DERIVED_REFERENCE_SOURCE_ROOT=/path/to/w9-derived-retry-context-bb
ORIGINAL_CAPTURE_RECEIPT=/path/to/original-reference/capture.receipt
ORIGINAL_EXACT_BUILD_RECEIPT=/path/to/original-reference-exact/build.receipt
ORIGINAL_EXACT_CAPTURE_RECEIPT=/path/to/original-reference-exact/capture.receipt
CANDIDATE_CAPTURE_RECEIPT=/path/to/candidate/capture.receipt
CANDIDATE_EXACT_BUILD_RECEIPT=/path/to/candidate-exact/build.receipt
CANDIDATE_EXACT_CAPTURE_RECEIPT=/path/to/candidate-exact/capture.receipt
CORRECTION_CAPTURE_OUTPUT="$CANDIDATE_ROOT/.work/w9-derived-reference-capture"
COMPARISON_OUTPUT="$CANDIDATE_ROOT/.work/w9-derived-reference-comparison"
```

Choose unused output paths under `CANDIDATE_ROOT/.work` before running the
commands.

Build the derived reference harness:

```sh
python3 "$RUNNER" build-derived-reference \
  --candidate-root "$CANDIDATE_ROOT" \
  --candidate-source-receipt "$CANDIDATE_SOURCE_RECEIPT" \
  --derived-reference-source-root "$DERIVED_REFERENCE_SOURCE_ROOT"
```

Set `DERIVED_REFERENCE_BUILD_RECEIPT` to the build receipt path printed by
that command:

```sh
DERIVED_REFERENCE_BUILD_RECEIPT=/path/from/build-derived-reference
```

Capture the derived reference while reusing the original reference capture and
its exact-owner proof:

```sh
python3 "$RUNNER" capture-derived-reference \
  --candidate-root "$CANDIDATE_ROOT" \
  --candidate-source-receipt "$CANDIDATE_SOURCE_RECEIPT" \
  --derived-reference-source-root "$DERIVED_REFERENCE_SOURCE_ROOT" \
  --derived-reference-build-receipt "$DERIVED_REFERENCE_BUILD_RECEIPT" \
  --original-capture-receipt "$ORIGINAL_CAPTURE_RECEIPT" \
  --original-exact-build-receipt "$ORIGINAL_EXACT_BUILD_RECEIPT" \
  --original-exact-capture-receipt "$ORIGINAL_EXACT_CAPTURE_RECEIPT" \
  --output "$CORRECTION_CAPTURE_OUTPUT"
```

Compare the candidate against all 6,760 derived-reference outcomes:

```sh
python3 "$RUNNER" compare-derived-captures \
  --candidate-root "$CANDIDATE_ROOT" \
  --candidate-source-receipt "$CANDIDATE_SOURCE_RECEIPT" \
  --reference-source-root "$ORIGINAL_REFERENCE_SOURCE_ROOT" \
  --derived-reference-source-root "$DERIVED_REFERENCE_SOURCE_ROOT" \
  --derived-reference-correction-receipt "$CORRECTION_CAPTURE_OUTPUT/correction.receipt" \
  --original-capture-receipt "$ORIGINAL_CAPTURE_RECEIPT" \
  --reference-exact-build-receipt "$ORIGINAL_EXACT_BUILD_RECEIPT" \
  --candidate-exact-build-receipt "$CANDIDATE_EXACT_BUILD_RECEIPT" \
  --candidate-capture-receipt "$CANDIDATE_CAPTURE_RECEIPT" \
  --reference-exact-capture-receipt "$ORIGINAL_EXACT_CAPTURE_RECEIPT" \
  --candidate-exact-capture-receipt "$CANDIDATE_EXACT_CAPTURE_RECEIPT" \
  --output "$COMPARISON_OUTPUT"
```

The capture writes `correction.receipt` with schema 2. It binds the derived
source, build, capture, original capture, and original exact-owner capture by
identity and hash. The comparison writes
`comparison/original-to-derived-reference.tsv` with the first changed outcome
field and both field hashes, or records `none` when no field changed. The
comparison receipt records the capture and correction receipt paths and hashes,
the full outcome count, and the delta-file hash. The comparator checks all four
profiles and grants no diagnostic waivers. A candidate mismatch returns exit
status 1 and appears in `comparison/mismatches.tsv`.
