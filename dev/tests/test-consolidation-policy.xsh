use consolidation_metrics_current as metrics

type AuditRise = {metric: Str, baseline: Int, current: Int, ceiling: Int, approval: Str, reason: Str, remove_when: Str}

pure report(src_lines: Int, xsht_lines: Int, manual_payloads: Int, blanket_lines: Int) -> metrics.Report {
  {
    schema: 1,
    sources: [],
    metrics: [
      {name: "blanket_dead_code_lines", value: blanket_lines, evidence: []},
      {name: "instruction_variants", value: 0, evidence: []},
      {name: "lowering_type_resolver_definitions", value: 0, evidence: []},
      {name: "manual_payload_calls", value: manual_payloads, evidence: []},
      {name: "nested_machine_factory_calls", value: 0, evidence: []},
      {name: "optional_raw_calls", value: 0, evidence: []},
      {name: "pattern_variants", value: 0, evidence: []},
      {name: "source_lines_src", value: src_lines, evidence: []},
      {name: "source_lines_xsht", value: xsht_lines, evidence: []},
      {name: "stage_variants", value: 0, evidence: []},
      {name: "static_type_predicate_definitions", value: 0, evidence: []},
      {name: "type_storage_mapping_definitions", value: 0, evidence: []},
      {name: "wide_arena_wildcards", value: 0, evidence: []},
    ],
    reviews: [],
  }
}

pure metric_value(report: metrics.Report, name: Str) -> Int {
  let rows = report.metrics |> where .name == name |> collect
  rows[0].value
}

test test_source_equality_and_growth_keep_the_measured_report_and_raw_audit { |ctx|
  let root = test.temp_dir(ctx, name: "source-policy")?
  let baseline = report(20, 30, 1, 0)
  let baseline_path = fp"{root}/baseline.json"
  let current_path = fp"{root}/current.json"
  let output_path = fp"{root}/saved.json"
  let audit_path = fp"{root}/audit.json"
  let envelope_path = fp"{root}/envelope.json"
  let driver = p"dev/consolidation/metrics.xsh".read_text()?
  fp"{root}/dev/consolidation".mkdir()
  json.write(fp"{root}/dev/consolidation/baseline-metadata.json", {start_commit: "fixture"})
  json.write(baseline_path, baseline)
  let flags = ["--root", root.display(), "--source-root", "src", "--xsht-source-root", "crates/xsht/src", "--input", current_path.display(), "--baseline", baseline_path.display(), "--output", output_path.display(), "--audit", audit_path.display()]
  let limits = [
    {metric: "source_lines_src", ceiling: 20, approval: "fixture owner", reason: "advisory source total", remove_when: "fixture ends"},
    {metric: "source_lines_xsht", ceiling: 30, approval: "fixture owner", reason: "advisory source total", remove_when: "fixture ends"},
  ]
  json.write(envelope_path, {start_commit: "fixture", limits: limits})

  for current in [report(20, 30, 1, 0), report(21, 30, 1, 0), report(20, 31, 1, 0), report(21, 31, 1, 0)] {
    json.write(current_path, current)
    assert test.run_script(ctx, driver, ["check", @flags, "--envelope", envelope_path])?.success
    let saved: metrics.Report = json.read(output_path)?.require()?
    assert metric_value(saved, "source_lines_src") == metric_value(current, "source_lines_src")
    assert metric_value(saved, "source_lines_xsht") == metric_value(current, "source_lines_xsht")
    let rises = json.get(json.read(audit_path)?, ["rises"])?.require(List[AuditRise])?
    assert rises.len() == (if metric_value(current, "source_lines_src") > 20 { 1 } else { 0 }) + (if metric_value(current, "source_lines_xsht") > 30 { 1 } else { 0 })
    for rise in rises {
      if rise.metric == "source_lines_src" {
        assert rise.baseline == 20 and rise.current == metric_value(current, rise.metric)
        assert rise.ceiling == 20 and rise.approval == "fixture owner"
      } else if rise.metric == "source_lines_xsht" {
        assert rise.baseline == 30 and rise.current == metric_value(current, rise.metric)
        assert rise.ceiling == 30 and rise.approval == "fixture owner"
      } else {
        test.fail(f"unexpected audited rise {rise.metric}")
      }
    }
  }
}

test test_structural_rises_stay_hard_under_envelopes_and_reports_are_validated { |ctx|
  let root = test.temp_dir(ctx, name: "structural-policy")?
  fp"{root}/dev/consolidation".mkdir()
  let baseline = report(20, 30, 1, 0)
  let current = report(20, 30, 2, 0)
  let baseline_path = fp"{root}/baseline.json"
  let current_path = fp"{root}/current.json"
  let output_path = fp"{root}/saved.json"
  let audit_path = fp"{root}/audit.json"
  let envelope_path = fp"{root}/envelope.json"
  let driver = p"dev/consolidation/metrics.xsh".read_text()?
  json.write(baseline_path, baseline)
  json.write(current_path, current)
  json.write(fp"{root}/dev/consolidation/baseline-metadata.json", {start_commit: "fixture"})
  let flags = ["--root", root.display(), "--source-root", "src", "--xsht-source-root", "crates/xsht/src", "--input", current_path.display(), "--baseline", baseline_path.display(), "--output", output_path.display(), "--audit", audit_path.display()]
  let limit = {metric: "manual_payload_calls", ceiling: 2, approval: "fixture owner", reason: "bounded fixture", remove_when: "fixture ends"}
  json.write(envelope_path, {start_commit: "fixture", limits: [limit]})
  assert ! test.run_script(ctx, driver, ["check", @flags])?.success
  assert ! test.run_script(ctx, driver, ["check", @flags, "--envelope", envelope_path])?.success
  let rises = json.get(json.read(audit_path)?, ["rises"])?.require(List[AuditRise])?
  assert rises.len() == 1
  assert rises[0].metric == "manual_payload_calls" and rises[0].baseline == 1 and rises[0].current == 2
  assert rises[0].ceiling == 2 and rises[0].approval == limit.approval

  let malformed_reports = [
    {...baseline, schema: 2},
    {...baseline, metrics: collect { for row in baseline.metrics { yield row when row.name != "wide_arena_wildcards" } }},
    {...baseline, metrics: collect { for row in baseline.metrics { yield if row.name == "source_lines_src" { {...row, value: -1} } else { row } } }},
    {...baseline, metrics: baseline.metrics + [baseline.metrics[0]]},
    {...baseline, metrics: collect { for row in baseline.metrics { yield row when row.name != "source_lines_xsht" } }},
  ]
  for malformed in malformed_reports {
    json.write(current_path, malformed)
    assert ! test.run_script(ctx, driver, ["check", @flags])?.success
  }
}
