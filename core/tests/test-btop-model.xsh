use core.lib.btop_model as model
use core.lib.btop_source as source
use core.lib.btop_render as renderer
use core.lib.btop_ui as ui
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
  assert (first.cpu_cores |> map .utilization.state) == [rates.Warming, rates.Warming]
  assert first.network.rx_bytes_per_second.state == rates.Warming
  assert first.processes[0].cpu_percent.state == rates.Warming
  assert next.cpu_percent.value?.format(precision: 1) == "40.0"
  assert (next.cpu_cores |> map .id) == [0, 1]
  assert next.cpu_cores[0].utilization.value == 40.0
  assert next.cpu_cores[1].utilization.value == 40.0
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

test test_btop_model_tracks_sparse_cpu_ids_and_hotplug_without_inheriting_history {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_frame(bundle, "frames/000000", 0, default_processes())
  fixture.write_b3_frame(bundle, "frames/000001", 1, default_processes())
  fixture.write_b3_frame(bundle, "frames/000002", 1, default_processes())
  fixture.write_b3_frame(bundle, "frames/000003", 1, default_processes())
  bundle.write(
    p"frames/000000/proc/stat",
    "cpu 100 0 50 850 0 0 0 0 0 0\ncpu0 50 0 25 425 0 0 0 0 0 0\ncpu4 50 0 25 425 0 0 0 0 0 0\n",
  )
  bundle.write(
    p"frames/000001/proc/stat",
    "cpu 160 0 70 970 0 0 0 0 0 0\ncpu4 80 0 35 485 0 0 0 0 0 0\ncpu9 80 0 35 485 0 0 0 0 0 0\n",
  )
  bundle.write(
    p"frames/000002/proc/stat",
    "cpu 220 0 90 1090 0 0 0 0 0 0\ncpu0 30 0 15 455 0 0 0 0 0 0\ncpu4 110 0 45 545 0 0 0 0 0 0\ncpu9 110 0 45 545 0 0 0 0 0 0\n",
  )
  bundle.write(
    p"frames/000003/proc/stat",
    "cpu 280 0 110 1210 0 0 0 0 0 0\ncpu0 40 0 20 480 0 0 0 0 0 0\ncpu4 20 0 10 560 0 0 0 0 0 0\ncpu9 140 0 55 605 0 0 0 0 0 0\n",
  )
  let first = capture(bundle, p"frames/000000", "cpu-hotplug", 1000000000, 0)?
  let second = capture(bundle, p"frames/000001", "cpu-hotplug", 2000000000, 1)?
  let third = capture(bundle, p"frames/000002", "cpu-hotplug", 3000000000, 2)?
  let fourth = capture(bundle, p"frames/000003", "cpu-hotplug", 4000000000, 3)?
  let initial = model.step(first, null)
  let hotplug = model.step(second, first)
  assert (initial.cpu_cores |> map .id) == [0, 4]
  assert (hotplug.cpu_cores |> map .id) == [4, 9]
  assert hotplug.cpu_cores[0].utilization.state == rates.Observed
  assert hotplug.cpu_cores[1].utilization.state == rates.Warming

  let returned = model.step(third, second)
  assert (returned.cpu_cores |> map .id) == [0, 4, 9]
  assert returned.cpu_cores[0].utilization.state == rates.Warming
  assert returned.cpu_cores[1].utilization.state == rates.Observed

  let reset = model.step(fourth, third)
  assert reset.cpu_percent.state == rates.Observed
  assert reset.cpu_cores[0].utilization.state == rates.Observed
  assert reset.cpu_cores[1].utilization.state == rates.Reset
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

  let reversed_time = capture(bundle, p"frames/000001", "first-source", 999999999, 1)?
  let reversed = model.step(reversed_time, before)
  assert reversed.cpu_percent.state == rates.Invalid
  assert reversed.network.rx_bytes_per_second.state == rates.Invalid
  assert reversed.processes[0].cpu_percent.state == rates.Invalid
}

test test_btop_model_marks_missed_samples_as_gaps_and_ignores_uptime_jumps_for_rates {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_frame(bundle, "frames/000000", 0, default_processes())
  fixture.write_b3_frame(
    bundle,
    "frames/000001",
    1,
    [{pid: 42, start_ticks: 1000, cpu_ticks: 35, rss_pages: 16384, uid: 1000}],
  )
  let before = capture(bundle, p"frames/000000", "clock", 1000000000, 0)?
  let current = capture(bundle, p"frames/000001", "clock", 2000000000, 1)?
  let shifted_uptime: model.Observation = {
    ...current,
    cpu: {...current.cpu, uptime: {state: report.Observed, seconds: 604800.0}},
  }

  let ordinary = model.step(current, before)
  let after_uptime_jump = model.step(shifted_uptime, before)
  assert after_uptime_jump.cpu_percent == ordinary.cpu_percent
  assert after_uptime_jump.network.rx_bytes_per_second == ordinary.network.rx_bytes_per_second
  assert after_uptime_jump.processes[0].cpu_percent == ordinary.processes[0].cpu_percent

  let gap = model.step(current, before, gap: true)
  assert gap.cpu_percent.state == rates.Gap and gap.cpu_percent.value == null
  assert gap.cpu_cores[0].utilization.state == rates.Gap
  assert gap.network.rx_bytes_per_second.state == rates.Gap
  assert gap.processes[0].cpu_percent.state == rates.Gap
}

test test_btop_network_name_changes_and_replacement_start_new_counter_epochs {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_frame(bundle, "frames/000000", 0, default_processes())
  fixture.write_b3_frame(bundle, "frames/000001", 1, default_processes())
  let before = capture(bundle, p"frames/000000", "net-epoch", 1000000000, 0)?
  let previous = model.step(before, null)

  bundle.write(p"frames/000001/sys/class/net/eth0/ifindex", "3\n")
  let replaced = capture(bundle, p"frames/000001", "net-epoch", 2000000000, 1)?
  let replacement = model.step(replaced, before)
  assert replacement.network.interface_name == "eth0"
  assert replacement.network.rx_bytes_per_second.state == rates.Warming

  bundle.write(
    p"frames/000001/proc/net/dev",
    "Inter-| Receive                                                | Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\nenp0: 1010240 100 0 0 0 0 0 0 502048 100 0 0 0 0 0 0\nlo: 64 1 0 0 0 0 0 0 64 1 0 0 0 0 0 0\n",
  )
  bundle.mkdir(p"frames/000001/sys/class/net/enp0", parents: true)
  bundle.write(p"frames/000001/sys/class/net/enp0/ifindex", "2\n")
  let renamed = capture(bundle, p"frames/000001", "net-epoch", 2000000000, 1)?
  let rename_step = model.step(renamed, before)
  assert rename_step.network.interface_name == "enp0"
  assert rename_step.network.rx_bytes_per_second.state == rates.Warming

  bundle.write(
    p"frames/000001/proc/net/dev",
    "Inter-| Receive                                                | Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\neth0: 999999 100 0 0 0 0 0 0 499999 100 0 0 0 0 0 0\nlo: 64 1 0 0 0 0 0 0 64 1 0 0 0 0 0 0\n",
  )
  bundle.write(p"frames/000001/sys/class/net/eth0/ifindex", "2\n")
  let reset = capture(bundle, p"frames/000001", "net-epoch", 2000000000, 1)?
  let reset_step = model.step(reset, before)
  assert reset_step.network.rx_bytes_per_second.state == rates.Reset
  assert previous.network.rx_bytes_per_second.state == rates.Warming

  var previous_observation = before
  for index in range(40) {
    let epoch = 10 + index
    bundle.write(p"frames/000001/sys/class/net/eth0/ifindex", f"{epoch}\n")
    let current = capture(bundle, p"frames/000001", "net-epoch", 3000000000 + index * 1000000000, index)?
    let current_model = model.step(current, previous_observation)
    assert current_model.network.rx_bytes_per_second.state == rates.Warming
    previous_observation = current
  }
}

test test_btop_process_uid_cache_expires_and_invalidates_on_exec_or_credentials {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_frame(bundle, "frames/000000", 0, default_processes())
  fixture.write_b3_frame(bundle, "frames/000001", 1, default_processes())
  let first = capture(bundle, p"frames/000000", "uid-cache", 1000000000, 0)?
  let second = capture(bundle, p"frames/000001", "uid-cache", 2000000000, 1)?
  let seeded = model.step_cached(first, null, model.empty_metadata_cache("uid-cache"))
  assert seeded.metadata.by_pid.len() == 1
  assert seeded.value.processes[0].uid_state == model.Fresh

  let missing_uid: model.Observation = {
    ...second,
    processes: {
      ...second.processes,
      processes: [{...second.processes.processes[0], uid: null}],
    },
  }
  let stale = model.step_cached(missing_uid, first, seeded.metadata)
  assert stale.value.processes[0].uid == 1000
  assert stale.value.processes[0].uid_state == model.Stale
  assert stale.metadata.by_pid.len() == 1
  let stale_screen = renderer.render_view(stale.value, ui.initial(stale.value, 1), 80, 24)
  assert (stale_screen.lines |> join("\n")).find("~1000") != null

  let expired_input: model.Observation = {...missing_uid, at_ns: 6000000000}
  let expired = model.step_cached(expired_input, missing_uid, stale.metadata)
  assert expired.value.processes[0].uid == null
  assert expired.value.processes[0].uid_state == model.Unavailable
  assert expired.metadata.by_pid.len() == 0

  let changed_command: model.Observation = {
    ...missing_uid,
    at_ns: 3000000000,
    processes: {
      ...missing_uid.processes,
      processes: [{...missing_uid.processes.processes[0], command: "exec-worker"}],
    },
  }
  let after_exec = model.step_cached(changed_command, missing_uid, seeded.metadata)
  assert after_exec.value.processes[0].uid == null
  assert after_exec.value.processes[0].uid_state == model.Unavailable
  assert after_exec.metadata.by_pid.len() == 0

  let changed_credentials: model.Observation = {
    ...second,
    at_ns: 3000000000,
    processes: {
      ...second.processes,
      processes: [{...second.processes.processes[0], uid: 2000}],
    },
  }
  let refreshed = model.step_cached(changed_credentials, first, seeded.metadata)
  assert refreshed.value.processes[0].uid == 2000
  assert refreshed.value.processes[0].uid_state == model.Fresh
}

test test_btop_process_uid_cache_evicts_exits_and_pid_reuse {
  let bundle = fs.tempdir()?
  defer bundle.close()
  fixture.write_b3_frame(bundle, "frames/000000", 0, default_processes())
  fixture.write_b3_frame(bundle, "frames/000001", 1, default_processes())
  let first = capture(bundle, p"frames/000000", "uid-churn", 1000000000, 0)?
  let seeded = model.step_cached(first, null, model.empty_metadata_cache("uid-churn"))

  let reused: model.Observation = {
    ...first,
    at_ns: 2000000000,
    processes: {
      ...first.processes,
      processes: [{...first.processes.processes[0], start_ticks: 9000, uid: null}],
    },
  }
  let reused_step = model.step_cached(reused, first, seeded.metadata)
  assert reused_step.value.processes[0].uid == null
  assert reused_step.value.processes[0].uid_state == model.Unavailable
  assert reused_step.metadata.by_pid.len() == 0

  let exited: model.Observation = {...first, at_ns: 3000000000, processes: {...first.processes, processes: []}}
  let exited_step = model.step_cached(exited, first, seeded.metadata)
  assert exited_step.value.processes == []
  assert exited_step.metadata.by_pid.len() == 0

  var churn_cache = model.empty_metadata_cache("uid-churn")
  var churn_previous: model.Observation? = null
  for index in range(40) {
    let row = {pid: 100 + index, start_ticks: 1000 + index, cpu_ticks: index, rss_pages: 1, uid: 1000}
    fixture.write_b3_frame(bundle, f"frames/churn-{index}", 0, [row])
    let current = capture(bundle, fp"frames/churn-{index}", "uid-churn", 4000000000 + index * 1000000000, index)?
    let next = model.step_cached(current, churn_previous, churn_cache)
    assert next.metadata.by_pid.len() <= 1
    churn_cache = next.metadata
    churn_previous = current
  }
}
