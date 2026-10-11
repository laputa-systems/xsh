type DebugCase = {args: List[Str], message: Str}
use support.uu

proc modes() -> List[List[Str]] { [[], ["---disable-inotify"]] }
proc fast(args: List[Str], mode: List[Str]) -> List[Str] {
  mode.extend(["-s.1", "--max-unchanged-stats=1"]).extend(args)
}
proc following(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[ProcessHandle, Error] {
  uu.touch(s, "out")?
  uu.touch(s, "err")?
  let plan = uu.command(s, "tail", args, stdout: uu.at(s, "out"), stderr: uu.at(s, "err"), timeout: 60s)?
  Ok(spawn plan?)
}
proc seen(s: uu.Scene, file: Str, needle: Str, attempts: Int = 7) [fs, process, time, error] {
  var delay = 100ms
  for _ in range(attempts) {
    if needle in uu.read_text(s, file)? { return }
    time.sleep(delay)?
    delay = delay * 2
  }
  assert needle in uu.read_text(s, file)?, f"{file} lacks {needle}: {uu.read_text(s, file)?}"
}
proc live(child: ProcessHandle) [process, error] {
  assert process.wait_timeout([child], 0ms)? == null, "follower exited"
}
proc stop(child: ProcessHandle) [process, error] { child.cancel(signal: "KILL", kill_after: 0ms)? }
proc sequence(first: Int, last: Int) -> Str { [f"{n}\n" for n in range(first, last + 1)].join("") }

# origin: gnu tail/F-headers.log
test test_gnu_tail_F_headers_log { |ctx|
  for mode in modes() {
    let s = uu.scene(ctx)?
    let child = following(s, fast(["-F", "a", "b"], mode))?
    defer stop(child)
    seen(s, "err", "cannot open 'b'")
    uu.write(s, "a", "x\n")?
    seen(s, "out", "==> a <==")
    uu.write(s, "b", "y\n")?
    seen(s, "out", "==> b <==")
  }
}
# origin: gnu tail/F-vs-missing.log
test test_gnu_tail_F_vs_missing_log { |ctx|
  for mode in modes() {
    let s = uu.scene(ctx)?
    let child = following(s, fast(["-F", "missing/file"], mode))?
    defer stop(child)
    seen(s, "err", "cannot open")
    uu.mkdir(s, "missing")?
    uu.write(s, "missing/file", "x\n")?
    seen(s, "err", "has appeared")
  }
}
# origin: gnu tail/F-vs-rename.log
test test_gnu_tail_F_vs_rename_log { |ctx|
  for mode in modes() {
    let s = uu.scene(ctx)?
    uu.touch(s, "a")?
    uu.touch(s, "b")?
    let child = following(s, fast(["-F", "a", "b"], mode))?
    defer stop(child)
    uu.write(s, "a", "x\n")?
    seen(s, "out", "==> a <==\nx\n")
    uu.at(s, "a").rename(to: uu.at(s, "b"), overwrite: true)?
    seen(s, "err", "inaccessible")
    seen(s, "err", "replaced")
    seen(s, "out", "==> b <==\nx\n")
    uu.write(s, "a", "x2\n")?
    seen(s, "err", "has appeared")
    seen(s, "out", "==> a <==\nx2\n")
    uu.append(s, "b", "y\n")?
    seen(s, "out", "==> b <==\ny\n")
    uu.append(s, "a", "z\n")?
    seen(s, "out", "==> a <==\nz\n")
  }
}

# origin: gnu tail/assert-2.log
test test_gnu_tail_assert_2_log { |ctx|
  for mode in modes() {
    let s = uu.scene(ctx)?
    uu.touch(s, "a")?
    let child = following(s, fast(["-F", "a", "foo"], mode))?
    defer stop(child)
    uu.write(s, "a", "x\n")?
    seen(s, "out", "x\n")
    uu.write(s, "foo", "ok ok ok\n")?
    seen(s, "out", "ok ok ok\n")
  }
}

# origin: gnu tail/assert.log
test test_gnu_tail_assert_log { |ctx|
  for mode in modes() {
    let s = uu.scene(ctx)?
    uu.touch(s, "a")?
    uu.touch(s, "foo")?
    let child = following(s, fast(["--follow=name", "a", "foo"], mode))?
    defer stop(child)
    uu.write(s, "a", "x\n")?
    seen(s, "out", "x\n")
    uu.remove(s, "foo")?
    seen(s, "err", "No such file")
    uu.write(s, "foo", "ok ok ok\n")?
    seen(s, "out", "ok ok ok\n")
  }
}

# origin: gnu tail/descriptor-vs-rename.log
test test_gnu_tail_descriptor_vs_rename_log { |ctx|
  for mode in modes() {
    let s = uu.scene(ctx)?
    uu.touch(s, "a")?
    let child = following(s, fast(["-f", "a"], mode))?
    defer stop(child)
    uu.write(s, "a", "x\n")?
    seen(s, "out", "x\n")
    uu.at(s, "a").rename(to: uu.at(s, "b"), overwrite: true)?
    uu.append(s, "b", "y\n")?
    seen(s, "out", "y\n")
  }
}
# origin: gnu tail/follow-symlink.log
test test_gnu_tail_follow_symlink_log { |ctx|
  for mode in modes() {
    let s = uu.scene(ctx)?
    uu.touch(s, "file")?
    uu.symlink(s, "file", "link")?
    let child = following(s, fast(["-f", "link"], mode))?
    defer stop(child)
    for letter in ["a", "b", "c"] {
      uu.append(s, "file", f"{letter}\n")?
      seen(s, "out", f"{letter}\n")
    }
    assert uu.read(s, "out")? == uu.read(s, "file")?
    assert uu.read(s, "err")? == b""
  }
}
# origin: gnu tail/flush-initial.log
test test_gnu_tail_flush_initial_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "line\n")?
  let child = following(s, fast(["-f", "in"], []))?
  defer stop(child)
  seen(s, "out", "line\n", 5)
}
# origin: gnu tail/truncate.log
test test_gnu_tail_truncate_log { |ctx|
  for follow in ["-f", "-F"] {
    for mode in modes() {
      let s = uu.scene(ctx)?
      uu.write(s, "f", sequence(1, 10))?
      let child = following(s, fast([follow, "f"], mode))?
      defer stop(child)
      seen(s, "out", "10\n")
      uu.write(s, "f", sequence(11, 15))?
      seen(s, "out", "15\n")
    }
  }
}
# origin: gnu tail/inotify-hash-abuse.log
test test_gnu_tail_inotify_hash_abuse_log { |ctx|
  for mode in modes() {
    let s = uu.scene(ctx)?
    let names = [f"{n}" for n in range(1, 10)]
    for name in names { uu.touch(s, name)? }
    let child = following(s, fast(["-qF"].extend(names), mode))?
    defer stop(child)
    uu.write(s, "9", "x\n")?
    seen(s, "out", "x\n")
    uu.rename(s, "1", "f")?
    seen(s, "err", "inaccessible")
    uu.write(s, "1", "a\n")?
    seen(s, "err", "has appeared", 6)
    live(child)
  }
}
# origin: gnu tail/inotify-hash-abuse2.log
test test_gnu_tail_inotify_hash_abuse2_log { |ctx|
  for mode in modes() {
    let s = uu.scene(ctx)?
    uu.touch(s, "f")?
    let child = following(s, fast(["-F", "f"], mode))?
    defer stop(child)
    for _ in range(200) {
      live(child)
      uu.touch(s, "g")?
      uu.at(s, "g").rename(to: uu.at(s, "f"), overwrite: true)?
    }
    live(child)
  }
}
# origin: gnu tail/inotify-rotate.log
test test_gnu_tail_inotify_rotate_log { |ctx|
  for _ in range(50) {
    let s = uu.scene(ctx)?
    uu.touch(s, "k")?
    uu.touch(s, "x")?
    let child = following(s, fast(["-F", "k"], []))?
    defer stop(child)
    uu.write(s, "k", "tailed\n")?
    seen(s, "out", "tailed", 8)
    uu.at(s, "x").rename(to: uu.at(s, "k"), overwrite: true)?
    seen(s, "err", "tail:", 8)
    uu.append(s, "k", "ok\n")?
    seen(s, "out", "ok", 8)
  }
}
# origin: gnu tail/basic-seek.log
test test_gnu_tail_basic_seek_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file.in", ["=================================\n" for _ in range(1024)].join(""))?
  let r = uu.invoke(s, "tail", ["-n200", "file.in"])?
  uu.succeeds(r)
  assert r.stdout.utf8()?.split("\n").len() == 201
}
# origin: gnu tail/big-4gb.log
test test_gnu_tail_big_4gb_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "big", "abcdefgh")?
  assert bytes.write_at(uu.at(s, "big"), 4294967288, b"87654321")? == 8
  let r = uu.invoke(s, "tail", ["-c1", "big"])?
  uu.succeeds(r)
  uu.stdout_is(r, "1")
}
# origin: gnu tail/quote-headers.log
test test_gnu_tail_quote_headers_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "\n")?
  uu.touch(s, "normal")?
  let r = uu.invoke(s, "tail", ["-n1", "\n", "normal"])?
  uu.succeeds(r)
  uu.stdout_is(r, "==> ''$'\\n' <==\n\n==> normal <==\n")
}
# origin: gnu tail/proc-ksyms.log
test test_gnu_tail_proc_ksyms_log { |ctx|
  if !p"/proc/ksyms".exists()? { return }
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "tail", ["/proc/ksyms"] )?)
}
# origin: gnu tail/tail-sysfs.log
test test_gnu_tail_tail_sysfs_log { |ctx|
  if !p"/sys/kernel/profiling".exists()? { return }
  let s = uu.scene(ctx)?
  let expected = p"/sys/kernel/profiling".read_bytes()?
  for arg in ["-n1", "-c2"] {
    let r = uu.invoke(s, "tail", [arg, "/sys/kernel/profiling"])?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, expected)
  }
}
# origin: gnu tail/tail-c.log
test test_gnu_tail_tail_c_log { |ctx|
  let s = uu.scene(ctx)?
  for name in ["/proc/version", "/sys/kernel/profiling"] {
    if Path(name).exists()? {
      uu.write_bytes(s, "copy", Path(name).read_bytes()?)?
      let expected = uu.invoke(s, "tail", ["-c", "-1", "copy"])?
      uu.succeeds(expected)
      let actual = uu.invoke(s, "tail", ["-c", "-1", name])?
      uu.succeeds(actual)
      uu.stdout_is_bytes(actual, expected.stdout)
    }
  }
  let pipe = uu.invoke(s, "tail", ["-c3"], b"123456")?
  uu.succeeds(pipe)
  uu.stdout_is(pipe, "456")
  let zero = uu.invoke(s, "tail", ["-c", "4096", "/dev/zero"], timeout: 10s)?
  uu.succeeds(zero)
  uu.stdout_is_bytes(zero, bytes.concat([b"\0" for _ in range(4096)]))
  if p"/dev/urandom".exists()? { uu.succeeds(uu.invoke(s, "tail", ["-c", "4096", "/dev/urandom"], timeout: 10s)?) }
}

proc wrapped(s: uu.Scene, prefix: List[Path], args: List[Str], input: Path? = null, timeout: Duration = 10s) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let tail = uu.argv(s, "tail", [Path(arg) for arg in args])?
  let words = if prefix[0] == p"timeout" {
    uu.argv(s, "timeout", prefix[1..].extend(tail))?
  } else { prefix.extend(tail) }
  let out = uu.at(s, "wrapped-out")
  let err = uu.at(s, "wrapped-err")
  let plan = if let source = input {
    process.command_argv(words[0], words, s.root, {}, source, out, err, timeout: timeout)
  } else {
    process.command_argv(words[0], words, s.root, {}, b"", out, err, timeout: timeout)
  }
  let status = process.run(plan)?
  Ok({util: "tail", args: args, status: status.shell_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}
# origin: gnu tail/start-middle.log
test test_gnu_tail_start_middle_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "k", "1\n2\n")?
  let r = wrapped(s, [p"sh", p"-c", p"dd bs=1 count=2 of=/dev/null 2>/dev/null; exec \"$@\"", p"positioned"], [], uu.at(s, "k"))?
  uu.succeeds(r)
  uu.stdout_is(r, "2\n")
}
# origin: gnu tail/inotify-only-regular.log
test test_gnu_tail_inotify_only_regular_log { |ctx|
  let s = uu.scene(ctx)?
  let r = wrapped(s, [p"timeout", p".1", p"strace", p"-e", p"trace=inotify_add_watch", p"-o", p"strace.out"], ["-f", "/dev/null"])?
  uu.fails_with_code(r, 124)
  assert !("inotify" in uu.read_text(s, "strace.out")?)
}
# origin: gnu tail/debug.log
test test_gnu_tail_debug_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file.debug")?
  let probe = wrapped(s, [p"timeout", p".1", p"strace", p"-e", p"trace=inotify_add_watch", p"-o", p"strace.out"], ["-F", "file.debug"])?
  uu.fails_with_code(probe, 124)
  var cases: List[DebugCase] = [
    {args: ["--debug", "-f", "/dev/null"], message: "tail: using blocking mode"},
    {args: ["--debug", "-F", "/dev/null"], message: "tail: using polling mode"},
  ]
  if "inotify" in uu.read_text(s, "strace.out")? {
    cases = cases.extend([
      {args: ["--debug", "-n0", "-f", "file.debug"], message: "tail: using notification mode"},
      {args: ["--debug", "---disable-inotify", "-f", "file.debug"], message: "tail: using polling mode"},
    ])
  }
  for case in cases {
    let child = following(s, case.args)?
    defer stop(child)
    seen(s, "err", case.message, 5)
  }
}
# origin: gnu tail/pid.log
test test_gnu_tail_pid_log { |ctx|
  for mode in modes() {
    let s = uu.scene(ctx)?
    uu.touch(s, "empty")?
    uu.touch(s, "here")?
    let child = following(s, ["-f"].extend(mode).extend(["here"]))?
    defer stop(child)
    for pids in [[f"--pid={child.pid}"], ["--pid=2147483647", f"--pid={child.pid}"]] {
      let plan = uu.command(s, "tail", ["-f", "-s.1"].extend(pids).extend(mode).extend(["here"]), stdout: uu.at(s, "monitor-out"), stderr: uu.at(s, "monitor-err"))?
      let monitor = spawn plan?
      defer stop(monitor)
      assert process.wait_timeout([monitor], 1s)? == null
    }
    for interval in ["-s.1", "-s10"] {
      let r = uu.invoke(s, "tail", ["-f", interval, "--pid=2147483647"].extend(mode).extend(["empty"]), timeout: 10s)?
      uu.succeeds(r)
    }
  }
}
# origin: gnu tail/tail-n0f.log
test test_gnu_tail_tail_n0f_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "empty")?
  uu.write(s, "nonempty", "anything\n")?
  uu.touch(s, "unreadable")?
  uu.set_mode(s, "unreadable", 0)?
  for unit in ["c", "n"] { uu.succeeds(uu.invoke(s, "tail", [f"-{unit}0", "unreadable"])?) }
  for mode in modes() {
    for file in ["empty", "nonempty"] {
      for unit in ["c", "n"] {
        let child = following(s, ["--sleep=4", f"-{unit}", "0", "-f"].extend(mode).extend([file]))?
        defer stop(child)
        var sleeping = false
        var delay = 100ms
        for _ in range(4) {
          time.sleep(delay)?
          let state = fp"/proc/{child.pid}/status".read_text()?
          if "State:\tS" in state { sleeping = true; break }
          delay = delay * 2
        }
        assert sleeping, "zero-count follower did not sleep within 1.5 seconds"
      }
    }
  }
}
# origin: gnu tail/pipe-f2.log
test test_gnu_tail_pipe_f2_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "fifo")?
  let writer = spawn process.command_argv("sh", ["sh", "-c", "printf '1\\n' > fifo"], s.root)?
  defer stop(writer)
  let child = following(s, fast(["-f", "fifo"], []))?
  defer stop(child)
  seen(s, "out", "1\n")
  assert uu.read(s, "out")? == b"1\n"
  live(child)
}
# origin: gnu tail/pid-pipe.log
test test_gnu_tail_pid_pipe_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "fifo")?
  for opened in [false, true] {
    if opened {
      let writer = spawn process.command_argv("sh", ["sh", "-c", "exec 3>fifo; touch writer-ready; sleep 20"], s.root)?
      defer stop(writer)
      let reader = spawn process.command_argv("sh", ["sh", "-c", "cat fifo >/dev/null"], s.root)?
      defer stop(reader)
      var ready = false
      var delay = 100ms
      for _ in range(7) {
        if uu.exists(s, "writer-ready")? { ready = true; break }
        time.sleep(delay)?
        delay = delay * 2
      }
      assert ready
      stop(reader)
      let sleeper = spawn run sleep 1 ?
      defer stop(sleeper)
      let plan = uu.command(s, "tail", fast(["-f", f"--pid={sleeper.pid}", "fifo"], []))?
      let child = spawn plan?
      defer stop(child)
      let slept = wait sleeper?
      assert slept.exited_with(0)
      assert process.wait_timeout([child], 9s)? != null
    } else {
      let sleeper = spawn run sleep 1 ?
      defer stop(sleeper)
      let plan = uu.command(s, "tail", fast(["-f", f"--pid={sleeper.pid}", "fifo"], []))?
      let child = spawn plan?
      defer stop(child)
      let slept = wait sleeper?
      assert slept.exited_with(0)
      assert process.wait_timeout([child], 9s)? != null
    }
  }
}

proc combined(s: uu.Scene) [fs, error] -> Result[Str, Error] {
  Ok(uu.read_text(s, "err")? + uu.read_text(s, "out")?)
}
proc line_count(s: uu.Scene, expected: Int) [fs, error] {
  let lines = combined(s)?.split("\n")
  let filtered = [line for line in lines if line != "" and !("inotify resources exhausted" in line) and !("inotify cannot be used" in line)]
  assert filtered.len() == expected, combined(s)?
}
# origin: gnu tail/symlink.log
test test_gnu_tail_symlink_log { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "target", "symlink")?
  let first = following(s, fast(["-F", "symlink"], []))?
  defer stop(first)
  seen(s, "err", "cannot open", 6)
  uu.write(s, "target", "X\n")?
  seen(s, "err", "has appeared", 6)
  seen(s, "out", "X\n", 6)
  stop(first)
  line_count(s, 3)
  uu.remove(s, "target")?
  uu.remove(s, "symlink")?
  uu.write(s, "target1", "X1\n")?
  uu.symlink(s, "target1", "symlink")?
  let second = following(s, fast(["-F", "symlink"], []))?
  defer stop(second)
  seen(s, "out", "X1\n", 6)
  uu.remove(s, "symlink")?
  uu.symlink(s, "target2", "symlink")?
  seen(s, "err", "become inacce", 6)
  uu.write(s, "target2", "X2\n")?
  seen(s, "err", "has appeared", 6)
  seen(s, "out", "X2\n", 6)
  stop(second)
  line_count(s, 4)
}
# origin: gnu tail/retry.log
test test_gnu_tail_retry_log { |ctx|
  let initial = uu.scene(ctx)?
  uu.touch(initial, "file")?
  let existing = uu.invoke(initial, "tail", ["--retry", "file"])?
  uu.succeeds(existing)
  uu.stderr_contains(existing, "tail: warning: --retry ignored")
  assert existing.stderr.utf8()?.split("\n").len() == 2
  let absent = uu.invoke(initial, "tail", ["--retry", "missing"])?
  uu.fails_with_code(absent, 1)
  uu.stderr_contains(absent, "tail: warning: --retry ignored")
  assert absent.stderr.utf8()?.split("\n").len() == 3
  for mode in modes() {
    for follow in ["name", "descriptor"] {
      let s = uu.scene(ctx)?
      let child = following(s, fast([f"--follow={follow}", "--retry", "missing"], mode))?
      defer stop(child)
      seen(s, "err", "cannot open", 6)
      if follow == "descriptor" { seen(s, "err", "retry only effective for the initial open", 6) }
      uu.write(s, "missing", if follow == "name" { "X\n" } else { "X1\n" })?
      seen(s, "err", "has appeared", 6)
      seen(s, "out", if follow == "name" { "X\n" } else { "X1\n" }, 6)
      if follow == "descriptor" {
        uu.write(s, "missing", "X\n")?
        seen(s, "err", "file truncated", 6)
        seen(s, "out", "X\n", 6)
      }
      stop(child)
      line_count(s, if follow == "name" { 3 } else { 6 })
    }
    let directory = uu.scene(ctx)?
    let plan = uu.command(directory, "tail", fast(["--follow=descriptor", "--retry", "missing"], mode), stdout: uu.at(directory, "out"), stderr: uu.at(directory, "err"))?
    let follower = spawn plan?
    defer stop(follower)
    seen(directory, "err", "cannot open", 6)
    seen(directory, "err", "retry only effective for the initial open", 6)
    uu.mkdir(directory, "missing")?
    seen(directory, "err", "replaced with an untailable file", 6)
    seen(directory, "err", "no files remaining", 6)
    let finished = process.wait_timeout([follower], 10s)?
    assert finished != null
    assert finished.status.exit_code()? == 1
    line_count(directory, 4)

    let mixed = uu.scene(ctx)?
    uu.touch(mixed, "existing")?
    let child = following(mixed, fast(["--follow=descriptor", "missing", "existing"], mode))?
    defer stop(child)
    seen(mixed, "err", "cannot open", 6)
    seen(mixed, "out", "==> existing <==", 6)
    line_count(mixed, 2)
    uu.write(mixed, "missing", "Y\n")?
    uu.write(mixed, "existing", "X\n")?
    seen(mixed, "out", "X\n", 6)
    line_count(mixed, 3)
    assert !("Y\n" in uu.read_text(mixed, "out")?)
    stop(child)
    for follow in ["descriptor", "name"] {
      let fresh = uu.scene(ctx)?
      let r = uu.invoke(fresh, "tail", mode.extend([f"--follow={follow}", "missing"]))?
      uu.fails_with_code(r, 1)
      uu.stderr_contains(r, "cannot open")
      uu.stderr_contains(r, "no files remaining")
      assert r.stderr.utf8()?.split("\n").len() == 3
    }
    let untailable = uu.scene(ctx)?
    uu.mkdir(untailable, "untailable")?
    let directory_child = following(untailable, fast(["-F", "untailable"], mode))?
    defer stop(directory_child)
    seen(untailable, "err", "cannot follow", 6)
    line_count(untailable, 2)
    uu.remove(untailable, "untailable")?
    uu.write(untailable, "untailable", "foo\n")?
    seen(untailable, "out", "foo", 6)
    let diagnostics = uu.read_text(untailable, "err")?
    assert "become accessible" in diagnostics or "has appeared" in diagnostics
    assert !("giving up" in diagnostics)
    stop(directory_child)
    line_count(untailable, 4)
  }
}
# origin: gnu tail/wait.log
test test_gnu_tail_wait_log { |ctx|
  for mode in modes() {
    let s = uu.scene(ctx)?
    uu.touch(s, "here")?
    uu.touch(s, "unreadable")?
    uu.set_mode(s, "unreadable", 0o222)?
    for file in ["not_here", "unreadable"] {
      let r = wrapped(s, [p"timeout", p"10"], fast(["-f", file], mode))?
      assert r.status != 124
    }
    for follow in ["-f", "-F"] {
      let r = wrapped(s, [p"timeout", p".1"], fast([follow, "here"], mode))?
      uu.fails_with_code(r, 124)
      assert r.stderr == b""
    }
    for file in ["not_here", "unreadable"] {
      let r = wrapped(s, [p"timeout", p".1"], fast(["-F", file], mode))?
      uu.fails_with_code(r, 124)
    }
  }
}

proc repeated(value: Str, count: Int) -> Str { [value for _ in range(count)].join("") }
type MatrixCase = {name: Str, args: List[Str], input: Str, output: Str, status: Int, error: Str, normalize: Bool}
proc matrix_case(name: Str, args: List[Str], input: Str, output: Str, status: Int = 0, error: Str = "", normalize: Bool = false) -> MatrixCase {
  {name: name, args: args, input: input, output: output, status: status, error: error, normalize: normalize}
}
# origin: gnu tail/tail.log
test test_gnu_tail_tail_log { |ctx|
  let lines = "x\n" + repeated("y\n", 10) + "z"
  let last = repeated("y\n", 9) + "z"
  let cases = [
    matrix_case("obs-plus-c1", ["+2c"], "abcd", "bcd"),
    matrix_case("obs-plus-c2", ["+8c"], "abcd", ""),
    matrix_case("obs-plus-c3", ["+999999999999999999999999999999999999999999c"], "abcd", ""),
    matrix_case("obs-c3", ["-1c"], "abcd", "d"),
    matrix_case("obs-c4", ["-9c"], "abcd", "abcd"),
    matrix_case("obs-c5", ["-12c"], "x" + repeated("y", 12) + "z", repeated("y", 11) + "z"),
    matrix_case("obs-l1", ["-1l"], "x", "x"),
    matrix_case("obs-l2", ["-1l"], "x\ny\n", "y\n"),
    matrix_case("obs-l3", ["-1l"], "x\ny", "y"),
    matrix_case("obs-plus-l4", ["+1l"], "x\ny\n", "x\ny\n"),
    matrix_case("obs-plus-l5", ["+2l"], "x\ny\n", "y\n"),
    matrix_case("obs-1", ["-1"], "x", "x"),
    matrix_case("obs-2", ["-1"], "x\ny\n", "y\n"),
    matrix_case("obs-3", ["-1"], "x\ny", "y"),
    matrix_case("obs-plus-4", ["+1"], "x\ny\n", "x\ny\n"),
    matrix_case("obs-plus-5", ["+2"], "x\ny\n", "y\n"),
    matrix_case("obs-plus-x1", ["+c"], "x" + repeated("y", 10) + "z", "yyz"),
    matrix_case("obs-plus-x2", ["+l"], lines, "y\ny\nz"),
    matrix_case("obs-l", ["-l"], lines, last),
    matrix_case("obs-b", ["-b"], repeated("x\n", 2561), repeated("x\n", 2560)),
    matrix_case("err-1", ["+cl"], "", "", 1, "tail: cannot open '+cl' for reading: No such file or directory\n"),
    matrix_case("err-2", ["-cl"], "", "", 1, "tail: invalid number of bytes: 'l'\n", true),
    matrix_case("err-3", ["+2cz"], "", "", 1, "tail: cannot open '+2cz' for reading: No such file or directory\n"),
    matrix_case("err-4", ["-2cX"], "", "", 1, "tail: option used in invalid context -- 2\n"),
    matrix_case("err-6", ["-c", "--"], "", "", 1, "tail: invalid number of bytes: '-'\n", true),
    matrix_case("big-c", ["-c99999999999999999999"], "", ""),
    matrix_case("minus-1", ["-"], "", ""),
    matrix_case("minus-2", ["-"], lines, last),
    matrix_case("c-2", ["-c", "2"], "abcd\n", "d\n"),
    matrix_case("c-2-minus", ["-c", "2", "--"], "abcd\n", "d\n"),
    matrix_case("c2", ["-c2"], "abcd\n", "d\n"),
    matrix_case("c2-minus", ["-c2", "--"], "abcd\n", "d\n"),
    matrix_case("n-1", ["-n", "10"], lines, last),
    matrix_case("n-2", ["-n", "-10"], lines, last),
    matrix_case("n-3", ["-n", "+10"], lines, "y\ny\nz"),
    matrix_case("n-4", ["-n", "+0"], repeated("y\n", 5), repeated("y\n", 5)),
    matrix_case("n-4a", ["-n", "+1"], repeated("y\n", 5), repeated("y\n", 5)),
    matrix_case("n-5", ["-n", "-0"], repeated("y\n", 5), ""),
    matrix_case("n-5a", ["-n", "-1"], repeated("y\n", 5), "y\n"),
    matrix_case("n-5b", ["-n", "0"], repeated("y\n", 5), ""),
    matrix_case("f-pipe-1", ["-f", "-n", "1"], "a\nb\n", "b\n"),
    matrix_case("zero-1", ["-z", "-n", "1"], "x\0y", "y"),
    matrix_case("zero-2", ["-z", "-n", "2"], "x\0y", "x\0y"),
  ]
  for case in cases {
    let s = uu.scene(ctx)?
    uu.write(s, "input", case.input)?
    let vars: Record = if case.name.starts_with("minus-") { {_POSIX2_VERSION: "199209"} } else if case.name == "err-6" or case.name == "c-2" { {_POSIX2_VERSION: "200112"} } else if case.name.starts_with("obs-plus-") { {_POSIX2_VERSION: "200809"} } else if case.name.starts_with("f-pipe-") { {POSIXLY_CORRECT: "1"} } else { {} }
    for transport in ["file", "redirect", "pipe"] {
      if case.name.starts_with("f-") and transport != "pipe" { continue }
      if transport == "file" and (case.name.starts_with("minus-") or case.name == "err-1" or case.name == "err-3") { continue }
      let r = if transport == "file" { uu.invoke(s, "tail", case.args.extend(["input"]), vars: vars)? } else if transport == "redirect" { uu.invoke_from_path(s, "tail", case.args, uu.at(s, "input"), vars: vars)? } else { uu.invoke(s, "tail", case.args, bytes.from_text(case.input), vars: vars)? }
      assert r.status == case.status, f"{case.name}/{transport}: status {r.status}"
      uu.stdout_is(r, case.output)
      let diagnostics = r.stderr.utf8()?
      let normalized = if case.normalize { rx"': .*".replace(diagnostics, with: "'") } else { diagnostics }
      assert normalized == case.error, f"{case.name}/{transport}: stderr {diagnostics}"
    }
  }
}
