use support.uu

type Live = {child: ProcessHandle, out: Path, err: Path, args: List[Str]}

pure repeated(text: Str, count: Int) -> Str {
  [text for _ in range(count)].join("")
}

pure sample_bytes(size: Int) -> Bytes {
  bytes.from_text(repeated("Az0b1\nQ", size / 7 + 1))[..size]
}

pure sample_lines(size: Int, delimiters: Int, final_newline: Bool) -> Bytes {
  let prefix_count = delimiters - (if final_newline { 1 } else { 0 })
  let positions = [(index + 1) * (size - 1) / (prefix_count + 1) for index in range(prefix_count)]
  let text = [if index in positions or (final_newline and index == size - 1) { "\n" } else { "a" } for index in range(size)].join("")
  bytes.from_text(text)
}

proc skip_lines(data: Bytes, count: Int) [error] -> Result[Bytes] {
  Ok(bytes.from_text(data.utf8()?.split("\n")[count..].join("\n")))
}

proc fixture(s: uu.Scene, name: Str) [fs, error] -> Result[Bytes] {
  fp"{s.ctx.core_dir}/tests/data/uutils/tail/{name}".read_bytes()
}

proc scene(ctx: TestContext) [fs, error] -> Result[uu.Scene] {
  let s = uu.scene(ctx)?
  for name in ["foobar.txt", "foobar2.txt", "foobar_with_null.txt", "follow_name.txt"] {
    uu.write_bytes(s, name, fixture(s, name)?)
  }
  Ok(s)
}

# Append through the kernel descriptor so the follower never sees a rewritten prefix.
proc append_bytes(file: Path, data: Bytes) [fs, process, net, error] {
  if !file.exists() { file.write(b"") }
  let fd = unix.open_fd(file, write: true, flags: ["append"])?
  defer unix.close_fd(fd)
  var offset = 0
  while offset < data.len() {
    let count = unix.write_fd(fd, data[offset..])?
    assert count > 0, "append must make progress"
    offset += count
  }
  Ok()
}

proc wrapped(s: uu.Scene, args: List[Str], words: List[Path], vars: Record = {}) [fs, process, error] -> Result[uu.Ran] {
  let captures = test.temp_dir(s.ctx, name: "tail-capture")?
  let out = fp"{captures}/stdout"
  let err = fp"{captures}/stderr"
  let plan = process.command_argv(words[0], words, s.root, vars, b"", out, err, timeout: 15s)
  let status = process.run(plan)?
  Ok({util: "tail", args: args, status: status.shell_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# The child's stdin is an open regular file whose cursor already passed the first line.
proc seek_stdin(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran] {
  let launcher = test.temp_file(s.ctx, name: "seek.xsh", contents: bytes.from_text(r"""proc main(...raw: List[Bytes]) [fs, process, error] {
  let words = [Path.parse_bytes(word)? for word in raw]
  unix.redirect_fd(0, words[0])?
  let _ = unix.seek_fd(0, 2)?
  unix.exec(process.command_argv(words[1], words[1..]))?
}
"""))?
  let argv = uu.argv(s, "tail", [Path(arg) for arg in args])?
  wrapped(s, args, [s.ctx.xsh_bin, launcher, uu.at(s, "k")].extend(argv))
}

proc start(s: uu.Scene, args: List[Str], held_stdin = false) [fs, process, env, error] -> Result[Live] {
  let captures = test.temp_dir(s.ctx, name: "tail-live")?
  let out = fp"{captures}/stdout"
  let err = fp"{captures}/stderr"
  out.write(b"")
  err.write(b"")
  let argv = uu.argv(s, "tail", [Path(arg) for arg in args])?
  let words = if held_stdin {
    let fifo = fp"{captures}/stdin"
    fs.mkfifo(fifo, 0o600)
    [p"/bin/sh", p"-c", p"exec 3<> \"$1\"; shift; exec \"$@\" <&3", p"sh", fifo].extend(argv)
  } else { argv }
  let plan = process.command_argv(words[0], words, s.root, {}, b"", out, err)
  let child = spawn plan?
  Ok({child: child, out: out, err: err, args: args})
}

proc alive(p: Live) [fs, process, error] {
  process.kill(p.child.pid, signal: "0")
  let stat = fp"/proc/{p.child.pid}/stat".read_text()?
  assert ") Z " not in stat and ") X " not in stat, "tail must remain alive"
  Ok()
}

# Cancellation kills and reaps the child even when an assertion exits early.
proc cleanup(p: Live) [process, error] {
  p.child.cancel(signal: "KILL", kill_after: 0ms)
  Ok()
}

proc finish(p: Live) [fs, process, error] -> Result[uu.Ran] {
  process.kill(p.child.pid, signal: "KILL")
  let status = wait p.child?
  Ok({util: "tail", args: p.args, status: status.shell_code()?, stdout: p.out.read_bytes()?, stderr: p.err.read_bytes()?})
}

proc checkpoint(p: Live, file: Path, expected: Str) [fs, process, time, error] {
  let began = time.now()
  while expected not in file.read_text()? {
    alive(p)
    assert time.now() - began < 5000, f"tail checkpoint missing: {expected}; stdout={p.out.read_text()?}; stderr={p.err.read_text()?}"
    time.sleep(10ms)
  }
  Ok()
}

pure shell_quote(word: Path) -> Str {
  "'" + word.display().replace("'", with: "'\"'\"'") + "'"
}

# origin: uutils test_tail::test_negative_indexing
test test_uu_tail_negative_indexing { |ctx|
  let s = scene(ctx)?
  let positive_lines = uu.invoke(s, "tail", ["-n", "5", "foobar.txt"])?
  uu.succeeds(positive_lines)
  let negative_lines = uu.invoke(s, "tail", ["-n", "-5", "foobar.txt"])?
  uu.succeeds(negative_lines)
  let positive_bytes = uu.invoke(s, "tail", ["-c", "20", "foobar.txt"])?
  uu.succeeds(positive_bytes)
  let negative_bytes = uu.invoke(s, "tail", ["-c", "-20", "foobar.txt"])?
  uu.succeeds(negative_bytes)
  assert positive_lines.stdout == negative_lines.stdout
  assert positive_bytes.stdout == negative_bytes.stdout
}

# origin: uutils test_tail::test_no_skip_after_error
test test_uu_tail_no_skip_after_error { |ctx|
  let s = scene(ctx)?
  uu.write(s, "f", "hello")
  let r = uu.invoke(s, "tail", ["/proc/self/mem", "f"], stdin: b"", timeout: 15s)?
  uu.fails(r)
  uu.stdout_contains(r, "hello")
}

# origin: uutils test_tail::test_no_such_file
test test_uu_tail_no_such_file { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["missing"], stdin: b"", timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "tail: cannot open 'missing' for reading: No such file or directory\n")
  uu.no_stdout(r)
  
  }
}

# origin: uutils test_tail::test_no_trailing_newline
test test_uu_tail_no_trailing_newline { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", [], stdin: bytes.from_text("x"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("x"))
  
  }
}

# origin: uutils test_tail::test_null_default
test test_uu_tail_null_default { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-z", "foobar_with_null.txt"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "foobar_with_null_default.expected")?)
  
  }
}

# origin: uutils test_tail::test_num_with_undocumented_sign_bytes
test test_uu_tail_num_with_undocumented_sign_bytes { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-c", "5"], stdin: bytes.from_text("abcdefghijklmnopqrstuvwxyz"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("vwxyz"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "-5"], stdin: bytes.from_text("abcdefghijklmnopqrstuvwxyz"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("vwxyz"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "+5"], stdin: bytes.from_text("abcdefghijklmnopqrstuvwxyz"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("efghijklmnopqrstuvwxyz"))
  
  }
}

# origin: uutils test_tail::test_obsolete_syntax_positive_lines
test test_uu_tail_obsolete_syntax_positive_lines { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-3"], stdin: bytes.from_text("a\nb\nc\nd\ne\n"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("c\nd\ne\n"))
  
  }
}

# origin: uutils test_tail::test_obsolete_syntax_positive_zero_bytes
test test_uu_tail_obsolete_syntax_positive_zero_bytes { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["+0c"], stdin: bytes.from_text("a\nb\nc\nd\ne\n"), timeout: 15s)?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is_bytes(r, bytes.from_text("a\nb\nc\nd\ne\n"))
  
  }
}

# origin: uutils test_tail::test_obsolete_syntax_positive_zero_lines
test test_uu_tail_obsolete_syntax_positive_zero_lines { |ctx|
  let s = scene(ctx)?
  for arg in ["+0", "+00", "+0l"] {
  let r = uu.invoke(s, "tail", [arg], stdin: b"a\nb\nc\nd\ne\n", timeout: 15s)?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "a\nb\nc\nd\ne\n")
  }
}

# origin: uutils test_tail::test_obsolete_syntax_small_file
test test_uu_tail_obsolete_syntax_small_file { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-10"], stdin: bytes.from_text("a\nb\nc\nd\ne\n"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("a\nb\nc\nd\ne\n"))
  
  }
}

# origin: uutils test_tail::test_obsolete_syntax_zero_lines_file
test test_uu_tail_obsolete_syntax_zero_lines_file { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-0", "foobar.txt"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
}

# origin: uutils test_tail::test_oversized_num
test test_uu_tail_oversized_num { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-c", "99999999999999999999999999999"], stdin: bytes.from_text("abcd"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is(r, "abcd")
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "99999999999999999999999999999"], stdin: bytes.from_text("a\nb\n"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is(r, "a\nb\n")
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "+99999999999999999999999999999"], stdin: bytes.from_text("abcd"), timeout: 15s)?
  uu.succeeds(r)
  uu.no_stdout(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "+99999999999999999999999999999"], stdin: bytes.from_text("a\nb\n"), timeout: 15s)?
  uu.succeeds(r)
  uu.no_stdout(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["+99999999999999999999999999999c"], stdin: bytes.from_text("abcd"), timeout: 15s)?
  uu.succeeds(r)
  uu.no_stdout(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-99999999999999999999999999999c"], stdin: bytes.from_text("abcd"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is(r, "abcd")
  
  }
}

# origin: uutils test_tail::test_permission_denied
test test_uu_tail_permission_denied { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "unreadable")
  uu.set_mode(s, "unreadable", 0o000)
  let r = uu.invoke(s, "tail", ["unreadable"], stdin: b"", timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tail: cannot open 'unreadable' for reading: Permission denied\n")
}

# origin: uutils test_tail::test_permission_denied_is_not_reported_as_not_found
test test_uu_tail_permission_denied_is_not_reported_as_not_found { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "noexec")
  uu.touch(s, "noexec/file")
  uu.set_mode(s, "noexec", 0o000)
  defer uu.set_mode(s, "noexec", 0o700)
  let r = uu.invoke(s, "tail", ["noexec/file"], stdin: b"", timeout: 15s)?
  uu.fails(r)
  uu.stderr_contains(r, "Permission denied")
  uu.set_mode(s, "noexec", 0o700)
}

# origin: uutils test_tail::test_permission_denied_multiple
test test_uu_tail_permission_denied_multiple { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "file1")
  uu.touch(s, "file2")
  uu.touch(s, "unreadable")
  uu.set_mode(s, "unreadable", 0o000)
  let r = uu.invoke(s, "tail", ["file1", "unreadable", "file2"], stdin: b"", timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "==> file1 <==\n\n==> file2 <==\n")
  uu.stderr_is(r, "tail: cannot open 'unreadable' for reading: Permission denied\n")
}

# origin: uutils test_tail::test_pipe_when_bytes_option_given_input_size_has_multiple_size_of_buffer_size
test test_uu_tail_pipe_when_bytes_option_given_input_size_has_multiple_size_of_buffer_size { |ctx|
  let s = scene(ctx)?
  let data = sample_bytes(8192 * 3)
  {
  let r = uu.invoke(s, "tail", ["-c", "+0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "-0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
  {
  let wanted = data[8192..]
  let r = uu.invoke(s, "tail", ["-c", "+8193"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let wanted = data[8193..]
  let r = uu.invoke(s, "tail", ["-c", "+8194"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let wanted = data[16384..]
  let r = uu.invoke(s, "tail", ["-c", "+16385"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let wanted = data[16385..]
  let r = uu.invoke(s, "tail", ["-c", "+16386"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let wanted = data[16384..]
  let r = uu.invoke(s, "tail", ["-c", "-8192"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let wanted = data[16383..]
  let r = uu.invoke(s, "tail", ["-c", "-8193"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let wanted = data[8192..]
  let r = uu.invoke(s, "tail", ["-c", "-16384"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let wanted = data[8191..]
  let r = uu.invoke(s, "tail", ["-c", "-16385"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "-24576"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
}

# origin: uutils test_tail::test_pipe_when_bytes_option_given_input_size_is_equal_to_buffer_size
test test_uu_tail_pipe_when_bytes_option_given_input_size_is_equal_to_buffer_size { |ctx|
  let s = scene(ctx)?
  let data = sample_bytes(8192)
  {
  let r = uu.invoke(s, "tail", ["-c", "+0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let wanted = data[1..]
  let r = uu.invoke(s, "tail", ["-c", "+2"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "-0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
  {
  let wanted = data[1..]
  let r = uu.invoke(s, "tail", ["-c", "-8191"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "-8192"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "-8193"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let wanted = data[8192 - 1..]
  let r = uu.invoke(s, "tail", ["-c", "-1"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
}

# origin: uutils test_tail::test_pipe_when_bytes_option_given_input_size_is_one_byte_greater_than_buffer_size
test test_uu_tail_pipe_when_bytes_option_given_input_size_is_one_byte_greater_than_buffer_size { |ctx|
  let s = scene(ctx)?
  let data = sample_bytes(8192 + 1)
  {
  let r = uu.invoke(s, "tail", ["-c", "+0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let wanted = data[1..]
  let r = uu.invoke(s, "tail", ["-c", "+2"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "-0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
  {
  let wanted = data[8192..]
  let r = uu.invoke(s, "tail", ["-c", "-1"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let wanted = data[1..]
  let r = uu.invoke(s, "tail", ["-c", "-8192"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "-8193"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
}

# origin: uutils test_tail::test_pipe_when_bytes_option_given_multibyte_utf8_characters
test test_uu_tail_pipe_when_bytes_option_given_multibyte_utf8_characters { |ctx|
  let s = scene(ctx)?
  let data = bytes.from_text("𝅘𝅥𝅮⏻ƒa")
  {
  let r = uu.invoke(s, "tail", ["-c", "+0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "+2"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data[1..])
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "+5"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("⏻ƒa"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "+8"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("ƒa"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "+10"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("a"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "+11"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "-1"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("a"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "-2"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("ƒa")[1..])
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "-3"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("ƒa"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "-6"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("⏻ƒa"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "-10"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
}

# origin: uutils test_tail::test_pipe_when_bytes_option_value_is_higher_than_contained_bytes
test test_uu_tail_pipe_when_bytes_option_value_is_higher_than_contained_bytes { |ctx|
  let s = scene(ctx)?
  let data = bytes.from_text("a\nb")
  {
  let r = uu.invoke(s, "tail", ["-c", "4"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "5"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "999"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "+4"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "+5"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "+999"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
}

# origin: uutils test_tail::test_pipe_when_lines_option_given_input_size_has_multiple_size_of_buffer_size
test test_uu_tail_pipe_when_lines_option_given_input_size_has_multiple_size_of_buffer_size { |ctx|
  let s = scene(ctx)?
  let delimiter_count = 100
  let data = sample_lines(8192 * 3 + 1, delimiter_count, true)
  {
  let r = uu.invoke(s, "tail", ["-n", "+0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let wanted = skip_lines(data, 1)?
  let r = uu.invoke(s, "tail", ["-n", "+2"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "-0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
  {
  let wanted = skip_lines(data, delimiter_count - 1)?
  let r = uu.invoke(s, "tail", ["-n", "-1"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let wanted = skip_lines(data, 1)?
  let r = uu.invoke(s, "tail", ["-n", "-99"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "-100"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
}

# origin: uutils test_tail::test_pipe_when_lines_option_given_input_size_is_equal_to_buffer_size
test test_uu_tail_pipe_when_lines_option_given_input_size_is_equal_to_buffer_size { |ctx|
  let s = scene(ctx)?
  let delimiter_count = 100
  let data = sample_lines(8192, delimiter_count, true)
  {
  let r = uu.invoke(s, "tail", ["-n", "+0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let wanted = skip_lines(data, 1)?
  let r = uu.invoke(s, "tail", ["-n", "+2"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "-0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
  {
  let wanted = skip_lines(data, delimiter_count - 1)?
  let r = uu.invoke(s, "tail", ["-n", "-1"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let wanted = skip_lines(data, 1)?
  let r = uu.invoke(s, "tail", ["-n", "-99"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "-100"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
}

# origin: uutils test_tail::test_pipe_when_lines_option_given_input_size_is_equal_to_buffer_size_no_newline_at_eof
test test_uu_tail_pipe_when_lines_option_given_input_size_is_equal_to_buffer_size_no_newline_at_eof { |ctx|
  let s = scene(ctx)?
  let delimiter_count = 1
  let data = sample_lines(8192, delimiter_count, false)
  {
  let wanted = skip_lines(data, 1)?
  let r = uu.invoke(s, "tail", ["-n", "+2"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let wanted = skip_lines(data, 1)?
  let r = uu.invoke(s, "tail", ["-n", "-1"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
}

# origin: uutils test_tail::test_pipe_when_lines_option_given_input_size_is_one_byte_greater_than_buffer_size
test test_uu_tail_pipe_when_lines_option_given_input_size_is_one_byte_greater_than_buffer_size { |ctx|
  let s = scene(ctx)?
  let delimiter_count = 100
  let data = sample_lines(8192 + 1, delimiter_count, true)
  {
  let r = uu.invoke(s, "tail", ["-n", "+0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let wanted = skip_lines(data, delimiter_count - 1)?
  let r = uu.invoke(s, "tail", ["-n", "-1"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let wanted = skip_lines(data, 1)?
  let r = uu.invoke(s, "tail", ["-n", "+2"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
  {
  let wanted = skip_lines(data, 1)?
  let r = uu.invoke(s, "tail", ["-n", "-99"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, wanted)
  
  }
}

# origin: uutils test_tail::test_pipe_when_lines_option_given_multibyte_utf8_characters
test test_uu_tail_pipe_when_lines_option_given_multibyte_utf8_characters { |ctx|
  let s = scene(ctx)?
  let data = bytes.from_text("𝅘𝅥𝅮\n⏻\nƒ\na")
  {
  let r = uu.invoke(s, "tail", ["-n", "+0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "+2"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("⏻\nƒ\na"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "+3"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("ƒ\na"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "+4"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("a"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "+5"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "-4"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "-3"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("⏻\nƒ\na"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "-2"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("ƒ\na"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "-1"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("a"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "-0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
}

# origin: uutils test_tail::test_pipe_when_lines_option_value_is_higher_than_contained_lines
test test_uu_tail_pipe_when_lines_option_value_is_higher_than_contained_lines { |ctx|
  let s = scene(ctx)?
  let data = bytes.from_text("a\nb\n")
  {
  let r = uu.invoke(s, "tail", ["-n", "3"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "4"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "999"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "+3"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "+4"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "+999"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
}

# origin: uutils test_tail::test_pipe_when_negative_lines_option_given_no_newline_at_eof
test test_uu_tail_pipe_when_negative_lines_option_given_no_newline_at_eof { |ctx|
  let s = scene(ctx)?
  let data = bytes.from_text("a\nb")
  {
  let r = uu.invoke(s, "tail", ["-n", "0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "1"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("b"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "2"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("a\nb"))
  
  }
}

# origin: uutils test_tail::test_pipe_when_positive_lines_option_given_no_newline_at_eof
test test_uu_tail_pipe_when_positive_lines_option_given_no_newline_at_eof { |ctx|
  let s = scene(ctx)?
  let data = bytes.from_text("a\nb")
  {
  let r = uu.invoke(s, "tail", ["-n", "+0"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("a\nb"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "+1"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("a\nb"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "+2"], stdin: data, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("b"))
  
  }
}

# origin: uutils test_tail::test_positive_bytes
test test_uu_tail_positive_bytes { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-c", "+3"], stdin: bytes.from_text("abcde"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("cde"))
  
  }
}

# origin: uutils test_tail::test_positive_bytes_file
test test_uu_tail_positive_bytes_file { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-c", "+42", "foobar.txt"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("ho\nnueve\ndiez\nonce\n"))
  
  }
}

# origin: uutils test_tail::test_positive_bytes_file_offset_past_seek_limit
test test_uu_tail_positive_bytes_file_offset_past_seek_limit { |ctx|
  let s = scene(ctx)?
  uu.write(s, "big", repeated("a", 8192))
  let r = uu.invoke(s, "tail", ["-c", "+18446744073709551615", "big"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
  uu.no_stdout(r)
}

# origin: uutils test_tail::test_positive_lines
test test_uu_tail_positive_lines { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-n", "+3"], stdin: bytes.from_text("a\nb\nc\nd\ne\n"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("c\nd\ne\n"))
  
  }
}

# origin: uutils test_tail::test_positive_lines_file
test test_uu_tail_positive_lines_file { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-n", "+7", "foobar.txt"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("siette\nocho\nnueve\ndiez\nonce\n"))
  
  }
}

# origin: uutils test_tail::test_positive_zero_bytes
test test_uu_tail_positive_zero_bytes { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-c", "+0"], stdin: bytes.from_text("abcde"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("abcde"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c", "0"], stdin: bytes.from_text("abcde"), timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
}

# origin: uutils test_tail::test_positive_zero_lines
test test_uu_tail_positive_zero_lines { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-n", "+0"], stdin: bytes.from_text("a\nb\nc\nd\ne\n"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("a\nb\nc\nd\ne\n"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-n", "0"], stdin: bytes.from_text("a\nb\nc\nd\ne\n"), timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
}

# origin: uutils test_tail::test_presume_input_pipe_default
test test_uu_tail_presume_input_pipe_default { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["---presume-input-pipe"], stdin: fixture(s, "foobar.txt")?, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "foobar_stdin_default.expected")?)
  uu.no_stderr(r)
  
  }
}

# origin: uutils test_tail::test_retry8
test test_uu_tail_retry8 { |ctx|
  let s = scene(ctx)?
  let p = start(s, ["-F", "-s.1", "--max-unchanged-stats=1", "parent_dir/watched_file"])?
  defer cleanup(p)
  time.sleep(1000ms)
  alive(p)
  checkpoint(p, p.err, "cannot open")
  uu.mkdir(s, "parent_dir")
  append_bytes(uu.at(s, "parent_dir/watched_file"), b"foo\n")
  time.sleep(1000ms)
  checkpoint(p, p.out, "foo\n")
  uu.at(s, "parent_dir/watched_file").remove()
  uu.at(s, "parent_dir").remove()
  time.sleep(1000ms)
  checkpoint(p, p.err, "has become inaccessible")
  uu.mkdir(s, "parent_dir")
  append_bytes(uu.at(s, "parent_dir/watched_file"), b"bar\n")
  time.sleep(1000ms)
  checkpoint(p, p.out, "bar\n")
  alive(p)
  let r = finish(p)?
  uu.stdout_is(r, "foo\nbar\n")
  uu.stderr_is(r, "tail: cannot open 'parent_dir/watched_file' for reading: No such file or directory\ntail: 'parent_dir/watched_file' has appeared;  following new file\ntail: 'parent_dir/watched_file' has become inaccessible: No such file or directory\ntail: 'parent_dir/watched_file' has appeared;  following new file\n")
}

# origin: uutils test_tail::test_retry9
test test_uu_tail_retry9 { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "parent_dir")
  uu.write(s, "parent_dir/watched_file", "foo\n")
  let p = start(s, ["-F", "-s.2", "--max-unchanged-stats=1", "parent_dir/watched_file"])?
  defer cleanup(p)
  time.sleep(400ms)
  alive(p)
  checkpoint(p, p.out, "foo\n")
  var expected = "foo\n"
  for text in ["bar\n", "foo\n", "bar\n"] {
    uu.at(s, "parent_dir/watched_file").remove()
    uu.at(s, "parent_dir").remove()
    time.sleep(400ms)
    checkpoint(p, p.err, "has become inaccessible")
    uu.mkdir(s, "parent_dir")
    uu.write(s, "parent_dir/watched_file", text)
    time.sleep(400ms)
    expected += text
    checkpoint(p, p.out, expected)
  }
  alive(p)
  let r = finish(p)?
  uu.stdout_is(r, "foo\nbar\nfoo\nbar\n")
  uu.stderr_is(r, "tail: 'parent_dir/watched_file' has become inaccessible: No such file or directory\ntail: directory containing watched file was removed\ntail: inotify cannot be used, reverting to polling\ntail: 'parent_dir/watched_file' has appeared;  following new file\ntail: 'parent_dir/watched_file' has become inaccessible: No such file or directory\ntail: 'parent_dir/watched_file' has appeared;  following new file\ntail: 'parent_dir/watched_file' has become inaccessible: No such file or directory\ntail: 'parent_dir/watched_file' has appeared;  following new file\n")
}

# origin: uutils test_tail::test_retry_descriptor_detects_truncation
test test_uu_tail_retry_descriptor_detects_truncation { |ctx|
  let s = scene(ctx)?
  for round in range(2) {
    let args = ["-s.1", "--max-unchanged-stats=1", "--follow=descriptor", "--retry", "watchme"].extend(if round == 0 { ["---disable-inotify"] } else { [] })
    let delay = if round == 0 { 150ms } else { 50ms }
    let p = start(s, args)?
    defer cleanup(p)
    time.sleep(delay)
    alive(p)
    checkpoint(p, p.err, "cannot open")
    uu.touch(s, "watchme")
    time.sleep(delay)
    checkpoint(p, p.err, "has appeared")
    uu.write(s, "watchme", "greetings\n")
    time.sleep(delay)
    checkpoint(p, p.out, "greetings\n")
    uu.write(s, "watchme", "hi\n")
    time.sleep(delay)
    checkpoint(p, p.out, "hi\n")
    alive(p)
    let r = finish(p)?
    uu.stderr_is(r, "tail: warning: --retry only effective for the initial open\ntail: cannot open 'watchme' for reading: No such file or directory\ntail: 'watchme' has appeared;  following new file\ntail: watchme: file truncated\n")
    uu.stdout_is(r, "greetings\nhi\n")
    uu.at(s, "watchme").remove()
  }
}

# origin: uutils test_tail::test_retry_missing_file_error
test test_uu_tail_retry_missing_file_error { |ctx|
  let s = scene(ctx)?
  let r = uu.invoke(s, "tail", ["absent", "--retry"], stdin: b"", timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "tail: warning: --retry ignored; --retry is useful only when following\ntail: cannot open 'absent' for reading: No such file or directory\n")
}

# origin: uutils test_tail::test_retry_warn_without_follow
test test_uu_tail_retry_warn_without_follow { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "FILE")
  let r = uu.invoke(s, "tail", ["FILE", "--retry"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
  uu.stderr_is(r, "tail: warning: --retry ignored; --retry is useful only when following\n")
  uu.fails_with_code(r, 0)
}

# origin: uutils test_tail::test_seek_bytes_backward_outside_file
test test_uu_tail_seek_bytes_backward_outside_file { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-c", "100", "foobar.txt"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "foobar.txt")?)
  
  }
}

# origin: uutils test_tail::test_seek_bytes_forward_outside_file
test test_uu_tail_seek_bytes_forward_outside_file { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-c", "+100", "foobar.txt"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
}

# origin: uutils test_tail::test_single_big_args
test test_uu_tail_single_big_args { |ctx|
  let s = scene(ctx)?
  let file = uu.at(s, "single_big_args.txt")
  let expected = uu.at(s, "single_big_args_expected.txt")
  file.write("")
  expected.write("")
  for chunk in range(100) {
    append_bytes(file, bytes.from_text([f"Line {index}\n" for index in range(chunk * 10000, (chunk + 1) * 10000)].join("")))
  }
  for chunk in range(90, 100) {
    append_bytes(expected, bytes.from_text([f"Line {index}\n" for index in range(chunk * 10000, (chunk + 1) * 10000)].join("")))
  }
  let r = uu.invoke(s, "tail", ["single_big_args.txt", "-n", "100000"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
}

# origin: uutils test_tail::test_single_default
test test_uu_tail_single_default { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["foobar.txt"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "foobar_single_default.expected")?)
  
  }
}

# origin: uutils test_tail::test_sleep_interval
test test_uu_tail_sleep_interval { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-s", "10", "foobar.txt"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-s", ".1", "foobar.txt"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-s.1", "foobar.txt"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-s", "-1", "foobar.txt"], stdin: b"", timeout: 15s)?
  uu.fails(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-s", "1..1", "foobar.txt"], stdin: b"", timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "invalid number of seconds: '1..1'")
  
  }
}

# origin: uutils test_tail::test_small_file
test test_uu_tail_small_file { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-n -10"], stdin: bytes.from_text("a\nb\nc\nd\ne\n"), timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tail: invalid number of lines: ' -10': Value too large for defined data type\n")
  
  }
}

# origin: uutils test_tail::test_stdin_default
test test_uu_tail_stdin_default { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", [], stdin: fixture(s, "foobar.txt")?, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "foobar_stdin_default.expected")?)
  uu.no_stderr(r)
  
  }
}

# origin: uutils test_tail::test_stdin_explicit
test test_uu_tail_stdin_explicit { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-"], stdin: fixture(s, "foobar.txt")?, timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture(s, "foobar_stdin_default.expected")?)
  uu.no_stderr(r)
  
  }
}

# origin: uutils test_tail::test_stdin_redirect_dir
test test_uu_tail_stdin_redirect_dir { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "dir")
  for args in [[], ["-"]] {
    let r = uu.invoke_from_path(s, "tail", args, uu.at(s, "dir"), timeout: 5s)?
    uu.fails_with_code(r, 1)
    uu.stderr_only(r, "tail: error reading 'standard input': Is a directory\n")
  }
}

# origin: uutils test_tail::test_stdin_redirect_file
test test_uu_tail_stdin_redirect_file { |ctx|
  let s = scene(ctx)?
  uu.write(s, "f", "foo")
  {
    let r = uu.invoke_from_path(s, "tail", [], uu.at(s, "f"), timeout: 5s)?
    uu.succeeds(r)
    uu.stdout_is(r, "foo")
  }
  let r = uu.invoke_from_path(s, "tail", ["-v"], uu.at(s, "f"), timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only(r, "==> 'standard input' <==\nfoo")
}

# origin: uutils test_tail::test_stdin_redirect_offset
test test_uu_tail_stdin_redirect_offset { |ctx|
  let s = scene(ctx)?
  uu.write(s, "k", "1\n2\n")
  let r = seek_stdin(s, [])?
  uu.succeeds(r)
  uu.stdout_only(r, "2\n")
}

# origin: uutils test_tail::test_stdin_redirect_offset2
test test_uu_tail_stdin_redirect_offset2 { |ctx|
  let s = scene(ctx)?
  uu.write(s, "k", "1\n2\n")
  uu.write(s, "l", "3\n4\n")
  uu.write(s, "m", "5\n6\n")
  let r = seek_stdin(s, ["k", "-", "l", "m"])?
  uu.succeeds(r)
  uu.stdout_only(r, "==> k <==\n1\n2\n\n==> 'standard input' <==\n2\n\n==> l <==\n3\n4\n\n==> m <==\n5\n6\n")
}

# origin: uutils test_tail::test_stdin_via_script_redirection_and_pipe
test test_uu_tail_stdin_via_script_redirection_and_pipe { |ctx|
  let s = scene(ctx)?
  uu.write(s, "file.txt", "line1\nline2\n")
  uu.write(s, "test.sh", "#!/usr/bin/env sh\ntail\n")
  uu.set_mode(s, "test.sh", 0o755)
  let bin = test.temp_dir(ctx, name: "tail-bin")?
  let shim = fp"{bin}/tail"
  let vars = {PATH: f"{bin}:/usr/local/bin:/usr/bin:/bin"}
  let argv = uu.argv(s, "tail", [], vars)?
  shim.write("#!/bin/sh\nexec " + [shell_quote(word) for word in argv].join(" ") + "\n", mode: 0o755)
  for script in ["./test.sh < file.txt", "cat file.txt | ./test.sh"] {
    let r = wrapped(s, [], [p"/bin/sh", p"-c", Path(script)], vars)?
    uu.succeeds(r)
    uu.stdout_only(r, "line1\nline2\n")
  }
}

# origin: uutils test_tail::test_tail_bytes_exceeds_file_size
test test_uu_tail_tail_bytes_exceeds_file_size { |ctx|
  let s = scene(ctx)?
  uu.write(s, "test_file.txt", repeated("x", 5000))
  let r = uu.invoke(s, "tail", ["-c", "1048576", "test_file.txt"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only(r, repeated("x", 5000))
}

# origin: uutils test_tail::test_tail_obsolete_blocks
test test_uu_tail_tail_obsolete_blocks { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-b"], stdin: bytes.from_text(repeated("x\n", 2561)), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text(repeated("x\n", 2560)))
  
  }
}

# origin: uutils test_tail::test_tail_obsolete_bytes
test test_uu_tail_tail_obsolete_bytes { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-1c"], stdin: bytes.from_text("wxyz"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("z"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-9c"], stdin: bytes.from_text("wxyz"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("wxyz"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-12c"], stdin: bytes.from_text("x" + repeated("y", 12) + "z"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text(repeated("y", 11) + "z"))
  
  }
}

# origin: uutils test_tail::test_tail_obsolete_error_cases
test test_uu_tail_tail_obsolete_error_cases { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["+cl"], stdin: b"", timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_is(r, "tail: cannot open '+cl' for reading: No such file or directory\n")
  
  }
  {
  let r = uu.invoke(s, "tail", ["-cl"], stdin: b"", timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_is(r, "tail: invalid number of bytes: 'l'\n")
  
  }
  {
  let r = uu.invoke(s, "tail", ["+2cz"], stdin: b"", timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_is(r, "tail: cannot open '+2cz' for reading: No such file or directory\n")
  
  }
  {
  let r = uu.invoke(s, "tail", ["-2cX"], stdin: b"", timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_is(r, "tail: option used in invalid context -- 2\n")
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c99999999999999999999"], stdin: bytes.from_text("x"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("x"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c --"], stdin: b"", timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_is(r, "tail: invalid number of bytes: ' --'\n")
  
  }
  {
  let r = uu.invoke(s, "tail", ["-5cz"], stdin: b"", timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_is(r, "tail: option used in invalid context -- 5\n")
  
  }
  {
  let r = uu.invoke(s, "tail", ["-9999999999999999999b"], stdin: bytes.from_text("x"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("x"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-999999999999999999999b"], stdin: bytes.from_text("x"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("x"))
  
  }
}

# origin: uutils test_tail::test_tail_obsolete_f_flag
test test_uu_tail_tail_obsolete_f_flag { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "file")
  {
    let p = start(s, ["+f", "file"])?
    defer cleanup(p)
    time.sleep(500ms)
    alive(p)
    uu.no_output(finish(p)?)
  }
  let p = start(s, ["+f"], held_stdin: true)?
  defer cleanup(p)
  time.sleep(500ms)
  alive(p)
  uu.no_output(finish(p)?)
}

# origin: uutils test_tail::test_tail_obsolete_lines
test test_uu_tail_tail_obsolete_lines { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-1l"], stdin: bytes.from_text("x"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("x"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-1l"], stdin: bytes.from_text("x\ny\n"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("y\n"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-1l"], stdin: bytes.from_text("x\ny"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("y"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-l"], stdin: bytes.from_text("x" + repeated("y\n", 10) + "z"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text(repeated("y\n", 9) + "z"))
  
  }
}

# origin: uutils test_tail::test_tail_obsolete_number
test test_uu_tail_tail_obsolete_number { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-1"], stdin: bytes.from_text("x"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("x"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-1"], stdin: bytes.from_text("x\ny\n"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("y\n"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["-1"], stdin: bytes.from_text("x\ny"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("y"))
  
  }
}

# origin: uutils test_tail::test_tail_obsolete_plus_bytes
test test_uu_tail_tail_obsolete_plus_bytes { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["+2c"], stdin: bytes.from_text("wxyz"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("xyz"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["+8c"], stdin: bytes.from_text("wxyz"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text(""))
  
  }
  {
  let r = uu.invoke(s, "tail", ["+c"], stdin: bytes.from_text("x" + repeated("y", 10) + "z"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("yyz"))
  
  }
}

# origin: uutils test_tail::test_tail_obsolete_plus_lines
test test_uu_tail_tail_obsolete_plus_lines { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["+1l"], stdin: bytes.from_text("x\ny\n"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("x\ny\n"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["+2l"], stdin: bytes.from_text("x\ny\n"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("y\n"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["+l"], stdin: bytes.from_text("x\n" + repeated("y\n", 10) + "z"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("y\ny\nz"))
  
  }
}

# origin: uutils test_tail::test_tail_obsolete_plus_number
test test_uu_tail_tail_obsolete_plus_number { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["+1"], stdin: bytes.from_text("x\ny\n"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("x\ny\n"))
  
  }
  {
  let r = uu.invoke(s, "tail", ["+2"], stdin: bytes.from_text("x\ny\n"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, bytes.from_text("y\n"))
  
  }
}

# origin: uutils test_tail::test_when_argument_file_is_a_directory
test test_uu_tail_when_argument_file_is_a_directory { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "dir")
  let r = uu.invoke(s, "tail", ["dir"], stdin: b"", timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tail: error reading 'dir': Is a directory\n")
}

# origin: uutils test_tail::test_when_argument_file_is_a_symlink
test test_uu_tail_when_argument_file_is_a_symlink { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "target")
  uu.symlink(s, "target", "link")
  {
  let r = uu.invoke(s, "tail", ["-c", "+0", "link"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
  let data = sample_bytes(100)
  append_bytes(uu.at(s, "target"), data)
  let r = uu.invoke(s, "tail", ["-c", "+0", "link"], stdin: b"", timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, data)
}

# origin: uutils test_tail::test_when_argument_file_is_a_symlink_to_directory_then_error
test test_uu_tail_when_argument_file_is_a_symlink_to_directory_then_error { |ctx|
  let s = scene(ctx)?
  uu.mkdir(s, "dir")
  uu.symlink(s, "dir", "dir_link")
  let r = uu.invoke(s, "tail", ["dir_link"], stdin: b"", timeout: 15s)?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tail: error reading 'dir_link': Is a directory\n")
}

# origin: uutils test_tail::test_when_output_closed_then_no_broken_pipe
test test_uu_tail_when_output_closed_then_no_broken_pipe { |ctx|
  let s = scene(ctx)?
  uu.touch(s, "input")
  uu.truncate(s, "input", 10000000)
  let state = test.temp_dir(ctx, name: "tail-closed")?
  let status_file = fp"{state}/status"
  let args = ["-c", "10000000", "input"]
  let argv = uu.argv(s, "tail", [Path(arg) for arg in args])?
  let shell = Path(r"""status=$1; shift; ( "$@"; rc=$?; printf '%s\n' "$rc" >"$status" ) | { exit 0; }""")
  let result = wrapped(s, args, [p"/bin/sh", p"-c", shell, p"sh", status_file].extend(argv))?
  let status = status_file.read_text()?.trim() as Int
  assert status != 0, "closed stdout must make tail fail"
  uu.no_stderr(result)
}

# origin: uutils test_tail::test_zero_bytes_with_suffix
test test_uu_tail_zero_bytes_with_suffix { |ctx|
  let s = scene(ctx)?
  {
  let r = uu.invoke(s, "tail", ["-c0K"], stdin: bytes.from_text("qwerty"), timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c00K"], stdin: bytes.from_text("qwerty"), timeout: 15s)?
  uu.succeeds(r)
  uu.no_output(r)
  
  }
  {
  let r = uu.invoke(s, "tail", ["-c+0K"], stdin: bytes.from_text("qwerty"), timeout: 15s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, bytes.from_text("qwerty"))
  
  }
}

