type IRReasonCount = {reason: Str, count: Int}

type IRReasonGroup = {group: Str, total: Int, reasons: List[IRReasonCount]}

type IRPureScan = {path: Str, line: Int, name: Str, lowerable: Bool, reasons: List[Str]}

type IRProcScan = {path: Str, line: Int, name: Str, effects: List[Str], lowerable: Bool, reasons: List[Str]}

type IRScriptScan = {path: Str, line: Int, shape: Str, lowerable: Bool, reasons: List[Str]}

type IRCorpusReport[T] = {
  roots: List[Str],
  total: Int,
  lowerable: Int,
  percent: Int,
  reasons: List[IRReasonCount],
  groups: List[IRReasonGroup],
  samples: List[T],
}

type IRRow = {name: Str, covered: Int, total: Int, percent: Int, supported: List[Str], unsupported: List[Str]}

type IRLoweredCounts = {statements: Int, expressions: Int, pipeline_stages: Int, types: Int}

type IRWireReport = {
  rows: List[IRRow],
  lowered_nodes: IRLoweredCounts,
  lowered_methods: List[Str],
  corpus: IRCorpusReport[IRPureScan],
  procs: IRCorpusReport[IRProcScan],
  script: IRCorpusReport[IRScriptScan],
}

test test_ir_coverage_cli_retains_typed_report_and_scan_counts { |ctx|
  let repo = fs.cwd()?
  let root = test.temp_dir(ctx, name: "ir-coverage-wire")?.resolve()?
  fp"{root}/src/syntax".mkdir()
  fp"{root}/src/sema".mkdir()
  fp"{root}/src/sema/records.rs".write_atomic("")
  fp"{root}/src/runtime/eval/indexed".mkdir()
  fp"{root}/core".mkdir()
  fp"{root}/src/syntax/arena.rs".write_atomic("""
pub enum ArenaStmtKind {
    Let,
    Command,
}
pub enum ArenaExprKind {
    Int,
}
pub enum ArenaTypeExprTag {
    Named,
}
""")
  fp"{root}/src/syntax/node.rs".write_atomic("""
pub enum BinaryOp {
    Add,
}
pub enum AssignOp {
    Set,
}
""")
  fp"{root}/src/runtime/eval.rs".write_atomic("""
pub enum LoweredPipelineStage {
    Map,
}
pub enum LoweredType {
    Int,
}
const LOWERED_METHOD_NAMES: &[&str] = &[
    "len",
];
""")
  fp"{root}/src/runtime/eval/indexed/full.rs".write_atomic("""
pub enum FullTag {
    StmtLet,
    ExprInt,
}
""")
  fp"{root}/core/sample.xsh".write_atomic("""
pure identity(value: Int) -> Int { value }
proc count() [] -> Int { 1 }
let value = identity(1)
let values = [1, 2] |> batch(count: 1)
""")
  let report_path = fp"{root}/reports/ir.json"
  let stdout_path = fp"{root}/stdout.txt"
  let stderr_path = fp"{root}/stderr.txt"
  let xsh = ctx.xsh_bin
  let tool = fp"{repo}/tools/xsh-ir-coverage.xsh"
  let command = process.command {
    stdout = stdout_path
    stderr = stderr_path
    run $xsh $tool -- --root $root --json $report_path
  }
  let status = process.run(command)?
  let succeeded = status.exited_with(0)
  let diagnostic = stderr_path.read_text()?
  assert succeeded, diagnostic
  let report = json.read(report_path)?.require(IRWireReport)?
  assert report.rows.len() == 5
  assert report.rows[0].total == 2
  assert report.rows[0].unsupported == ["Command"]
  assert report.lowered_nodes == {statements: 1, expressions: 1, pipeline_stages: 1, types: 1}
  assert report.lowered_methods == ["len"]
  assert report.corpus.total == 1
  assert report.corpus.lowerable == 1
  assert report.procs.total == 1
  assert report.procs.lowerable == 1
  assert report.script.total == 2
  assert report.script.lowerable == 1
  assert report.script.reasons == [{reason: "expr.pipeline", count: 1}]
  assert report.script.groups[0].group == "expression"
  assert report.script.samples[0].shape == "Let"
  assert "lowered IR coverage" in stdout_path.read_text()?
  let invalid = process.command {
    stdout = stdout_path
    stderr = stderr_path
    run $xsh $tool -- --root fp"{root}/absent"
  }
  assert ! process.run(invalid)?.exited_with(0)
  assert stderr_path.read_text()? != ""
}

test test_ir_coverage_report_validation_rejects_incomplete_wire_data {
  let incomplete = json.decode(r"""{"rows": []}""")?
  test.error_kind(incomplete.require(IRWireReport), "schema")
}
