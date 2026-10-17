# Phase D typing cohort

This design freezes a representative XSH source cohort and its paired behavior checks before any annotation removal. It makes no annotation-removal or performance claim.

## Freeze the source views

The XSH program-source pin is `ddc17d3d53c819c343f9ecaa738ac67d59e9033a` in `/home/josh/d/laputa-systems/xsh`. The read-only Laputa source pin is `ca658ce1da9d89c55209c25268dfdf687ff4fdc6` in `/home/josh/d/laputa-systems/laputa`. The XSH pin freezes the program cohort. It does not identify the checker compiler.

Use these XSH root programs:

| Workload | Root paths |
|---|---|
| Required core command | `core/wc.xsh` |
| Text and byte utilities | `core/cut.xsh`, `core/tr.xsh` |
| Command parsing | `core/tr.xsh`, `dev/main.xsh` |
| Structured data transformation | `core/system-report.xsh` |
| Library helpers | `core/date.xsh`, `core/lib/date_parse.xsh` |
| Developer tooling | `dev/gates.xsh`, `dev/internal.xsh`, `dev/docker.xsh`, `dev/main.xsh` |
| External consumer | `/home/josh/d/laputa-systems/laputa/pm/plan_json.xsh` |

`core/date.xsh` imports `core/lib/date_parse.xsh`. `core/system-report.xsh` imports the report model and live collector. Count each reachable source module once, even when several roots import it. Resolve imports with the same qualified successor checker used for both source views. Laputa already participates in the campaign's check and lint scope, so `pm/plan_json.xsh` adds no external source scope. Do not edit the Laputa checkout. Make any candidate copy under `.work/tmp`.

The root set and the categories below stay fixed while the later harness tests removals. Existing module and native test sources are witnesses, not additional roots in the annotation denominator.

## Freeze annotation classes

Count annotation sites by syntax node. Record the count and rationale for each class before removing any annotation.

| Class | Treatment | Target denominator |
|---|---|---|
| Domain and wire schema declarations, including report and build-plan DTO fields | Preserve as schema | None |
| Intentional restrictions on exported signatures | Preserve as public contract and compare exported interfaces separately | None |
| Annotations identified before removal as semantically required | Preserve and record the reason at the source site | None |
| Eligible local type annotations | Remove with all eligible sites in the joint candidate | Eligible local target |
| Eligible internal parameter, return, and effect annotations | Remove with all eligible sites in the joint candidate | Eligible internal target |
| Exported inferred interfaces | Compare names, labels, defaults, parameter types, return types, effects, and schema expectations separately | No removal percentage |

Freeze the eligible site lists after the AST inventory and semantic review, before the first removal run. If a preselected eligible site proves necessary, restore it and count it as eligible but not removed. Do not reclassify it to improve a percentage. Count one parameter annotation per parameter and one effect-list annotation per declaration. A type definition is a schema declaration, not a removable local annotation.

For each eligible class, divide sites removed in a passing joint candidate by the frozen eligible count. A failed joint candidate has no removal percentage.

## Count syntax-tree sites

`xsht ast SCRIPT` prints parser debug output. `xsht grep` searches semantic AST patterns. Neither command reports a complete, stable annotation inventory. `xsht check --annotate` writes source edits, so it does not establish the denominator.

The later harness needs one AST visitor over the parser's `AstArena`. Start at the pinned root paths, follow the same module loader as `xsht check`, and de-duplicate loaded modules by source identity. Count local type annotations, callable parameter annotations, explicit return annotations, and explicit effect lists by their owning AST nodes and source spans. Classify declarations using the checker’s module visibility and schema information. Do not count type definition fields, casts, or imports as removable annotations. Use `xsht ast` or `xsht grep` only to spot-check visitor selections.

The source-qualified AST inventory is recorded in [typing-cohort-inventory.tsv](../evidence/typing-cohort-inventory.tsv). It uses XSH program source `ddc17d3d53c819c343f9ecaa738ac67d59e9033a`, read-only Laputa source `ca658ce1da9d89c55209c25268dfdf687ff4fdc6`, and helper source commit `cbc34b991bf1ca256ed9d2193752822354700e66`. The standalone helper was linked against the XSH library pair built from source `037dbfbe4a191fe68f07e7f7e93f3e16e1ed1e27`; that artifact qualifies the inventory tool's API, not a later consolidation checker.

The inventory run exited zero after verifying the loaded files and selected project configs against their source pins. The first launch stopped before loading sources because Git rejected the nested XSH worktree path; the authorized rerun supplied the exact source roots as safe directories. The TSV is 2,593,425 bytes and contains all per-file rows, source spans, and site classifications.

The 11 explicit roots produced 48 unique source files: 38 in XSH and 10 in Laputa. Four explicit roots were represented by their imported module identities; the imports contributed 37 additional unique files. Each explicit root appears once in the inventory.

| Site kind | Sites | Preliminary classification |
|---|---:|---|
| Local type | 702 | 386 local-candidate-review; 316 semantic-boundary-review |
| Parameter type | 2,287 | 964 internal-signature-candidate-review; 327 semantic-boundary-review; 996 exported-interface |
| Return type | 1,326 | 382 internal-signature-candidate-review; 329 semantic-boundary-review; 615 exported-interface |
| Effect list | 474 | 206 internal-signature-candidate-review; 268 exported-interface |
| Module contract | 41 | 41 exported-interface |
| Schema site | 4,364 | 4,364 type-definition sites outside the annotation denominator. Review their domain and wire roles. |
| Effect bound | 0 | Preserved separately |

Preliminary totals are 386 local-candidate-review, 1,552 internal-signature-candidate-review, 972 semantic-boundary-review, 1,920 exported-interface, and 4,364 schema-declaration sites. Candidate and boundary sites still need semantic review. Keep exported interfaces separate for comparison. The count does not show that every exported annotation is an intentional restriction. The inventory records no removals and no removal percentage. It does not identify a consolidation compiler. Phase E freezes the performance baseline compiler after consolidation closes.

## Review the preliminary eligibility classes

Use these source facts to classify sites before freezing eligibility.

### Established source contracts

- Preserve `proc main` rest-parameter annotations. `List[Bytes]` selects raw argument bytes. `List[Str]` selects text arguments. The inventory currently classifies all six entry parameters as internal candidates. Rows 249 (`core/cut.xsh`) and 418 (`core/system-report.xsh`) show the byte and text forms.
- Preserve annotations that supply the target for `.require()` without a type argument. That target controls runtime validation. Row 458 (`dev/main.xsh:333`) types `parsed.require()` as `ReleaseOptions`; row 8592 (`pm/recipe.xsh:109`) types the decoded `arch` field as `Str`. An internal parameter or return can also supply the target. Rows 1984 and 2031 supply `SystemReport` for dynamic report validation. Row 3510 supplies `GrammarReference` for `grammar_reference`'s tail expression. The inventory splits these target providers across semantic-boundary and candidate classes. Keep them unless the source call changes to `.require(T)`, which is outside this annotation-removal cohort.
- Keep exported signatures and module contracts separate from removal. Compare their parameter names, labels, defaults, types, returns, and effects. The count does not show that every exported annotation was intended as a restriction.

### Candidate proposals

Treat other private schema references as candidates unless a source-level contract requires them. Same-module or qualified imported schema names do not make an annotation a runtime validation boundary. XSH annotations state an expected type for the checker to prove. For example, rows 83 (`core/wc.xsh`, `ByteCount` return) and 317 (`core/date.xsh`, `date_parse.Instant` parameter) are private signatures and merit candidate review.

Review the 28 explicit `Any` locals as local candidates. In the inspected sites, typed values are widened immediately for `json.encode` or `json.set`. Rows 1988 (`core/lib/system_report.xsh`) and 6476 (`dev/system_report_check.xsh`) are examples. Preserve an `Any` annotation only when later dynamic operations rely on that boundary.

Keep all schema declarations outside the annotation denominator. Every schema row represents a type definition. The current `domain-wire-schema` label is broader than the source evidence. The TSV has 2,150 generic type-or-wire rows and 2,214 exported-schema rows. Neither reason establishes that a schema is a wire contract. Row 66 (`core/wc.xsh`) is a local options schema. Row 488 (`pm/plan_json.xsh`) is the `BuildPlanDto` wire schema. Preserve both definitions. Use domain or wire rationale only for schemas with that role. Compare exported schema declarations separately.

### Unresolved entry effects

Private internal effect lists remain candidates only when the checker infers the same effect set after removal. The inventory recorded no separate effect-bound sites. Keep `proc main` effect lists unresolved. This review did not establish whether the successor checker or runner exposes them as an entry contract. Do not assign those sites to the eligible effect denominator until that contract is checked.

These are review rules, not a frozen eligible list. This review changes no inventory counts, removes no annotations, and computes no removal percentage.

No line-search count is an annotation count. The inventory must print per-file counts and category totals before the candidate copy is edited. It must not add a repository configuration file or a committed provenance log.

## Pair checks and execution

Run one qualified successor verification `xsht` binary against the annotated source view and the joint-removal candidate. Use the same binary and its paired `xsh` product for both views. The candidate view contains the pinned test tree and only the preselected annotation edits. The native tests assert the expected behavior for their case, such as output, status, errors, order, or cleanup.

Set `XSHT` to the absolute path of the qualified successor verification binary. Freeze its source revision and product identity before the paired checks. Run each command from the matching source-view root. The source revision for this cohort does not select a consolidation compiler, because it predates inference.

From each XSH source view, run:

```sh
"$XSHT" check core/wc.xsh core/cut.xsh core/tr.xsh core/date.xsh core/lib/date_parse.xsh core/system-report.xsh dev/gates.xsh dev/internal.xsh dev/docker.xsh dev/main.xsh
"$XSHT" test --exact core/tests/test-wc.xsh::test_wc_characters_and_invalid_bytes
"$XSHT" test --exact core/tests/test-wc.xsh::test_wc_count_selection_and_order
"$XSHT" test --exact core/tests/test-tr.xsh::test_tr_translate_delete_squeeze_and_stdin
"$XSHT" test --exact core/tests/test-tr.xsh::test_tr_errors_identify_range_and_delete_mode_requirements
"$XSHT" test --exact core/tests/test-cut.xsh::test_cut_reads_files_and_stdin_in_operand_order
"$XSHT" test --exact core/tests/test-cut.xsh::test_cut_unicode_field_delimiter
"$XSHT" test --exact core/tests/test-date.xsh::test_date_grammar_epoch_precision_and_boundaries
"$XSHT" test --exact tests/xsh/system-report.xsh::test_system_report_command_replays_saved_json_offline
"$XSHT" test --exact tests/xsh/system-report.xsh::test_system_report_command_rejects_malformed_replay
"$XSHT" test --exact dev/tests/test-gates.xsh::test_changed_paths_select_and_deduplicate_their_verification_owners
"$XSHT" test --exact dev/tests/test-targets.xsh::test_dev_main_target_override_reaches_context
"$XSHT" test --exact dev/tests/test-lifecycle.xsh::test_docker_container_failure_runs_target_ownership_cleanup
```

These cases use inline bytes, temporary files, and the existing system-report JSON fixture. The cut case checks operand order. The developer lifecycle case creates fake Git, Cargo, and `chown` commands and checks cleanup after a build failure. The system-report cases check JSON replay, redaction, malformed input, and exit status. They need no live `/proc` data or real Docker daemon.

From the pinned Laputa source view and its disposable candidate copy, run the same external check and witness:

```sh
"$XSHT" check pm/plan_json.xsh
"$XSHT" test --exact tests/pm/pm_plan.xsh::test_build_plan_json_round_trip_and_detects_corruption
```

The Laputa test uses `tests/pm/fixtures/plans/basic-aarch64.json` and temporary output files. It checks a JSON round trip, stable output, unsupported format rejection, and digest rejection. Do not write candidate edits into `/home/josh/d/laputa-systems/laputa`.

Require both source views to pass `xsht check` and report any diagnostic changes. Require each exact native test to pass in both views. Preserve call order, cleanup, selected operations, and failure behavior. Do not treat individual annotation-removal success as evidence that the joint candidate passes.

## Keep the existing targets

The 95% eligible-local target and the 85% eligible-internal parameter, return, and effect target remain campaign targets. Source budgets remain advisory. Report measured counts only after the AST inventory and joint candidate exist. Phase E separately freezes the performance baseline compiler after consolidation closes. Do not infer that compiler from the program-source pin. This cohort design makes no timing claim.
