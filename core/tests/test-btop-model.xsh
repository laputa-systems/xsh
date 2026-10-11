use core.lib.btop_model as model
use core.lib.btop_source as source
use core.tests.support.btop as fixture
use core.lib.btop_rates as rates
use core.lib.system_report as report

pure default_processes() -> List[fixture.ProcessFixture] {
  [{pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 16384, uid: 1000}]
}

proc capture(root: FsRoot, prefix: Path, source_id: Str, at_ns: Int, index: Int) [fs, error] -> Result[model.Observation, Error] {
  let frame_root = root.open_root(prefix)?
  defer frame_root.close()
  source.capture(frame_root, source_id, at_ns, 4096, 100)
}

test test_btop_model_matches_the_independent_b3_monitor_oracle {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_frame(bundle, "frames/000000", 0, default_processes())
  fixture.write_b3_frame(
    bundle,
    "frames/000001",
    1,
    [{pid: 42, start_ticks: 1000, cpu_ticks: 35, rss_pages: 16384, uid: 1000}],
  )
  let before = capture(bundle, p"frames/000000", "b3", 1000000000, 0)?
  let current = capture(bundle, p"frames/000001", "b3", 2000000000, 1)?
  let first = model.step(before, null)
  let next = model.step(current, before)

  assert first.cpu_percent.state == rates.Warming
  assert first.network.rx_bytes_per_second.state == rates.Warming
  assert first.processes[0].cpu_percent.state == rates.Warming
  assert next.cpu_percent.value?.format(precision: 1) == "40.0"
  assert next.processes[0].cpu_percent.value?.format(precision: 1) == "12.5"
  assert next.processes[0].rss_bytes == 67108864
  assert next.processes[0].uid == 1000
  assert next.memory.used_bytes.value == 805306368
  assert next.network.interface_name == "eth0"
  assert next.network.rx_bytes_per_second.value == 10240.0
  assert next.network.tx_bytes_per_second.value == 2048.0
  assert next.memory.swap_total_state == report.Absent
  assert next.memory.swap_used_bytes.state == rates.Unavailable
}

test test_btop_model_ranks_equal_process_rates_by_pid_and_tracks_lifetimes {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_frame(
    bundle,
    "frames/000000",
    0,
    [
      {pid: 43, start_ticks: 2000, cpu_ticks: 20, rss_pages: 20, uid: 1000},
      {pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 10, uid: 1000},
    ],
  )
  fixture.write_b3_frame(
    bundle,
    "frames/000001",
    1,
    [
      {pid: 43, start_ticks: 2000, cpu_ticks: 45, rss_pages: 20, uid: 1000},
      {pid: 42, start_ticks: 1000, cpu_ticks: 35, rss_pages: 10, uid: 1000},
    ],
  )
  fixture.write_b3_frame(
    bundle,
    "frames/000002",
    1,
    [
      {pid: 44, start_ticks: 4000, cpu_ticks: 5, rss_pages: 5, uid: 1000},
      {pid: 42, start_ticks: 3000, cpu_ticks: 50, rss_pages: 10, uid: 1000},
    ],
  )
  let before = capture(bundle, p"frames/000000", "lifetime", 1000000000, 0)?
  let current = capture(bundle, p"frames/000001", "lifetime", 2000000000, 1)?
  let next = capture(bundle, p"frames/000002", "lifetime", 3000000000, 1)?
  let stable = model.step(current, before)
  assert [row.pid for row in stable.processes] == [42, 43]
  assert stable.processes[0].cpu_percent.value == stable.processes[1].cpu_percent.value

  let changed = model.step(next, current)
  assert [row.pid for row in changed.processes] == [42, 44]
  assert changed.processes[0].cpu_percent.state == rates.Warming
  assert changed.processes[1].cpu_percent.state == rates.Warming
}

test test_btop_model_does_not_mix_source_identities_or_invalid_intervals {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_frame(bundle, "frames/000000", 0, default_processes())
  fixture.write_b3_frame(
    bundle,
    "frames/000001",
    1,
    [{pid: 42, start_ticks: 1000, cpu_ticks: 35, rss_pages: 16384, uid: 1000}],
  )
  let before = capture(bundle, p"frames/000000", "first-source", 1000000000, 0)?
  let next_source = capture(bundle, p"frames/000001", "second-source", 2000000000, 1)?
  let changed_source = model.step(next_source, before)
  assert changed_source.cpu_percent.state == rates.Warming
  assert changed_source.network.rx_bytes_per_second.state == rates.Warming
  assert changed_source.processes[0].cpu_percent.state == rates.Warming

  let same_time = capture(bundle, p"frames/000001", "first-source", 1000000000, 1)?
  let invalid = model.step(same_time, before)
  assert invalid.cpu_percent.state == rates.Invalid
  assert invalid.network.rx_bytes_per_second.state == rates.Invalid
  assert invalid.processes[0].cpu_percent.state == rates.Invalid
}
