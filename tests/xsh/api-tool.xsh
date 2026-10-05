# `xsht api` answers selectors from the registry: text by default, one JSON
# object per selector with `--format jsonl`. The fragments below are the parts
# of the generated reference a reader relies on, so a reworded contract fails
# here.

type Guide = {kind: Str}

type Summary = {modules: List[Any], method_receivers: List[Any], records: List[Any], language_groups: List[Any]}

# The stdout of a successful `xsht api ARGS...` run that printed no diagnostics.
proc api(arguments: List[Str]) [process, error] -> Result[Str] {
  let output = run.capture --text "xsht" api @arguments ?
  assert output.status.exited_with(0), f"{arguments.join(" ")}: {output.stdout}{output.stderr}"
  assert output.stderr == "", output.stderr
  output.stdout
}

proc assert_contains(text: Str, fragments: List[Str]) [error] {
  for fragment in fragments {
    assert fragment in text, f"missing {fragment}: {text}"
  }
}

# Every fragment occurs, each one after the previous.
proc assert_ordered(text: Str, fragments: List[Str]) [error] {
  var start = 0
  for fragment in fragments {
    let offset = text.find(fragment, start)
    assert offset != null, f"missing {fragment} after byte {start}: {text}"
    start = offset + fragment.byte_len()
  }
}

pure occurrences(text: Str, needle: Str) -> Int {
  text.split(needle).len() - 1
}

test test_api_fs_root_lists_native_receiver_operations {
  api(["method:FsRoot.read_bytes", "method:FsRoot.mkdir", "method:FsRoot.close"])? |> assert_contains(
    _,
    ["method.FsRoot.read_bytes", "method.FsRoot.mkdir", "method.FsRoot.close", "root.write", "payload"],
  )
}

test test_api_builtin_templates_render_receiver_argument_and_result_relationships {
  api(
    [
      "method:List.get",
      "method:Map.set",
      "method:Map.values",
      "method:List.join",
      "method:List.collect",
      "method:Str.lines",
      "method:Bytes.lines",
    ],
  )? |> assert_contains(
    _,
    [
      "List[T].get(index: Int) -> Result[T, Error]",
      "Map[K, V].set(key: K, value: V) -> Map[K, V]",
      "Map[K, V].values() -> List[V]",
      "List[Str].join(separator: Str = default) -> Str",
      "List[T].collect() -> List[T]",
      "Str.lines() -> List[Str]",
      "Bytes.lines() -> List[Bytes]",
    ],
  )
}

test test_api_boolean_guards_explains_exits_refinements_and_no_error_input {
  api(["language:core.boolean-guards"])? |> assert_contains(
    _,
    [
      "Bool",
      "Status",
      "evaluates once",
      "every reachable failure path",
      "no input parameter or new Result boundary",
      "mutation invalidation",
      "Float",
      "NaN",
      "guard let",
    ],
  )
}

test test_api_mixed_batch_preserves_query_order {
  let stdout = api(["api:json.read", "method:Path.read_text", "record:FsEntry", "language:run.status"])?
  assert_contains(
    stdout,
    ["api: module.json.read", "api: method.Path.read_text", "api: record.FsEntry", "api: language.run.status"],
  )
  assert_ordered(
    stdout,
    [
      "query: api:json.read",
      "query: method:Path.read_text",
      "query: record:FsEntry",
      "query: language:run.status",
    ],
  )
}

test test_api_without_query_is_a_standalone_onboarding_guide {
  api([])? |> assert_contains(
    _,
    [
      "XSH API getting started",
      "proc main(...argv: List[Str])",
      "xsht check hello.xsh",
      "xsht fmt hello.xsh",
      "xsht lint hello.xsh",
      "xsht api module:fs",
      "xsht api api:fs.read_text",
    ],
  )
}

test test_api_without_query_jsonl_is_a_valid_guide_object {
  let stdout = api(["--format", "jsonl"])?
  assert stdout.lines().len() == 1, stdout
  let guide = json.decode(stdout.trim())?.require(Guide)?
  assert guide.kind == "guide", stdout
}

test test_api_onboarding_script_passes_xsht_check { |ctx|
  let stdout = api([])?
  let start = stdout.find("proc main(...argv: List[Str])")
  assert start != null, stdout
  let end = stdout.find("\n\nBasic development loop:", start)
  assert end != null, stdout
  let script = test.temp_file(
    ctx,
    name: "hello.xsh",
    contents: bytes.from_text(stdout.byte_slice(start, end - start)),
  )?
  let checked = run.capture --text "xsht" check $script ?
  assert checked.status.exited_with(0), checked.stderr
}

test test_api_module_query_lists_the_module_and_its_members {
  api(["module:fs"])? |> assert_contains(
    _,
    ["status: matches", "api: module.fs\n", "api: module.fs.read_text\n", "purpose:"],
  )
}

test test_api_error_fail_is_exactly_registered_and_searchable {
  api(["api:error.fail"])? |> assert_contains(
    _,
    ["status: exact", "api: module.error.fail", "error.fail", "validation", "error effect"],
  )
  api(["search:fail"])? |> assert_contains(_, ["api: module.error.fail", "status: matches"])
}

test test_api_exact_item_explains_effects_and_contract {
  api(["api:fs.read_text"])? |> assert_contains(_, ["contract:", "effects: fs", "signature: fs.read_text"])
}

test test_api_filesystem_walk_contract_documents_hidden_default {
  let stdout = api(["api:fs.files", "api:fs.walk"])?
  assert occurrences(stdout, "hidden: false") == 2, stdout
  assert occurrences(stdout, "dot-prefixed files and directories") == 2, stdout
  assert_contains(stdout, ["query: api:fs.files", "query: api:fs.walk"])
}

test test_api_language_group_includes_the_language_contract {
  api(["language:effect"])? |> assert_contains(_, ["status: matches", "api: language.effect.fs", "contract:"])
}

test test_api_print_builtin_is_indexed_with_signature_effects_and_example {
  api(["language:core.print"])? |> assert_contains(
    _,
    [
      "status: exact",
      "api: language.core.print",
      "effects: none",
      "signature: print [--flush] ARG...",
      "separated by a single space",
      "expression string literals",
      "example:",
      "print \"hello\" $name",
    ],
  )
}

test test_api_print_builtin_is_discoverable_by_search {
  api(["search:print"])? |> assert_contains(
    _,
    ["status: matches", "api: language.core.print", "Prints values to standard output."],
  )
}

test test_api_print_builtin_is_found_by_output_and_builtin_terms {
  for term in ["search:builtin", "search:output"] {
    api([term])? |> assert_contains(_, ["status: matches", "api: language.core.print"])
  }
}

test test_api_summary_jsonl_is_one_structured_response {
  let output = run.capture --text "xsht" api summary --format jsonl ?
  assert output.status.exited_with(0), output.stderr
  let stdout = output.stdout
  assert stdout.lines().len() == 1, stdout
  assert_contains(
    stdout,
    [
      "\"kind\":\"summary\"",
      "\"total_queryable_items\":",
      "\"documented_items\":",
      "\"modules\":[",
      "\"method_receivers\":[",
      "\"records\":[",
      "\"language_groups\":[",
    ],
  )
  let summary = json.decode(stdout.trim())?.require(Summary)?
  assert ! summary.modules.is_empty(), stdout
}

test test_api_summary_rejects_selectors {
  let output = run.capture --text "xsht" api summary "api:json.read" ?
  assert output.status.exited_with(2), output.stdout
  assert "cannot be combined with selectors" in output.stderr, output.stderr
}

test test_api_jsonl_has_one_response_per_selector {
  let stdout = api(["--format", "jsonl", "api:json.read", "language:effect.process"])?
  let lines = stdout.lines()
  assert lines.len() == 2, stdout
  for line in lines {
    let _ = json.decode(line)?
  }

  assert "\"schema_version\":1" in lines[0], lines[0]
  assert "\"query\":\"api:json.read\"" in lines[0], lines[0]
  assert "\"query\":\"language:effect.process\"" in lines[1], lines[1]
}

test test_api_strict_renders_all_queries_before_failing {
  let output = run.capture --text "xsht" api --strict "api:json.read" "api:json.missing" ?
  assert output.status.exited_with(1), output.stderr
  assert_contains(
    output.stdout,
    ["query: api:json.read\nstatus: exact", "query: api:json.missing\nstatus: missing"],
  )
  assert output.stderr == "", output.stderr
}

test test_api_combines_query_file_and_argv_queries { |ctx|
  let queries = test.temp_file(
    ctx,
    name: "queries.txt",
    contents: bytes.from_text("api:json.read\nlanguage:effect.fs\n"),
  )?
  let output = run.capture --text "xsht" api --query-file $queries "record:FsEntry" ?
  assert output.status.exited_with(0), output.stderr
  assert_ordered(output.stdout, ["query: record:FsEntry", "query: api:json.read", "query: language:effect.fs"])
}

test test_api_stdin_queries_join_argv_batch_in_request_order {
  let output = run.capture --text "xsht" api "record:FsEntry" --stdin < b"api:json.read\nlanguage:effect.fs\n" ?
  assert output.status.exited_with(0), output.stderr
  assert_ordered(output.stdout, ["query: record:FsEntry", "query: api:json.read", "query: language:effect.fs"])
}

test test_api_search_is_local_and_deterministic {
  let output = run.capture --text "xsht" api "search:rooted" ?
  assert output.status.exited_with(0), output.stderr
  assert_contains(
    output.stdout,
    ["status: matches", "api: module.archive.tar_create", "api: module.patch.apply"],
  )
}

test test_api_defaulted_parameters_explain_positional_only_calls {
  api(["api:fs.files"])? |> assert_contains(
    _,
    [
      "Function arguments are positional-only; parameters marked `= default` may be omitted, but cannot be supplied as `name = value`.",
    ],
  )
}

test test_api_stream_sort_by_shows_options_before_block {
  let stdout = api(["language:stream.sort-by"])?
  assert_contains(
    stdout,
    [
      "status: exact",
      "signature: sort-by(desc: Bool = false, block) -> Stream[T]",
      "|> sort-by(desc: true) { |e| e.size }",
    ],
  )
  assert "sort-by(--desc, { |e| e.size })" not in stdout, stdout
}

test test_api_stream_stage_group_by_shows_signature_and_record_shape {
  api(["language:stream.group-by"])? |> assert_contains(
    _,
    ["status: exact", "api: language.stream.group-by", "signature: ", "Stream[{key, items: List[T]}]", "items"],
  )
}

test test_api_stream_stages_carry_a_signature_in_jsonl {
  let stages = ["map", "where", "sort-by", "fold", "each", "collect", "unique-by"]
  let stdout = api(["--format", "jsonl", @[f"language:stream.{stage}" for stage in stages]])?
  let lines = stdout.lines()
  assert lines.len() == 7, stdout
  for stage in stages {
    let query = f"\"query\":\"language:stream.{stage}\""
    let answers = [line for line in lines if query in line]
    assert answers.len() == 1, f"{stage}: {stdout}"
    assert "\"signatures\":[]" not in answers[0], answers[0]
    assert "\"signatures\":[" in answers[0], answers[0]
  }
}

test test_api_module_member_text_shows_the_signature {
  api(["module:tui.left_pad"])? |> assert_contains(
    _,
    ["status: exact", "api: module.tui.left_pad", "signature: tui.left_pad(text: Str, width: Int) -> Str"],
  )
}

test test_api_module_member_jsonl_matches_text_signature {
  api(["--format", "jsonl", "module:tui.left_pad"])? |> assert_contains(
    _,
    ["\"signatures\":[", "tui.left_pad(text: Str, width: Int) -> Str"],
  )
}

test test_api_module_overview_stays_concise {
  let stdout = api(["module:env"])?
  assert_contains(stdout, ["status: matches", "api: module.env\n"])
  # An overview lists members by purpose, not by dumping every signature.
  assert "signature: env." not in stdout, stdout
}

test test_api_method_receiver_query_lists_every_method_of_a_type {
  let stdout = api(["method:Str"])?
  # A bare receiver query lists the receiver's methods by id without error.
  assert_contains(stdout, ["status: matches", "api: method.Str.lower\n", "purpose:"])
  # Like a module overview, a receiver overview stays concise: no full signature dump.
  assert "signature: Str.lower" not in stdout, stdout
}

test test_api_map_receiver_query_discloses_its_constructor {
  api(["method:Map"])? |> assert_contains(
    _,
    ["api: method.Map.constructor\n", "map.empty()", "`{}` is an empty Record"],
  )
}

test test_api_map_summary_discloses_its_constructor {
  let stdout = api(["summary"])?
  let receiver = stdout.find("── Map (")
  assert receiver != null, stdout
  assert stdout.find("module.map.empty", receiver) != null, stdout
}

test test_api_method_receiver_query_keeps_exact_member_lookup {
  api(["method:Str.lower"])? |> assert_contains(
    _,
    ["status: exact", "api: method.Str.lower\n", "contract:", "signature: Str.lower"],
  )
}

# The Path constructor receiver shares the "Path" receiver name, so a bare
# receiver query lists its methods alongside the path methods.
test test_api_method_receiver_works_for_path_constructor_receiver {
  api(["method:Path"])? |> assert_contains(_, ["status: matches", "api: method.Path.ext\n"])
}

# The mutable-binding token must be discoverable from the reference, and `let`
# must be described as immutable, so a first-time agent writing a mutable
# counter does not have to guess `let mut` / `mut` / `let var`.
test test_api_core_bindings_names_var_and_let_immutability {
  api(["language:core.bindings"])? |> assert_contains(
    _,
    ["status: exact", "api: language.core.bindings\n", "var", "let", "immutable", "let mut"],
  )
}

# The public API surface remains unchanged when implementations move into
# embedded scripts.
#
# The recorded API summary covers every standard module, function, overload
# count, method receiver, method, and record. Additions, removals, renames,
# shape changes, and overload-count changes all fail this check. The behavior
# of those entries is covered separately; this test guards the public surface.
test test_api_surface_matches_the_recorded_reference {
  let expected = p"tests/fixtures/modules/standard-api-surface.jsonl".read_text()?
  let output = run.capture --text "xsht" api summary --format jsonl ?
  assert output.status.exited_with(0), output.stderr
  assert output.stdout.trim() == expected.trim(), "the public API surface changed; regenerate the fixture only when the change is intended"
}

test test_api_core_procs_demonstrates_lexical_named_argument_puns {
  api(["language:core.procs"])? |> assert_contains(_, ["ordinary lexical value", "greet(name:)"])
}

test test_api_core_fallback_explains_error_parameter_and_lexical_targets {
  api(["language:core.fallback"])? |> assert_contains(
    _,
    [
      "{ |failure| statements; tail_value }",
      "exactly one immutable parameter containing the exact error",
      "Handler tails match the success type",
      "lexical return, loop, propagation, and cleanup targets",
      "right-associative",
      "lint.error-fallback-block",
    ],
  )
}

test test_api_slicing_documents_bounds_units_and_retained_count_method {
  api(["language:core.slicing", "method:Bytes.slice"])? |> assert_contains(
    _,
    [
      "api: language.core.slicing",
      "negative bounds count from the end",
      "Str counts Unicode scalars; Bytes counts bytes",
      "data[..2]",
      "data[2..]",
      "api: method.Bytes.slice",
      "Uses offset/count with nonnegative bounds",
    ],
  )
}

test test_api_comprehensions_reference_exposes_order_cleanup_and_example {
  api(["language:core.comprehensions"])? |> assert_contains(
    _,
    ["later duplicate keys win", "Streams are pulled lazily", "for package in packages"],
  )
}

test test_api_list_splicing_documents_nesting_order_and_explicit_domains {
  api(["language:core.list-splicing"])? |> assert_contains(
    _,
    [
      "ordinary List-valued element remains nested",
      "left to right",
      "Results require explicit handling",
      "@flags",
      "collect",
    ],
  )
}

test test_api_regex_literals_exposes_preparation_raw_syntax_and_dynamic_compile {
  api(["language:core.regex-literals", "module:regex.compile"])? |> assert_contains(
    _,
    [
      "rx\"",
      "no escapes or interpolation",
      "unreachable code",
      "repeated calls share",
      "regex.compile(runtime_pattern)",
      "structured regex-compile errors",
    ],
  )
}

test test_api_streams_explains_yield_delegation_and_cleanup_order {
  api(["language:core.streams"])? |> assert_contains(
    _,
    [
      "yield @source",
      "Results require explicit handling",
      "closes children before parent cleanup",
      "yield @rows()",
    ],
  )
}

test test_api_core_records_demonstrates_schema_owned_defaults_and_constructor_puns {
  api(["language:core.records"])? |> assert_contains(
    _,
    [
      "bounded literal constants",
      "enabled: Bool = true",
      "Config(name:)",
      "Observation[T]",
      "Observation(value: 7)",
      "Observation(value: \"demo\", samples: [\"demo\"])",
      "let absent: CountObservation = Observation(value: null)",
      "disjoint existing field paths",
      "{...settings, build.jobs: 4}",
    ],
  )
}

test test_api_private_pure_returns_explain_definition_inference_and_explicit_boundaries {
  api(["language:core.pure-functions"])? |> assert_contains(
    _,
    [
      "Defaulted parameters infer concrete checked types",
      "null and unconstrained empty collections",
      "only for omitted slots",
      "build_defaults.jobs + 1",
      "jobs = initial_jobs()",
      "Private helpers",
      "recursive",
      "pure add(left: Int, right: Int) {",
    ],
  )
}

test test_api_field_labels_distinguishes_wire_names_from_lexical_bindings {
  api(["language:core.field-labels"])? |> assert_contains(
    _,
    [
      "Keyword spellings",
      "reserved binding and import names",
      "cannot be shorthand or puns",
      "dynamic values retain require validation",
      "Entry(type:",
      "type: entry_kind",
    ],
  )
}

test test_api_map_literals_exposes_classification_order_and_boundaries {
  api(["language:core.map-literals"])? |> assert_contains(
    _,
    [
      "Map[K, V]",
      "Int, UInt, Bool, Bytes, Path, or Duration",
      "constant labels remain Str keys",
      "spread-only Maps require context",
      "canonical key order",
      "[name]",
      "before its value",
    ],
  )
}

test test_api_causes_exposes_typed_translation_and_source_example {
  api(["language:core.causes"])? |> assert_contains(
    _,
    [
      "cause: failure",
      "nominal family",
      "immutable",
      "once",
      "BuildCauseError",
      "Matching inspects only the outer error",
    ],
  )
}

test test_api_core_assert_documents_lazy_context_and_core_error_identity {
  api(["language:core.assert"])? |> assert_contains(
    _,
    ["api: language.core.assert", "assert actual == expected", "assertion-failed", "only on false"],
  )
}

test test_api_core_enums_documents_nominal_constructors_aliases_and_singletons {
  api(["language:core.enums"])? |> assert_contains(
    _,
    [
      "nominal",
      "module namespace",
      "parse.enum-migration",
      "enum Token { Present(Str) }",
      "type SelectedMode = Mode",
      "enum State: Str",
      "atomically",
      "never convert Str",
    ],
  )
}

test test_api_path_interpolation_distinguishes_native_bytes_and_human_text {
  api(["language:core.path-literals"])? |> assert_contains(
    _,
    ["Path fragments as native bytes", "F-strings, print", "{config_path}.sha256"],
  )
  api(["language:core.command-interpolation"])? |> assert_contains(
    _,
    ["Compound process words retain interpolated Path bytes"],
  )
}

test test_api_duration_arithmetic_explains_dimensions_and_adapter_boundaries {
  api(["language:core.duration-arithmetic"])? |> assert_contains(
    _,
    [
      "nonnegative Int",
      "interval count",
      "once left to right",
      "pure",
      "clamping and saturation",
      "250ms * attempt",
    ],
  )
  api(["api:time.millis"])? |> assert_contains(_, ["Negative counts clamp to zero"])
}

test test_api_block_strings_explains_exact_margin_source_boundaries_and_literal_domains {
  api(["language:core.block-strings"])? |> assert_contains(
    _,
    [
      "exact prefix",
      "no implicit trailing newline",
      "longest matching",
      "original source spans",
      "Bytes, Path, glob, regex",
      "name={name}",
    ],
  )
}

test test_api_process_commands_document_exact_bytes_stdin_ownership {
  api(["api:process.command_argv", "api:process.command"])? |> assert_contains(
    _,
    ["stdin: Bytes", "stdin: Path", "temporary file", "empty Bytes", "hello"],
  )
}

test test_api_core_procs_demonstrates_static_named_argument_spreading {
  api(["language:core.procs"])? |> assert_contains(_, ["checked finite Record fields", "greet(...options)"])
}

test test_api_lexical_error_context_retains_contract_and_executable_example {
  api(["language:core.error-context"])? |> assert_contains(
    _,
    [
      "status: exact",
      "api: language.core.error-context",
      "Stored or directly returned Err data stays unchanged",
      "let count = ctx",
    ],
  )
}

test test_api_constants_retains_preparation_contract_and_executable_example {
  api(["language:core.constants"])? |> assert_contains(
    _,
    ["status: exact", "prepared immutable data", "const format_version = 1"],
  )
}

test test_api_value_pipeline_retains_argument_placement_and_evaluation_contract {
  api(["language:core.value-pipelines"])? |> assert_contains(
    _,
    ["status: exact", "Input evaluates once before", "pipeline_join(\"[\", _, \"]\")"],
  )
}

test test_api_absence_lookups_preserves_byte_and_result_boundaries {
  api(["language:core.absence-lookups"])? |> assert_contains(
    _,
    [
      "successful zero",
      "negative/out-of-range",
      "Ok(null)",
      "typed errors",
      "ordered receiver/index/fallback",
      "checked lookup origin",
      "entries.get(\"missing\") ?? 7",
    ],
  )
}

test test_api_scalar_iteration_keeps_direct_source_and_snapshot_contract {
  api(["language:core.scalar-iteration"])? |> assert_contains(
    _,
    [
      "api: language.core.scalar-iteration",
      "retains its snapshot and view bounds",
      "for character in \"café\"",
      "for octet in b\"\\0\\xff\"",
    ],
  )
}

test test_api_process_accept_policy_documents_actual_status_and_completion_boundary {
  api(["language:run.status"])? |> assert_contains(
    _,
    ["--accept=EXPR", "ProcessError.UnexpectedExit", "actual Status and .ok are unchanged"],
  )
  api(["api:process.command_argv"])? |> assert_contains(_, ["accept: List[Int] = default"])
}

test test_api_context_scopes_describes_restoration_and_demonstrates_value_forms {
  api(["language:core.context-scopes"])? |> assert_contains(
    _,
    ["body ? propagates", "env ({CC:", "cd (p\".\")"],
  )
}

test test_api_local_inference_describes_one_fixed_type_and_static_contributions {
  api(["language:core.local-inference"])? |> assert_contains(
    _,
    ["status: exact", "Aliases share the same type identity", "var selected = null"],
  )
}
