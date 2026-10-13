##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_yes.rs.

use support.uu

pure repeat_text(word: Str, count: Int) -> Str {
  let width = if count < 1024 { count } else { 1024 }
  let block = [word for _ in range(width)].join("")
  [block for _ in range(count / width)].join("") + [word for _ in range(count % width)].join("")
}

# The only reader closes after the entire expected prefix, so the producer's
# termination must be SIGPIPE rather than an ordinary exit with code 141.
proc yes_prefix(s: uu.Scene, args: List[Path], expected: Bytes) [fs, process, net, env, time, error] {
  let fifo = uu.at(s, "stdout-pipe")
  fs.mkfifo(fifo, 0o600)
  let reader = unix.open_fd(fifo, nonblock: true)?
  var reader_open = true
  defer { if reader_open { unix.close_fd(reader) } }
  let argv = uu.argv(s, "yes", args)?
  let child = spawn process.command_argv(s.ctx.xsh_bin, argv, s.root, {}, b"", fifo, uu.at(s, "stderr"), timeout: 60s)?
  defer child.cancel(kill_after: 100ms)
  let deadline = time.now() + 60000
  var offset = 0
  while offset < expected.len() {
    assert time.now() < deadline, f"yes prefix stopped at {offset} of {expected.len()} bytes"
    let ready = unix.poll_fd(reader, ["readable"], timeout_ms: 100)?
    if "readable" in ready {
      let remaining = expected.len() - offset
      let amount = if remaining < 65536 { remaining } else { 65536 }
      let data = unix.read_fd(reader, amount)?
      assert ! data.is_empty(), f"yes prefix ended at {offset}"
      assert data == expected[offset..offset + data.len()], f"yes prefix differs at byte {offset}"
      offset += data.len()
    }
  }
  unix.close_fd(reader)
  reader_open = false
  let finished = process.wait_timeout([child], 60s)?
  if let result = finished {
    assert result.status.signaled(), "yes must die from SIGPIPE after closing its sole reader"
    assert result.status.signal_number()? == process.signal("PIPE")?.number
  } else {
    test.fail("yes did not terminate after closing its sole reader")
  }
  Ok()
}

# origin: uutils test_yes::test_args
test test_uu_yes_args { |ctx|
  let s = uu.scene(ctx)?
  yes_prefix(s, [p"a", p"bar", p"c"], b"a bar c\na bar c\na ba")
}

# origin: uutils test_yes::test_invalid_arg
test test_uu_yes_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "yes", ["--definitely-invalid"], timeout: 60s)?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_yes::test_long_input
test test_uu_yes_long_input { |ctx|
  let s = uu.scene(ctx)?
  let word = repeat_text("abcdef", 14000)
  let expected = bytes.from_text(repeat_text(word + "\n", 5))
  yes_prefix(s, [fp"{word}"], expected)
}

# origin: uutils test_yes::test_long_line_exceeds_pipe_capacity
test test_uu_yes_long_line_exceeds_pipe_capacity { |ctx|
  let s = uu.scene(ctx)?
  let word = repeat_text("a", 60000)
  let args = [word for _ in range(20)]
  let line = args.join(" ")
  yes_prefix(s, [fp"{arg}" for arg in args], bytes.from_text(line + "\n" + line + "\n"))
}

# origin: uutils test_yes::test_long_odd_output
test test_uu_yes_long_odd_output { |ctx|
  let s = uu.scene(ctx)?
  yes_prefix(s, [p"abcdef"], bytes.from_text(repeat_text("abcdef\n", 1MiB)))
}

# origin: uutils test_yes::test_long_output
test test_uu_yes_long_output { |ctx|
  let s = uu.scene(ctx)?
  yes_prefix(s, [], bytes.from_text(repeat_text("y\n", 512KiB)))
}

# origin: uutils test_yes::test_non_utf8
test test_uu_yes_non_utf8 { |ctx|
  let s = uu.scene(ctx)?
  let word = b"\xbf\xff\xee" as Path
  let expected = bytes.concat([b"\xbf\xff\xee bar\n" for _ in range(5000)])
  yes_prefix(s, [word, p"bar"], expected)
}

# origin: uutils test_yes::test_piped_to_dev_full
test test_uu_yes_piped_to_dev_full { |ctx|
  for append in [true, false] {
    let s = uu.scene(ctx)?
    let r = uu.invoke(s, "yes", [], stdout: p"/dev/full", stdout_append: append, timeout: 60s)?
    uu.fails(r)
    uu.stderr_contains(r, "No space left on device")
  }
}

# origin: uutils test_yes::test_simple
test test_uu_yes_simple { |ctx|
  let s = uu.scene(ctx)?
  yes_prefix(s, [], b"y\ny\ny\ny\n")
}

# origin: uutils test_yes::test_version
test test_uu_yes_version { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "yes", ["--version"], timeout: 60s)?
  uu.succeeds(r)
}
