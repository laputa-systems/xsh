# W9 assignment-facts proof review: `31661cc8`

## Finding

**GO for this source-reviewed proof checkpoint.** The previous narrow HOLD is resolved: binding `input_index` and direct candidate `OwnerKey` coordinates now require exact nonnegative integers. This GO covers the proof helper and source controls only. It does not accept W9 corpus parity or award any build/test gate credit.

## Review

- The independent assignment-name oracle in `dev/consolidation/w9-publication-assignment-facts.rs` implements the pinned legacy target projection (`Name`, recursive `Field`/`Index`, skip `Env`) and eligible statement/block traversal. It does not call candidate assignment-facts helpers. It uses `CheckBundleView::for_root` to select the root range and reachable modules whose candidate owner keys are being checked; the traversal inside each selected range is independently implemented.
- The new AST-source controls exercise the actual parser and `ArenaProgramBuilder`: target roots/deduplication/environment exclusion; eligible raw-sugar traversal versus expression-owned/defer exclusions; a documented empty root with an explicit source identity; and a separately sourced imported module whose `nested` assignment stays in its own source/range owner. The module fixture sets the root import’s resolved key, adds the parsed child module, and checks two owners with distinct `SourceId`s. These controls are source-reviewed only; I did not compile or run them.
- The proof keeps exact `ArenaRange.start` and `.len`, binds source rows and semantic root/module identity to their source IDs/paths, emits empty owner rows, validates owner counts/order/uniqueness, and compares the complete candidate owner-key and name-set maps. The earlier `start=32` versus 12 statement-node fixture still guards against treating range start as a statement count.
- `ProofBinding.input_index` now rejects `bool`, `float`, and negative values; direct candidate-map owner coordinates also reject booleans and floats. The added Python mutation cases cover these paths. Existing source tests also cover changed names/owner keys, missing empty owners, proof digest and binding changes, and malformed schema rows. Parent reports 19/19 Python checks; I did not run them.

## Evidence boundary

This commit adds source tests and proof parsing/generation but does not establish the 1,690-input comparison. The reported host Cargo sample remains diagnostic only; no pinned or serialized Rust test receipt exists from this review. I ran no builds, tests, captures, or corpus comparisons.
