use consolidation_metrics_current as metrics

pure value(scan: metrics.SourceScan, name: Str) -> Int {
  let rows = scan.metrics |> where .name == name |> collect
  rows[0].value
}

pure report_value(report: metrics.Report, name: Str) -> Int {
  let rows = report.metrics |> where .name == name |> collect
  rows[0].value
}

pure pattern_table() -> Str {
  [
    "macro_rules! pattern_schema { () => { enum BuildPatternRow { TemplateRow } }; }",
    "pattern_schema! {",
    "enum FullPatternTag {",
    "Alias = 0 { pattern: BuildPatternId, slot: usize, } [Normal, CaptureSlots],",
    "Alternation = 1 { patterns: Vec<BuildPatternId>, } [Normal, Alternation],",
    "List = 2 { elements: Vec<BuildPatternId>, rest: Option<BuildPatternId>, } [Normal, ListRest],",
    "TagType = 3 { type_name: Name, variants: Vec<Name>, } [Normal, None],",
    "RecordTest = 4 { fields: Vec<(Name, BuildPatternId)> => Box<Vec<(Name, BuildPatternId)>>, } [Normal, None],",
    "ResultTest = 5 { ok: bool, inner: BuildPatternId, } [Normal, None],",
    "TagTest = 6 { type_name: Name, name: Name, fields: Vec<BuildPatternId>, } [Normal, None],",
    "ErrorTest = 7 { family: Name, variant: Name, fields: Vec<(Name, BuildPatternId)> => Box<Vec<(Name, BuildPatternId)>>, } [Normal, None],",
    "Wildcard = 8 [Normal, None],",
    "Bind = 9 { slot: usize, } [Normal, None],",
    "Type = 10 { ty: Type, slot: Option<usize>, } [Normal, None],",
    "Literal = 11 (value: LoweredValue) [Normal, None],",
    "ResultOk = 12 { slot: Option<usize>, unit_only: bool, } [Normal, None],",
    "ResultErr = 13 { slot: Option<usize>, unit_only: bool, } [Normal, None],",
    "Facet = 15 { facet: Name, result_wrapped: bool, } [Normal, None],",
    "Tag = 16 { type_name: Name, name: Name, slots: BuildPatternIdSlots, } [Normal, None],",
    "Text = 17 { holes: Vec<BuildPatternId>, kinds: Vec<TextHoleKind>, segments: Vec<Arc<str>>, } [Text, None],",
    "}",
    "}",
  ].join("\n")
}

pure instruction_schema_source() -> Str {
  [
    "macro_rules! instruction_schema {",
    "  (@eager_atom) => { false };",
    "  (@eager_atom EagerAtom) => { true };",
    "  ($($row:ident => $view:ident [$kind:ident] {$($variant:ident => $tag:ident = $ordinal:literal [$effects:expr; $class:ident; $strategy:ident $(; $classification:ident)?];)*})*) => {",
    "    match self { $($(Self::$tag => instruction_schema!(@eager_atom $($classification)?),)*)* }",
    "  };",
    "}",
    "instruction_schema! {",
    "  BuildIntRow => IntView [Int] { Int => IntInt = 0 [0; Operands; FrameApply]; }",
    "  BuildBoolRow => BoolView [Bool] { Bool => BoolBool = 1 [0; Operands; FrameApply]; }",
    "  BuildExprRow => ExprView [Expr] { Null => ExprNull = 2 [0; Operands; FrameApply; EagerAtom]; }",
    "  BuildStmtRow => StmtView [Stmt] { Break => StmtBreak = 3 [0; Operands; FrameApply]; }",
    "}",
  ].join("\n")
}

proc rejection_at(source_path: Str, source: Str) [error] -> Str {
  match metrics.scan_file(source_path, source) {
    Ok(_) => "accepted"
    Err(metrics.MetricError.Invalid {detail, ..}) => detail
    Err(error) => error.message
  }
}

proc tree(ctx: TestContext) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name: "pattern-authority")?
  fp"{root}/src/runtime/eval/indexed/full".mkdir()
  fp"{root}/src/patterns.rs".write("")
  fp"{root}/crates/xsht/src".mkdir()
  fp"{root}/crates/xsht/src/lib.rs".write("// tool root\n")
  root
}

test test_generated_pattern_schema_counts_seventeen_rows_and_keeps_the_ordinal_hole {
  let scan = metrics.scan_file("src/patterns.rs", pattern_table())?
  let rows = scan.metrics |> where .name == "pattern_variants" |> collect
  assert rows.len() == 1
  assert rows[0].value == 17
  assert rows[0].evidence.len() == 17
  assert (rows[0].evidence |> where .path != "src/patterns.rs" |> count()) == 0
  assert (rows[0].evidence |> where .detail == "pattern_schema::Facet" |> count()) == 1
  assert (rows[0].evidence |> where .detail == "pattern_schema::ErrorVariant" |> count()) == 0
}

test test_current_metrics_cli_measures_generated_pattern_rows { |ctx|
  let root = tree(ctx)?
  fp"{root}/src/lib.rs".write("#[path = \"patterns.rs\"] mod patterns;\n")
  fp"{root}/src/patterns.rs".write(pattern_table())
  let output_path = fp"{root}/current.json"
  let driver = p"dev/consolidation/metrics.xsh".read_text()?
  test.expect(
    ctx,
    driver,
    status: 0,
    args: [
      "measure",
      "--root",
      root,
      "--source-root",
      "src",
      "--xsht-source-root",
      "crates/xsht/src",
      "--output",
      output_path,
    ],
  )?
  let report: metrics.Report = json.read(output_path)?.require(metrics.Report)?
  assert report_value(report, "pattern_variants") == 17
}

test test_legacy_pattern_row_enum_remains_a_frozen_baseline_fallback {
  let scan = metrics.scan_file("src/runtime/eval.rs", "enum BuildPatternRow { Wildcard, ErrorVariant }\n")?
  assert value(scan, "pattern_variants") == 2
  let rows = scan.metrics |> where .name == "pattern_variants" |> collect
  let details = rows[0].evidence |> map .detail |> sort
  assert details == ["BuildPatternRow::ErrorVariant", "BuildPatternRow::Wildcard"]
}

test test_pattern_schema_rejects_malformed_duplicate_and_unrecognized_rows {
  assert "malformed pattern schema entry" in rejection_at("src/patterns.rs", pattern_table().replace("ResultOk = 12", with: "ResultOk 12"))
  assert "duplicate pattern schema variant" in rejection_at("src/patterns.rs", pattern_table().replace("Tag = 16", with: "Facet = 16"))
  assert "duplicate pattern schema ordinal" in rejection_at("src/patterns.rs", pattern_table().replace("Facet = 15", with: "Facet = 13"))
  assert "unrecognized pattern schema" in rejection_at("src/patterns.rs", pattern_table().replace("FullPatternTag", with: "OtherPatternTag"))
}

test test_pattern_schema_table_and_legacy_enum_cannot_double_count_across_modules { |ctx|
  let root = tree(ctx)?
  fp"{root}/src/lib.rs".write("mod runtime;\n#[path = \"patterns.rs\"] mod patterns;\n")
  fp"{root}/src/runtime.rs".write("mod eval;\n")
  fp"{root}/src/runtime/eval.rs".write("enum BuildPatternRow { Wildcard }\n")
  fp"{root}/src/patterns.rs".write(pattern_table())
  match metrics.measure(root, ["src", "crates/xsht/src"]) {
    Ok(_) => test.fail("generated and legacy pattern authorities were both counted")
    Err(metrics.MetricError.Invalid {detail, ..}) => assert "pattern schema overlaps the handwritten BuildPatternRow enum" in detail
    Err(error) => test.fail(error.message)
  }
}

test test_canonical_observer_provenance_pins_the_frozen_same_source_report {
  let baseline_path = p"dev/consolidation/baseline-blanket-scope.json"
  let provenance_path = p"dev/consolidation/blanket-metric-provenance-source-advisory.json"
  let baseline: metrics.Report = json.read(baseline_path)?.require(metrics.Report)?
  match metrics.verify_observer_successor(p".", baseline_path.display(), provenance_path.display(), baseline) {
    Ok(_) => {}
    Err(metrics.MetricError.Contract {detail}) => test.fail(detail)
    Err(error) => test.fail(error.message)
  }
}

test test_instruction_schema_macro_accepts_its_eager_atom_expansion {
  let source = instruction_schema_source()
  let scan = metrics.scan_file("src/runtime/eval/indexed/full/schema.rs", source)?
  assert value(scan, "instruction_variants") == 4
  assert "outside its generated source authority" in rejection_at("src/fixture.rs", source)
}

test test_current_instruction_schema_preserves_retired_wire_ordinal_gaps {
  let source_path = "src/runtime/eval/indexed/full/schema.rs"
  let source = p"src/runtime/eval/indexed/full/schema.rs".read_text()?
  let scan = metrics.scan_file(source_path, source)?
  let rows = scan.metrics |> where .name == "instruction_variants" |> collect
  assert rows.len() == 1
  assert rows[0].value == 152
  assert rows[0].evidence.len() == 152
}

test test_instruction_schema_eager_classification_and_table_shape_fail_loudly {
  let source = instruction_schema_source()
  let source_path = "src/runtime/eval/indexed/full/schema.rs"
  assert "unrecognized instruction schema eager helper expansion" in rejection_at(source_path, source.replace("@eager_atom $($classification)?", with: "@eager_atom EagerAtom"))
  assert "unrecognized instruction schema classification" in rejection_at(source_path, source.replace("FrameApply; EagerAtom", with: "FrameApply; OtherAtom"))
  assert "nested in an unrelated macro definition" in rejection_at("src/fixture.rs", "macro_rules! unrelated { () => { instruction_schema!(@eager_atom EagerAtom) }; }")
  assert "unrecognized instruction schema eager helper expansion" in rejection_at(source_path, source.replace("match self { $($(Self::$tag => instruction_schema!(@eager_atom $($classification)?),)*)* }", with: "match self { $($(Self::$tag => instruction_schema!(@eager_atom $($classification)?), instruction_schema!(@eager_atom EagerAtom))*)* }"))
  assert "duplicate instruction schema tag" in rejection_at(source_path, source.replace("BoolBool", with: "IntInt"))
  assert "duplicate instruction schema variant" in rejection_at(source_path, source.replace("Bool => BoolBool = 1 [0; Operands; FrameApply];", with: "Bool => BoolBool = 1 [0; Operands; FrameApply]; Bool => BoolAgain = 2 [0; Operands; FrameApply];"))
  assert "duplicate instruction schema ordinal" in rejection_at(source_path, source.replace("= 1 [0; Operands; FrameApply]", with: "= 0 [0; Operands; FrameApply]"))
  assert "outside the u8 range" in rejection_at(source_path, source.replace("= 3 [0; Operands; FrameApply]", with: "= 256 [0; Operands; FrameApply]"))
  assert "negative instruction schema ordinal" in rejection_at(source_path, source.replace("= 3 [0; Operands; FrameApply]", with: "= -1 [0; Operands; FrameApply]"))
  assert "not strictly increasing" in rejection_at(source_path, source.replace("= 1 [0; Operands; FrameApply]", with: "= 3 [0; Operands; FrameApply]"))
  assert "malformed instruction schema ordinal" in rejection_at(source_path, source.replace("= 3 [0; Operands; FrameApply]", with: "= unknown [0; Operands; FrameApply]"))
  assert "malformed instruction schema header" in rejection_at(source_path, source.replace("BuildBoolRow => BoolView [Bool]", with: "BuildBoolRow => OtherView [Bool]"))
  assert "incomplete instruction schema table set" in rejection_at(source_path, source.replace("  BuildStmtRow => StmtView [Stmt] { Break => StmtBreak = 3 [0; Operands; FrameApply]; }\n", with: ""))
}
