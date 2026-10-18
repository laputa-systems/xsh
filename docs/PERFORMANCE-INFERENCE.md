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
directory to `PATH`. Set `XSH_LAPUTA_CORPUS` to a Laputa checkout, then run
`XSH_LAPUTA_CORPUS="$LAPUTA_ROOT" python3 dev/performance/corpus/test_check.py`
to test the adapter's qualification guards and exact-output parser.

## Retained-site source classification

The denominator remains the frozen 9,194-site inventory at `15243e97`. Its
1,843 retained eligible rows are the exact `retained.tsv` at that checkpoint;
the current 877 table is not a substitute. The rows include 758 Proc signature
restrictions (549 non-default parameter types and 209 explicit non-Unit
returns), 149 purposeful empty-Map annotations, the local annotation at site
3598, and 202 private effect clauses. The 149 empty Maps preserve the requested
distinction between a Map and an empty record. Site 3598 is a local type
annotation for a nonempty Map literal; the eligibility inventory records it as
`eligible-local`, and its source evidence says erasure leaves an unchecked
dynamic Record. It is a local Map-selection requirement. The accepted 1,040
README at `15243e97:dev/consolidation/typing-corpus/README.md` gives the source
context.

Source review surfaced 18 additional retained sites but does not establish a
single cause for them. Sites 2332, 2543, 3406, 3407, 3476, 6436, 6564, 7427,
7822, and 7823 join plain Null with a concrete type without an Optional witness;
whether those joins should infer an Optional remains unresolved. Site 2285
retains `MonitorBattery` field context, and site 6338 depends on a later USB
endpoints producer; source order and intended context have not been separated
from an inference limitation. Sites 973 and 3914 need nominal enum return
context, but the evidence does not establish that inference should choose a
variant without that context. Sites 9111 and 9112 are outside the checked
membership body grammar, which records a capability boundary without proving
that the annotations are semantically unnecessary. Keep these 16 sites pending
with those source facts attached. Sites 3599 and 3665 need a Str anchor for
`replace`, which also accepts Regex; the accepted negative-control contract
requires this call to remain ambiguous without local evidence, so these two are
semantic restrictions. Site 2850 is an unavailable-information case: its
parameter is used only in interpolation, which does not constrain its type.

The retained table records observed diagnostic families, including `infer-return` (288),
`local-inference` (232), `dynamic-boundary` (70), `type-mismatch` (66),
`unknown-method` (56), and `try-context` (37), with smaller families for
iterators, result fallback, display conversion, indexing, assignment targets,
and other checks. A diagnostic identifies where erasure currently fails; it
does not by itself establish whether the missing fact is unavailable, a
semantic restriction, or a specific inference relationship. Keep those rows
pending source-level witnesses rather than assigning a cause from the code
alone.

All 202 effect clauses remain pending joint proof. The accepted source lint
records individual equality and rewrite-equivalence results for 142 clauses;
the other 60 have no retained per-site effect fact or unknown-call chain. The
60 rows group as 42 in `dev/system_report_check.xsh` (38 `compare_live_*`
procedures and four helpers), ten across Laputa's `pm/` modules, and eight in
`core/wc.xsh`, `core/lib/system_report_live.xsh`, `core/lib/textio_a1.xsh`,
`core/lib/text_a2.xsh`, and `dev/stage.xsh`. These source groups show host
operations and `Result` handling, but do not establish whether the missing
proof reflects an intentional wider caller bound, an unknown summary, or an
inference relationship. For example, `compare_live_identity` at line 21766
uses filesystem operations, `time.now`, `process.run`, and propagated errors;
`pm/plan.xsh`'s `require_supported_target` constructs and returns `Err` without
`?`, so its `[error]` clause cannot be called required from the body alone.
Neither example proves intent or a checker defect.

The effect-graph behavior described here is from the accepted implementation
source commit `0fe1469d1808b5f963bd841165af6c10f2c3e93b` (tree
`79fdb956ee01e3bf4cced9b85ff109bcb8b7d662`), not the current master source
view. Its `src/sema/check/infer_effects.rs` records resolved calls as edges;
edges to explicitly declared callees keep reading those clauses. The
redundancy proof unclauses one declaration at a time, and its own-contract
traversal stops at other declared clauses. This cannot prove a joint deletion
through mutually supporting clauses, but the checkpoint does not establish
that such a cycle exists. The existing recursive-effect test covers an
inferred `even`/`odd` fixed point, not simultaneous removal of explicit
contracts.

Opaque `Proc.call` and `Pure.call` are assigned unknown effect contracts in that
source view. A lexical scan of the 22 frozen clause-bearing files found no
`.call(` use, so that dynamic-call restriction is not supported as a
cohort-wide explanation. Callable schemes carry inferred-effect metadata, but
no retained output connects the 60 cases to scheme publication or a recursive
generic boundary. Keep all 202 clauses and the 2,190-site internal denominator;
the 142 single-clause results earn no joint-removal credit. The next evidence
needed is per-site effect facts and resolved call edges for the 60, followed by
a joint solve only for source-confirmed dependency components. The exact list
of the 142 lint findings is at `15243e97` in
`dev/consolidation/evidence/typing-corpus-host-obligations/individual-effects-sites.tsv`.

The accounting is 910 semantic restrictions/selections (758 Proc contracts,
149 empty Maps, site 3598, and the two ambiguous `replace` receivers), one
unavailable-information site, 730 checker-restored rows pending cause review,
and 202 pending effects: 1,843 retained eligible sites. This classifies 911
sites from direct source evidence; the other 932 remain pending. Of the 730
checker-restored rows, 16 have the source facts above but unresolved causes; the
other 714 have only diagnostic-family evidence recorded in the retained table.

The frozen local target cannot be reached while preserving the 149 required
empty-Map locals and site 3598: at most 543 of 693 locals (78.35%) could be
removed, even if every other retained local became inferable. The 758 required
Proc signatures cap internal removals at 1,432 of 2,190 (65.39%), below 85%;
retaining all 202 effect clauses pending joint proof lowers that ceiling to
1,230 of 2,190 (56.16%). These ceilings preserve the frozen denominator and
make the existing 95% local and 85% internal goals unattainable under the
established contract restrictions.

## Practical source-view preparation

The frozen inventory covers the selected 48 files but contains no spans for
`core/sort.xsh`, `core/awk.xsh`, or Laputa `pm/generation.xsh`. The existing
`cohort.py` must not be treated as inventory for these files. The practical
corpus adapter already checks `wc`, `sort`, and `awk`, plus Laputa plan and
generation modules, but that is a behavior/checking corpus without annotation
removal credit. Before I1 accepts the compiler baseline, do not transform these
files or claim runtime behavior for a reduced view.

After I1, archive the exact qualified source commits into `.work/tmp`, retaining
whole trees so imports resolve, then copy each `original` tree to a `reduced`
tree. Extend the source inventory with exact AST spans for eligible annotations
in the four practical roots, preserve their source commit and denominator, and
apply only those selected spans to the reduced copies. Run the existing source
validator against both copies before using the corpus adapter with the accepted
products. The frozen 1,040 `cohort.py` recipe remains a separate baseline and
must use its historical source commits and retained table; combine its result
with practical files only as an explicitly labeled extension, never by
silently changing the frozen inventory.

The isolated Laputa source view used by the adapter test is
`639184e65a712b9d33fa5033e045f8913cd04a5a`. Its adapter test requires explicit
`XSH_LAPUTA_CORPUS`, so set that variable to the isolated checkout. Do not use
the canonical Laputa checkout as an implicit fallback.

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
   supports the bounded sites in the accepted corpus. The accepted compiler also
   contains the host receiver obligations, generic body sequences, mutable Set
   inference, Optional joins, unsigned evidence, and constant-origin repairs
   listed below. Those changes precede `0fe1469d1` in its ancestry.

This is an ordering guide, not a cherry-pick list. Supporting solver,
publication, verifier, and regression fixes within those commits are required.
The scheme encoding and verifier metadata are private indexed representation.
The port also changes first-party frontend types and registry receiver metadata.
Those contract changes need the checks listed below.

## Integration conflicts and limits

The source audit below compares integration freeze `ea73ca4fb` with accepted
compiler `0fe1469d1808b5f963bd841165af6c10f2c3e93b`. Their common base is
`b235a1a43d06e44580fde4c1b8c58ff27a17ca78`. Commit counts measure history,
not source conflicts. At that freeze the histories have 505 and 285 commits
above the common base.

The integration freeze has no changes from the common base in `src/sema`,
`src/runtime/eval/indexed`, or `src/syntax`. Its entire `src` and `crates` diff
changes 14 files. Those files contain IPC support, runner and runtime stats,
tooling fixes, and IPC registry entries. The accepted compiler changes 77
files under `src` and `crates`, with 33,809 insertions and 2,837 deletions.
The only overlapping paths are `crates/xsht/src/lint.rs`,
`crates/xsht/src/lint_callable_alias.rs`, and `crates/xsht/src/lint_probes.rs`.
Their hunks do not overlap. The accepted changes adapt omitted annotations
and generic module exports. The integration changes preserve lint findings
when the checker reports redundant parentheses.

Both read-only patch checks return status 0 at `ea73ca4fb`:

```sh
git diff b235a1a43 0fe1469d1 -- src crates | git apply --check
git diff b235a1a43 0fe1469d1 -- src crates tests | git apply --check
```

These commands apply nothing. They establish textual compatibility, not
compiler correctness. Repeat them against the accepted G1 integration source
before applying any patch. Keep the current IPC, stats, and lint hunks.
`Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, and `formal/` have no changes
between the common base and the accepted compiler. The compiler's
`formal_bridge.rs` does change to recognize `ExprUInt`. Keep that adapter
change and recheck its finite controls without replacing the Lean source.

The earlier claim that master independently changed the same compiler
boundaries was incorrect. A whole-branch merge remains inappropriate because
it would also import unrelated source, corpus, tooling, and campaign history.

### Bounded I1 integration

I1 implementation waits for G1 acceptance. This source-only audit applies no
compiler patch and runs no build or runtime test. After G1 accepts,
use the following order within the single compiler lane:

1. Recheck the source diff against G1's exact accepted commit. Review the
   final compiler delta by the four capability groups above. Prefer the
   accepted final `src` and `crates` delta over replaying hundreds of repair
   commits. The final delta already contains the supporting verifier repairs.
2. Include all 20 changed test files from the accepted compiler. Review the
   `docs/templates/SPEC.md` delta alongside each behavior change and update
   its generated page through the existing documentation command. Keep
   unrelated documentation and corpus changes out of the implementation patch.
3. Apply the reviewed compiler, tooling adapters, tests, and SPEC changes as
   one dependency-complete candidate. Most capability boundaries share files.
   Splitting those files into intermediate runnable versions creates new
   compiler combinations that the accepted checkpoint never qualified.
4. Run the targeted checks below, then the original and reduced corpus.
   Run the combined I1 regression boundary once after the focused checks pass.
   Freeze a new source-qualified product pair before any artifact experiment.

The final compiler delta is a practical starting point, not proof of a minimal
semantic subset. In particular `a972c8d13` is an ancestor of `0fe1469d1`.
`src/sema/check/host_obligations.rs` and the concrete registry receiver
metadata already exist in the accepted source. `d30b05abb` adds generic pure
body sequences. `7491228ab` and `020f9c0c7` add mutable empty Set inference.
`0ee3e2090` and `0481479e7` change Optional joins. `21f55d848` adds checked
unsigned argument evidence. `b045485b8` authenticates folded values and
constructor origins. Removing those capabilities would create a different
compiler and requires its own dependency review and qualification. Do not
label them later optional additions to this source checkpoint.

The source-facing contract changes include omitted pure parameter annotations,
rank-1 pure exports, and the accepted local inference extensions. Proc
parameters, Proc non-Unit returns, explicit effect contracts, and empty record
interpretation retain their restrictions. First-party frontend consumers must
adapt from `ArenaParam.ty` and `ty_defaulted` to `annotation: Option<TypeExprId>`.
`ArenaProgramBuilder::push_params` changes its parameter tuple accordingly.
`ModuleExportType` gains `GenericPure { optional }`. Registry `receiver_ty`
metadata changes from `None` to a concrete type for eleven instance receiver
domains. Int and Record remain blocking domains. Preserve the current IPC
registry additions while changing those receiver templates.

The private IR changes include callable schemes, body and call facts,
lexical binding identities, Let annotation evidence, and `ExprUInt = 158`.
Existing instruction tags keep their ordinals. `FullVerifier` authenticates
scheme binders, rows, actual producer values, captures, writers, and call
substitutions before execution. `TypeScheme` owns the finalized callable
contract. Compact module facts retain that same immutable scheme instead of
rebuilding it from a ground signature. The executor must not perform inference
or consult a retained frontend.

The current `FullProgram` is an in-process private representation. This port
adds no persisted format version or cross-version loading contract. Qualify
its scheme and schema together by the candidate source revision and matched
products. Any later persisted artifact needs an explicit format version and
rejection checks before decoding. Reusing a historical harness or unchecked
scheme payload does not qualify the candidate.

### Source-review controls before I1 acceptance

Independent review of `0fe1469d1808b5f963bd841165af6c10f2c3e93b`
identified two cases to reproduce on the dependency-complete port. These are
source findings, not executable failures or accepted fixes.

- In `full/schemes.rs`, `intern_scheme_schema` computes a parent's instance
  range after recursively interning children. A nested nominal schema may
  therefore give parent and child overlapping ranges. Exercise a generic
  callable with an annotated `List[Item]` parameter and a separately inferred
  parameter, then require successful verification and execution. Cover nested
  return schemas too.
- In `full.rs`, the unsigned literal sign check appears to depend on an
  optional checked-expression fact. Extend
  `full_verifier_preserves_unsigned_integer_generic_arguments` by removing
  that fact and forging a negative UInt payload; require rejection. Exercise
  the producer path too. A literal tag's invariant must hold without optional
  evidence.

Existing constructor-origin tests mutate type facts. They do not establish
rejection of coordinated origin-and-type forgery. Keep that coverage limit
explicit rather than claiming universal authentication from those tests.
Preserve the current timed-stream identity and cleanup contracts when adapting
the overlapping evaluator and producer files.

### Focused I1 checks

Reuse G1's pinned container, kache configuration, target mount, and dependency
artifacts after its exclusive build slot is released. Rebuild changed crates
for the candidate. G1 products cannot qualify changed compiler source.
For schema changes, start with the existing debug checks for `xsh --lib` and
then `xsht --lib`, with the pinned musl target and `--all-features`.
Do not start a workspace build or a release build during this loop.

Build the debug root-lib test harness once inside the pinned container:

```sh
cargo test --locked --offline --target "$TARGET" -p xsh --lib --all-features --no-run --message-format=json-render-diagnostics
```

When tooling adapters change, build only their library harness with the same flags:

```sh
cargo test --locked --offline --target "$TARGET" -p xsht --lib --all-features --no-run --message-format=json-render-diagnostics
```

Record each emitted executable path. Set `HARNESS` to the candidate root-lib
executable for the selectors below.
For each selector, run `"$HARNESS" --exact SELECTOR --nocapture` and require
exactly one pass with zero failures or skips. Never reuse the hardcoded old
harness path in the historical evidence scripts.

The initial exact selectors are:

- `sema::constraints::tests::failed_unification_rolls_back_rank_binding_and_path_compression`
- `sema::constraints::tests::row_occurs_rejects_a_recursive_tail`
- `sema::constraints::tests::any_bearing_row_binding_fails_and_rolls_back_prior_field_constraints`
- `sema::constraints::tests::type_scheme_generalization_preserves_open_record_rows`
- `sema::schemes::tests::row_evidence_rejects_any_and_call_instantiations_require_valid_evidence`
- `sema::check::module_driver::tests::recursive_inferred_parameters_share_provisional_types_in_both_orders`
- `sema::check::module_driver::tests::recursive_parameters_are_solved_without_caller_evidence`
- `sema::check::module_driver::tests::captured_mutable_bindings_are_not_generalized`
- `runtime::eval::lower::checked_constructor_fact_tests::checker_rejects_generic_list_body_before_lowering`
- `runtime::eval::indexed::full::tests::user_schemes_execute_after_frontend_drop_on_both_routes`
- `runtime::eval::indexed::full::tests::full_verifier_rejects_forged_generic_rows_and_body_facts`
- `runtime::eval::indexed::full::tests::full_verifier_checks_generic_list_argument_contents`
- `runtime::eval::indexed::full::tests::full_verifier_authenticates_capture_owner_and_defining_writer`
- `runtime::eval::indexed::full::tests::full_verifier_authenticates_unsigned_runtime_check_evidence`
- `runtime::eval::indexed::full::tests::generic_pure_verifier_rejects_forged_host_plans_slots_and_types`

Then select every test named by
`git show 0fe1469d1:dev/consolidation/evidence/typing-host-obligations/run-exact.sh`
against the same new harness. Its 36 selectors include the receiver ambiguity,
rollback-on-panic, registry parity, late anchors, and frontend-drop controls.
Those checks are required for the accepted final compiler delta.

Build the matched verification binaries once for native checks. Use exact
selectors during iteration, including:

- `tests/xsh/rank1-inference.xsh::rank1_exported_identity_instantiates_each_import_root`
- `tests/xsh/rank1-inference.xsh::rank1_generic_function_cannot_be_stored_as_typed_callable`
- `tests/xsh/typing-generic-body.xsh::test_typing_generic_body_keeps_host_ambiguity_and_effect_rejections`
- `tests/xsh/typing-imported-pure.xsh::test_imported_required_pure_calls_match_local_and_module_aliases`
- `tests/xsh/typing-imported-pure.xsh::test_imported_generic_calls_keep_kind_and_parameter_restrictions`
- `tests/xsh/local-inference.xsh::unannotated_empty_record_does_not_become_map_from_later_use`

Run the changed native files before the combined boundary. Preserve G1's
process cleanup and IPC behavior, G2's stats contract, current lint fixes,
and applicable finite formal controls. The source-only audit establishes no
fresh runtime, build, performance, or corpus result.

The following obligations are the minimum acceptance floor for a port:

- Re-run the original and reduced eleven-root sets through ordinary checking
  and indexed lowering, with all 48 source transformations validated. Run
  all 13 selected paired native witnesses as new integration evidence. The
  historical 1,040 receipt contains only the targeted witnesses described above. The original 1,040 evidence's paired
  `test_system_report_memory_reports_malformed_and_oversized_meminfo_fields`
  covers the two removed `meminfo_issue` parameters in both views.
- Keep exact scheme, row, capture, and lowering tests from the implementation
  lineage. In particular retain `checker_rejects_generic_list_body_before_lowering`,
  `user_schemes_execute_after_frontend_drop_on_both_routes`,
  `inferred_host_replace_executes_after_frontend_drop_on_both_routes`, and
  `inferred_host_nested_split_executes_after_frontend_drop_on_both_routes`.
  The host cases are mandatory for the accepted final compiler delta.
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
queued behind the former consolidation closure. Run them for the accepted
final compiler delta. No fresh build, suite, or test was run
for this audit because G1 is not accepted at this branch point.
