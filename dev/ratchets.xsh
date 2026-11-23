##! Checks the structural source ratchets in `dev/ratchets.json`. `check` fails when a structural metric of the
##! production source differs from its recorded value; `update` records lowered values and refuses a rise.
use source_metrics as metrics

type Options = {action: Str, root: Str, report: Str}

proc execute(options: Options) [fs, error, io] -> Result[Unit] {
  guard options.action in ["check", "update"] else { fail "expected check or update" }
  let root = fp"{options.root}"
  let report = metrics.measure(root, metrics.RATCHET_SOURCE_ROOTS)?
  if options.report != "" {
    let report_path = fp"{options.report}"
    if ! report_path.parent().exists() { report_path.parent().mkdir(parents: true) }
    json.write(report_path, report)
  }
  metrics.assert_source_contract(report)
  let baseline_path = fp"{root}/{metrics.RATCHET_BASELINE}"
  let baseline = json.read(baseline_path)?.require(metrics.Baseline)?
  if options.action == "update" {
    let updated = metrics.lowered(report, baseline)?
    baseline_path.write(json.encode(updated, pretty: true)? + "\n")
    print f"recorded {updated.metrics.len()} structural metrics in {metrics.RATCHET_BASELINE}"
    return
  }
  metrics.verify(report, baseline)
  let advisory = collect { for row in report.metrics { yield f"{row.name} {row.value}" when row.name in metrics.ADVISORY_METRICS } }
  print f"{baseline.metrics.len()} structural metrics match {metrics.RATCHET_BASELINE}; advisory {advisory.join(", ")}"
}

proc main(...argv: List[Str]) [fs, error, io] {
  let options: Options = cli.parse(argv, {
    action: {positional: true, form: "check|update"},
    root: {form: "--root PATH", default: "."},
    report: {form: "--report PATH", default: ""},
  })?
  match execute(options) {
    Ok(_) => {}
    Err(metrics.MetricError.Invalid {path: source_path, line, detail}) => { eprint f"{source_path}:{line}: {detail}"; exit 1 }
    Err(metrics.MetricError.Rise {name, before, after}) => { eprint f"ratchet rise {name}: {before} -> {after}"; exit 1 }
    Err(metrics.MetricError.Contract {detail}) => { eprint $detail; exit 1 }
    Err(error) => { eprint $error.message; exit 1 }
  }
}
