##! Transcribed behavior from the MIT-licensed uutils tail integration tests.
use support.uu as uu
type InputCase = {args: List[Str], input: Union[Bytes, Path], expected: Str}

proc fixture_bytes(ctx: TestContext, name: Str) [fs, error] -> Result[Bytes, Error] {
  Ok(fp"{ctx.core_dir}/tests/data/uutils/tail/{name}".read_bytes()?)
}

# Append through the held inode without a truncate event, as the upstream scene does.
proc append_bytes(s: uu.Scene, name: Str, data: Bytes) [fs, process, net, error] -> Result[Unit, Error] {
  if !uu.at(s, name).exists() { uu.touch(s, name) }
  let fd = unix.open_fd(uu.at(s, name), write: true, nonblock: false, flags: ["append"])?
  defer unix.close_fd(fd)
  var offset = 0
  while offset < data.len() {
    let count = unix.write_fd(fd, data[offset..])?
    assert count > 0
    offset += count
  }
  Ok()
}

proc append(s: uu.Scene, name: Str, data: Str) [fs, process, net, error] -> Result[Unit, Error] {
  append_bytes(s, name, bytes.from_text(data))
}

# Every follow run uses its own captures; callers own the handle and cancel it on scope exit.
proc follow(s: uu.Scene, args: List[Str], input: Union[Bytes, Path] = b"", merged: Bool = false) [fs, process, env, error] -> Result[Command, Error] {
  let argv = uu.argv(s, "tail", [Path(word) for word in args])?
  let out = uu.at(s, ".live-out")
  let err = uu.at(s, ".live-err")
  if merged {
    let words = [p"/bin/sh", p"-c", Path(r"""exec "$@" 2>&1"""), p"tail-merged"].extend(argv)
    if input is Path { Ok(process.command_argv(p"/bin/sh", words, s.root, {}, input, out, err)) } else { Ok(process.command_argv(p"/bin/sh", words, s.root, {}, input, out, err)) }
  } else {
    if input is Path { Ok(process.command_argv(s.ctx.xsh_bin, argv, s.root, {}, input, out, err)) } else { Ok(process.command_argv(s.ctx.xsh_bin, argv, s.root, {}, input, out, err)) }
  }
}

# Status belongs to the separately asserted process handle. This view carries only stream bytes.
proc streams(s: uu.Scene, out_offset: Int = 0, err_offset: Int = 0) [fs, error] -> Result[uu.Ran, Error] {
  let out = uu.at(s, ".live-out").read_bytes()?
  let err = uu.at(s, ".live-err").read_bytes()?
  Ok({util: "tail", args: [], status: 0, stdout: out[out_offset..], stderr: err[err_offset..]})
}

proc alive(child: ProcessHandle) [process, error] {
  assert process.wait_timeout([child], 0ms)? == null
}

proc wait_output(s: uu.Scene, out: Bytes, err: Bytes) [fs, process, time, error] {
  for i in range(500) {
    let current = streams(s)?
    if current.stdout == out and current.stderr == err { return }
    time.sleep(10ms)
  }
  let current = streams(s)?
  uu.stdout_is_bytes(current, out)
  uu.stderr_is_bytes(current, err)
}

proc random_input() [fs, process, net, error] -> Result[Str, Error] {
  let fd = unix.open_fd(p"/dev/urandom")?
  defer unix.close_fd(fd)
  let data = unix.read_fd(fd, 1000)?
  assert data.len() == 1000
  let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789\n"
  Ok([alphabet.byte_slice((data.byte_at(i) ?? 0) % 63, length: 1) for i in range(1000)].join(""))
}

# origin: uutils test_tail::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_tail_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-n", "5QQ", "/dev/null"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "tail: invalid number of lines: '5QQ'\n")
  }
}

# origin: uutils test_tail::tail_n_lines_with_emoji
test test_uu_tail_tail_n_lines_with_emoji { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-n", "1"], stdin: bytes.from_text("a\n💐\n"), timeout: 30s)?
  uu.succeeds(r)
  uu.stdout_only(r, "💐\n")
  }
}

# origin: uutils test_tail::test_accepted_multiplier_suffixes
test test_uu_tail_accepted_multiplier_suffixes { |ctx|
  let s = uu.scene(ctx)?
  for suffix in ["b", "k", "m", "K", "M", "G", "T", "P", "E", "Z", "Y", "R", "Q", "kB", "KiB", "kD", "MiB", "GB"] {
    let r = uu.invoke(s, "tail", ["-c", f"1{suffix}"], stdin: b"x")?
    uu.succeeds(r)
  }
}

# origin: uutils test_tail::test_args_sleep_interval_when_illegal_argument_then_usage_error::case_01_underscore_delimiter
test test_uu_tail_test_args_sleep_interval_when_illegal_argument_then_usage_error_case_01_underscore_delimiter { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["--sleep-interval", "1_000"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tail: invalid number of seconds: '1_000'\n")
}

# origin: uutils test_tail::test_args_sleep_interval_when_illegal_argument_then_usage_error::case_02_only_point
test test_uu_tail_test_args_sleep_interval_when_illegal_argument_then_usage_error_case_02_only_point { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["--sleep-interval", "."])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tail: invalid number of seconds: '.'\n")
}

# origin: uutils test_tail::test_args_sleep_interval_when_illegal_argument_then_usage_error::case_04_space
test test_uu_tail_test_args_sleep_interval_when_illegal_argument_then_usage_error_case_04_space { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["--sleep-interval", " "])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tail: invalid number of seconds: ' '\n")
}

# origin: uutils test_tail::test_args_sleep_interval_when_illegal_argument_then_usage_error::case_05_empty
test test_uu_tail_test_args_sleep_interval_when_illegal_argument_then_usage_error_case_05_empty { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["--sleep-interval", ""])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tail: invalid number of seconds: ''\n")
}

# origin: uutils test_tail::test_args_sleep_interval_when_illegal_argument_then_usage_error::case_06_comma_separator
test test_uu_tail_test_args_sleep_interval_when_illegal_argument_then_usage_error_case_06_comma_separator { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["--sleep-interval", "0,0"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tail: invalid number of seconds: '0,0'\n")
}

# origin: uutils test_tail::test_args_sleep_interval_when_illegal_argument_then_usage_error::case_07_words_nominator_fract
test test_uu_tail_test_args_sleep_interval_when_illegal_argument_then_usage_error_case_07_words_nominator_fract { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["--sleep-interval", "one.zero"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tail: invalid number of seconds: 'one.zero'\n")
}

# origin: uutils test_tail::test_args_sleep_interval_when_illegal_argument_then_usage_error::case_08_words_fract
test test_uu_tail_test_args_sleep_interval_when_illegal_argument_then_usage_error_case_08_words_fract { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["--sleep-interval", ".zero"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tail: invalid number of seconds: '.zero'\n")
}

# origin: uutils test_tail::test_args_sleep_interval_when_illegal_argument_then_usage_error::case_09_words_nominator
test test_uu_tail_test_args_sleep_interval_when_illegal_argument_then_usage_error_case_09_words_nominator { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["--sleep-interval", "one."])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tail: invalid number of seconds: 'one.'\n")
}

# origin: uutils test_tail::test_args_sleep_interval_when_illegal_argument_then_usage_error::case_10_two_points
test test_uu_tail_test_args_sleep_interval_when_illegal_argument_then_usage_error_case_10_two_points { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["--sleep-interval", "0..0"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tail: invalid number of seconds: '0..0'\n")
}

# origin: uutils test_tail::test_args_sleep_interval_when_illegal_argument_then_usage_error::case_11_seconds_unit
test test_uu_tail_test_args_sleep_interval_when_illegal_argument_then_usage_error_case_11_seconds_unit { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["--sleep-interval", "1.0s"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tail: invalid number of seconds: '1.0s'\n")
}

# origin: uutils test_tail::test_args_sleep_interval_when_illegal_argument_then_usage_error::case_12_circumflex_exponent
test test_uu_tail_test_args_sleep_interval_when_illegal_argument_then_usage_error_case_12_circumflex_exponent { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["--sleep-interval", "1.0e^1000"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "tail: invalid number of seconds: '1.0e^1000'\n")
}

# origin: uutils test_tail::test_args_when_directory_given_shorthand_big_f_together_with_retry
test test_uu_tail_args_when_directory_given_shorthand_big_f_together_with_retry { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")
  for args in [["-F", "--retry", "dir"], ["--retry", "-F", "dir"]] {
  let plan = follow(s, args)?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(500ms)
  alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
  uu.stderr_only(streams(s)?, "tail: error reading 'dir': Is a directory\ntail: dir: cannot follow end of this type of file\n")
  }
}

# origin: uutils test_tail::test_args_when_presume_input_pipe_given_input_is_file
test test_uu_tail_args_when_presume_input_pipe_given_input_is_file { |ctx|
  let s = uu.scene(ctx)?
  let input = random_input()?
  uu.write(s, "data", input)
  for option in ["-c", "-n"] {
    for count in ["-0", "+0"] {
      let args = ["---presume-input-pipe", option, count].extend(["data"])
      let r = uu.invoke(s, "tail", args, stdin: b"")?
      uu.succeeds(r)
      if count == "-0" { uu.no_output(r) } else { uu.stdout_only(r, input) }
    }
  }
}

# origin: uutils test_tail::test_args_when_presume_input_pipe_given_input_is_pipe
test test_uu_tail_args_when_presume_input_pipe_given_input_is_pipe { |ctx|
  let s = uu.scene(ctx)?
  let input = random_input()?
  for option in ["-c", "-n"] {
    for count in ["-0", "+0"] {
      let args = ["---presume-input-pipe", option, count]
      let r = uu.invoke(s, "tail", args, stdin: bytes.from_text(input))?
      uu.succeeds(r)
      if count == "-0" { uu.no_output(r) } else { uu.stdout_only(r, input) }
    }
  }
}

# origin: uutils test_tail::test_args_when_settings_check_warnings_follow_indefinitely_then_no_warning
test test_uu_tail_args_when_settings_check_warnings_follow_indefinitely_then_no_warning { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "data", "file data\n")
  uu.write(s, "fifo", "fifo data\n")
  let rows: List[InputCase] = [
    {args: ["--follow=descriptor", "-", "data"], input: b"pipe data", expected: "==> 'standard input' <==\npipe data\n==> data <==\nfile data\n"},
    {args: ["--follow=descriptor", "-", "data"], input: uu.at(s, "fifo"), expected: "==> 'standard input' <==\nfifo data\n\n==> data <==\nfile data\n"},
    {args: ["--follow=descriptor", "--pid=0", "-", "data"], input: uu.at(s, "fifo"), expected: "==> 'standard input' <==\nfifo data\n\n==> data <==\nfile data\n"},
  ]
  for row in rows {
  let plan = follow(s, row.args, input: row.input, merged: true)?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
    time.sleep(500ms)
    alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
    time.sleep(500ms)
    uu.stdout_only(streams(s)?, row.expected)
  }
}

# origin: uutils test_tail::test_args_when_settings_check_warnings_follow_indefinitely_then_warning
test test_uu_tail_args_when_settings_check_warnings_follow_indefinitely_then_warning { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "data", "file data\n")
  for row in [
    {args: ["--follow=descriptor", "-", "data"], merged: false, expected: ""},
    {args: ["--follow=descriptor", "-", "data"], merged: true, expected: "tail: warning: following standard input indefinitely is ineffective\n"},
    {args: ["--follow=descriptor", "data", "-"], merged: true, expected: "tail: warning: following standard input indefinitely is ineffective\n"},
    {args: ["--follow=descriptor", "-", "-"], merged: true, expected: "tail: warning: following standard input indefinitely is ineffective\n"},
    {args: ["--follow=descriptor", "-", "-", "data"], merged: true, expected: "tail: warning: following standard input indefinitely is ineffective\n"},
    {args: ["--follow=descriptor", "--pid=100000", "-", "data"], merged: true, expected: "tail: warning: following standard input indefinitely is ineffective\n"},
  ] {
  let plan = follow(s, row.args, input: p"/dev/ptmx", merged: row.merged)?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(500ms)
  alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
  let r = streams(s)?
  if row.merged { uu.stdout_only(r, row.expected) } else {
    uu.stdout_is(r, row.expected)
    uu.stderr_is(r, "tail: warning: following standard input indefinitely is ineffective\n")
  }
  }
}

# origin: uutils test_tail::test_args_when_settings_check_warnings_follow_retry
test test_uu_tail_args_when_settings_check_warnings_follow_retry { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "data", "file data\n")
  let plan = follow(s, ["--follow=descriptor", "--retry", "data"], merged: true)?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(500ms)
  child.cancel(signal: "KILL", kill_after: 0ms)
  uu.stdout_only(streams(s)?, "tail: warning: --retry only effective for the initial open\nfile data\n")
}

# origin: uutils test_tail::test_args_when_settings_check_warnings_then_shows_warnings
test test_uu_tail_args_when_settings_check_warnings_then_shows_warnings { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "data", "file data\n")
  for row in [
    {args: ["--retry", "data"], expected: "tail: warning: --retry ignored; --retry is useful only when following\nfile data\n"},
    {args: ["--pid=0", "data"], expected: "tail: warning: PID ignored; --pid=PID is useful only when following\nfile data\n"},
    {args: ["--pid=1000", "--retry", "data"], expected: "tail: warning: --retry ignored; --retry is useful only when following\ntail: warning: PID ignored; --pid=PID is useful only when following\nfile data\n"},
    {args: ["--pid=1000", "--pid=1000", "--retry", "data"], expected: "tail: warning: --retry ignored; --retry is useful only when following\ntail: warning: PID ignored; --pid=PID is useful only when following\nfile data\n"},
  ] {
  let status = process.run(follow(s, row.args, input: b"", merged: true)?)?.exit_code()?
  let r = streams(s)?
    assert status == 0
    uu.stdout_only(r, row.expected)
  }
}

# origin: uutils test_tail::test_bytes_big
test test_uu_tail_bytes_big { |ctx|
  let s = uu.scene(ctx)?
  let content = [f"{i % 10}" for i in range(1000000)].join("")
  let expected = [f"{i % 10}" for i in range(900000, 1000000)].join("")
  uu.write(s, "test_bytes_big.txt", content)
  uu.write(s, "test_bytes_big_expected.txt", expected)
  let r = uu.invoke(s, "tail", ["test_bytes_big.txt", "-c", "100000"])?
  uu.succeeds(r)
  assert r.stdout.len() == expected.byte_len()
  uu.stdout_is(r, expected)
}

# origin: uutils test_tail::test_bytes_for_funny_unix_files
test test_uu_tail_bytes_for_funny_unix_files { |ctx|
  let s = uu.scene(ctx)?
  for file in [p"/proc/version", p"/sys/kernel/profiling"] {
    if !file.is_file() { continue }
    let data = file.read_bytes()?
    let expected = if data.is_empty() { b"" } else { data[data.len() - 1..] }
    let r = uu.invoke(s, "tail", ["--bytes", "1", file.display()])?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, expected)
    uu.stderr_is(r, "")
  }
}

# origin: uutils test_tail::test_bytes_single
test test_uu_tail_bytes_single { |ctx|
  {
  let s = uu.scene(ctx)?
  uu.fixture(s, "tail", "foobar.txt", "foobar.txt")
  let r = uu.invoke(s, "tail", ["-c", "10", "foobar.txt"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture_bytes(ctx, "foobar_bytes_single.expected")?)
  }
}

# origin: uutils test_tail::test_bytes_stdin
test test_uu_tail_bytes_stdin { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-c", "13"], stdin: fixture_bytes(ctx, "foobar.txt")?, timeout: 30s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture_bytes(ctx, "foobar_bytes_stdin.expected")?)
  uu.no_stderr(r)
  }
}

# origin: uutils test_tail::test_child_when_run_with_stderr_to_stdout
test test_uu_tail_child_when_run_with_stderr_to_stdout { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "data", "file data\n")
  let status = process.run(follow(s, ["data", "missing"], input: b"", merged: true)?)?.exit_code()?
  let r = streams(s)?
  assert status != 0
  uu.stdout_only(r, "==> data <==\nfile data\ntail: cannot open 'missing' for reading: No such file or directory\n")
}

# origin: uutils test_tail::test_descriptor_no_retry_skips_late_file
test test_uu_tail_descriptor_no_retry_skips_late_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "active")
  let plan = follow(s, ["--follow=descriptor", "nofile", "active"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(1000ms)
  alive(child)
  uu.write(s, "nofile", "gone\n")
  time.sleep(1000ms)
  uu.write(s, "active", "here\n")
  time.sleep(1000ms)
  alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
  let r = streams(s)?
  uu.stdout_is(r, "==> active <==\nhere\n")
  uu.stderr_is(r, "tail: cannot open 'nofile' for reading: No such file or directory\n")
}

# origin: uutils test_tail::test_dev_zero
test test_uu_tail_dev_zero { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-c", "1", "/dev/zero"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.succeeds(r)
  uu.stdout_only(r, "\0")
  }
}

# origin: uutils test_tail::test_dir
test test_uu_tail_dir { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "DIR")
  let r = uu.invoke(s, "tail", ["DIR"], stdin: b"")?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "tail: error reading 'DIR': Is a directory\n")
}

# origin: uutils test_tail::test_dir_follow
test test_uu_tail_dir_follow { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "DIR")
  for mode in ["--follow=descriptor", "--follow=name"] {
    let r = uu.invoke(s, "tail", [mode, "DIR"])?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
    uu.stderr_is(r, "tail: error reading 'DIR': Is a directory\ntail: DIR: cannot follow end of this type of file; giving up on this name\ntail: no files remaining\n")
  }
}

# origin: uutils test_tail::test_dir_follow_retry
test test_uu_tail_dir_follow_retry { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "DIR")
  let r = uu.invoke(s, "tail", ["--follow=descriptor", "--retry", "DIR"], stdin: b"")?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "tail: warning: --retry only effective for the initial open\ntail: error reading 'DIR': Is a directory\ntail: DIR: cannot follow end of this type of file\ntail: no files remaining\n")
}

# origin: uutils test_tail::test_failed_warning_write_is_reported
test test_uu_tail_failed_warning_write_is_reported { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["--pid=0", "/dev/null"], stdin: b"", stderr: p"/dev/full")?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_tail::test_fifo
test test_uu_tail_fifo { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, "FIFO")
  for args in [["FIFO"], ["-f", "FIFO"], ["-F", "FIFO"]] {
  let plan = follow(s, args)?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(500ms)
  alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
  uu.no_output(streams(s)?)
  }
}

# origin: uutils test_tail::test_fifo_with_pid
test test_uu_tail_fifo_with_pid { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, ".dummy-input")
  let dummy_plan = process.command_argv(p"/bin/sh", [p"/bin/sh", p"-c", Path(r"""echo $$ > .dummy-pid; exec /bin/sh < .dummy-input""")], s.root, {}, b"", uu.at(s, ".dummy-out"), uu.at(s, ".dummy-err"))
  let dummy = spawn dummy_plan ?
  defer dummy.cancel(signal: "KILL", kill_after: 0ms)
  let writer = unix.open_fd(uu.at(s, ".dummy-input"), write: true)?
  defer unix.close_fd(writer)
  for i in range(500) {
    if uu.at(s, ".dummy-pid").exists() and uu.read_text(s, ".dummy-pid")?.trim() != "" { break }
    time.sleep(10ms)
  }
  let pid = uu.read_text(s, ".dummy-pid")?.trim().parse_int()?
  uu.mkfifo(s, "FIFO")
  let plan = follow(s, ["-f", f"--pid={pid}", "FIFO"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(500ms)
  alive(child)
  process.kill(pid, "USR1")
  let dead = process.wait_any([dummy])?
  time.sleep(1000ms)
  let finished = process.wait_timeout([child], 0ms)?
  assert finished != null
  assert finished.status.exit_code()? == 0
  uu.no_output(streams(s)?)
}

# origin: uutils test_tail::test_follow_dangling_symlink
test test_uu_tail_follow_dangling_symlink { |ctx|
  let s = uu.scene(ctx)?
  uu.symlink(s, "target", "link")
  let plan = follow(s, ["-s.1", "--max-unchanged-stats=1", "-F", "link"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(500ms)
  uu.write(s, "target", "X\n")
  time.sleep(500ms)
  child.cancel(signal: "KILL", kill_after: 0ms)
  uu.stdout_is(streams(s)?, "X\n")
}

# origin: uutils test_tail::test_follow_descriptor_vs_rename1
test test_uu_tail_follow_descriptor_vs_rename1 { |ctx|
  let s = uu.scene(ctx)?
  for row in [{delay: 100ms, extra: ["---disable-inotify"]}, {delay: 33ms, extra: []}] {
    uu.touch(s, "FILE_A")
  let plan = follow(s, ["--follow=descriptor", "-s.1", "--max-unchanged-stats=1", "FILE_A"].extend(row.extra))?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
    time.sleep(row.delay)
    alive(child)
    append(s, "FILE_A", "A\n")
    # Tail has opened the file once it echoes the first line; a rename before
    # that races its startup, not the descriptor behavior under test.
    wait_output(s, bytes.from_text("A\n"), b"")
    time.sleep(row.delay)
    uu.rename(s, "FILE_A", "FILE_B")
    time.sleep(row.delay)
    append(s, "FILE_B", "B\n")
    wait_output(s, bytes.from_text("A\nB\n"), b"")
    time.sleep(row.delay)
    uu.at(s, "FILE_B").rename(to: uu.at(s, "FILE_C"), overwrite: true)
    time.sleep(row.delay)
    append(s, "FILE_C", "C\n")
    wait_output(s, bytes.from_text("A\nB\nC\n"), b"")
    time.sleep(row.delay)
    alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
    uu.stdout_only(streams(s)?, "A\nB\nC\n")
  }
}

# origin: uutils test_tail::test_follow_descriptor_vs_rename2
test test_uu_tail_follow_descriptor_vs_rename2 { |ctx|
  let s = uu.scene(ctx)?
  for row in [{delay: 150ms, extra: ["---disable-inotify"]}, {delay: 50ms, extra: []}] {
    uu.touch(s, "FILE_A")
    uu.touch(s, "FILE_B")
  let plan = follow(s, ["--follow=descriptor", "-s.1", "--max-unchanged-stats=1", "FILE_A", "FILE_B", "--verbose"].extend(row.extra))?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
    time.sleep(row.delay)
    alive(child)
    # Do not rename either path until tail has opened both descriptors.
    wait_output(s, bytes.from_text("==> FILE_A <==\n\n==> FILE_B <==\n"), b"")
    uu.at(s, "FILE_A").rename(to: uu.at(s, "FILE_C"), overwrite: true)
    time.sleep(row.delay)
    append(s, "FILE_C", "X\n")
    time.sleep(row.delay)
    alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
    uu.stdout_only(streams(s)?, "==> FILE_A <==\n\n==> FILE_B <==\n\n==> FILE_A <==\nX\n")
  }
}

# origin: uutils test_tail::test_follow_detects_file_truncation
test test_uu_tail_follow_detects_file_truncation { |ctx|
  let s = uu.scene(ctx)?
  if (env.get("CI") ?? "").lower() == "true" { test.skip("upstream skips tight timing under CI") }
  for row in [{delay: 1000ms, extra: ["---disable-inotify"]}, {delay: 250ms, extra: []}] {
    for mode in ["-f", "-F"] {
      uu.write(s, "data", "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n")
  let plan = follow(s, ["-s.1", "--max-unchanged-stats=1", "data"].extend(row.extra).extend([mode]))?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
      time.sleep(row.delay)
      alive(child)
      uu.write(s, "data", "11\n12\n13\n14\n15\n")
      time.sleep(row.delay)
      alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
      let r = streams(s)?
      uu.stderr_is(r, "tail: data: file truncated\n")
      uu.stdout_is(r, "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n14\n15\n")
    }
  }
}

# origin: uutils test_tail::test_follow_missing
test test_uu_tail_follow_missing { |ctx|
  let s = uu.scene(ctx)?
  for mode in ["--follow=descriptor", "--follow=name", "--fo=d", "--fo=n"] {
    let r = uu.invoke(s, "tail", [mode, "missing"])?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
    uu.stderr_is(r, "tail: cannot open 'missing' for reading: No such file or directory\ntail: no files remaining\n")
  }
}

# origin: uutils test_tail::test_follow_multiple
test test_uu_tail_follow_multiple { |ctx|
  for option in ["-f"] {
  let s = uu.scene(ctx)?
  uu.fixture(s, "tail", "foobar.txt", "foobar.txt")
  uu.fixture(s, "tail", "foobar2.txt", "foobar2.txt")
  let plan = follow(s, [option, "foobar.txt", "foobar2.txt"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(500ms)
  alive(child)
  let initial = streams(s)?
  uu.stdout_only_bytes(initial, fixture_bytes(ctx, "foobar_follow_multiple.expected")?)
  append(s, "foobar2.txt", "trois\n")
  time.sleep(1000ms)
  let first = streams(s)?
  uu.stdout_only(streams(s, initial.stdout.len(), initial.stderr.len())?, "trois\n")
  append(s, "foobar.txt", "twenty\nthirty\n")
  time.sleep(1000ms)
  uu.stdout_only_bytes(streams(s, first.stdout.len(), first.stderr.len())?, fixture_bytes(ctx, "foobar_follow_multiple_appended.expected")?)
  alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
  }
}

# origin: uutils test_tail::test_follow_multiple_untailable
test test_uu_tail_follow_multiple_untailable { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "DIR1")
  uu.mkdir(s, "DIR2")
  let r = uu.invoke(s, "tail", ["-f", "DIR1", "DIR2"], stdin: b"")?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "==> DIR1 <==\n\n==> DIR2 <==\n")
  uu.stderr_is(r, "tail: error reading 'DIR1': Is a directory\ntail: DIR1: cannot follow end of this type of file; giving up on this name\ntail: error reading 'DIR2': Is a directory\ntail: DIR2: cannot follow end of this type of file; giving up on this name\ntail: no files remaining\n")
}

# origin: uutils test_tail::test_follow_name_hash_table_stress
test test_uu_tail_follow_name_hash_table_stress { |ctx|
  let s = uu.scene(ctx)?
  for name in ["1", "2", "3", "4", "5", "6", "7", "8", "9"] { uu.touch(s, name) }
  for row in [{delay: 100ms, extra: [], expected: "x\na\n"}, {delay: 300ms, extra: ["---disable-inotify"], expected: "a\nx\na\n"}] {
  let plan = follow(s, ["-s.1", "--max-unchanged-stats=1", "-q", "-F", "1", "2", "3", "4", "5", "6", "7", "8", "9"].extend(row.extra))?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
    time.sleep(row.delay)
    alive(child)
    uu.write(s, "9", "x\n")
    time.sleep(row.delay)
    uu.rename(s, "1", "moved")
    time.sleep(row.delay)
    uu.write(s, "1", "a\n")
    time.sleep(row.delay)
    alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
    let r = streams(s)?
    uu.stderr_is(r, "tail: '1' has become inaccessible: No such file or directory\ntail: '1' has appeared;  following new file\n")
    uu.stdout_is(r, row.expected)
    uu.remove(s, "moved")
  }
}

# origin: uutils test_tail::test_follow_name_move_create1
test test_uu_tail_follow_name_move_create1 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "tail", "follow_name.txt", "follow_name.txt")
  let plan = follow(s, ["--follow=name", "follow_name.txt"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(500ms)
  alive(child)
  uu.rename(s, "follow_name.txt", "backup")
  time.sleep(500ms)
  uu.at(s, "backup").copy(to: uu.at(s, "follow_name.txt"), overwrite: true)
  time.sleep(500ms)
  let ended = process.wait_timeout([child], 0ms)?
  assert ended != null
  child.cancel(signal: "KILL", kill_after: 0ms)
  let r = streams(s)?
  uu.stdout_is_bytes(r, fixture_bytes(ctx, "follow_name_short.expected")?)
  uu.stderr_is(r, "tail: 'follow_name.txt' has become inaccessible: No such file or directory\ntail: no files remaining\n")
}

# origin: uutils test_tail::test_follow_name_multiple
test test_uu_tail_follow_name_multiple { |ctx|
  for option in ["--follow=name", "--follo=nam", "--f=n"] {
  let s = uu.scene(ctx)?
  uu.fixture(s, "tail", "foobar.txt", "foobar.txt")
  uu.fixture(s, "tail", "foobar2.txt", "foobar2.txt")
  let plan = follow(s, [option, "foobar.txt", "foobar2.txt"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(100ms)
  alive(child)
  let initial = streams(s)?
  uu.stdout_only_bytes(initial, fixture_bytes(ctx, "foobar_follow_multiple.expected")?)
  append(s, "foobar2.txt", "trois\n")
  time.sleep(100ms)
  let first = streams(s)?
  uu.stdout_only(streams(s, initial.stdout.len(), initial.stderr.len())?, "trois\n")
  append(s, "foobar.txt", "twenty\nthirty\n")
  time.sleep(100ms)
  uu.stdout_only_bytes(streams(s, first.stdout.len(), first.stderr.len())?, fixture_bytes(ctx, "foobar_follow_multiple_appended.expected")?)
  alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
  }
}

# origin: uutils test_tail::test_follow_name_shows_headers_on_creation
test test_uu_tail_follow_name_shows_headers_on_creation { |ctx|
  let s = uu.scene(ctx)?
  let delay = 150ms
  let plan = follow(s, ["-F", "-s.1", "--max-unchanged-stats=1", "log1", "log2", "---disable-inotify"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  let initial_stderr = "tail: cannot open 'log1' for reading: No such file or directory\ntail: cannot open 'log2' for reading: No such file or directory\n"
  time.sleep(delay)
  alive(child)
  # The missing-file diagnostics confirm tail completed its initial open pass.
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

# origin: uutils test_tail::test_follow_name_stdin
test test_uu_tail_follow_name_stdin { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "FILE1")
  uu.touch(s, "FILE2")
  for args in [["--follow=name", "-"], ["--follow=name", "FILE1", "-", "FILE2"]] {
    let r = uu.invoke(s, "tail", args)?
    uu.fails_with_code(r, 1)
    uu.stderr_is(r, "tail: cannot follow '-' by name\n")
  }
}

# origin: uutils test_tail::test_follow_name_truncate1
test test_uu_tail_follow_name_truncate1 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "tail", "follow_name.txt", "follow_name.txt")
  let expected = fixture_bytes(ctx, "follow_name.expected")?
  let text = expected.utf8()?
  let prefix = text.split("END(25)\n")[0] + "END(25)\n"
  let plan = follow(s, ["--follow=name", "follow_name.txt"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  alive(child)
  for i in range(500) {
    if streams(s)?.stdout == bytes.from_text(prefix) { break }
    time.sleep(10ms)
  }
  uu.at(s, "follow_name.txt").copy(to: uu.at(s, "backup"))
  time.sleep(1000ms)
  uu.touch(s, "follow_name.txt")
  time.sleep(1000ms)
  uu.at(s, "backup").copy(to: uu.at(s, "follow_name.txt"), overwrite: true)
  time.sleep(1000ms)
  alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
  let r = streams(s)?
  uu.stderr_is(r, "tail: follow_name.txt: file truncated\n")
  uu.stdout_is_bytes(r, expected)
}

# origin: uutils test_tail::test_follow_name_truncate2
test test_uu_tail_follow_name_truncate2 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")
  let plan = follow(s, ["--follow=name", "file"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  alive(child)
  append(s, "file", "x\n")
  wait_output(s, b"x\n", b"")
  append(s, "file", "x\n")
  wait_output(s, b"x\nx\n", b"")
  append(s, "file", "x\n")
  wait_output(s, b"x\nx\nx\n", b"")
  uu.write(s, "file", "x\n")
  wait_output(s, b"x\nx\nx\nx\n", b"tail: file: file truncated\n")
  alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
  let r = streams(s)?
  uu.stdout_is(r, "x\nx\nx\nx\n")
  uu.stderr_is(r, "tail: file: file truncated\n")
}

# origin: uutils test_tail::test_follow_name_truncate3
test test_uu_tail_follow_name_truncate3 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "file")
  let plan = follow(s, ["--follow=name", "file"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(1000ms)
  alive(child)
  uu.write(s, "file", "x\n")
  time.sleep(1000ms)
  alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
  uu.stdout_only(streams(s)?, "x\n")
}

# origin: uutils test_tail::test_follow_name_truncate4
test test_uu_tail_follow_name_truncate4 { |ctx|
  let s = uu.scene(ctx)?
  for row in [{delay: 500ms, extra: []}, {delay: 1500ms, extra: ["---disable-inotify"]}] {
  append(s, "file", "foobar\n")
  let plan = follow(s, ["-s.1", "--max-unchanged-stats=1", "-F", "file"].extend(row.extra))?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(row.delay)
  alive(child)
  uu.write(s, "file", "foobar\n")
  time.sleep(row.delay)
  alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
  uu.stdout_only(streams(s)?, "foobar\n")
  uu.remove(s, "file")
  }
}

# origin: uutils test_tail::test_follow_non_utf8_bytes
test test_uu_tail_follow_non_utf8_bytes { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "tail", "foobar.txt", "foobar.txt")
  let plan = follow(s, ["-f", "foobar.txt"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(500ms)
  alive(child)
  let initial = streams(s)?
  uu.stdout_only_bytes(initial, fixture_bytes(ctx, "foobar_single_default.expected")?)
  append_bytes(s, "foobar.txt", b"\x80\n")
  time.sleep(1000ms)
  uu.stdout_only_bytes(streams(s, initial.stdout.len(), initial.stderr.len())?, b"\x80\n")
  alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)

}

# origin: uutils test_tail::test_follow_pipe_f
test test_uu_tail_follow_pipe_f { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-f", "-c3", "-s.1", "--max-unchanged-stats=1"], stdin: bytes.from_text("foo\n"), timeout: 30s)?
  uu.succeeds(r)
  uu.stdout_only(r, "oo\n")
  }
}

# origin: uutils test_tail::test_follow_redirect_stdin_name_retry
test test_uu_tail_follow_redirect_stdin_name_retry { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")
  for args in [["-F", "-"], ["-F"]] {
    let r = uu.invoke_from_path(s, "tail", args, uu.at(s, "f"))?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
    uu.stderr_is(r, "tail: cannot follow '-' by name\n")
  }
}

# origin: uutils test_tail::test_follow_single
test test_uu_tail_follow_single { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "tail", "foobar.txt", "foobar.txt")
  let plan = follow(s, ["-f", "foobar.txt"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(200ms)
  alive(child)
  let initial = streams(s)?
  uu.stdout_only_bytes(initial, fixture_bytes(ctx, "foobar_single_default.expected")?)
  append(s, "foobar.txt", "line1\nline2\n")
  time.sleep(1000ms)
  alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
  uu.stdout_only(streams(s, initial.stdout.len(), initial.stderr.len())?, "line1\nline2\n")
}

# origin: uutils test_tail::test_follow_stdin_descriptor
test test_uu_tail_follow_stdin_descriptor { |ctx|
  let s = uu.scene(ctx)?
  uu.mkfifo(s, ".input-pipe")
  for args in [["-f", "-"], ["-f"]] {
  let argv = uu.argv(s, "tail", [Path(word) for word in args])?
  let words = [p"/bin/sh", p"-c", Path(r"""exec "$@" < .input-pipe"""), p"tail-input"].extend(argv)
  let plan = process.command_argv(p"/bin/sh", words, s.root, {}, b"", uu.at(s, ".live-out"), uu.at(s, ".live-err"))
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  let writer = unix.open_fd(uu.at(s, ".input-pipe"), write: true)?
  defer unix.close_fd(writer)
  time.sleep(500ms)
  alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
  uu.no_output(streams(s)?)
  }
}

# origin: uutils test_tail::test_follow_stdin_name_retry
test test_uu_tail_follow_stdin_name_retry { |ctx|
  let s = uu.scene(ctx)?
  for args in [["-F", "-"], ["-F"]] {
    let r = uu.invoke(s, "tail", args)?
    uu.fails_with_code(r, 1)
    uu.no_stdout(r)
    uu.stderr_is(r, "tail: cannot follow '-' by name\n")
  }
}

# origin: uutils test_tail::test_follow_stdin_pipe
test test_uu_tail_follow_stdin_pipe { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-f"], stdin: fixture_bytes(ctx, "foobar.txt")?, timeout: 30s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture_bytes(ctx, "follow_stdin.expected")?)
  uu.no_stderr(r)
  }
}

# origin: uutils test_tail::test_follow_stdout_pipe_close
test test_uu_tail_follow_stdout_pipe_close { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "f", "line1\nline2\n")
  uu.mkfifo(s, ".output-pipe")
  let argv = uu.argv(s, "tail", [p"-f", p"-s.1", p"--max-unchanged-stats=1", p"f"])?
  let words = [p"/bin/sh", p"-c", Path(r"""exec "$@" > .output-pipe"""), p"tail-output"].extend(argv)
  let plan = process.command_argv(p"/bin/sh", words, s.root, {}, b"", uu.at(s, ".live-out"), uu.at(s, ".live-err"))
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  let reader = unix.open_fd(uu.at(s, ".output-pipe"), nonblock: false)?
  let first_line = unix.read_fd(reader, 6)?
  assert first_line == b"line1\n"
  unix.close_fd(reader)
  time.sleep(2000ms)
  assert process.wait_timeout([child], 0ms)? != null
}

# origin: uutils test_tail::test_follow_symlink_target_change
test test_uu_tail_follow_symlink_target_change { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "t1", "A\n")
  uu.symlink(s, "t1", "link")
  let plan = follow(s, ["-s.1", "--max-unchanged-stats=1", "-F", "link"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(500ms)
  uu.remove(s, "link")
  uu.symlink(s, "t2", "link")
  time.sleep(500ms)
  uu.write(s, "t2", "B\n")
  time.sleep(500ms)
  child.cancel(signal: "KILL", kill_after: 0ms)
  let r = streams(s)?
  uu.stdout_contains(r, "A\n")
  uu.stdout_contains(r, "B\n")
}

# origin: uutils test_tail::test_follow_when_files_are_pointing_to_same_relative_file_and_file_stays_same_size
test test_uu_tail_follow_when_files_are_pointing_to_same_relative_file_and_file_stays_same_size { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "data", "file data")
  let absolute = uu.at(s, "data").resolve()?.display()
  let plan = follow(s, ["--follow=descriptor", "--max-unchanged-stats=1", "--sleep-interval=0.1", "data", absolute])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(500ms)
  uu.write(s, "data", "same data")
  time.sleep(500ms)
  alive(child)
  child.cancel(signal: "KILL", kill_after: 0ms)
  uu.stdout_only(streams(s)?, f"==> data <==\nfile data\n==> {absolute} <==\nfile data")
}

# origin: uutils test_tail::test_follow_with_pid
test test_uu_tail_follow_with_pid { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "tail", "foobar.txt", "foobar.txt")
  uu.fixture(s, "tail", "foobar2.txt", "foobar2.txt")
  uu.mkfifo(s, ".dummy-input")
  let dummy_plan = process.command_argv(p"/bin/sh", [p"/bin/sh", p"-c", Path(r"""echo $$ > .dummy-pid; exec /bin/sh < .dummy-input""")], s.root, {}, b"", uu.at(s, ".dummy-out"), uu.at(s, ".dummy-err"))
  let dummy = spawn dummy_plan ?
  defer dummy.cancel(signal: "KILL", kill_after: 0ms)
  let writer = unix.open_fd(uu.at(s, ".dummy-input"), write: true)?
  defer unix.close_fd(writer)
  for i in range(500) {
    if uu.at(s, ".dummy-pid").exists() and uu.read_text(s, ".dummy-pid")?.trim() != "" { break }
    time.sleep(10ms)
  }
  let pid = uu.read_text(s, ".dummy-pid")?.trim().parse_int()?
  let plan = follow(s, ["-f", f"--pid={pid}", "foobar.txt", "foobar2.txt"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(500ms)
  alive(child)
  let initial = streams(s)?
  uu.stdout_only_bytes(initial, fixture_bytes(ctx, "foobar_follow_multiple.expected")?)
  append(s, "foobar2.txt", "trois\n")
  time.sleep(1000ms)
  let first = streams(s)?
  uu.stdout_only(streams(s, initial.stdout.len(), initial.stderr.len())?, "trois\n")
  append(s, "foobar.txt", "twenty\nthirty\n")
  time.sleep(1000ms)
  alive(child)
  let second = streams(s)?
  uu.stdout_only_bytes(streams(s, first.stdout.len(), first.stderr.len())?, fixture_bytes(ctx, "foobar_follow_multiple_appended.expected")?)
  process.kill(pid, "USR1")
  let dead = process.wait_any([dummy])?
  time.sleep(1000ms)
  append(s, "foobar.txt", "should\nbe\nignored\n")
  time.sleep(1000ms)
  let finished = process.wait_timeout([child], 0ms)?
  assert finished != null
  assert finished.status.exit_code()? == 0
  uu.no_output(streams(s, second.stdout.len(), second.stderr.len())?)
}

# origin: uutils test_tail::test_following_with_pid
test test_uu_tail_following_with_pid { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")
  let dummy_plan = process.command_argv(p"/bin/sh", [p"/bin/sh", p"-c", Path(r"""echo $$ > .dummy-pid; exec sleep 999d""")], s.root, {}, b"", uu.at(s, ".dummy-out"), uu.at(s, ".dummy-err"))
  let dummy = spawn dummy_plan ?
  defer dummy.cancel(signal: "KILL", kill_after: 0ms)
  for i in range(500) {
    if uu.at(s, ".dummy-pid").exists() { break }
    time.sleep(10ms)
  }
  for i in range(500) {
    if uu.at(s, ".dummy-pid").exists() and uu.read_text(s, ".dummy-pid")?.trim() != "" { break }
    time.sleep(10ms)
  }
  let pid = uu.read_text(s, ".dummy-pid")?.trim().parse_int()?
  let plan = follow(s, ["--pid", f"{pid}", "-f", uu.at(s, "f").display()], merged: true)?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(2000ms)
  alive(child)
  process.kill(pid, "KILL")
  let dead = process.wait_any([dummy])?
  time.sleep(2000ms)
  assert process.wait_timeout([child], 0ms)? != null
  child.cancel(signal: "KILL", kill_after: 0ms)

}

# origin: uutils test_tail::test_header_quotes_name_with_control_char
test test_uu_tail_header_quotes_name_with_control_char { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "tab\there", "t\n")
  let r = uu.invoke(s, "tail", ["-v", "-n1", "tab\there"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "==> 'tab'$'\\t''here' <==\nt\n")
}

# origin: uutils test_tail::test_header_quotes_names_needing_it
test test_uu_tail_header_quotes_names_needing_it { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "plain", "p\n")
  uu.write(s, "two words", "w\n")
  let r = uu.invoke(s, "tail", ["-n1", "plain", "two words"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "==> plain <==\np\n\n==> 'two words' <==\nw\n")
}

# origin: uutils test_tail::test_invalid_arg
test test_uu_tail_invalid_arg { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["--definitely-invalid"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.fails_with_code(r, 1)
  }
}

# origin: uutils test_tail::test_invalid_count_keeps_its_leading_zeros
test test_uu_tail_invalid_count_keeps_its_leading_zeros { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-c", "0fb", "/dev/null"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "tail: invalid number of bytes: '0fb'\n")
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-n", "000ff", "/dev/null"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "tail: invalid number of lines: '000ff'\n")
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-c-0fb", "/dev/null"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.fails_with_code(r, 1)
  uu.stderr_is(r, "tail: invalid number of bytes: '0fb'\n")
  }
}

# origin: uutils test_tail::test_invalid_num
test test_uu_tail_invalid_num { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-c", "1024R", "emptyfile.txt"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.fails(r)
  assert r.stderr.utf8()?.starts_with("tail: cannot open 'emptyfile.txt' for reading: No such file or directory")
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-n", "1024R", "emptyfile.txt"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.fails(r)
  assert r.stderr.utf8()?.starts_with("tail: cannot open 'emptyfile.txt' for reading: No such file or directory")
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-c", "1Y"], stdin: bytes.from_text("x"), timeout: 30s)?
  uu.succeeds(r)
  uu.stdout_is(r, "x")
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-n", "1Y"], stdin: bytes.from_text("x\n"), timeout: 30s)?
  uu.succeeds(r)
  uu.stdout_is(r, "x\n")
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-c", "-³"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.fails(r)
  assert r.stderr.utf8()?.starts_with("tail: invalid number of bytes: '\\302\\263'")
  }
}

# origin: uutils test_tail::test_lines_with_size_suffix
test test_uu_tail_lines_with_size_suffix { |ctx|
  let s = uu.scene(ctx)?
  let content = [f"Line {i}\n" for i in range(3000)].join("")
  let expected = [f"Line {i}\n" for i in range(952, 3000)].join("")
  uu.write(s, "test_lines_with_size_suffix.txt", content)
  uu.write(s, "test_lines_with_size_suffix_expected.txt", expected)
  let r = uu.invoke(s, "tail", ["test_lines_with_size_suffix.txt", "-n", "2K"])?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, uu.read(s, "test_lines_with_size_suffix_expected.txt")?)
}

# origin: uutils test_tail::test_lines_zero_terminated
test test_uu_tail_lines_zero_terminated { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-z", "-n", "2"], stdin: bytes.from_text("a\0b\0c\0d\0e\0"), timeout: 30s)?
  uu.succeeds(r)
  uu.stdout_only(r, "d\0e\0")
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-z", "-n", "+2"], stdin: bytes.from_text("a\0b\0c\0d\0e\0"), timeout: 30s)?
  uu.succeeds(r)
  uu.stdout_only(r, "b\0c\0d\0e\0")
  }
}

# origin: uutils test_tail::test_lowercase_multiplier_suffixes_rejected
test test_uu_tail_lowercase_multiplier_suffixes_rejected { |ctx|
  let s = uu.scene(ctx)?
  for suffix in ["g", "t", "p", "e", "z", "y", "r", "q"] {
    for row in [{option: "-c", unit: "bytes"}, {option: "-n", unit: "lines"}] {
      let r = uu.invoke(s, "tail", [row.option, f"2{suffix}"])?
      uu.fails(r)
      assert r.stderr.utf8()?.starts_with(f"tail: invalid number of {row.unit}: '2{suffix}'")
    }
  }
}

# origin: uutils test_tail::test_multiple_input_files
test test_uu_tail_multiple_input_files { |ctx|
  {
  let s = uu.scene(ctx)?
  uu.fixture(s, "tail", "foobar.txt", "foobar.txt")
  uu.fixture(s, "tail", "foobar2.txt", "foobar2.txt")
  let r = uu.invoke(s, "tail", ["foobar.txt", "foobar2.txt"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is_bytes(r, fixture_bytes(ctx, "foobar_follow_multiple.expected")?)
  }
}

# origin: uutils test_tail::test_multiple_input_files_missing
test test_uu_tail_multiple_input_files_missing { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "tail", "foobar.txt", "foobar.txt")
  uu.fixture(s, "tail", "foobar2.txt", "foobar2.txt")
  let r = uu.invoke(s, "tail", ["foobar.txt", "missing1", "foobar2.txt", "missing2"], stdin: b"")?
  uu.fails(r)
  uu.stdout_is_bytes(r, fixture_bytes(ctx, "foobar_follow_multiple.expected")?)
  uu.stderr_is(r, "tail: cannot open 'missing1' for reading: No such file or directory\ntail: cannot open 'missing2' for reading: No such file or directory\n")
}

# origin: uutils test_tail::test_multiple_input_files_with_suppressed_headers
test test_uu_tail_multiple_input_files_with_suppressed_headers { |ctx|
  {
  let s = uu.scene(ctx)?
  uu.fixture(s, "tail", "foobar.txt", "foobar.txt")
  uu.fixture(s, "tail", "foobar2.txt", "foobar2.txt")
  let r = uu.invoke(s, "tail", ["foobar.txt", "foobar2.txt", "-q"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture_bytes(ctx, "foobar_multiple_quiet.expected")?)
  }
}

# origin: uutils test_tail::test_multiple_input_quiet_flag_overrides_verbose_flag_for_suppressing_headers
test test_uu_tail_multiple_input_quiet_flag_overrides_verbose_flag_for_suppressing_headers { |ctx|
  {
  let s = uu.scene(ctx)?
  uu.fixture(s, "tail", "foobar.txt", "foobar.txt")
  uu.fixture(s, "tail", "foobar2.txt", "foobar2.txt")
  let r = uu.invoke(s, "tail", ["foobar.txt", "foobar2.txt", "-v", "-q"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture_bytes(ctx, "foobar_multiple_quiet.expected")?)
  }
}

# origin: uutils test_tail::test_n0_with_follow
test test_uu_tail_n0_with_follow { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "test.txt", "line1\nline2\nline3\n")
  let plan = follow(s, ["-n0", "-f", "test.txt"])?
  let child = spawn plan ?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(500ms)
  alive(child)
  append(s, "test.txt", "new\n")
  time.sleep(1000ms)
  uu.stdout_only(streams(s)?, "new\n")
  child.cancel(signal: "KILL", kill_after: 0ms)

}

# origin: uutils test_tail::test_n_greater_than_number_of_lines
test test_uu_tail_n_greater_than_number_of_lines { |ctx|
  {
  let s = uu.scene(ctx)?
  uu.fixture(s, "tail", "foobar.txt", "foobar.txt")
  let r = uu.invoke(s, "tail", ["-n", "99999999", "foobar.txt"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, fixture_bytes(ctx, "foobar.txt")?)
  }
}

# origin: uutils test_tail::test_nc_0_wo_follow
test test_uu_tail_nc_0_wo_follow { |ctx|
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-n0", "missing"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.succeeds(r)
  uu.no_output(r)
  }
  {
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "tail", ["-c0", "missing"], stdin: bytes.from_text(""), timeout: 30s)?
  uu.succeeds(r)
  uu.no_output(r)
  }
}

# origin: uutils test_tail::test_nc_0_wo_follow2
test test_uu_tail_nc_0_wo_follow2 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "unreadable")
  uu.set_mode(s, "unreadable", 0)
  defer uu.set_mode(s, "unreadable", 0o600)
  for option in ["-n0", "-c0"] {
    let r = uu.invoke(s, "tail", [option, "unreadable"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
}

# origin: uutils test_tail::test_negative_bytes_file_count_past_seek_limit
test test_uu_tail_negative_bytes_file_count_past_seek_limit { |ctx|
  let s = uu.scene(ctx)?
  let content = ["a" for i in range(8192)].join("")
  uu.write(s, "big", content)
  for count in ["-9223372036854775808", "-18446744073709551615"] {
    let r = uu.invoke(s, "tail", ["-c", count, "big"])?
    uu.succeeds(r)
    uu.stdout_only(r, content)
  }
}
