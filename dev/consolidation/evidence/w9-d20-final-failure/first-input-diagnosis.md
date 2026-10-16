# W9 first-input mismatch triage

## Finding

The first compared input, index `0` (`bench/consolidation/xsh/directory.xsh`, digest `f2d37baf0324766fad4a2091f5d30be4364a96ab8d41d1e0e7218c3959ee1924`), mismatches in the publication for all four profiles (`default`, `reveal`, `execution`, `check-cli`). Its `kind` and diagnostics are equal between candidate and derived-reference. The existing one-input `_compare_publication_input(0, context)` produced exactly four `AdapterError: common checked facts mismatch` rows after checking capture-derived receipt hashes and exact-owner indexes. The bounded probe did not rerun the comparison. The authorized full comparison later completed. See [the final failure record](README.md).

## Smallest normalized difference

I passed the captured index-0 publications through the controller's existing `w9_publication_adapter.project_publication` normalization, using `current` for candidate and `legacy` for derived-reference with each capture's exact-owner map. For `default`, the normalized common `CheckOutput` fields are identical except that candidate retains one extra top-level field:

```text
candidate.assignment_facts = AssignmentFacts{owners:{AssignmentFactOwner{source_id:SourceId(0),statement_start:32,statement_len:1}:{}}}
derived-reference.assignment_facts = absent
```

The same sole normalized field-set difference occurs for the other three profiles. The raw `default` publication files are candidate `df11f58ed3ee4b77423908161b7d9ecc892184ed5b6fc7e43535e3dccf2a6075` and derived-reference `c130ced803d98097f295932d2625b68b8a105383256af3bd1b95919fa0462857`. The normalized publications differ in a field, so whitespace does not explain the mismatch. The derived-reference publication also matches the immutable original-reference publication for this row.

## Source-backed cause hypothesis

Candidate source `src/sema/check.rs` derives `Debug` on `CheckOutput` at line 536, adds private `assignment_facts` at line 541, and publishes it from `Checker::into_output` at line 1640. That field entered the candidate history in commit `170ceae2d` (`feat(check): add checked assignment name facts`). The derived-reference source is pinned to `1fd9e5f2f366833b786e57d1fc54bcd9a238731e`, based on `e240efad273fe35d66d91f4037b6a8193527da2e`; its `src/sema/check.rs:376-382` has no `assignment_facts`. Controller adapter normalization at `w9_publication_adapter.py:27-35` removes `selected_function_calls` and `checked_control_flow` for the current schema, but leaves `assignment_facts`. This explains the first mismatch: a new private checked-output field is emitted by derived `Debug`, but the legacy comparison projection has no matching fact and the current projection has no rule for it.

This is a schema and publication mismatch candidate. The compact formatter's same-object normalization proof does not cover this cross-source field-set change. No fix is proposed in this triage.

## Evidence and limits

- The [final comparison](comparison/mismatches.tsv) contains the complete 1,690-input result.
- Candidate raw capture: `.work/wt/consolidation-w9-format/.work/consolidation/w9-d20af40/capture-chain-01/adapter/candidate/raw/record-0-default/`.
- Derived-reference raw capture: `.work/wt/consolidation-w9-format/.work/consolidation/w9-d20af40/capture-chain-01/derived/derived-reference/raw/record-0-default/`.
- Bounded probe invoked the existing pure Python index-0 comparison and schema normalizer only; it ran no Docker, native tests, build, or full compare and changed no capture or source data.
