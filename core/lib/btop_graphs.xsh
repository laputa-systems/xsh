##! Retains graph series derived from the shared display model.
##!
##! Each model timestamp contributes one point per series; missing readings
##! remain explicit and rendering never advances this state.
use btop_disks as disks
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
  network_interface: Str?,
  temperature: graph.History,
  disk_identity: Str?,
  disk_read: graph.History,
  disk_write: graph.History,
  disk_total: graph.History,
}

## Starts every panel graph with the same empty sample timeline.
export pure empty() -> Histories {
  {
    cpu: graph.empty_history(),
    memory: graph.empty_history(),
    network_receive: graph.empty_history(),
    network_transmit: graph.empty_history(),
    network_interface: null,
    temperature: graph.empty_history(),
    disk_identity: null,
    disk_read: graph.empty_history(),
    disk_write: graph.empty_history(),
    disk_total: graph.empty_history(),
  }
}

pure rate_sample(at_ns: Int, value: rates.Rate) -> graph.Sample {
  if value.state == .Observed and value.value != null {
    return {at_ns: at_ns, value: value.value, state: graph.Observed}
  }
  return {at_ns: at_ns, value: null, state: graph.Gap} when value.state == .Gap
  {at_ns: at_ns, value: null, state: graph.Unavailable}
}

pure memory_sample(value: model.Model) -> graph.Sample {
  if value.memory.total_bytes != null and value.memory.total_bytes > 0 and value.memory.used_bytes.value != null {
    let percent = value.memory.used_bytes.value.float() * 100.0 / value.memory.total_bytes.float()
    return {at_ns: value.at_ns, value: percent, state: graph.Observed}
  }
  {at_ns: value.at_ns, value: null, state: graph.Unavailable}
}

pure temperature_sample(value: model.Model) -> graph.Sample {
  if value.telemetry.temperature.state == .Observed and value.telemetry.temperature.package_millidegrees != null {
    let celsius = value.telemetry.temperature.package_millidegrees.float() / 1000.0
    return {at_ns: value.at_ns, value: celsius, state: graph.Observed}
  }
  {at_ns: value.at_ns, value: null, state: graph.Unavailable}
}

# Pins the one device whose identity owns the monitor's bounded disk graph.
pure first_disk(devices: List[disks.Device]) -> disks.Device? {
  for device in devices {
    return device when device.identity != null
  }
  null
}

# Keeps a combined disk graph unavailable unless both byte-rate directions exist.
pure disk_sum_rate(device: disks.Device?) -> rates.Rate {
  if let current = device {
    let {read_bytes_per_second: read, write_bytes_per_second: write, ..} = current
    if read.state == .Observed and write.state == .Observed and read.value != null and write.value != null {
      return {state: rates.Observed, value: read.value + write.value}
    }
    if read.state == .Gap or write.state == .Gap { return {state: rates.Gap, value: null} }
    if read.state == .Reset or write.state == .Reset { return {state: rates.Reset, value: null} }
    if read.state == .Invalid or write.state == .Invalid { return {state: rates.Invalid, value: null} }
    if read.state == .Warming or write.state == .Warming { return {state: rates.Warming, value: null} }
  }
  {state: rates.Unavailable, value: null}
}

## Adds each model time once and resets only network history on interface changes.
export pure observe(
  histories: Histories,
  value: model.Model,
  interface_name: Str? = null,
) -> Result[Histories, Error] {
  let selected_name = if interface_name == null { value.network.interface_name } else { interface_name }
  let selected = model.network_metric(value.network, selected_name)
  let network_changed = histories.network_interface != selected_name
  let same_sample = ! histories.cpu.samples.is_empty() and
    histories.cpu.samples[-1].at_ns == value.at_ns
  let disk = first_disk(value.storage.devices)
  let disk_identity = if let current = disk { current.identity } else { null }
  let disk_changed = histories.disk_identity != disk_identity
  let network_history = if network_changed {
    {
      ...histories,
      network_receive: graph.empty_history(),
      network_transmit: graph.empty_history(),
      network_interface: selected_name,
    }
  } else { histories }
  let disk_history = if disk_changed {
    {
      ...histories,
      disk_identity: disk_identity,
      disk_read: graph.empty_history(),
      disk_write: graph.empty_history(),
      disk_total: graph.empty_history(),
    }
  } else { histories }
  let receive_rate = if let current = selected { current.rx_bytes_per_second } else { {state: rates.Unavailable, value: null} }
  let transmit_rate = if let current = selected { current.tx_bytes_per_second } else { {state: rates.Unavailable, value: null} }
  let disk_read_rate = if let current = disk { current.read_bytes_per_second } else { {state: rates.Unavailable, value: null} }
  let disk_write_rate = if let current = disk { current.write_bytes_per_second } else { {state: rates.Unavailable, value: null} }
  let disk_total_rate = disk_sum_rate(disk)
  Ok({
    cpu: if same_sample { histories.cpu } else { graph.append(histories.cpu, rate_sample(value.at_ns, value.cpu_percent))? },
    memory: if same_sample { histories.memory } else { graph.append(histories.memory, memory_sample(value))? },
    network_receive: if same_sample and !network_changed {
      histories.network_receive
    } else {
      graph.append(network_history.network_receive, rate_sample(value.at_ns, receive_rate))?
    },
    network_transmit: if same_sample and !network_changed {
      histories.network_transmit
    } else {
      graph.append(network_history.network_transmit, rate_sample(value.at_ns, transmit_rate))?
    },
    network_interface: selected_name,
    temperature: if same_sample { histories.temperature } else { graph.append(histories.temperature, temperature_sample(value))? },
    disk_identity: disk_identity,
    disk_read: if same_sample and !disk_changed { histories.disk_read } else { graph.append(disk_history.disk_read, rate_sample(value.at_ns, disk_read_rate))? },
    disk_write: if same_sample and !disk_changed { histories.disk_write } else { graph.append(disk_history.disk_write, rate_sample(value.at_ns, disk_write_rate))? },
    disk_total: if same_sample and !disk_changed { histories.disk_total } else { graph.append(disk_history.disk_total, rate_sample(value.at_ns, disk_total_rate))? },
  })
}
