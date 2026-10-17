# Inference audit for performance acceptance

The inference checkpoint is accepted as an isolated capability and corpus
result. It is not an integration source. Commit
`15243e97f6fd751016ab28690ada2cadb6fc973a` changes evidence and reproduction
instructions; it contains no compiler implementation. Its 1,040-removal source
view was checked with compiler source `0fe1469d1808b5f963bd841165af6c10f2c3e93b`,
tree `79fdb956ee01e3bf4cced9b85ff109bcb8b7d662`. The protected local `xsh` and
`xsht` products still match the checkpoint's recorded hashes
`38b848c828591d6aa46a902d0404dfe42298e36cc2a6c6512eb2af9c1f460736` and
`23ca0dd9ccb8b9723f41d199ab41d4a242c90c96277ce83e13b002e67b23a2b2`. The
checkpoint binds these products and source in
`dev/consolidation/evidence/typing-corpus-host-obligations/combined-b045/README.md`;
the products were hash-checked locally. They qualify the historical 1,040
checker result, not the future G1 baseline.

The frozen cohort contains 9,194 AST sites across 48 loaded files, from eleven
roots: ten XSH roots and Laputa `pm/plan_json.xsh`. It has 693 eligible local
annotations and 2,190 eligible private parameter, return, and effect
annotations. The accepted source view removes 499 locals and 541 internal
annotations. All 149 `Map[...] = {}` annotations and all 202 private effect
clauses remain. Schemas, exported contracts, entry decoding and validation
targets retain their frozen treatment. The historical 95% and 85% goals remain
unmet; no denominator or semantic rule changes.

The current `dev/consolidation/typing-corpus/README.md` describes a later
877-removal result. The accepted 1,040 retained table and recipe are the versions
at `15243e97`; replay those files explicitly rather than using the current
`retained.tsv`. The original and reduced source views both contain 14,287
entries, with digests `e3429dcf2abe9524754a28458651443bc56523b50130268a1c1ff44c03e36899`
and `b67b498467a1dfe613573690288a8c514b9c03d0f895a4a3b20e4fd3b55968f9`.
The retained table there is built on the accepted 1,038 set plus two removals
in `sys_mem.meminfo_issue`; it is not the mutable-sets branch's later 877 view.
The current `dev/consolidation/typing-corpus/witnesses.tsv` belongs to the later
877 checkpoint and must not be presented as fresh 1,040 behavior evidence. The
1,038 checkpoint has one paired truncated-source witness, once per view. The
1,040 checkpoint adds the targeted
`tests/xsh/system-report.xsh::test_system_report_memory_reports_malformed_and_oversized_meminfo_fields`
once in each view. That exact selector covers both removed `meminfo_issue`
parameters. The 1,040 receipt records an 11-root check/lowering result and
these targeted executions; it does not record a broad paired behavior matrix.

The source-view reproduction commands are:

```sh
scratch="$PWD/.work/tmp/performance-inference-audit"
mkdir -p "$scratch/original/xsh" "$scratch/original/laputa"
git archive ddc17d3d53c819c343f9ecaa738ac67d59e9033a | tar -x -C "$scratch/original/xsh"
git -C "$LAPUTA_ROOT" archive ca658ce1da9d89c55209c25268dfdf687ff4fdc6 | tar -x -C "$scratch/original/laputa"
git show 15243e97:dev/consolidation/typing-corpus/retained.tsv > "$scratch/retained-1040.tsv"
python3 dev/consolidation/typing-corpus/cohort.py \
  "$scratch/original" --reduced "$scratch/reduced-1040" --retain "$scratch/retained-1040.tsv"
python3 dev/consolidation/typing-corpus/apply_witness_fixture.py \
  "$scratch/original/xsh" "$scratch/reduced-1040/xsh"
python3 dev/consolidation/typing-corpus/validate.py \
  "$scratch/original" "$scratch/reduced-1040" --retain "$scratch/retained-1040.tsv"
python3 dev/consolidation/typing-corpus/view_digest.py \
  "$scratch/original" "$scratch/reduced-1040"
```

The historical checker command is preserved in
`git show 15243e97:dev/consolidation/typing-corpus/check.sh`. It uses the
source-qualified 0fe product pair above. Do not use the current corpus
`check.sh` for this source view: its product path and retained table belong to a
later checkpoint. After G1 accepts integration, check both views with that
accepted pair and retain the exact source and product guards. The source-only
reconstruction and AST validation were reproduced for this audit: 1,040 removals,
499 locals, 541 internal annotations, all 48 loaded files protected, with the
recorded original and reduced digests above. No checker or runtime execution
was performed in this follow-up.

The practical current-source extension is `dev/performance/corpus/check.sh`.
After G1 accepts a compiler baseline, invoke it with that `xsht`, this XSH
checkout, and the Laputa checkout. It checks the frozen XSH utility roots plus
`wc`, `sort`, and `awk`, then runs focused behavior selectors for each. It
checks Laputa plan and generation modules and runs deterministic generation
and JSON identity tests. The practical extension has no annotation score and
does not replace the frozen corpus.
Run it as `bash dev/performance/corpus/check.sh "$XSH_SOURCE" "$LAPUTA_SOURCE" \
  "$XSH_PRODUCT" "$XSHT_PRODUCT" "$XSH_SHA256" "$XSHT_SHA256" \
  "$XSH_ROOT" "$LAPUTA_ROOT"`. Set the source revisions and product digests
from the accepted G1 qualification. The adapter checks the checkout revisions,
both product hashes, required source files, and each check command's file count
and `check` and `lower` stage summary. It confirms `.work/` is ignored through
the checkout's `.git/info/exclude` before creating scratch. It rechecks the
source revisions, clean trees, and product hashes after all commands finish.
Each exact test must select its requested name and report
`1 passed; 0 failed; 0 skipped` before the adapter returns success. The adapter
sets `XSH_BIN` and `XSHT_BIN` to the qualified products and prepends their
directory to `PATH`. Run `python3 dev/performance/corpus/test_check.py` to test
the adapter's qualification guards and exact-output parser.

## Capability and patch order

The checkpoint's accepted implementation line is a set of bounded capabilities
whose last corpus-specific prerequisite is imported pure-call admission in
`0fe1469d1`. Reapply or review that implementation lineage by capability and
preserve its dependencies; do not merge the divergent branch wholesale.

1. Solver and rank-1 representation: ranked row constraints and rigid row
   scope, followed by omitted-annotation preparation and preserving parameter
   and row-level facts. The relevant lineage begins at `39b3b32c0` and
   `62b07b443`; parser retention is separately repaired by `2d90f6d0a`.
2. Scheme collection and checking: capture call/body facts, generalize eligible
   pure rank-1 declarations (including private identity and constrained exported
   functions), partition generalized facts at compaction,
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
