use support.uu
use support.util_linux as ul

type Holder = {pid: Int, inode: Int, device: Str}
type Barrier = {holder_pid: Int, inode: Int, reference_row: Str, candidate_row: Str}
type Capture = {returncode: Int, stdout_b64: Str, stderr_b64: Str}
type Captures = {reference: Capture, candidate: Capture}

proc reply(payload: Str) [error] -> Result[Any, Error] {
  let value = json.decode(payload)?
  if let Ok(failure) = json.get(value, ["error"]) {
    return Err(error.failure(failure.require(Str)?))
  }
  Ok(value)
}

pure observation(args: List[Path], captured: Capture) -> Result[ul.Observed, Error] {
  let termination = if captured.returncode < 0 { ul.Signaled(-captured.returncode) } else { ul.Exited(captured.returncode) }
  Ok({util: "lslocks", args: [word.display() for word in args], termination: termination,
    stdout: captured.stdout_b64.base64_decode()?, stderr: captured.stderr_b64.base64_decode()?})
}

test test_util_linux_lslocks_owner_exit_after_both_kernel_snapshots { |ctx|
  ul.require_private_namespace()
  let s = uu.scene(ctx)?
  let target = uu.at(s, "owner-disappears-during-collection")
  let owner = ul.start_keeper(s, fp"{ctx.core_dir}/tests/fixtures/util-linux/locks/keeper.py",
    ["hold", "posix--w", target.display(), "none"])?
  defer ul.stop_keeper(owner)
  let held = json.decode(owner.ready)?.require(Holder)?
  assert fs.stat(target)?.ino == held.inode
  let options = [p"--json", p"--pid", fp"{held.pid}", p"--output", p"PID,INODE,PATH"]
  let reference_words = ul.reference_argv(s, "lslocks", options)?
  let candidate_words = uu.argv(s, "lslocks", options)?
  let configuration = json.encode({holder_pid: held.pid, inode: held.inode, device: held.device,
    reference: [word.display() for word in reference_words], candidate: [word.display() for word in candidate_words]})?
  let observer = ul.start_keeper(s, fp"{ctx.core_dir}/tests/fixtures/util-linux/races/owner_disappearance.py", [configuration])?
  defer ul.stop_keeper(observer)
  let barrier = reply(observer.ready)?.require(Barrier)?
  assert barrier.holder_pid == held.pid and barrier.inode == held.inode
  assert (f" {held.pid} " in barrier.reference_row) and (f" {held.pid} " in barrier.candidate_row)
  ul.keeper_send(owner, "DONE")
  ul.finish_keeper(owner)
  assert ! fp"/proc/{held.pid}".exists()
  for line in p"/proc/locks".read_text()?.lines() {
    let fields = line.fields()
    assert fields.len() < 6 or fields[4] != f"{held.pid}" or ! fields[5].ends_with(f":{held.inode}")
  }
  ul.keeper_send(observer, "RESUME")
  let completed = reply(ul.keeper_receive(observer)?)?.require(Captures)?
  ul.finish_keeper(observer)
  let reference = observation(options, completed.reference)?
  let candidate = observation(options, completed.candidate)?
  assert reference.termination == ul.Exited(0), reference.stderr.utf8()?
  assert reference.stderr == b""
  let rows = json.get(json.decode(reference.stdout.utf8()?)?, ["locks"])?.require(List[Any])?
  assert rows.len() == 1
  assert json.get(rows[0], ["pid"])?.require(Int)? == held.pid
  assert json.get(rows[0], ["inode"])?.require(Int)? == held.inode
  assert json.get(rows[0], ["path"])?.require(Str)? != target.display()
  assert reference == candidate, json.encode(completed)?
}
