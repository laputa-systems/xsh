# Joint annotation cohort

This cohort retains the roots, source pins, source graph and 9,194 AST sites in
[the cohort design](../designs/typing-cohort.md). `eligibility.tsv` fixes each
site's category before the first removal run. Site numbers index the retained
inventory's data rows from zero. No compiler result determines eligibility.

All private local and signature sites are eligible, including private schema
references and explicit `Any` locals. Exceptions retain entry argument decoding,
implicit `require()` validation targets, and explicit entry effect bounds.
Schema definitions and exported interfaces remain unchanged. The `schema`
category includes application options, domain models and wire definitions. It
does not claim that every definition is a wire format.

`cohort.py` reconstructs the joint candidate from exact pinned source archives.
It removes only AST inventory edit spans. Sites restored after a failed joint
check stay in their frozen eligible denominator. `--retain` accepts a TSV whose
first column identifies those restored sites.

The [minimal capability fixtures](../evidence/typing-corpus-b235/capability-witnesses/)
are retained evidence for explicit checks with the qualified typing successor.
They include intentionally rejected inputs and syntax unsupported by the
consolidation compiler, so they live outside the current repository source corpus.

The source pins are XSH `ddc17d3d53c819c343f9ecaa738ac67d59e9033a` and Laputa
`ca658ce1da9d89c55209c25268dfdf687ff4fdc6`. The checking compiler is the qualified
typing successor, not either program-source pin. Compiler or runtime inference,
caller-dependent training and algorithm changes are outside this migration.

## Reproduce the candidate

Run from the isolated corpus worktree. Preserve any earlier scratch attempt
before preparing fresh source views.

```sh
bash dev/consolidation/typing-corpus/prepare.sh
bash dev/consolidation/typing-corpus/check.sh \
  "$PWD/.work/tmp/corpus/original" "$PWD/.work/tmp/corpus/check-original"
bash dev/consolidation/typing-corpus/check.sh \
  "$PWD/.work/tmp/corpus/reduced-final" "$PWD/.work/tmp/corpus/check-joint"
```

Preparation adds the accepted `fff7fed23` flock helper and its one lifecycle
witness call to both copied test trees. This fixture correction is outside the
48 program sources and changes no assertion. The pinned program-source views
and annotation counts remain unchanged.

Run `run-witnesses.sh` only in the assigned native-suite slot. It fails immediately
if the shared native lock is busy. Each invocation creates a fresh attempt
folder. Each exact selector must actually select and pass one test with no
failures or skips. Container cleanup and source and product checks run on failure
and interruption too.

The twelve XSH witnesses assert byte and character counts, fixed count order,
translation, deletion, squeezing, parser errors, file and stdin operand order,
Unicode delimiters, date precision, offline structured replay and redaction,
malformed replay rejection, gate selection, target propagation and failed
container cleanup. The Laputa witness asserts stable build-plan JSON, round-trip
identity, format rejection and digest rejection. Those assertions provide the
observable comparison for both views. Harness durations are not compared.

## Read the result

The current joint checker candidate removes 348 of 693 eligible locals (50.22%)
and 529 of 2,190 eligible internal annotations (24.16%). It restores 345 eligible
locals and 1,661 eligible internal sites. The internal removals are 165 parameter
and 364 return annotations. All 202 private effect clauses are retained.
Neither campaign target is met. No restored site leaves the frozen denominator.

Restoration uses parameters first, then required return contexts and directly
implicated local declarations. Some failing declaration groups remain
conservative. The result does not prove that every restored annotation is
individually necessary or that 877 is the maximum possible removal count.
Individual effect-clause lint proofs do not establish joint equivalence through
calls to other declared clauses. No unmeasured joint effect change contributes
to the score.

Required Proc parameters and explicit non-Unit Proc returns retain current
language semantics. Ambiguous method receivers need a distinguishing annotation:
both Str and Bytes provide `byte_at(Int) -> Int?`. The return type cannot select
between them. A caller must not train that declaration. `slice` and `record`
witnesses pass when their existing return context resolves the receiver or field.
The `issue` witness rejects the generic body under the existing direct-composition
restriction. These are distinct from the `replace` and `split` witnesses, whose
known Str helper anchors the parameter but whose earlier method calls fail
verified-indexed lowering. The direct-anchor gaps are fixed by the qualified host-anchor repair. The separate
`indirect-anchor` witness has a genuinely ambiguous receiver: Str and Regex
`replace` both accept the positional Str and labeled `with: Str` arguments and
return Str. Constraining its local intermediate to Str does not determine the
input receiver. Its rejection is preserved; a caller must not select Str for it.

The [prior raw evidence](../evidence/typing-corpus-b235/) preserves the original
613-removal checkpoint. The [host-anchor continuation](../evidence/typing-corpus-host-anchor-b235/)
preserves the 615-removal candidate. The
[finite host-obligation continuation](../evidence/typing-corpus-host-obligations/)
records the current 877-removal candidate and its qualified product identities.
Existing generic and adversarial acceptance relies on the retained typing and
host-obligation qualification. This migration changes no compiler source or
embedded standard library. The thirteen existing behavior witnesses pass in each view through the
[documented attempt composition](../evidence/typing-corpus-host-obligations/README.md#paired-behavior-acceptance).
The initial fixture failure remains recorded. Neither annotation target nor
general inference completion is claimed.

## Next source-only candidate

The next prepared candidate removes 891 sites: 352 locals and 539 internal
annotations (174 parameters and 365 returns). These are planned edits, not
checker or behavior results. The accepted 877 views and retained-site table stay
unchanged. All 202 effect clauses remain in the planned retained table.

The candidate retries these fourteen retained sites:

| Sites | Declaration | Reason to retry |
|---|---|---|
| 2867 | `sys_mem.source_error` parameter | Required projections feed four fixed arguments of exported pure `src.issue`. This fits the expanded generic body grammar. |
| 972, 3913 | `cpu_list_error`, `check_failure` parameters | Retained enum return contexts constrain the constructor payloads. |
| 2569–2571 | `unsupported_issue` parameters and return | The retained concrete `issue` signature supplies argument and result context. |
| 2854–2856 | `sys_mem.error_field` parameters | The retained `meminfo_field` signature and concrete state default supply context. |
| 3601 | `tour_html.attr` parameter | The retained `esc(Str)` argument contract constrains the receiver before replacement. |
| 2430, 2457, 2539, 3022 | CPU, memory, network address and USB endpoint locals | Each initializer names its nominal constructor explicitly. |

Only site 2867 depends directly on quantified body admission. The other thirteen
retry conservative restoration groups with existing concrete context. They must
resolve to closed signatures or retain their annotations. In particular, the
`error_field` call uses user defaults, which the quantified body grammar excludes.
The enum return annotations at sites 973 and 3914 remain explicit.

The source screen covers all 366 Pure declarations with retained signature sites.
The initializer review covers all 58 retained immutable locals. Larger groups contain operators, branches,
loops, aggregates, optional access, or receiver ambiguity. Those groups do not
follow from immutable-let and static-call admission. Return context is not proof
that an arbitrary body becomes ground. No such proof is assumed here.

Reproduce the separate source-only copy after preparing the accepted views:

```sh
plan="$PWD/.work/tmp/corpus/planned-generic-body"
mkdir -p "$plan"
python3 - "$plan/retained.tsv" <<'PY'
import csv, pathlib, sys
retry = {972, 2430, 2457, 2539, 2569, 2570, 2571,
         2854, 2855, 2856, 2867, 3022, 3601, 3913}
rows = list(csv.reader(pathlib.Path(
    'dev/consolidation/typing-corpus/retained.tsv').open(), delimiter='\t'))
assert retry <= {int(row[0]) for row in rows if row[0].isdigit()}
with pathlib.Path(sys.argv[1]).open('w') as stream:
    csv.writer(stream, delimiter='\t', lineterminator='\n').writerows(
        row for row in rows if not row[0].isdigit() or int(row[0]) not in retry)
PY
python3 dev/consolidation/typing-corpus/cohort.py \
  .work/tmp/corpus/original --reduced "$plan/candidate" --retain "$plan/retained.tsv"
python3 dev/consolidation/typing-corpus/validate.py \
  .work/tmp/corpus/original "$plan/candidate" --retain "$plan/retained.tsv"
```

Use a fresh candidate path. Source validation confirms exactly 891 eligible edit
spans across the same 48 sources, with all other annotation spans protected.
It does not run a compiler. The validator's default still checks accepted 877.

Qualification waits for the corrected generic-body compiler, its strict unsigned
value verification, capture ownership, structural return and evaluation-order
checks, and actual source-qualified products. Programs must retain ordinary
semantics. No edit works around missing validation. Then run all eleven original
and reduced check/lower roots and the existing thirteen paired witnesses in an
assigned slot. The current offline system-report and tooling witnesses do not
cover every new helper path.

Existing additional behavior oracles are `test_system_report_cpu_list_parser_handles_sparse_and_large_ids`,
`test_system_report_memory_reports_malformed_and_oversized_meminfo_fields`,
`test_system_report_cpu_collection_preserves_128_present_ids`,
`test_system_report_memory_collects_cgroup_v2_limits_and_visible_ancestors`,
`test_system_report_assembles_network_links_addresses_routes_and_rules`, and
`test_system_report_usb_descriptors_keep_configuration_and_endpoint_ownership`
in `tests/xsh/system-report.xsh`. The tour rendering oracle is
`test_tour_html_renders_prose_markup_and_escapes_text` in `dev/tests/test-tour-html.xsh`.
`test_system_report_memory_does_not_parse_truncated_meminfo_prefix` exercises
the non-observed source path into `source_error` and checks the retained truncation
issue. These selectors are planned coverage, not new passes.

### Remaining target limits

The retained locals comprise 287 `var`, 57 `let`, and one `const` annotation.
Even removing all 58 immutable annotations reaches only 406 of 693 locals.
The 95% target requires 659 removals, including at least 253 currently retained
mutable sites. Immutable-let admission alone cannot reach that target.

The existing local solver already creates inference variables for empty lists,
mutable null initializers and `Map.empty()`. The next local investigation should
trace constraints through collection yields, branch joins and later assignments
before final publication. Adding another initializer fallback would duplicate
that mechanism. A minimal failing case must establish whether constraints are
lost or genuinely ambiguous. Unconstrained collections and incompatible writes
must still reject, and mutable values must remain monomorphic.

The 85% internal target requires 1,862 removals. Required Proc parameters account
for 549 retained sites and non-Unit Proc returns account for 209. Preserving both
current rules caps internal removals at 1,432 of 2,190, even before other retained
sites. This is a current-contract limit, not a denominator change or approval to
expand Proc inference. The remaining 137 Pure return contexts and 564 Pure or
defaulted parameter sites also include ambiguity and aggregate or control-flow
bodies beyond the new grammar.

A bounded next generic slice could address operators on already-ground scalar
operands obtained from required row fields. That requires checked operator
identity and operand/result evidence, including strict signed and unsigned
physical kinds. It must not guess an unconstrained numeric domain. Aggregate
construction and control flow remain separate compiler work. Joint effect
elision still lacks authoritative equality evidence through declared calls.
The 142 individual proofs neither establish that equality nor prove a cycle.

## Independent mutable Set candidate

The Set tranche starts from accepted 877, independently of the generic-body plan.
It removes all 109 retained eligible local annotations whose inventory owner is
exactly `var name: Set[...] = set.empty()`. All 109 are `Set[Str]` annotations.
The nine affected sources contain no binding, parameter or import alias named
`set`. The declarations are inside ordinary functions. Source review does not
establish that every element constraint resolves jointly.

| Source | Additional planned removals |
|---|---:|
| `core/lib/system_report.xsh` | 4 |
| `core/lib/system_report_live.xsh` | 9 |
| `core/lib/sys_mem.xsh` | 3 |
| `core/lib/sys_mount.xsh` | 2 |
| `core/lib/sys_block.xsh` | 1 |
| `core/lib/system_report_collect.xsh` | 1 |
| `dev/system_report_check.xsh` | 82 |
| Laputa `pm/catalog.xsh` | 2 |
| Laputa `pm/recipe.xsh` | 5 |

The `986` candidate removes 457 of 693 eligible locals and the unchanged 529 of
2,190 eligible internal annotations when fully applied. All signatures, all 202
private effects, protected schemas and entry contracts retain the accepted 877
choices. The accepted original and reduced views remain unchanged. The
candidate inherits the same fixture-only flock correction outside the 48
program sources.

Prepare a fresh separate copy without invoking compiler products:

```sh
python3 dev/consolidation/typing-corpus/prepare_mutable_sets.py \
  "$PWD/.work/tmp/corpus/original" "$PWD/.work/tmp/corpus/mutable-set-plan"
```

The command prints every selected site ID and validates the reconstructed 48
sources against their frozen AST spans. It rejects an existing plan directory.
It does not inspect compiler results or change eligibility. The prepared source
passed that validation with exactly 109 additions. The initial compiler attempt
failed on the reduced view, as recorded below. The corrected retry qualifies
checking and lowering only. Runtime qualification remains pending.

The initial Set attempt used compiler source `7491228ab`, based on the accepted
host-obligation source without the pending generic-body changes. Its failure
remains preserved below. The corrected retry uses source `020f` and qualifies
checking and lowering only. It does not qualify runtime behavior or replace the
accepted 877 pair.

The existing thirteen paired witnesses remain the initial behavior matrix.
Seven wc, tr, cut and date witnesses protect unchanged roots. The two
system-report replay witnesses reach serialization and rejection contracts but
do not exercise all live collector accumulators. The gate, target and lifecycle
witnesses protect tooling behavior, not every one of the 82 checker accumulators.
The Laputa plan witness exercises recipe and catalog dependencies but does not
prove every duplicate-input branch. No per-site coverage claim is made.

Existing focused oracles for the additional paths include CPU-list rejection,
malformed and duplicate meminfo, duplicate swap identity, cgroup CPU and IO
counters, layered block identity, kernel-module duplicate identity, and Laputa
`test_recipe_rejects_duplicate_dependency`. Their exact source selectors are
available in `tests/xsh/system-report.xsh` and Laputa `tests/pm/pm_recipe.xsh`.
They require explicit selection review before extending the named slot.

The Set feature's existing `tests/xsh/local-inference.xsh` tests contain the
minimal safety cases: incompatible branch writes, a conflicting alias contract,
unsupported Float elements, an unsolved local, immutable and top-level empty
Sets, and a shadowed `set` binding. Their rejection expectations remain in force.
The positive case fixes the element type through branch and loop writes and
asserts alias visibility, the empty result, deduplication and insertion order.
Corpus removals neither replace those tests nor broaden their grammar.

The initial Set attempt used source `749`. The annotated original checked all
ten XSH roots and the Laputa root successfully, including ordinary lowering.
Both reduced batches exited 2 with 102 XSH and seven Laputa empty-Set inference
diagnostics. The [raw Set attempt](../evidence/typing-corpus-host-obligations/mutable-sets/README.md)
retains the exact runner, original and reduced outputs, statuses, source and
product guards, view digests and container cleanup. No native test ran in that
attempt.

The frozen compiler source is `7491228abdfd77e4be502247675b08682cbf16b8`.
Its tree and clean status, all three qualified binary hashes and both complete
view digests match before and after execution. Both owned containers are absent.
The attempt wrapper also uses `1 files` where the actual summary says `1 file`.
That matcher error is preserved. It does not change the original exit-0 and
reduced exit-2 compiler results, and no passing wrapper result is claimed.

Source inspection identifies an initializer admission defect. `check_binding_arena`
checks `set.empty()` before calling `infer_local_binding`. Without an existing
binding expectation, the empty Set constructor emits a diagnostic and returns
`Unknown`. The new Set seed then requires that ordinary result to be `Set`, so it
returns before creating the inference variable. Empty List admission has a
container-shaped ordinary result instead. The Set owner has the failure and
this trace. An initializer seed must carry correct binding, shadowing and element
domain authority before constructor checking. Restoring the 109 annotations
would hide this defect. This describes the failed 749 attempt only.

### Check and lowering qualification

The unchanged 986 candidate passes the corrected check/lower retry with the
pre-initializer fix at source `020f9c0c799a0e17c2b0006ced779e416a3a8052` and
tree `1f67a4262727c8779e3135bf0fa49132ef9f7c12`. Both views check all ten XSH
roots and the Laputa root. All four summaries report ordinary lowering with zero
diagnostics. All four compiler statuses and the wrapper status are zero. The
[020f check proof](../evidence/typing-corpus-host-obligations/mutable-sets/020f-check/run-check.sh)
retains the fresh runner and complete outputs. The prior 749 rejection remains
unchanged. The retry corrects the singular summary matcher.

All 109 new Set annotations are therefore jointly removable for checking and
lowering: 457 of 693 locals and the unchanged 529 of 2,190 internal annotations.
These are checker results. Runtime parity for the new source and candidate is
pending the assigned native slot. No previous native result is relabeled.

The 020f source stays clean throughout the retry. All three qualified physical
product hashes and both complete source-view digests match before and after.
The original digest remains `e3429dcf2abe9524754a28458651443bc56523b50130268a1c1ff44c03e36899`.
The reduced 986 digest is `43ad0beddeb95ef7f695598664156df29605831df3156433a11ea356d39d8c35`.
Both owned containers are absent. The [paired behavior archive](../evidence/typing-corpus-host-obligations/mutable-sets/README.md)
retains the native run and runner guards. All 13 exact selectors passed in each
view, with no failures or skips. These witnesses do not exercise all 109
accumulators or complete the Set feature gates.

The Set API fixture at
`a4418a5253dd557f7a96419f4212b09dd29084a1` passes. The documentation gate
fails because its slot verifier rejects valid nested destructuring in the
`5277` and `020f`
examples. Repair source `2cb3abd0c9be1a7b04f2bf7e9d9f7243c24c239f` has
qualified source-bound products, but the minimal SPEC 18 check-only probe
fails: the verifier reports `driver slot identity is not the currently visible
binding`. The first repair added the missing writer slot but exposed a
fresh-output versus prior-input identity mismatch; the same-name top-level
duplicate check also rejects. The owner is tracing the output-slot
representation, and no rebuild is authorized before root reviews a source
proposal. The original wrapper cleanup-check failure is preserved separately
and is not a source-build failure. Full accepted removal credit remains 877
until the API and documentation gates pass.

The separate 1,065-site collection plan adds 54 List and 25 conditional or
mutable-null removals to 986. Its reduced check remains red: source `76caa` passes
the original roots but reports 92 XSH diagnostics in the reduced view. Reduced
Laputa passes. The plan preserves all 149 Map annotations on `{}` initializers
and adds no accepted removal credit. See the [collection evidence](../evidence/typing-corpus-host-obligations/mutable-sets/README.md#unqualified-collection-plan).

## Empty-record contract

Owner decision: keep an unannotated `{}` as a record. This
follows [SPEC 5.1](../../../docs/SPEC.md#51-inference-and-annotations) and the
native
[`unannotated_empty_record_does_not_become_map_from_later_use`](../../../tests/xsh/local-inference.xsh)
regression. Later use must not make the record a map. The 149 remaining Map
annotations on `{}` initializers stay required. The denominator remains 693
locals. Removing every other eligible local would reach at most 544/693
(78.50%), below the 95% target.
No waiver or denominator reclassification applies.
