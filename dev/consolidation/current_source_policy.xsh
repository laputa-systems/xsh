##! Current-source report policy for the serial structural gate.
use consolidation_metrics_current as metrics

const LOWERING_RESOLVER_METRIC = "lowering_type_resolver_definitions"
const GENERATED_STAGE_METRIC = "stage_variants"
const GENERATED_STAGE_COUNT = 46
const GENERATED_STAGE_SOURCE = "src/runtime/eval/indexed/full.rs"

## Requires the current source report to preserve the lowering and stage-schema contracts.
export pure assert_current_source_contract(report: metrics.Report) -> Result[Unit, metrics.MetricError] {
  guard report.schema == 1 else { return Err(metrics.MetricError.Contract(detail: "unsupported current source report schema")) }
  let lowering = report.metrics |> where .name == LOWERING_RESOLVER_METRIC |> collect
  if lowering.len() != 1 { return Err(metrics.MetricError.Contract(detail: f"current source report requires exactly one {LOWERING_RESOLVER_METRIC} metric")) }
  if lowering[0].value != 0 { return Err(metrics.MetricError.Contract(detail: f"current source report requires zero {LOWERING_RESOLVER_METRIC}")) }
  let stages = report.metrics |> where .name == GENERATED_STAGE_METRIC |> collect
  if stages.len() != 1 { return Err(metrics.MetricError.Contract(detail: f"current source report requires exactly one {GENERATED_STAGE_METRIC} metric")) }
  if stages[0].value != GENERATED_STAGE_COUNT { return Err(metrics.MetricError.Contract(detail: f"current source report requires {GENERATED_STAGE_COUNT} {GENERATED_STAGE_METRIC}")) }
  if stages[0].evidence.len() != GENERATED_STAGE_COUNT { return Err(metrics.MetricError.Contract(detail: f"current source report requires {GENERATED_STAGE_COUNT} {GENERATED_STAGE_METRIC} evidence rows")) }
  if (stages[0].evidence |> where .path != GENERATED_STAGE_SOURCE |> count()) != 0 {
    return Err(metrics.MetricError.Contract(detail: f"current source report requires {GENERATED_STAGE_METRIC} evidence only from {GENERATED_STAGE_SOURCE}"))
  }
  Ok()
}
