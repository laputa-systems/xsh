use source_metrics as metrics

pure value(scan: metrics.SourceScan, name: Str) -> Int {
  let rows = scan.metrics |> where .name == name |> collect
  rows[0].value
}

pure current_stage_evidence(source_path: Str, count: Int) -> List[metrics.Evidence] {
  let evidence: List[metrics.Evidence] = collect {
    var line = 1
    while line <= count {
      yield {path: source_path, line: line, detail: f"stage {line}"}
      line += 1
    }
  }
  evidence
}
pure current_source_report() -> metrics.Report {
  metrics.Report(schema: 1, sources: [], metrics: [
    {name: "lowering_type_resolver_definitions", value: 0, evidence: []},
    {name: "stage_variants", value: 46, evidence: current_stage_evidence("src/runtime/eval/indexed/full.rs", 46)},
  ])
}

pure source_contract_rejection(report: metrics.Report) -> Str {
  match metrics.assert_source_contract(report) {
    Ok(_) => "accepted"
    Err(.Contract {detail}) => detail
    Err(error) => error.message
  }
}

proc rejection_at(source_path: Str, source: Str) [error] -> Str {
  match metrics.scan_file(source_path, source) {
    Ok(_) => "accepted"
    Err(metrics.MetricError.Invalid {detail, ..}) => detail
    Err(error) => error.message
  }
}

proc rejection(source: Str) [error] -> Str {
  rejection_at("src/fixture.rs", source)
}

pure schema_rows(start_ordinal: Int, end_ordinal: Int) -> Str {
  var rows = ""
  var ordinal = start_ordinal
  while ordinal < end_ordinal {
    rows = f"{rows}Variant{ordinal} => Tag{ordinal} = {ordinal} [0];\n"
    ordinal += 1
  }
  rows
}

proc measured(root: Path) [fs, error] -> Result[metrics.Report] {
  match metrics.measure(root, ["src", "crates/xsht/src"]) {
    Ok(report) => report
    Err(metrics.MetricError.Invalid {path: source_path, line, detail}) => fail f"{source_path}:{line}: {detail}"
    Err(error) => Err(error)
  }
}

proc tree(ctx: TestContext) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "structural-source")?
  fp"{root}/src".mkdir()
  fp"{root}/crates/xsht/src".mkdir()
  fp"{root}/crates/xsht/src/lib.rs".write("// tool root\n")
  root
}

test test_literal_delimiters_comments_and_lifetimes_do_not_own_items {
  let source = "fn live<'a>(s: &'a str) -> &'a str {\n let a = \"} // indexed_raw() \\\"\";\n let b = br###\"/* { indexed_finish() */\"###;\n let c = cr#\"} cfg(test)\"#;\n let d = b'}'; let e = '\\u{7d}';\n /* nested /* } */ indexed_decode() { */\n s\n}\n#[cfg(test)]\nmod tests {\n let a = r#\"} /*\"#;\n #[cfg(test)] mod nested { fn hidden() {} }\n}\n// retained comment\n\n"
  let scan = metrics.scan_file("src/fixture.rs", source)?
  assert scan.count.physical == 15
  assert scan.count.retained_lines == [1, 2, 3, 4, 5, 6, 7, 8, 14, 15]
  assert value(scan, "manual_payload_calls") == 0
  assert value(scan, "optional_raw_calls") == 0
}

test test_cfg_keeps_every_possible_non_test_assignment {
  let source = "#[cfg(any(target_os = \"linux\", test))]\nfn platform() {}\n#[cfg(all(test, unix))]\nfn hidden() {}\n#[cfg(all(unix, not(unix)))]\nfn impossible() {}\n#[cfg(not(test))]\nfn production() {}\n#[cfg_attr(test, allow(dead_code), derive(Debug))]\nstruct Metadata;\n"
  let scan = metrics.scan_file("src/fixture.rs", source)?
  assert scan.count.retained_lines == [1, 2, 7, 8, 9, 10]
  assert value(scan, "blanket_dead_code_lines") == 0
}

test test_cfg_predicates_share_assignments_across_attributes {
  let source = "#[cfg(unix)]\n#[cfg(not(unix))]\nfn impossible() {}\nfn live() {}\n"
  assert metrics.scan_file("src/fixture.rs", source)?.count.retained_lines == [4]
}

test test_physical_line_is_retained_when_any_production_token_intersects {
  let source = "#[cfg(test)] fn hidden() {} fn live() {}\n#[cfg(test)]\nfn other() {}\n\n// outside\n"
  let scan = metrics.scan_file("src/fixture.rs", source)?
  assert scan.count.physical == 5
  assert scan.count.retained_lines == [1, 4, 5]
}

test test_multiline_production_literal_retains_every_physical_line {
  let source = "const TEXT: &str = r#\"first\n#[cfg(test)] fn pretend() {}\nlast\"#;\n#[cfg(test)] fn hidden() {}\n"
  assert metrics.scan_file("src/fixture.rs", source)?.count.retained_lines == [1, 2, 3]
}

test test_external_test_modules_follow_path_and_normal_nested_module_rules { |ctx|
  let root = tree(ctx)?
  fp"{root}/src/feature".mkdir()
  fp"{root}/src/lib.rs".write("mod feature;\n#[cfg(test)]\n#[path = \"odd_tests.rs\"]\nmod tests;\n")
  fp"{root}/src/odd_tests.rs".write("fn hidden() {}\n")
  fp"{root}/src/feature.rs".write("#[cfg(test)]\nmod tests;\nfn live() {}\n")
  fp"{root}/src/feature/tests.rs".write("fn hidden_nested() {}\n")
  let report = measured(root)?
  let excluded = report.sources |> where .production == 0 |> map .path |> sort
  assert excluded == ["src/feature/tests.rs", "src/odd_tests.rs"]
  let source = report.metrics |> where .name == "source_lines_src" |> collect
  assert source[0].value == 2
  let encoded = json.encode(report)?
  assert json.decode(encoded)?.require(metrics.Report)? == report
}

test test_shared_external_module_remains_production_when_any_reference_is_production { |ctx|
  let root = tree(ctx)?
  fp"{root}/src/lib.rs".write("#[cfg(test)] #[path = \"shared.rs\"] mod tests;\n#[cfg(any(unix, test))] #[path = \"shared.rs\"] mod platform;\n")
  fp"{root}/src/shared.rs".write("fn live() {}\n")
  let report = measured(root)?
  let shared = report.sources |> where .path == "src/shared.rs" |> collect
  assert shared[0].production == 1
}

test test_inline_path_and_external_blanket_scopes_follow_module_ownership { |ctx|
  let root = tree(ctx)?
  fp"{root}/src/thread_files".mkdir()
  fp"{root}/src/lib.rs".write("#[allow(dead_code)]\n#[path = \"thread_files\"]\nmod thread { #[path = \"tls.rs\"] mod local_data; }\n")
  fp"{root}/src/thread_files/tls.rs".write("// inherited allowance\nfn unused() {}\n")
  let report = measured(root)?
  let blanket = report.metrics |> where .name == "blanket_dead_code_lines" |> collect
  assert blanket[0].value == 5
  assert (blanket[0].evidence |> where .path == "src/thread_files/tls.rs" |> count()) == 2
}

test test_dead_code_blanket_metric_separates_item_and_scope_allowances {
  let narrow = "#[allow(dead_code)]\nfn hidden() {}\n#[allow(dead_code)]\nstruct Item;\n#[allow(dead_code)]\nenum Kind { A, B }\n"
  let scan = metrics.scan_file("src/fixture.rs", narrow)?
  assert value(scan, "blanket_dead_code_lines") == 0
  assert (scan.item_allowances |> map .target |> collect) == ["fn::hidden", "struct::Item", "enum::Kind"]
  assert (scan.item_allowances |> where .covered_by_local_scope |> count()) == 0

  let field = "struct Owner {\n    #[allow(dead_code)]\n    value: usize,\n}\n"
  let field_scan = metrics.scan_file("src/fixture.rs", field)?
  assert value(field_scan, "blanket_dead_code_lines") == 0
  assert field_scan.item_allowances[0].target == "field::Owner.value"

  let broad = "#[allow(dead_code)]\nmod child { fn hidden() {} }\n#[allow(dead_code)]\nimpl Thing { fn hidden() {} }\n#[allow(dead_code)]\ntrait Contract { fn hidden(); }\n"
  let scopes = metrics.scan_file("src/fixture.rs", broad)?
  assert value(scopes, "blanket_dead_code_lines") == scopes.count.production
  assert scopes.item_allowances.is_empty()
}

test test_macro_generated_scopes_follow_rust_lint_attributes {
  let macro_definition = "#[allow(dead_code)]\nmacro_rules! generated { () => { fn generated_function() {} struct GeneratedType; }; }\ngenerated!();\n"
  let macro_definition_scan = metrics.scan_file("src/fixture.rs", macro_definition)?
  assert value(macro_definition_scan, "blanket_dead_code_lines") == 0
  assert macro_definition_scan.item_allowances[0].kind == "macro_definition"

  let macro_invocation = "macro_rules! generated { () => { fn generated_function() {} struct GeneratedType; }; }\n#[allow(dead_code)]\ngenerated!();\n"
  let macro_invocation_scan = metrics.scan_file("src/fixture.rs", macro_invocation)?
  assert value(macro_invocation_scan, "blanket_dead_code_lines") == 0
  assert macro_invocation_scan.item_allowances[0].kind == "macro_invocation"

  let generated = "macro_rules! generated {\n    () => {\n        #[allow(dead_code)]\n        mod generated_items {\n            fn generated_function() {}\n            struct GeneratedType;\n        }\n    };\n}\ngenerated!();\n"
  let generated_scan = metrics.scan_file("src/fixture.rs", generated)?
  assert value(generated_scan, "blanket_dead_code_lines") == 5
  assert generated_scan.item_allowances.is_empty()
}

test test_inner_blankets_and_inherited_module_scopes_cover_item_inventory { |ctx|
  let local = "impl Thing {\n#![allow(dead_code)]\n#[allow(dead_code)]\nfn hidden() {}\n}\n"
  let scan = metrics.scan_file("src/fixture.rs", local)?
  assert value(scan, "blanket_dead_code_lines") == scan.count.production
  assert scan.item_allowances[0].covered_by_local_scope

  let root = tree(ctx)?
  fp"{root}/src/lib.rs".write("#[allow(dead_code)]\n#[path = \"external.rs\"]\nmod external;\n")
  fp"{root}/src/external.rs".write("#[allow(dead_code)]\nfn hidden() {}\n")
  let allowance_report = metrics.measure_with_allowances(root, ["src", "crates/xsht/src"])?
  assert allowance_report.item_allowances.entries.len() == 1
  assert allowance_report.item_allowances.entries[0].path == "src/external.rs"
  assert allowance_report.item_allowances.entries[0].inherited_module_scope
  assert ! allowance_report.item_allowances.entries[0].covered_by_local_scope
}

test test_inner_cfg_and_braced_constant_initializers_have_exact_boundaries {
  let source = "mod tests {\n#![cfg(test)]\nfn hidden() {}\n}\nfn live() {}\n#[cfg(test)] const VALUE: Foo = Foo { x: 1 }\n .field;\nfn other() {}\n"
  assert metrics.scan_file("src/fixture.rs", source)?.count.retained_lines == [5, 8]
  let generic = "#[cfg(test)] fn hidden<const N: usize>() { let value = 1; }\nfn live() {}\n"
  assert metrics.scan_file("src/fixture.rs", generic)?.count.retained_lines == [2]
}

test test_file_inner_cfg_removes_the_complete_file {
  let source = "// file comment\n#![cfg(test)]\n\nfn hidden() {}\n"
  assert metrics.scan_file("src/fixture.rs", source)?.count.production == 0
}

test test_only_tokens_contribute_calls_and_wide_alias_patterns {
  let source = "use ArenaPatternKind as P;\nfn live() { indexed_raw(x); indexed_decode::<bool>(x); indexed_finish(x); indexed_optional_raw(x);\nmatch kind { P::A => {}, P::B => {}, P::C => {}, P::D => {}, P::E => {}, P::F => {}, P::G => {}, P::H => {}, _ => {} }\n}\n#[cfg(test)] fn hidden() { indexed_raw(x); }\n"
  let scan = metrics.scan_file("src/fixture.rs", source)?
  assert value(scan, "manual_payload_calls") == 3
  assert value(scan, "optional_raw_calls") == 1
  assert value(scan, "wide_arena_wildcards") == 1
  let wide = scan.metrics |> where .name == "wide_arena_wildcards" |> collect
  assert wide[0].evidence[0].line == 3
  let tuple = "use ArenaPatternKind as P; fn live() { match (kind, flag) { (P::A, _) => {}, (P::B, _) => {}, (P::C, _) => {}, (P::D, _) => {}, (P::E, _) => {}, (P::F, _) => {}, (P::G, _) => {}, (P::H, _) => {}, _ => {} } }"
  assert value(metrics.scan_file("src/fixture.rs", tuple)?, "wide_arena_wildcards") == 1
}

test test_instruction_variants_support_hand_enums_and_generated_schema {
  let handwritten = "enum BuildIntRow { Int, Slot }\nenum BuildBoolRow { Bool }\nenum BuildExprRow { Null, Binary { left: u32, right: u32 } }\nenum BuildStmtRow { Break, Run { command: u32 } }\n"
  assert value(metrics.scan_file("src/fixture.rs", handwritten)?, "instruction_variants") == 7

  let generated = [
    "instruction_schema! {",
    "BuildIntRow => IntView [Int] {", "Int(value: i64) => IntInt = 0 [0; Operands];", "Slot(slot: usize) => IntSlot = 1 [0];", schema_rows(2, 6), "}",
    "BuildBoolRow => BoolView [Bool] {", "Bool(value: bool) => BoolBool = 6 [0];", "ContainsSlot { slot: usize, needle: Vec<(BuildExprId, Option<BuildExprId>)> } => BoolContainsSlot = 7 [EFFECT_PROCESS | EFFECT_TRACE; Scheduled];", schema_rows(8, 18), "}",
    "BuildExprRow => ExprView [Expr] {", "Null => ExprNull = 18 [0];", "Binary { op: BinaryOp, operands: Vec<(BuildExprId, Option<BuildExprId>)>, label: &'static str, span: Span } => ExprBinary = 19 [EFFECT_TRACE];", schema_rows(20, 115), "}",
    "BuildStmtRow => StmtView [Stmt] {", "Break => StmtBreak = 115 [0];", "Run { body: Vec<BuildStmtId>, span: Span } => StmtRun = 116 [EFFECT_PROCESS | EFFECT_CANCELLATION | EFFECT_TRACE];", schema_rows(117, 158), "}",
    "}",
  ].join("\n")
  match metrics.scan_file("src/runtime/eval/indexed/full/schema.rs", generated) {
    Ok(schema) => {
      assert value(schema, "instruction_variants") == 158
      assert schema.count.production == generated.split("\n").len()
    }
    Err(metrics.MetricError.Invalid {detail, ..}) => test.fail(detail)
    Err(error) => test.fail(error.message)
  }
}

test test_instruction_schema_unknown_and_malformed_layouts_fail_loudly {
  assert "unknown instruction schema row" in rejection("instruction_schema! { BuildTaskRow => TaskView [Task] { Item => TaskItem = 0 [0]; } }")
  assert "malformed instruction schema entry" in rejection("instruction_schema! { BuildIntRow => IntView [Int] { Value Missing = 0 [0]; } }")
  assert "no supported table invocation" in rejection("macro_rules! instruction_schema { () => {}; }")
}

test test_type_storage_mapping_metric_recognizes_both_direct_directions {
  let source = "fn map_type_to_kind(ty: &Type) -> Result<LoweredType, Error> { match ty { Type::Unit => Ok(LoweredType::Unit), Type::Int => Ok(LoweredType::Int), Type::Bool => Ok(LoweredType::Bool), Type::Str => Ok(LoweredType::Str), Type::Bytes => Ok(LoweredType::Bytes), Type::Path => Ok(LoweredType::Path), Type::Command => Ok(LoweredType::Command), Type::Duration => Ok(LoweredType::Duration), } }\nfn map_kind_to_type(kind: LoweredType) -> Option<Type> { match kind { LoweredType::Unit => Some(Type::Unit), LoweredType::Int => Some(Type::Int), LoweredType::Bool => Some(Type::Bool), LoweredType::Str => Some(Type::Str), LoweredType::Bytes => Some(Type::Bytes), LoweredType::Path => Some(Type::Path), LoweredType::Command => Some(Type::Command), LoweredType::Duration => Some(Type::Duration), } }\n"
  let mappings = metrics.scan_file("src/runtime/eval/lower/types.rs", source)?.metrics |> where .name == "type_storage_mapping_definitions" |> collect
  assert mappings[0].value == 2
  assert mappings[0].evidence[0].detail == "map_type_to_kind"
  assert mappings[0].evidence[1].detail == "map_kind_to_type"
}

test test_type_storage_mapping_metric_excludes_delegating_and_partial_wrappers {
  let source = "fn lowered_checked_type(ty: &Type) -> Option<LoweredType> { match ty { Type::Validated(_) => None, _ => map_type_to_kind(ty).ok() } }\nfn type_for_lowered_type(kind: LoweredType) -> Option<Type> { match kind { LoweredType::Set | LoweredType::Tag => None, _ => map_kind_to_type(kind) } }\nfn project_one_checked_case(ty: &Type) -> Option<LoweredType> { match ty { Type::Int => Some(LoweredType::Int), _ => None } }\n"
  assert value(metrics.scan_file("src/runtime/eval/lower/types.rs", source)?, "type_storage_mapping_definitions") == 0
}

test test_type_storage_mapping_metric_rejects_unclassified_large_conversions {
  let source = "fn map_semantic_type(ty: &Type) -> Result<LoweredType, Error> { match ty { Type::Unit => map_unit(ty), Type::Int => map_int(ty), Type::Bool => map_bool(ty), Type::Str => map_str(ty), Type::Bytes => map_bytes(ty), Type::Path => map_path(ty), Type::Command => map_command(ty), Type::Duration => map_duration(ty), } }\n"
  assert "unclassified Type/LoweredType conversion body" in rejection_at("src/runtime/eval/lower/types.rs", source)
}

test test_lowering_type_resolver_metric_recognizes_recursive_semantic_walkers {
  let source = "fn resolve_runtime_type(arena: &AstArena, ty: TypeExprId) -> Option<LoweredType> {\n let index = ty.index();\n let data = arena.type_expr_data[index];\n match arena.type_expr_tags[index] {\n ArenaTypeExprTag::List => Some(LoweredType::List),\n ArenaTypeExprTag::Named => resolve_runtime_type(arena, TypeExprId::from_index(data.lhs as usize)),\n }\n}\n"
  let scan = metrics.scan_file("src/runtime/eval/lower/types.rs", source)?
  let resolvers = scan.metrics |> where .name == "lowering_type_resolver_definitions" |> collect
  assert resolvers.len() == 1
  assert resolvers[0].value == 1
  assert resolvers[0].evidence[0].detail == "resolve_runtime_type"
}

test test_lowering_type_resolver_metric_recognizes_nonrecursive_primitive_walkers {
  let source = "fn lower_primitive_type(arena: &AstArena, ty: TypeExprId) -> Type {\n let index = ty.index();\n match arena.type_expr_tags[index] {\n ArenaTypeExprTag::Bool => Type::Bool,\n ArenaTypeExprTag::Int => Type::Int,\n }\n}\n"
  let scan = metrics.scan_file("src/runtime/eval/lower/types.rs", source)?
  let resolvers = scan.metrics |> where .name == "lowering_type_resolver_definitions" |> collect
  assert resolvers.len() == 1
  assert resolvers[0].value == 1
  assert resolvers[0].evidence[0].detail == "lower_primitive_type"
}

test test_lowering_type_resolver_metric_excludes_checked_type_wrappers_and_renderers {
  let source = "fn compact_type_check(kind: LoweredType, checked: &Type, name: Arc<str>) -> Option<LoweredTypeCheck> {\n (needs_static_check(kind) || checked.is_validated()).then(|| LoweredTypeCheck { ty: checked.clone(), name })\n}\nfn compact_type_expr_name_string(arena: &AstArena, ty: TypeExprId) -> String {\n let index = ty.index();\n match arena.type_expr_tags[index] {\n ArenaTypeExprTag::List => compact_type_expr_name_string(arena, TypeExprId::from_index(index)),\n ArenaTypeExprTag::Named => String::new(),\n }\n}\n"
  let scan = metrics.scan_file("src/runtime/eval/lower/types.rs", source)?
  assert value(scan, "lowering_type_resolver_definitions") == 0
}

test test_lowering_type_resolver_metric_rejects_unclassified_recursive_walkers {
  let source = "fn walk_type_expr(arena: &AstArena, ty: TypeExprId) -> UnknownType {\n let index = ty.index();\n match arena.type_expr_tags[index] {\n ArenaTypeExprTag::List => walk_type_expr(arena, TypeExprId::from_index(index)),\n ArenaTypeExprTag::Named => UnknownType::fallback(),\n }\n}\n"
  assert "unclassified type-expression tag walker" in rejection_at("src/runtime/eval/lower/types.rs", source)
}

test test_lowering_type_resolver_metric_rejects_indirect_semantic_walkers {
  let source = "fn resolve_primitive_type(arena: &AstArena, ty: TypeExprId) -> Result<Type, TypeError> {\n let index = ty.index();\n match arena.type_expr_tags[index] {\n ArenaTypeExprTag::Bool => build_bool_type(arena, ty),\n ArenaTypeExprTag::Int => build_int_type(arena, ty),\n }\n}\n"
  assert "unclassified type-expression tag walker" in rejection_at("src/runtime/eval/lower/types.rs", source)
}

test test_current_report_requires_zero_lowering_type_resolvers {
  let report = current_source_report()
  metrics.assert_source_contract(report)
  let nonzero = { ...report, metrics: collect {
    for row in report.metrics { yield if row.name == "lowering_type_resolver_definitions" { {...row, value: 1} } else { row } }
  } }
  assert "lowering_type_resolver_definitions" in source_contract_rejection(nonzero)
}

test test_pipeline_stage_schema_counts_unit_tuple_and_struct_variants {
  let source = "pipeline_stage_schema! { @context stage_builder stage_decoder stage_payload; One unit {}; Two tuple { value: BuildExprId }; Three struct { body: Vec<BuildStmtId>, value: Option<(BuildExprId, Option<BuildExprId>)>, }; }"
  let scan = metrics.scan_file("src/fixture.rs", source)?
  let stages = scan.metrics |> where .name == "stage_variants" |> collect
  assert stages[0].value == 3
  assert stages[0].evidence[0].detail == "LoweredPipelineStage::One"
  assert stages[0].evidence[1].detail == "LoweredPipelineStage::Two"
  assert stages[0].evidence[2].detail == "LoweredPipelineStage::Three"
}

test test_pipeline_stage_schema_requires_one_valid_authority_table {
  let table = "pipeline_stage_schema! { @context stage_builder stage_decoder stage_payload; One unit {}; }"
  assert "malformed pipeline stage schema context" in rejection("pipeline_stage_schema! { One unit {}; }")
  assert "empty pipeline stage schema" in rejection("pipeline_stage_schema! { @context stage_builder stage_decoder stage_payload; }")
  assert "unknown pipeline stage schema form" in rejection("pipeline_stage_schema! { @context stage_builder stage_decoder stage_payload; One opaque {}; }")
  assert "repeated pipeline stage schema variant" in rejection("pipeline_stage_schema! { @context stage_builder stage_decoder stage_payload; One unit {}; One unit {}; }")
  assert "multiple pipeline stage schema tables" in rejection(f"{table} {table}")
  assert "no supported table invocation" in rejection("macro_rules! pipeline_stage_schema { () => {}; }")
  assert "multiple pipeline stage schema definitions" in rejection("macro_rules! pipeline_stage_schema { () => {}; } macro_rules! pipeline_stage_schema { () => {}; }")
  assert "unrecognized handwritten pipeline stage layout" in rejection_at("src/runtime/eval/indexed/full.rs", "enum LoweredPipelineStage { Old, }")
}

test test_legacy_pipeline_stage_layout_is_validated_without_double_counting {
  let indexed = p"dev/tests/fixtures/consolidation-legacy-full-pipeline.rs".read_text()?
  let encoded_tags = metrics.scan_file("src/runtime/eval/indexed/full.rs", indexed)?
  assert value(encoded_tags, "stage_variants") == 0

  let semantic = p"dev/tests/fixtures/consolidation-legacy-eval-pipeline.rs".read_text()?
  let lowered_stages = metrics.scan_file("src/runtime/eval.rs", semantic)?
  assert value(lowered_stages, "stage_variants") == 46
  assert value(encoded_tags, "stage_variants") + value(lowered_stages, "stage_variants") == 46
}

test test_pipeline_stage_authority_layouts_reject_overlap_and_partial_legacy_shapes {
  let legacy = p"dev/tests/fixtures/consolidation-legacy-full-pipeline.rs".read_text()?
  let generated = "macro_rules! pipeline_stage_schema { () => {}; }\npipeline_stage_schema! { @context stage_builder stage_decoder stage_payload; One unit {}; }\n"
  assert "overlap" in rejection_at("src/runtime/eval/indexed/full.rs", f"{legacy}\n{generated}")
  assert "unrecognized handwritten pipeline stage layout" in rejection_at("src/runtime/eval/indexed/full.rs", "enum FullStageTag { TextLines, }\n")
  let changed_codec = legacy.replace("=> TextLines {", with: "=> JsonLines {")
  assert "handwritten pipeline stage codec" in rejection_at("src/runtime/eval/indexed/full.rs", changed_codec)
}

test test_pipeline_stage_schema_accepts_complete_borrowed_view_context_only {
  assert "malformed pipeline stage schema context" in rejection("pipeline_stage_schema! { @context stage_builder stage_decoder stage_payload stage_view_context; One unit {}; }")
  assert "malformed pipeline stage schema context" in rejection("pipeline_stage_schema! { @context stage_builder stage_decoder stage_payload stage_view_context stage_view_input stage_view_value; One unit {}; }")
}

test test_current_report_requires_generated_pipeline_stage_schema {
  let report = current_source_report()
  metrics.assert_source_contract(report)
  let wrong_value = { ...report, metrics: collect {
    for row in report.metrics { yield if row.name == "stage_variants" { {...row, value: 45} } else { row } }
  } }
  let wrong_evidence_count = { ...report, metrics: collect {
    for row in report.metrics { yield if row.name == "stage_variants" { {...row, evidence: current_stage_evidence("src/runtime/eval/indexed/full.rs", 45)} } else { row } }
  } }
  let wrong_evidence_path = { ...report, metrics: collect {
    for row in report.metrics { yield if row.name == "stage_variants" { {...row, evidence: current_stage_evidence("src/runtime/eval/indexed/other.rs", 46)} } else { row } }
  } }
  assert "stage_variants" in source_contract_rejection(wrong_value)
  assert "stage_variants" in source_contract_rejection(wrong_evidence_count)
  assert "stage_variants" in source_contract_rejection(wrong_evidence_path)
}

test test_current_report_requires_unique_lowering_and_stage_metrics {
  let report = current_source_report()
  let without_lowering = { ...report, metrics: report.metrics |> where .name != "lowering_type_resolver_definitions" |> collect }
  let without_stages = { ...report, metrics: report.metrics |> where .name != "stage_variants" |> collect }
  let lowering = report.metrics |> where .name == "lowering_type_resolver_definitions" |> collect
  let stages = report.metrics |> where .name == "stage_variants" |> collect
  let duplicate_lowering = { ...report, metrics: report.metrics + [lowering[0]] }
  let duplicate_stages = { ...report, metrics: report.metrics + [stages[0]] }
  assert "lowering_type_resolver_definitions" in source_contract_rejection(without_lowering)
  assert "stage_variants" in source_contract_rejection(without_stages)
  assert "lowering_type_resolver_definitions" in source_contract_rejection(duplicate_lowering)
  assert "stage_variants" in source_contract_rejection(duplicate_stages)
}

test test_malformed_lexical_forms_and_eligibility_cfg_attr_fail_loudly {
  assert "unterminated block comment" in rejection("/* open\n")
  assert "unterminated Rust literal" in rejection("fn f() { let x = r##\"open\"#; }")
  assert "mismatched Rust delimiters" in rejection("fn f() { ] }")
  assert "multi-character" in rejection("fn f() { let x = 'ab'; }")
  assert "unsupported Rust escape" in rejection("fn f() { let x = \"bad\\z\"; }")
  assert "invalid Unicode scalar" in rejection("fn f() { let x = '\\u{d800}'; }")
  assert "cfg_attr eligibility" in rejection("#[cfg_attr(test, cfg(unix))] fn f() {}")
  assert "cfg_attr eligibility" in rejection("#[cfg_attr(test, path = \"tests.rs\")] mod f;")
  assert "cfg not requires one" in rejection("#[cfg(not(test, unix))] fn f() {}")
}


pure ratchet_report(manual_payloads: Int, src_lines: Int) -> metrics.Report {
  metrics.Report(schema: 1, sources: [], metrics: [
    {name: "blanket_dead_code_lines", value: 0, evidence: []},
    {name: "manual_payload_calls", value: manual_payloads, evidence: []},
    {name: "source_lines_src", value: src_lines, evidence: []},
    {name: "source_lines_xsht", value: 5, evidence: []},
  ])
}

pure ratchet_baseline(manual_payloads: Int) -> metrics.Baseline {
  metrics.Baseline(schema: 1, metrics: [
    {name: "blanket_dead_code_lines", value: 0},
    {name: "manual_payload_calls", value: manual_payloads},
  ])
}

pure ratchet_rejection(report: metrics.Report, baseline: metrics.Baseline) -> Str {
  match metrics.verify(report, baseline) {
    Ok(_) => "accepted"
    Err(.Rise {name, before, after}) => f"rise {name}: {before} -> {after}"
    Err(.Contract {detail}) => detail
    Err(error) => error.message
  }
}

pure stage_table(count: Int) -> Str {
  var rows = ""
  var index = 0
  while index < count {
    rows = f"{rows} Stage{index} unit {{}};"
    index += 1
  }
  f"macro_rules! pipeline_stage_schema {{ () => {{}}; }}\npipeline_stage_schema! {{ @context stage_builder stage_decoder stage_payload;{rows} }}\n"
}

proc ratchet_tree(ctx: TestContext, payload_calls: Int) [fs, error] -> Result[Path] {
  let root = tree(ctx)?
  fp"{root}/src/runtime/eval/indexed".mkdir(parents: true)
  fp"{root}/src/runtime/eval/indexed/full.rs".write(stage_table(46))
  var calls = ""
  for _ in range(payload_calls) { calls = f"{calls} indexed_raw(x);" }
  fp"{root}/src/extra.rs".write(f"fn live() {{{calls} }}\n")
  fp"{root}/src/lib.rs".write("#[path = \"runtime/eval/indexed/full.rs\"] mod full;\nmod extra;\n")
  fp"{root}/dev".mkdir()
  root
}

test test_structural_rise_fails_with_the_metric_and_values {
  assert ratchet_rejection(ratchet_report(1, 10), ratchet_baseline(1)) == "accepted"
  assert ratchet_rejection(ratchet_report(2, 10), ratchet_baseline(1)) == "rise manual_payload_calls: 1 -> 2"
  match metrics.lowered(ratchet_report(2, 10), ratchet_baseline(1)) {
    Ok(_) => test.fail("a risen metric was recorded as lowered")
    Err(.Rise {name, before, after}) => assert name == "manual_payload_calls" and before == 1 and after == 2
    Err(error) => test.fail(error.message)
  }
}

test test_a_lowered_metric_must_be_recorded_in_the_baseline {
  assert "manual_payload_calls fell from 2 to 1" in ratchet_rejection(ratchet_report(1, 10), ratchet_baseline(2))
  let recorded = metrics.lowered(ratchet_report(1, 10), ratchet_baseline(2))?
  assert recorded == ratchet_baseline(1)
  assert ratchet_rejection(ratchet_report(1, 10), recorded) == "accepted"
}

test test_source_line_totals_stay_advisory {
  for src_lines in [0, 10, 11, 100000] {
    assert ratchet_rejection(ratchet_report(1, src_lines), ratchet_baseline(1)) == "accepted"
  }
  let values = metrics.structural_values(ratchet_report(1, 10))? |> map .name
  assert values == ["blanket_dead_code_lines", "manual_payload_calls"]
}

test test_changed_inventories_and_malformed_reports_fail {
  let report = ratchet_report(1, 10)
  let baseline = ratchet_baseline(1)
  let malformed_reports = [
    {...report, schema: 2},
    {...report, metrics: report.metrics |> where .name != "blanket_dead_code_lines" |> collect},
    {...report, metrics: report.metrics + [{name: "new_metric", value: 0, evidence: []}]},
    {...report, metrics: report.metrics + [report.metrics[1]]},
    {...report, metrics: report.metrics |> where .name != "source_lines_xsht" |> collect},
    {...report, metrics: collect { for row in report.metrics { yield if row.name == "source_lines_src" { {...row, value: -1} } else { row } } }},
    {...report, metrics: collect { for row in report.metrics { yield if row.name == "blanket_dead_code_lines" { {...row, value: -1} } else { row } } }},
  ]
  for malformed in malformed_reports {
    assert ratchet_rejection(malformed, baseline) != "accepted"
  }
  let malformed_baselines = [
    {...baseline, schema: 2},
    {...baseline, metrics: baseline.metrics + [baseline.metrics[0]]},
    {...baseline, metrics: baseline.metrics |> where .name != "manual_payload_calls" |> collect},
    {...baseline, metrics: baseline.metrics + [{name: "source_lines_src", value: 10}]},
  ]
  for malformed in malformed_baselines {
    assert ratchet_rejection(report, malformed) != "accepted"
  }
}

test test_check_and_update_commands_measure_the_source_tree { |ctx|
  let root = ratchet_tree(ctx, 1)?
  let recorded = metrics.structural_values(measured(root)?)?
  fp"{root}/dev/ratchets.json".write(json.encode(metrics.Baseline(schema: 1, metrics: recorded), pretty: true)? + "\n")
  let driver = p"dev/ratchets.xsh".read_text()?
  let report_path = fp"{root}/.work/gate/ratchets.json"
  let passed = test.expect(ctx, driver, status: 0, args: ["check", "--root", root, "--report", report_path])?
  assert "structural metrics match dev/ratchets.json" in passed.stdout, passed.stdout
  let saved: metrics.Report = json.read(report_path)?.require()?
  assert (saved.metrics |> where .name == "manual_payload_calls" |> map .value) == [1]

  fp"{root}/src/extra.rs".write("fn live() { indexed_raw(x); indexed_raw(y); }\n")
  let risen = test.expect(ctx, driver, status: 1, args: ["check", "--root", root])?
  assert "ratchet rise manual_payload_calls: 1 -> 2" in risen.stderr, risen.stderr
  let refused = test.expect(ctx, driver, status: 1, args: ["update", "--root", root])?
  assert "ratchet rise manual_payload_calls: 1 -> 2" in refused.stderr, refused.stderr

  fp"{root}/src/extra.rs".write("fn live() {}\n")
  let fell = test.expect(ctx, driver, status: 1, args: ["check", "--root", root])?
  assert "manual_payload_calls fell from 1 to 0" in fell.stderr, fell.stderr
  test.expect(ctx, driver, status: 0, args: ["update", "--root", root])?
  let lowered: metrics.Baseline = json.read(fp"{root}/dev/ratchets.json")?.require()?
  assert (lowered.metrics |> where .name == "manual_payload_calls" |> map .value) == [0]
  assert fp"{root}/dev/ratchets.json".read_text()? == json.encode(lowered, pretty: true)? + "\n"
  test.expect(ctx, driver, status: 0, args: ["check", "--root", root])?

  fp"{root}/src/runtime/eval/indexed/full.rs".write(stage_table(45))
  let contract = test.expect(ctx, driver, status: 1, args: ["check", "--root", root])?
  assert "requires 46 stage_variants" in contract.stderr, contract.stderr
}
