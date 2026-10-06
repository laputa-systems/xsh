use core.lib.procps

pure row(pid: Int, name: Str, argv: Str, uid: Int = 1000, parent: Int = 1) -> ProcessEntry {
  {pid: pid, parent_pid: parent, command: name, argv: argv, argv0: name, user: "fixture", uid: uid, status: "S", start_time: "", start_time_ms: pid * 1000, runtime_seconds: 10, user_ticks: null, system_ticks: null, cpu_ticks: null, start_ticks: null, ticks_per_second: null, rss_bytes: null, vsize_bytes: null, pgrp: null, session: null, tty_number: null, nice: null, priority: null, thread_count: null, processor: null, tty: null}
}

test test_procps_selector_matches_fields_and_excludes_self {
  let rows = [row(1, "worker", "worker --serve"), row(2, "helper", "helper worker"), row(3, "worker2", "worker2")]
  let base = procps.selector()
  assert [p.pid for p in procps.select(rows, {...base, pattern: "worker", exact: true}, 99)?] == [1]
  assert [p.pid for p in procps.select(rows, {...base, pattern: "worker", full: true}, 99)?] == [1, 2, 3]
  assert [p.pid for p in procps.select(rows, {...base, pattern: "worker"}, 1)?] == [3]
  assert [p.pid for p in procps.select(rows, {...base, pids: [2]}, 99)?] == [2]
  assert [p.pid for p in procps.select(rows, {...base, users: ["1000"], parents: [1]}, 99)?] == [1, 2, 3]
  assert [p.pid for p in procps.select(rows, {...base, pattern: "worker", newest: true}, 99)?] == [3]
  assert [p.pid for p in procps.select(rows, {...base, pattern: "worker", oldest: true}, 99)?] == [1]
  assert procps.select(rows, {...base, pattern: "["}, 99) is Err(_)
  assert procps.select(rows, {...base, groups: [1]}, 99) is Err(_)
  let tied = [row(2, "worker", "worker"), {...row(1, "worker", "worker"), start_time_ms: 2000}]
  assert procps.select(tied, {...base, newest: true}, 99)?[0].pid == 2
  assert procps.select(tied, {...base, oldest: true}, 99)?[0].pid == 1
}

test test_procps_pid_lists_reject_bad_ids {
  assert procps.ids("1,20")? == [1, 20]
  for input in ["", "1,", "-1", "0", "abc"] { assert procps.ids(input) is Err(_), input }
}

test test_procps_cli_selects_owned_child { |ctx|
  let ready = test.temp_path(ctx, name: "owned-child.ready")
  let release = test.temp_path(ctx, name: "owned-child.release")
  let fixture = test.temp_file(ctx, name: "owned-child.xsh", contents: bytes.from_text("""
fp"{args[0]}".write(f"{process.current_pid()?}")
let release = fp"{args[1]}"
while ! release.exists()? { time.sleep(10ms) }
"""))?
  let child = spawn run ${ctx.xsh_bin} $fixture -- $ready $release ?
  # The marker is written after exec and interpreter naming; observing a process
  # row alone can catch a command name that changes during program startup.
  repeat 20 times {
    break when ready.exists()?
    time.sleep(10ms)
  }
  assert ready.exists()?, "owned child did not announce readiness"
  assert ready.read_text()?.trim() == f"{child.pid}"
  let owned: List[ProcessEntry] = process.list()? |> where .pid == child.pid |> collect
  assert owned.len() == 1
  let parent = owned[0].parent_pid
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pgrep.xsh" -- -P $parent -x ${owned[0].command}
  assert f"{child.pid}" in output.split("\n")
  let delivered = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/pkill.xsh" -- --signal 0 -P $parent -x ${owned[0].command}
  assert delivered.exited_with(0)
  let ps = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/ps.xsh" -- -p ${child.pid} -o pid=,comm=
  assert ps.trim() == f"{child.pid} {owned[0].command}"
  let pidof = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/pidof.xsh" -- ${owned[0].command}
  assert f"{child.pid}" in pidof.trim().split(" ")
  release.write("done")
  assert (wait child?).exited_with(0)
  test.error_kind(process.kill(child.pid, signal: "0"), "process-missing")
}

test test_procps_cli_reports_no_matches { |ctx|
  let status = run.status ${ctx.xsh_bin} fp"{ctx.core_dir}/pgrep.xsh" -- -x xsh_fixture_name_that_cannot_exist
  assert status.exited_with(1)
  let bad_regex = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/pgrep.xsh" -- "["
  assert bad_regex.status.exited_with(2)
  let unknown = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/ps.xsh" -- -o invented
  assert ! unknown.status.exited_with(0)
}

test test_free_renders_memory_units_and_bounded_samples { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/free.xsh" -- -m
  assert "Mem:" in output and "Swap:" in output and "available" in output
  let samples = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/free.xsh" -- -h -s 0.01 -c 2
  assert samples.split("Mem:").len() == 3
  assert "Gi" in samples or "Mi" in samples
}


test test_procps_cpu_sampling_counts_ticks_and_rejects_reset {
  let first: LinuxCpuSample = {user: 20, nice: 5, system: 10, idle: 50, iowait: 5, irq: 4, softirq: 4, steal: 2}
  let cpu_usage = procps.cpu_percent(first, null)?
  assert cpu_usage.user == 20.0 and cpu_usage.nice == 5.0 and cpu_usage.system == 10.0 and cpu_usage.irq == 4.0 and cpu_usage.softirq == 4.0 and cpu_usage.idle == 50.0 and cpu_usage.wait == 5.0 and cpu_usage.steal == 2.0
  let second: LinuxCpuSample = {...first, user: 30, idle: 60}
  let interval = procps.cpu_percent(second, first)?
  assert interval.user == 50.0 and interval.idle == 50.0
  assert procps.cpu_percent(first, second) is Err(_)
  let corrected: LinuxCpuSample = {...first, user: 30, iowait: 3}
  assert procps.cpu_percent(corrected, first)?.user == 100.0
  assert procps.sampling_args(["0", "1"]) is Err(_)
}

test test_procps_sampling_tools_render_bounded_samples { |ctx|
  for tool in ["vmstat", "iostat", "pidstat"] {
    let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/{tool}.xsh" -- 0.01 2
    assert ! output.trim().is_empty(), tool
  }
  let top = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/top.xsh" -- -b -n 2 -d 0.01
  assert top.split("Tasks:").len() == 3
  let ps = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/ps.xsh" -- aux
  assert "%CPU" in ps and "RSS" in ps and "TTY" in ps
}


test test_procps_sampling_native_fixture_retains_units {
  let root = fs.tempdir()?
  defer root.close()
  root.write(p"stat", "cpu 20 5 10 50 5 4 4 2\nbtime 1000\nctxt 100\nprocesses 30\nprocs_running 2\nprocs_blocked 1\nintr 50 0 0\n")
  root.write(p"vmstat", "pgpgin 10\npgpgout 20\npswpin 3\npswpout 4\n")
  root.write(p"uptime", "20.25 10.00\n")
  root.write(p"meminfo", "MemTotal: 1000 kB\nMemFree: 100 kB\nMemAvailable: 300 kB\nBuffers: 40 kB\nCached: 80 kB\nShmem: 10 kB\nSReclaimable: 20 kB\nSwapTotal: 400 kB\nSwapFree: 200 kB\n")
  root.write(p"diskstats", "8 0 fixture 1 2 3 4 5 6 7 8 9 10 11\n")
  let snapshot = linux.sample(proc_root: root.host_path()?)?
  assert snapshot.uptime_ms == 20250
  assert snapshot.memory.total == 1024000 and snapshot.memory.shared == 10240
  assert snapshot.page_in_kib == 10 and snapshot.swap_in_pages == 3
  assert snapshot.disks[0].sectors_read == 3 and snapshot.disks[0].writes_completed == 5
  assert snapshot.processes.is_empty()
}

test test_procps_inspection_finds_owned_directory_descriptor { |ctx|
  let root = fs.tempdir()?
  defer root.close()
  let directory = root.host_path()?
  let pid = process.current_pid()?
  let listing = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/lsof.xsh" -- -p $pid $directory
  assert "DIR" in listing and f"{pid}" in listing
  let terse = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/lsof.xsh" -- -t -p $pid $directory
  assert terse.trim() == f"{pid}"
  let owners = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/fuser.xsh" -- $directory
  assert f"{pid}" in owners.trim().split(" ")
}


test test_ps_orders_secondary_keys_within_primary_ties { |ctx|
  let output = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/ps.xsh" -- -e -o uid=,pid= --sort uid,-pid
  var previous_uid = -1
  var previous_pid = 9223372036854775807
  for line in output.trim().split("\n") {
    let fields = line.split(" ") |> where . != ""
    let uid = fields[0].parse_int()?
    let pid = fields[1].parse_int()?
    assert uid >= previous_uid
    if uid == previous_uid { assert pid < previous_pid }
    previous_uid = uid
    previous_pid = pid
  }
}

test test_procps_watch_stops_after_command_error_and_killall_reports_no_match { |ctx|
  let watched = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/watch.xsh" -- -x -e -t false
  assert watched.status.exited_with(1)
  assert "\u{1b}[H" in watched.stdout
  let absent = run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/killall.xsh" -- xsh_fixture_process_name_that_cannot_exist
  assert absent.status.exited_with(1)
  assert "no process found" in absent.stderr
}
