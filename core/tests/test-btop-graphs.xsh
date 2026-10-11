use core.lib.btop_graphs as graphs
use core.lib.btop_graph as graph
use core.lib.btop_replay as replay
use core.tests.support.btop as fixture
use core.lib.btop_rates as rates

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
  assert second.cpu.samples[0].state == graph.Unavailable
  assert second.cpu.samples[1].value == 40.0
  assert second.memory.samples[1].value == 75.0
  assert second.network_receive.samples[1].value == 10240.0
  assert second.network_transmit.samples[1].value == 2048.0
  assert second.temperature.samples[1].state == graph.Unavailable
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
    },
  }
  let next = graphs.observe(baseline, gap_model)?

  assert next.cpu.samples[1].state == graph.Gap
  assert next.cpu.samples[1].value == null
  assert next.memory.samples[1].state == graph.Observed
  assert next.memory.samples[1].value == 0.0
  assert next.network_receive.samples[1].state == graph.Unavailable
}
