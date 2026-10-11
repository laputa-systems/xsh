##! Retains graph series derived from the shared display model.
##!
##! Each model timestamp contributes one point per series; missing readings
##! remain explicit and rendering never advances this state.
use btop_graph as graph
use btop_model as model
use btop_rates as rates
use system_report as report

## Keeps the independent sample history for each default panel graph.
export type Histories = {
  cpu: graph.History,
  memory: graph.History,
  network_receive: graph.History,
  network_transmit: graph.History,
  temperature: graph.History,
}

## Starts every panel graph with the same empty sample timeline.
export pure empty() -> Histories {
  {
    cpu: graph.empty_history(),
    memory: graph.empty_history(),
    network_receive: graph.empty_history(),
    network_transmit: graph.empty_history(),
    temperature: graph.empty_history(),
  }
}

pure rate_sample(at_ns: Int, value: rates.Rate) -> graph.Sample {
  if value.state == rates.Observed and value.value != null {
    return {at_ns: at_ns, value: value.value, state: graph.Observed}
  }
  if value.state == rates.Gap {
    return {at_ns: at_ns, value: null, state: graph.Gap}
  }
  {at_ns: at_ns, value: null, state: graph.Unavailable}
}

pure memory_sample(value: model.Model) -> graph.Sample {
  if value.memory.total_bytes != null and (value.memory.total_bytes ?? 0) > 0 and value.memory.used_bytes.value != null {
    let percent = (value.memory.used_bytes.value ?? 0).float() * 100.0 / (value.memory.total_bytes ?? 1).float()
    return {at_ns: value.at_ns, value: percent, state: graph.Observed}
  }
  {at_ns: value.at_ns, value: null, state: graph.Unavailable}
}

pure temperature_sample(value: model.Model) -> graph.Sample {
  if value.telemetry.temperature.state == report.Observed and value.telemetry.temperature.package_millidegrees != null {
    let celsius = (value.telemetry.temperature.package_millidegrees ?? 0).float() / 1000.0
    return {at_ns: value.at_ns, value: celsius, state: graph.Observed}
  }
  {at_ns: value.at_ns, value: null, state: graph.Unavailable}
}

## Appends one model timestamp to each panel series without filling absent data.
export pure observe(histories: Histories, value: model.Model) -> Result[Histories, Error] {
  Ok({
    cpu: graph.append(histories.cpu, rate_sample(value.at_ns, value.cpu_percent))?,
    memory: graph.append(histories.memory, memory_sample(value))?,
    network_receive: graph.append(histories.network_receive, rate_sample(value.at_ns, value.network.rx_bytes_per_second))?,
    network_transmit: graph.append(histories.network_transmit, rate_sample(value.at_ns, value.network.tx_bytes_per_second))?,
    temperature: graph.append(histories.temperature, temperature_sample(value))?,
  })
}
