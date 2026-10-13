use core.lib.btop_model as model
use core.lib.btop_rates as rates
use core.lib.btop_render as renderer
use core.lib.btop_source as btop_source
use core.lib.sys_cpu as syscpu
use core.lib.sys_mem as memory
use core.lib.sys_net as network
use core.lib.sys_proc as processes
use core.lib.sys_source as src
use core.lib.system_report as report
use core.lib.system_report_live as system_live
use core.lib.btop_ui as ui
use core.tests.support.btop as fixture

proc write_proc_stat(root: FsRoot, aggregate: Str, cpu0: Str, cpu1: Str) [fs, error] {
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/stat",
    f"{aggregate}\n{cpu0}\n{cpu1}\nintr 1\nctxt 2\nbtime 1700000000\nprocesses 3\nprocs_running 1\nprocs_blocked 0\nsoftirq 4 0 0 0 0 0 0 0 0 0 0\n",
  )
  root.write(p"proc/uptime", "25.50 100.00\n")
  root.write(p"proc/loadavg", "0.10 0.20 0.30 1/100 42\n")
}

proc process_stat(pid: Int, start_ticks: Int, user_ticks: Int, system_ticks: Int, rss_pages: Int, command: Str = "sample (worker)") -> Str {
  f"{pid} ({command}) S 1 {pid} {pid} 0 -1 4194560 100 0 0 0 {user_ticks} {system_ticks} 0 0 20 1 1 0 {start_ticks} 16777216 {rss_pages} 0 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n"
}

pure observed_source(text: Str) -> src.SourceRead {
  {
    observation: {state: report.Observed, value: text, raw_bytes_base64: null},
    errno: null,
    error_kind: null,
  }
}

proc write_process(root: FsRoot, pid: Int, start_ticks: Int, ticks: Int, rss_pages: Int) [fs, error] {
  root.mkdir(fp"proc/{pid}", parents: true)
  root.write(fp"proc/{pid}/stat", process_stat(pid, start_ticks, ticks, 0, rss_pages))
  root.write(fp"proc/{pid}/statm", f"65536 {rss_pages} 100 0 0 1 0\n")
  root.write(fp"proc/{pid}/status", f"Name:\tsample\nUid:\t1000\t1000\t1000\t1000\n")
}

test test_btop_cpu_source_reads_complete_aggregate_uptime_load_and_sparse_cores {
  let root = fs.tempdir()?
  defer root.close()
  write_proc_stat(
    root,
    "cpu 160 0 70 970 0 0 0 0 0 0",
    "cpu0 80 0 35 485 0 0 0 0 0 0",
    "cpu1 80 0 35 485 0 0 0 0 0 0",
  )

  let sample = syscpu.collect(root)
  assert sample.stat.state == report.Observed
  assert sample.stat.aggregate?.fields == [160, 0, 70, 970, 0, 0, 0, 0, 0, 0]
  assert [item.id for item in sample.stat.cores] == [0, 1]
  assert sample.uptime.state == report.Observed and sample.uptime.seconds == 25.5
  assert sample.load.state == report.Observed and sample.load.values == [0.1, 0.2, 0.3]
  assert sample.stat.issues == []
}

test test_btop_cpu_source_withholds_a_truncated_stat_prefix {
  let root = fs.tempdir()?
  defer root.close()
  write_proc_stat(root, "cpu 160 0 70 970 0 0 0 0 0 0", "cpu0 80 0 35 485", "cpu1 80 0 35 485")

  let truncated = src.read_source_text(root, p"proc/stat", max_bytes: 12)
  let parsed = syscpu.parse_stat(truncated)
  assert parsed.state == report.Truncated
  assert parsed.aggregate == null
  assert parsed.cores == []
}

test test_btop_cpu_source_marks_malformed_uptime_and_load_without_values {
  let root = fs.tempdir()?
  defer root.close()
  write_proc_stat(
    root,
    "cpu 160 0 70 970 0 0 0 0 0 0",
    "cpu0 80 0 35 485 0 0 0 0 0 0",
    "cpu1 80 0 35 485 0 0 0 0 0 0",
  )
  root.write(p"proc/uptime", "25.50x 100.00\n")
  root.write(p"proc/loadavg", "0.10 0.20 invalid 1/100 42\n")

  let sample = syscpu.collect(root)
  assert sample.uptime.state == report.Malformed and sample.uptime.seconds == null
  assert sample.load.state == report.Malformed and sample.load.values == []
}

test test_btop_memory_source_distinguishes_absent_from_zero_counters {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/meminfo",
    "MemTotal: 1048576 kB\nMemAvailable: 262144 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n",
  )
  let with_swap = memory.collect(root)
  assert with_swap.total.state == report.Observed and with_swap.total.bytes == 1073741824
  assert with_swap.available.state == report.Observed and with_swap.available.bytes == 268435456
  assert with_swap.swap_total.state == report.Observed and with_swap.swap_total.bytes == 0
  assert with_swap.swap_free.state == report.Observed and with_swap.swap_free.bytes == 0

  root.write(p"proc/meminfo", "MemTotal: 1048576 kB\n")
  let without_optional = memory.collect(root)
  assert without_optional.available.state == report.Absent
  assert without_optional.swap_total.state == report.Absent
  assert without_optional.swap_free.state == report.Absent
}

test test_btop_memory_source_marks_malformed_and_truncated_values {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"proc", parents: true)
  root.write(p"proc/meminfo", "MemTotal: nope kB\nMemAvailable: 262144 kB\n")
  let malformed = memory.collect(root)
  assert malformed.total.state == report.Malformed
  assert malformed.total.bytes == null
  assert malformed.available.bytes == 268435456

  let truncated = memory.parse(src.read_source_text(root, p"proc/meminfo", max_bytes: 4))
  assert truncated.total.state == report.Truncated
  assert truncated.total.bytes == null
}

test test_btop_memory_source_keeps_overlapping_categories_as_raw_counters {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/meminfo",
    "MemTotal: 100000 kB\nMemFree: 10000 kB\nMemAvailable: 20000 kB\nBuffers: 3000 kB\nCached: 9000 kB\nActive: 50000 kB\nInactive: 35000 kB\nDirty: 250 kB\nWriteback: 125 kB\nSwapTotal: 1000 kB\nSwapFree: 900 kB\n",
  )

  let observed = memory.collect(root)
  assert observed.total.bytes == 102400000
  assert observed.free.bytes == 10240000
  assert observed.available.bytes == 20480000
  assert observed.buffers.bytes == 3072000
  assert observed.cached.bytes == 9216000
  assert observed.active.bytes == 51200000
  assert observed.inactive.bytes == 35840000
  assert observed.dirty.bytes == 256000
  assert observed.writeback.bytes == 128000
  assert observed.total.bytes == 100000 * 1024
}

test test_btop_memory_source_withholds_duplicate_fields {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/meminfo",
    "MemTotal: 1024 kB\nMemAvailable: 512 kB\nMemTotal: 4096 kB\n",
  )

  let duplicate = memory.collect(root)
  assert duplicate.total.state == report.Malformed
  assert duplicate.total.bytes == null
  assert duplicate.total.error_kind == "duplicate_field"
  assert duplicate.available.bytes == 524288
}

test test_btop_memory_source_checks_large_kilobyte_conversions {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"proc", parents: true)
  root.write(p"proc/meminfo", "MemTotal: 8796093022207 kB\nMemAvailable: 1 kB\n")
  let largest_exact = memory.collect(root)
  assert largest_exact.total.state == report.Observed
  assert largest_exact.total.bytes == 9007199254739968

  root.write(p"proc/meminfo", "MemTotal: 8796093022208 kB\nMemAvailable: 1 kB\n")
  let out_of_range = memory.collect(root)
  assert out_of_range.total.state == report.RangeFailure
  assert out_of_range.total.bytes == null
}

test test_btop_network_source_selects_name_order_and_skips_loopback {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"proc/net", parents: true)
  root.write(
    p"proc/net/dev",
    "Inter-| Receive                                                | Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\nlo: 10 1 0 0 0 0 0 0 20 1 0 0 0 0 0 0\neth1: 30 1 0 0 0 0 0 0 40 1 0 0 0 0 0 0\neth0: 50 1 0 0 0 0 0 0 60 1 0 0 0 0 0 0\n",
  )
  root.mkdir(p"sys/class/net/eth0", parents: true)
  root.write(p"sys/class/net/eth0/ifindex", "2\n")
  root.mkdir(p"sys/class/net/eth1", parents: true)
  root.write(p"sys/class/net/eth1/ifindex", "3\n")
  root.mkdir(p"sys/class/net/lo", parents: true)
  root.write(p"sys/class/net/lo/ifindex", "1\n")

  let sample = network.collect(root)
  assert sample.state == report.Observed
  assert [item.name for item in sample.interfaces] == ["eth0", "eth1", "lo"]
  let selected = network.select_interface(sample.interfaces)?
  assert selected.name == "eth0"
  assert selected.ifindex == 2
  assert selected.rx_bytes == 50 and selected.tx_bytes == 60
}

test test_btop_network_selection_uses_loopback_when_it_is_the_only_interface {
  let selected = network.selected_interface([
    {name: "lo", ifindex: 1, rx_bytes: 64, tx_bytes: 64},
  ])
  assert selected?.name == "lo"
}

test test_btop_network_source_distinguishes_empty_from_unavailable {
  let root = fs.tempdir()?
  defer root.close()
  let missing = network.collect(root)
  assert missing.state == report.Absent and missing.interfaces == []
  root.mkdir(p"proc/net", parents: true)
  root.write(p"proc/net/dev", "Inter-| Receive | Transmit\n face |bytes packets|bytes packets\n")
  let empty = network.collect(root)
  assert empty.state == report.Observed and empty.interfaces == []
}

test test_btop_network_identity_requires_a_positive_sysfs_index {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"proc/net", parents: true)
  root.mkdir(p"sys/class/net/eth0", parents: true)
  root.write(
    p"proc/net/dev",
    "Inter-| Receive | Transmit\n face |bytes packets|bytes packets\neth0: 10 1 0 0 0 0 0 0 20 1 0 0 0 0 0 0\n",
  )
  root.write(p"sys/class/net/eth0/ifindex", "0\n")
  let invalid = network.collect(root)
  assert invalid.interfaces[0].ifindex == null
  assert invalid.issues[0].state == report.RangeFailure

  root.remove(p"sys/class/net/eth0/ifindex")
  let absent = network.collect(root)
  assert absent.interfaces[0].ifindex == null
  assert absent.issues[0].state == report.Absent
}

test test_btop_process_source_keeps_identity_uid_cpu_ticks_and_rss {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"proc", parents: true)
  write_process(root, 42, 1000, 35, 16384)

  let sample = processes.collect(root, page_size_bytes: 4096)
  assert sample.state == report.Observed
  assert sample.issues == []
  assert sample.processes.len() == 1
  let item = sample.processes[0]
  assert item.pid == 42 and item.start_ticks == 1000
  assert item.parent_pid == 1 and item.thread_count == 1
  assert item.command == "sample (worker)"
  assert item.uid == 1000 and item.cpu_ticks == 35
  assert item.rss_bytes == 67108864
}

test test_btop_process_source_keeps_i64_start_identity_outside_report_json_v1 {
  let root = fs.tempdir()?
  defer root.close()
  let maximum = 9223372036854775807
  let text = process_stat(42, maximum, 10, 0, 2)

  assert system_live.parse_proc_stat(text) is Err(_)
  let monitor_stat = system_live.parse_proc_stat_for_monitor(text)?
  assert monitor_stat.start_ticks == maximum

  write_process(root, 42, maximum, 10, 2)
  let sample = processes.collect(root, page_size_bytes: 4096)
  assert sample.processes[0].start_ticks == maximum
}

test test_btop_process_stat_keeps_a_command_with_embedded_parentheses_and_spaces {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"proc", parents: true)
  root.mkdir(p"proc/42", parents: true)
  root.write(p"proc/42/stat", process_stat(42, 1000, 35, 0, 16384, command: "worker) with spaces (v2"))
  root.write(p"proc/42/statm", "65536 16384 100 0 0 1 0\n")
  root.write(p"proc/42/status", "Name:\tworker\nUid:\t1000\t1000\t1000\t1000\n")

  let sample = processes.collect(root, page_size_bytes: 4096)
  assert sample.processes.len() == 1
  assert sample.processes[0].command == "worker) with spaces (v2"
}

test test_btop_process_read_tape_withholds_pid_reuse_and_disappearance {
  let statm = observed_source("65536 16384 100 0 0 1 0\n")
  let status = observed_source("Name:\tworker\nUid:\t1000\t1000\t1000\t1000\n")
  let raced = processes.process_read_from_tape(
    42,
    4096,
    {
      before_stat: observed_source(process_stat(42, 1000, 35, 0, 16384)),
      statm: statm,
      status: status,
      after_stat: observed_source(process_stat(42, 2000, 40, 0, 16384)),
    },
  )
  assert raced.process == null
  assert raced.issues[0].state == report.Raced

  let disappeared = processes.process_read_from_tape(
    42,
    4096,
    {
      before_stat: observed_source(process_stat(42, 1000, 35, 0, 16384)),
      statm: statm,
      status: status,
      after_stat: {observation: src.empty_text(report.Absent), errno: null, error_kind: null},
    },
  )
  assert disappeared.process == null
  assert disappeared.issues[0].state == report.Disappeared
}

test test_btop_local_command_line_is_identity_checked_and_never_captured {
  let root = fs.tempdir()?
  defer root.close()
  fixture.write_b3_frame(
    root,
    "frame",
    0,
    [{pid: 42, start_ticks: 1000, cpu_ticks: 10, rss_pages: 2, uid: 1000}],
  )
  let frame_root = root.open_root(p"frame")?
  defer frame_root.close()
  frame_root.write(p"proc/42/cmdline", b"worker\0--secret-argument\0")
  frame_root.write(p"proc/42/environ", b"BTOP_PRIVATE_ENV=do-not-render\0")

  let observation = btop_source.capture(frame_root, "private-fixture", 1000000000, 4096, 100)?
  let value = model.step(observation, null)
  let local_display = processes.collect_local_display(frame_root, observation.processes.processes)
  assert local_display.len() == 1
  assert local_display[0].command_line == "worker --secret-argument"
  let screen = renderer.render_view(value, ui.initial(value, renderer.process_capacity(40)), 120, 40, local_display)?
  assert "worker --se" in screen.lines.join("\n")
  assert "BTOP_PRIVATE_ENV" not in screen.lines.join("\n")
  let observation_json = json.encode(observation)?
  let model_json = json.encode(value)?
  assert "--secret-argument" not in observation_json
  assert "--secret-argument" not in model_json
  assert "BTOP_PRIVATE_ENV" not in observation_json
  assert "BTOP_PRIVATE_ENV" not in model_json

  frame_root.write(p"proc/42/stat", process_stat(42, 2000, 10, 0, 2))
  let changed_identity = processes.collect_local_display(frame_root, observation.processes.processes)
  assert changed_identity[0].command_line == null
}

test test_btop_process_average_cpu_uses_process_lifetime_ticks {
  let root = fs.tempdir()?
  defer root.close()
  fixture.write_b3_frame(
    root,
    "frame",
    1,
    [
      {pid: 42, start_ticks: 1000, cpu_ticks: 11, rss_pages: 2, uid: 1000},
      {pid: 43, start_ticks: 2000, cpu_ticks: 45, rss_pages: 2, uid: 1000},
      {pid: 44, start_ticks: 3000, cpu_ticks: 40, rss_pages: 2, uid: 1000},
    ],
  )
  let frame_root = root.open_root(p"frame")?
  defer frame_root.close()
  let observation = btop_source.capture(frame_root, "average-fixture", 1000000000, 4096, 100)?
  let value = model.step(observation, null)
  assert value.processes[0].pid == 42
  assert value.processes[0].cpu_average_percent.state == rates.Observed
  assert (value.processes[0].cpu_average_percent.value ?? 0.0) > 0.70
  assert (value.processes[0].cpu_average_percent.value ?? 0.0) < 0.72
  assert value.processes[1].pid == 43
  assert (value.processes[1].cpu_average_percent.value ?? 0.0) > 8.17
  assert (value.processes[1].cpu_average_percent.value ?? 0.0) < 8.19
  assert value.processes[2].pid == 44
  assert value.processes[2].cpu_average_percent.state == rates.Invalid
}

test test_btop_process_source_exposes_a_reused_pid_as_a_new_identity {
  let root = fs.tempdir()?
  defer root.close()
  root.mkdir(p"proc", parents: true)
  write_process(root, 42, 1000, 35, 16384)
  let first = processes.collect(root, page_size_bytes: 4096)
  assert first.processes[0].start_ticks == 1000

  root.write(p"proc/42/stat", process_stat(42, 2000, 40, 0, 16384))
  let second = processes.collect(root, page_size_bytes: 4096)
  assert second.processes[0].start_ticks == 2000
  assert second.processes[0].cpu_ticks == 40
}

test test_btop_rates_match_the_independent_gate_one_oracle {
  let previous_cpu: syscpu.CpuCounters = {fields: [100, 0, 50, 850, 0, 0, 0, 0, 0, 0]}
  let current_cpu: syscpu.CpuCounters = {fields: [160, 0, 70, 970, 0, 0, 0, 0, 0, 0]}
  let aggregate = rates.cpu_percent(current_cpu, previous_cpu)
  assert aggregate.state == rates.Observed and aggregate.value == 40.0
  assert rates.process_percent(35, 10, current_cpu, previous_cpu, 2, per_core: false).value == 12.5
  assert rates.process_percent(35, 10, current_cpu, previous_cpu, 2, per_core: true).value == 25.0
  assert rates.counter_rate(1010240, 1000000, elapsed_ns: 1000000000).value == 10240.0
  assert rates.used_bytes(1073741824, 268435456).value == 805306368
  assert rates.pages_to_bytes(16384, 4096) == 67108864
}

test test_btop_rates_distinguish_warming_idle_reset_and_invalid_interval {
  let previous: syscpu.CpuCounters = {fields: [100, 0, 50, 850, 0, 0, 0, 0, 0, 0]}
  let idle: syscpu.CpuCounters = {fields: [100, 0, 50, 950, 0, 0, 0, 0, 0, 0]}
  let reset: syscpu.CpuCounters = {fields: [90, 0, 50, 950, 0, 0, 0, 0, 0, 0]}
  assert rates.cpu_percent(idle, null).state == rates.Warming
  assert rates.cpu_percent(idle, previous).value == 0.0
  assert rates.cpu_percent(reset, previous).state == rates.Reset
  assert rates.counter_rate(4, 5, elapsed_ns: 1000000000).state == rates.Reset
  assert rates.counter_rate(5, 4, elapsed_ns: 0).state == rates.Invalid
  assert rates.used_bytes(10, null).state == rates.Unavailable
}

test test_btop_counter_precision_preserves_small_deltas_at_large_bases {
  let base = 9007199254740992
  let previous_cpu: syscpu.CpuCounters = {fields: [base, 0, 0, base, 0, 0, 0, 0, 0, 0]}
  let current_cpu: syscpu.CpuCounters = {fields: [base + 10, 0, 0, base + 10, 0, 0, 0, 0, 0, 0]}
  assert rates.cpu_percent(current_cpu, previous_cpu).value == 50.0
  assert rates.process_percent(base + 10, base, current_cpu, previous_cpu, 2).value == 50.0
  assert rates.counter_rate(base + 100, base, elapsed_ns: 1000000000).value == 100.0

  let maximum = 9223372036854775807
  assert rates.counter_rate(maximum, maximum - 10, elapsed_ns: 10).value == 1000000000.0
  let at_limit = syscpu.parse_stat(observed_source(f"cpu {maximum} 0 0 0\n"))
  assert at_limit.state == report.Observed
  assert at_limit.aggregate?.fields == [maximum, 0, 0, 0]

  let out_of_range = syscpu.parse_stat(observed_source("cpu 9223372036854775808 0 0 0\n"))
  assert out_of_range.aggregate == null
  assert out_of_range.issues[0].state == report.RangeFailure
}

test test_btop_counter_precision_withholds_aggregate_sum_overflow {
  let half = 4611686018427387903
  let previous: syscpu.CpuCounters = {fields: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]}
  let current: syscpu.CpuCounters = {fields: [half, half, half, half, half, half, half, half, 0, 0]}
  assert rates.cpu_percent(current, previous).state == rates.Invalid
  assert rates.cpu_percent(current, current).state == rates.Invalid
}

test test_btop_cpu_usage_excludes_guest_time_and_corrects_iowait_regression {
  let previous: syscpu.CpuCounters = {fields: [100, 20, 30, 200, 10, 10, 5, 5, 50, 10]}
  let current: syscpu.CpuCounters = {fields: [200, 30, 40, 300, 20, 15, 10, 10, 150, 15]}
  let usage = rates.cpu_percent(current, previous)
  assert usage.state == rates.Observed
  assert usage.value?.format(precision: 1) == "21.4"

  let wait_before: syscpu.CpuCounters = {fields: [100, 0, 0, 50, 20, 0, 0, 0, 0, 0]}
  let wait_after: syscpu.CpuCounters = {fields: [100, 0, 0, 100, 10, 0, 0, 0, 0, 0]}
  assert rates.cpu_percent(wait_after, wait_before).value == 0.0

  let reset: syscpu.CpuCounters = {fields: [90, 20, 30, 300, 20, 15, 10, 10, 150, 15]}
  assert rates.cpu_percent(reset, previous).state == rates.Reset
}
