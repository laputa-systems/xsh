# CPU affinity, scheduler policy, I/O priority, and the resource limits of
# another process. Every change targets a sleeping child so the test process
# keeps its own scheduling state for the tests that follow.

# The CPUs the process may use, as the kernel lists them in /proc: ranges
# `a-b`, comma separated. This reads the list the way a person would, without
# the primitive under test.
pure expand_cpu_list(text: Str) -> List[Int] {
  var cpus: List[Int] = []

  for part in text.trim().split(",") {
    let ends = part.split("-")
    let first = ends[0].parse_int() ?? 0
    let last = if ends.len() > 1 { ends[1].parse_int() ?? first } else { first }
    var at = first

    while at <= last {
      cpus += [at]
      at += 1
    }
  }

  cpus
}

# Waits until the spawned child has finished executing `sleep`. The spawn can
# return while the child is still inside exec, and a resource limit set then
# can make that exec fail, so the limit tests wait for the name the kernel
# reports.
proc wait_for_sleep(pid: Int) [fs, time, error] {
  if ! fp"/proc/{pid}/comm".exists()? {
    return
  }

  for _ in range(500) {
    if fp"/proc/{pid}/comm".read_text()?.trim() == "sleep" {
      return
    }

    time.sleep(10ms)?
  }
}

proc errno_of_unit(result: Result[Unit]) -> Int {
  match result {
    Ok(_) => 0
    Err(failure) => failure.errno ?? -1
  }
}

proc errno_of_cpus(result: Result[List[Int]]) -> Int {
  match result {
    Ok(_) => 0
    Err(failure) => failure.errno ?? -1
  }
}

test test_affinity_lists_the_cpus_the_kernel_allows { |ctx|
  if ! p"/proc/self/status".exists()? {
    test.skip("the host has no /proc/self/status")
  }

  let cpus = process.affinity()?
  assert ! cpus.is_empty()

  var previous = -1

  for id in cpus {
    assert id > previous, "CPUs are ascending"
    previous = id
  }

  assert process.affinity(0)? == cpus
  assert process.affinity(process.current_pid()?)? == cpus

  var listed = ""

  for line in fp"/proc/self/status".read_text()?.lines() {
    if line.starts_with("Cpus_allowed_list:") {
      listed = line.byte_slice("Cpus_allowed_list:".byte_len())
    }
  }

  assert expand_cpu_list(listed) == cpus
}

test test_set_affinity_restricts_another_process_only { |ctx|
  let cpus = process.affinity()?

  if cpus.len() < 2 {
    test.skip("the process may use one CPU, so there is nothing to restrict")
  }

  let child = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  defer process.kill(child.pid, signal: "KILL")

  assert process.affinity(child.pid)? == cpus
  process.set_affinity(child.pid, [cpus[0]])?
  assert process.affinity(child.pid)? == [cpus[0]]
  assert process.affinity()? == cpus

  # CPUs the kernel has no bit for are ignored; the rest still applies.
  process.set_affinity(child.pid, [cpus[1], cpus[0], 100000])?
  assert process.affinity(child.pid)? == [cpus[0], cpus[1]]

  process.set_affinity(child.pid, cpus)?
  assert process.affinity(child.pid)? == cpus
}

test test_affinity_failures_carry_the_kernels_answer { |ctx|
  let cpus = process.affinity()?

  assert errno_of_cpus(process.affinity(2147483647)) == 3
  assert errno_of_unit(process.set_affinity(2147483647, cpus)) == 3
  # No CPU in the set: EINVAL from the kernel.
  assert errno_of_unit(process.set_affinity(0, [])) == 22
  test.error_kind(process.set_affinity(0, [-1]), "invalid-argument")
  test.error_kind(process.set_affinity(0, [1048576]), "invalid-argument")
  test.error_kind(process.affinity(2147483648), "pid-range")
}

test test_scheduler_policy_is_read_and_changed_for_another_process { |ctx|
  let child = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  defer process.kill(child.pid, signal: "KILL")

  let before = process.scheduler(child.pid)?
  assert before.policy == "other"
  assert before.priority == 0
  assert ! before.reset_on_fork

  process.set_scheduler(child.pid, "batch")?
  assert process.scheduler(child.pid)?.policy == "batch"

  process.set_scheduler(child.pid, "other")?
  assert process.scheduler(child.pid)?.policy == "other"

  process.set_scheduler(child.pid, "idle", reset_on_fork: true)?
  let idle = process.scheduler(child.pid)?
  assert idle.policy == "idle"
  assert idle.reset_on_fork

  # The caller's own policy is untouched by all of it.
  assert process.scheduler()?.policy == process.scheduler(process.current_pid()?)?.policy
}

test test_scheduler_failures_carry_the_kernels_answer { |ctx|
  let child = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  defer process.kill(child.pid, signal: "KILL")

  assert errno_of_unit(process.set_scheduler(2147483647, "batch")) == 3
  test.error_kind(process.scheduler(2147483647), "process-scheduler")
  # The fair policies take no priority.
  assert errno_of_unit(process.set_scheduler(child.pid, "other", 5)) == 22
  test.error_kind(process.set_scheduler(child.pid, "nosuch"), "invalid-argument")
  test.error_kind(process.set_scheduler(child.pid, "other", runtime_ns: -1), "invalid-argument")

  if unix.id()?.euid != 0 {
    # Real-time policies need CAP_SYS_NICE or an rtprio limit.
    if (process.rlimit("rtprio")?.soft ?? 0) == 0 {
      assert errno_of_unit(process.set_scheduler(child.pid, "fifo", 5)) == 1
    }
  }
}

test test_scheduler_priorities_report_each_policys_range { |ctx|
  for name in ["other", "batch", "idle", "deadline"] {
    let range = process.scheduler_priorities(name)?
    assert range.min == 0 and range.max == 0, name
  }

  for name in ["fifo", "rr"] {
    let range = process.scheduler_priorities(name)?
    assert range.min == 1 and range.max == 99, name
  }

  match process.scheduler_priorities("ext") {
    Ok(range) => assert range.min == 0 and range.max == 0
    Err(failure) => assert (failure.errno ?? -1) == 22
  }

  test.error_kind(process.scheduler_priorities("nosuch"), "invalid-argument")
}

test test_scheduler_time_slice_can_be_set_for_the_fair_policies { |ctx|
  let child = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  defer process.kill(child.pid, signal: "KILL")

  process.set_scheduler(child.pid, "batch", runtime_ns: 3000000)?
  let sliced = process.scheduler(child.pid)?
  assert sliced.policy == "batch"

  if sliced.runtime_ns != 3000000 {
    test.skip("the kernel has no per-task time slice")
  }

  # A change without a time slice through the time-aware call restores the
  # default one.
  process.set_scheduler(child.pid, "batch", runtime_ns: 0)?
  assert process.scheduler(child.pid)?.runtime_ns != 3000000
}

test test_io_priority_is_read_and_changed_for_another_process { |ctx|
  let child = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  defer process.kill(child.pid, signal: "KILL")

  process.set_io_priority(child.pid, "best-effort", 3)?
  let best = process.io_priority(child.pid)?
  assert best.class == "best-effort"
  assert best.level == 3

  process.set_io_priority(child.pid, "idle")?
  assert process.io_priority(child.pid)?.class == "idle"

  process.set_io_priority(child.pid, "none")?
  let none = process.io_priority(child.pid)?
  assert none.class == "none"
  assert none.level == 0

  # Reading the caller's group and user is allowed and names a class.
  for which in ["process", "group", "user"] {
    let own = process.io_priority(0, which)?
    assert own.class in ["none", "realtime", "best-effort", "idle"], which
    assert own.level >= 0
  }
}

test test_io_priority_failures_carry_the_kernels_answer { |ctx|
  let child = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  defer process.kill(child.pid, signal: "KILL")

  assert errno_of_unit(process.set_io_priority(2147483647, "idle")) == 3
  # A level outside the 13-bit field is refused the way the kernel refuses it.
  assert errno_of_unit(process.set_io_priority(child.pid, "best-effort", -1)) == 22
  assert errno_of_unit(process.set_io_priority(child.pid, "best-effort", 8192)) == 22
  test.error_kind(process.set_io_priority(child.pid, "bogus"), "invalid-argument")
  test.error_kind(process.io_priority(0, "bogus"), "invalid-argument")

  if unix.id()?.euid != 0 {
    assert errno_of_unit(process.set_io_priority(child.pid, "realtime", 4)) == 1
  }
}

test test_resource_limits_are_read_and_changed_for_another_process { |ctx|
  let child = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  defer process.kill(child.pid, signal: "KILL")

  wait_for_sleep(child.pid)?

  let own_core = process.rlimit("core")?
  let before = process.rlimit("core", pid: child.pid)?
  assert before.resource == "core"

  process.set_rlimit("core", soft: 0, hard: 0, pid: child.pid)?
  let lowered = process.rlimit("core", pid: child.pid)?
  assert lowered.soft == 0 and lowered.hard == 0

  # The caller's own limit is untouched, and the whole table agrees.
  assert process.rlimit("core")? == own_core

  let table = process.rlimits(pid: child.pid)?
  let core = [l for l in table if l.resource == "core"]
  assert core.len() == 1 and core[0].soft == 0
  assert table.len() == process.rlimits()?.len()

  # An omitted bound keeps the other process's value, as it does for this one.
  process.set_rlimit("nofile", soft: 11, hard: 22, pid: child.pid)?
  process.set_rlimit("nofile", soft: 5, pid: child.pid)?
  assert process.rlimit("nofile", pid: child.pid)?.hard == 22
  process.set_rlimit("nofile", hard: 8, pid: child.pid)?
  let clamped = process.rlimit("nofile", pid: child.pid)?
  assert clamped.soft == 5 and clamped.hard == 8
}

test test_resource_limit_failures_carry_the_kernels_answer { |ctx|
  match process.rlimit("nofile", pid: 2147483647) {
    Ok(_) => test.fail("a missing process has no limits")
    Err(failure) => assert (failure.errno ?? -1) == 3
  }

  match process.set_rlimit("nofile", soft: 5, pid: 2147483647) {
    Ok(_) => test.fail("a missing process cannot be changed")
    Err(failure) => assert (failure.errno ?? -1) == 3
  }

  test.error_kind(process.rlimit("nofile", pid: -5), "pid-range")
  test.error_kind(process.rlimit("nosuch", pid: 1), "invalid-argument")
}
