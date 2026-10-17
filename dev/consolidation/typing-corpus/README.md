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
