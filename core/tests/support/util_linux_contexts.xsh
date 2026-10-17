##! Real private identities and bounded terminal captures for util-linux parity.
use uu
use util_linux as ul

proc terminal_pair(columns: Int) [process, error] -> Result[UnixPty, Error] {
  assert columns > 0 and columns <= 512
  let pair = unix.open_pty()?
  var ready = false
  defer { if ! ready { unix.close_fd(pair.replica) } }
  defer { if ! ready { unix.close_fd(pair.master) } }
  unix.set_window_size(24, columns, fd: pair.replica)
  assert unix.window_size(pair.replica)?.cols == columns
  ready = true
  Ok(pair)
}

proc terminal_drain(fd: Int, captured: Bytes) [process, net, error] -> Result[Bytes, Error] {
  var output = captured
  while "readable" in unix.poll_fd(fd, ["readable"], timeout_ms: 0)? {
    let chunk = unix.read_fd(fd, 8192)?
    assert ! chunk.is_empty(), "terminal closed before capture completed"
    output = bytes.concat([output, chunk])
    assert output.len() <= 1048576, "terminal capture exceeds 1 MiB"
  }
  Ok(output)
}

## Captures two distinct fixed-width terminal streams, preserving raw bytes and signal termination.
## Owns the child until observed completion and cancels it on failure, with a 1 MiB limit per stream.
export proc terminal_observe(s: uu.Scene, command: Str, args: List[Path], words: List[Path], columns: Int, vars: Record = {}) [fs, process, net, time, error] -> Result[ul.Observed, Error] {
  let output = terminal_pair(columns)?
  defer unix.close_fd(output.master)
  defer unix.close_fd(output.replica)
  let errors = terminal_pair(columns)?
  defer unix.close_fd(errors.master)
  defer unix.close_fd(errors.replica)
  let plan = process.command_argv(words[0], words, s.root, vars, b"", fp"{output.name}", fp"{errors.name}", timeout: 5s)
  let child = spawn plan?
  defer child.cancel(signal: "TERM", kill_after: 100ms)
  let deadline = time.now() + 6000
  var stdout = b""
  var stderr = b""
  while true {
    stdout = terminal_drain(output.master, stdout)?
    stderr = terminal_drain(errors.master, stderr)?
    if let completed = process.wait_timeout([child], 0ms)? {
      stdout = terminal_drain(output.master, stdout)?
      stderr = terminal_drain(errors.master, stderr)?
      let status = completed.status
      let termination = if status.signaled() { ul.Signaled(status.signal_number()?) } else { ul.Exited(status.exit_code()?) }
      return Ok({util: command, args: [word.display() for word in args], termination: termination, stdout: stdout, stderr: stderr})
    }
    assert time.now() < deadline, "terminal command timed out"
    let events = unix.poll_fd(output.master, ["readable"], timeout_ms: 20)?
    assert ! ("invalid" in events or "error" in events), "terminal descriptor failed"
  }
  Err(error.failure("terminal capture ended without observing child"))
}

## Runs the qualified reference through the shared launcher with both output streams on terminals.
export proc terminal_reference(s: uu.Scene, command: Str, args: List[Path], columns: Int = 40, vars: Record = {}) [fs, process, net, time, error] -> Result[ul.Observed, Error] {
  terminal_observe(s, command, args, ul.reference_argv(s, command, args, vars)?, columns, vars)
}

## Runs the candidate through the shared launcher with identical terminal dimensions and capture bounds.
export proc terminal_candidate(s: uu.Scene, command: Str, args: List[Path], columns: Int = 40, vars: Record = {}) [fs, process, net, time, error] -> Result[ul.Observed, Error] {
  terminal_observe(s, command, args, uu.argv(s, command, args, vars)?, columns, vars)
}

## Starts an owned keeper as UID and GID 10002 with empty groups and all capability sets dropped.
## The caller owns release and completion through the shared keeper protocol.
export proc start_identity_keeper(s: uu.Scene, script: Path, args: List[Str]) [fs, process, env, net, time, error] -> Result[ul.Keeper, Error] {
  let launcher = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/contexts/as_owner.py"
  ul.start_keeper(s, launcher, [script.display()].extend(args))
}

## Sends the keeper release request and observes successful completion while its owning scope is live.
export proc release_keeper(keeper: ul.Keeper, request: Str = "DONE") [fs, process, net, error] {
  ul.keeper_send(keeper, request)
  ul.finish_keeper(keeper)
}

## The exact live parent-namespace lock identity exported by the owned outer runner.
export type OutsideHolder = {pid: Int, path: Path, device: Int, inode: Int}

## Verifies the exported outside inode and device remain present but absent from private procfs locks.
export proc outside_holder() [fs, env, error] -> Result[OutsideHolder, Error] {
  ul.require_private_namespace()
  let holder: OutsideHolder = {
    pid: e"XSH_UTIL_LINUX_OUTSIDE_PID"?.parse_int()?,
    path: env.Path.XSH_UTIL_LINUX_OUTSIDE_LOCK_PATH?,
    device: e"XSH_UTIL_LINUX_OUTSIDE_DEV"?.parse_int()?,
    inode: e"XSH_UTIL_LINUX_OUTSIDE_INO"?.parse_int()?,
  }
  let metadata = fs.stat(holder.path)?
  assert metadata.dev == holder.device and metadata.ino == holder.inode
  let locks = p"/proc/locks".read_text()?
  for line in locks.lines() {
    let fields = line.fields()
    if fields.len() >= 6 {
      assert ! fields[5].ends_with(f":{holder.inode}"), "outside holder leaked into fresh procfs"
    }
  }
  Ok(holder)
}
