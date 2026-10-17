##! Exact comparisons against the qualified util-linux reference in native scenes.
use uu
use core.lib.gnu

## The kernel distinguishes an exit code from termination by a signal.
export enum Termination { Exited(Int), Signaled(Int) }

## A complete process observation retains signal identity without shell-code conversion.
export type Observed = {util: Str, args: List[Str], termination: Termination, stdout: Bytes, stderr: Bytes}

## An owned Python fixture process with bounded FIFO control and its exact ready record.
export type Keeper = {child: ProcessHandle, input_fd: Int, output_fd: Int, ready: Str, stderr: Path}

## Reads one fixture reply, capped at 64 KiB and a five-second readiness deadline.
export proc keeper_receive(keeper: Keeper) [net, process, time, error] -> Result[Str, Error] {
  var data = b""
  let deadline = time.now() + 5000
  while data.len() < 65536 {
    let remaining = deadline - time.now()
    assert remaining > 0, "fixture reply timed out"
    let events = unix.poll_fd(keeper.output_fd, ["readable"], timeout_ms: remaining)?
    assert "readable" in events, "fixture exited or failed to reply"
    let chunk = unix.read_fd(keeper.output_fd, 1)?
    assert chunk.len() == 1, "fixture reply ended before newline"
    return data.utf8() when chunk == b"\n"
    data = bytes.concat([data, chunk])
  }
  Err(error.failure("fixture reply exceeds 64 KiB"))
}

## Sends one complete newline-delimited fixture request without accepting embedded newlines.
export proc keeper_send(keeper: Keeper, request: Str) [net, process, error] {
  assert ! ("\n" in request) and request.byte_len() <= 65535
  let data = bytes.from_text(request + "\n")
  var offset = 0
  while offset < data.len() {
    assert "writable" in unix.poll_fd(keeper.input_fd, ["writable"], timeout_ms: 5000)?
    let count = unix.write_fd(keeper.input_fd, data[offset..])?
    assert count > 0
    offset += count
  }
}

## Cancels and reaps the fixture if needed, then closes its two owned control descriptors.
export proc stop_keeper(keeper: Keeper) [fs, process, error] {
  defer unix.close_fd(keeper.output_fd)
  defer unix.close_fd(keeper.input_fd)
  keeper.child.cancel(signal: "TERM", kill_after: 2s)
}

## Observes successful fixture completion after a release request while its owning scope is live.
export proc finish_keeper(keeper: Keeper) [fs, process, error] {
  let completed = process.wait_timeout([keeper.child], 2s)?
  if let result = completed {
    assert result.status.exited_with(0), keeper.stderr.read_text()?
  } else {
    keeper.child.cancel(signal: "TERM", kill_after: 2s)
    assert false, "fixture did not finish after release"
  }
}

## Starts a test-only Python keeper and returns after its first complete ready record.
export proc start_keeper(s: uu.Scene, script: Path, args: List[Str]) [fs, process, env, net, time, error] -> Result[Keeper, Error] {
  require_private_namespace()
  let root = test.temp_dir(s.ctx, name: "util-linux-keeper")?
  let input = fp"{root}/input"
  let output = fp"{root}/output"
  let stderr = fp"{root}/stderr"
  fs.mkfifo(input, 0o600)
  fs.mkfifo(output, 0o600)
  let input_fd = unix.open_fd(input, write: true, nonblock: true)?
  var input_open = true
  defer { if input_open { unix.close_fd(input_fd) } }
  let output_fd = unix.open_fd(output, nonblock: true)?
  var output_open = true
  defer { if output_open { unix.close_fd(output_fd) } }
  let words = [p"/usr/bin/python3", script].extend([fp"{word}" for word in args])
  let plan = process.command_argv(p"/usr/bin/python3", words, s.root,
    {PYTHONDONTWRITEBYTECODE: "1", LC_ALL: "C", TZ: "UTC"}, input, output, stderr, timeout: 25s)
  let child = spawn plan?
  var started = false
  defer { if ! started { child.cancel(signal: "TERM", kill_after: 2s) } }
  let keeper: Keeper = {child: child, input_fd: input_fd, output_fd: output_fd, ready: "", stderr: stderr}
  let ready = keeper_receive(keeper)?
  started = true
  input_open = false
  output_open = false
  Ok({...keeper, ready: ready})
}

proc observe(s: uu.Scene, command: Str, args: List[Path], words: List[Path], vars: Record, timeout: Duration) [fs, process, error] -> Result[Observed, Error] {
  assert timeout > 0ms and timeout <= 30s
  let stdout = uu.at(s, ".util-linux-stdout")
  let stderr = uu.at(s, ".util-linux-stderr")
  let plan = process.command_argv(words[0], words, s.root, vars, b"", stdout, stderr, timeout:)
  let status = process.run(plan)?
  let termination = if status.signaled() { Signaled(status.signal_number()?) } else { Exited(status.exit_code()?) }
  let out = stdout.read_bytes()?
  let err = stderr.read_bytes()?
  stdout.remove()
  stderr.remove()
  Ok({util: command, args: [word.display() for word in args], termination: termination, stdout: out, stderr: err})
}

## Builds lossless reference argv with the same isolated launcher policy as candidate fixtures.
export proc reference_argv(s: uu.Scene, command: Str, args: List[Path], vars: Record = {}) [error] -> Result[List[Path], Error] {
  assert command in ["lslocks", "lsipc", "lsmem", "mountpoint", "namei"]
  let executable = if command == "mountpoint" { p"/bin/mountpoint" } else { fp"/usr/bin/{command}" }
  let metadata = json.encode({caller_keys: vars.keys(), umask: null})?
  let launcher = fp"{s.ctx.core_dir}/tests/support/uu-launch.xsh"
  Ok([s.ctx.xsh_bin, launcher, fp"{metadata}", executable].extend(args))
}

proc exited(observation: Observed) [error] -> Result[uu.Ran, Error] {
  match observation.termination {
    Exited(code) => Ok({util: observation.util, args: observation.args, status: code, stdout: observation.stdout, stderr: observation.stderr})
    Signaled(number) => Err(error.failure(f"{observation.util}: signal {number} requires observed capture"))
  }
}

## Runs the reference while preserving actual signal termination.
export proc observe_reference(s: uu.Scene, command: Str, args: List[Path], vars: Record = {}, timeout = 5s) [fs, process, error] -> Result[Observed, Error] {
  assert timeout > 0ms and timeout <= 30s
  observe(s, command, args, reference_argv(s, command, args, vars)?, vars, timeout)
}

## Runs the candidate while preserving actual signal termination.
export proc observe_candidate(s: uu.Scene, command: Str, args: List[Path], vars: Record = {}, timeout = 5s) [fs, process, error] -> Result[Observed, Error] {
  assert timeout > 0ms and timeout <= 30s
  observe(s, command, args, uu.argv(s, command, args, vars)?, vars, timeout)
}

## Compares signal-aware observations and explicit fixture-state snapshots exactly.
export pure observed_equal(reference: Observed, candidate: Observed, reference_state = b"", candidate_state = b"") -> Bool {
  reference == candidate and reference_state == candidate_state
}

pure under_uid(words: List[Path], uid: Int) -> List[Path] {
  [p"/bin/setpriv", fp"--reuid={uid}", fp"--regid={uid}", p"--clear-groups",
    p"--bounding-set=-all", p"--inh-caps=-all", p"--ambient-caps=-all", p"--no-new-privs"].extend(words)
}

## Runs the reference as one of the two unprivileged fixture identities with capabilities dropped.
export proc reference_as_user(s: uu.Scene, command: Str, args: List[Str], uid: Int, vars: Record = {}, timeout = 5s) [fs, process, env, error] -> Result[uu.Ran, Error] {
  assert uid in [10001, 10002]
  require_private_namespace()
  let paths = [fp"{word}" for word in args]
  exited(observe(s, command, paths, under_uid(reference_argv(s, command, paths, vars)?, uid), vars, timeout)?)
}

## Runs the candidate as the same unprivileged fixture identity and capability set as the reference.
export proc candidate_as_user(s: uu.Scene, command: Str, args: List[Str], uid: Int, vars: Record = {}, timeout = 5s) [fs, process, env, error] -> Result[uu.Ran, Error] {
  assert uid in [10001, 10002]
  require_private_namespace()
  let paths = [fp"{word}" for word in args]
  exited(observe(s, command, paths, under_uid(uu.argv(s, command, paths, vars)?, uid), vars, timeout)?)
}

## Requires the coordinator's private IPC, mount and PID namespaces before privileged fixtures.
export proc require_private_namespace() [fs, env, error] {
  assert e"XSH_UTIL_LINUX_PRIVATE"? == "1"
  for name in ["ipc", "mnt", "pid"] {
    let outer = env.get(f"XSH_UTIL_LINUX_PARENT_{name}")?
    assert fp"/proc/self/ns/{name}".readlink()?.display() != outer
  }
  assert p"/proc/1/ns/pid".readlink()? == p"/proc/self/ns/pid".readlink()?
}

## Runs the reference with lossless arguments in the candidate's scene and environment.
export proc reference_paths(
  s: uu.Scene,
  command: Str,
  args: List[Path],
  timeout = 5s,
  vars: Record = {},
) [fs, process, error] -> Result[uu.Ran, Error] {
  exited(observe_reference(s, command, args, vars:, timeout:)?)
}

## Runs the reference with text arguments and the same capture contract as uu.invoke.
export proc reference(
  s: uu.Scene,
  command: Str,
  args: List[Str],
  timeout = 5s,
  vars: Record = {},
) [fs, process, error] -> Result[uu.Ran, Error] {
  reference_paths(s, command, [fp"{word}" for word in args], timeout:, vars:)
}

## Compares every captured byte, status, argument and explicit fixture-state snapshot.
export pure equal(reference: uu.Ran, candidate: uu.Ran, reference_state = b"", candidate_state = b"") -> Bool {
  reference.util == candidate.util and reference.args == candidate.args and
    reference.status == candidate.status and reference.stdout == candidate.stdout and
    reference.stderr == candidate.stderr and reference_state == candidate_state
}

pure byte_difference(label: Str, reference: Bytes, candidate: Bytes) -> Str {
  var offset = 0
  let limit = if reference.len() < candidate.len() { reference.len() } else { candidate.len() }
  while offset < limit and reference.slice(offset, length: 1) == candidate.slice(offset, length: 1) { offset += 1 }
  let start = if offset > 8 { offset - 8 } else { 0 }
  let reference_remaining = reference.len() - start
  let candidate_remaining = candidate.len() - start
  let reference_length = if reference_remaining > 24 { 24 } else { reference_remaining }
  let candidate_length = if candidate_remaining > 24 { 24 } else { candidate_remaining }
  f"{label} byte={offset} ref_len={reference.len()} candidate_len={candidate.len()} start={start} ref_b64={reference.slice(start, length: reference_length).base64()} candidate_b64={candidate.slice(start, length: candidate_length).base64()}"
}

## Asserts exact equality without normalizing identifiers, ordering or whitespace.
export proc compare(reference: uu.Ran, candidate: uu.Ran, reference_state = b"", candidate_state = b"") {
  assert reference.util == candidate.util
  assert reference.args == candidate.args
  assert reference.status == candidate.status, candidate.stderr.utf8() ?? "candidate stderr is not UTF-8"
  assert reference.stdout == candidate.stdout, byte_difference("stdout", reference.stdout, candidate.stdout)
  assert reference.stderr == candidate.stderr, byte_difference("stderr", reference.stderr, candidate.stderr)
  assert reference_state == candidate_state, byte_difference("fixture state", reference_state, candidate_state)
}

## Asserts each implementation's exact provenance for the sole version exception.
export proc version_identity(reference: uu.Ran, candidate: uu.Ran) {
  assert reference.util == candidate.util
  assert reference.args in [["--version"], ["-V"]] and reference.args == candidate.args
  assert reference.status == 0 and candidate.status == 0
  assert reference.stderr == b"" and candidate.stderr == b""
  assert reference.stdout == bytes.from_text(f"{reference.util} from util-linux 2.42.4\n")
  assert candidate.stdout == bytes.from_text(gnu.version_text(candidate.util) + "\n")
}
