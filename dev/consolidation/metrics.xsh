#!/usr/bin/env -S xsh --
use consolidation_metrics as metrics

type Options = {action: Str, root: Path, source: Str, xsht_source: Str, output: Path, audit: Str, baseline: Str, baseline_provenance: Str, envelope: Str, input: Str, item_inventory: Str, review_evidence: Str}
type Limit = {metric: Str, ceiling: Int, approval: Str, reason: Str, remove_when: Str}
type Envelope = {start_commit: Str, limits: List[Limit]}
type Metadata = {start_commit: Str}
type Rise = {metric: Str, baseline: Int, current: Int, ceiling: Int, approval: Str, reason: Str, remove_when: Str}
type Audit = {schema: Int, mode: Str, envelope: Str, rises: List[Rise]}

# Keep both source totals in the report and rise audit, but omit only their values from ratchet comparisons.
const advisory_source_metrics = ["source_lines_src", "source_lines_xsht"]

# Validate the complete report before applying the source-budget exception.
proc validate_policy_report(report: metrics.Report) [error] -> Result[Unit, Error] {
  if report.schema != 1 { return Err(metrics.MetricError.Contract(detail: "unsupported consolidation metric schema")) }
  metrics.assert_not_risen(report, report)?
  for name in advisory_source_metrics {
    let rows = report.metrics |> where .name == name |> collect
    if rows.len() != 1 { return Err(metrics.MetricError.Contract(detail: f"expected exactly one {name} metric")) }
  }
}

# Preserve each source row in output and audit while excluding only these values from ratchets.
pure structural_report(report: metrics.Report) -> metrics.Report {
  {...report, metrics: collect { for row in report.metrics { if row.name not in advisory_source_metrics { yield row } } }}
}

proc assert_structural_not_risen(current: metrics.Report, baseline: metrics.Report) [error] -> Result[Unit, Error] {
  validate_policy_report(current)?
  validate_policy_report(baseline)?
  metrics.assert_not_risen(structural_report(current), structural_report(baseline))
}

# Final close still requires every blanket dead-code scope to be gone.
proc assert_final_close(current: metrics.Report, baseline: metrics.Report) [error] -> Result[Unit, Error] {
  assert_structural_not_risen(current, baseline)?
  let blanket = current.metrics |> where .name == "blanket_dead_code_lines" |> collect
  if blanket.len() != 1 { return Err(metrics.MetricError.Contract(detail: "expected one blanket_dead_code_lines metric")) }
  if blanket[0].value != 0 { return Err(metrics.MetricError.Contract(detail: "final close requires zero blanket_dead_code_lines")) }
}

# Envelope approvals annotate rises in the audit; structural acceptance remains tied to the baseline.
proc execute(o: Options) [fs, error, io] -> Result[Unit] {
  if o.action not in ["measure", "check", "final-close"] { fail "expected measure, check, or final-close" }
  if ([o.source, o.xsht_source] |> sort) != ["crates/xsht/src", "src"] { fail "explicit source roots must be src and crates/xsht/src" }
  let measured = if o.input == "" { metrics.measure_with_allowances(o.root, [o.source, o.xsht_source])? } else { if o.item_inventory != "" { fail "--item-inventory requires a source measurement" }; {report: json.read(fp"{o.input}")?.require(metrics.Report)?, item_allowances: {schema: 1, entries: []}} }
  let current = measured.report
  json.write(o.output, current)
  if o.item_inventory != "" { json.write(fp"{o.item_inventory}", measured.item_allowances) }
  assert_structural_not_risen(current, current)?
  if o.action == "measure" { return }
  if o.baseline == "" or o.audit == "" { fail "checks require --baseline and --audit" }
  let baseline = json.read(fp"{o.baseline}")?.require(metrics.Report)?
  if o.baseline_provenance != "" {
    metrics.verify_blanket_baseline(o.root, o.baseline, o.baseline_provenance, baseline)?
  } else if o.baseline.ends_with("baseline-blanket-scope.json") {
    fail "the corrected blanket baseline requires --baseline-provenance"
  }
  assert_structural_not_risen(baseline, baseline)?
  var rises: List[Rise] = []
  for row in current.metrics {
    let old = baseline.metrics |> where .name == row.name |> collect
    if old.len() == 1 and row.value > old[0].value { rises += [{metric: row.name, baseline: old[0].value, current: row.value, ceiling: old[0].value, approval: "", reason: "", remove_when: ""}] }
  }
  # Persist the unapproved inventory even when an envelope is malformed.
  let initial_audit: Audit = {schema: 1, mode: o.action, envelope: o.envelope, rises: rises}
  json.write(fp"{o.audit}", initial_audit)
  if o.action == "final-close" and o.envelope != "" { fail "final-close rejects temporary envelopes" }
  if o.envelope != "" {
    let envelope = json.read(fp"{o.envelope}")?.require(Envelope)?
    let metadata = json.read(fp"{o.root}/dev/consolidation/baseline-metadata.json")?.require(Metadata)?
    if envelope.start_commit != metadata.start_commit { fail "envelope baseline commit changed" }
    var seen: List[Str] = []
    for limit in envelope.limits {
      let old = baseline.metrics |> where .name == limit.metric |> collect
      if old.len() != 1 or limit.metric in seen { fail f"unknown or repeated envelope metric {limit.metric}" }
      if limit.ceiling < old[0].value { fail f"envelope ceiling below baseline for {limit.metric}" }
      if limit.approval.trim() == "" or limit.reason.trim() == "" or limit.remove_when.trim() == "" { fail f"incomplete envelope approval for {limit.metric}" }
      seen += [limit.metric]
    }
    rises = collect {
      for rise in rises {
        let approved = envelope.limits |> where .metric == rise.metric |> collect
        yield if approved.is_empty() { rise } else { {...rise, ceiling: approved[0].ceiling, approval: approved[0].approval, reason: approved[0].reason, remove_when: approved[0].remove_when} }
      }
    }
    let approved_audit: Audit = {schema: 1, mode: o.action, envelope: o.envelope, rises: rises}
    json.write(fp"{o.audit}", approved_audit)
  }
  for rise in rises { print f"{rise.metric}: {rise.baseline} -> {rise.current}; ceiling {rise.ceiling}; approval {rise.approval}" }
  if o.action == "final-close" {
    assert_final_close(current, baseline)?
    if o.review_evidence == "" { fail "final-close requires --review-evidence" }
    let evidence = json.read(fp"{o.review_evidence}")?.require(metrics.ReviewEvidenceBundle)?
    metrics.verify_review_evidence(current, evidence, o.root)?
  } else {
    assert_structural_not_risen(current, baseline)?
    if o.review_evidence != "" { fail "--review-evidence is only valid with final-close" }
  }
}

let options: Options = cli.parse(args, {
  action: {positional: true, form: "ACTION"}, root: {form: "--root PATH", kind: "Path", required: true},
  source: {form: "--source-root SOURCE", required: true}, xsht_source: {form: "--xsht-source-root SOURCE", required: true},
  output: {form: "--output PATH", kind: "Path", required: true}, audit: {form: "--audit PATH", default: ""},
  baseline: {form: "--baseline PATH", default: ""}, baseline_provenance: {form: "--baseline-provenance PATH", default: ""}, envelope: {form: "--envelope PATH", default: ""}, input: {form: "--input PATH", default: ""},
  item_inventory: {form: "--item-inventory PATH", default: ""},
  review_evidence: {form: "--review-evidence PATH", default: ""},
})?
match execute(options) {
  Ok(_) => {}
  Err(metrics.MetricError.Invalid {path: source_path, line, detail}) => { print f"{source_path}:{line}: {detail}"; exit 1 }
  Err(metrics.MetricError.Rise {name, before, after}) => { print f"ratchet rise {name}: {before} -> {after}"; exit 1 }
  Err(metrics.MetricError.Contract {detail}) => { print $detail; exit 1 }
  Err(error) => { print $error.message; exit 1 }
}
