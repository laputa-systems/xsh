use core.lib.btop_disks as disks
use core.lib.btop_graph as graph
use core.lib.btop_graphs as graphs
use core.lib.btop_rates as rates
use core.lib.btop_replay as replay
use core.lib.system_report as report
use core.tests.support.btop as fixture

test test_btop_graphs_append_cpu_memory_network_and_temperature_at_sample_times {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(
    bundle,
    [{pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 16384, uid: 1000}],
  )
  let loaded = replay.load(bundle.host_path()?)?
  let frames = replay.models(loaded.observations)
  let first = graphs.observe(graphs.empty(), frames[0])?
  let second = graphs.observe(first, frames[1])?

  assert second.cpu.samples.len() == 2
  assert second.cpu.samples[0].state == .Unavailable
  assert second.cpu.samples[1].value == 40.0
  assert second.memory.samples[1].value == 75.0
  assert second.network_receive.samples[1].value == 10240.0
  assert second.network_transmit.samples[1].value == 2048.0
  assert second.temperature.samples[1].state == .Unavailable
  assert second.cpu.samples[1].at_ns == frames[1].at_ns
}

test test_btop_graphs_preserve_gap_zero_and_missing_source_states {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(bundle, [])
  let loaded = replay.load(bundle.host_path()?)?
  let frames = replay.models(loaded.observations)
  let baseline = graphs.observe(graphs.empty(), frames[0])?
  let gap_model = {
    ...frames[1],
    at_ns: frames[1].at_ns + 1,
    cpu_percent: {state: rates.Gap, value: null},
    memory: {
      ...frames[1].memory,
      used_bytes: {state: rates.Observed, value: 0},
    },
    network: {
      ...frames[1].network,
      rx_bytes_per_second: {state: rates.Unavailable, value: null},
      interfaces: [
        {...frames[1].network.interfaces[0], rx_bytes_per_second: {state: rates.Unavailable, value: null}},
        frames[1].network.interfaces[1],
      ],
    },
  }
  let next = graphs.observe(baseline, gap_model)?

  assert next.cpu.samples[1].state == .Gap
  assert next.cpu.samples[1].value == null
  assert next.memory.samples[1].state == .Observed
  assert next.memory.samples[1].value == 0.0
  assert next.network_receive.samples[1].state == .Unavailable
}

test test_btop_network_selection_resets_only_the_selected_interface_history {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(
    bundle,
    [{pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 16384, uid: 1000}],
  )
  let loaded = replay.load(bundle.host_path()?)?
  let frames = replay.models(loaded.observations)
  let first = graphs.observe(graphs.empty(), frames[0])?
  let selected = graphs.observe(first, frames[1], "lo")?

  assert selected.network_interface == "lo"
  assert selected.network_receive.samples.len() == 1
  assert selected.network_receive.samples[0].value == 0.0
  assert selected.network_transmit.samples[0].value == 0.0
  assert selected.cpu.samples.len() == 2
  assert selected.memory.samples.len() == 2
}

test test_btop_network_selection_at_the_current_sample_time_preserves_other_histories {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(bundle, [])
  let loaded = replay.load(bundle.host_path()?)?
  let value = replay.models(loaded.observations)[0]
  let first = graphs.observe(graphs.empty(), value)?
  let selected = graphs.observe(first, value, "lo")?
  let repeated = graphs.observe(selected, value, "lo")?

  assert selected.network_interface == "lo"
  assert selected.network_receive.samples.len() == 1
  assert selected.network_transmit.samples.len() == 1
  assert selected.cpu.samples == first.cpu.samples
  assert selected.memory.samples == first.memory.samples
  assert selected.temperature.samples == first.temperature.samples
  assert repeated == selected
}

test test_btop_graphs_track_read_write_and_combined_io_for_one_device_identity {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_bundle(bundle, [])
  let loaded = replay.load(bundle.host_path()?)?
  let base = replay.models(loaded.observations)[0]
  let device: disks.Device = disks.Device(identity: "41:8:0", identity_state: report.Observed, name: "sda", major: 8, minor: 0, mount_targets: ["/mnt/data"], filesystem: "ext4", capacity_state: report.Observed, capacity_age_ns: 0, total_bytes: 1073741824, used_bytes: 268435456, available_bytes: 805306368, read_sectors: 10, read_state: report.Observed, read_bytes_per_second: {state: rates.Observed, value: 1024.0}, write_sectors: 20, write_state: report.Observed, write_bytes_per_second: {state: rates.Observed, value: 2048.0}, in_flight: 0, in_flight_state: report.Observed)
  let value = {...base, storage: {block_state: report.Observed, mount_state: report.Observed, devices: [device]}}
  let history = graphs.observe(graphs.empty(), value)?

  assert history.disk_identity == "41:8:0"
  assert history.disk_read.samples[0].value == 1024.0
  assert history.disk_write.samples[0].value == 2048.0
  assert history.disk_total.samples[0].value == 3072.0
}
