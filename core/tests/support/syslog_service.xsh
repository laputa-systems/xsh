##! Drives a real syslogd process the way a supervisor does: a readiness
##! descriptor, one append-only log for stdout and stderr, signals to stop it.

## Directories for one service run; `dev` stands in for /dev.
export type Layout = {root: Path, dev: Path, socket: Path, state: Path, log: Path}

## A service about to be spawned: its command and the readiness FIFO. The test
## spawns `command` itself because a process handle cannot outlive its scope.
export type Prepared = {command: Command, layout: Layout, ready: Int, inheritable: Int}

## Creates the directories of a run. The socket directory is private and 0755.
export proc layout(ctx: TestContext, name: Str) [fs, error] -> Result[Layout, Error] {
  let root = test.temp_dir(ctx, name: name)?
  let dev = fp"{root}/dev"
  let state = fp"{root}/state"
  dev.mkdir()
  state.mkdir()
  dev.chmod(0o755)
  state.chmod(0o755)
  Ok({root: root, dev: dev, socket: fp"{dev}/log", state: state, log: fp"{root}/syslogd.log"})
}

pure xsh_command(ctx: TestContext, lay: Layout, wrapper: Str, extra: List[Str], ready: Int, errors: Path) -> Command {
  let script = fp"{ctx.core_dir}/syslogd.xsh"
  let args = ["--state", lay.state.display(), "--socket", lay.socket.display()].extend(extra)
  let direct = [ctx.xsh_bin.display(), script.display()].extend(args)
  let environment = {LC_ALL: "C", NOTIFY_FD: f"{ready}"}
  if wrapper.is_empty() {
    return process.command_argv(ctx.xsh_bin, direct, lay.root, environment, stdout: lay.log, stderr: errors, stdout_append: true, stderr_append: true)
  }
  process.command_argv(
    "/bin/sh",
    ["/bin/sh", "-c", f"{wrapper}; exec \"$@\"", "sh"].extend(direct),
    lay.root,
    environment,
    stdout: lay.log,
    stderr: errors,
    stdout_append: true,
    stderr_append: true,
  )
}

## Builds the command for the service under `wrapper`, a shell prefix such as
## `ulimit -n 40`, with extra arguments, and opens the readiness FIFO.
export proc prepare(ctx: TestContext, lay: Layout, wrapper: Str = "", extra: List[Str] = [], stderr_file: Path? = null) [fs, process, net, time, error] -> Result[Prepared, Error] {
  let fifo = fp"{lay.root}/ready.fifo"
  fifo.remove(missing_ok: true)
  fs.mkfifo(fifo, 0o600)
  let reader = unix.open_fd(fifo, nonblock: true)?
  let writer = unix.open_fd(fifo, write: true, nonblock: true)?
  let inheritable = unix.duplicate_fd(writer, min_fd: 100)?
  unix.dup_fd(inheritable, inheritable)?
  unix.close_fd(writer)?
  Ok({command: xsh_command(ctx, lay, wrapper, extra, inheritable, stderr_file ?? lay.log), layout: lay, ready: reader, inheritable: inheritable})
}

## Drops the parent's copy of the readiness descriptor once the child is spawned.
export proc attach(prepared: Prepared) [process, error] {
  unix.close_fd(prepared.inheritable)?
}

## Waits until the readiness byte arrives, then checks that the socket is already
## published, which is the order the service promises.
export proc await_ready(prepared: Prepared) [fs, net, process, error] {
  let events = unix.poll_fd(prepared.ready, ["readable"], timeout_ms: 30000)?
  assert "readable" in events, "the service did not report readiness"
  assert unix.read_fd(prepared.ready, 1)?.len() == 1, "the readiness channel closed without a byte"
  assert fs.stat(prepared.layout.socket)?.kind == "socket", "readiness arrived before the socket was published"
}

## Closes the readiness reader.
export proc finish(prepared: Prepared) [process, error] {
  unix.close_fd(prepared.ready)?
}

## The log text so far.
export proc log_text(lay: Layout) [fs, error] -> Result[Str, Error] {
  lay.log.read_text()
}

## Lines of the log that start with the given prefix.
export proc lines_with(lay: Layout, prefix: Str) [fs, error] -> Result[List[Str], Error] {
  Ok([line for line in lay.log.read_text()?.lines() if line.starts_with(prefix)])
}

## Waits until the log holds at least `count` lines containing the text.
export proc await_containing(lay: Layout, needle: Str, count: Int) [fs, time, error] -> Result[List[Str], Error] {
  let deadline = time.now() + 30000
  while time.now() < deadline {
    let found = [line for line in lay.log.read_text()?.lines() if needle in line]
    if found.len() >= count { return Ok(found) }
    time.sleep(20ms)?
  }
  Err(error.failure(f"the log never reached {count} lines containing {needle}"))
}

## Waits until a file holds at least `count` lines containing the text.
export proc await_containing_in(file: Path, needle: Str, count: Int) [fs, time, error] -> Result[List[Str], Error] {
  let deadline = time.now() + 30000
  while time.now() < deadline {
    let found = [line for line in file.read_text()?.lines() if needle in line]
    if found.len() >= count { return Ok(found) }
    time.sleep(20ms)?
  }
  Err(error.failure(f"{file} never reached {count} lines containing {needle}"))
}

## Waits until the log holds at least `count` lines with the prefix.
export proc await_lines(lay: Layout, prefix: Str, count: Int) [fs, time, error] -> Result[List[Str], Error] {
  let deadline = time.now() + 30000
  while time.now() < deadline {
    let found = lines_with(lay, prefix)?
    if found.len() >= count { return Ok(found) }
    time.sleep(20ms)?
  }
  Err(error.failure(f"the log never reached {count} lines starting with {prefix}"))
}

## The pid in the latest start event of the log.
export proc reported_pid(lay: Layout) [fs, time, error] -> Result[Int, Error] {
  let starts = await_lines(lay, "event=start", 1)?
  let start = starts[starts.len() - 1]
  let field = rx"pid=([0-9]+)".captures(start)
  Ok(field[1].parse_int()?)
}

## Sends one datagram to the socket from a fresh sender socket. The socket is
## reached through its directory's descriptor so a long test path still fits a
## socket address.
export proc send(socket: Path, data: Bytes) [net, process, error] -> Result[Int, Error] {
  let c = linux.net_constants()
  let dir = unix.open_fd(socket.parent(), flags: ["directory"])?
  defer unix.close_fd(dir)
  let fd = linux.socket(c.AF_UNIX, c.SOCK_DGRAM)?
  defer unix.close_fd(fd)
  Ok(linux.sendto(fd, data, {family: "unix", address: f"/proc/self/fd/{dir}/{socket.name()}"})?)
}

## Sends a signal to the service process named in its start event.
export proc signal(lay: Layout, name: Str) [fs, process, time, error] -> Result[Unit, Error] {
  process.kill(reported_pid(lay)?, signal: name)
}

## What a sender run reported: its pid and how many sends succeeded, hit a full
## queue, or failed otherwise.
export type Sent = {pid: Int, sent: Int, again: Int, other: Int}

## The Python sender used for descriptor passing and floods.
export proc sender_command(ctx: TestContext, root: Path, args: List[Str], stdin = b"") [process] -> Command {
  let script = fp"{ctx.core_dir}/tests/support/syslog_sender.py"
  process.command_argv("python3", ["python3", "-I", script.display()].extend(args), root, {LC_ALL: "C"}, stdin, fp"{root}/sender.out", timeout: 120s)
}

## Runs the sender to completion and returns what it reported.
export proc send_with(ctx: TestContext, lay: Layout, scenario: Str, args: List[Str], stdin = b"") [fs, process, error] -> Result[Sent, Error] {
  let command = sender_command(ctx, lay.root, [scenario, lay.socket.display()].extend(args), stdin)
  let status = process.run(command)?
  assert status.exited_with(0), "the sender failed"
  Ok(json.decode(fp"{lay.root}/sender.out".read_text()?)?.require(Sent)?)
}

## Open descriptors of a process.
export proc open_descriptors(pid: Int) [fs, error] -> Result[Int, Error] {
  Ok([entry.name for entry in fs.children(Path(f"/proc/{pid}/fd"))?].len())
}

## One `name=value` counter from an event line.
export proc counter(line: Str, name: Str) [error] -> Result[Int, Error] {
  let found = regex.compile(f"(?:^| ){name}=([0-9]+)")?.captures(line)
  if found.is_empty() { return Err(error.failure(f"{name} is missing from {line}")) }
  Ok(found[1].parse_int()?)
}
