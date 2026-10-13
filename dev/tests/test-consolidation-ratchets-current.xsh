use consolidation_metrics_current as metrics

type AuditRise = {metric: Str, baseline: Int, current: Int, ceiling: Int, approval: Str, reason: Str, remove_when: Str}

const review_directory_paths = ["src", "crates", "tests", "core", "examples", "showcase", "tools", "dev/tests"]
const review_source_paths = [@review_directory_paths, "dev/consolidation_metrics.xsh", "dev/consolidation_metrics_current.xsh", "dev/consolidation/metrics.xsh"]

pure value(scan: metrics.SourceScan, name: Str) -> Int {
  let rows = scan.metrics |> where .name == name |> collect
  rows[0].value
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

proc install_blanket_baseline(root: Path, original: metrics.Report, corrected: metrics.Report) [fs, error] -> Result[Path] {
  fp"{root}/dev/consolidation".mkdir()
  fp"{root}/dev/tests".mkdir()
  fp"{root}/dev/consolidation_metrics.xsh".write("scanner fixture\n")
  fp"{root}/dev/consolidation_metrics_current.xsh".write("current scanner fixture\n")
  fp"{root}/dev/tests/test-consolidation-ratchets.xsh".write("ratchet fixture\n")
  fp"{root}/dev/tests/test-consolidation-ratchets-current.xsh".write("current ratchet fixture\n")
  let original_path = fp"{root}/dev/consolidation/baseline.json"
  let corrected_path = fp"{root}/dev/consolidation/baseline-blanket-scope.json"
  let diagnostic_path = fp"{root}/dev/consolidation/blanket-metric-start-diagnostic.json"
  let item_inventory_path = fp"{root}/dev/consolidation/item-allowances-start.json"
  json.write(original_path, original)
  json.write(corrected_path, corrected)
  json.write(diagnostic_path, corrected)
  json.write(item_inventory_path, {schema: 1, entries: []})
  let original_hash = hash.sha256(original_path)?.hex().lower()
  let corrected_hash = hash.sha256(corrected_path)?.hex().lower()
  let source_manifest_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  json.write(fp"{root}/dev/consolidation/baseline-metadata.json", {
    start_commit: "fixture", source_manifest_sha256: source_manifest_hash, baseline_report_sha256: original_hash,
  })
  let blanket = corrected.metrics |> where .name == "blanket_dead_code_lines" |> collect
  assert blanket.len() == 1
  let provenance = metrics.BlanketBaselineProvenance(
    schema: 1,
    start_commit: "fixture",
    source_manifest_sha256: source_manifest_hash,
    original_baseline_path: "dev/consolidation/baseline.json",
    original_baseline_sha256: original_hash,
    corrected_baseline_path: "dev/consolidation/baseline-blanket-scope.json",
    corrected_baseline_sha256: corrected_hash,
    corrected_blanket_lines: blanket[0].value,
    scanner_sha256: hash.sha256(fp"{root}/dev/consolidation_metrics.xsh")?.hex().lower(),
    ratchet_tests_sha256: hash.sha256(fp"{root}/dev/tests/test-consolidation-ratchets.xsh")?.hex().lower(),
  )
  let provenance_path = fp"{root}/dev/consolidation/blanket-metric-provenance.json"
  json.write(provenance_path, provenance)
  let metric_values = collect { for row in corrected.metrics { yield {name: row.name, value: row.value} } }
  json.write(fp"{root}/dev/consolidation/metric-observer-successor.json", {
    schema: 1,
    original_scanner_path: "dev/consolidation_metrics.xsh",
    original_scanner_sha256: provenance.scanner_sha256,
    successor_scanner_path: "dev/consolidation_metrics_current.xsh",
    successor_scanner_sha256: hash.sha256(fp"{root}/dev/consolidation_metrics_current.xsh")?.hex().lower(),
    original_baseline_path: "dev/consolidation/baseline.json",
    original_baseline_sha256: original_hash,
    corrected_baseline_path: "dev/consolidation/baseline-blanket-scope.json",
    corrected_baseline_sha256: corrected_hash,
    metadata_sha256: hash.sha256(fp"{root}/dev/consolidation/baseline-metadata.json")?.hex().lower(),
    historical_provenance_sha256: hash.sha256(provenance_path)?.hex().lower(),
    baseline_provenance_sha256: hash.sha256(provenance_path)?.hex().lower(),
    historical_ratchet_tests_sha256: provenance.ratchet_tests_sha256,
    baseline_ratchet_tests_sha256: provenance.ratchet_tests_sha256,
    current_ratchet_tests_path: "dev/tests/test-consolidation-ratchets-current.xsh",
    current_ratchet_tests_sha256: hash.sha256(fp"{root}/dev/tests/test-consolidation-ratchets-current.xsh")?.hex().lower(),
    start_commit: provenance.start_commit,
    source_manifest_sha256: provenance.source_manifest_sha256,
    same_baseline: {
      report_path: "dev/consolidation/blanket-metric-start-diagnostic.json",
      report_sha256: hash.sha256(diagnostic_path)?.hex().lower(),
      legacy_report_sha256: hash.sha256(diagnostic_path)?.hex().lower(),
      successor_report_sha256: hash.sha256(diagnostic_path)?.hex().lower(),
      metric_values: metric_values,
      source_count: corrected.sources.len(),
      item_inventory_path: "dev/consolidation/item-allowances-start.json",
      item_inventory_sha256: hash.sha256(item_inventory_path)?.hex().lower(),
      item_inventory_entries: 0,
    },
  })
  provenance_path
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

test test_current_lowering_tree_has_no_type_expr_resolvers {
  let root = p".".resolve()?
  var count = 0
  for entry in fs.walk(p"src/runtime/eval/lower")? |> where .kind == "file" and .path.ext() == "rs" |> sort-by .path {
    let source_path = entry.path.strip_prefix(root)?.display()
    count += value(metrics.scan_file(source_path, entry.path.read_text()?)?, "lowering_type_resolver_definitions")
  }
  let root_source = p"src/runtime/eval/lower.rs".read_text()?
  count += value(metrics.scan_file("src/runtime/eval/lower.rs", root_source)?, "lowering_type_resolver_definitions")
  assert count == 0
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

test test_generated_pipeline_stage_schema_counts_current_source {
  let source_path = "src/runtime/eval/indexed/full.rs"
  let scan = metrics.scan_file(source_path, p"src/runtime/eval/indexed/full.rs".read_text()?)?
  let stages = scan.metrics |> where .name == "stage_variants" |> collect
  assert stages.len() == 1
  assert stages[0].value == 46
  assert stages[0].evidence.len() == 46
  assert (stages[0].evidence |> where .path != source_path |> count()) == 0
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

test test_structural_rise_fails_with_the_metric_and_values { |ctx|
  let baseline = metrics.Report(schema: 1, sources: [], metrics: [
    {name: "blanket_dead_code_lines", value: 0, evidence: []},
    {name: "manual_payload_calls", value: 1, evidence: []},
    {name: "source_lines_src", value: 5, evidence: []},
    {name: "source_lines_xsht", value: 5, evidence: []},
  ], reviews: [])
  let current = metrics.Report(schema: baseline.schema, sources: baseline.sources, metrics: collect {
    for row in baseline.metrics { yield if row.name == "manual_payload_calls" { {...row, value: 2} } else { row } }
  }, reviews: baseline.reviews)
  match metrics.assert_not_risen(current, baseline) {
    Ok(_) => test.fail("rising structural metric passed")
    Err(metrics.MetricError.Rise {name, before, after}) => assert name == "manual_payload_calls" and before == 1 and after == 2
    Err(error) => test.fail(error.message)
  }
  metrics.assert_not_risen(baseline, current)
  let root = tree(ctx)?; fp"{root}/dev/consolidation".mkdir()
  json.write(fp"{root}/base.json", baseline); json.write(fp"{root}/current.json", current)
  json.write(fp"{root}/dev/consolidation/baseline-metadata.json", {start_commit: "fixture"})
  let driver = p"dev/consolidation/metrics.xsh".read_text()?
  let flags = ["--root", root.display(), "--source-root", "src", "--xsht-source-root", "crates/xsht/src", "--input", fp"{root}/current.json".display(), "--baseline", fp"{root}/base.json".display(), "--output", fp"{root}/saved.json".display(), "--audit", fp"{root}/audit.json".display()]
  assert ! test.run_script(ctx, driver, ["check", @flags])?.success
  let approved = {metric: "manual_payload_calls", ceiling: 2, approval: "fixture owner", reason: "bounded fixture", remove_when: "fixture ends"}
  json.write(fp"{root}/envelope.json", {start_commit: "fixture", limits: [approved]})
  assert ! test.run_script(ctx, driver, ["check", @flags, "--envelope", fp"{root}/envelope.json"])?.success
  let rises = json.get(json.read(fp"{root}/audit.json")?, ["rises"])?.require(List[Any])?
  assert rises.len() == 1
  assert ! test.run_script(ctx, driver, ["final-close", @flags, "--envelope", fp"{root}/envelope.json"])?.success
  json.write(fp"{root}/envelope.json", {start_commit: "fixture", limits: [{...approved, metric: "unknown"}]}); assert ! test.run_script(ctx, driver, ["check", @flags, "--envelope", fp"{root}/envelope.json"])?.success
}

test test_corrected_blanket_baseline_requires_provenance_and_changes_only_its_metric { |ctx|
  let root = tree(ctx)?
  let original = metrics.Report(schema: 1, sources: [], metrics: [
    {name: "blanket_dead_code_lines", value: 7, evidence: [{path: "src/lib.rs", line: 1, detail: "old blanket scope"}]},
    {name: "manual_payload_calls", value: 1, evidence: []},
    {name: "source_lines_src", value: 5, evidence: []},
    {name: "source_lines_xsht", value: 5, evidence: []},
  ], reviews: [])
  let corrected = metrics.Report(schema: original.schema, sources: original.sources, metrics: collect {
    for row in original.metrics { yield if row.name == "blanket_dead_code_lines" { {...row, value: 3, evidence: []} } else { row } }
  }, reviews: original.reviews)
  let current = metrics.Report(schema: corrected.schema, sources: corrected.sources, metrics: collect {
    for row in corrected.metrics { yield if row.name == "blanket_dead_code_lines" { {...row, value: 0} } else { row } }
  }, reviews: corrected.reviews)
  let current_path = fp"{root}/current.json"
  json.write(current_path, current)
  let provenance_path = install_blanket_baseline(root, original, corrected)?
  let baseline_path = fp"{root}/dev/consolidation/baseline-blanket-scope.json"
  let successor_path = fp"{root}/dev/consolidation/metric-observer-successor.json"
  let successor = json.read(successor_path)?.require(metrics.ObserverSuccessorProvenance)?
  match metrics.verify_observer_successor(root, baseline_path.display(), provenance_path.display(), corrected) {
    Ok(_) => {}
    Err(metrics.MetricError.Contract {detail}) => test.fail(detail)
    Err(error) => test.fail(error.message)
  }
  json.write(successor_path, {...successor, baseline_ratchet_tests_sha256: successor.current_ratchet_tests_sha256})
  match metrics.verify_observer_successor(root, baseline_path.display(), provenance_path.display(), corrected) {
    Ok(_) => test.fail("source advisory ratchet hash was replaced by the current test hash")
    Err(metrics.MetricError.Contract {detail}) => assert "source advisory ratchet attestation" in detail
    Err(error) => test.fail(error.message)
  }
  json.write(successor_path, successor)
  json.write(successor_path, {...successor, current_ratchet_tests_sha256: successor.baseline_ratchet_tests_sha256})
  match metrics.verify_observer_successor(root, baseline_path.display(), provenance_path.display(), corrected) {
    Ok(_) => test.fail("current ratchet hash was replaced by the source advisory hash")
    Err(metrics.MetricError.Contract {detail}) => assert "ratchet fixture hash mismatch" in detail
    Err(error) => test.fail(error.message)
  }
  json.write(successor_path, successor)
  let driver = p"dev/consolidation/metrics.xsh".read_text()?
  let flags = ["--root", root.display(), "--source-root", "src", "--xsht-source-root", "crates/xsht/src", "--input", current_path.display(), "--baseline", baseline_path.display(), "--output", fp"{root}/saved.json".display(), "--audit", fp"{root}/audit.json".display()]
  assert ! test.run_script(ctx, driver, ["check", @flags])?.success
  let valid_provenance = json.read(provenance_path)?.require(metrics.BlanketBaselineProvenance)?
  assert test.run_script(ctx, driver, ["check", @flags, "--baseline-provenance", provenance_path])?.success

  let unrelated_change = metrics.Report(schema: corrected.schema, sources: corrected.sources, metrics: collect {
    for row in corrected.metrics { yield if row.name == "manual_payload_calls" { {...row, value: 9} } else { row } }
  }, reviews: corrected.reviews)
  json.write(baseline_path, unrelated_change)
  json.write(provenance_path, {...valid_provenance, corrected_baseline_sha256: hash.sha256(baseline_path)?.hex().lower()})
  assert ! test.run_script(ctx, driver, ["check", @flags, "--baseline-provenance", provenance_path])?.success

  json.write(baseline_path, corrected)
  json.write(provenance_path, valid_provenance)
  fp"{root}/dev/consolidation_metrics.xsh".write("tampered scanner fixture\n")
  assert ! test.run_script(ctx, driver, ["check", @flags, "--baseline-provenance", provenance_path])?.success
}

type ReviewFixture = {root: Path, commit: Str, sources: List[metrics.ReviewSource], artifact: metrics.ReviewArtifact}

proc git_output(root: Path, arguments: List[Str]) [process, error] -> Str {
  let argv = ["git", "-C", root.display(), @arguments]
  let output = run.capture --text --timeout=30s @argv
  assert output.status.shell_code()? == 0
  output.stdout.trim()
}

proc review_fixture(ctx: TestContext) [fs, process, error] -> Result[ReviewFixture] {
  let root = test.temp_dir(ctx, name: "review-evidence")?
  for source_path in review_directory_paths {
    fp"{root}/{source_path}".mkdir()
    fp"{root}/{source_path}/fixture.txt".write(f"fixture for {source_path}\n")
  }
  fp"{root}/src/lib.rs".write("fn fixture() {}\n")
  fp"{root}/crates/xsht/src".mkdir()
  fp"{root}/crates/xsht/src/lib.rs".write("// xsht fixture\n")
  fp"{root}/dev/consolidation".mkdir()
  fp"{root}/dev/consolidation_metrics.xsh".write("# metrics fixture\n")
  fp"{root}/dev/consolidation_metrics_current.xsh".write("# current metrics fixture\n")
  fp"{root}/dev/consolidation/metrics.xsh".write("# driver fixture\n")
  fp"{root}/.work".mkdir()
  fp"{root}/.work/proofs".mkdir()
  let _ = git_output(root, ["init", "--quiet"])
  let _ = git_output(root, ["add", "."])
  let _ = git_output(root, ["-c", "user.name=Fixture Agent Reviewer", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "source fixture"])
  let commit = git_output(root, ["rev-parse", "HEAD"])
  let sources = collect {
    for source_path in review_source_paths {
      yield {path: source_path, oid: git_output(root, ["rev-parse", f"{commit}:{source_path}"])}
    }
  }
  let artifact_path = ".work/proofs/review.txt"
  fp"{root}/{artifact_path}".write("synthetic review evidence\n")
  let artifact = metrics.ReviewArtifact(
    path: artifact_path,
    sha256: hash.sha256(fp"{root}/{artifact_path}")?.hex(),
    command: "xsht test dev/tests/test-consolidation-ratchets-current.xsh",
    method: "native fixture assertion",
    result: "verified",
  )
  {root: root, commit: commit, sources: sources, artifact: artifact}
}

pure close_report(src_lines: Int, xsht_lines: Int, blanket_lines: Int) -> metrics.Report {
  {
    schema: 1,
    sources: [],
    metrics: [
      {name: "source_lines_src", value: src_lines, evidence: []},
      {name: "source_lines_xsht", value: xsht_lines, evidence: []},
      {name: "manual_payload_calls", value: 0, evidence: []},
      {name: "optional_raw_calls", value: 0, evidence: []},
      {name: "wide_arena_wildcards", value: 0, evidence: []},
      {name: "blanket_dead_code_lines", value: blanket_lines, evidence: []},
    ],
    reviews: [],
  }
}

proc final_close_failure(current: metrics.Report, baseline: metrics.Report) [error] -> Str {
  match metrics.assert_final_close(current, baseline) {
    Ok(_) => "accepted"
    Err(metrics.MetricError.Contract {detail}) => detail
    Err(metrics.MetricError.Rise {name, ..}) => name
    Err(error) => error.message
  }
}

pure report_metric(report: metrics.Report, name: Str) -> Int {
  (report.metrics |> where .name == name |> collect)[0].value
}

pure with_metric(report: metrics.Report, name: Str, amount: Int) -> metrics.Report {
  {...report, metrics: collect { for row in report.metrics { yield if row.name == name { {...row, value: amount} } else { row } } }}
}

pure proof_bundle(report: metrics.Report, fixture: ReviewFixture) -> metrics.ReviewEvidenceBundle {
  {
    schema: 1,
    source_commit: fixture.commit,
    sources: fixture.sources,
    rows: collect {
      for review in report.reviews {
        yield {
          name: review.name,
          status: "review_verified",
          reviewer: "fixture agent reviewer",
          evidence: [fixture.artifact],
        }
      }
    },
  }
}

proc review_failure(report: metrics.Report, bundle: metrics.ReviewEvidenceBundle, root: Path) [fs, process, error] -> Str {
  match metrics.verify_review_evidence(report, bundle, root) {
    Ok(_) => "accepted"
    Err(metrics.MetricError.Contract {detail}) => detail
    Err(error) => error.message
  }
}

test test_final_close_requires_structural_zeros_and_keeps_source_totals_advisory {
  let baseline = close_report(10, 8, 4)
  let source_equal = close_report(10, 8, 0)
  let source_growth = close_report(11, 9, 0)
  let closed = close_report(9, 7, 0)
  assert final_close_failure(source_equal, baseline) == "accepted"
  assert final_close_failure(source_growth, baseline) == "accepted"
  for name in ["manual_payload_calls", "optional_raw_calls", "wide_arena_wildcards", "blanket_dead_code_lines"] {
    let current = with_metric(closed, name, 1)
    let prior = with_metric(baseline, name, 2)
    assert final_close_failure(current, prior) == f"final close requires zero {name}"
  }
  assert final_close_failure({...closed, metrics: closed.metrics |> where .name != "manual_payload_calls" |> collect}, baseline) == "expected one manual_payload_calls metric"
  assert "manual_payload_calls" in final_close_failure({...closed, metrics: closed.metrics + [closed.metrics[2]]}, baseline)
  assert final_close_failure(closed, {...baseline, metrics: baseline.metrics |> where .name != "manual_payload_calls" |> collect}) == "expected one manual_payload_calls metric"
  assert "manual_payload_calls" in final_close_failure(closed, {...baseline, metrics: baseline.metrics + [baseline.metrics[2]]})
  metrics.assert_final_close(closed, baseline)
}

test test_review_evidence_requires_all_unique_verified_rows_and_hashed_workspace_artifacts { |ctx|
  let fixture = review_fixture(ctx)?
  let report = metrics.measure(fixture.root, ["src", "crates/xsht/src"])?
  let valid = proof_bundle(report, fixture)
  assert review_failure(report, valid, fixture.root) == "accepted"
  let missing = {...valid, rows: valid.rows |> where .name != "timings" |> collect}
  assert review_failure(report, missing, fixture.root) != "accepted"
  let duplicate = {...valid, rows: valid.rows + [valid.rows[0]]}
  assert review_failure(report, duplicate, fixture.root) != "accepted"
  let unresolved = {...valid, rows: collect { for row in valid.rows { yield if row.name == "timings" { {...row, status: "open"} } else { row } } }}
  assert review_failure(report, unresolved, fixture.root) != "accepted"
  let human_only_status = {...valid, rows: collect { for row in valid.rows { yield if row.name == "timings" { {...row, status: "human_verified"} } else { row } } }}
  assert review_failure(report, human_only_status, fixture.root) != "accepted"
  let failed_result = {...valid, rows: collect { for row in valid.rows { yield if row.name == "timings" { {...row, evidence: [{...row.evidence[0], result: "failed"}]} } else { row } } }}
  assert review_failure(report, failed_result, fixture.root) != "accepted"
  let bad_hash = {...valid, rows: collect { for row in valid.rows { yield if row.name == "timings" { {...row, evidence: [{...row.evidence[0], sha256: "0000000000000000000000000000000000000000000000000000000000000000"}]} } else { row } } }}
  assert review_failure(report, bad_hash, fixture.root) != "accepted"
  let unreadable = {...valid, rows: collect { for row in valid.rows { yield if row.name == "timings" { {...row, evidence: [{...row.evidence[0], path: ".work/proofs/missing.txt"}]} } else { row } } }}
  assert review_failure(report, unreadable, fixture.root) != "accepted"
  let outside = {...valid, rows: collect { for row in valid.rows { yield if row.name == "timings" { {...row, evidence: [{...row.evidence[0], path: "../outside.txt"}]} } else { row } } }}
  assert review_failure(report, outside, fixture.root) != "accepted"
  let nonhex = {...valid, source_commit: "not-a-commit"}
  assert review_failure(report, nonhex, fixture.root) != "accepted"
  let stale_sources = {...valid, sources: collect { for source in valid.sources { yield if source.path == "src" { {...source, oid: "0000000000000000000000000000000000000000"} } else { source } } }}
  assert review_failure(report, stale_sources, fixture.root) != "accepted"
}

test test_review_evidence_accepts_synthetic_commit_after_docs_only_commit { |ctx|
  let fixture = review_fixture(ctx)?
  let report = metrics.measure(fixture.root, ["src", "crates/xsht/src"])?
  let proof = proof_bundle(report, fixture)
  fp"{fixture.root}/README.md".write("docs-only update\n")
  let _ = git_output(fixture.root, ["add", "README.md"])
  let _ = git_output(fixture.root, ["-c", "user.name=Fixture Agent Reviewer", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "documentation only"])
  assert review_failure(report, proof, fixture.root) == "accepted"
  fp"{fixture.root}/src/lib.rs".write("fn changed_fixture() {}\n")
  let _ = git_output(fixture.root, ["add", "src/lib.rs"])
  let _ = git_output(fixture.root, ["-c", "user.name=Fixture Agent Reviewer", "-c", "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "changed source"])
  assert review_failure(report, proof, fixture.root) != "accepted"
}

test test_final_close_cli_keeps_source_totals_advisory_and_preserves_final_gates { |ctx|
  let fixture = review_fixture(ctx)?
  let report = metrics.measure(fixture.root, ["src", "crates/xsht/src"])?
  let proof = proof_bundle(report, fixture)
  let baseline = metrics.Report(
    schema: report.schema,
    sources: report.sources,
    metrics: collect {
      for row in report.metrics {
        yield if row.name in ["source_lines_src", "source_lines_xsht", "blanket_dead_code_lines"] { {...row, value: row.value + 1} } else { row }
      }
    },
    reviews: report.reviews,
  )
  let driver = p"dev/consolidation/metrics.xsh".read_text()?
  let proof_path = fp"{fixture.root}/.work/proofs/bundle.json"
  json.write(proof_path, proof)
  json.write(fp"{fixture.root}/current.json", report)
  json.write(fp"{fixture.root}/baseline.json", baseline)
  fp"{fixture.root}/dev/consolidation/baseline-metadata.json".write("{\"start_commit\":\"fixture\"}")
  let flags = ["--root", fixture.root.display(), "--source-root", "src", "--xsht-source-root", "crates/xsht/src", "--input", fp"{fixture.root}/current.json".display(), "--baseline", fp"{fixture.root}/baseline.json".display(), "--output", fp"{fixture.root}/saved.json".display(), "--audit", fp"{fixture.root}/audit.json".display()]
  assert ! test.run_script(ctx, driver, ["final-close", @flags])?.success
  assert test.run_script(ctx, driver, ["final-close", @flags, "--review-evidence", proof_path])?.success
  let unresolved = {...proof, rows: collect { for row in proof.rows { yield if row.name == "timings" { {...row, status: "open"} } else { row } } }}
  json.write(proof_path, unresolved)
  assert ! test.run_script(ctx, driver, ["final-close", @flags, "--review-evidence", proof_path])?.success
  json.write(proof_path, proof)
  for name in ["manual_payload_calls", "optional_raw_calls", "wide_arena_wildcards", "blanket_dead_code_lines"] {
    json.write(fp"{fixture.root}/current.json", with_metric(report, name, 1))
    json.write(fp"{fixture.root}/baseline.json", with_metric(baseline, name, 2))
    assert ! test.run_script(ctx, driver, ["final-close", @flags, "--review-evidence", proof_path])?.success
  }
  json.write(fp"{fixture.root}/current.json", report)
  json.write(fp"{fixture.root}/baseline.json", baseline)
  let equal_src = metrics.Report(
    schema: report.schema,
    sources: report.sources,
    metrics: collect {
      for row in report.metrics {
        yield if row.name == "source_lines_src" { {...row, value: report_metric(baseline, row.name)} } else { row }
      }
    },
    reviews: report.reviews,
  )
  json.write(fp"{fixture.root}/current.json", equal_src)
  json.write(proof_path, proof)
  assert test.run_script(ctx, driver, ["final-close", @flags, "--review-evidence", proof_path])?.success
  let source_growth = metrics.Report(
    schema: report.schema,
    sources: report.sources,
    metrics: collect {
      for row in report.metrics {
        yield if row.name == "source_lines_src" { {...row, value: report_metric(baseline, row.name) + 1} } else { row }
      }
    },
    reviews: report.reviews,
  )
  json.write(fp"{fixture.root}/current.json", source_growth)
  assert test.run_script(ctx, driver, ["final-close", @flags, "--review-evidence", proof_path])?.success
  let saved: metrics.Report = json.read(fp"{fixture.root}/saved.json")?.require()?
  assert report_metric(saved, "source_lines_src") == report_metric(source_growth, "source_lines_src")
  let rises = json.get(json.read(fp"{fixture.root}/audit.json")?, ["rises"])?.require(List[AuditRise])?
  assert rises.len() == 1
  assert rises[0].metric == "source_lines_src"
  assert rises[0].baseline == report_metric(baseline, "source_lines_src")
  assert rises[0].current == report_metric(source_growth, "source_lines_src")
  let blanket_remaining = metrics.Report(
    schema: source_growth.schema,
    sources: source_growth.sources,
    metrics: collect {
      for row in source_growth.metrics {
        yield if row.name == "blanket_dead_code_lines" { {...row, value: 1} } else { row }
      }
    },
    reviews: source_growth.reviews,
  )
  json.write(fp"{fixture.root}/current.json", blanket_remaining)
  assert ! test.run_script(ctx, driver, ["final-close", @flags, "--review-evidence", proof_path])?.success
}
