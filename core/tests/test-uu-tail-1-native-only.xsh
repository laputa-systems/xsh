##! Native-only timing coverage for the follow-by-name notification path.
use support.uu

proc follow(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[Command, Error] {
  let argv = uu.argv(s, "tail", [fp"{word}" for word in args])?
  let out = uu.at(s, ".live-out")
  let err = uu.at(s, ".live-err")
  Ok(process.command_argv(s.ctx.xsh_bin, argv, s.root, {}, b"", out, err))
}

proc streams(s: uu.Scene) [fs, error] -> Result[uu.Ran, Error] {
  let out = uu.at(s, ".live-out").read_bytes()?
  let err = uu.at(s, ".live-err").read_bytes()?
  Ok({util: "tail", args: [], status: 0, stdout: out, stderr: err})
}

proc alive(child: ProcessHandle) [process, error] {
  assert process.wait_timeout([child], 0ms)? == null
}

# The initial warnings prove startup reached its missing-file pass before creation.
proc wait_output(s: uu.Scene, out: Bytes, err: Bytes) [fs, process, time, error] {
  for i in range(500) {
    let current = streams(s)?
    return when current.stdout == out and current.stderr == err
    time.sleep(10ms)
  }
  let current = streams(s)?
  uu.stdout_is_bytes(current, out)
  uu.stderr_is_bytes(current, err)
}

# This default-inotify timing row is native-only: the pinned GNU oracle polls
# when every followed path starts absent, so its 100ms poll cannot satisfy the
# upstream 50ms checkpoints.
test test_uu_tail_follow_name_shows_headers_on_creation_default_50ms { |ctx|
  let s = uu.scene(ctx)?
  let delay = 50ms
  let plan = follow(s, ["-F", "-s.1", "--max-unchanged-stats=1", "log1", "log2"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  let initial_stderr = "tail: cannot open 'log1' for reading: No such file or directory\ntail: cannot open 'log2' for reading: No such file or directory\n"
  time.sleep(delay)
  alive(child)
  wait_output(s, b"", bytes.from_text(initial_stderr))
  uu.write(s, "log1", "ping\n")
  time.sleep(delay)
  uu.write(s, "log2", "pong\n")
  time.sleep(delay)
  alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
  let r = streams(s)?
  uu.stderr_is(r, initial_stderr + "tail: 'log1' has appeared;  following new file\ntail: 'log2' has appeared;  following new file\n")
  uu.stdout_is(r, "==> log1 <==\nping\n\n==> log2 <==\npong\n")
  uu.remove(s, "log1")
  uu.remove(s, "log2")
}
