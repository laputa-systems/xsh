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


Integration candidate `6468326009b62cfd8ed682b0dc7007b371d3cb4d` now passes
ordinary checking and lowering for both eleven-root views. Fresh AST validation
confirms 499 local and 541 internal removals with all 48 loaded sources protected.
The explicit retained table is the accepted `15243e97` version. Both whole-view
digests and the candidate products stay unchanged across the checks. Logs are
under `.work/tmp/i2-1040-check-r7-646832600`. These focused results earn no
runtime or G1 acceptance credit. Full combined qualification remains pending.

R9 source `5454112e9f5fc10eae2faa75e5d6a5d09586bd4f` passes the fifteen
selected runtime witnesses in each frozen view: thirty passes, zero failures
and zero skips. The selection includes both historical `meminfo_issue` witnesses
and Laputa plan JSON behavior. Whole-view digests and product hashes stay
unchanged, and both owned containers are removed. Evidence is under
`.work/tmp/i2-1040-witnesses-r9-5454112e9-r2`. An earlier attempt rejects stale
bootstrap product hashes before execution; the successful run binds the final
products recorded by R9's canonical owner. This finite replay adds no removals
and does not qualify G1 or the separate four-site practical cohort.

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

The frozen paired adapter accepts an explicit compiler product pair:

```sh
bash dev/performance/corpus/check_views.sh --frozen \
  "$COMPILER_ROOT" "$COMPILER_REV" \
  "$XSH_ROOT" ddc17d3d53c819c343f9ecaa738ac67d59e9033a \
  "$LAPUTA_ROOT" ca658ce1da9d89c55209c25268dfdf687ff4fdc6 \
  "$XSH_PRODUCT" "$XSHT_PRODUCT" "$XSH_SHA256" "$XSHT_SHA256" \
  "$RETAINED" "$VIEWS" \
  --selector xsh 'core/tests/test-wc.xsh::test_wc_characters_and_invalid_bytes'
```

All source roots must be clean checkouts at their specified revisions. Products
must reside under the compiler checkout's `.work/`. The retained table and views
must reside under the invoking checkout's `.work/tmp/`. `VIEWS` contains
`original/{xsh,laputa}` and `reduced/{xsh,laputa}`.

The adapter reconstructs the accepted views with the witness-fixture correction
and validates their frozen digests through `frozen_witnesses.py`. It reconstructs
the candidate from the supplied retained table and compares both complete trees.
The candidate may erase selected Proc parameter and return contracts from the
accepted retained table. Every other retained row, including its reason, stays
unchanged. All 202 effect clauses remain retained in this tranche.

The adapter checks ten XSH roots and Laputa `pm/plan_json.xsh` in both views.
Repeat `--selector PROJECT SELECTOR` for each affected behavior, where `PROJECT`
is `xsh` or `laputa`. Each selector must pass exactly once with zero skips in
each view. Source and product checks run before and after execution. The single
selector above illustrates invocation; it does not qualify the whole cohort.
Adapter tests use controlled products and earn no compiler or removal credit.

The adapter compares checked interfaces across the same eleven root entries.
It runs `xsht check --compare-with ORIGINAL_ROOT --candidate-root CANDIDATE_ROOT PATH...`
from each reduced project root. One invocation compares the ten XSH roots. The
other compares Laputa `pm/plan_json.xsh`. Each invocation must exit successfully
without an error diagnostic. It must emit exactly one stdout line in this form:

```text
xsht check interface qualification: N callable, M CLI
```

At least one count must be nonzero for each project. The callable count reports
unique merged callable declarations. The CLI count reports unique merged CLI
parsing declarations. The adapter rejects a nonzero command status, an error
diagnostic, and missing, duplicate, malformed, or zero-count summaries. After
either comparison succeeds or fails, the exit guard rechecks the source
revisions, product hashes, and both complete accepted view trees. It preserves
the comparison's status if a guard also fails. Besides callable and CLI parsing
interfaces, the comparison checks captured registered-operation selections,
published elaborations and concrete per-declaration effect summaries. The
stdout counts describe interfaces only. Built-in operator selections and
symbolic callback-effect relations are not yet captured. Paired runtime
selectors separately check their asserted behavior; help-text checks retain
their own scope.

Run the controlled frozen-adapter tests with
`XSH_LAPUTA_CORPUS="$LAPUTA_ROOT" python3 dev/performance/corpus/test_check_views.py`.
The tests check command roots and entries, rejected results, and cleanup guards.
They do not build or qualify compiler products.

### Proposed 758 Proc paired-behavior selection

This source-reviewed list identifies exact selectors for a paired run of the
proposed 758 Proc-signature view against the original through the frozen adapter.
The list is not an execution result and earns no parity or removal credit. The
accepted 1,040 evidence above remains historical. It records the R9 paired
selection and the targeted `meminfo_issue` witness. Those executions do not
count as runs of the proposed 758 view, even when a selector overlaps. All 21
selector names below were checked against exact test definitions in the frozen
original tree.

Replace the sample selector pair in the frozen adapter command above with the
pairs below. Each line is an argument-list fragment, not a standalone command.
Combine the lines with the existing `--frozen` command. The adapter runs each
selector once in each view.

The `wc` selectors reach `posix_mode`, `read_file`, `files0_input`,
`count_input`, and `count_one`. The `cut` selectors reach `bound`, `ranges`, and
`charset`. The `tr` selectors reach `warnings`, `parse_set`, `set_error`,
`repeat_count`, and `validate_classes`. The `date` selectors reach `parse_date`,
`date_parse.relative`, `format_date`, `format_text`, `format_directive`,
`format_nanoseconds`, and `locale_name`:

```text
  --selector xsh 'core/tests/test-wc.xsh::test_wc_characters_and_invalid_bytes'
  --selector xsh 'core/tests/test-wc.xsh::test_wc_files0_processes_each_name_before_list_eof'
  --selector xsh 'core/tests/test-wc.xsh::test_wc_words_follow_unicode_white_space_unless_posix'
  --selector xsh 'core/tests/test-cut.xsh::test_cut_keeps_adjacent_range_boundaries'
  --selector xsh 'core/tests/test-cut.xsh::test_cut_unicode_field_delimiter'
  --selector xsh 'core/tests/test-tr.xsh::test_tr_octal_warning_preserves_gnu_layout_and_precedes_set_errors'
  --selector xsh 'core/tests/test-tr.xsh::test_tr_repeat_leaves_room_for_suffix_and_rejects_misaligned_classes'
  --selector xsh 'core/tests/test-date.xsh::test_date_negative_relative_offset'
  --selector xsh 'core/tests/test-date.xsh::test_date_format_modifiers_and_width_bounds'
```

The identity selector reaches `collect_identity`. The CPU selector reaches
`collect_cpu`, `read_cpu_info`, `collect_frequency_policies`, and
`read_effective_cgroup_cpuset`. The meminfo and cgroup selectors reach
`collect_memory`, `read_value`, and `collect_cgroups`. The storage selector
reaches `collect_storage`. The sensor and power selector reaches
`collect_sensors` and `collect_power`. The process selector reaches
`collect_processes` and `read_process`. The USB selector reaches `collect_usb`.
The network-device selector reaches `link_network_device_sources`. The capture
selector reaches `capture_metadata_bytes`, `replay_mountinfo_bundle`,
`encode_replayed_report`, and the production storage collector:

```text
  --selector xsh 'tests/xsh/system-report.xsh::test_system_report_live_collection_uses_explicit_root_and_redacts_by_default'
  --selector xsh 'tests/xsh/system-report.xsh::test_system_report_cpu_collection_keeps_sparse_models_policies_and_cpuset'
  --selector xsh 'tests/xsh/system-report.xsh::test_system_report_memory_reports_malformed_and_oversized_meminfo_fields'
  --selector xsh 'tests/xsh/system-report.xsh::test_system_report_memory_collects_cgroup_v2_limits_and_visible_ancestors'
  --selector xsh 'tests/xsh/system-report.xsh::test_system_report_storage_parses_mountinfo_escapes_and_stacked_mounts'
  --selector xsh 'tests/xsh/system-report.xsh::test_system_report_sensor_units_and_powercap_ranges_are_bounded'
  --selector xsh 'tests/xsh/system-report.xsh::test_system_report_process_collection_scales_pages_and_omits_private_sources'
  --selector xsh 'tests/xsh/system-report.xsh::test_system_report_usb_truncated_scalar_sources_do_not_publish_prefixes'
  --selector xsh 'tests/xsh/system-report.xsh::test_system_report_network_device_links_keep_absence_separate_from_failures'
  --selector xsh 'dev/tests/test-system-report-check.xsh::test_system_report_mountinfo_capture_replays_production_storage_collector'
```

The plan JSON selector reaches `plan_json_write_dto`, `plan_json_from_dto`,
`plan_json_node`, and `plan_json_dependency`. The recipe selector reaches
`decode_filetree` and `decode_filetree_entry` for path validation:

```text
  --selector laputa 'tests/pm/pm_plan.xsh::test_build_plan_json_round_trip_and_detects_corruption'
  --selector laputa 'tests/pm/pm_recipe.xsh::test_recipe_rejects_parent_filetree_traversal'
```

The plan JSON round-trip uses an empty remote snapshot. It does not exercise
`plan_json_remote` or non-null remote retrieval serialization. The
network-device test checks fixture symlink resolution. It does not exercise
`collect_network` or live host network collection. The identity test also uses a
temporary root. The dev selector covers mountinfo capture and replay. It does
not cover dev CLI dispatch, gates, stage execution, or cargo orchestration.

The `pm/generation.xsh` JSON selector in the practical current-source extension
tests `GenerationPlanDto`. It does not exercise Laputa `pm/plan_json.xsh`. The
extension's `sort` and `awk` selectors are unchanged by proposed 758. They are
excluded from this proposed 758 selection only. Their practical-extension
results do not qualify the proposed frozen view.

The practical current-source extension has two paths. `check.sh` remains a
behavior-only check for the XSH utility roots, `wc`, `sort`, `awk`, and Laputa
plan and generation modules. `inventory.sh`, `prepare_views.sh`, and
`check_views.sh` add a separate annotation experiment over `wc`, `sort`,
`awk`, and `pm/generation.xsh`; its selected removals and candidate results are
recorded below. Neither path changes the frozen 48-file accounting. The
candidate products used for that experiment qualify focused I1 changes only;
they are not the G1 baseline or full performance acceptance.
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
the current 877 table is not a substitute. The rows include 758 historically
retained Proc annotations (549 non-default parameter types and 209 explicit
non-Unit returns), 150 local Map selectors, two `replace` ambiguity controls, one
interpolation observation, 17 source-context observations, 713 rows with
historical diagnostics only, and 202 effect clauses. These classes partition
the denominator:

| Retained class | Rows | Evidence and current status |
|---|---:|---|
| Historical Proc inference frontier | 758 | The checker at `15243e97` retained these parameter and explicit value-return annotations. They are an in-scope implementation frontier for the current campaign, not a permanent annotation floor; classify intentional contracts separately from missing inference. |
| Local Map selector | 150 | 149 annotations select a Map for an empty literal; site 3598 selects a Map for a nonempty literal. The exact type annotation remains in the existing witness output. |
| `replace` ambiguity control | 2 | The `Str` receiver distinguishes string replacement from Regex replacement; retain as negative controls. |
| Interpolation observation | 1 | The body uses the parameter only in interpolation; whether a generalized display relationship suffices remains open. |
| Source-context observations | 17 | The source shapes below are known; the cause for each retained annotation remains open. |
| Historical diagnostics only | 713 | Diagnostic families and source shapes identify the old failure context, not the missing inference relationship. |
| Joint effect proof pending | 202 | All clauses remain in the source; individual lint suggestions do not prove simultaneous removal. |
| **Total** | **1,843** | |

With source-qualified `654fe6e99` products, erasing all 202 effect clauses alone
passes checks of `core/date.xsh` and `core/lib/system_report_live.xsh`.
Adding fourteen Proc annotation erasures from the broader candidate makes
both roots fail with unresolved Proc contracts and downstream unknown-effect
diagnostics. The two local helper chains also pass when only their effect
clauses are erased. These checks separate that Proc interaction from a
standalone effect-summary failure; they earn no joint removal or runtime credit.
Raw results are under
`.work/wt/full-inference-effect-corpus/.work/tmp/effect-propagation-isolation-654-r2/runs-r2/`.

### Bounded 20-site source relationship review

`frozen_witnesses.py` identifies twelve Proc sites in this sample: 94, 95, 97,
102, 104, 106–109, 111, 112, and 121. It identifies eight effect sites: 96,
100, 103, 110, 120, 124, 135, and 439. The first nineteen sites are in
`xsh/core/wc.xsh`. Site 439 is in `laputa/pm/plan_json.xsh`. This sample records
source relationships and supports no cause or removal claim.

The `wc.xsh` sites form one source-level helper chain. `main` calls
`number_width` at line 656 and `posix_mode` at line 665. It calls
`files0_input` at lines 639, 693, and 706. `count_one` calls `count_input` at
line 497 and `write_streamed_line` at line 522. `main` calls `count_one` at
lines 693, 706, and 710. `count_input` calls `read_file` at line 432.
`number_width` consumes `List[Input]` and `Shown`, then returns a width used
for output. `count_input` returns `InputResult`, whose fields `count_one` reads.
`files0_input` constructs `Input` records that flow into the same loop. This
review did not authenticate the lexical references as resolved declaration
edges.

The effect sites separate direct operations from called capabilities.
`posix_mode` reads `POSIXLY_CORRECT` through `env.get`. `read_file` calls
`Path.read_bytes` and `Path.bytes_lines` with `?`. `files0_input` calls
`gnu.quote_maybe` on its diagnostic path, and that imported function has an
`env` effect. `count_input` reads stdin and calls `read_file`; its `io` effect
also covers `env` under SPEC 9.6. `write_streamed_line` writes and flushes
output, while `count_one` combines it with `count_input`. `number_width` reads
`Path.metadata` inside `if let Ok`, so its `fs` effect has a direct source
operation and its `error` effect remains a negative inference control. These
source facts do not establish whether a retained clause is an intentional
capability contract.

Three bounded annotation-pair probes against the matching `a57c981` source and
products tested representative relationships. These results are diagnostic
for that pair only. For `read_file`, both forms checked and revealed
`Result[Bytes, Error]`; both preserved the caller's `?`, printed `4` for the
same input, and rejected a pure caller. This demonstrates inference in the
reduced body, not whole-`wc` equivalence. For `files0_input`, the erased effect
clause still propagated the imported `gnu.quote_maybe` `env` effect: a caller
restricted to `[error]` was rejected in both forms. The return pair differed:
the annotated call revealed `Record`, while the erased call revealed
`Result[Record, Error]`; direct field access succeeded only for the annotated
form. Site 111 is therefore a current
source meaning/elaboration selector, not evidence of a missing inference
relationship. SPEC 9.3 says a private proc's plain value tail defaults to
`Result[T]`, while an explicit `-> Input` preserves the direct record result.
Deleting this annotation alone changes the return contract. The 209 retained
Proc return annotations split into 170 outer `Result[...]` types and 39 plain
value types. The plain types include aliases to records: `InputResult` at site
121 resolves to the record declared in `core/wc.xsh`, and
`collectors.SourceRead` at site 2061 resolves through
`core/lib/system_report_collect.xsh` to the record in `core/lib/sys_source.xsh`.
Under SPEC 9.3, these explicit plain return types select a direct value result
where an inferred Proc value tail has a `Result[T]` envelope. This class is a
source meaning/elaboration selector, not a missing inference relationship. It
does not establish author intent or make plain return annotations an
immutable floor. Site 110's `env` edge is independently inferable in this
reduced probe, which does not classify the entire effect cohort.

A separate diagnostic on compiler source `f87c6d69` used the canonical
1,843-row retained table, so it included only the historical 1,040 removals.
The original roots and first nine utility selectors passed; the reduced XSH
root check then failed with seven diagnostics: one list-splice error at
`cut.xsh` site 168, one `RecoveryType` error for `date_parse.xsh`'s `multiply`
at sites 738–740, two `stream-sort` errors, and three indexed-build errors.
The candidate retained all 758 Proc annotations and 202 effect clauses. The
765 and 1,501 diagnostics from more-erased views are not independent counts of
causes and cannot be attributed wholesale to the additional 719 Proc and 202
effect erasures. This diagnostic ran no reduced selectors, Laputa checks, or
interface comparisons and earns no removal or parity credit; the frozen
9,194-site denominator and accepted 1,040 table are unchanged.

For site 439, the reduced `plan_json_remote` pair checked with and without its
annotations, the caller's `?` worked in both forms, and both printed the same
target. Revealed output was `Result[Record, Error]` in both forms. This confirms
the reduced structural result shape but does not prove nominal
`RemoteRetrieval` identity or author intent. None of these probes is a full
practical-root comparison or removal credit; remaining sample relationships
stay unclassified until evidence establishes them.

For the 149 empty Maps, the annotation bytes encode 145 single-argument forms
and four explicit key/value forms.
[XSH Map type syntax](SPEC.md#45-lists-maps-and-records) defines `Map[V]` as
`Map[Str, V]`; the four two-argument forms use three `Str` keys and one `Int`
key. Site 3598 is `Map[Str]` on a nonempty literal,
also meaning `Map[Str, Str]`. Nineteen other retained parameter annotations
are Maps: ten Proc parameters already counted among the 758 Proc rows, and
nine pure-function parameters in the 713 diagnostic-only rows. These source
shapes preserve Map-versus-record and key/value meaning; they do not show that
a future inference rule could not select the same type. The frozen retained
reason for site 3598 says erasure leaves an unchecked dynamic Record; that
historical reason is not a current qualification result. The accepted 1,040
README at `15243e97:dev/consolidation/typing-corpus/README.md` gives the source
context.
The current source records assign no historical diagnostic-only row to absent
information, higher-rank use, or a missing in-scope relationship. Those causes
remain unassigned within the 713 rows; the 17 source-context rows and site 2850
also remain open, and no effect clause has joint-removal credit.

Source review gives specific context for 19 retained sites. Of these, 17
remain pending cause review; sites 3599 and 3665 are `replace` negative
controls described below. Site 2850 is classified separately as an
interpolation-only source observation pending contract review. Site 2483 is
`cgroup_maximum`'s `SourceRead` parameter.
Its body reads `source.observation.state` and `.value`, then passes `source` to
`collectors.bounded_number`. The frozen retained reason says the inferred field
shape disagrees with the original `SourceRead` schema. These facts identify a
context path for a candidate query. The query should determine whether current
inference chooses a different shape and, if so, why. Sites 2332, 2543, 3406,
3407, 3476, 6436, 6564, 7427,
7822, and 7823 join plain Null with a concrete type without an Optional witness;
whether those joins should infer an Optional remains unresolved. Site 2285
retains `MonitorBattery` field context, and site 6338 depends on a later USB
endpoints producer; source order and intended context have not been separated
from an inference limitation. Sites 973 and 3914 carry nominal enum return
annotations. Site 973 constructs the `InvalidCpuList` variant; site 3914 uses the
shorthand `.Invalid`, whose spelling appears in several nominal error families
in the frozen source. That lexical difference does not establish whether either
return annotation is needed for selection. Sites 9111 and 9112 use enum
membership operands with typed call-site arguments, but the body shape does not
identify which relationship is missing. Keep these 17 sites pending with these
source shapes attached. Sites 3599 and 3665 need a `Str` anchor for
`replace`, which also accepts Regex; the accepted negative-control contract
requires this call to remain ambiguous without local evidence, so these two are
negative controls. Site 2850's parameter is used only in interpolation in the
observed body. That source fact does not establish that the parameter needs a
ground type; a safe generalized variable with a sealed display relationship
may satisfy the contract. Keep the site pending until that contract is tested.

The 730 checker-restored rows split into 17 with source facts but unresolved
causes and 713 with diagnostic-family evidence only. Removing 150 Map-selection
locals, two ambiguous `replace` receivers, and site 2850 from the 883 joint-check
rows gives those 730. The 17 source-fact rows are site 2483, the ten
null/concrete joins, two producer or source-order cases, two nominal variant
returns, and two membership-grammar boundaries listed above. They remain
pending cause review.

The focused candidate source `4233e34eeab1450d695ec4d5d0f61d0fe2959f06`
passes the original 11-root corpus and the historical 1,040-removal view.
Adding these 730 rows to make a 1,770-removal view fails both projects:
XSH reports 1,845 diagnostic lines across 34 codes, and Laputa reports 143
across 15 codes. The lines include cascading diagnostics, so these totals do
not identify unique failing annotations or assign causes. No part of this
batch is accepted for removal; the frozen source classifications and retained
set remain unchanged.

The remaining 713 rows have these frozen diagnostic families:

| Diagnostic | Sites | Diagnostic | Sites |
|---|---:|---|---:|
| `infer-return` | 284 | `local-inference` | 229 |
| `type-mismatch` | 38 | `try-context` | 37 |
| `for-iterator` | 27 | `result-fallback` | 25 |
| `display-conversion` | 20 | `index-type` | 8 |
| `slice-type` | 7 | `spread-not-record` | 7 |
| `listcomp-iterator` | 5 | `required-return` | 5 |
| `ambiguous-overload` | 3 | `if-condition` | 3 |
| `inferred-variant` | 2 | `dynamic-boundary` | 2 |
| `unknown-method` | 2 | `membership-type` | 2 |
| `pattern-type` | 2 | `infer-param` | 1 |
| `field-access` | 1 | `try-error` | 1 |
| `guard-binding` | 1 | `record-update-field` | 1 |

A historical diagnostic identifies where that earlier erasure failed. It does not establish
whether the missing fact is unavailable, a semantic restriction, or a specific
inference relationship. Keep these 713 rows pending source-level witnesses;
do not assign a cause from the code alone.

The validator's `source_shape` column groups sites by frozen owner and fixed
lexical cue priority. These labels summarize source structure; they do not say
that an annotation supplies a missing relationship or that inference should
select one.

| Source-body shape | Sites | Owner declarations |
|---|---:|---:|
| Coupled parameter and return annotations | 330 | 131 |
| Mixed iteration, join, construction, or match cues | 116 | 89 |
| Optional or row join cues | 80 | 55 |
| Iteration or collection cues | 64 | 40 |
| Record or variant construction cues | 39 | 16 |
| Typed call or method cues | 38 | 29 |
| Operator, member, or index cues | 17 | 8 |
| Format interpolation cues | 13 | 4 |
| Variant match cues | 7 | 7 |
| Empty collection initializer | 8 | 8 |
| Other typed initializer | 1 | 1 |

These counts partition all 713 rows and 388 owner declarations. The 17
source-context rows have their own shape partition: ten nullable branch joins;
one each for an empty-list accumulator, later-produced nullable record state,
schema projection with typed call, single visible variant spelling, and shared
variant spelling; and two enum-membership operand sites. Those shapes keep the
source distinctions inspectable without changing their pending status.

Source review against R12 (`55032a7ee`) reconciles the two largest families.
These are historical diagnostic groups, not annotation kinds or current failures:

| Historical diagnostic | Retained annotations | Declarations |
|---|---|---:|
| `infer-return` | 126 returns and 158 parameters | 154 |
| `local-inference` | 229 required pure-function parameters, all without defaults | 178 |

The 284 `infer-return` rows exclude four previously identified source-fact rows
from the 288-row total below. Its declarations split into 64 with both kinds,
62 with returns only, and 28 with parameters only. Of the 126 return annotations,
83 name Result types, 40 name records or other types, and three name Lists.
Null/empty constructors, typed record converters, and coupled parameter/return
helpers provide distinct probe classes. No checker result is inferred from these
source shapes.

The 229 `local-inference` rows exclude sites 9111, 9112, and 2850 from 232 raw
rows. They cover 203 XSH parameters and 26 Laputa parameters across 18 files.
Their source relationships split as follows:

| Body relationship | Sites |
|---|---:|
| Nominal or error-family matching without a body-derived selector | 6 |
| Ordering or sorting without a concrete operand type | 6 |
| Interpolation without a selected concrete display type | 14 |
| Equality links values but does not establish their annotated domain | 7 |
| Typed returns, fields, conditions, calls, or other body constraints | 196 |

R12 seeds unannotated pure parameters with fresh variables and solves constraints
from their bodies. Caller argument types do not determine those declarations.
It can generalize type and open-row variables. Neither an available body
constraint nor a typed caller proves an annotation removable. These source
groups remain pending focused checks; frozen removals and denominators are unchanged.

The 288 `infer-return` rows cover 128 return annotations and 160 parameter
annotations across 158 declarations. Sixty-four declarations retain both
kinds, 64 retain only a return, and 30 retain only parameters. The accepted
implementation's `infer_local_pure_returns` checks each pure body before its
callers, and `return_type_is_concrete` rejects `Null`, `Unknown`, `Any`, and
`ErasedRecord` return shapes, including those nested in optionals, lists, maps,
results, and records. That makes caller context unavailable to this return
inference pass. The source supports this mechanism, but does not show whether
each retained declaration should remain annotated or whether a later inference
relationship should supply its missing type.

Three retained examples expose the boundary. `system_report_live.xsh`'s
`empty_text` builds a `TextObservation` record with two `null` fields (site
2033). `system_report.xsh`'s `optional_firmware_identity_xsh` returns `null`
on one branch and a converted identity on the other (site 1796).
`system_report_check.xsh`'s `ip_rule_prefix` returns `Ok(null)` for a valid
zero-prefix case (site 7160). Each body contains a null value whose optional
payload type is not stated by that expression. The functions have concrete
return annotations, but source inspection alone cannot decide whether those
annotations are required semantic witnesses or whether inference should gain
more context. The annotations remain pending, with no removal credit.

The largest source groups also contain direct typed data flow: the
`not_requested_*` helpers receive report sections at `system_report.xsh:1004`
and following, `cgroup_resource` is called from live collection paths, and
reference parsers such as `process_reference_number` feed structured process
records. These call sites show where a caller has useful types. They do not
prove a missing inference relationship because the accepted return pass
checks bodies before callers. After I1, test this boundary by removing return
and parameter annotations separately for representative null-bearing builders,
report converters, and reference parsers. Compare exact diagnostics and
retained source views; do not generalize from `infer-return` alone.

All 202 effect clauses remain pending joint proof. On focused candidate source
`4233e34eeab1450d695ec4d5d0f61d0fe2959f06`,
`xsht lint --only lint.prefer-inferred-private-effects` reports individual
equality and rewrite-equivalence proofs for 142 clauses. The other 60 receive
no suggestion. The per-project result is:

| Source | Eligible effects | Individual suggestions | No suggestion |
|---|---:|---:|---:|
| XSH | 192 | 142 | 50 |
| Laputa | 10 | 0 | 10 |
| Total | 202 | 142 | 60 |

The 60 without a suggestion match the historical pending declaration list
below. A missing suggestion is not proof that a clause is semantically
required. The exact declarations in the frozen source views are:

| Source | Declarations without a candidate suggestion |
|---|---|
| Laputa `pm/catalog.xsh` | `make_catalog` |
| Laputa `pm/fingerprint.xsh` | `digest_lines` |
| Laputa `pm/graph.xsh` | `closure_from_edges` |
| Laputa `pm/plan.xsh` | `plan_digest_lines`, `absolute_recipe_package`, `find_remote`, `require_supported_target`, `require_build_epoch` |
| Laputa `pm/plan_json.xsh` | `plan_json_remote` |
| Laputa `pm/recipe.xsh` | `package_contract_error` |
| XSH `core/lib/system_report_live.xsh` | `network_link_target`, `collect_network` |
| XSH `core/lib/text_a2.xsh` | `tab_error` |
| XSH `core/lib/textio_a1.xsh` | `size_is_reached` |
| XSH `core/wc.xsh` | `number_width`, `count_input`, `write_streamed_line` |
| XSH `dev/stage.xsh` | `execute_with` |
| XSH `dev/system_report_check.xsh` | `reference_powercap_parent`, `device_class_reference_parent`, `print_cpu_id_set_result`, `compare_live_identity`, `compare_live_namespaces`, `compare_live_meminfo`, `compare_live_thp`, `compare_live_huge_pages`, `compare_live_psi`, `compare_live_vulnerabilities`, `compare_live_swaps`, `compare_live_pci_identity`, `compare_live_pci_links`, `compare_live_usb_topology`, `compare_live_usb_ids`, `compare_live_usb_power`, `compare_live_usb_interfaces`, `compare_live_power_supplies`, `compare_live_powercap`, `compare_live_device_classes`, `compare_live_hwmon`, `compare_live_pci_bindings`, `compare_live_thermal`, `compare_live_ip_links`, `compare_live_ip_addresses`, `compare_live_ip_rules`, `compare_live_ip_routes`, `compare_live_storage`, `compare_live_queue`, `compare_live_mounts`, `compare_live_mount_usage`, `compare_live_modules`, `compare_live_kernel_command_line`, `compare_live_kernel_parameters`, `compare_live_cpu_scope`, `compare_live_cgroup2`, `compare_live_cpu_cache`, `compare_live_cpu_topology`, `compare_live_cpufreq`, `compare_live_cpuidle`, `compare_live_processes`, `run_fixture_cases` |

The R6 fact query used compiler source commit
`0f2cbe61b3f8a20f695315d696cac51239b5861d` with default features in the pinned
Linux container. It checked the 60 frozen declarations against the reconstructed
original views at digest
`e3429dcf2abe9524754a28458651443bc56523b50130268a1c1ff44c03e36899`. The
reduced-view digest
`b67b498467a1dfe613573690288a8c514b9c03d0f895a4a3b20e4fd3b55968f9` was
validated separately and was not an input to this query. The query verified the
selected Cargo lock packages against the compiler lock.
The 60 unique rows cover 12 source files. The extraction is in
[`effect-facts-r6.tsv`](../dev/performance/corpus/effect-facts-r6.tsv), with its
namespace, body span, declared and effective effects, required effects,
inference flags, redundancy, unknown chain, and written source call sites.

All 60 facts have `inferred=false`, `inference_allowed=true`,
`redundant_clause=false`, and an empty `unknown_chain`. Each `effective` set
matches its explicit clause, and each differs from `required`. The reported
`required` set is a strict subset of `effective` in 58 rows. In the other two,
`required` also includes `env`, which neither written set spells out. Both
clauses include `io`; SPEC 9.6 defines `io` to imply `env`, and the R6 checker
uses that implication when checking whether a clause covers a required effect
([`SPEC 9.6`](SPEC.md#96-effects),
[`effects_covers`](../src/sema/check.rs:2528)). The TSV records written and
required effect sets without expanding that coverage implication, so these
both clauses cover their required effects. The linter returns unless the checked
fact has `redundant_clause=true`
([`lint_annotations.rs`](../crates/xsht/src/lint_annotations.rs:200)). It then
checks whether deleting the clause preserves checked output. All 60 fail the
first proof. That explains the absent suggestions without proving that the
clauses must remain or that several clauses can be removed together.

Eleven bodies have `required=[]`: `make_catalog`, `digest_lines`,
`closure_from_edges`, `plan_digest_lines`, `find_remote`,
`require_supported_target`, `require_build_epoch`, `plan_json_remote`,
`package_contract_error`, `absolute_recipe_package`, and
`size_is_reached`. The ten Laputa helpers build data, hash input, transform
paths, or construct `Result` values. Their source does not use `?` to propagate
an error effect. `size_is_reached` is a separate case: it calls
`bytes.read_at` in the R6 source at compiler commit
`0f2cbe61b3f8a20f695315d696cac51239b5861d`; its runtime implementation opens,
seeks, and reads a file ([`bytes.rs`](../src/modules/bytes.rs:363)). Module-call
effect conversion uses `Effect::from_module_call`
([`node.rs`](../src/syntax/node.rs:49),
[`signature.rs`](../src/modules/signature.rs:348)); that mapping has no `bytes`
case, so this R6 fact omits the filesystem effect. This finding is specific to
the R6 source; it transfers no removal credit to the separate R7 Fs repair.
Keep this row out of any over-annotation classification.

The 37 `compare_live_*` functions named in the table above all have effective
`[fs,process,time,error,io]` and required `[error,fs,process,time]`. Their source
calls include filesystem reads, `time.now`, and `process.run`. This proves a
shared listed-set difference, but it does not explain why the clauses include
`io`.
The separate `compare_live_processes` row has a required set that also contains
`env`. Its body calls `system.execution_units()` at
`dev/system_report_check.xsh:25474`, and the checker maps `system` calls to
`env`. Its clause includes `io`, which covers the required `env` permission.

The other twelve rows are recorded individually in the TSV. Ten of them have
required effects that are strict subsets of their clauses. The other two have
an additional required `env` effect: `compare_live_processes` gets it from
`system.execution_units()`, and `core/wc.xsh`'s `count_input` gets it from
`gnu.quote_bytes` and `gnu.quote_maybe` at lines 449 and 456 in the frozen
original XSH archive `ddc17d3d53c819c343f9ecaa738ac67d59e9033a`. In
`core/lib/gnu.xsh`, both callees declare `[env]`; `quote_bytes` reads locale
variables through `utf8_locale` at lines 218–238, and `quote_maybe` can
delegate to it at lines 278–281. `count_input`'s other listed calls—
`io.stdin_read`, `io.stdin_bytes`, `Path.metadata`, and the `[fs,error]`
`read_file`—do not account for `env`. Both clauses include `io`, so the
checker allows these required effects under SPEC 9.6. This source trace
explains the `count_input` set difference; it does not change the R6 facts or
grant clause-removal credit.

`CheckOutput.function_effect_facts` does not expose resolved call edges. The
TSV's call-site column records written call spellings, not checker-resolved
dependencies. Those spellings contain no direct calls between two of the 60
target names, but that lexical result does not rule out aliases or paths through
other functions. For navigation, the validator also matches unqualified
spellings from those 60 fact rows against the 202 frozen effect-clause owner
names in the same file. It finds 42 distinct caller-to-owner spelling
candidates whose target names belong to owners without an R6 fact. Their
202-node candidate graph has 164 components: 157 singletons, four pairs, and components of sizes
three, nine, and twenty-five. The
`effect_spelling_candidate_component` field identifies these groups. The
other 142 owners have no call-site fact rows; some are candidate targets of
these spellings, and the rest are singleton nodes because their outgoing calls
are unobserved. A same-file spelling can be
shadowed or aliased, so these are navigation candidates, not confirmed edges or
complete dependency components. They establish no cycle and do not narrow the
all-202 joint proof group. The lexical scan still finds no direct call between
two of the 60 fact owners.
`compare_live_identity` at line 21766 shows source operations that account for
its required filesystem, time, process, and error effects. `pm/plan.xsh`'s
`require_supported_target` returns `Err` without propagating it, which matches
its empty required set. Neither example proves intent or a solver defect.

To reproduce the query inside the same pinned container, create an isolated
Cargo binary project under the container's `.work/tmp` mount. The container
uses `kache` and `mold`. Copy the helper into `src/main.rs`, then use this
manifest:

```sh
mkdir -p /query/query-r1/src
cp /helper/dev/performance/corpus/effect_facts.rs /query/query-r1/src/main.rs
cat > /query/query-r1/Cargo.toml <<'EOF'
[package]
name = "effect-facts"
version = "0.0.0"
edition = "2024"

[workspace]

[profile.dev]
debug = 0
strip = "debuginfo"

[dependencies]
xsh = { path = "/work" }

[patch.crates-io]
regex-lite = { path = "/work/third_party/regex-lite" }
EOF
TMPDIR=/work/.work/tmp CARGO_TARGET_DIR=/work/.work/target-final-canonical \
  cargo build --offline --target x86_64-unknown-linux-musl \
  --manifest-path /query/query-r1/Cargo.toml
python3 - <<'PY'
import tomllib
from pathlib import Path

compiler = tomllib.loads(Path("/work/Cargo.lock").read_text())
query = tomllib.loads(Path("/query/query-r1/Cargo.lock").read_text())
compiler_registry = {
    (package["name"], package["version"], package["source"]): package["checksum"]
    for package in compiler["package"]
    if package.get("source", "").startswith("registry+")
}
selected_registry = [
    package
    for package in query["package"]
    if package.get("source", "").startswith("registry+")
]
for package in selected_registry:
    identity = (package["name"], package["version"], package["source"])
    assert identity in compiler_registry, f"missing from compiler lock: {identity}"
    assert package["checksum"] == compiler_registry[identity], f"checksum differs: {identity}"
print(f"validated {len(selected_registry)} selected registry packages")
PY
/work/.work/target-final-canonical/x86_64-unknown-linux-musl/debug/effect-facts \
  /views/xsh /views/laputa \
  /helper/dev/performance/corpus/effect-facts-targets.tsv \
  > /query/facts-r1.tsv
```

The query used no lint or runtime results. The compiler source remains
unqualified, and the 142 earlier individual suggestions earn no joint-removal
credit.

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
the fact query does not connect the 60 cases to scheme publication or a
recursive generic boundary. Keep all 202 clauses and the 2,190-site internal
denominator; the 142 single-clause results earn no joint-removal credit. Any
joint solve requires resolved, source-confirmed components. This query
establishes none. The exact list of the 142 historical lint findings is at
`15243e97` in
`dev/consolidation/evidence/typing-corpus-host-obligations/individual-effects-sites.tsv`.

The accounting is 910 source and selection records (758 historically retained
Proc annotations, 149 empty Maps, site 3598, and the two `replace` negative
controls), one interpolation-only source observation pending contract review,
730 checker-restored rows pending cause review, and 202 pending effects: 1,843
retained eligible sites. The 17 source-context records and 713
diagnostic-family records remain pending; none is excluded from future
capability rechecks.

### Private effect ceiling source classes

This source review keeps the exact 202 effect sites and the existing
`all-202-effect-clauses` proof group. It assigns no removal credit. The
historical evidence partitions into 142 individual lint candidates and 60
without a suggestion. Those 60 comprise ten Laputa helpers, 38
`compare_live_*` helpers, and twelve other sites. Of the 38 comparisons, 37
share the required set `[error,fs,process,time]`; site 8277,
`compare_live_processes`, also requires `env`. Site 8274,
`compare_live_cpu_sets`, belongs to the 142 historical candidates.

The frozen source supports these more specific relationships:

- Sites 3640, 3644, 3652, 3656, 3660, and 3671 are the six private tour renderers.
  `item_html` calls `blocks_html`, which calls `block_html`, which calls
  `item_html`. This three-function cycle also reaches `code_html` and then
  `xsh_html`. The latter writes a source file, invokes the highlighter, and
  propagates errors. `page_html` calls `block_html`. The `Highlighter` fields
  are paths, and these calls refer to the same-file declarations without
  local callable substitutions. Their filesystem, process, and error
  relationships have body operations that anchor them; a generic callback
  ceiling does not account for this group. These source calls supplement the
  incomplete spelling graph above. Joint inference and interface comparison
  remain required.
- Laputa sites 439, 8311, 8337, 8350, 8376, 8414, 8460, 8508, 9071, and 9145
  transform typed data, sort or hash in-memory bytes, or construct returned
  errors. Their bodies contain no host operation or `?` propagation. For
  example, `absolute_recipe_package` constructs a path without accessing the
  filesystem; `require_supported_target` returns an error as data. These are
  fixed-body effect-summary candidates with surplus declared permissions,
  not evidence of an unknown callback effect. Keep the Proc kind and Result
  behavior in their comparisons.
- The 38 comparison helpers call concrete filesystem, process, and clock
  operations and return through `validate_and_run`'s explicit exported
  contract. Their printed diagnostics alone do not require `io`. The R6
  facts have no unknown call chain. A decision about their surplus `io`
  permission needs a joint interface comparison; an absent lint suggestion
  does not establish a missing inference relationship or an intentional
  generic capability restriction.

The remaining twelve sites need the following distinctions:

| Sites | Source relationship and remaining question |
|---|---|
| 96, 2552, 6434, 6486 | Filesystem results are handled or returned as data; the R6 body requirements contain `fs` without outward `error`. Distinguish returned failure data from propagation before deciding whether the extra declared permission is intentional. |
| 120, 124 | `io` covers the differing `env` and `process` entries. Compare permission coverage as well as written sets; this difference alone is not an effect-inference failure. |
| 592 | `size_is_reached` actually calls file-backed `bytes.read_at`. The old empty R6 requirement is a mapping defect, not a pure-body witness. Current `Effect::from_module_call` maps that operation to `fs`; transfer to the combined product still needs checking. |
| 668 | `tab_error` calls the `[process]` `gnu.error` and exits. The extra `env` has no direct body source in this helper; retain the declared-contract comparison as migration work. |
| 2564 | `collect_network` uses `linux.network_dump` and rooted filesystem collection. The R6 set is `[error,fs,process]`; the extra declared `env` needs contract review, not a guessed generic-effect explanation. |
| 3259 | `execute_with` consumes command data and calls `process.run`. Its public wrappers and `StageRunner` module contract retain their explicit bounds. That separate interface promise does not classify this private ceiling as a generic callable restriction. |
| 8044, 8300 | JSON propagation or fixture filesystem/process work accounts for the listed body effects; printing alone does not explain the extra `io`. Keep their permission and failure behavior in the joint comparison. |

Effect site 2421, `collect_cpu`, has a separate body-publication prerequisite.
A focused checker diagnostic at source `075882e1d`, using a byte-confirmed
copy of the frozen reduced source, finds its first unsupported child at
`system_report_live.xsh:2879`:
`root.children(cpu_path, max_entries: 256)`. The result is already a closed
Result of the directory-listing record and Error, but the call has neither
checked-operation nor registered-call evidence. The later `linked_cpus`
collector is not the observed first failure. This witness directs review to
the known FsRoot call's publication authority; it does not demonstrate a
missing effect summary or a necessary effect ceiling. It remains diagnostic
evidence pending the combined compiler qualification.

A genuine generic ceiling constrains a callable effect parameter. The existing
`explicit_recursive_effect_bounds_accept_pure_and_empty_proc_callbacks`
control has a recursive `left`/`right` pair whose explicit `[]` accepts pure
and empty-effect callbacks and rejects an `io` callback. This review identifies
no such restriction at a site in the frozen 202. The concrete `StageRunner`
and `SystemReportModelEncoder` module contracts remain interface or schema
annotations outside this class; their meaning cannot be assigned to an
adjacent private clause. Opaque invocation, missing resolved effect evidence,
and migration failures remain open wherever an actual witness establishes
one.

These source classes do not change the accepted 1,040-site boundary, the 149
empty-Map selectors, or the 39 Proc return-wrapper selectors. In particular,
effect site 96 and `number_width`'s explicit return site 97 are different
contracts. Historical lint equality and the two-root effect-only checks above
are not current joint qualification of the nullable-target and collector
changes. Reuse the existing joint checker, lowering, interface, and native
owners with their next matching source/product pair; no new solver or effect
registry follows from this review.

### Semantic joint candidate

The current joint proposal retains the 39 plain Proc return selectors, removes
the other 719 historically retained Proc annotations and all 202 effect clauses,
and restores four annotations removed in the historical 1,040 view:

| Site | Frozen source | Preserved return contract |
|---|---|---|
| 17 | `core/wc.xsh::byte_count` | `ByteCount` UInt fields and return schema |
| 20 | `core/wc.xsh::add_bytes` | `ByteCount` UInt fields and return schema |
| 79 | `core/wc.xsh::count_data` | `Counts` UInt fields and return schema |
| 82 | `core/wc.xsh::add_counts` | `Counts` UInt fields and return schema |

The matching `4de7c99e4` historical replay reaches all original and reduced root
checks and 21 selectors in each view, then fails interface qualification. These
four erased returns change UInt fields into inferred arithmetic results and lose
their return schemas. They are meaning selectors for this proposal. Other
interface mismatches, including identical printed nullable outputs, remain under
comparison review; this profile adds no guessed nullable-output retainers.

`frozen_witnesses.py FROZEN_VIEWS --prepare-semantic-candidate` produces the new
retained table from validated `original` and `reduced-1040` views. The existing
`cohort.py` and `validate.py` consume it without changing frozen source bytes,
inventory, or denominators. Candidate validation permits only the complete four
WC restorations with their explicit reasons; original retained reasons remain
unchanged, and effect removal still selects zero or all 202 clauses.

The generated table has 926 retainers: 194 local and 732 internal. Its source
view has 1,957 proposed removals: 499 of 693 locals and 1,458 of 2,190 internal
sites. Relative to the historical table, it adds 921 erasures and restores four
annotations. The historical 1,843 retainers and accepted 1,040 removals are not
revised. All 149 empty-Map selectors, the additional Map-literal selector, the
39 plain Proc returns, and the remaining checker-restored classes retain their
source. Missing in-scope inference remains open work.

The candidate table and all 48 transformed source files validate; comparison
with the existing `compare_tree.py` confirms the original copy is unchanged.
Matching `4de7c99e4` interface-only attempts select ten XSH entries and Laputa's
`pm/plan_json.xsh`, and both exit 2 before a qualification summary. The XSH
attempt stops at `core/wc.xsh:445` in `count_input`:
`counted ?? count_data(data, shown, posix, utf8)` cannot be lowered through the
supported generic projection path. The Laputa attempt reports 204 checker
diagnostics, beginning with the unannotated `validate_positive_release` body
in `pm/recipe.xsh`; the later unresolved calls and material-type errors include
cascades. These are diagnostic totals, not independent cause counts.
The comparison loader labels candidate byte overrides with original source
paths. Those path labels alone do not establish which preparation phase failed.

Both container completions and cleanup were consumed. Source revision, product
hashes, and source-view bytes match their preflight guards afterward. Neither
project is interface-qualified by these attempts. The XSH generic-body
relationship remains inference work. The Laputa diagnostics need source-cause
review; this profile does not classify them as intentional retainers. These
results grant no new removal, behavior, or paired-corpus credit.

### Build source witness records

`dev/performance/corpus/frozen_witnesses.py` reads the eligibility, retained,
inventory, and 142-site effect tables from commit `15243e97`. It joins those
tables to the R6 facts for the other 60 effect clauses. It fails if the frozen
counts, site identities, or effect partition change.

Pass the root containing the supplied `original` and `reduced-1040` views. The
tool first runs the existing `view_digest.py` helper and requires the accepted
14,287-entry view digests recorded above. It then compares every path and byte
in both trees against the accepted 1,040 edits in memory. This rejects changed
views, including matching edits to both trees. The tool then checks every
retained annotation and its owner bytes at the shifted offset in each view.
Source paths stay inside `xsh` or `laputa`. The tool rejects symlinks in
selected source paths. It writes one minimal source record per retained site
and does not edit either view.

```sh
VIEWS=/path/to/perf-effect-facts-r6/.work/tmp/effect-facts-r6
PYTHONDONTWRITEBYTECODE=1 python3 dev/performance/corpus/frozen_witnesses.py \
  "$VIEWS" > "$PWD/.work/tmp/f1-retained-witnesses.tsv"
```

The output selects 549 Proc parameter contracts and 209 explicit Proc returns
by exact source site. It selects all 202 private effect clauses in one joint
group. The group contains 142 historical individual lint findings and 60 R6
source-fact rows. Neither evidence kind proves a joint effect removal. The
remaining records split into 149 empty-Map literal selectors, one nonempty Map
selector at site 3598, two `replace` negative controls, one interpolation-only
parameter at site 2850 pending contract review, 17 source-context rows, and 713
rows with historical diagnostic evidence only. The `source_shape` field groups
the 17 source-context rows and the 713 diagnostic-only rows by source
structure. The effect component field records only the spelling candidates
described above; it does not resolve calls or change the all-202 joint group.

These are source observations and witness selections. The 758 Proc rows remain
in scope for capability-specific rechecks under the current inference rules.
The 17 source-context rows still need cause review, and the 713
diagnostic-only rows remain unclassified by cause. A historical diagnostic
family does not establish why an annotation stays. The site records earn no
removal or behavior-equivalence credit.

Run the focused Python tests with:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 dev/performance/corpus/test_frozen_witnesses.py
```

The tests check the accepted denominator and effect partition, both source-shape
partitions, the spelling-candidate graph counts, source review classification
boundaries, exact span shifting, overlap rejection, path containment, and
rejection of lockstep source-view mutation. They do not run a compiler or native
test.

The foundation receipt remains the planning evidence in
`full-inference-campaign.md`. It records one pass with no failures or skips for
`tests/xsh/typing-capability-witnesses.xsh::test_typing_identity_and_open_record_projection`
and for
`tests/xsh/rank1-inference.xsh::rank1_generic_function_cannot_be_stored_as_typed_callable`.
The second test preserves the old rejection as a frontier control. F1 did not
rerun either foundation probe or build compiler products. The reference probes
below use the combined-55032 canonical verification products.

### Combined-55032 canonical baseline probes

`dev/performance/corpus/witnesses/` keeps annotated and one-annotation-erased
forms with the same bodies and calls. These paired probes add no corpus rows or
removal credit. Each annotated form passed `xsht check` and normal `xsh`
execution. The membership element and list erasures share one annotated
control. `constructor-unique` and `constructor-shared-invalid` each have
annotated and erased forms; `membership-element.annotated.xsh` is paired with
each single-parameter membership erasure. `constructor-unique-qualified.xsh`
is an explicit-family control. The erased results on the combined-55032
canonical products were:

| Retained source class | Erased check result | Successful output |
|---|---|---|
| Null/concrete return join | Passed; output matched the annotated form. | `7`, then `-1` |
| Site 2850 interpolation-only parameter | Exit 2: `check.display-conversion` and `check.local-inference`. The inferred value needs an annotation for display, and the generic pure body fails the current local-inference rule. | `meminfo.MemTotal` |
| Recursive pure parameter | Passed; output matched the annotated form. | `4` |
| Recursive pure return | Exit 2: `check.required-return`; recursive pure `recursive_return` requires an explicit return annotation. | `4` |
| Empty-List accumulator | Passed; output matched the annotated form. | `3 3 5 8` |
| `SourceRead` parameter | Exit 2: `check.type-mismatch` at the `cgroup_maximum(source)` call. | `8` |
| `SourceRead` structural-row alias | Passed and matched the `SourceRead` output. | `8` |
| Site 973, unique `.InvalidCpuList` | Exit 2: `check.inferred-variant`: `.InvalidCpuList` has no expected enum or error-family type to select from. Cascades include `check.infer-return` and `check.field-access`. | Annotated stdout `b'invalid CPU list\n'`; erased form not run. |
| Site 3914, reused `.Invalid` | Exit 2 with the same `check.inferred-variant` message as the unique label, plus `check.infer-return` and `check.field-access`. No separate ambiguity diagnostic. | Annotated stdout `b'invalid input\n'`; erased form not run. |
| Site 9111, membership element | Passed after erasing `kind: DependencyKind`, with `kinds: List[DependencyKind]` retained; output matched. | `b'selected\n'` |
| Site 9112, membership list | Exit 2: `check.membership-type` (“membership requires List, Set, Map, Record, Str, Bytes, Path, or env.PATH”) and `check.local-inference` after erasing `kinds: List[DependencyKind]`. | Not run after check failure. |

The `SourceRead` structural-row case preserves the field reads and call to the
typed `bounded_number` helper. It shows that this explicit row shape composes
with the helper in this probe; it does not resolve the historical source site's
cause. Site 2850's display failure does not show that a concrete parameter type
is required. A later inference contract may generalize that parameter. These
probes report compiler behavior and do not establish why historical source
sites retain annotations. Keep the source classifications diagnostic-only;
they establish no removal or behavior-equivalence credit.

The two nominal probes use one family with a single `InvalidCpuList` label and
two families that both declare `Invalid`. The erased forms fail with the same
primary diagnostic: a target-typed variant has no expected enum or error-family
type. A separate no-return-annotation control using the explicit
`ProbeError.InvalidCpuList` constructor passes and prints the same
`b'invalid CPU list\n'` bytes as the annotated form. This distinguishes the
current unqualified-constructor behavior from source-level label uniqueness;
it does not decide whether future inference should select a unique visible
variant or generalize its nominal type.

The membership element erasure shows that this baseline relates an unannotated
element to an explicitly typed list in `kind in kinds`. The list erasure shows
that the current checker does not select the collection shape from the
annotated element alone. Neither result establishes a permanent annotation
requirement or full-inference contract. The command stdout bytes above were
captured unchanged between the adapter's readiness line and its final timing
line. A source that failed `xsht check` was not executed.

The probes used source checkout
`.work/wt/perf-i1-combined-freeze-r12` at commit
`55032a7ee2c4d340a6e5df9d4f362b0074d8a9e0`, tree
`8255279c80818f3a159d2052f6eed102e0976938`. The canonical verification
products were
`.work/wt/perf-i1-combined-freeze-r12/.work/target-final-canonical/x86_64-unknown-linux-musl/verification/xsh`
(`aa674a2899adc10a0e1cbe9810176821e4e3b6165f89910aadaf2f1c3e62023e`) and
`.work/wt/perf-i1-combined-freeze-r12/.work/target-final-canonical/x86_64-unknown-linux-musl/verification/xsht`
(`caaaf0b7555d9620c048c76027579eb716cc92da3df6297fd113945977c96437`). The
canonical replay receipt at
`.work/tmp/i1-combined-r12-55032a7ee/canonical-replay.json` records a passed
canonical gate batch replay with no acceptance credit. It is not a
functional-close receipt; its SHA-256 is
`5495bc3fc2560295596529d893c2702d0d5853a9f0ef6b4a67b257b084ca8d87`. Each
probe ran through the pinned offline adapter
with two CPUs and 4 GiB of memory. No build or suite ran for these probes.

If all 150 Map selectors remain annotated, at most 543 of 693 locals (78.35%)
could be removed, even if every other retained local became inferable. If all
758 historical Proc annotations remain, at most 1,432 of 2,190 internal sites
(65.39%) could be removed. Retaining all 202 effect clauses pending joint
proof would lower that conditional internal ceiling to 1,230 of 2,190
(56.16%). These are conditional accounting scenarios, not language limits:
the Proc annotations are in-scope for inference, and source meaning at Map
selectors must be classified separately. The frozen denominator and historical
95% local and 85% internal targets remain unchanged.

## Practical source-view preparation

The frozen inventory covers the selected 48 historical files but does not
provide current-source spans for `core/wc.xsh`, `core/sort.xsh`, `core/awk.xsh`,
or Laputa `pm/generation.xsh`. The existing `cohort.py` is not inventory for
these current-source files. `dev/performance/corpus/inventory.rs` records AST
type and effect spans for them; the candidate inventory contained 724 spans,
668 type annotations and 56 effect clauses. `inventory.sh` pins the compiler
and both clean source revisions and writes its output under `.work/tmp`.
`apply_spans.py` accepts explicit site selections and rejects exported, entry,
Proc, Stream, Map-typed, and `.require()` target annotations.

The paired run used compiler source
`4233e34eeab1450d695ec4d5d0f61d0fe2959f06`, XSH source at the same revision,
and isolated Laputa source `a8176072460c6b19f4899f6108f5978f7631b227`. Its
matched candidate products were `xsh`
`1cd72ec147cc3e6930c8b7d79adf63d3b684cc2927cace28b77b716b00b10ea0` and
`xsht` `6504330399712ed25a4d5eb2a8fa55e5a1af73cc715e6a4b297bb89862cb94ca`.
This focused I1 candidate is not a G1 baseline or full performance
acceptance.

The practical source boundary is `core/wc.xsh`, `core/sort.xsh`,
`core/awk.xsh`, and isolated Laputa `pm/generation.xsh`. `check_views.sh`
checks 12 XSH roots and three Laputa roots so their imports resolve. It runs
these exact behavior selectors in both original and reduced views:

```text
core/tests/test-wc.xsh::test_wc_characters_and_invalid_bytes
core/tests/test-sort.xsh::test_sort_general_numeric_orders_past_double_precision
core/tests/test-awk.xsh::test_awk_arrays_functions_and_loops
core/tests/test-awk.xsh::test_awk_matrix_program_files_and_includes_shard_001
tests/pm/pm_generation.xsh::test_generation_plan_and_receipt_are_deterministic_for_root_order_and_duplicates
tests/pm/pm_generation.xsh::test_generation_plan_json_round_trips_and_rejects_changed_identity
```

The changed-path selectors are the sort numeric-order test for
`padded_decimal`, the awk program-files/includes shard for `directive_operand`
and `newlines`, and the two generation tests for `generation_sorted_unique`.
The awk shard covers directive parsing and source-line preservation after
includes. The first generation test covers root sorting and deduplication; the
JSON test exercises single-root plan creation and identity validation. The awk
arrays/functions selector executes the no-directive path through
`directive_operand`, but does not cover a parsed directive or include line
preservation. The `wc` selector is an unrelated retained behavior control.

The matching `d2c820629` per-root diagnostic replay ran only the original 12
XSH roots: `date_parse` and `dev/docker` passed, and the other ten failed. It
did not run reduced XSH, Laputa, interface, body, or runtime checks. A separate
full practical replay with matching `e0f8cbfac` products passed source-view
validation, then failed the original XSH check with six
`compact.indexed-build` diagnostics; later stages did not run. Neither replay
qualifies a selected erasure or the current six-selector matrix. Later accepted
source updates do not yet have a matching product pair.

`inventory.sh`, `prepare_views.sh`, and `check_views.sh` take explicit source
revisions and reject dirty or mismatched checkouts. The adapter test's isolated
Laputa revision `639184e65a712b9d33fa5033e045f8913cd04a5a` is a separate test
input; it is not the practical generation corpus. F1 added no practical
erasures and claims no new removal credit. The four-return I1 experiment below
is a separate historical candidate.

The earlier selected reduced view removes only four private pure-function return
annotations for which `lint.prefer-inferred-pure-return` provided individual
proofs:

```sh
SITE_IDS='xsh|core/awk.xsh|return|154|pure directive_operand return,xsh|core/awk.xsh|return|166|pure newlines return,xsh|core/sort.xsh|return|183|pure padded_decimal return,laputa|pm/generation.xsh|return|76|pure generation_sorted_unique return'
```

In that candidate, no `wc` annotation is removed. `byte_count` and
`generation_plan_dto` remain unchanged signature probes. All six probed
callable signatures, including those for the four selected functions, matched
between the original and reduced views. The earlier five-selector F1 replay
passed checks for 12 XSH and 3 Laputa files in each view. The current
adapter adds the awk program-files/includes shard listed above; this selector
is not covered by that historical five-selector result. Removing only `data`'s parameter
annotation from `wc.continuation`, `wc.within`, or `wc.byte_words` produces
both `check.ambiguous-overload` and `check.result-fallback`.
`wc.sequence_width.data` remains reserved and untouched.

The signature probes cover `byte_count`, `padded_decimal`,
`directive_operand`, `newlines`, `generation_sorted_unique`, and
`generation_plan_dto`. Four return annotations are selected as described above;
`byte_count` and `generation_plan_dto` remain unchanged query targets. The
runner appends the same `reveal_type` probes to original and reduced copies and
compares inferred callable signatures after checking both views.
`wc.continuation`'s `data`, `wc.within`'s `data`, and `wc.byte_words`'s `data`
are diagnostic controls: each body uses `byte_at`, and `byte_words` also uses
`len`, methods available on both `Str` and `Bytes`. Removing one parameter
annotation at a time in disposable copies produces both `check.ambiguous-overload`
and `check.result-fallback`. Keep `wc.sequence_width.data` separate because
typed helper calls may constrain it; it remains unclassified and untouched.

The current runtime pair uses the six exact selectors above against both
source views; a pass is useful only when it selects one test and reports zero
failures and skips. This pairs inferred signatures with retained diagnostics
and observable command behavior without changing the frozen 48-file
accounting.

To reproduce the practical run from exact source commits, first write the AST
inventory to `.work/tmp`, then build original and selected reduced trees with
`prepare_views.sh`. `check_views.sh` recreates both trees from the pinned
archives, verifies the selected spans byte-for-byte, runs the same checker and
six exact runtime selectors on each view, and compares all six revealed
signatures. It also checks each of the three `wc` diagnostic controls after
removing only that parameter annotation, and restores temporary probe lines
before returning. Tree comparisons include symlink targets without following
them, so the source archives are compared safely when they contain cyclic
links. The inventory helper was rebuilt in the pinned image with the I1 kache
wrapper, Linux musl mold adapter, static-CRT target flags, and a lane-local
target. It reproduced the same 724 spans byte-for-byte. The lane-local kache
cache mount was not writable, so caching was disabled for that build; this
result establishes inventory equivalence, not build performance. The matched
candidate products remained unchanged.
`SITE_IDS` is a comma-separated list of exact
`project|path|kind|line|owner` selectors from the generated inventory. The
paired runner rejects numeric row ids and reserves the three diagnostic
controls plus `wc.sequence_width.data` from the reduced view.

```sh
INVENTORY="$PWD/.work/tmp/performance-practical-sites.tsv"
VIEWS="$PWD/.work/tmp/performance-practical-views"
bash dev/performance/corpus/inventory.sh \
  "$COMPILER_SOURCE" "$COMPILER_REV" \
  "$XSH_ROOT" "$XSH_REV" "$LAPUTA_ROOT" "$LAPUTA_REV" "$INVENTORY"
bash dev/performance/corpus/prepare_views.sh \
  "$XSH_ROOT" "$XSH_REV" "$LAPUTA_ROOT" "$LAPUTA_REV" \
  "$INVENTORY" "$SITE_IDS" "$VIEWS"
bash dev/performance/corpus/check_views.sh \
  "$COMPILER_SOURCE" "$COMPILER_REV" \
  "$XSH_ROOT" "$XSH_REV" "$LAPUTA_ROOT" "$LAPUTA_REV" \
  "$XSH_PRODUCT" "$XSHT_PRODUCT" "$XSH_SHA256" "$XSHT_SHA256" \
  "$INVENTORY" "$SITE_IDS" "$VIEWS"
```

The selected practical run archives its exact source commits into `.work/tmp`,
retains whole trees so imports resolve, and applies the chosen spans only to the
reduced copies. It preserves the practical files and revisions as a separate
extension. The frozen 1,040 `cohort.py` recipe remains a separate baseline and
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
- Preserve Proc entry decoding and required/default/rest input behavior while
  inferring parameter types where source determines them. Preserve non-Unit
  return behavior, declared effect restrictions, and every `Map[...] = {}`
  record interpretation. Individual effect-clause redundancy results do not
  prove simultaneous removal through declared-call cycles; no one of the 202
  effect clauses is authorized for elision by this checkpoint.

The compact Rust selector names and their raw results are in
`dev/consolidation/evidence/typing-host-obligations/README.md` and its
`*-final.log` files. The six native host-obligation selectors there were still
queued behind the former consolidation closure. Run them for the accepted
final compiler delta. No fresh build, suite, or test was run
for this audit because G1 is not accepted at this branch point.
