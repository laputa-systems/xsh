#!/usr/bin/env -S xsh --
use consolidation_metrics as metrics

type Options = {action: Str, root: Path, source: Str, xsht_source: Str, output: Path, audit: Str, baseline: Str, envelope: Str, input: Str}
type Limit = {metric: Str, ceiling: Int, approval: Str, reason: Str, remove_when: Str}
type Envelope = {start_commit: Str, limits: List[Limit]}
type Metadata = {start_commit: Str}
type Rise = {metric: Str, baseline: Int, current: Int, ceiling: Int, approval: Str, reason: Str, remove_when: Str}
type Audit = {schema: Int, mode: Str, envelope: Str, rises: List[Rise]}

# Ceilings are temporary absolute bounds; every rise still belongs in the audit.
proc execute(o: Options) [fs, error, io] -> Result[Unit] {
  if o.action not in ["measure", "check", "final-close"] { fail "expected measure, check, or final-close" }
  if ([o.source, o.xsht_source] |> sort) != ["crates/xsht/src", "src"] { fail "explicit source roots must be src and crates/xsht/src" }
  let current = if o.input == "" { metrics.measure(o.root, [o.source, o.xsht_source])? } else { json.read(fp"{o.input}")?.require(metrics.Report)? }
  json.write(o.output, current)
  metrics.assert_not_risen(current, current)?
  if o.action == "measure" { return }
  if o.baseline == "" or o.audit == "" { fail "checks require --baseline and --audit" }
  let baseline = json.read(fp"{o.baseline}")?.require(metrics.Report)?
  metrics.assert_not_risen(baseline, baseline)?
  var ceilings = baseline
  var limits: List[Limit] = []
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
    limits = envelope.limits
    ceilings = {...baseline, metrics: collect {
      for row in baseline.metrics {
        let approved = limits |> where .metric == row.name |> collect
        yield if approved.is_empty() { row } else { {...row, value: approved[0].ceiling} }
      }
    }}
    rises = collect {
      for rise in rises {
        let approved = limits |> where .metric == rise.metric |> collect
        yield if approved.is_empty() { rise } else { {...rise, ceiling: approved[0].ceiling, approval: approved[0].approval, reason: approved[0].reason, remove_when: approved[0].remove_when} }
      }
    }
    let approved_audit: Audit = {schema: 1, mode: o.action, envelope: o.envelope, rises: rises}
    json.write(fp"{o.audit}", approved_audit)
  }
  for rise in rises { print f"{rise.metric}: {rise.baseline} -> {rise.current}; ceiling {rise.ceiling}; approval {rise.approval}" }
  metrics.assert_not_risen(current, ceilings)?
  if o.action == "final-close" and ! current.reviews.is_empty() { fail f"unmeasured semantic rows: {(current.reviews |> map .name |> collect).join(", ")}" }
}

let options: Options = cli.parse(args, {
  action: {positional: true, form: "ACTION"}, root: {form: "--root PATH", kind: "Path", required: true},
  source: {form: "--source-root SOURCE", required: true}, xsht_source: {form: "--xsht-source-root SOURCE", required: true},
  output: {form: "--output PATH", kind: "Path", required: true}, audit: {form: "--audit PATH", default: ""},
  baseline: {form: "--baseline PATH", default: ""}, envelope: {form: "--envelope PATH", default: ""}, input: {form: "--input PATH", default: ""},
})?
match execute(options) {
  Ok(_) => {}
  Err(metrics.MetricError.Invalid {path: source_path, line, detail}) => { print f"{source_path}:{line}: {detail}"; exit 1 }
  Err(metrics.MetricError.Rise {name, before, after}) => { print f"ratchet rise {name}: {before} -> {after}"; exit 1 }
  Err(metrics.MetricError.Contract {detail}) => { print $detail; exit 1 }
  Err(error) => { print $error.message; exit 1 }
}
