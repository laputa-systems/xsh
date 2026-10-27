##! Changed-path verification plans and the complete integration gate.
use context
use cargo_steps
use gate_execution
use docker
use docs as documentation
use stage as stages
use stage_contract as contract
use test_workflows as tests

## Gate families owned by the repository's testing contract.
export enum Area { Compile, Syntax, Grammar, Checker, Lowering, Runtime, RouteWitness, Sugar, Tooling, Api, Stdlib, Memory, Docs, Automation, Core, Showcase, Soundness }

## A path that has no known verification owner.
export error GateError = UnmappedPath(path: Str)

## Selected gate families and exact changed native test modules.
export type Selection = {areas: List[Area], native_files: List[Str]}

## Automatic tiers exclude full verification authority.
export enum Tier { Edit, Change, Scenario }

## Runtime limits use seconds; manual cases retain their existing owner lifecycle.
export enum CaseLimit { RuntimeSeconds(Int), ManualOwner }

## Products provided by one retained manual preparation command.
export type ManualProducts = {products: List[tests.Product], command: contract.CommandSpec}

## Commands and aliases cannot grant a manual full request.
export enum CaseExecution {
  RunCommand(contract.CommandSpec), DocumentationCheck, Alias, ManualFull,
  ManualPreparation(ManualProducts),
  ManualCommand(contract.CommandSpec), ManualNative, ManualDocumentation,
}

## A declared case owns its command, prerequisites, products and bounded scope.
export type CatalogCase = {
  id: Str,
  tier: Tier,
  limit: CaseLimit,
  products: List[tests.Product],
  prerequisite_ids: List[Str],
  reasons: List[Str],
  execution: CaseExecution,
}

## A focused plan describes selected cases and unresolved scope.
export type CatalogPlan = {
  tier: Tier,
  cases: List[CatalogCase],
  products: List[tests.Product],
  unresolved: List[Str],
  available_scenarios: List[Str],
}

enum OutputMode { Execute, InspectText, InspectJson }

enum RequestAuthority { FocusedAuthority, FullAuthority }

enum GateRequest { Focused(Tier, List[Str], List[Str], OutputMode, Str?), Full }

## Invalid requests and dependency closures fail before process execution.
export error PlanError = Invalid(detail: Str)

## Compilation and leaf execution remain distinct process boundaries.
export enum CaseAction { Prepare, ExecuteLeaf }

## Focused execution retains raw child status and structured failure causes.
export error ExecutionError = Failed(case_id: Str, raw_status: Int) | TimedOut(case_id: Str, cause: ProcessError) | Cancelled(case_id: Str, raw_status: Int) | Interrupted(case_id: Str, cause: ProcessError) | ProcessFailed(case_id: Str, cause: ProcessError) | BudgetExpired(case_id: Str) | Incomplete(case_id: Str, raw_status: Int) | DiscoveryUnavailable(case_id: Str, detail: Str) | ResultsUnavailable(case_id: Str, detail: Str) | ProductChanged(case_id: Str, product: Str) | ProductUnavailable(product: Str, detail: Str) | SourceChanged(case_id: Str) | SourceUnavailable(detail: Str)

## Only the sole literal --full requests comprehensive verification.
export pure parse_request(arguments: List[Str]) -> Result[GateRequest, Error] {
  return Full when arguments == ["--full"]
  var tier = Change
  var paths: List[Str] = []
  var case_ids: List[Str] = []
  var laputa_root: Str? = null
  var inspect = false
  var machine = false
  var index = 0
  while index < arguments.len() {
    let arg = arguments[index]
    match arg {
      "--plan" => inspect = true
      "--json" => machine = true
      "--tier" => {
        return Err(PlanError.Invalid(detail: "--tier requires edit or change")) when index + 1 >= arguments.len()
        index += 1
        tier = match arguments[index] {
          "edit" => Edit
          "change" => Change
          else => { return Err(PlanError.Invalid(detail: "--tier accepts only edit or change")) }
        }
      }
      "--laputa-root" => {
        return Err(PlanError.Invalid(detail: "--laputa-root requires one absolute path")) when index + 1 >= arguments.len() or laputa_root != null
        index += 1
        let root = arguments[index]
        return Err(PlanError.Invalid(detail: "--laputa-root requires one absolute path")) when ! root.starts_with("/") or root == "/" or ".." in root.split("/") or "." in root.split("/")
        laputa_root = root
      }
      "--case" => {
        return Err(PlanError.Invalid(detail: "--case requires an ID")) when index + 1 >= arguments.len()
        index += 1
        let id = arguments[index]
        return Err(PlanError.Invalid(detail: "--case requires a non-option ID")) when id == "" or id.starts_with("-")
        return Err(PlanError.Invalid(detail: f"duplicate case ID {id}")) when id in case_ids
        case_ids += [id]
      }
      else => {
        return Err(PlanError.Invalid(detail: f"unsupported gate option {arg}")) when arg.starts_with("-")
        paths += [arg]
      }
    }
    index += 1
  }
  return Err(PlanError.Invalid(detail: "--json requires --plan")) when machine and ! inspect
  let output = if machine { InspectJson } else if inspect { InspectText } else { Execute }
  let selected_laputa_ids = [id for id in case_ids if id in laputa_case_ids]
  let selected_core_workloads = [item for item in focused_core_workloads if item.id in case_ids]
  return Err(PlanError.Invalid(detail: "core workload correctness must be selected alone")) when ! selected_core_workloads.is_empty() and case_ids.len() != 1
  return Err(PlanError.Invalid(detail: "Laputa and json-n correctness must be selected separately")) when ! selected_laputa_ids.is_empty() and focused_json_case_id in case_ids
  return Err(PlanError.Invalid(detail: "Laputa and Tokei correctness must be selected separately")) when ! selected_laputa_ids.is_empty() and tokei_scenario_case_id in case_ids
  return Err(PlanError.Invalid(detail: "json-n and Tokei correctness must be selected separately")) when focused_json_case_id in case_ids and tokei_scenario_case_id in case_ids
  return Err(PlanError.Invalid(detail: "Laputa cases require --laputa-root")) when ! selected_laputa_ids.is_empty() and laputa_root == null
  return Err(PlanError.Invalid(detail: "select one Laputa case per request")) when selected_laputa_ids.len() > 1
  return Err(PlanError.Invalid(detail: "--laputa-root requires an explicit Laputa case")) when laputa_root != null and selected_laputa_ids.is_empty()
  Focused(tier, paths, case_ids, output, laputa_root)
}

pure resolve_case(entries: List[CatalogCase], id: Str, explicit: List[Str], visiting: List[Str], completed: List[CatalogCase], authority: RequestAuthority) -> Result[List[CatalogCase], Error] {
  return Err(PlanError.Invalid(detail: f"catalogue cycle at {id}")) when id in visiting
  return completed when id in [entry.id for entry in completed]
  let matches = [entry for entry in entries |> where .id == id]
  return Err(PlanError.Invalid(detail: f"case {id} has {matches.len()} declarations")) when matches.len() != 1
  let entry = matches[0]
  match entry.execution {
    ManualFull | ManualPreparation(_) | ManualCommand(_) | ManualNative | ManualDocumentation => {
      return Err(PlanError.Invalid(detail: f"manual case {id} requires sole --full")) when authority == FocusedAuthority
    }
    RunCommand(_) | DocumentationCheck | Alias => {}
  }
  return Err(PlanError.Invalid(detail: f"scenario {id} requires explicit --case")) when entry.tier == Scenario and id not in explicit
  match entry.limit {
    RuntimeSeconds(seconds) => {
      return Err(PlanError.Invalid(detail: f"case {id} has no positive runtime limit")) when seconds <= 0
      match entry.execution {
        ManualFull | ManualPreparation(_) | ManualCommand(_) | ManualNative | ManualDocumentation => { return Err(PlanError.Invalid(detail: f"manual case {id} must retain its owner lifecycle")) }
        RunCommand(_) | DocumentationCheck | Alias => {}
      }
    }
    ManualOwner => match entry.execution {
      RunCommand(_) | DocumentationCheck => { return Err(PlanError.Invalid(detail: f"case {id} requires a runtime limit")) }
      Alias | ManualFull | ManualPreparation(_) | ManualCommand(_) | ManualNative | ManualDocumentation => {}
    }
  }
  var resolved = completed
  for dependency in entry.prerequisite_ids {
    resolved = resolve_case(entries, dependency, explicit, [@visiting, id], resolved, authority)?
  }
  [@resolved, entry]
}

## Resolves the entire focused dependency closure before returning commands.
export pure resolve_plan(entries: List[CatalogCase], ids: List[Str], tier: Tier) -> Result[CatalogPlan, Error] {
  resolve_selection(entries, ids, ids, tier)
}

pure validate_cases(entries: List[CatalogCase]) -> Result[Unit, Error] {
  var ids: List[Str] = []
  for entry in entries {
    return Err(PlanError.Invalid(detail: "catalogue case IDs cannot be empty")) when entry.id.trim() == ""
    return Err(PlanError.Invalid(detail: f"duplicate catalogue case ID {entry.id}")) when entry.id in ids
    ids += [entry.id]
  }
  return
}

pure resolve_selection(entries: List[CatalogCase], ids: List[Str], explicit_ids: List[Str], tier: Tier) -> Result[CatalogPlan, Error] {
  validate_cases(entries)?
  var resolved: List[CatalogCase] = []
  for id in ids {
    resolved = resolve_case(entries, id, explicit_ids, [], resolved, FocusedAuthority)?
  }
  var products: List[tests.Product] = []
  for entry in resolved {
    return Err(PlanError.Invalid(detail: f"case {entry.id} requires Change; Edit cannot escalate")) when tier == Edit and entry.tier == Change
    for product in entry.products {
      products += [product] when product not in products
    }
  }
  {tier: tier, cases: resolved, products: products, unresolved: [], available_scenarios: []}
}

const cargo_caller_native_files = [
  "dev/tests/test-cargo-steps.xsh",
  "dev/tests/test-lifecycle.xsh",
  "dev/tests/test-gates.xsh",
  "dev/tests/test-native-workflows.xsh",
]

const lsmem_consumer_test_ids = [
  "core/tests/test-util-linux-contexts.xsh::test_util_linux_context_terminal_rooted_memory",
  "core/tests/test-util-linux-io-boundaries.xsh::test_util_linux_lsmem_preserves_raw_sysroot_argv",
]

const cat_byte_stream_test_ids = [
  "core/tests/test-cat.xsh::test_cat_file_and_stdin",
  "core/tests/test-cat.xsh::test_cat_preserves_non_utf8_bytes_across_chunks",
  "core/tests/test-cat.xsh::test_cat_reports_each_failure_and_exits_one",
  "core/tests/test-cat.xsh::test_cat_accepts_non_utf8_file_path_arguments",
]

const cat_external_consumer_test_ids = [
  "core/tests/test-bb-cat.xsh::test_bb_cat_cat_b_ac843bd8",
  "core/tests/test-bb-cat.xsh::test_bb_cat_cat_e_6276c7a9",
  "core/tests/test-bb-cat.xsh::test_bb_cat_cat_n_17793a2a",
  "core/tests/test-bb-cat.xsh::test_bb_cat_cat_prints_a_file_70aae576",
  "core/tests/test-bb-cat.xsh::test_bb_cat_cat_prints_a_file_and_standard_input_ee00c67d",
  "core/tests/test-bb-cat.xsh::test_bb_cat_cat_v_03d8e57e",
  "core/tests/test-gnu-cat.xsh::test_gnu_cat_cat_E_log",
  "core/tests/test-gnu-cat.xsh::test_gnu_cat_cat_buf_log",
  "core/tests/test-gnu-cat.xsh::test_gnu_cat_cat_proc_log",
  "core/tests/test-gnu-cat.xsh::test_gnu_cat_cat_self_log",
  "core/tests/test-gnu-cat.xsh::test_gnu_cat_splice_log",
  "core/tests/test-gnu-cksum.xsh::test_gnu_cksum_md5sum_parallel_log",
  "core/tests/test-gnu-cp.xsh::test_gnu_cp_cp_HL_log",
  "core/tests/test-gnu-cp.xsh::test_gnu_cp_sparse_to_pipe_log",
  "core/tests/test-gnu-wc.xsh::test_gnu_wc_wc_parallel_log",
  "core/tests/test-gnu-misc.xsh::test_gnu_misc_invalid_opt_log",
  "core/tests/test-uu-cat.xsh::test_uu_cat_appending_same_input_output",
  "core/tests/test-uu-cat.xsh::test_uu_cat_cat_non_utf8_paths",
  "core/tests/test-uu-cat.xsh::test_uu_cat_directory",
  "core/tests/test-uu-cat.xsh::test_uu_cat_directory_and_file",
  "core/tests/test-uu-cat.xsh::test_uu_cat_domain_socket",
  "core/tests/test-uu-cat.xsh::test_uu_cat_error_loop",
  "core/tests/test-uu-cat.xsh::test_uu_cat_exit_code_is_one_regardless_of_error_count",
  "core/tests/test-uu-cat.xsh::test_uu_cat_fifo_symlink",
  "core/tests/test-uu-cat.xsh::test_uu_cat_no_options",
  "core/tests/test-uu-cat.xsh::test_uu_cat_no_options_big_input",
  "core/tests/test-uu-cat.xsh::test_uu_cat_non_blank_overrides_number",
  "core/tests/test-uu-cat.xsh::test_uu_cat_non_blank_overrides_number_even_when_present",
  "core/tests/test-uu-cat.xsh::test_uu_cat_numbered_lines_no_trailing_newline",
  "core/tests/test-uu-cat.xsh::test_uu_cat_numbered_lines_with_crlf",
  "core/tests/test-uu-cat.xsh::test_uu_cat_output_multi_files_print_all_chars",
  "core/tests/test-uu-cat.xsh::test_uu_cat_output_multi_files_print_all_chars_repeated",
  "core/tests/test-uu-cat.xsh::test_uu_cat_output_simple",
  "core/tests/test-uu-cat.xsh::test_uu_cat_piped_to_dev_full",
  "core/tests/test-uu-cat.xsh::test_uu_cat_piped_to_dev_null",
  "core/tests/test-uu-cat.xsh::test_uu_cat_piped_to_regular_file",
  "core/tests/test-uu-cat.xsh::test_uu_cat_show_ends_crlf",
  "core/tests/test-uu-cat.xsh::test_uu_cat_squeeze_all_files",
  "core/tests/test-uu-cat.xsh::test_uu_cat_squeeze_all_files_repeated",
  "core/tests/test-uu-cat.xsh::test_uu_cat_squeeze_blank_before_numbering",
  "core/tests/test-uu-cat.xsh::test_uu_cat_stdin_nonprinting_and_endofline",
  "core/tests/test-uu-cat.xsh::test_uu_cat_stdin_nonprinting_and_endofline_repeated",
  "core/tests/test-uu-cat.xsh::test_uu_cat_stdin_nonprinting_and_tabs",
  "core/tests/test-uu-cat.xsh::test_uu_cat_stdin_nonprinting_and_tabs_repeated",
  "core/tests/test-uu-cat.xsh::test_uu_cat_stdin_number_non_blank",
  "core/tests/test-uu-cat.xsh::test_uu_cat_stdin_show_all",
  "core/tests/test-uu-cat.xsh::test_uu_cat_stdin_show_ends",
  "core/tests/test-uu-cat.xsh::test_uu_cat_stdin_show_nonprinting",
  "core/tests/test-uu-cat.xsh::test_uu_cat_stdin_show_tabs",
  "core/tests/test-uu-cat.xsh::test_uu_cat_stdin_squeeze_blank",
  "core/tests/test-uu-cat.xsh::test_uu_cat_stdin_tabs_no_newline",
  "core/tests/test-uu-cat.xsh::test_uu_cat_three_directories_and_file_and_stdin",
  "core/tests/test-uu-cat.xsh::test_uu_cat_u_ignored",
  "core/tests/test-uu-cat.xsh::test_uu_cat_version_help_dev_full",
  "core/tests/test-uu-cat.xsh::test_uu_cat_write_fast_fallthrough_uses_flush",
  "core/tests/test-uu-cat.xsh::test_uu_cat_write_fast_read_error",
  "core/tests/test-uu-cat.xsh::test_uu_cat_write_to_self",
  "core/tests/test-uu-cat.xsh::test_uu_cat_write_to_self_empty",
  "core/tests/test-uu-numfmt-1.xsh::test_uu_numfmt_diagnostics_plain_message_when_stderr_is_a_pipe",
  "core/tests/test-uu-numfmt-1.xsh::test_uu_numfmt_field_diagnostics_plain_message_when_stderr_is_a_pipe",
  "core/tests/test-uu-stat.xsh::test_uu_stat_diagnostics_plain_message_when_stderr_is_a_pipe",
  "core/tests/test-uu-support.xsh::test_uu_support_command_preserves_parallel_input_and_append_output",
  "core/tests/test-uu-support.xsh::test_uu_support_non_utf8_argv",
  "core/tests/test-uu-support.xsh::test_uu_support_path_stdin_redirected_streams",
  "core/tests/test-uu-support.xsh::test_uu_support_redirected_stdout",
  "core/tests/test-uu-support.xsh::test_uu_support_same_input_append_output",
]

const uniq_record_test_ids = [
  "core/tests/test-uniq.xsh::test_uniq_merges_adjacent_lines_and_counts",
  "core/tests/test-uniq.xsh::test_uniq_count_spans_stdin_chunks",
  "core/tests/test-uniq.xsh::test_uniq_skips_fields_chars_and_limits_width",
  "core/tests/test-uniq.xsh::test_uniq_zero_terminated",
]

const find_traversal_test_ids = [
  "core/tests/test-find.xsh::test_find_boolean_expression_prune_and_print0",
  "core/tests/test-find.xsh::test_find_parentheses_negation_depth_and_predicates",
  "core/tests/test-find.xsh::test_find_metadata_survives_delete_for_later_actions",
  "core/tests/test-find.xsh::test_find_accepts_non_utf8_operands",
]

const sort_ordering_test_ids = [
  "core/tests/test-sort.xsh::test_sort_unique_reverse",
  "core/tests/test-sort.xsh::test_sort_numeric_preserves_decimal_precision_and_uniqueness",
  "core/tests/test-sort.xsh::test_sort_separator_null_and_invalid_character_counts",
  "core/tests/test-sort.xsh::test_sort_buffer_size_spills_sorted_runs",
]

const xargs_process_test_ids = [
  "core/tests/test-xargs.xsh::test_xargs_quotes_and_nul_preserve_argument_boundaries",
  "core/tests/test-xargs.xsh::test_xargs_batch_replacement_empty_and_status",
  "core/tests/test-xargs.xsh::test_xargs_parallel_and_fatal_child_status",
  "core/tests/test-xargs.xsh::test_xargs_argfile_preserves_child_standard_input",
  "core/tests/test-xargs.xsh::test_xargs_token_spans_reader_chunk_boundary",
]

const awk_record_test_ids = [
  "core/tests/test-awk.xsh::test_awk_fields_begin_end_and_precedence",
  "core/tests/test-awk.xsh::test_awk_arrays_functions_and_loops",
  "core/tests/test-awk.xsh::test_awk_record_and_field_mutation",
  "core/tests/test-awk.xsh::test_awk_gawk_regex_record_separators_and_rt",
]

const json_structured_test_ids = [
  "tests/xsh/stdlib/json.xsh::test_json_codec_rejects_duplicates_and_preserves_numeric_boundaries",
  "tests/xsh/stdlib/json.xsh::test_json_get_overloads_and_encoded_lines",
  "tests/xsh/stdlib/json.xsh::test_json_set_updates_the_named_position",
  "tests/xsh/stdlib/json.xsh::test_json_remove_drops_the_named_position",
]

const effect_inference_test_ids = [
  "tests/xsh/full-inference-effect-union.xsh::test_imported_effect_union_preserves_promised_bounds_and_independent_types",
  "tests/xsh/full-inference-effect-union.xsh::test_imported_effect_union_reports_a_missing_capability",
  "tests/xsh/full-inference-effect-union.xsh::test_imported_effect_union_keeps_unknown_dynamic_effects_rejected",
  "tests/xsh/full-inference-stream-effects.xsh::test_full_inference_inferred_stream_effect_alias_rejects_empty_bound",
]

type SourceEditCase = {source: Str, id: Str, reason: Str, test_ids: List[Str]}

const source_edit_cases: List[SourceEditCase] = [
  {source: "core/uniq.xsh", id: "core/uniq/records", reason: "adjacent record grouping, chunk boundaries, comparison keys and NUL delimiters", test_ids: uniq_record_test_ids},
  {source: "core/find.xsh", id: "core/find/traversal", reason: "pruned and depth-limited traversal, metadata actions and raw path operands", test_ids: find_traversal_test_ids},
  {source: "core/sort.xsh", id: "core/sort/ordering", reason: "unique and numeric ordering, NUL separators and spilled runs", test_ids: sort_ordering_test_ids},
  {source: "core/xargs.xsh", id: "core/xargs/processes", reason: "argument boundaries, process status, parallel children and stdin streams", test_ids: xargs_process_test_ids},
  {source: "core/awk.xsh", id: "core/awk/records", reason: "record and field mutation, program evaluation and regex separators", test_ids: awk_record_test_ids},
  {source: "src/modules/json.rs", id: "runtime/json/structured-data", reason: "JSON numeric and duplicate boundaries, lines and path mutations", test_ids: json_structured_test_ids},
  {source: "src/sema/check/infer_effects.rs", id: "compiler/effect-inference/correctness", reason: "inferred capability bounds and rejected dynamic effects", test_ids: effect_inference_test_ids},
]

const local_inference_test_ids = [
  "tests/xsh/local-inference.xsh::nullable_local_has_one_fixed_optional_type",
  "tests/xsh/local-inference.xsh::empty_list_collects_loop_contributions_before_earlier_reads",
  "tests/xsh/local-inference.xsh::nullable_local_rejects_incompatible_branch_contributions",
]

const imported_inference_test_ids = [
  "tests/xsh/full-inference-callables.xsh::test_full_inference_imported_generic_alias_instantiates_int_str_bool",
  "tests/xsh/full-inference-callables.xsh::test_full_inference_imported_alias_flows_through_proc_result_and_producer",
  "tests/xsh/full-inference-callables.xsh::test_full_inference_private_module_callables_remain_inaccessible",
]

const current_metric_native_files = [
  "dev/tests/test-consolidation-metrics-current.xsh",
  "dev/tests/test-consolidation-ratchets-current.xsh",
]

pure cargo_caller_tests(changed_path: Str) -> List[Str] {
  match changed_path {
    "dev/cargo_steps.xsh" => [
      "dev/tests/test-cargo-steps.xsh",
      "dev/tests/test-lifecycle.xsh",
    ]
    "dev/cargo_test_runner.sh" => ["dev/tests/test-cargo-steps.xsh"]
    "dev/test_workflows.xsh" => cargo_caller_native_files
    else => []
  }
}

## Determines the verification owner from repository-relative source paths.
export pure areas_for(changed_path: Str) -> Result[List[Area], Error] {
  return [Syntax] when changed_path.starts_with("tests/fixtures/syntax/") or changed_path.starts_with("tests/fixtures/fmt/")
  return [Checker] when changed_path.starts_with("tests/fixtures/sema/")
  return [Runtime] when changed_path.starts_with("tests/fixtures/runtime/") or changed_path.starts_with("tests/fixtures/interactive-parity/")
  return [Memory] when changed_path.starts_with("tests/fixtures/frontend-indexed/")
  return [Api, Stdlib] when changed_path.starts_with("tests/fixtures/modules/")
  return Err(GateError.UnmappedPath(path: changed_path)) when changed_path.starts_with("tests/fixtures/")
  return [Syntax, Grammar, Docs] when changed_path == "src/syntax/grammar.rs" or changed_path.starts_with("src/syntax/grammar/")
  return [Syntax] when changed_path.starts_with("src/syntax/") or changed_path == "tests/grammar.rs" or changed_path == "crates/xsht/tests/syntax.rs"
  return [Memory] when changed_path == "src/frontend_stats.rs"
  return [Checker] when changed_path.starts_with("src/sema/") or changed_path == "src/sema.rs" or changed_path == "tests/sema.rs" or changed_path == "src/loader.rs"
  return [Lowering] when changed_path.starts_with("src/runtime/eval/indexed/") or changed_path == "src/runtime/eval/indexed.rs" or changed_path.starts_with("src/runtime/eval/lower.") or changed_path.starts_with("src/runtime/eval/lower/")
  return [RouteWitness] when changed_path == "src/runtime/eval/route_witness.rs"
  return [Runtime] when changed_path.starts_with("src/runtime/") or changed_path == "src/runtime.rs" or changed_path.starts_with("src/trace") or changed_path == "src/runner.rs" or changed_path.starts_with("tests/runtime/") or changed_path == "tests/runtime.rs"
  return [Sugar] when changed_path.starts_with("crates/xsht/src/sugar_") or changed_path.starts_with("crates/xsht/src/desugar")
  return [Tooling] when changed_path.starts_with("crates/xsht/src/") or changed_path.starts_with("crates/xsht/tests/")
  return [Api] when changed_path.starts_with("crates/xsh-registry/") or changed_path.starts_with("src/api/") or changed_path == "tests/libxsh_api.rs"
  return [Stdlib, Api] when changed_path == "src/stdlib.rs" or changed_path.starts_with("stdlib/") or changed_path.starts_with("src/modules/") or changed_path == "src/modules.rs"
  return [Docs, Api] when changed_path.starts_with("docs/snippets/api/")
  return [Docs] when changed_path.starts_with("docs/")
  return [Core] when changed_path.starts_with("core/")
  return [Showcase] when changed_path.starts_with("showcase/")
  return [Automation] when changed_path.starts_with("dev/") or changed_path.starts_with("tools/") or changed_path == "Makefile"
  return [Soundness] when changed_path.starts_with("crates/xsh-fuzz/")
  return [Runtime] when changed_path.starts_with("crates/xsh-net/") or changed_path == "tests/linux_priv.rs"
  return [Compile, Api] when changed_path == "Cargo.toml" or changed_path == "Cargo.lock" or changed_path == "build.rs" or changed_path == "rust-toolchain.toml" or changed_path.starts_with(".cargo/")
  return [Compile] when changed_path.starts_with("src/") or changed_path.starts_with("crates/") or changed_path.starts_with("tests/") or changed_path == "AGENTS.md" or changed_path == "TODO.md" or changed_path == "xsht-config.ini" or changed_path == "Dockerfile.test"
  Err(GateError.UnmappedPath(path: changed_path))
}

## Deduplicates gates while retaining the order in which their owners appear.
export pure select(paths: List[Str]) -> Result[Selection, Error] {
  var areas: List[Area] = []
  var native_files = []
  for changed_path in paths {
    if changed_path == "dev/consolidation_metrics_current.xsh" or changed_path == "dev/consolidation/metric-observer-successor.json" or changed_path in current_metric_native_files {
      for file in current_metric_native_files {
        native_files += [file] when file not in native_files
      }
      continue
    }
    if changed_path == "dev/consolidation/current_source_policy.xsh" {
      native_files += ["dev/tests/test-consolidation-ratchets-current.xsh"] when "dev/tests/test-consolidation-ratchets-current.xsh" not in native_files
      continue
    }
    let caller_tests = cargo_caller_tests(changed_path)
    if ! caller_tests.is_empty() {
      for file in caller_tests {
        native_files += [file] when file not in native_files
      }
      continue
    }
    if changed_path.split("/").len() == 3 and changed_path.starts_with("core/tests/test-") and changed_path.ends_with(".xsh") {
      native_files += [changed_path] when changed_path not in native_files
      continue
    }
    if ! changed_path.starts_with("tests/fixtures/") and changed_path.ends_with(".xsh") and (changed_path.starts_with("tests/") or changed_path.starts_with("dev/tests/") or changed_path.starts_with("showcase/tests/")) {
      native_files += [changed_path] when changed_path not in native_files
      continue
    }
    for area in areas_for(changed_path)? {
      areas += [area] when area not in areas
    }
  }
  {areas: areas, native_files: native_files}
}

pure rust_gate(ctx: context.Context, profile: tests.TestProfile, name: Str, args: List[Str], debug = false) -> Result[contract.CommandSpec, Error] {
  let profile_args = if debug { [] } else { ["--profile", tests.profile_name(profile)] }
  stages.command(name, ctx.target.triple, "cargo",
    ["cargo", "test", "--locked", "--target", ctx.target.triple, @profile_args, @args],
    ctx.root, tests.cargo_environment(ctx)?)
}

pure native_gate(ctx: context.Context, profile: tests.TestProfile, filter: Str) -> contract.CommandSpec {
  let xsht = tests.product_path(ctx, profile, tests.Xsht).display()
  stages.command(f"native-{filter}", ctx.target.triple, xsht, [xsht, "test", filter], ctx.root, {})
}

pure native_exact_ids_gate(ctx: context.Context, profile: tests.TestProfile, name: Str, ids: List[Str]) -> contract.CommandSpec {
  let xsht = tests.product_path(ctx, profile, tests.Xsht).display()
  var argv = [xsht, "test"]
  for id in ids { argv += ["--exact-id", id] }
  stages.command(f"native-{name}", ctx.target.triple, xsht, argv, ctx.root, {})
}

pure native_exact_gate(ctx: context.Context, profile: tests.TestProfile, name: Str, exact_test: Str) -> contract.CommandSpec {
  let xsht = tests.product_path(ctx, profile, tests.Xsht).display()
  stages.command(f"native-{name}", ctx.target.triple, xsht, [xsht, "test", "--exact", exact_test], ctx.root, {})
}

pure requires_process_products(area: Area) -> Bool {
  match area {
    Compile => false
    Syntax => true
    Grammar => true
    Checker => true
    Lowering => true
    Runtime => true
    RouteWitness => true
    Sugar => true
    Tooling => true
    Api => true
    Stdlib => true
    Memory => false
    Docs => false
    Automation => true
    Core => true
    Showcase => true
    Soundness => false
  }
}

pure required_products(selection: Selection) -> List[tests.Product] {
  guard selection.native_files.is_empty() else {
    var cargo_caller_only = selection.areas.is_empty()
    for file in selection.native_files {
      cargo_caller_only = false when file not in cargo_caller_native_files
    }
    return [tests.Xsh, tests.Xsht] when cargo_caller_only
    return tests.all_products()
  }
  for area in selection.areas {
    return tests.all_products() when requires_process_products(area)
  }
  return [tests.Xsh, tests.Xsht] when Docs in selection.areas
  return [tests.Xsht] when Memory in selection.areas
  []
}

## Maps each selected area to its focused compiler or Rust gate and native
## behavioral module. No area runs the whole native corpus.
export pure lane_plan(ctx: context.Context, profile: tests.TestProfile, selection: Selection) -> Result[List[contract.CommandSpec], Error] {
  let root_target = ["-p", "xsh", "--test", "integration"]
  let xsht_target = ["-p", "xsht", "--test", "integration"]
  let root_lib = ["-p", "xsh", "--lib"]
  var plan = tests.product_builds(ctx, profile, required_products(selection))?
  for area in selection.areas {
    match area {
      Compile => plan += [stages.command("check-compile", ctx.target.triple, "cargo",
        ["cargo", "check", "--locked", "--target", ctx.target.triple, "-p", "xsh", "-p", "xsht", "-p", "xsh-fuzz", "--all-targets"],
        ctx.root, tests.cargo_environment(ctx)?)],
      Syntax => plan += [rust_gate(ctx, profile, "test-syntax", [@xsht_target, "syntax::", "--", @tests.sibling_syntax_exclusions])?, rust_gate(ctx, profile, "test-grammar-rust", [@root_target, "grammar::"])?, native_gate(ctx, profile, "tests/xsh/formatter.xsh")],
      Grammar => plan += [rust_gate(ctx, profile, "test-grammar", ["-p", "xsh-fuzz", "--test", "soundness", "generated_programs_are_grammar_sentences"])?, native_gate(ctx, profile, "tests/xsh/tooling-commands.xsh")],
      Checker => plan += [rust_gate(ctx, profile, "test-sema", [@root_target, "sema::"])?, native_gate(ctx, profile, "tests/xsh/checker-")],
      Lowering => plan += [rust_gate(ctx, profile, "test-lowering", [@root_lib, "runtime::eval"], debug: true)?, rust_gate(ctx, profile, "test-type-agreement", [@root_lib, "corpus_annotations_agree_with_binding_facts"], debug: true)?, native_gate(ctx, profile, "tests/xsh/lowering-coverage.xsh")],
      Runtime => plan += [rust_gate(ctx, profile, "test-runtime", [@root_target, "runtime::", "--", "--skip", "runtime::coverage::"])?, native_gate(ctx, profile, "tests/xsh/lowering-coverage.xsh")],
      RouteWitness => {
        plan += [rust_gate(ctx, profile, "test-route-witness-unit", [@root_lib, "runtime::eval::route_witness::tests"], debug: true)?]
        plan += [rust_gate(ctx, profile, "test-route-witness-inherited-env", [@root_lib, "runtime_env_route_witness_tests"], debug: true)?]
        plan += [rust_gate(ctx, profile, "test-route-witness-cli", [@xsht_target, "route_witness"])?] when Tooling not in selection.areas
        plan += [native_exact_gate(ctx, profile, "route-witness-env", "tests/xsh/stdlib/env.xsh::test_route_witness_prefix_names_are_ordinary_environment_entries")] when "tests/xsh/stdlib/env.xsh" not in selection.native_files
      },
      Sugar => plan += [rust_gate(ctx, profile, "test-sugar", ["-p", "xsht", "--lib", "sugar_expansion_tests"], debug: true)?, rust_gate(ctx, profile, "test-desugar-unit", ["-p", "xsht", "--lib", "desugar_tests"], debug: true)?, rust_gate(ctx, profile, "test-desugar", [@xsht_target, "desugar::"])?, native_gate(ctx, profile, "tests/xsh/desugar.xsh")],
      Tooling => plan += [rust_gate(ctx, profile, "test-tooling", [@xsht_target, "--", @tests.sibling_syntax_exclusions, @tests.xsht_integration_exclusions])?, native_gate(ctx, profile, "tests/xsh/tooling-")],
      Api => plan += [rust_gate(ctx, profile, "test-libxsh-api", [@root_target, "libxsh_api"])?, rust_gate(ctx, profile, "test-registry", ["-p", "xsh-registry", "--lib"], debug: true)?, rust_gate(ctx, profile, "test-signatures", [@root_lib, "modules::signature"], debug: true)?, rust_gate(ctx, profile, "test-api", [@xsht_target, "api::"])?, native_gate(ctx, profile, "tests/xsh/api-tool.xsh"), stages.command("check-api-snippets", ctx.target.triple, tests.product_path(ctx, profile, tests.Xsht).display(), [tests.product_path(ctx, profile, tests.Xsht).display(), "check", "docs/snippets/api"], ctx.root, {})],
      Stdlib => plan += [rust_gate(ctx, profile, "test-stdlib-catalog", [@root_lib, "stdlib::tests::every_catalog_module_parses_checks_and_lowers"], debug: true)?, native_gate(ctx, profile, "tests/xsh/stdlib")],
      Memory => plan += [rust_gate(ctx, profile, "test-frontend-memory", [@root_lib, "frontend_stats::tests"], debug: true)?, stages.command("frontend-memory-report", ctx.target.triple, tests.product_path(ctx, profile, tests.Xsht).display(), [tests.product_path(ctx, profile, tests.Xsht).display(), "frontend-stats", "--json", "tests/fixtures/frontend-indexed"], ctx.root, {})],
      Docs => {},
      Automation => plan += [native_gate(ctx, profile, "dev/tests")],
      Core => plan += [native_gate(ctx, profile, "core/tests")],
      Showcase => plan += [native_gate(ctx, profile, "showcase/tests")],
      Soundness => plan += [rust_gate(ctx, profile, "test-soundness", ["-p", "xsh-fuzz", "--test", "soundness"]) ?],
    }
  }
  for file in selection.native_files {
    plan += [native_gate(ctx, profile, file)]
  }
  plan
}

proc docs_gate(ctx: context.Context, profile: tests.TestProfile) [fs, process, env, error, io] {
  documentation.check(ctx.root, documentation.DocTools(
    xsh: tests.product_path(ctx, profile, tests.Xsh), xsht: tests.product_path(ctx, profile, tests.Xsht)))
}

## Automatic repository checks include the retained command contracts and
## source-bound consolidation ratchets.
export pure batch_checks(ctx: context.Context, profile: tests.TestProfile) -> List[contract.CommandSpec] {
  let xsht = tests.product_path(ctx, profile, tests.Xsht).display()
  let xsh = tests.product_path(ctx, profile, tests.Xsh).display()
  [
    stages.command("batch-check", ctx.target.triple, xsht, [xsht, "check"], ctx.root, {}),
    stages.command("batch-lint", ctx.target.triple, xsht, [xsht, "lint"], ctx.root, {}),
    stages.command("batch-diff", ctx.target.triple, "git", ["git", "diff", "--check"], ctx.root, {}),
    stages.command("batch-ratchets", ctx.target.triple, xsh,
      [xsh, "dev/consolidation/metrics.xsh", "check-current", "--root", ctx.root.display(),
        "--source-root", "src", "--xsht-source-root", "crates/xsht/src",
        "--baseline", "dev/consolidation/baseline-blanket-scope.json", "--baseline-provenance", "dev/consolidation/blanket-metric-provenance-source-advisory.json",
        "--envelope", "dev/consolidation/envelope-upstream.json",
        "--output", ".work/consolidation/current.json", "--audit", ".work/consolidation/rises.json"], ctx.root, {}),
  ]
}

## Runs the complete sequence at an integration boundary without source-tree
## formatters or autofixers.
export proc batch(ctx: context.Context, request: GateRequest) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  let profile = tests.execution_profile()?
  dispatch_catalogue(ctx, profile, request,
    catalogue(ctx, profile, Selection(areas: [], native_files: []), ["explicit Full request"])?)
}

## Dispatches live catalogue entries only with an opaque, separately parsed Full request.
export proc dispatch_catalogue(ctx: context.Context, profile: tests.TestProfile, request: GateRequest, entries: List[CatalogCase]) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  present(dispatch_manual(ctx, profile, request, entries))
}

proc dispatch_manual(ctx: context.Context, profile: tests.TestProfile, request: GateRequest, entries: List[CatalogCase]) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  match request {
    Full => {
      let plan = resolve_full(entries)?
      return execute_full(ctx, profile, plan)
    }
    Focused(_, _, _, _, _) => { return Err(PlanError.Invalid(detail: "manual dispatch requires the sole literal --full request")) }
  }
}

proc execute_full(ctx: context.Context, profile: tests.TestProfile, plan: CatalogPlan) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  for entry in plan.cases {
    match entry.execution {
      ManualPreparation(preparation) => tests.run_commands([preparation.command])
      ManualCommand(spec) => tests.run_commands([spec])
      RunCommand(_) | DocumentationCheck => execute_plan(ctx, profile,
        CatalogPlan(tier: entry.tier, cases: [entry], products: [], unresolved: [], available_scenarios: []), [], null)
      ManualNative => tests.native(ctx, profile, ["--jobs", "4"])
      ManualDocumentation => docs_gate(ctx, profile)
      ManualFull | Alias => {}
    }
  }
}

pure tier_name(tier: Tier) -> Str {
  match tier { Edit => "edit", Change => "change", Scenario => "scenario" }
}

pure declared_case(spec: contract.CommandSpec, products: List[tests.Product], reasons: List[Str]) -> CatalogCase {
  let scenario = spec.stage == soundness_scenario_id
  {id: spec.stage, tier: if scenario { Scenario } else { Change },
    limit: RuntimeSeconds(if scenario { 1200 } else { 300 }), products: products,
    prerequisite_ids: [], reasons: reasons, execution: RunCommand(spec)}
}

const laputa_case_id = "laputa/pm.make/plan"
const laputa_kbuild_case_id = "laputa/kbuild/discovery"
const laputa_kbuild_graph_case_id = "laputa/kbuild/graph-cache"
const laputa_kbuild_make_case_id = "laputa/kbuild/make-semantics"
const laputa_reuse_case_id = "laputa/pm.execute/reuse-count"
const laputa_root_case_id = "laputa/pm.root/compose"
const laputa_inventory_case_id = "laputa/pm.plan/inventory"
const laputa_world_case_id = "laputa/world/host-plan"
const laputa_case_ids = [laputa_case_id, laputa_kbuild_case_id, laputa_kbuild_graph_case_id, laputa_kbuild_make_case_id, laputa_reuse_case_id, laputa_root_case_id, laputa_inventory_case_id, laputa_world_case_id]
const cat_byte_stream_case_id = "core/cat/byte-stream"
const cat_full_module_case_id = "core/cat/full-module"
const cat_external_consumers_case_id = "core/cat/external-consumers"
const focused_adapter_case_id = "verification/focused-adapter"
const performance_check_case_id = "verification/performance-check"
const tokei_corpus_case_id = "verification/tokei-corpus"
const tokei_adapter_case_id = "verification/tokei-scenario"
const resource_diagnostics_case_id = "verification/resource-diagnostics"
const laputa_adapter_case_id = "verification/laputa-adapter"
const native_leases_case_id = "verification/native-leases"
const focused_json_case_id = "performance/json-n/correctness"
const frontend_scenario_case_id = "performance/frontend-edit/correctness"
const core_standalone_trace_cases = [
  {id: "performance/core-wc-small/trace", name: "core-wc-small"},
  {id: "performance/core-wc-medium/trace", name: "core-wc-medium"},
  {id: "performance/core-sort-small/trace", name: "core-sort-small"},
  {id: "performance/core-awk-small/trace", name: "core-awk-small"},
  {id: "performance/core-traversal-small/trace", name: "core-traversal-small"},
  {id: "performance/core-traversal-medium/trace", name: "core-traversal-medium"},
  {id: "performance/core-traversal-large/trace", name: "core-traversal-large"},
  {id: "performance/core-spawn-small/trace", name: "core-spawn-small"},
]
const core_pipeline_cases = [
  {id: "performance/core-pipeline-small/correctness", shape: "small"},
  {id: "performance/core-pipeline-medium/correctness", shape: "medium"},
  {id: "performance/core-pipeline-large/correctness", shape: "large"},
]
const focused_core_workloads = [
  {id: "performance/core-wc-small/correctness", name: "core-wc-small"},
  {id: "performance/core-wc-medium/correctness", name: "core-wc-medium"},
  {id: "performance/core-wc-large/correctness", name: "core-wc-large"},
  {id: "performance/core-sort-small/correctness", name: "core-sort-small"},
  {id: "performance/core-sort-medium/correctness", name: "core-sort-medium"},
  {id: "performance/core-sort-large/correctness", name: "core-sort-large"},
  {id: "performance/core-traversal-small/correctness", name: "core-traversal-small"},
  {id: "performance/core-traversal-medium/correctness", name: "core-traversal-medium"},
  {id: "performance/core-traversal-large/correctness", name: "core-traversal-large"},
  {id: "performance/core-awk-small/correctness", name: "core-awk-small"},
  {id: "performance/core-awk-medium/correctness", name: "core-awk-medium"},
  {id: "performance/core-awk-large/correctness", name: "core-awk-large"},
]
const tokei_scenario_case_id = "showcase/tokei/uutils-rust-correctness"
const constraint_solver_case_id = "compiler/constraint-solver/correctness"

pure laputa_products(case_id: Str) -> List[tests.Product] {
  if case_id == laputa_reuse_case_id { [tests.Xsh, tests.Xsht, tests.Xshi] } else { [tests.Xsh, tests.Xsht] }
}

type ProductObservation = {digest: Str, mode: Int}

const soundness_scenario_id = "test-soundness"

pure soundness_owners(areas: List[Area]) -> Bool {
  ! [area for area in areas if area in [Checker, Lowering, Runtime, RouteWitness, Syntax, Grammar, Soundness]].is_empty()
}

## Derives cases from the established owner plans without a second test registry.
export pure catalogue(ctx: context.Context, profile: tests.TestProfile, selection: Selection, reasons: List[Str]) -> Result[List[CatalogCase], Error] {
  var entries: List[CatalogCase] = []
  let selected_stages = [spec.stage for spec in lane_plan(ctx, profile, selection)?]
  for area in selection.areas {
    let owner = Selection(areas: [area], native_files: [])
    for spec in lane_plan(ctx, profile, owner)? {
      continue when spec.stage == "test-products" or spec.stage not in selected_stages
      if spec.stage not in [entry.id for entry in entries] {
        entries += [declared_case(spec, required_products(owner), reasons)]
      }
    }
  }
  if Docs in selection.areas {
    entries += [CatalogCase(id: "check-docs", tier: Change, limit: RuntimeSeconds(300),
      products: [tests.Xsh, tests.Xsht], prerequisite_ids: [], reasons: reasons,
      execution: DocumentationCheck)]
  }
  for file in selection.native_files {
    let owner = Selection(areas: [], native_files: [file])
    let spec = native_gate(ctx, profile, file)
    entries += [declared_case(spec, required_products(owner), reasons)] when spec.stage not in [entry.id for entry in entries]
  }
  entries += [CatalogCase(id: "showcase/tokei/correctness", tier: Edit,
    limit: RuntimeSeconds(30), products: [tests.Xsh, tests.Xsht], prerequisite_ids: [],
    reasons: ["showcase/tokei.xsh output and classification contract"],
    execution: RunCommand(native_gate(ctx, profile, "showcase/tests/test-tokei.xsh")))]
  entries += [CatalogCase(id: "core/lsmem/correctness", tier: Edit,
    limit: RuntimeSeconds(30), products: [tests.Xsh, tests.Xsht], prerequisite_ids: [],
    reasons: ["core/lsmem.xsh rooted memory inventory contract"],
    execution: RunCommand(native_gate(ctx, profile, "core/tests/test-lsmem.xsh")))]
  entries += [CatalogCase(id: "core/lsmem/consumers", tier: Change,
    limit: RuntimeSeconds(300), products: [tests.Xsh, tests.Xsht], prerequisite_ids: [],
    reasons: ["native tests outside test-lsmem.xsh that run the lsmem applet"],
    execution: RunCommand(native_exact_ids_gate(ctx, profile, "core/lsmem/consumers", lsmem_consumer_test_ids)))]
  entries += [CatalogCase(id: cat_byte_stream_case_id, tier: Edit,
    limit: RuntimeSeconds(30), products: [tests.Xsh, tests.Xsht], prerequisite_ids: [],
    reasons: ["cat byte-preserving file/stdin streams, malformed input bytes, and operand errors"],
    execution: RunCommand(native_exact_ids_gate(ctx, profile, cat_byte_stream_case_id, cat_byte_stream_test_ids)))]
  entries += [CatalogCase(id: cat_full_module_case_id, tier: Change,
    limit: RuntimeSeconds(300), products: [tests.Xsh, tests.Xsht], prerequisite_ids: [],
    reasons: ["full native core/cat.xsh behavior module"],
    execution: RunCommand(native_gate(ctx, profile, "core/tests/test-cat.xsh")))]
  entries += [CatalogCase(id: cat_external_consumers_case_id, tier: Change,
    limit: RuntimeSeconds(300), products: [tests.Xsh, tests.Xsht], prerequisite_ids: [],
    reasons: ["exact native tests that exercise cat and its external consumers"],
    execution: RunCommand(native_exact_ids_gate(ctx, profile, cat_external_consumers_case_id, cat_external_consumer_test_ids)))]
  for case in source_edit_cases {
    entries += [CatalogCase(id: case.id, tier: Edit,
      limit: RuntimeSeconds(120), products: [tests.Xsh, tests.Xsht], prerequisite_ids: [],
      reasons: [case.reason],
      execution: RunCommand(native_exact_ids_gate(ctx, profile, case.id, case.test_ids)))]
  }
  entries += [CatalogCase(id: "compiler/local-inference/correctness", tier: Edit,
    limit: RuntimeSeconds(120), products: [tests.Xsh, tests.Xsht], prerequisite_ids: [],
    reasons: ["local nullable and collection inference behavior and diagnostics"],
    execution: RunCommand(native_exact_ids_gate(ctx, profile, "compiler/local-inference/correctness", local_inference_test_ids)))]
  entries += [CatalogCase(id: "compiler/imported-inference/correctness", tier: Edit,
    limit: RuntimeSeconds(120), products: [tests.Xsh, tests.Xsht], prerequisite_ids: [],
    reasons: ["imported callable instantiation, producer propagation, and module privacy"],
    execution: RunCommand(native_exact_ids_gate(ctx, profile, "compiler/imported-inference/correctness", imported_inference_test_ids)))]
  entries += [CatalogCase(id: "compiler/inference-work/correctness", tier: Edit,
    limit: RuntimeSeconds(120), products: [], prerequisite_ids: [],
    reasons: ["inference work collection isolation and instantiation counts"],
    execution: RunCommand(rust_gate(ctx, profile, "compiler/inference-work/correctness",
      ["-p", "xsh", "--lib", "frontend_stats::tests::inference_work_invocations_are_isolated_and_generic_calls_count_instantiations", "--", "--exact"], debug: true)?))]
  entries += [CatalogCase(id: "compiler/frontend-disposal/schemes", tier: Scenario,
    limit: RuntimeSeconds(120), products: [], prerequisite_ids: [],
    reasons: ["execute imported schemes on both runtime routes after frontend state is dropped"],
    execution: RunCommand(rust_gate(ctx, profile, "compiler/frontend-disposal/schemes",
      ["-p", "xsh", "--lib", "runtime::eval::indexed::full::tests::user_schemes_execute_after_frontend_drop_on_both_routes", "--", "--exact"], debug: true)?))]
  entries += [CatalogCase(id: "compiler/frontend-disposal/compact-entry", tier: Scenario,
    limit: RuntimeSeconds(120), products: [], prerequisite_ids: [],
    reasons: ["execute compact entry after frontend and adapter scratch is dropped"],
    execution: RunCommand(rust_gate(ctx, profile, "compiler/frontend-disposal/compact-entry",
      ["-p", "xsh", "--lib", "runtime::eval::indexed::full::tests::compact_entry_executes_after_all_frontend_and_adapter_scratch_is_dropped", "--", "--exact"], debug: true)?))]
  entries += [CatalogCase(id: "compiler/frontend-retention/correctness", tier: Edit,
    limit: RuntimeSeconds(120), products: [], prerequisite_ids: [],
    reasons: ["stable retained frontend measurement columns and reconciliation"],
    execution: RunCommand(rust_gate(ctx, profile, "compiler/frontend-retention/correctness",
      ["-p", "xsh", "--lib", "frontend_stats::tests::indexed_execution_retained_columns_are_stable", "--", "--exact"], debug: true)?))]
  entries += [CatalogCase(id: constraint_solver_case_id, tier: Edit,
    limit: RuntimeSeconds(120), products: [], prerequisite_ids: [],
    reasons: ["constraint trial rollback and row continuation against an independent oracle"],
    execution: RunCommand(rust_gate(ctx, profile, constraint_solver_case_id,
      ["-p", "xsh", "--lib", "sema::constraints::reference_tests::production_trials_restore_generalization_and_row_continuations", "--", "--exact"], debug: true)?))]
  entries += [CatalogCase(id: "verification/results", tier: Change,
    limit: RuntimeSeconds(30), products: [], prerequisite_ids: [],
    reasons: ["result reconciliation contract"],
    execution: RunCommand(stages.command("verification/results", ctx.target.triple, "python3",
      ["python3", "-B", "-m", "unittest", "dev.performance.test_results"], ctx.root, {})))]
  entries += [CatalogCase(id: "verification/source", tier: Change,
    limit: RuntimeSeconds(30), products: [], prerequisite_ids: [],
    reasons: ["source snapshot contract"],
    execution: RunCommand(stages.command("verification/source", ctx.target.triple, "python3",
      ["python3", "-B", "-m", "unittest", "dev.performance.test_gate_source"], ctx.root, {})))]
  entries += [CatalogCase(id: focused_adapter_case_id, tier: Change,
    limit: RuntimeSeconds(90), products: [], prerequisite_ids: [],
    reasons: ["focused workload and Docker cleanup adapter contracts"],
    execution: RunCommand(stages.command(focused_adapter_case_id, ctx.target.triple, "python3",
      ["python3", "-B", "-m", "unittest", "dev.performance.test_focused", "dev.performance.test_check.AdapterDeadlineTests"], ctx.root,
      {TMPDIR: fp"{ctx.root}/.work/tmp"})))]
  entries += [CatalogCase(id: "verification/consolidation-harness", tier: Change,
    limit: RuntimeSeconds(90), products: [], prerequisite_ids: [],
    reasons: ["comparative runner, fixtures, and process cleanup contracts"],
    execution: RunCommand(stages.command("verification/consolidation-harness", ctx.target.triple, "python3",
      ["python3", "-B", "-m", "unittest", "bench.consolidation.test_harness"], ctx.root,
      {TMPDIR: fp"{ctx.root}/.work/tmp", PYTHONPATH: fp"{ctx.root}/bench/consolidation"})))]
  entries += [CatalogCase(id: "verification/frozen-witnesses", tier: Change,
    limit: RuntimeSeconds(30), products: [], prerequisite_ids: [],
    reasons: ["frozen inference witness tooling contracts"],
    execution: RunCommand(stages.command("verification/frozen-witnesses", ctx.target.triple, "python3",
      ["python3", "-B", "-m", "unittest", "dev.performance.corpus.test_frozen_witnesses"], ctx.root,
      {TMPDIR: fp"{ctx.root}/.work/tmp", PYTHONPATH: fp"{ctx.root}/dev/performance/corpus"})))]
  entries += [CatalogCase(id: "verification/inference-workloads", tier: Change,
    limit: RuntimeSeconds(30), products: [], prerequisite_ids: [],
    reasons: ["inference workload generation and measurement contracts"],
    execution: RunCommand(stages.command("verification/inference-workloads", ctx.target.triple, "python3",
      ["python3", "-B", "-m", "unittest", "dev.performance.test_inference_workloads", "dev.performance.test_inference_measurement", "dev.performance.test_frontend_cases"], ctx.root,
      {TMPDIR: fp"{ctx.root}/.work/tmp", PYTHONPATH: fp"{ctx.root}/dev/performance"})))]
  entries += [CatalogCase(id: "verification/core-workloads", tier: Change,
    limit: RuntimeSeconds(30), products: [], prerequisite_ids: [],
    reasons: ["core workload fixture and oracle contracts"],
    execution: RunCommand(stages.command("verification/core-workloads", ctx.target.triple, "python3",
      ["python3", "-B", "-m", "unittest", "dev.performance.test_core_cases"], ctx.root,
      {TMPDIR: fp"{ctx.root}/.work/tmp"})))]
  entries += [CatalogCase(id: performance_check_case_id, tier: Change,
    limit: RuntimeSeconds(90), products: [], prerequisite_ids: [],
    reasons: ["performance check and workload catalogue contracts"],
    execution: RunCommand(stages.command(performance_check_case_id, ctx.target.triple, "python3",
      ["python3", "-B", "-m", "unittest", "dev.performance.test_check"], ctx.root,
      {TMPDIR: fp"{ctx.root}/.work/tmp"})))]
  entries += [CatalogCase(id: tokei_corpus_case_id, tier: Change,
    limit: RuntimeSeconds(30), products: [], prerequisite_ids: [],
    reasons: ["pinned Tokei corpus projection and file-set reconciliation"],
    execution: RunCommand(stages.command(tokei_corpus_case_id, ctx.target.triple, "python3",
      ["python3", "-B", "-m", "unittest", "dev.performance.test_tokei_corpus"], ctx.root,
      {TMPDIR: fp"{ctx.root}/.work/tmp"})))]
  entries += [CatalogCase(id: tokei_adapter_case_id, tier: Change,
    limit: RuntimeSeconds(30), products: [], prerequisite_ids: [],
    reasons: ["Tokei scenario admission, bindings and reconciliation"],
    execution: RunCommand(stages.command(tokei_adapter_case_id, ctx.target.triple, "python3",
      ["python3", "-B", "-m", "unittest", "dev.performance.test_tokei_scenario"], ctx.root,
      {TMPDIR: fp"{ctx.root}/.work/tmp"})))]
  entries += [CatalogCase(id: resource_diagnostics_case_id, tier: Change,
    limit: RuntimeSeconds(30), products: [], prerequisite_ids: [],
    reasons: ["resource observer and diagnostic contract"],
    execution: RunCommand(stages.command(resource_diagnostics_case_id, ctx.target.triple, "python3",
      ["python3", "-B", "-m", "unittest", "dev.performance.test_resources"], ctx.root,
      {TMPDIR: fp"{ctx.root}/.work/tmp"})))]
  entries += [CatalogCase(id: laputa_adapter_case_id, tier: Change,
    limit: RuntimeSeconds(90), products: [], prerequisite_ids: [],
    reasons: ["Laputa host adapter contract"],
    execution: RunCommand(stages.command(laputa_adapter_case_id, ctx.target.triple, "python3",
      ["python3", "-B", "-m", "unittest", "dev.performance.test_laputa_plan"], ctx.root,
      {TMPDIR: fp"{ctx.root}/.work/tmp"})))]
  entries += [CatalogCase(id: native_leases_case_id, tier: Change,
    limit: RuntimeSeconds(90), products: [], prerequisite_ids: [],
    reasons: ["native lease and cleanup quarantine contract"],
    execution: RunCommand(stages.command(native_leases_case_id, ctx.target.triple, "python3",
      ["python3", "-B", "dev/util-linux/test_leases.py"], ctx.root,
      {TMPDIR: fp"{ctx.root}/.work/tmp"})))]
  entries += [CatalogCase(id: laputa_case_id, tier: Scenario,
    limit: RuntimeSeconds(300), products: laputa_products(laputa_case_id), prerequisite_ids: [],
    reasons: ["Laputa pure PM task graph planning contract; explicit absolute Git root required"],
    execution: Alias)]
  entries += [CatalogCase(id: laputa_kbuild_case_id, tier: Scenario,
    limit: RuntimeSeconds(300), products: laputa_products(laputa_kbuild_case_id), prerequisite_ids: [],
    reasons: ["Laputa synthetic Kbuild discovery and planning contract; explicit absolute Git root required"],
    execution: Alias)]
  entries += [CatalogCase(id: laputa_kbuild_graph_case_id, tier: Scenario,
    limit: RuntimeSeconds(300), products: laputa_products(laputa_kbuild_graph_case_id), prerequisite_ids: [],
    reasons: ["synthetic Kbuild graph parity, cache invalidation and plan output"],
    execution: Alias)]
  entries += [CatalogCase(id: laputa_kbuild_make_case_id, tier: Scenario,
    limit: RuntimeSeconds(300), products: laputa_products(laputa_kbuild_make_case_id), prerequisite_ids: [],
    reasons: ["Kbuild Make assignment, nested function and compile-flag contracts"],
    execution: Alias)]
  entries += [CatalogCase(id: laputa_reuse_case_id, tier: Scenario,
    limit: RuntimeSeconds(300), products: laputa_products(laputa_reuse_case_id), prerequisite_ids: [],
    reasons: ["Laputa PM artifact reuse and invalidation contract; explicit absolute Git root required"],
    execution: Alias)]
  entries += [CatalogCase(id: laputa_root_case_id, tier: Scenario,
    limit: RuntimeSeconds(300), products: laputa_products(laputa_root_case_id), prerequisite_ids: [],
    reasons: ["isolated Laputa root inventory, ownership, links and collision contracts"],
    execution: Alias)]
  entries += [CatalogCase(id: laputa_inventory_case_id, tier: Scenario,
    limit: RuntimeSeconds(300), products: laputa_products(laputa_inventory_case_id), prerequisite_ids: [],
    reasons: ["isolated Laputa package graph, reuse and executor provenance contracts"],
    execution: Alias)]
  entries += [CatalogCase(id: laputa_world_case_id, tier: Scenario,
    limit: RuntimeSeconds(300), products: laputa_products(laputa_world_case_id), prerequisite_ids: [],
    reasons: ["isolated world host-plan selection and mirror environment contract"],
    execution: Alias)]
  entries += [CatalogCase(id: focused_json_case_id, tier: Scenario,
    limit: RuntimeSeconds(300), products: [tests.Xsh], prerequisite_ids: [],
    reasons: ["json-n output/status/state correctness only; product and source bound to gate preparation; performance not measured"],
    execution: Alias)]
  for item in focused_core_workloads {
    entries += [CatalogCase(id: item.id, tier: Scenario,
      limit: RuntimeSeconds(300), products: [tests.Xsh], prerequisite_ids: [],
      reasons: [f"{item.name} correctness; product and source bound to gate preparation; performance not measured"],
      execution: Alias)]
  }
  entries += [CatalogCase(id: frontend_scenario_case_id, tier: Scenario,
    limit: RuntimeSeconds(300), products: [tests.Xsh, tests.Xsht], prerequisite_ids: [],
    reasons: ["annotated and reduced frontend evidence and runtime output; scaling and disposal execution unqualified"],
    execution: RunCommand(stages.command(frontend_scenario_case_id, ctx.target.triple, "python3",
      ["python3", "-B", "-m", "dev.performance.frontend_scenario", "--xsh",
        tests.product_path(ctx, profile, tests.Xsh).display(), "--xsht",
        tests.product_path(ctx, profile, tests.Xsht).display(), "--scratch", fp"{ctx.root}/.work/tmp".display()], ctx.root,
      {TMPDIR: fp"{ctx.root}/.work/tmp"})))]
  for item in core_standalone_trace_cases {
    entries += [CatalogCase(id: item.id, tier: Scenario,
      limit: RuntimeSeconds(120), products: [tests.Xsht], prerequisite_ids: [],
      reasons: [f"{item.name} exact output, syscall summary, and completed-helper trace; performance not measured"],
      execution: RunCommand(stages.command(item.id, ctx.target.triple, "python3",
        ["python3", "-B", "-m", "dev.performance.core_trace", "--xsht",
          tests.product_path(ctx, profile, tests.Xsht).display(), "--scratch", fp"{ctx.root}/.work/tmp".display(),
          "--case", item.name], ctx.root, {TMPDIR: fp"{ctx.root}/.work/tmp"})))]
  }
  for item in core_pipeline_cases {
    entries += [CatalogCase(id: item.id, tier: Scenario,
      limit: RuntimeSeconds(120), products: [tests.Xsht], prerequisite_ids: [],
      reasons: ["core pipeline output and completed-helper trace; performance not measured"],
      execution: RunCommand(stages.command(item.id, ctx.target.triple, "python3",
        ["python3", "-B", "-m", "dev.performance.core_trace", "--xsht",
          tests.product_path(ctx, profile, tests.Xsht).display(), "--scratch", fp"{ctx.root}/.work/tmp".display(),
          "--case", f"core-pipeline-{item.shape}"], ctx.root, {TMPDIR: fp"{ctx.root}/.work/tmp"})))]
  }
  entries += [CatalogCase(id: tokei_scenario_case_id, tier: Scenario,
    limit: RuntimeSeconds(360), products: [tests.Xsh], prerequisite_ids: [],
    reasons: ["exact XSH and Tokei v14.0.0 Rust file sets on pinned uutils source; Tokei provenance unqualified"],
    execution: Alias)]
  entries += manual_cases(ctx, profile)?
  entries
}

pure manual_cases(ctx: context.Context, profile: tests.TestProfile) -> Result[List[CatalogCase], Error] {
  let rust = tests.rust_plan(ctx, profile, privileged: ctx.target.os == "linux")?
  return Err(PlanError.Invalid(detail: "the Full Rust owner omitted product preparation")) when rust.is_empty() or rust[0].stage != "test-products"
  let preparation_id = "full/products"
  var cases = [CatalogCase(id: preparation_id, tier: Change, limit: ManualOwner,
    products: [], prerequisite_ids: [], reasons: ["Full product preparation"],
    execution: ManualPreparation(ManualProducts(products: tests.all_products(), command: rust[0])))]
  var root_ids = [preparation_id]
  var index = 1
  for spec in rust |> drop(1) {
    let id = f"full/rust/{index}/{spec.stage}"
    cases += [CatalogCase(id: id, tier: Change, limit: ManualOwner,
      products: tests.all_products(), prerequisite_ids: [preparation_id],
      reasons: ["Full Rust owner"], execution: ManualCommand(spec))]
    root_ids += [id]
    index += 1
  }
  cases += [CatalogCase(id: "full/native", tier: Change, limit: ManualOwner,
    products: tests.all_products(), prerequisite_ids: [preparation_id],
    reasons: ["Full native owner with retained admission policy"], execution: ManualNative)]
  root_ids += ["full/native"]
  for spec in batch_checks(ctx, profile) {
    let id = f"full/checks/{spec.stage}"
    let products = match spec.stage {
      "batch-diff" => []
      "batch-ratchets" => [tests.Xsh]
      else => [tests.Xsht]
    }
    cases += [CatalogCase(id: id, tier: Change, limit: ManualOwner,
      products: products, prerequisite_ids: [preparation_id],
      reasons: ["Full repository check owner"], execution: ManualCommand(spec))]
    root_ids += [id]
  }
  cases += [CatalogCase(id: "full/docs", tier: Change, limit: ManualOwner,
    products: [tests.Xsh, tests.Xsht], prerequisite_ids: [preparation_id],
    reasons: ["Full documentation owner"], execution: ManualDocumentation)]
  root_ids += ["full/docs"]
  cases += [CatalogCase(id: "full", tier: Change, limit: ManualOwner,
    products: [], prerequisite_ids: root_ids,
    reasons: ["explicit comprehensive verification"], execution: ManualFull)]
  cases
}

## Resolves the manual root and its complete dependency and product closure without effects.
export pure resolve_full(entries: List[CatalogCase]) -> Result[CatalogPlan, Error] {
  validate_cases(entries)?
  let roots = [entry for entry in entries |> where .id == "full"]
  return Err(PlanError.Invalid(detail: "Full requires exactly one manual root")) when roots.len() != 1 or roots[0].execution != ManualFull
  return Err(PlanError.Invalid(detail: "Full root has no declared verification dependencies")) when roots[0].prerequisite_ids.is_empty()
  let cases = resolve_case(entries, "full", [], [], [], FullAuthority)?
  var available: List[tests.Product] = []
  for entry in cases {
    for product in entry.products {
      return Err(PlanError.Invalid(detail: f"Full case {entry.id} requires an unprepared product")) when product not in available
    }
    match entry.execution {
      ManualPreparation(preparation) => {
        for product in preparation.products { available += [product] when product not in available }
      }
      RunCommand(_) | DocumentationCheck | Alias | ManualFull | ManualCommand(_) | ManualNative | ManualDocumentation => {}
    }
  }
  {tier: Change, cases: cases, products: available, unresolved: [], available_scenarios: []}
}

## Constructs Full from the same catalogue and the retained batch owners before dispatch.
export pure full_plan(ctx: context.Context, profile: tests.TestProfile) -> Result[CatalogPlan, Error] {
  resolve_full(catalogue(ctx, profile, Selection(areas: [], native_files: []), ["explicit Full request"])?)
}

pure all_areas() -> List[Area] {
  [Compile, Syntax, Grammar, Checker, Lowering, Runtime, RouteWitness, Sugar, Tooling, Api, Stdlib, Memory, Docs, Automation, Core, Showcase, Soundness]
}

## Plans known ownership, discovers owned native-file IDs, and records unknown scope.
export proc plan_paths(ctx: context.Context, profile: tests.TestProfile, paths: List[Str], explicit_ids: List[Str], tier: Tier) [fs, error] -> Result[CatalogPlan, Error] {
  var chosen: List[CatalogCase] = []
  var unresolved: List[Str] = []
  var selected_ids = explicit_ids
  var known_paths: List[Str] = []
  for changed_path in paths {
    if changed_path.starts_with("/") or ".." in changed_path.split("/") {
      unresolved += [f"path outside repository scope: {changed_path}"]
      continue
    }
    if changed_path in ["dev/performance/results.py", "dev/performance/test_results.py"] {
      selected_ids += ["verification/results"] when "verification/results" not in selected_ids
      if changed_path == "dev/performance/results.py" {
        selected_ids += ["verification/core-workloads"] when "verification/core-workloads" not in selected_ids
        selected_ids += [performance_check_case_id] when performance_check_case_id not in selected_ids
        selected_ids += [laputa_adapter_case_id] when laputa_adapter_case_id not in selected_ids
      }
      continue
    }
    if changed_path in ["dev/performance/tokei_corpus.py", "dev/performance/test_tokei_corpus.py"] {
      selected_ids += [tokei_corpus_case_id] when tokei_corpus_case_id not in selected_ids
      continue
    }
    if changed_path in ["dev/performance/tokei_scenario.py", "dev/performance/test_tokei_scenario.py"] {
      selected_ids += [tokei_adapter_case_id] when tokei_adapter_case_id not in selected_ids
      continue
    }
    if changed_path in ["dev/gate_source.py", "dev/performance/test_gate_source.py"] {
      selected_ids += ["verification/source"] when "verification/source" not in selected_ids
      continue
    }
    if changed_path in ["dev/performance/focused.py", "dev/performance/test_focused.py"] {
      selected_ids += [focused_adapter_case_id] when focused_adapter_case_id not in selected_ids
      continue
    }
    if changed_path in ["bench/consolidation/run.py", "bench/consolidation/test_harness.py", "bench/consolidation/fixtures.py", "bench/consolidation/prepare.py"] {
      selected_ids += ["verification/consolidation-harness"] when "verification/consolidation-harness" not in selected_ids
      if changed_path == "bench/consolidation/run.py" and "verification/core-workloads" not in selected_ids {
        selected_ids += ["verification/core-workloads"]
      }
      continue
    }
    if changed_path in ["dev/performance/corpus/frozen_witnesses.py", "dev/performance/corpus/test_frozen_witnesses.py"] {
      selected_ids += ["verification/frozen-witnesses"] when "verification/frozen-witnesses" not in selected_ids
      continue
    }
    if changed_path.starts_with("dev/performance/corpus/") {
      unresolved += [f"inference corpus verification requires explicit external inputs: {changed_path}"]
      continue
    }
    if changed_path == "dev/performance/owned_case.py" {
      selected_ids += ["verification/core-workloads"] when "verification/core-workloads" not in selected_ids
      selected_ids += ["verification/inference-workloads"] when "verification/inference-workloads" not in selected_ids
      continue
    }
    if changed_path in ["dev/performance/frontend_cases.py", "dev/performance/frontend_scenario.py", "dev/performance/test_frontend_cases.py"] {
      selected_ids += ["verification/inference-workloads"] when "verification/inference-workloads" not in selected_ids
      continue
    }
    if changed_path in ["dev/performance/inference_workloads.py", "dev/performance/test_inference_workloads.py", "dev/performance/inference_measurement.py", "dev/performance/test_inference_measurement.py"] {
      selected_ids += ["verification/inference-workloads"] when "verification/inference-workloads" not in selected_ids
      continue
    }
    if changed_path in ["dev/performance/core_cases.py", "dev/performance/core_pipeline.py", "dev/performance/core_trace.py", "dev/performance/test_core_cases.py"] or changed_path.starts_with("dev/performance/fixtures/core-cases/") {
      selected_ids += ["verification/core-workloads"] when "verification/core-workloads" not in selected_ids
      continue
    }
    if changed_path in ["dev/performance/check.py", "dev/performance/test_check.py"] {
      selected_ids += [performance_check_case_id] when performance_check_case_id not in selected_ids
      continue
    }
    if changed_path == "dev/performance/workloads.py" {
      selected_ids += [focused_adapter_case_id] when focused_adapter_case_id not in selected_ids
      selected_ids += [performance_check_case_id] when performance_check_case_id not in selected_ids
      selected_ids += ["verification/core-workloads"] when "verification/core-workloads" not in selected_ids
      continue
    }
    if changed_path in ["dev/performance/resources.py", "dev/performance/test_resources.py"] {
      selected_ids += [resource_diagnostics_case_id] when resource_diagnostics_case_id not in selected_ids
      continue
    }
    if changed_path in ["dev/performance/laputa_plan.py", "dev/performance/test_laputa_plan.py"] {
      selected_ids += [laputa_adapter_case_id] when laputa_adapter_case_id not in selected_ids
      continue
    }
    if changed_path in ["dev/util-linux/leases.py", "dev/util-linux/test_leases.py"] {
      selected_ids += [native_leases_case_id] when native_leases_case_id not in selected_ids
      continue
    }
    if changed_path in ["showcase/tokei.xsh", "core/lsmem.xsh"] {
      let id = if changed_path == "showcase/tokei.xsh" { "showcase/tokei/correctness" } else { "core/lsmem/correctness" }
      selected_ids += [id] when id not in selected_ids
      if changed_path == "core/lsmem.xsh" and tier == Change {
        selected_ids += ["core/lsmem/consumers"] when "core/lsmem/consumers" not in selected_ids
      }
      continue
    }
    if changed_path == "core/cat.xsh" {
      if tier == Change {
        selected_ids += [cat_full_module_case_id] when cat_full_module_case_id not in selected_ids
        selected_ids += [cat_external_consumers_case_id] when cat_external_consumers_case_id not in selected_ids
      } else {
        selected_ids += [cat_byte_stream_case_id] when cat_byte_stream_case_id not in selected_ids
      }
      continue
    }
    let matching_source_edit_cases = [case for case in source_edit_cases if case.source == changed_path]
    if tier == Edit and ! matching_source_edit_cases.is_empty() {
      for case in matching_source_edit_cases { selected_ids += [case.id] when case.id not in selected_ids }
      continue
    }
    if changed_path == "src/sema/check/local_inference.rs" {
      selected_ids += ["compiler/local-inference/correctness"] when "compiler/local-inference/correctness" not in selected_ids
      continue when tier == Edit
    }
    if changed_path == "src/sema/schemes.rs" {
      selected_ids += ["compiler/imported-inference/correctness"] when "compiler/imported-inference/correctness" not in selected_ids
      continue when tier == Edit
    }
    if changed_path == "src/sema/inference_work.rs" {
      selected_ids += ["compiler/inference-work/correctness"] when "compiler/inference-work/correctness" not in selected_ids
      continue when tier == Edit
    }
    if changed_path == "src/frontend_stats.rs" and tier == Edit {
      selected_ids += ["compiler/frontend-retention/correctness"] when "compiler/frontend-retention/correctness" not in selected_ids
      continue
    }
    if tier == Edit and (changed_path == "src/sema/constraints.rs" or (changed_path.starts_with("src/sema/constraints/") and ! changed_path.ends_with("_tests.rs"))) {
      selected_ids += [constraint_solver_case_id] when constraint_solver_case_id not in selected_ids
      continue
    }
    match select([changed_path]) {
      Ok(_) => known_paths += [changed_path]
      Err(_) => unresolved += [f"unknown verification owner: {changed_path}"]
    }
  }
  let selection = select(known_paths)?
  for entry in catalogue(ctx, profile, selection, [f"changed {changed_path}" for changed_path in known_paths])? {
    match entry.execution {
      ManualFull | ManualPreparation(_) | ManualCommand(_) | ManualNative | ManualDocumentation => continue
      RunCommand(_) | DocumentationCheck | Alias => {}
    }
    continue when entry.id in ["showcase/tokei/correctness", "core/lsmem/correctness", "core/lsmem/consumers", cat_byte_stream_case_id, cat_full_module_case_id, cat_external_consumers_case_id,
      "compiler/local-inference/correctness", "compiler/imported-inference/correctness", "compiler/inference-work/correctness", "compiler/frontend-retention/correctness", constraint_solver_case_id, "verification/results", "verification/source", "verification/consolidation-harness", "verification/core-workloads", "verification/inference-workloads", "verification/frozen-witnesses", focused_adapter_case_id, performance_check_case_id, tokei_corpus_case_id, tokei_adapter_case_id, resource_diagnostics_case_id, laputa_adapter_case_id,
      native_leases_case_id]
    continue when entry.id in [case.id for case in source_edit_cases]
    continue when entry.tier == Scenario
    chosen += [entry]
    selected_ids += [entry.id] when entry.id not in selected_ids
  }
  for area in all_areas() {
    for entry in catalogue(ctx, profile, Selection(areas: [area], native_files: []), ["explicit case selection"])? {
      chosen += [entry] when entry.id not in [known.id for known in chosen]
    }
  }
  var seen_ids: List[Str] = []
  for id in explicit_ids {
    return Err(PlanError.Invalid(detail: f"duplicate case ID {id}")) when id in seen_ids
    seen_ids += [id]
    continue when ! id.starts_with("native-") or ! id.ends_with(".xsh")
    let file = id[7..]
    return Err(PlanError.Invalid(detail: f"invalid native case path {file}")) when file.starts_with("/") or ".." in file.split("/")
    let owner = match select([file]) {
      Ok(owned) => owned
      Err(_) => { return Err(PlanError.Invalid(detail: f"native case {id} has no file owner")) }
    }
    return Err(PlanError.Invalid(detail: f"native case {id} has no file owner")) when file not in owner.native_files
    let native_path = fp"{ctx.root}/{file}"
    return Err(PlanError.Invalid(detail: f"native case {id} is missing")) when ! native_path.exists()?
    let resolved_path = native_path.resolve()?
    let owned_root = ctx.root.resolve()?.display() + "/"
    return Err(PlanError.Invalid(detail: f"native case {id} escapes the owned source root")) when ! resolved_path.display().starts_with(owned_root)
    return Err(PlanError.Invalid(detail: f"native case {id} is not a file")) when resolved_path.metadata()?.kind != "file"
    if id not in [entry.id for entry in chosen] {
      for entry in catalogue(ctx, profile, Selection(areas: [], native_files: [file]), ["explicit native file selection"])? {
        chosen += [entry] when entry.id == id
      }
    }
  }
  let resolved = resolve_selection(chosen, selected_ids, explicit_ids, tier)?
  let scenarios = if soundness_owners(selection.areas) and soundness_scenario_id not in explicit_ids { [soundness_scenario_id] } else { [] }
  {...resolved, unresolved: unresolved, available_scenarios: scenarios}
}

pure product_name(product: tests.Product) -> Str {
  match product { tests.Xsh => "xsh", tests.Xsht => "xsht", tests.Xshi => "xshi" }
}

pure case_argv(execution: CaseExecution) -> List[Str] {
  match execution { RunCommand(spec) => spec.argv, DocumentationCheck => [], Alias => [], ManualFull => [], ManualPreparation(preparation) => preparation.command.argv, ManualCommand(spec) => spec.argv, ManualNative => [], ManualDocumentation => [] }
}

pure runtime_seconds(limit: CaseLimit) -> Int? {
  match limit { RuntimeSeconds(seconds) => seconds, ManualOwner => null }
}

pure limit_owner(limit: CaseLimit) -> Str {
  match limit { RuntimeSeconds(_) => "runtime", ManualOwner => "manual_owner" }
}

pure required_runtime_seconds(limit: CaseLimit) -> Result[Int, Error] {
  match limit {
    RuntimeSeconds(seconds) => seconds
    ManualOwner => Err(PlanError.Invalid(detail: "automatic command cannot use manual lifecycle policy"))
  }
}

proc inspect_plan(plan: CatalogPlan, machine: Bool) [error, io] {
  let cases = [
    {id: entry.id, tier: tier_name(entry.tier), timeout_seconds: runtime_seconds(entry.limit), limit_owner: limit_owner(entry.limit),
      reasons: entry.reasons, prerequisite_ids: entry.prerequisite_ids,
      products: [product_name(product) for product in entry.products], argv: case_argv(entry.execution)}
    for entry in plan.cases
  ]
  if machine {
    print json.encode({schema_version: 1, kind: "verification_plan", tier: tier_name(plan.tier),
      selection_complete: plan.unresolved.is_empty(), cases: cases, unresolved: plan.unresolved,
      available_scenarios: plan.available_scenarios,
      products: [product_name(product) for product in plan.products],
      full: "not_run", execution_authority: false}, pretty: true)?
    return
  }
  print f"selected {tier_name(plan.tier)} scope: {plan.cases.len()} cases; Full not run"
  for entry in plan.cases { print f"{entry.id}: {entry.reasons.join("; ")}" }
  for gap in plan.unresolved { print f"incomplete: {gap}" }
  for id in plan.available_scenarios { print f"scenario {id} not selected; request it with --case {id}" }
}

proc changed_paths(ctx: context.Context) [fs, process, env, error] -> Result[List[Str], Error] {
  let discovered = try {
    cd (ctx.root) {
      let paths = run.text python3 -B fp"{ctx.root}/dev/gate_source.py" --changed
      paths.split("\0") |> where . != ""
    }?
  }
  match discovered {
    Ok(paths) => paths
    Err(cause) => match cause {
      ProcessError.Canceled {..} => Err(ExecutionError.Cancelled(case_id: "selection", raw_status: 130))
      else => Err(PlanError.Invalid(detail: f"changed path discovery failed: {cause.message}"))
    }
  }
}

proc bind_laputa_source(ctx: context.Context, root: Str, case_id: Str) [fs, process, env, time, error, io] -> Result[Str, Error] {
  stages.ensure_dir(fp"{ctx.root}/.work/tmp")
  let scratch = fs.tempdir_in(fp"{ctx.root}/.work/tmp")?
  let output = fp"{scratch.host_path()?}/digest"
  let command = stages.command("laputa-source", ctx.target.triple, "python3",
    ["python3", "-B", "-m", "dev.performance.laputa_plan", "--owner", ctx.root.display(), "--observe", root], ctx.root, {})
  match gate_execution.execute(command, 30000, output: output)? {
    gate_execution.Finished(status) => {
      let raw = status.shell_code()?
      return Err(ExecutionError.Cancelled(case_id: case_id, raw_status: raw)) when raw == 130
      return Err(ExecutionError.SourceUnavailable(detail: f"Laputa source inspection exited {raw}")) when raw != 0
      let digest = output.read_text()?.trim()
      return Err(ExecutionError.SourceUnavailable(detail: "invalid Laputa source digest")) when digest.byte_len() != 64
      for digit in digest.split("") {
        return Err(ExecutionError.SourceUnavailable(detail: "invalid Laputa source digest")) when digit not in "0123456789abcdef"
      }
      digest
    }
    gate_execution.TimedOut(cause) => Err(ExecutionError.TimedOut(case_id: case_id, cause: cause))
    gate_execution.Cancelled(cause) => Err(ExecutionError.Interrupted(case_id: case_id, cause: cause))
    gate_execution.CouldNotExecute(cause) => Err(ExecutionError.SourceUnavailable(detail: cause.message))
  }
}

proc gate_image(ctx: context.Context, case_id: Str) [fs, process, env, time, error, io] -> Result[Str, Error] {
  stages.ensure_dir(fp"{ctx.root}/.work/tmp")
  let scratch = fs.tempdir_in(fp"{ctx.root}/.work/tmp")?
  let output = fp"{scratch.host_path()?}/image"
  let ensured = try { docker.ensure_image(ctx)? }
  let image = match ensured {
    Ok(value) => value
    Err(ProcessError.Canceled {..}) => { return Err(ExecutionError.Cancelled(case_id: case_id, raw_status: 130)) }
    Err(stages.StageError.Failed {detail: detail, ..}) => {
      return Err(ExecutionError.Cancelled(case_id: case_id, raw_status: 130)) when detail == "exit 130" or detail == "signal 2"
      return Err(ExecutionError.Incomplete(case_id: case_id, raw_status: 0))
    }
    Err(_) => { return Err(ExecutionError.Incomplete(case_id: case_id, raw_status: 0)) }
  }
  let command = stages.command(f"{case_id}-image", ctx.target.triple, "docker",
    ["docker", "image", "inspect", "--format", "{{.Id}}", image], ctx.root, {})
  match gate_execution.execute(command, 15000, output: output)? {
    gate_execution.Finished(status) => {
      let raw = status.shell_code()?
      return Err(ExecutionError.Cancelled(case_id: case_id, raw_status: raw)) when raw == 130
      return Err(ExecutionError.Incomplete(case_id: case_id, raw_status: raw)) when raw != 0
      let identity = output.read_text()?.trim()
      return Err(ExecutionError.Incomplete(case_id: case_id, raw_status: raw)) when ! identity.starts_with("sha256:") or identity.byte_len() != 71
      return Err(ExecutionError.Incomplete(case_id: case_id, raw_status: raw)) when image.starts_with("sha256:") and identity != image
      for digit in identity[7..].split("") {
        return Err(ExecutionError.Incomplete(case_id: case_id, raw_status: raw)) when digit not in "0123456789abcdef"
      }
      identity
    }
    gate_execution.TimedOut(cause) => Err(ExecutionError.TimedOut(case_id: case_id, cause: cause))
    gate_execution.Cancelled(cause) => Err(ExecutionError.Interrupted(case_id: case_id, cause: cause))
    gate_execution.CouldNotExecute(cause) => Err(ExecutionError.ProcessFailed(case_id: case_id, cause: cause))
  }
}

proc run_laputa(ctx: context.Context, profile: tests.TestProfile, case_id: Str, root: Str, source: Str, laputa_source: Str, image: Str) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  return Err(ExecutionError.Incomplete(case_id: case_id, raw_status: 0)) when ctx.target.os != "linux"
  let scratch = fp"{ctx.root}/.work/tmp"
  let command = stages.command(case_id, ctx.target.triple, "python3", [
    "python3", "-B", "-m", "dev.performance.laputa_plan", "--case", case_id, "--owner", ctx.root.display(), "--observe", root,
    "--xsh", tests.product_path(ctx, profile, tests.Xsh).display(),
    "--xsht", tests.product_path(ctx, profile, tests.Xsht).display(), "--scratch", scratch.display(),
    @(if case_id == laputa_reuse_case_id { ["--xshi", tests.product_path(ctx, profile, tests.Xshi).display()] } else { [] }),
    "--image", image, "--platform", ctx.target.docker_platform,
    "--xsh-source", source, "--laputa-source", laputa_source,
  ], ctx.root, {})
  match gate_execution.execute(command, 330000)? {
    gate_execution.Finished(status) => {
      let raw = status.shell_code()?
      return Err(ExecutionError.Cancelled(case_id: case_id, raw_status: raw)) when raw == 130
      return Err(ExecutionError.Incomplete(case_id: case_id, raw_status: raw)) when raw == 2
      return Err(ExecutionError.Failed(case_id: case_id, raw_status: raw)) when raw != 0
      return
    }
    gate_execution.TimedOut(cause) => Err(ExecutionError.TimedOut(case_id: case_id, cause: cause))
    gate_execution.Cancelled(cause) => Err(ExecutionError.Interrupted(case_id: case_id, cause: cause))
    gate_execution.CouldNotExecute(cause) => Err(ExecutionError.SourceUnavailable(detail: cause.message))
  }
}

proc run_focused_workload(ctx: context.Context, profile: tests.TestProfile, source: Str,
    product: ProductObservation, image: Str, case_id: Str, case_name: Str) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  return Err(ExecutionError.Incomplete(case_id: case_id, raw_status: 0)) when ctx.target.os != "linux"
  let xsh = tests.product_path(ctx, profile, tests.Xsh)
  let commit = cd (ctx.root) { run.text python3 -B fp"{ctx.root}/dev/gate_source.py" --commit }?.trim()
  let tree = cd (ctx.root) { run.text python3 -B fp"{ctx.root}/dev/gate_source.py" --tree }?.trim()
  stages.ensure_dir(fp"{ctx.root}/.work/tmp")
  let scratch = fs.tempdir_in(fp"{ctx.root}/.work/tmp")?
  let scratch_path = scratch.host_path()?
  let output = fp"{scratch_path}/report.json"
  let config_path = fp"{scratch_path}/config.json"
  let fixtures = fp"{scratch_path}/fixtures"
  fixtures.mkdir()?
  let fixture_args = if case_name == "json-n" {
    ["python3", "-B", fp"{ctx.root}/bench/consolidation/fixtures.py".display(), fp"{fixtures}/2048".display()]
  } else {
    ["python3", "-B", "-m", "dev.performance.core_cases", "--fixtures", fixtures.display(), "--case", case_name]
  }
  let fixture_command = stages.command(f"{case_id}-fixtures", ctx.target.triple, "python3", fixture_args, ctx.root, {})
  match gate_execution.execute(fixture_command, 30000)? {
    gate_execution.Finished(status) => {
      let raw = status.shell_code()?
      return Err(ExecutionError.Cancelled(case_id: case_id, raw_status: raw)) when raw == 130
      return Err(ExecutionError.Incomplete(case_id: case_id, raw_status: raw)) when raw == 2
      return Err(ExecutionError.Failed(case_id: case_id, raw_status: raw)) when raw != 0
    }
    gate_execution.TimedOut(cause) => { return Err(ExecutionError.TimedOut(case_id: case_id, cause: cause)) }
    gate_execution.Cancelled(cause) => { return Err(ExecutionError.Interrupted(case_id: case_id, cause: cause)) }
    gate_execution.CouldNotExecute(cause) => { return Err(ExecutionError.ProcessFailed(case_id: case_id, cause: cause)) }
  }
  let config = {
    schema: 1,
    image_id: image,
    source_root: ctx.root.display(),
    commit: commit,
    tree: tree,
    xsh: xsh.display(),
    scratch: scratch_path.display(),
    measurement: {fixtures: fixtures.display(), output: output.display()},
    gate_binding: {source_digest: source, product_digest: product.digest, product_mode: product.mode},
  }
  config_path.write(json.encode(config)?)?
  let command = stages.command(case_id, ctx.target.triple, "python3", [
    "python3", "-B", "-m", "dev.performance.focused", "--config", config_path.display(), "--case", case_name,
  ], ctx.root, {})
  match gate_execution.execute(command, 330000)? {
    gate_execution.Finished(status) => {
      let raw = status.shell_code()?
      return Err(ExecutionError.Cancelled(case_id: case_id, raw_status: raw)) when raw == 130
      return Err(ExecutionError.Incomplete(case_id: case_id, raw_status: raw)) when raw == 2
      return Err(ExecutionError.Failed(case_id: case_id, raw_status: raw)) when raw != 0
      let report = try {
        let decoded = json.decode(output.read_text()?)?
        let record = decoded.require(Record)?
        let binding = record.get("gate_binding")?.require(Record)?
        let product_record = record.get("product")?.require(Record)?
        record.get("case")?.require(Str)? == case_name
          and record.get("correctness_complete")?.require(Bool)?
          and record.get("cleanup_complete")?.require(Bool)?
          and ! record.get("full_cohort_run")?.require(Bool)?
          and ! record.get("performance_complete")?.require(Bool)?
          and ! record.get("timing_complete")?.require(Bool)?
          and record.get("performance_status")?.require(Str)? == "not-measured"
          and record.get("product_source_binding")?.require(Str)? == "gate-prepared-product-bound-to-source-snapshot"
          and record.get("image_id")?.require(Str)? == image
          and binding.get("source_digest")?.require(Str)? == source
          and binding.get("product_digest")?.require(Str)? == product.digest
          and binding.get("product_mode")?.require(Int)? == product.mode
          and product_record.get("sha256")?.require(Str)? == product.digest
          and product_record.get("mode")?.require(Int)? == product.mode
      }
      match report {
        Ok(true) => {}
        Ok(false) => { return Err(ExecutionError.ResultsUnavailable(case_id: case_id,
          detail: f"focused report did not qualify {case_name} correctness")) }
        Err(cause) => { return Err(ExecutionError.ResultsUnavailable(case_id: case_id,
          detail: cause.message)) }
      }
      require_source(ctx, source, case_id, 30000000000)?
      let current = bind_product(ctx, xsh.display(), "xsh", case_id, 30000000000)?
      return Err(ExecutionError.ProductChanged(case_id: case_id, product: "xsh")) when current != product
      let resource_report = try {
        let decoded = json.decode(output.read_text()?)?
        let record = decoded.require(Record)?
        json.encode(record.get("resource_diagnostics")?.require(Record)?)?
      }
      let resource_text = match resource_report {
        Ok(value) => value
        Err(_) => "unavailable"
      }
      print f"Selected scope: {case_name} correctness passed"
      print "Product source binding: gate-prepared product bound to selected source"
      print f"Resource diagnostics (diagnostic only): {resource_text}"
      print "Performance: not measured"
      print "Full cohort: not run"
      return
    }
    gate_execution.TimedOut(cause) => { return Err(ExecutionError.TimedOut(case_id: case_id, cause: cause)) }
    gate_execution.Cancelled(cause) => { return Err(ExecutionError.Interrupted(case_id: case_id, cause: cause)) }
    gate_execution.CouldNotExecute(cause) => { return Err(ExecutionError.ProcessFailed(case_id: case_id, cause: cause)) }
  }
}

proc run_tokei_rust(ctx: context.Context, profile: tests.TestProfile, source: Str,
    xsh_product: ProductObservation, tokei_path: Str, corpus_root: Str, image: Str) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  let xsh = tests.product_path(ctx, profile, tests.Xsh)
  let commit = cd (ctx.root) { run.text python3 -B fp"{ctx.root}/dev/gate_source.py" --commit }?.trim()
  let tree = cd (ctx.root) { run.text python3 -B fp"{ctx.root}/dev/gate_source.py" --tree }?.trim()
  stages.ensure_dir(fp"{ctx.root}/.work/tmp")
  let scratch = fs.tempdir_in(fp"{ctx.root}/.work/tmp")?
  let scratch_path = scratch.host_path()?
  let output = fp"{scratch_path}/report.json"
  let config_path = fp"{scratch_path}/config.json"
  let tokei_product = bind_product(ctx, tokei_path, "tokei-14.0.0", tokei_scenario_case_id, 30000000000)?
  let config = {
    schema: 1,
    image_id: image,
    source_root: ctx.root.display(),
    corpus_root: corpus_root,
    commit: commit,
    tree: tree,
    xsh: xsh.display(),
    tokei: tokei_path,
    scratch: scratch_path.display(),
    output: output.display(),
    gate_binding: {
      source_digest: source,
      xsh_digest: xsh_product.digest,
      xsh_mode: xsh_product.mode,
      tokei_digest: tokei_product.digest,
      tokei_mode: tokei_product.mode,
    },
  }
  config_path.write(json.encode(config)?)?
  let command = stages.command(tokei_scenario_case_id, ctx.target.triple, "python3", [
    "python3", "-B", "-m", "dev.performance.tokei_scenario", "--config", config_path.display(),
  ], ctx.root, {TMPDIR: fp"{ctx.root}/.work/tmp"})
  match gate_execution.execute(command, 330000)? {
    gate_execution.Finished(status) => {
      let raw = status.shell_code()?
      return Err(ExecutionError.Cancelled(case_id: tokei_scenario_case_id, raw_status: raw)) when raw == 130
      return Err(ExecutionError.Incomplete(case_id: tokei_scenario_case_id, raw_status: raw)) when raw == 2
      return Err(ExecutionError.Failed(case_id: tokei_scenario_case_id, raw_status: raw)) when raw != 0
      var count_summary = ""
      let qualified = try {
        let record = json.decode(output.read_text()?)?.require(Record)?
        let binding = record.get("products")?.require(Record)?
        let xsh_record = binding.get("xsh")?.require(Record)?
        let tokei_record = binding.get("tokei")?.require(Record)?
        let corpus = record.get("corpus")?.require(Record)?
        let reference = record.get("tokei_reference")?.require(Record)?
        let reconciliation = record.get("reconciliation")?.require(Record)?
        let counts = record.get("rust_line_counts")?.require(Record)?
        let xsh_counts = counts.get("xsh")?.require(Record)?
        let tokei_counts = counts.get("tokei")?.require(Record)?
        let differences = counts.get("xsh_minus_tokei")?.require(Record)?
        var counts_valid = true
        for key in ["code", "comments", "blanks"] {
          let xsh_count = xsh_counts.get(key)?.require(Int)?
          let tokei_count = tokei_counts.get(key)?.require(Int)?
          let difference = differences.get(key)?.require(Int)?
          counts_valid = counts_valid and xsh_count >= 0 and tokei_count >= 0 and difference == xsh_count - tokei_count
        }
        count_summary = json.encode(counts)?
        record.get("case")?.require(Str)? == "tokei-rust-correctness"
          and counts_valid
          and record.get("correctness_complete")?.require(Bool)?
          and record.get("cleanup_complete")?.require(Bool)?
          and ! record.get("full_cohort_run")?.require(Bool)?
          and ! record.get("performance_complete")?.require(Bool)?
          and ! record.get("timing_complete")?.require(Bool)?
          and record.get("performance_status")?.require(Str)? == "not-measured"
          and record.get("product_bindings")?.require(Record)?.get("xsh")?.require(Str)? == "gate-prepared-and-bound-to-selected-source-snapshot"
          and record.get("product_bindings")?.require(Record)?.get("tokei")?.require(Str)? == "version-checked-binary-digest-and-mode-snapshot"
          and record.get("image_id")?.require(Str)? == image
          and record.get("source_digest")?.require(Str)? == source
          and xsh_record.get("sha256")?.require(Str)? == xsh_product.digest
          and xsh_record.get("mode")?.require(Int)? == xsh_product.mode
          and tokei_record.get("sha256")?.require(Str)? == tokei_product.digest
          and tokei_record.get("mode")?.require(Int)? == tokei_product.mode
          and corpus.get("revision")?.require(Str)? == "e7c9f3194280835c4487c2945c68d5f01ccacc8d"
          and corpus.get("files")?.require(Int)? >= 500
          and reference.get("version")?.require(Str)? == "tokei 14.0.0 compiled with serialization support: json"
          and reference.get("expected_source_commit")?.require(Str)? == "8cdd6fa3a54f8cd69442d2f00effb29aa3110353"
          and reference.get("source_provenance")?.require(Str)? == "unqualified"
          and reference.get("binary_sha256")?.require(Str)? == tokei_product.digest
          and record.get("p0_qualification")?.require(Str)? == "unqualified"
          and reconciliation.get("exact")?.require(Bool)?
      }
      match qualified {
        Ok(true) => {}
        Ok(false) => { return Err(ExecutionError.ResultsUnavailable(case_id: tokei_scenario_case_id,
          detail: "Tokei report did not qualify exact pinned-corpus correctness")) }
        Err(cause) => { return Err(ExecutionError.ResultsUnavailable(case_id: tokei_scenario_case_id,
          detail: cause.message)) }
      }
      require_source(ctx, source, tokei_scenario_case_id, 30000000000)?
      let current_xsh = bind_product(ctx, xsh.display(), "xsh", tokei_scenario_case_id, 30000000000)?
      return Err(ExecutionError.ProductChanged(case_id: tokei_scenario_case_id, product: "xsh")) when current_xsh != xsh_product
      let current_tokei = bind_product(ctx, tokei_path, "tokei-14.0.0", tokei_scenario_case_id, 30000000000)?
      return Err(ExecutionError.ProductChanged(case_id: tokei_scenario_case_id, product: "tokei-14.0.0")) when current_tokei != tokei_product
      print "Selected scope: Tokei Rust corpus file sets passed"
      print f"Rust line counts: {count_summary}"
      print "Product binding: XSH gate-prepared; Tokei digest stable during this run"
      print "Tokei source provenance: unqualified; P0: not qualified"
      print "Performance: not measured"
      print "Full cohort: not run"
      return
    }
    gate_execution.TimedOut(cause) => Err(ExecutionError.TimedOut(case_id: tokei_scenario_case_id, cause: cause))
    gate_execution.Cancelled(cause) => Err(ExecutionError.Interrupted(case_id: tokei_scenario_case_id, cause: cause))
    gate_execution.CouldNotExecute(cause) => Err(ExecutionError.ProcessFailed(case_id: tokei_scenario_case_id, cause: cause))
  }
}

proc focused(ctx: context.Context, tier: Tier, supplied: List[Str], ids: List[Str], output: OutputMode, laputa_root: Str?) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  let source = if output == Execute { bind_source(ctx, "selection", automatic_budget_ns(tier))? } else { null }
  let selected_laputa_ids = [id for id in ids if id in laputa_case_ids]
  return Err(PlanError.Invalid(detail: "Laputa root requires an explicit case")) when laputa_root != null and selected_laputa_ids.is_empty()
  let laputa_id = if selected_laputa_ids.is_empty() { laputa_case_id } else { selected_laputa_ids[0] }
  let laputa_source = if output == Execute and laputa_root != null { bind_laputa_source(ctx, laputa_root ?? "", laputa_id)? } else { null }
  let paths = if supplied.is_empty() and ids.is_empty() { changed_paths(ctx)? } else { supplied }
  let profile = tests.execution_profile()?
  let plan = plan_paths(ctx, profile, paths, ids, tier)?
  inspect_plan(plan, output == InspectJson)
  if ! plan.unresolved.is_empty() { exit 2 }
  return when output != Execute
  return Err(ExecutionError.Incomplete(case_id: "selection", raw_status: 0)) when plan.cases.is_empty()
  if let root = laputa_root {
    return Err(ExecutionError.Incomplete(case_id: laputa_id, raw_status: 0)) when ctx.target.os != "linux"
    let image = gate_image(ctx, laputa_id)?
    let regular_ids = [id for id in ids if id not in laputa_case_ids]
    let preparation = ["--source-digest", source ?? "", "--laputa-preparation", laputa_id, "--tier", tier_name(tier)]
    env XSH_TEST_IMAGE=$image XSH_TEST_IMAGE_BUILD="0" {
      container_status(docker.run_internal_status({...ctx, profile: tests.profile_name(profile)}, "gate-lane", false, preparation)?)?
    }
    var regular_incomplete = false
    if ! paths.is_empty() or ! regular_ids.is_empty() {
      var forwarded = ["--source-digest", source ?? "", "--tier", tier_name(tier), @paths]
      for id in regular_ids { forwarded += ["--case", id] }
      let regular = try {
        env XSH_TEST_IMAGE=$image XSH_TEST_IMAGE_BUILD="0" {
          container_status(docker.run_internal_status({...ctx, profile: tests.profile_name(profile)}, "gate-lane", false, forwarded)?)?
        }
      }
      match regular {
        Ok(_) => {}
        Err(ExecutionError.Incomplete {..}) => regular_incomplete = true
        Err(cause) => { return Err(cause) }
      }
    }
    let external = try { run_laputa(ctx, profile, laputa_id, root, source ?? "", laputa_source ?? "", image)? }
    match external {
      Ok(_) => {}
      Err(cause) => { return Err(cause) }
    }
    return Err(ExecutionError.Incomplete(case_id: "container", raw_status: 2)) when regular_incomplete
    return
  }
  let selected_core_workloads = [item for item in focused_core_workloads if item.id in ids]
  if focused_json_case_id in ids or ! selected_core_workloads.is_empty() {
    let focused_case_id = if focused_json_case_id in ids { focused_json_case_id } else { selected_core_workloads[0].id }
    let focused_case_name = if focused_case_id == focused_json_case_id { "json-n" } else { selected_core_workloads[0].name }
    return Err(PlanError.Invalid(detail: f"{focused_case_id} must run alone")) when ! paths.is_empty() or ids.len() != 1
    return Err(ExecutionError.Incomplete(case_id: focused_case_id, raw_status: 0)) when ctx.target.os != "linux"
    let image = gate_image(ctx, focused_case_id)?
    let preparation = ["--source-digest", source ?? "", "--tier", tier_name(tier), "--case", focused_case_id]
    env XSH_TEST_IMAGE=$image XSH_TEST_IMAGE_BUILD="0" {
      container_status(docker.run_internal_status({...ctx, profile: tests.profile_name(profile)}, "gate-lane", false, preparation)?)?
    }
    require_source(ctx, source ?? "", focused_case_id, 30000000000)?
    let xsh = tests.product_path(ctx, profile, tests.Xsh)
    let product = bind_product(ctx, xsh.display(), "xsh", focused_case_id, 30000000000)?
    return run_focused_workload(ctx, profile, source ?? "", product, image, focused_case_id, focused_case_name)
  }
  if tokei_scenario_case_id in ids {
    return Err(PlanError.Invalid(detail: f"{tokei_scenario_case_id} must run alone")) when ! paths.is_empty() or ids.len() != 1
    return Err(ExecutionError.Incomplete(case_id: tokei_scenario_case_id, raw_status: 0)) when ctx.target.os != "linux"
    let tokei_path = env.get_or("XSH_TOKEI_REFERENCE", "")?.trim()
    let corpus_root = env.get_or("XSH_TOKEI_CORPUS_ROOT", "")?.trim()
    return Err(ExecutionError.Incomplete(case_id: tokei_scenario_case_id, raw_status: 0)) when tokei_path == "" or corpus_root == ""
    var image = ""
    env XSH_TEST_IMAGE_BUILD="0" {
      image = gate_image(ctx, tokei_scenario_case_id)?
    }
    let preparation = ["--source-digest", source ?? "", "--tier", tier_name(tier), "--case", tokei_scenario_case_id]
    env XSH_TEST_IMAGE=$image XSH_TEST_IMAGE_BUILD="0" {
      container_status(docker.run_internal_status({...ctx, profile: tests.profile_name(profile)}, "gate-lane", false, preparation)?)?
    }
    require_source(ctx, source ?? "", tokei_scenario_case_id, 30000000000)?
    let xsh = tests.product_path(ctx, profile, tests.Xsh)
    let xsh_product = bind_product(ctx, xsh.display(), "xsh", tokei_scenario_case_id, 30000000000)?
    return run_tokei_rust(ctx, profile, source ?? "", xsh_product, tokei_path, corpus_root, image)
  }
  if ctx.target.os == "linux" {
    var forwarded = ["--source-digest", source ?? "", "--tier", tier_name(tier), @paths]
    for id in ids { forwarded += ["--case", id] }
    return container_status(docker.run_internal_status({...ctx, profile: tests.profile_name(profile)}, "gate-lane", false, forwarded)?)
  }
  execute_plan(ctx, profile, plan, paths, source)
}

pure automatic_budget_ns(tier: Tier) -> Int {
  match tier { Edit => 30000000000, Change => 300000000000, Scenario => 0 }
}

pure case_budget_ns(tier: Tier, seconds: Int, remaining_ns: Int, binding_ns: Int) -> Int {
  let case_ns = seconds * 1000000000 - binding_ns
  if tier == Scenario or case_ns < remaining_ns { case_ns } else { remaining_ns }
}

proc leaf_adapter(ctx: context.Context, profile: tests.TestProfile, action: Str, id: Str, paths: List[Str]) [process, error] -> Result[contract.CommandSpec, Error] {
  let xsh = applet.current_exe()?.display()
  stages.command(f"case-{action}-{id}", ctx.target.triple, xsh,
    [xsh, "dev/gate_case.xsh", "--", action, id, @paths], ctx.root,
    {TARGET: ctx.target.triple, DIST_PROFILE: tests.profile_name(profile), CARGO_TARGET_DIR: ctx.target_dir.display()})
}

## Revalidates the full selected closure, then prepares or executes only one leaf.
export proc run_case(ctx: context.Context, action: CaseAction, id: Str, paths: List[Str]) [fs, process, env, error, io] -> Result[Unit, Error] {
  let profile = tests.execution_profile()?
  let plan = plan_paths(ctx, profile, paths, [id], Change)?
  return Err(PlanError.Invalid(detail: "unresolved leaf scope")) when ! plan.unresolved.is_empty()
  let entries = [entry for entry in plan.cases |> where .id == id]
  return Err(PlanError.Invalid(detail: f"case {id} is not uniquely selected")) when entries.len() != 1
  let entry = entries[0]
  match entry.execution {
    RunCommand(spec) => {
      match action {
        Prepare => {
          guard spec.executable == "cargo" and spec.argv[1] == "test" else {
            return Err(PlanError.Invalid(detail: "only Cargo test harnesses require leaf preparation"))
          }
          let argv = harness_build_argv(spec.argv)
          return cargo_steps.run_build({...spec, stage: f"{spec.stage}-compile", argv: [@argv, "--message-format=json-render-diagnostics"]})
        }
        ExecuteLeaf => {
          return Err(PlanError.Invalid(detail: "prepared Cargo harnesses execute directly, never through Cargo")) when spec.executable == "cargo"
          return tests.run_commands([spec])
        }
      }
    }
    DocumentationCheck => {
      return Err(PlanError.Invalid(detail: "documentation is not a Cargo harness")) when action == Prepare
      return docs_gate(ctx, profile)
    }
    Alias => return
    ManualFull | ManualPreparation(_) | ManualCommand(_) | ManualNative | ManualDocumentation => { return Err(PlanError.Invalid(detail: "manual leaf cannot execute in a focused request")) }
  }
}

pure harness_build_argv(argv: List[Str]) -> List[Str] {
  var build = ["cargo", "test"]
  var index = 2
  while index < argv.len() {
    let word = argv[index]
    break when word == "--"
    if word in ["--target", "--profile", "-p", "--package", "--test", "--bin", "--features"] {
      build += [word, argv[index + 1]]
      index += 2
      continue
    }
    build += [word] when word.starts_with("-")
    index += 1
  }
  cargo_steps.test_build_argv(build)
}

pure harness_arguments(argv: List[Str]) -> List[Str] {
  var arguments: List[Str] = []
  var index = 2
  while index < argv.len() {
    let word = argv[index]
    if word == "--" {
      let literal_arguments = argv |> drop(index + 1)
      return [@arguments, @literal_arguments]
    }
    if word in ["--target", "--profile", "-p", "--package", "--test", "--bin", "--features"] {
      index += 2
      continue
    }
    arguments += [word] when ! word.starts_with("-")
    index += 1
  }
  arguments
}

pure harness_selects(id: Str, argv: List[Str]) -> Bool {
  var filter: Str? = null
  var skips: List[Str] = []
  var exact = false
  var index = 1
  while index < argv.len() {
    let word = argv[index]
    if word == "--skip" {
      return false when index + 1 >= argv.len()
      skips += [argv[index + 1]]
      index += 2
      continue
    }
    if word == "--exact" {
      exact = true
    } else if word.starts_with("-") or filter != null {
      return false
    } else {
      filter = word
    }
    index += 1
  }
  if let required = filter {
    return false when if exact { id != required } else { required not in id }
  }
  for skipped in skips {
    return false when if exact { id == skipped } else { skipped in id }
  }
  true
}

proc prepared_harness(spec: contract.CommandSpec, output: Path, target_dir: Path) [fs, error] -> Result[contract.CommandSpec, Error] {
  var executable: Str? = null
  var finished = false
  for line in output.read_lines()? {
    continue when line.starts_with("[")
    let record = json.decode(line)?.require(Record)?
    let reason = record.get("reason")?.require(Str)?
    if reason == "build-finished" {
      return Err(PlanError.Invalid(detail: "duplicate Cargo build completion")) when finished
      finished = record.get("success")?.require(Bool)?
      return Err(PlanError.Invalid(detail: "Cargo preparation did not succeed")) when ! finished
    }
    if reason == "compiler-artifact" {
      let profile = record.get("profile")?.require(Record)?
      if profile.get("test")?.require(Bool)? {
        let candidate = record.get("executable")?.require(Str?)?
        if candidate != null {
          return Err(PlanError.Invalid(detail: "multiple Cargo test harnesses selected")) when executable != null
          executable = candidate
        }
      }
    }
  }
  return Err(PlanError.Invalid(detail: "Cargo preparation omitted its test executable or completion")) when executable == null or ! finished
  let executable_path = fp"{executable ?? ""}".resolve()?
  let owned = target_dir.resolve()?.display() + "/"
  return Err(PlanError.Invalid(detail: "Cargo test executable escapes owned products")) when ! executable_path.display().starts_with(owned)
  {...spec, executable: executable_path.display(), argv: [executable_path.display(), @harness_arguments(spec.argv)]}
}

type ProductBinding = {product: tests.Product, digest: Str, mode: Int}
type InputObservation = {output: Str, diagnostics: Str, status: Int}
enum InputObserver { ProductObserver, SourceObserver }

proc observe_input(ctx: context.Context, observer: InputObserver, arguments: List[Str], case_id: Str, available_ns: Int) [fs, process, env, time, error, io] -> Result[InputObservation, Error] {
  return Err(ExecutionError.BudgetExpired(case_id: case_id)) when available_ns < 1000000
  stages.ensure_dir(fp"{ctx.root}/.work/tmp")
  let scratch = fs.tempdir_in(fp"{ctx.root}/.work/tmp")?
  let output = fp"{scratch.host_path()?}/output"
  let diagnostics = fp"{scratch.host_path()?}/stderr"
  let executable = match observer { ProductObserver => applet.current_exe()?.display(), SourceObserver => "python3" }
  let argv = match observer {
    ProductObserver => [executable, fp"{ctx.root}/dev/gate_product.xsh".display(), "--", @arguments]
    SourceObserver => [executable, "-B", fp"{ctx.root}/dev/gate_source.py".display()]
  }
  let command = process.command_argv(executable, argv,
    cwd: ctx.root, stdout: output, stderr: diagnostics, timeout: time.millis(available_ns / 1000000))
  let status = match process.run(command) {
    Ok(value) => value
    Err(cause) => match cause {
      ProcessError.Timeout {..} => { return Err(ExecutionError.TimedOut(case_id: case_id, cause: cause)) }
      ProcessError.Canceled {..} => { return Err(ExecutionError.Interrupted(case_id: case_id, cause: cause)) }
      else => { return Err(ExecutionError.ProcessFailed(case_id: case_id, cause: cause)) }
    }
  }
  if ! status.exited() and ! status.signaled() {
    let message = if status.segments.is_empty() { "" } else { status.segments[0].error_message.require(Str?)? ?? "" }
    let detail = if message == "" { f"could not start {executable}" } else { message }
    match observer {
      SourceObserver => { return Err(ExecutionError.SourceUnavailable(detail: detail)) }
      ProductObserver => { return Err(ExecutionError.ProductUnavailable(product: arguments[0], detail: detail)) }
    }
  }
  let raw = status.shell_code()?
  return Err(ExecutionError.Cancelled(case_id: case_id, raw_status: raw)) when raw == 130
  {output: output.read_text()?, diagnostics: diagnostics.read_text()?.trim(), status: raw}
}

proc bind_product(ctx: context.Context, product_path: Str, product: Str, case_id: Str, available_ns: Int) [fs, process, env, time, error, io] -> Result[ProductObservation, Error] {
  let observed = observe_input(ctx, ProductObserver, [product_path], case_id, available_ns)?
  if observed.status != 0 {
    let detail = observed.diagnostics
    return Err(ExecutionError.ProductUnavailable(product: product,
      detail: if detail == "" { f"inspection exited {observed.status}" } else { detail }))
  }
  let decoded = match json.decode(observed.output) {
    Ok(value) => value
    Err(cause) => { return Err(ExecutionError.ProductUnavailable(product: product, detail: cause.message)) }
  }
  let observation = match decoded.require(ProductObservation) {
    Ok(value) => value
    Err(cause) => { return Err(ExecutionError.ProductUnavailable(product: product, detail: cause.message)) }
  }
  return Err(ExecutionError.ProductUnavailable(product: product, detail: "invalid digest")) when observation.digest.byte_len() != 64
  for digit in observation.digest.split("") {
    return Err(ExecutionError.ProductUnavailable(product: product, detail: "invalid digest")) when digit not in "0123456789abcdef"
  }
  observation
}

proc bind_products(ctx: context.Context, profile: tests.TestProfile, products: List[tests.Product], case_id: Str, available_ns: Int) [fs, process, env, time, error, io] -> Result[List[ProductBinding], Error] {
  var bindings: List[ProductBinding] = []
  let started = time.monotonic()?
  for product in products {
    let remaining = available_ns - (time.monotonic()? - started)
    return Err(ExecutionError.BudgetExpired(case_id: case_id)) when remaining < 1000000
    let product_path = tests.product_path(ctx, profile, product)
    let observation = bind_product(ctx, product_path.display(), product_name(product), case_id, remaining)?
    bindings += [{product: product, digest: observation.digest, mode: observation.mode}]
  }
  bindings
}

proc require_products(ctx: context.Context, profile: tests.TestProfile, bindings: List[ProductBinding], case_id: Str, available_ns: Int) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  let current = bind_products(ctx, profile, [binding.product for binding in bindings], case_id, available_ns)?
  var index = 0
  while index < bindings.len() {
    return Err(ExecutionError.ProductChanged(case_id: case_id, product: product_name(bindings[index].product))) when bindings[index] != current[index]
    index += 1
  }
}

proc bind_source(ctx: context.Context, case_id: Str, available_ns: Int) [fs, process, env, time, error, io] -> Result[Str, Error] {
  let observed = observe_input(ctx, SourceObserver, [], case_id, available_ns)?
  if observed.status != 0 {
    return Err(ExecutionError.SourceUnavailable(detail:
      if observed.diagnostics == "" { f"inspection exited {observed.status}" } else { observed.diagnostics }))
  }
  let digest = observed.output.trim()
  return Err(ExecutionError.SourceUnavailable(detail: "invalid digest")) when digest.byte_len() != 64
  for digit in digest.split("") {
    return Err(ExecutionError.SourceUnavailable(detail: "invalid digest")) when digit not in "0123456789abcdef"
  }
  digest
}

proc require_source(ctx: context.Context, source: Str, case_id: Str, available_ns: Int) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  return Err(ExecutionError.SourceChanged(case_id: case_id)) when bind_source(ctx, case_id, available_ns)? != source
}

proc execute_plan(ctx: context.Context, profile: tests.TestProfile, plan: CatalogPlan, paths: List[Str], expected_source: Str?) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  var first_limit_ns = automatic_budget_ns(plan.tier)
  for entry in plan.cases {
    match entry.execution {
      RunCommand(_) | DocumentationCheck => {
        first_limit_ns = required_runtime_seconds(entry.limit)? * 1000000000
        break
      }
      Alias | ManualFull | ManualPreparation(_) | ManualCommand(_) | ManualNative | ManualDocumentation => {}
    }
  }
  let initial_ns = if plan.tier == Scenario or first_limit_ns < automatic_budget_ns(plan.tier) { first_limit_ns } else { automatic_budget_ns(plan.tier) }
  let source_started = time.monotonic()?
  let source = bind_source(ctx, "source", initial_ns)?
  if let expected = expected_source {
    return Err(ExecutionError.SourceChanged(case_id: "selection")) when source != expected
  }
  let source_ns = time.monotonic()? - source_started
  print "compilation: preparing selected executable products outside the runtime budget"
  tests.run_commands(tests.product_builds(ctx, profile, plan.products)?)
  let binding_started = time.monotonic()?
  let products = bind_products(ctx, profile, plan.products, "products", initial_ns - source_ns)?
  require_source(ctx, source, "source", initial_ns - source_ns - (time.monotonic()? - binding_started))?
  var binding_ns = source_ns + time.monotonic()? - binding_started
  var remaining_ns = automatic_budget_ns(plan.tier) - (if plan.tier == Scenario { 0 } else { binding_ns })
  var harnesses: Map[Str] = {}
  var harness_bindings: Map[ProductObservation] = {}
  for entry in plan.cases {
    var commands: List[contract.CommandSpec] = []
    var compiled = false
    var harness: Str? = null
    match entry.execution {
      RunCommand(spec) => {
        if spec.executable == "cargo" and spec.argv[1] == "test" {
          let key = harness_build_argv(spec.argv).join("\0")
          if let Ok(executable) = harnesses.get(key) {
            commands = [{...spec, executable: executable, argv: [executable, @harness_arguments(spec.argv)]}]
            harness = executable
          } else {
            stages.ensure_dir(fp"{ctx.root}/.work/tmp")
            let scratch = fs.tempdir_in(fp"{ctx.root}/.work/tmp")?
            let output = fp"{scratch.host_path()?}/cargo.jsonl"
            print f"compilation: preparing {entry.id} outside the runtime budget"
            let prepare = leaf_adapter(ctx, profile, "prepare", entry.id, paths)?
            let status = process.run(process.command_argv(prepare.executable, prepare.argv,
              cwd: prepare.cwd, env: prepare.environment, stdout: output))?
            let raw = status.shell_code()?
            return Err(ExecutionError.Cancelled(case_id: entry.id, raw_status: raw)) when raw == 130
            if ! status.ok {
              print f"cargo preparation failed: {entry.id} raw_status={raw}"
              let reader = stages.command("cargo-diagnostics", ctx.target.triple, "python3",
                ["python3", "-B", fp"{ctx.root}/dev/performance/results.py".display(), "--runner", "cargo", "--events", output.display(), "--cargo-diagnostics"], ctx.root, {})
              match gate_execution.execute(reader, 1000) {
                Ok(gate_execution.Finished(reader_status)) => {
                  print "Cargo diagnostics could not be read" when ! reader_status.ok
                }
                Ok(gate_execution.Cancelled(_)) => return Err(ExecutionError.Cancelled(case_id: entry.id, raw_status: 130))
                Ok(gate_execution.TimedOut(_)) | Ok(gate_execution.CouldNotExecute(_)) | Err(_) => print "Cargo diagnostics could not be read"
              }
              return Err(ExecutionError.Failed(case_id: entry.id, raw_status: raw))
            }
            let prepared = prepared_harness(spec, output, ctx.target_dir)?
            harnesses[key] = prepared.executable
            commands = [prepared]
            harness = prepared.executable
          }
        } else if spec.executable == "cargo" and spec.argv[1] in ["build", "check"] {
          print f"compilation: {entry.id} outside the runtime budget"
          tests.run_commands([spec])
          compiled = true
        } else {
          commands = [spec]
        }
      }
      DocumentationCheck => commands = [leaf_adapter(ctx, profile, "execute", entry.id, paths)?]
      Alias => {}
      ManualFull | ManualPreparation(_) | ManualCommand(_) | ManualNative | ManualDocumentation => { return Err(PlanError.Invalid(detail: "manual request in focused execution")) }
    }
    if compiled {
      let started = time.monotonic()?
      let seconds = required_runtime_seconds(entry.limit)?
      let available_ns = case_budget_ns(entry.tier, seconds, remaining_ns, binding_ns)
      binding_ns = 0
      require_products(ctx, profile, products, entry.id, available_ns)?
      require_source(ctx, source, entry.id, available_ns - (time.monotonic()? - started))?
      let elapsed = time.monotonic()? - started
      return Err(ExecutionError.BudgetExpired(case_id: entry.id)) when elapsed > available_ns
      remaining_ns -= elapsed when entry.tier != Scenario
    }
    for command in commands {
      let case_started = time.monotonic()?
      let seconds = required_runtime_seconds(entry.limit)?
      let available_ns = case_budget_ns(entry.tier, seconds, remaining_ns, binding_ns)
      binding_ns = 0
      return Err(ExecutionError.BudgetExpired(case_id: entry.id)) when available_ns < 1000000
      require_products(ctx, profile, products, entry.id, available_ns)?
      require_source(ctx, source, entry.id, available_ns - (time.monotonic()? - case_started))?
      if let executable = harness {
        let observed = bind_product(ctx, executable, executable, entry.id, available_ns - (time.monotonic()? - case_started))?
        if let Ok(previous) = harness_bindings.get(executable) {
          return Err(ExecutionError.ProductChanged(case_id: entry.id, product: executable)) when previous != observed
        } else {
          harness_bindings[executable] = observed
        }
      }
      var selected_command = command
      var selected_ids: List[Str] = []
      var result_runner: Str? = null
      stages.ensure_dir(fp"{ctx.root}/.work/tmp")
      let scratch = fs.tempdir_in(fp"{ctx.root}/.work/tmp")?
      let events = fp"{scratch.host_path()?}/native-events.jsonl"
      let cases_file = fp"{scratch.host_path()?}/selected-cases.json"
      let xsht = tests.product_path(ctx, profile, tests.Xsht).display()
      if command.executable == xsht and command.argv.len() == 3 and command.argv[1] == "test" {
        let output = fp"{scratch.host_path()?}/native-ids"
        let discovery_ns = available_ns - (time.monotonic()? - case_started)
        return Err(ExecutionError.BudgetExpired(case_id: entry.id)) when discovery_ns < 1000000
        let discovery = process.command_argv(command.executable,
          [command.executable, "test", "--list-json", command.argv[2]], cwd: command.cwd,
          env: command.environment, stdout: output, timeout: time.millis(gate_execution.limit_ms(seconds, discovery_ns)?))
        let status = match process.run(discovery) {
          Ok(status) => status
          Err(cause) => match cause {
            ProcessError.Timeout {..} => { return Err(ExecutionError.TimedOut(case_id: entry.id, cause: cause)) }
            ProcessError.Canceled {..} => { return Err(ExecutionError.Interrupted(case_id: entry.id, cause: cause)) }
            else => { return Err(ExecutionError.ProcessFailed(case_id: entry.id, cause: cause)) }
          }
        }
        let raw = status.shell_code()?
        return Err(ExecutionError.Cancelled(case_id: entry.id, raw_status: raw)) when raw == 130
        return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: raw)) when raw == 2
        return Err(ExecutionError.Failed(case_id: entry.id, raw_status: raw)) when ! status.ok
        require_products(ctx, profile, products, entry.id, available_ns - (time.monotonic()? - case_started))?
        require_source(ctx, source, entry.id, available_ns - (time.monotonic()? - case_started))?
        let text = match output.read_text() {
          Ok(value) => value
          Err(cause) => { return Err(ExecutionError.DiscoveryUnavailable(case_id: entry.id, detail: cause.message)) }
        }
        let decoded = match json.decode(text) {
          Ok(value) => value
          Err(cause) => { return Err(ExecutionError.DiscoveryUnavailable(case_id: entry.id, detail: cause.message)) }
        }
        let ids = match decoded.require(List[Str]) {
          Ok(value) => value
          Err(cause) => { return Err(ExecutionError.DiscoveryUnavailable(case_id: entry.id, detail: cause.message)) }
        }
        return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: 0)) when ids.is_empty()
        var seen: List[Str] = []
        for id in ids {
          return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: 0)) when "::" not in id or command.argv[2] not in id or id in seen
          seen += [id]
        }
        selected_ids = seen
        result_runner = "xsht"
      } else if command.executable == xsht and command.argv.len() == 4 and command.argv[1] == "test" and command.argv[2] == "--exact" {
        selected_ids = [command.argv[3]]
        result_runner = "xsht"
      } else if command.executable == xsht and command.argv.len() > 2 and command.argv[1] == "test" {
        var index = 2
        while index < command.argv.len() {
          return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: 0)) when command.argv[index] != "--exact-id" or index + 1 >= command.argv.len()
          selected_ids += [command.argv[index + 1]]
          index += 2
        }
        result_runner = "xsht"
      }
      if let executable = harness {
        let discovery_output = fp"{scratch.host_path()?}/cargo-discovery.jsonl"
        let discovery_ns = available_ns - (time.monotonic()? - case_started)
        return Err(ExecutionError.BudgetExpired(case_id: entry.id)) when discovery_ns < 1000000
        let discovery = {...command, stage: f"{entry.id}-discovery",
          argv: [@command.argv, "--list", "--format", "json", "-Z", "unstable-options"]}
        match gate_execution.execute(discovery, gate_execution.limit_ms(seconds, discovery_ns)?, output: discovery_output)? {
          gate_execution.Finished(status) => {
            let raw = status.shell_code()?
            return Err(ExecutionError.Cancelled(case_id: entry.id, raw_status: raw)) when raw == 130
            return Err(ExecutionError.Failed(case_id: entry.id, raw_status: raw)) when ! status.ok
          }
          gate_execution.TimedOut(cause) => { return Err(ExecutionError.TimedOut(case_id: entry.id, cause: cause)) }
          gate_execution.Cancelled(cause) => { return Err(ExecutionError.Interrupted(case_id: entry.id, cause: cause)) }
          gate_execution.CouldNotExecute(cause) => { return Err(ExecutionError.ProcessFailed(case_id: entry.id, cause: cause)) }
        }
        let current = bind_product(ctx, executable, executable, entry.id, available_ns - (time.monotonic()? - case_started))?
        return Err(ExecutionError.ProductChanged(case_id: entry.id, product: executable)) when current != harness_bindings.get(executable)?
        require_products(ctx, profile, products, entry.id, available_ns - (time.monotonic()? - case_started))?
        require_source(ctx, source, entry.id, available_ns - (time.monotonic()? - case_started))?
        let names_output = fp"{scratch.host_path()?}/cargo-ids"
        let discovery_reader = {...command, stage: f"{entry.id}-discovery-results", executable: "python3",
          argv: ["python3", "-B", fp"{ctx.root}/dev/performance/results.py".display(), "--runner", "cargo",
            "--events", discovery_output.display(), "--discover-cargo"]}
        let remaining = available_ns - (time.monotonic()? - case_started)
        return Err(ExecutionError.BudgetExpired(case_id: entry.id)) when remaining < 1000000
        match gate_execution.execute(discovery_reader, gate_execution.limit_ms(seconds, remaining)?, output: names_output)? {
          gate_execution.Finished(status) => {
            return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: 0)) when ! status.exited() and ! status.signaled()
            let raw = status.shell_code()?
            return Err(ExecutionError.Cancelled(case_id: entry.id, raw_status: raw)) when raw == 130
            return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: raw)) when ! status.ok
          }
          gate_execution.TimedOut(cause) => { return Err(ExecutionError.TimedOut(case_id: entry.id, cause: cause)) }
          gate_execution.Cancelled(cause) => { return Err(ExecutionError.Interrupted(case_id: entry.id, cause: cause)) }
          gate_execution.CouldNotExecute(cause) => { return Err(ExecutionError.ResultsUnavailable(case_id: entry.id, detail: cause.message)) }
        }
        selected_ids = match names_output.read_lines() {
          Ok(lines) => lines
          Err(cause) => { return Err(ExecutionError.DiscoveryUnavailable(case_id: entry.id, detail: cause.message)) }
        }
        return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: 0)) when selected_ids.is_empty()
        for id in selected_ids {
          return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: 0)) when ! harness_selects(id, command.argv)
        }
        result_runner = "cargo"
        selected_command = {...command, argv: [@command.argv, "--format", "json", "-Z", "unstable-options"]}
      }
      if ! selected_ids.is_empty() {
        match cases_file.write(json.encode(selected_ids)?) {
          Ok(_) => {}
          Err(cause) => { return Err(ExecutionError.DiscoveryUnavailable(case_id: entry.id, detail: cause.message)) }
        }
        if result_runner == "xsht" {
          selected_command = {...command, argv: [command.executable, "test", "--exact-ids-file", cases_file.display(), "--events-jsonl", events.display()]}
        }
      }
      let execution_ns = available_ns - (time.monotonic()? - case_started)
      return Err(ExecutionError.BudgetExpired(case_id: entry.id)) when execution_ns < 1000000
      let limit = gate_execution.limit_ms(seconds, execution_ns)?
      let outcome = gate_execution.execute(selected_command, limit, output: if result_runner == "cargo" { events } else { null })?
      match outcome {
        gate_execution.Finished(status) => {
          let raw = status.shell_code()?
          return Err(ExecutionError.Cancelled(case_id: entry.id, raw_status: raw)) when raw == 130
          return Err(ExecutionError.Cancelled(case_id: entry.id, raw_status: raw)) when (entry.id in [item.id for item in core_pipeline_cases] or entry.id in [item.id for item in core_standalone_trace_cases] or entry.id == frontend_scenario_case_id) and raw == 143
          return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: raw)) when (entry.id in [item.id for item in core_pipeline_cases] or entry.id in [item.id for item in core_standalone_trace_cases] or entry.id == frontend_scenario_case_id) and raw == 2
          let failed_child = raw != 0 and (raw != 2 or result_runner == "cargo")
          if ! selected_ids.is_empty() {
            let arguments = ["python3", "-B", fp"{ctx.root}/dev/performance/results.py".display(),
              "--runner", result_runner ?? "", "--events", events.display(), "--child-status", f"{raw}",
              "--no-artifact-link", "--cases-json", cases_file.display()]
            let remaining = available_ns - (time.monotonic()? - case_started)
            if remaining < 1000000 {
              return Err(ExecutionError.Failed(case_id: entry.id, raw_status: raw)) when failed_child
              return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: raw)) when raw == 2
              return Err(ExecutionError.BudgetExpired(case_id: entry.id))
            }
            let reconcile = {...command, stage: f"{entry.id}-results", executable: "python3", argv: arguments}
            match gate_execution.execute(reconcile, gate_execution.limit_ms(seconds, remaining)?)? {
              gate_execution.Finished(result) => {
                if ! result.exited() and ! result.signaled() {
                  return Err(ExecutionError.Failed(case_id: entry.id, raw_status: raw)) when failed_child
                  return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: raw)) when raw == 2
                  let message = if result.segments.is_empty() { "could not start result reconciliation" } else { result.segments[0].error_message.require(Str?)? ?? "could not start result reconciliation" }
                  return Err(ExecutionError.ResultsUnavailable(case_id: entry.id, detail: message))
                }
                let result_raw = result.shell_code()?
                return Err(ExecutionError.Cancelled(case_id: entry.id, raw_status: result_raw)) when result_raw == 130
                return Err(ExecutionError.Failed(case_id: entry.id, raw_status: raw)) when failed_child
                return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: raw)) when raw == 2
                return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: result_raw)) when ! result.ok
              }
              gate_execution.TimedOut(cause) => {
                return Err(ExecutionError.Failed(case_id: entry.id, raw_status: raw)) when failed_child
                return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: raw)) when raw == 2
                return Err(ExecutionError.TimedOut(case_id: entry.id, cause: cause))
              }
              gate_execution.Cancelled(cause) => { return Err(ExecutionError.Interrupted(case_id: entry.id, cause: cause)) }
              gate_execution.CouldNotExecute(cause) => {
                return Err(ExecutionError.Failed(case_id: entry.id, raw_status: raw)) when failed_child
                return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: raw)) when raw == 2
                return Err(ExecutionError.ResultsUnavailable(case_id: entry.id, detail: cause.message))
              }
            }
          }
          return Err(ExecutionError.Incomplete(case_id: entry.id, raw_status: raw)) when raw == 2 and command.executable == xsht and command.argv[1] == "test"
          return Err(ExecutionError.Failed(case_id: entry.id, raw_status: raw)) when ! status.ok
          if let executable = harness {
            let current = bind_product(ctx, executable, executable, entry.id, available_ns - (time.monotonic()? - case_started))?
            return Err(ExecutionError.ProductChanged(case_id: entry.id, product: executable)) when current != harness_bindings.get(executable)?
          }
          require_products(ctx, profile, products, entry.id, available_ns - (time.monotonic()? - case_started))?
          require_source(ctx, source, entry.id, available_ns - (time.monotonic()? - case_started))?
        }
        gate_execution.TimedOut(cause) => { return Err(ExecutionError.TimedOut(case_id: entry.id, cause: cause)) }
        gate_execution.Cancelled(cause) => { return Err(ExecutionError.Interrupted(case_id: entry.id, cause: cause)) }
        gate_execution.CouldNotExecute(cause) => { return Err(ExecutionError.ProcessFailed(case_id: entry.id, cause: cause)) }
      }
      let case_elapsed = time.monotonic()? - case_started
      return Err(ExecutionError.BudgetExpired(case_id: entry.id)) when case_elapsed > available_ns
      remaining_ns -= case_elapsed when entry.tier != Scenario
    }
  }
}

## Parses authority before entering Docker or launching any verification command.
export proc execute(ctx: context.Context, requested: List[Str]) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  present(execute_request(ctx, requested))
}

proc present(result: Result[Unit, Error]) [error, io] -> Result[Unit, Error] {
  match result {
    Ok(_) => return
    Err(PlanError.Invalid {detail: detail}) => { print f"invalid gate plan: {detail}"; exit 2 }
    Err(ExecutionError.Failed {case_id: id, raw_status: raw}) => { print f"failed: {id} raw_status={raw}"; exit 1 }
    Err(ExecutionError.TimedOut {case_id: id, cause: cause}) => { print f"incomplete: {id} timed out: {cause.message}"; exit 2 }
    Err(ExecutionError.BudgetExpired {case_id: id}) => { print f"incomplete: runtime budget exhausted before {id}"; exit 2 }
    Err(ExecutionError.Cancelled {case_id: id, raw_status: raw}) => { print f"cancelled: {id} raw_status={raw}"; exit 130 }
    Err(ExecutionError.Interrupted {case_id: id, cause: cause}) => { print f"cancelled: {id} cause={cause.message}"; exit 130 }
    Err(ExecutionError.Incomplete {case_id: id, raw_status: raw}) => { print f"incomplete: {id} raw_status={raw}"; exit 2 }
    Err(ExecutionError.DiscoveryUnavailable {case_id: id, detail: detail}) => { print f"incomplete: {id} discovery unavailable: {detail}"; exit 2 }
    Err(ExecutionError.ResultsUnavailable {case_id: id, detail: detail}) => { print f"incomplete: {id} results unavailable: {detail}"; exit 2 }
    Err(ExecutionError.ProductChanged {case_id: id, product: product}) => { print f"incomplete: {id} product changed: {product}"; exit 2 }
    Err(ExecutionError.ProductUnavailable {product: product, detail: detail}) => { print f"incomplete: product {product} unavailable: {detail}"; exit 2 }
    Err(ExecutionError.SourceChanged {case_id: id}) => { print f"incomplete: {id} source changed"; exit 2 }
    Err(ExecutionError.SourceUnavailable {detail: detail}) => { print f"incomplete: source unavailable: {detail}"; exit 2 }
    Err(docker.ImageError.InvalidIdentity {detail: detail}) => { print f"incomplete: image unavailable: {detail}"; exit 2 }
    Err(docker.ImageError.Unavailable {raw_status: raw}) => { print f"incomplete: image unavailable: raw_status={raw}"; exit 2 }
    Err(docker.ImageError.Cancelled {raw_status: raw}) => { print f"cancelled: image inspection raw_status={raw}"; exit 130 }
    Err(ExecutionError.ProcessFailed {case_id: id, cause: cause}) => { print f"failed: {id} execution_error={cause.message}"; exit 1 }
    Err(stages.StageError.Failed {stage: stage, target: target, detail: detail}) => { print f"failed: {stage} target={target} execution_error={detail}"; exit 1 }
    Err(stages.StageError.MissingTool {tool: tool}) => { print f"failed: missing tool {tool}"; exit 1 }
    Err(cause) => { print f"failed: {cause.message}"; exit 1 }
  }
}

proc container_status(status: Status) [error] -> Result[Unit, Error] {
  let raw = status.shell_code()?
  return Err(ExecutionError.Cancelled(case_id: "container", raw_status: raw)) when raw == 130
  return Err(ExecutionError.Incomplete(case_id: "container", raw_status: raw)) when raw == 2
  return Err(ExecutionError.Failed(case_id: "container", raw_status: raw)) when ! status.ok
}

proc execute_request(ctx: context.Context, requested: List[Str]) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  let request = parse_request(requested)?
  match request {
    Full => {
      let profile = tests.execution_profile()?
      let _ = full_plan(ctx, profile)?
      if ctx.target.os == "linux" {
        return container_status(docker.run_internal_status({...ctx, profile: tests.profile_name(profile)}, "gate-full", true, [])?)
      }
      return batch(ctx, request)
    }
    Focused(tier, paths, ids, output, laputa_root) => return focused(ctx, tier, paths, ids, output, laputa_root)
  }
}

## Internal focused execution validates the same request and closure before effects.
export proc lane(ctx: context.Context, requested: List[Str]) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  present(lane_request(ctx, requested))
}

proc lane_request(ctx: context.Context, requested: List[Str]) [fs, process, env, time, error, io] -> Result[Unit, Error] {
  let expected_source = if ! requested.is_empty() and requested[0] == "--source-digest" {
    return Err(PlanError.Invalid(detail: "missing source digest")) when requested.len() < 2
    requested[1]
  } else { null }
  let incoming = if expected_source != null { requested[2..] } else { requested }
  let laputa_preparation = ! incoming.is_empty() and incoming[0] == "--laputa-preparation"
  return Err(PlanError.Invalid(detail: "Laputa preparation requires a case ID")) when laputa_preparation and incoming.len() < 2
  let arguments = if laputa_preparation { incoming[2..] } else { incoming }
  match parse_request(arguments)? {
    Full => { return Err(PlanError.Invalid(detail: "focused dispatch cannot request Full")) }
    Focused(tier, supplied, ids, output, laputa_root) => {
      return Err(PlanError.Invalid(detail: "Laputa case must run through host dispatch")) when laputa_root != null
      let paths = if supplied.is_empty() and ids.is_empty() and ! laputa_preparation { changed_paths(ctx)? } else { supplied }
      let profile = tests.execution_profile()?
      if laputa_preparation {
        let prep_case_id = incoming[1]
        return Err(PlanError.Invalid(detail: "Laputa preparation requires a known case ID")) when prep_case_id not in laputa_case_ids
        return Err(PlanError.Invalid(detail: "Laputa preparation accepts only its case ID and tier")) when ! paths.is_empty() or ! ids.is_empty()
      }
      let selected_ids = if laputa_preparation { [incoming[1]] } else { ids }
      let plan = plan_paths(ctx, profile, paths, selected_ids, tier)?
      inspect_plan(plan, output == InspectJson)
      if ! plan.unresolved.is_empty() { exit 2 }
      return when output != Execute
      return Err(ExecutionError.Incomplete(case_id: "selection", raw_status: 0)) when plan.cases.is_empty()
      return execute_plan(ctx, profile, plan, paths, expected_source)
    }
  }
}
