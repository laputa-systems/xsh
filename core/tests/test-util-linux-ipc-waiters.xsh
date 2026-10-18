use support.uu as uu
use support.util_linux as ul

type SemaphoreElement = {
  value: Int,
  waiting_for_increase: Int,
  waiting_for_zero: Int,
  pid: Int,
}

type WaiterSnapshot = {
  pid: Int,
  owner_uid: Int,
  owner_gid: Int,
  semaphore_id: Int,
  waiters: List[Int],
  elements: List[SemaphoreElement],
}

type ApiObservation = {
  errno: Int?,
  value: Int?,
  waiting_for_increase: Int?,
  waiting_for_zero: Int?,
  pid: Int?,
}

type ApiReport = {before: Int, after: Int, observations: List[ApiObservation]}
type KernelProbe = {exists: Bool, errno: Int?, value: Int?}

proc observe_element(s: uu.Scene, identity: Int, index: Int) [fs, process, env, error] -> Result[ApiReport, Error] {
  let output = uu.at(s, f"waiter-api-{index}.out")
  let errors = uu.at(s, f"waiter-api-{index}.err")
  let script = fp"{s.ctx.core_dir}/tests/data/util-linux/ipc/observe.xsh"
  let words = [
    p"/bin/setpriv",
    p"--reuid=10001",
    p"--regid=10001",
    p"--clear-groups",
    p"--bounding-set=-all",
    p"--inh-caps=-all",
    p"--ambient-caps=-all",
    p"--no-new-privs",
    p"--",
    s.ctx.xsh_bin,
    script,
    p"--",
    p"sysv_semaphore",
    fp"{identity}",
    fp"{index}",
  ]
  let vars = {LC_ALL: "C", TZ: "UTC", XSH_MODULE_PATH: s.ctx.core_dir.display()}
  let plan = process.command_argv(p"/bin/setpriv", words, s.root, vars, b"", output, errors, timeout: 10s)
  let status = process.run(plan)?
  assert status.exited_with(0), errors.read_text()?
  assert errors.read_bytes()? == b""
  json.decode(output.read_text()?)?.require(ApiReport)
}

proc assert_element_reads(report: ApiReport, expected: SemaphoreElement) [error] {
  assert report.before == report.after, f"API call leaked a descriptor: {report.before} -> {report.after}"
  assert report.observations.len() == 8
  let observation: ApiObservation = {
    errno: null,
    value: expected.value,
    waiting_for_increase: expected.waiting_for_increase,
    waiting_for_zero: expected.waiting_for_zero,
    pid: expected.pid,
  }
  for actual in report.observations {
    assert actual == observation
  }
}

proc probe_kernel(s: uu.Scene, script: Path, identity: Int) [fs, process, error] -> Result[KernelProbe, Error] {
  let output = uu.at(s, "waiter-kernel-probe.out")
  let errors = uu.at(s, "waiter-kernel-probe.err")
  let words = [
    p"/bin/setpriv",
    p"--reuid=10001",
    p"--regid=10001",
    p"--clear-groups",
    p"--bounding-set=-all",
    p"--inh-caps=-all",
    p"--ambient-caps=-all",
    p"--no-new-privs",
    p"--",
    p"/usr/bin/python3",
    script,
    p"--probe",
    fp"{identity}",
  ]
  let plan = process.command_argv(p"/bin/setpriv", words, s.root,
    {PYTHONDONTWRITEBYTECODE: "1", LC_ALL: "C", TZ: "UTC"}, b"", output, errors, timeout: 5s)
  let status = process.run(plan)?
  assert status.exited_with(0), errors.read_text()?
  assert errors.read_bytes()? == b""
  json.decode(output.read_text()?)?.require(KernelProbe)
}

proc assert_waiters_reaped(snapshot: WaiterSnapshot) [fs, error] {
  for pid in snapshot.waiters {
    assert ! fp"/proc/{pid}".exists()?, f"waiter {pid} survived fixture cleanup"
  }
}

test test_linux_system_v_semaphore_waiter_counts_and_pid_match_kernel { |ctx|
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let script = fp"{ctx.core_dir}/tests/fixtures/util-linux/waiters/keeper.py"
  let keeper = ul.start_keeper(s, script, [])?
  defer ul.stop_keeper(keeper)

  let ready = json.decode(keeper.ready)?.require(WaiterSnapshot)?
  assert ready.pid > 0 and ready.owner_uid == 10001 and ready.owner_gid == 10001
  assert ready.semaphore_id >= 0 and ready.waiters.len() == 2
  assert ready.waiters[0] != ready.waiters[1]
  assert ready.waiters[0] != ready.pid and ready.waiters[1] != ready.pid
  assert ready.elements.len() == 2

  let increase_waiter: SemaphoreElement = {
    value: 0,
    waiting_for_increase: 1,
    waiting_for_zero: 0,
    pid: ready.waiters[0],
  }
  let zero_waiter: SemaphoreElement = {
    value: 1,
    waiting_for_increase: 0,
    waiting_for_zero: 1,
    pid: ready.waiters[1],
  }
  assert ready.elements[0] == increase_waiter
  assert ready.elements[1] == zero_waiter

  assert_element_reads(observe_element(s, ready.semaphore_id, 0)?, increase_waiter)
  assert_element_reads(observe_element(s, ready.semaphore_id, 1)?, zero_waiter)

  ul.keeper_send(keeper, "snapshot")
  let after_reads = json.decode(ul.keeper_receive(keeper)?)?.require(WaiterSnapshot)?
  assert after_reads == ready, "read-only element observations changed the live kernel state"

  ul.keeper_send(keeper, "DONE")
  ul.finish_keeper(keeper)
  assert_waiters_reaped(ready)?
  let missing = probe_kernel(s, script, ready.semaphore_id)?
  assert missing == {exists: false, errno: 22, value: null}

  let cancelled_keeper = ul.start_keeper(s, script, [])?
  defer ul.stop_keeper(cancelled_keeper)
  let cancelled_ready = json.decode(cancelled_keeper.ready)?.require(WaiterSnapshot)?
  cancelled_keeper.child.cancel(signal: "TERM", kill_after: 2s)
  assert_waiters_reaped(cancelled_ready)?
  let missing_after_cancel = probe_kernel(s, script, cancelled_ready.semaphore_id)?
  assert missing_after_cancel == {exists: false, errno: 22, value: null}
}
