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

# Process names are unique to the runner (pid) and to the test (checksum of its
# name): tests run concurrently inside one runner, and a name shared between
# them would let one test's pkill signal another test's children. The result
# fits the 15-byte kernel command name.
proc unique_name(ctx: TestContext, tag: Str) [process, error] -> Result[Str, Error] {
  f"xs{process.current_pid()?}{hash.crc32(bytes.from_text(ctx.name)) % 10000:04}{tag}"
}

# A sleeper is an XSH child started through a symlink with a unique name, so
# name selectors under test can only ever match processes this file started. It
# exits on its own after about ten seconds if a test fails before releasing it.
type Sleeper = {name: Str, pid: Int, release: Path, child: ProcessHandle}

proc start_sleeper(ctx: TestContext, tag: Str, token: Str = "", reading: Path? = null, appending: Path? = null) [fs, process, time, error] -> Result[Sleeper, Error] {
  let name = unique_name(ctx, tag)?
  let link = fp"{test.temp_dir(ctx, name: name)?}/{name}"
  fs.symlink(ctx.xsh_bin, link)?
  let ready = test.temp_path(ctx, name: f"{name}.ready")
  let release = test.temp_path(ctx, name: f"{name}.release")
  let script = test.temp_file(ctx, name: f"{name}.xsh", contents: bytes.from_text("""
fp"{args[0]}".write("up")
let release = fp"{args[1]}"
repeat 1000 times {
  break when release.exists()?
  time.sleep(10ms)
}
"""))?
  let child = if reading != null {
    spawn run $link $script -- $ready $release $token < $reading ?
  } else if appending != null {
    spawn run $link $script -- $ready $release $token >> $appending ?
  } else {
    spawn run $link $script -- $ready $release $token ?
  }
  repeat 200 times {
    break when ready.exists()?
    time.sleep(10ms)
  }
  assert ready.exists()?, f"sleeper {name} did not announce readiness"
  {name: name, pid: child.pid, release: release, child: child}
}

proc alive(pid: Int) [process] -> Bool {
  process.kill(pid, signal: "0") is Ok(_)
}

proc stop(sleeper: Sleeper) [fs, error] {
  sleeper.release.write("done")
}

type Captured = {status: Status, stdout: Str, stderr: Str}

# Runs the killall applet with a plain, well-formed argument vector.
proc killall(ctx: TestContext, args: List[Str]) [process, error] -> Result[Captured, Error] {
  run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/killall.xsh" -- @args
}

# Runs the pkill applet with a plain, well-formed argument vector.
proc pkill(ctx: TestContext, args: List[Str]) [process, error] -> Result[Captured, Error] {
  run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/pkill.xsh" -- @args
}

test test_pkill_signals_only_the_named_child { |ctx|
  let target = start_sleeper(ctx, "a")?
  let bystander = start_sleeper(ctx, "b")?
  defer stop(bystander)
  let sent = pkill(ctx, ["-USR1", "-x", target.name])?
  assert sent.status.exited_with(0)
  let status = wait target.child?
  assert status.signaled() and status.signal_number()? == 10
  assert alive(bystander.pid)
}

test test_pkill_sends_term_by_default_and_honours_signal_option { |ctx|
  let target = start_sleeper(ctx, "a")?
  let named = start_sleeper(ctx, "b")?
  assert pkill(ctx, ["-x", target.name])?.status.exited_with(0)
  assert ! (wait target.child?).exited_with(0)
  assert pkill(ctx, ["--signal", "HUP", "-x", named.name])?.status.exited_with(0)
  let status = wait named.child?
  assert status.signaled() and status.signal_number()? == 1
}

test test_pkill_name_and_full_command_matching { |ctx|
  let token = f"needle-{process.current_pid()?}"
  let target = start_sleeper(ctx, "a", token: token)?
  defer stop(target)
  let prefix = target.name.byte_slice(0, target.name.byte_len() - 1)
  assert pkill(ctx, ["--signal", "0", prefix])?.status.exited_with(0), "an unanchored pattern matches part of the name"
  assert pkill(ctx, ["--signal", "0", "-x", prefix])?.status.exited_with(1), "-x requires the whole name"
  assert pkill(ctx, ["--signal", "0", token])?.status.exited_with(1), "the argument is not part of the process name"
  assert pkill(ctx, ["--signal", "0", "-f", token])?.status.exited_with(0), "-f searches the whole command line"
  assert pkill(ctx, ["--signal", "0", "-i", "-x", target.name.upper()])?.status.exited_with(0)
  assert pkill(ctx, ["--signal", "0", "-x", target.name.upper()])?.status.exited_with(1)
  assert alive(target.pid)
}

test test_pkill_counts_matches_and_selects_newest_or_oldest { |ctx|
  let first = start_sleeper(ctx, "n")?
  let second = start_sleeper(ctx, "n")?
  defer stop(first)
  defer stop(second)
  let counted = pkill(ctx, ["--signal", "0", "-c", "-x", first.name])?
  assert counted.status.exited_with(0) and counted.stdout.trim() == "2"
  assert pkill(ctx, ["-USR1", "-n", "-x", first.name])?.status.exited_with(0)
  assert (wait second.child?).signal_number()? == 10
  assert alive(first.pid)
  let third = start_sleeper(ctx, "n")?
  defer stop(third)
  assert pkill(ctx, ["-USR1", "-o", "-x", first.name])?.status.exited_with(0)
  assert (wait first.child?).signal_number()? == 10
  assert alive(third.pid)
}

test test_pkill_filters_by_user_and_parent { |ctx|
  let target = start_sleeper(ctx, "a")?
  defer stop(target)
  let me = user.current()?
  let owner: List[ProcessEntry] = process.list()? |> where .pid == target.pid |> collect
  let parent = owner[0].parent_pid
  assert pkill(ctx, ["--signal", "0", "-x", "-u", me.name, target.name])?.status.exited_with(0)
  assert pkill(ctx, ["--signal", "0", "-x", "-u", f"{me.uid}", target.name])?.status.exited_with(0)
  assert pkill(ctx, ["--signal", "0", "-x", "-u", f"{me.uid + 1}", target.name])?.status.exited_with(1)
  assert pkill(ctx, ["--signal", "0", "-x", "-P", f"{parent}", target.name])?.status.exited_with(0)
  assert pkill(ctx, ["--signal", "0", "-x", "-P", f"{target.pid}", target.name])?.status.exited_with(1)
  assert alive(target.pid)
}

test test_pkill_filters_by_process_group_and_session { |ctx|
  let target = start_sleeper(ctx, "a")?
  defer stop(target)
  let owner: List[ProcessEntry] = process.list()? |> where .pid == target.pid |> collect
  let pgroup = owner[0].pgrp ?? -1
  let session = owner[0].session ?? -1
  assert pgroup > 0 and session > 0
  assert pkill(ctx, ["--signal", "0", "-x", "-g", f"{pgroup}", target.name])?.status.exited_with(0)
  assert pkill(ctx, ["--signal", "0", "-x", "-s", f"{session}", target.name])?.status.exited_with(0)
  assert pkill(ctx, ["--signal", "0", "-x", "-g", f"{pgroup + 1}", target.name])?.status.exited_with(1)
  assert alive(target.pid)
}

test test_pkill_reports_usage_and_selection_errors_with_distinct_statuses { |ctx|
  assert pkill(ctx, ["-x", "xsh_fixture_name_that_cannot_exist"])?.status.exited_with(1)
  let no_criteria = pkill(ctx, [])?
  assert no_criteria.status.exited_with(2) and ! no_criteria.stderr.is_empty()
  assert pkill(ctx, ["["])?.status.exited_with(2)
  assert pkill(ctx, ["--signal", "NOSUCHSIGNAL", "x"])?.status.exited_with(2)
  assert pkill(ctx, ["-x", "a", "b"])?.status.exited_with(2)
  assert pkill(ctx, ["--no-such-option", "x"])?.status.exited_with(2)
}

test test_pkill_and_killall_never_select_their_own_process { |ctx|
  let name = unique_name(ctx, "s")?
  let link = fp"{test.temp_dir(ctx, name: name)?}/{name}"
  fs.symlink(ctx.xsh_bin, link)?
  let own = run.capture --text $link fp"{ctx.core_dir}/pkill.xsh" -- -x $name
  assert own.status.exited_with(1), "pkill matched its own process"
  let full = run.capture --text $link fp"{ctx.core_dir}/pkill.xsh" -- -f $name
  assert full.status.exited_with(1), "pkill matched its own command line"
  let killed = run.capture --text $link fp"{ctx.core_dir}/killall.xsh" -- $name
  assert killed.status.exited_with(1), "killall matched its own process"
  assert f"{name}: no process found" in killed.stderr
}

test test_killall_signals_exact_names_and_reports_the_signal_number { |ctx|
  let target = start_sleeper(ctx, "a")?
  let bystander = start_sleeper(ctx, "b")?
  defer stop(bystander)
  let prefix = target.name.byte_slice(0, target.name.byte_len() - 1)
  let partial = killall(ctx, [prefix])?
  assert partial.status.exited_with(1), "killall names are exact, not substrings"
  assert f"{prefix}: no process found" in partial.stderr
  assert alive(target.pid)
  let sent = killall(ctx, ["-v", "-s", "USR1", target.name])?
  assert sent.status.exited_with(0)
  assert sent.stderr.trim() == f"Killed {target.name}({target.pid}) with signal 10"
  let status = wait target.child?
  assert status.signaled() and status.signal_number()? == 10
  assert alive(bystander.pid)
}

test test_killall_signal_shorthand_and_default_term { |ctx|
  let hup = start_sleeper(ctx, "a")?
  let term = start_sleeper(ctx, "b")?
  assert killall(ctx, ["-HUP", hup.name])?.status.exited_with(0)
  assert (wait hup.child?).signal_number()? == 1
  assert killall(ctx, [term.name])?.status.exited_with(0)
  assert ! (wait term.child?).exited_with(0)
}

test test_killall_missing_names_fail_after_signaling_the_rest { |ctx|
  let target = start_sleeper(ctx, "a")?
  let absent = unique_name(ctx, "z")?
  let mixed = killall(ctx, ["-s", "USR1", absent, target.name])?
  assert mixed.status.exited_with(1)
  assert f"{absent}: no process found" in mixed.stderr
  assert (wait target.child?).signal_number()? == 10
  let quiet = killall(ctx, ["-q", absent])?
  assert quiet.status.exited_with(1) and quiet.stderr.is_empty()
  assert killall(ctx, [])?.status.exited_with(1)
  let unknown = killall(ctx, ["-s", "NOSUCHSIGNAL", absent])?
  assert unknown.status.exited_with(1) and "NOSUCHSIGNAL: unknown signal" in unknown.stderr
  assert killall(ctx, ["--no-such-option", absent])?.status.exited_with(1), "psmisc reports every usage error with status 1"
}

test test_killall_ignore_case_regexp_and_user_filters { |ctx|
  let target = start_sleeper(ctx, "a")?
  defer stop(target)
  let me = user.current()?
  let prefix = target.name.byte_slice(0, target.name.byte_len() - 1)
  assert killall(ctx, ["-s", "0", target.name.upper()])?.status.exited_with(1)
  assert killall(ctx, ["-s", "0", "-I", target.name.upper()])?.status.exited_with(0)
  assert killall(ctx, ["-s", "0", "-r", f"^{prefix}."])?.status.exited_with(0)
  assert killall(ctx, ["-s", "0", "-r", f"^{prefix}$"])?.status.exited_with(1)
  assert killall(ctx, ["-s", "0", "-u", me.name, target.name])?.status.exited_with(0)
  let other = if me.uid == 0 { "nobody" } else { "root" }
  if user.lookup(other) is Ok(_) {
    assert killall(ctx, ["-s", "0", "-u", other, target.name])?.status.exited_with(1)
  }
  let missing = killall(ctx, ["-s", "0", "-u", "xsh-no-such-user", target.name])?
  assert missing.status.exited_with(1) and "Cannot find user xsh-no-such-user" in missing.stderr
  assert alive(target.pid)
}

# Runs the fuser applet with a plain, well-formed argument vector.
proc fuser(ctx: TestContext, args: List[Str]) [process, error] -> Result[Captured, Error] {
  run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/fuser.xsh" -- @args
}

test test_fuser_lists_the_holders_of_a_file { |ctx|
  let held = test.temp_file(ctx, name: "held.txt", contents: bytes.from_text("held"))?
  let idle = test.temp_file(ctx, name: "idle.txt", contents: bytes.from_text("idle"))?
  let holder = start_sleeper(ctx, "h", reading: held)?
  defer stop(holder)
  let found = fuser(ctx, [f"{held}"])?
  assert found.status.exited_with(0)
  assert found.stdout.fields() == [f"{holder.pid}"]
  assert found.stderr.trim() == f"{held}:"
  let unused = fuser(ctx, [f"{idle}"])?
  assert unused.status.exited_with(1) and unused.stdout.is_empty()
  let missing = fuser(ctx, [f"{idle}.missing"])?
  assert missing.status.exited_with(1) and missing.stdout.is_empty()
  assert missing.stderr.trim() == f"Specified filename {idle}.missing does not exist."
}

test test_fuser_verbose_table_shows_user_pid_access_and_command { |ctx|
  let read_path = test.temp_file(ctx, name: "read.txt", contents: bytes.from_text("r"))?
  let write_path = test.temp_file(ctx, name: "write.txt", contents: bytes.from_text("w"))?
  let reader = start_sleeper(ctx, "r", reading: read_path)?
  let writer = start_sleeper(ctx, "w", appending: write_path)?
  defer stop(reader)
  defer stop(writer)
  let table = fuser(ctx, ["-v", f"{read_path}", f"{write_path}"])?
  assert table.status.exited_with(0)
  let lines = table.stderr.split("\n")
  assert lines[0] == "                     USER        PID ACCESS COMMAND"
  assert lines[1].starts_with(f"{read_path}:") and lines[1].ends_with(f" {reader.pid} f.... {reader.name}")
  assert lines[2].starts_with(f"{write_path}:") and lines[2].ends_with(f" {writer.pid} F.... {writer.name}")
  assert lines.len() == 4 and lines[3].is_empty()
  let pids = table.stdout.fields()
  assert pids == [f"{reader.pid}", f"{writer.pid}"]
}

test test_fuser_reports_executable_holders_with_access_letters { |ctx|
  let holder = start_sleeper(ctx, "e")?
  defer stop(holder)
  let table = fuser(ctx, ["-v", f"{ctx.xsh_bin}"])?
  assert table.status.exited_with(0)
  let mine = table.stderr.lines() |> where .ends_with(f" {holder.pid} ...e. {holder.name}") |> collect
  assert mine.len() == 1, "the sleeper holds the XSH binary only as its executable"
  let plain = fuser(ctx, [f"{ctx.xsh_bin}"])?
  assert f"{holder.pid}" in plain.stdout.fields()
  assert plain.stderr.starts_with(f"{ctx.xsh_bin}:") and plain.stderr.trim().ends_with("e")
}

test test_fuser_silent_all_and_usage_statuses { |ctx|
  let held = test.temp_file(ctx, name: "held.txt", contents: bytes.from_text("h"))?
  let idle = test.temp_file(ctx, name: "idle.txt", contents: bytes.from_text("i"))?
  let holder = start_sleeper(ctx, "h", reading: held)?
  defer stop(holder)
  let quiet = fuser(ctx, ["-s", f"{held}"])?
  assert quiet.status.exited_with(0) and quiet.stdout.is_empty() and quiet.stderr.is_empty()
  let none = fuser(ctx, ["-s", f"{idle}"])?
  assert none.status.exited_with(1) and none.stdout.is_empty() and none.stderr.is_empty()
  let everything = fuser(ctx, ["-a", f"{idle}"])?
  assert everything.status.exited_with(1) and everything.stdout.is_empty()
  assert everything.stderr == f"{idle}:\n"
  let nothing = fuser(ctx, [])?
  assert nothing.status.exited_with(1) and "No process specification given" in nothing.stderr
  assert fuser(ctx, ["-n", "nosuchspace", f"{held}"])?.status.exited_with(1)
}

test test_fuser_mount_lists_holders_of_the_whole_filesystem { |ctx|
  let held = test.temp_file(ctx, name: "held.txt", contents: bytes.from_text("h"))?
  let sibling = test.temp_file(ctx, name: "sibling.txt", contents: bytes.from_text("s"))?
  let holder = start_sleeper(ctx, "h", reading: held)?
  defer stop(holder)
  let exact = fuser(ctx, [f"{sibling}"])?
  assert exact.status.exited_with(1), "only the named file is searched without -m"
  let mount = fuser(ctx, ["-m", f"{sibling}"])?
  assert mount.status.exited_with(0)
  assert f"{holder.pid}" in mount.stdout.fields()
}

test test_fuser_kill_sends_the_requested_signal_to_holders_only { |ctx|
  let first = test.temp_file(ctx, name: "first.txt", contents: bytes.from_text("1"))?
  let second = test.temp_file(ctx, name: "second.txt", contents: bytes.from_text("2"))?
  let target = start_sleeper(ctx, "t", reading: first)?
  let default_target = start_sleeper(ctx, "d", reading: second)?
  let bystander = start_sleeper(ctx, "b")?
  defer stop(bystander)
  let sent = fuser(ctx, ["-k", "-USR1", f"{first}"])?
  assert sent.status.exited_with(0)
  assert (wait target.child?).signal_number()? == 10
  assert fuser(ctx, ["-k", f"{second}"])?.status.exited_with(0)
  assert (wait default_target.child?).signal_number()? == 9, "SIGKILL is the default"
  assert alive(bystander.pid)
}

# A connection this process closed first leaves a TIME_WAIT row with inode 0
# in the host TCP table. Descriptors of other kinds must not be matched to it,
# so lsof still lists a held regular file as REG and never as sock.
test test_lsof_lists_a_regular_file_as_reg_while_tcp_time_wait_rows_exist { |ctx|
  let c = linux.net_constants()
  let listener = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  defer unix.close_fd(listener)
  linux.bind(listener, {family: "inet", address: "127.0.0.1", port: 0})?
  linux.listen(listener, 1)?
  let address = linux.getsockname(listener)?
  let client = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  linux.connect(client, address)?
  let peer = linux.accept(listener)?
  unix.close_fd(peer.fd)?
  unix.close_fd(client)?

  let held = test.temp_file(ctx, name: "lsof-held.txt", contents: bytes.from_text("held"))?
  let holder = start_sleeper(ctx, "l", reading: held)?
  defer stop(holder)
  let pid = holder.pid
  let operand = f"{held}"
  let listing = run.text ${ctx.xsh_bin} fp"{ctx.core_dir}/lsof.xsh" -- -p $pid $operand
  let rows = listing.lines() |> where operand in . |> collect
  assert rows.len() == 1, listing
  assert " REG " in rows[0] and " sock " not in rows[0], rows[0]
}

test test_fuser_namespace_finds_the_owner_of_a_listening_port { |ctx|
  let c = linux.net_constants()
  let server = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  defer unix.close_fd(server)
  linux.bind(server, {family: "inet", address: "127.0.0.1", port: 0})?
  linux.listen(server, 1)?
  let port = linux.getsockname(server)?.port
  let me = f"{process.current_pid()?}"
  let named = fuser(ctx, ["-n", "tcp", f"{port}"])?
  assert named.status.exited_with(0)
  assert me in named.stdout.fields()
  assert named.stderr.starts_with(f"{port}/tcp:")
  let suffix = fuser(ctx, [f"{port}/tcp"])?
  assert suffix.status.exited_with(0) and me in suffix.stdout.fields()
  assert fuser(ctx, ["-n", "udp", f"{port}"])?.status.exited_with(1)
  let closed = linux.socket(c.AF_INET, c.SOCK_STREAM)?
  linux.bind(closed, {family: "inet", address: "127.0.0.1", port: 0})?
  linux.listen(closed, 1)?
  let gone = linux.getsockname(closed)?.port
  unix.close_fd(closed)?
  assert fuser(ctx, ["-n", "tcp", f"{gone}"])?.status.exited_with(1)
}

# The sampling applets read the host's /proc, which no fake covers, so these
# tests pin the report layout (headers, column counts, number formats and the
# number of reports) and never a host-dependent value.
proc sample_tool(ctx: TestContext, tool: Str, args: List[Str]) [process, error] -> Result[Captured, Error] {
  run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/{tool}.xsh" -- @args
}

pure is_fixed_point(word: Str) -> Bool {
  rx"^[0-9]+\.[0-9]{2}$".matches(word)
}

pure are_fixed_point(words: List[Str]) -> Bool {
  for word in words {
    return false when ! is_fixed_point(word)
  }
  true
}

pure data_rows(text: Str) -> List[Str] {
  var rows: List[Str] = []
  for line in text.lines() {
    rows = rows.push(line) when ! line.starts_with("UID")
  }
  rows
}

test test_iostat_prints_cpu_and_device_blocks_for_each_report { |ctx|
  let output = sample_tool(ctx, "iostat", ["0.01", "2"])?
  assert output.status.exited_with(0)
  let lines = output.stdout.lines()
  assert (lines |> where . == "avg-cpu: %user %nice %system %iowait %steal %idle" |> collect).len() == 2
  assert (lines |> where . == "Device tps kB_read/s kB_wrtn/s kB_read kB_wrtn" |> collect).len() == 2
  var in_devices = false
  var cpu_next = false
  for line in lines {
    if line.starts_with("avg-cpu:") { cpu_next = true; continue }
    if cpu_next {
      let shares = line.fields()
      assert shares.len() == 6 and are_fixed_point(shares), line
      var total = 0.0
      for share in shares { total = total + share.parse_float()? }
      # An interval too short to cross a kernel tick reports all zeros.
      assert total == 0.0 or (total > 99.9 and total < 100.1), "the six CPU shares add up to the whole interval"
      cpu_next = false
      continue
    }
    if line.starts_with("Device") { in_devices = true; continue }
    if line.is_empty() { in_devices = false; continue }
    assert in_devices, f"unexpected line {line}"
    let columns = line.fields()
    assert columns.len() == 6, line
    assert is_fixed_point(columns[1]) and is_fixed_point(columns[2]) and is_fixed_point(columns[3])
    assert columns[4].parse_int() is Ok(_) and columns[5].parse_int() is Ok(_)
  }
}

test test_iostat_selects_blocks_and_units_with_options { |ctx|
  let cpu_only = sample_tool(ctx, "iostat", ["-c", "0.01", "1"])?
  assert cpu_only.status.exited_with(0) and "avg-cpu:" in cpu_only.stdout and "Device" not in cpu_only.stdout
  let devices_only = sample_tool(ctx, "iostat", ["-d", "0.01", "1"])?
  assert devices_only.status.exited_with(0) and "Device tps kB_read/s kB_wrtn/s kB_read kB_wrtn" in devices_only.stdout
  assert "avg-cpu" not in devices_only.stdout
  let mebibytes = sample_tool(ctx, "iostat", ["-d", "-m", "0.01", "1"])?
  assert "Device tps MB_read/s MB_wrtn/s MB_read MB_wrtn" in mebibytes.stdout
  let skipped = sample_tool(ctx, "iostat", ["-y", "-c", "0.01", "2"])?
  assert skipped.status.exited_with(0)
  assert (skipped.stdout.lines() |> where .starts_with("avg-cpu:") |> collect).len() == 2, "-y drops the since-boot report, then COUNT reports follow"
  assert sample_tool(ctx, "iostat", ["0", "1"])?.status.exited_with(0) == false
  assert sample_tool(ctx, "iostat", ["1", "2", "3"])?.status.exited_with(0) == false
}

test test_pidstat_reports_one_row_per_selected_process_and_report { |ctx|
  let first = start_sleeper(ctx, "a")?
  let second = start_sleeper(ctx, "b")?
  defer stop(first)
  defer stop(second)
  let me = user.current()?
  let one = sample_tool(ctx, "pidstat", ["-p", f"{first.pid}", "0.01", "2"])?
  assert one.status.exited_with(0)
  let lines = one.stdout.lines()
  assert (lines |> where . == "UID PID %usr %system %CPU CPU Command" |> collect).len() == 2
  let rows = data_rows(one.stdout)
  assert rows.len() == 2, "one row per report for the selected pid"
  for row in rows {
    let columns = row.fields()
    assert columns.len() == 7, row
    assert columns[0] == f"{me.uid}" and columns[1] == f"{first.pid}" and columns[6] == first.name
    assert is_fixed_point(columns[2]) and is_fixed_point(columns[3]) and is_fixed_point(columns[4])
    assert columns[5].parse_int() is Ok(_)
  }
  let both = sample_tool(ctx, "pidstat", ["-p", f"{first.pid},{second.pid}", "0.01", "1"])?
  let pids = [row.fields()[1] for row in data_rows(both.stdout)]
  assert pids == [f"{first.pid}", f"{second.pid}"] or pids == [f"{second.pid}", f"{first.pid}"]
}

test test_pidstat_command_filter_and_memory_columns { |ctx|
  let target = start_sleeper(ctx, "m")?
  defer stop(target)
  let report = sample_tool(ctx, "pidstat", ["-r", "-C", f"^{target.name}$", "0.01", "1"])?
  assert report.status.exited_with(0)
  let lines = report.stdout.lines()
  assert lines[0] == "UID PID VSZ RSS %MEM Command"
  let rows = data_rows(report.stdout)
  assert rows.len() == 1
  let columns = rows[0].fields()
  assert columns.len() == 6 and columns[1] == f"{target.pid}" and columns[5] == target.name
  assert columns[2].parse_int()? > 0 and columns[3].parse_int()? > 0 and is_fixed_point(columns[4])
  let none = sample_tool(ctx, "pidstat", ["-C", f"^{target.name}x$", "0.01", "1"])?
  assert none.status.exited_with(0)
  assert none.stdout.trim() == "UID PID %usr %system %CPU CPU Command"
  assert sample_tool(ctx, "pidstat", ["-p", "0", "0.01", "1"])?.status.exited_with(0) == false
  assert sample_tool(ctx, "pidstat", ["0", "1"])?.status.exited_with(0) == false
}

# Each run of this command appends to a counter. `change_at` makes the output
# differ from that run onward, `fail_at` makes it fail from that run onward, and
# `numbered` appends the run number to the output (which then always differs).
proc counting_command(ctx: TestContext, name: Str) [fs, error] -> Result[Path, Error] {
  test.temp_file(ctx, name: name, contents: bytes.from_text("""
let counter = fp"{args[0]}"
let seen = (if counter.exists()? { counter.read_text()?.trim().parse_int()? } else { 0 }) + 1
counter.write(f"{seen}")
let change_at = args[1].parse_int()?
let fail_at = args[2].parse_int()?
print f"{if seen >= change_at { "changed" } else { "same" }}{if args[3] == "numbered" { f" {seen}" } else { "" }}"
if seen >= fail_at { exit 7 }
"""))
}

proc watch(ctx: TestContext, args: List[Str]) [process, error] -> Result[Captured, Error] {
  run.capture --text ${ctx.xsh_bin} fp"{ctx.core_dir}/watch.xsh" -- @args
}

test test_watch_reruns_the_command_each_interval_until_errexit { |ctx|
  let script = counting_command(ctx, "count.xsh")?
  let counter = test.temp_path(ctx, name: "count")
  let run_log = watch(ctx, ["-x", "-e", "-n", "0.1", f"{ctx.xsh_bin}", f"{script}", f"{counter}", "99", "3", "numbered"])?
  assert run_log.status.exited_with(1)
  let screens = run_log.stdout.split("\u{1b}[H\u{1b}[2J") |> drop(1) |> collect
  assert screens.len() == 3, "every run repaints the screen once"
  for index in [0, 1, 2] {
    let screen = screens[index]
    assert screen.starts_with("Every 0.1s: ")
    assert f"{script} {counter} 99 3 numbered" in screen.split("\n")[0]
    assert screen.trim().ends_with(f"same {index + 1}")
  }
  assert counter.read_text()? == "3"
}

test test_watch_no_title_and_interval_floor { |ctx|
  let script = counting_command(ctx, "count.xsh")?
  let counter = test.temp_path(ctx, name: "count")
  let quick = watch(ctx, ["-x", "-t", "-e", "-n", "0", f"{ctx.xsh_bin}", f"{script}", f"{counter}", "99", "2", "numbered"])?
  assert quick.status.exited_with(1)
  assert "Every" not in quick.stdout
  assert quick.stdout == "\u{1b}[H\u{1b}[2Jsame 1\n\u{1b}[H\u{1b}[2Jsame 2\n"
  let titled = watch(ctx, ["-x", "-e", "-n", "-5", f"{ctx.xsh_bin}", f"{script}", f"{counter}", "99", "3", "numbered"])?
  assert "Every 0.1s: " in titled.stdout, "an interval below 0.1 second is raised to 0.1"
}

test test_watch_chgexit_stops_with_success_when_output_changes { |ctx|
  let script = counting_command(ctx, "count.xsh")?
  let counter = test.temp_path(ctx, name: "count")
  let changed = watch(ctx, ["-x", "-t", "-g", "-n", "0.1", f"{ctx.xsh_bin}", f"{script}", f"{counter}", "3", "99", "plain"])?
  assert changed.status.exited_with(0)
  assert changed.stdout == "\u{1b}[H\u{1b}[2Jsame\n\u{1b}[H\u{1b}[2Jsame\n\u{1b}[H\u{1b}[2Jchanged\n"
}

test test_watch_rejects_unusable_invocations_with_status_one { |ctx|
  assert watch(ctx, ["-x"])?.status.exited_with(1)
  let bad_interval = watch(ctx, ["-x", "-n", "soon", "true"])?
  assert bad_interval.status.exited_with(1) and "failed to parse argument: 'soon'" in bad_interval.stderr
  assert watch(ctx, ["--no-such-option", "true"])?.status.exited_with(1)
}
