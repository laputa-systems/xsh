use core.lib.btop_rates as rates
use core.lib.sys_cpu as syscpu
use core.lib.sys_mem as memory
use core.lib.sys_net as network
use core.lib.sys_proc as processes
use core.lib.sys_source as src
use core.lib.system_report as report

proc write_proc_stat(root: FsRoot, aggregate: Str, cpu0: Str, cpu1: Str) [fs, error] {
  root.mkdir(p"proc", parents: true)
  root.write(
    p"proc/stat",
    f"{aggregate}\n{cpu0}\n{cpu1}\nintr 1\nctxt 2\nbtime 1700000000\nprocesses 3\nprocs_running 1\nprocs_blocked 0\nsoftirq 4 0 0 0 0 0 0 0 0 0 0\n",
  )
  root.write(p"proc/uptime", "25.50 100.00\n")
  root.write(p"proc/loadavg", "0.10 0.20 0.30 1/100 42\n")
}

proc process_stat(pid: Int, start_ticks: Int, user_ticks: Int, system_ticks: Int, rss_pages: Int) -> Str {
  f"{pid} (sample (worker)) S 1 {pid} {pid} 0 -1 4194560 100 0 0 0 {user_ticks} {system_ticks} 0 0 20 1 1 0 {start_ticks} 16777216 {rss_pages} 0 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n"
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
  assert item.command == "sample (worker)"
  assert item.uid == 1000 and item.cpu_ticks == 35
  assert item.rss_bytes == 67108864
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
