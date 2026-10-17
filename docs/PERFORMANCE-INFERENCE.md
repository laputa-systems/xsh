# Inference audit for performance acceptance

The inference checkpoint is accepted as an isolated capability and corpus
result. It is not an integration source. Commit
`15243e97f6fd751016ab28690ada2cadb6fc973a` changes evidence and reproduction
instructions; it contains no compiler implementation. Its 1,040-removal source
view was checked with the protected compiler source
`0fe1469d1808b5f963bd841165af6c10f2c3e93b` and the verified products recorded
in `dev/consolidation/evidence/typing-corpus-host-obligations/`. The implementation
is in that compiler revision's ancestry, not in the checkpoint's own patch.

The frozen cohort contains 9,194 AST sites across 48 loaded files, from eleven
roots: ten XSH roots and Laputa `pm/plan_json.xsh`. It has 693 eligible local
annotations and 2,190 eligible private parameter, return, and effect
annotations. The accepted source view removes 499 locals and 541 internal
annotations. All 149 `Map[...] = {}` annotations and all 202 private effect
clauses remain. Schemas, exported contracts, entry decoding and validation
targets retain their frozen treatment. The historical 95% and 85% goals remain
unmet; no denominator or semantic rule changes.

The source generator and frozen AST inventory already provide the original
annotated and 1,040-reduced 48-file views. From this worktree, use the documented
reproducer in `dev/consolidation/typing-corpus/README.md`: run its `prepare.sh`,
then `cohort.py` validation and the original/reduced check commands. The
reproduction uses the original XSH and Laputa pins and keeps scratch under
`.work/tmp`. Its `check.sh` points at a historical qualified product location;
after integration, substitute the accepted G1 `xsht` product while preserving
the exact view roots. The paired behavior selectors remain in
`dev/consolidation/typing-corpus/witnesses.tsv` and are executed by
`run-witnesses.sh` in the assigned native slot. They cover the original 13
behavior witnesses in both views, including the Laputa build-plan JSON and
corruption witness.

The source-view reproduction commands are:

```sh
bash dev/consolidation/typing-corpus/prepare.sh
python3 dev/consolidation/typing-corpus/view_digest.py \
  .work/tmp/corpus/original .work/tmp/corpus/reduced-final
bash dev/consolidation/typing-corpus/check.sh \
  "$PWD/.work/tmp/corpus/original" "$PWD/.work/tmp/corpus/check-original"
bash dev/consolidation/typing-corpus/check.sh \
  "$PWD/.work/tmp/corpus/reduced-final" "$PWD/.work/tmp/corpus/check-joint"
```

The historical `check.sh` verifies the archived product pair before and after
each check. For a new accepted compiler baseline, preserve those source-view
and output checks while pointing the invocation at the G1-qualified pair.

The practical current-source extension is `dev/performance/corpus/check.sh`.
After G1 accepts a compiler baseline, invoke it with that `xsht`, this XSH
checkout, and the Laputa checkout. It checks the frozen XSH utility roots plus
`wc`, `sort`, and `awk`, then runs focused behavior selectors for each. It
checks Laputa plan and generation modules and runs deterministic generation
and JSON identity tests. The practical extension has no annotation score and
does not replace the frozen corpus.
Run it as `bash dev/performance/corpus/check.sh "$XSHT" "$PWD" "$LAPUTA_ROOT"`.

## Capability and patch order

The checkpoint's accepted implementation line is a set of bounded capabilities
whose last corpus-specific prerequisite is imported pure-call admission in
`0fe1469d1`. Reapply or review that implementation lineage by capability and
preserve its dependencies; do not merge the divergent branch wholesale.

1. Solver and rank-1 representation: ranked row constraints and rigid row
   scope, followed by omitted-annotation preparation and preserving parameter
   and row-level facts. The relevant lineage begins at `39b3b32c0` and
   `62b07b443`; parser retention is separately repaired by `2d90f6d0a`.
2. Scheme collection and checking: capture call/body facts, generalize only
   exported pure rank-1 schemes, partition generalized facts at compaction,
   and reject unsupported body forms before publishing them. Key changes are
   `afceaec9d`, `255a4af6d`, `dfaec0559`, `9ec3483dd`, and `114052481` with
   `28b39aa51`.
3. Private indexed representation: encode schemes, lower only checked generic
   bodies, verify scheme and row evidence, authenticate captured values by
   lexical binding identity, and retain erased generic values through runtime.
   The main dependency commits are `5c9d4f070`, `96ddbe01e`, `340ae7a7a`,
   `658f8a572`, and `feb01ac37`, with subsequent verifier and lowerer repairs
   in the same ancestry.
4. Compositional calls and this cohort: compose the selected direct pure-call
   shape (`18df5d9b3`) and admit imported direct pure calls (`0fe1469d1`). This
   supports the bounded sites in the accepted corpus; host receiver
   obligations are a later, separate capability and are not required to claim
   the 1,040 checkpoint.

This is an ordering guide, not a cherry-pick list. Supporting solver,
publication, verifier, and regression fixes within those commits are required.
Existing production APIs remain unchanged; the schemes and verifier metadata
are private indexed representation.

## Integration conflicts and limits

At the accepted checkpoint, compiler source diverged from current master at
the common base `b235a1a43`; the histories are 485 and 281 commits apart. The
checkpoint source diff spans 72 files under `src/sema`, `src/runtime/eval`,
`src/syntax`, and inference tests. The largest integration conflicts sit in
`src/sema/constraints.rs`, checker declaration/call/module-driver and inference
files, indexed full IR decoding and verification, lowering and indexed
execution, and parser arena/parameter nodes. Current master has independently
changed those same boundaries, so port in capability slices and resolve each
against current ownership and invariants. Retain the current single indexed
executor, semantic verifier authority, and source-facing APIs.

The following obligations are the minimum acceptance floor for a port:

- Re-run the original and reduced eleven-root sets through ordinary checking
  and indexed lowering, with all 48 source transformations validated. Preserve
  all 13 paired native witnesses. The original 1,040 evidence's paired
  `test_system_report_memory_reports_malformed_and_oversized_meminfo_fields`
  covers the two removed `meminfo_issue` parameters in both views.
- Keep exact scheme, row, capture, and lowering tests from the implementation
  lineage. In particular retain `checker_rejects_generic_list_body_before_lowering`,
  `user_schemes_execute_after_frontend_drop_on_both_routes`,
  `inferred_host_replace_executes_after_frontend_drop_on_both_routes`, and
  `inferred_host_nested_split_executes_after_frontend_drop_on_both_routes`.
  The latter host cases apply only if that later capability is ported.
- Keep negative controls for forged row/body evidence, `Any` in row evidence,
  incomplete list contents, captured mutable bindings, unsupported generic
  list bodies, unresolved numeric domains, and ambiguous receivers. Ambiguous
  `Str`/`Bytes.byte_at`, `Record`/`Map.keys`, and `Str`/`Regex.replace` must
  remain ambiguous without local evidence; a caller must not train a callee.
- Keep frontend-drop execution on both ordinary and lowered routes. A
  successful check alone does not establish that the indexed artifact owns
  enough scheme and capture evidence to execute after frontend state is gone.
- Preserve Proc entry decoding, required Proc parameter types, non-Unit Proc
  return rules, declared effect restrictions, and every `Map[...] = {}` record
  interpretation. Individual effect-clause redundancy results do not prove
  simultaneous removal through declared-call cycles; no one of the 202 effect
  clauses is authorized for elision by this checkpoint.

The compact Rust selector names and their raw results are in
`dev/consolidation/evidence/typing-host-obligations/README.md` and its
`*-final.log` files. The six native host-obligation selectors there were still
queued behind the former consolidation closure; require them only if porting
that later receiver-obligation slice. No fresh build, suite, or test was run
for this audit because G1 is not accepted at this branch point.
