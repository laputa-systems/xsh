error ScriptError = Failed(kind: Str, message: Str)

type Suite = {name: Str, path: Path}

type SuiteInput = {name: Str, path: Str}

type CoverageHits = {tests: Int, examples: Int}

type CoverageTotal = {covered: Int, total: Int}

# API identifiers are JSON object keys; their counter records are checked individually.
type SuiteCoverageReport = {standard_apis: List[Str], api_hits: Record}

type CoverageTotalRow = {group: Str, covered: Int, total: Int}

type CoveredApiRow = {api_id: Str, tests: Int, examples: Int, total: Int}

type CoverageReport = {
  suites: List[SuiteInput],
  api_hits: Map[CoverageHits],
  standard_apis: List[Str],
  totals: List[CoverageTotalRow],
  uncovered: List[Str],
  covered: List[CoveredApiRow],
}

pure repo_path(root: Path, value: Str) -> Path {
  return fp"${value}" when value.starts_with("/")

  fp"${root}/${value}"
}

proc env_path(root: Path, name: Str, default: Path) [env] -> Path {
  let value = (env.get(name) ?? "").trim()

  return default when value == ""

  repo_path(root, value)
}

proc xsht_path(root: Path) [env] -> Path {
  let configured = (env.get("XSHT") ?? "").trim()

  return repo_path(root, configured) when configured != ""

  fp"${root}/target/release/xsht"
}

proc relative_display(root: Path, target: Path) [error] -> Result[Str] {
  target.strip_prefix(root)?.display()
}

pure suite_json_name(name: Str) -> Str {
  return "root.json" when name == "."

  f"${name.replace("/", "__")}.json"
}

pure suite_test_args(name: Str, suite_json: Path) -> List[Str] {
  if name == "." {
    return ["test", "--cov", "--api", "--cov-json", suite_json.display(), "tests/xsh"]
  }

  ["test", "--cov", "--api", "--cov-json", suite_json.display()]
}

proc discover_suites(root: Path) [fs, error] -> Result[List[Suite]] {
  var suites: List[Suite] = [{name: ".", path: root}]
  var seen = {[root.display()]: true}
  let core = fp"${root}/core"

  if fp"${core}/tests".exists()? {
    seen[core.display()] = true
    suites = suites.push({name: "core", path: core})
  }

  let prototypes = fp"${root}/prototypes"

  if prototypes.exists()? {
    for entry in fs.walk(prototypes)?
      |> where .kind == "dir" and .name == "tests"
      |> sort-by .path {
      let parent = entry.path.parent()

      if ! (seen.get(parent.display()) ?? false) {
        seen[parent.display()] = true
        suites = suites.push({name: relative_display(root, parent)?, path: parent})
      }
    }
  }

  suites
}

proc run_suites(
  root: Path,
  suites: List[Suite],
  out_dir: Path,
  xsht: Path,
) [fs, process, env, error, io] -> Result[List[SuiteInput]] {
  out_dir.mkdir()?
  var outputs: List[SuiteInput] = []
  var failed = false

  for suite in suites {
    let suite_json = fp"${out_dir}/${suite_json_name(suite.name)}"
    print f"coverage suite ${suite.name}"

    cd suite.path {
      let captured = run.capture --text $xsht @(suite_test_args(suite.name, suite_json)) ?
      io.write_stdout(captured.stdout)?

      if captured.stderr != "" {
        io.write_stdout(captured.stderr)?
      }

      if ! captured.status.ok {
        failed = true
      }
    }

    if suite_json.exists()? {
      outputs = outputs.push({name: suite.name, path: relative_display(root, suite_json)?})
    }
  }

  if failed {
    return Err(ScriptError.Failed("coverage", "one or more coverage suites failed"))
  }

  outputs
}

proc merge_reports(root: Path, inputs: List[SuiteInput]) [fs, error] -> Result[CoverageReport] {
  var api_hits: Map[CoverageHits] = {}
  var standard_apis: Map[Bool] = {}

  for input in inputs {
    let data = json.read(repo_path(root, input.path))?.require(SuiteCoverageReport)?
    let standard = data.standard_apis

    for api_id in standard {
      standard_apis[api_id] = true
    }

    let raw_hits = data.api_hits

    for api_id in raw_hits.keys() {
      let raw = raw_hits.get(api_id)?.require(Record)?

      # Missing counters retain zero before their values are checked together.
      let hits = {
        tests: raw.get("tests") ?? 0,
        examples: raw.get("examples") ?? 0,
      }.require(CoverageHits)?
      let current = api_hits.get(api_id) ?? {tests: 0, examples: 0}
      api_hits[api_id] = {tests: current.tests + hits.tests, examples: current.examples + hits.examples}
    }
  }

  let sorted_standard = standard_apis.keys() |> sort
  var totals: Map[CoverageTotal] = {}

  for api_id in sorted_standard {
    let group_name = api_id.split(".").get(0) ?? "other"
    let current = totals.get(group_name) ?? {covered: 0, total: 0}
    let covered = if api_id in api_hits { 1 } else { 0 }
    totals[group_name] = {covered: current.covered + covered, total: current.total + 1}
  }

  let total_rows: List[CoverageTotalRow] = [
    {group: group_name, covered: totals.get(group_name)?.covered, total: totals.get(group_name)?.total}
    for group_name in totals.keys() |> sort
  ]

  let uncovered = sorted_standard
    |> where { |api|
      api not in api_hits
    }
  var covered_rows: List[CoveredApiRow] = []

  for api_id in api_hits.keys() |> sort {
    let hits = api_hits.get(api_id)?
    let total = hits.tests + hits.examples

    if total > 0 {
      covered_rows = covered_rows.push({api_id: api_id, tests: hits.tests, examples: hits.examples, total: total})
    }
  }

  {
    suites: inputs,
    api_hits: api_hits,
    standard_apis: sorted_standard,
    totals: total_rows,
    uncovered: uncovered,
    covered: covered_rows,
  }
}

proc render_text(report: CoverageReport) [error] -> Result[Str] {
  var lines = ["coverage report", "API coverage"]
  let totals = report.totals

  for row in totals {
    let {group: group_name, covered, total, ..} = row
    lines = lines.push(f"${group_name}: ${covered}/${total}")
  }

  lines += ["", "uncovered standard APIs"]
  let uncovered = report.uncovered

  if uncovered.len() == 0 {
    lines += ["  none"]
  } else {
    var count = 0

    for api_id in uncovered {
      if count < 80 {
        lines = lines.push(f"  ${api_id}")
      }

      count += 1
    }

    if uncovered.len() > 80 {
      lines += ["  ..."]
    }
  }

  lines += ["", "APIs covered by examples/tests"]
  let covered_rows = report.covered

  if covered_rows.len() == 0 {
    lines += ["  none"]
  } else {
    for row in covered_rows {
      let {api_id, total, ..} = row
      lines = lines.push(f"  ${api_id}: ${total}")
    }
  }

  lines += [""]
  lines.join("\n")
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let _ = argv
  let root = fs.cwd()?
  let out_dir = env_path(root, "XSH_COV_DIR", fp"${root}/target/xsh-cov")
  let json_path = env_path(root, "XSH_COV_JSON", fp"${out_dir}/coverage.json")
  let text_path = env_path(root, "XSH_COV_REPORT", fp"${out_dir}/coverage.txt")
  let suites = discover_suites(root)?
  let inputs = run_suites(root, suites, out_dir, xsht_path(root))?
  let report = merge_reports(root, inputs)?
  let report_text = render_text(report)?
  json_path.parent().mkdir()?
  text_path.parent().mkdir()?
  json.write(json_path, report)?
  fs.write(text_path, report_text)?
  io.write_stdout(report_text)?
  print f"coverage JSON: ${relative_display(root, json_path)?}"
  print f"coverage text: ${relative_display(root, text_path)?}"
}
