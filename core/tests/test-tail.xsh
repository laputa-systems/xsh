type Ran = {status: Int, stdout: Bytes, stderr: Str}
type FollowCase = {args: List[Str], expected: Bytes}

# Runs core/tail.xsh by its real path inside `root`, capturing both streams.
proc tail_run(ctx: TestContext, root: Path, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/tail.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

proc numbers(from: Int, to: Int) -> Bytes {
  bytes.concat([bytes.from_text(f"{n}\n") for n in range(from, to + 1)])
}

test test_tail_last_lines_of_files_and_stdin { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?
  fp"{root}/n20".write(numbers(1, 20))

  assert tail_run(ctx, root, ["n20"])?.stdout == numbers(11, 20)
  assert tail_run(ctx, root, [], numbers(1, 20))?.stdout == numbers(11, 20)
  assert tail_run(ctx, root, ["-n", "3", "n20"])?.stdout == numbers(18, 20)
  assert tail_run(ctx, root, ["-n", "-3", "n20"])?.stdout == numbers(18, 20)
  assert tail_run(ctx, root, ["--lines=2"], numbers(1, 20))?.stdout == numbers(19, 20)
  assert tail_run(ctx, root, ["-n", "99", "n20"])?.stdout == numbers(1, 20)
  assert tail_run(ctx, root, ["-n", "1"], b"x\ny")?.stdout == b"y"
  assert tail_run(ctx, root, ["-n", "1"], b"a\n\xf0\x9f\x92\x90\n")?.stdout == b"\xf0\x9f\x92\x90\n"
}

test test_tail_last_lines_of_a_large_file_seek_backward_across_chunks { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?
  let big = numbers(1, 30000)
  fp"{root}/big".write(big)

  assert tail_run(ctx, root, ["-n", "12000", "big"])?.stdout == numbers(18001, 30000)
  assert tail_run(ctx, root, ["-c", "100000", "big"])?.stdout == big[big.len() - 100000..]
  assert tail_run(ctx, root, ["-n", "+29999", "big"])?.stdout == numbers(29999, 30000)
  assert tail_run(ctx, root, ["-n", "+1", "big"])?.stdout == big
}

test test_tail_from_start_counts { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?
  fp"{root}/n20".write(numbers(1, 20))

  assert tail_run(ctx, root, ["-n", "+19", "n20"])?.stdout == numbers(19, 20)
  assert tail_run(ctx, root, ["-n", "+0"], b"a\nb\n")?.stdout == b"a\nb\n"
  assert tail_run(ctx, root, ["-n", "+3"], b"a\nb")?.stdout == b""
  assert tail_run(ctx, root, ["-c", "+3"], b"abcde")?.stdout == b"cde"
  assert tail_run(ctx, root, ["-c", "+3", "n20"])?.stdout == numbers(1, 20)[2..]
  assert tail_run(ctx, root, ["-c", "+0"], b"abcde")?.stdout == b"abcde"
  assert tail_run(ctx, root, ["-c", "+99999999999999999999999"], b"abcde")?.stdout == b""
}

test test_tail_obsolete_forms_name_the_first_argument { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?

  assert tail_run(ctx, root, ["-3"], numbers(1, 5))?.stdout == numbers(3, 5)
  assert tail_run(ctx, root, ["+2"], b"x\ny\n")?.stdout == b"y\n"
  assert tail_run(ctx, root, ["+2c"], b"wxyz")?.stdout == b"xyz"
  assert tail_run(ctx, root, ["-1c"], b"wxyz")?.stdout == b"z"
  assert tail_run(ctx, root, ["-1l"], b"x\ny")?.stdout == b"y"
  assert tail_run(ctx, root, ["-0"], b"x\ny")?.stdout == b""
  assert tail_run(ctx, root, ["-9999999999999999999b"], b"x")?.stdout == b"x"
  assert tail_run(ctx, root, ["-2", "-n", "1"], b"x\ny\n")?.stdout == b"y\n", "later options override the obsolete count"

  let digits = tail_run(ctx, root, ["-5cz"])?
  assert digits.status == 1
  assert digits.stderr == "tail: option used in invalid context -- 5\n", digits.stderr

  let plus = tail_run(ctx, root, ["+cl"])?
  assert plus.stderr == "tail: cannot open '+cl' for reading: No such file or directory\n", plus.stderr
}

test test_tail_accepts_gnu_hidden_disable_inotify_option { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?
  fp"{root}/n20".write(numbers(1, 20))

  assert tail_run(ctx, root, ["---disable-inotify", "-n", "2", "n20"])?.stdout == numbers(19, 20)
  assert tail_run(ctx, root, ["-n", "2", "---disable-inotify"], numbers(1, 20))?.stdout == numbers(19, 20)
}

test test_tail_obsolete_unit_options_default_to_ten_units { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?
  let content = bytes.concat([b"x" for _ in range(5122)])

  assert tail_run(ctx, root, ["-l"], numbers(1, 12))?.stdout == numbers(3, 12)
  assert tail_run(ctx, root, ["-b"], content)?.stdout == content[2..]
}

test test_tail_warnings_precede_file_output { |ctx|
  let root = test.temp_dir(ctx, name: "tail-warning-order")?
  let file = fp"{root}/data"
  file.write(b"file data\n")
  let args: List[Union[Str, Path]] = [
    "sh",
    "-c",
    "cd \"$1\" && exec env LC_ALL=C \"$2\" \"$3\" --retry data 2>&1",
    "sh",
    root,
    ctx.xsh_bin,
    fp"{ctx.core_dir}/tail.xsh",
  ]
  let output = run.text @args

  assert output == "tail: warning: --retry ignored; --retry is useful only when following\nfile data\n"
}

test test_tail_counts_accept_gnu_suffixes_and_reject_others { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?
  let block = bytes.concat([b"x" for _ in range(1100)])

  assert tail_run(ctx, root, ["-c", "1kB"], block)?.stdout.len() == 1000
  assert tail_run(ctx, root, ["-c", "1K"], block)?.stdout.len() == 1024
  assert tail_run(ctx, root, ["-c", "1Y"], b"x")?.stdout == b"x"
  assert tail_run(ctx, root, ["-n", "99999999999999999999999999999"], b"a\n")?.stdout == b"a\n"

  let bytes_error = tail_run(ctx, root, ["-c", "2g"], b"x")?
  assert bytes_error.status == 1
  assert bytes_error.stderr == "tail: invalid number of bytes: '2g'\n", bytes_error.stderr

  let plus_error = tail_run(ctx, root, ["-n", "+1fb"], b"x")?
  assert plus_error.stderr == "tail: invalid number of lines: '+1fb'\n", plus_error.stderr

  let bare_option_end = tail_run(ctx, root, ["-c", "--"], b"x")?
  assert bare_option_end.status == 1
  assert bare_option_end.stderr == "tail: invalid number of bytes: '-'\n", bare_option_end.stderr
}

test test_tail_invalid_sleep_intervals_use_usage_diagnostics { |ctx|
  let root = test.temp_dir(ctx, name: "tail-sleep-interval")?
  let invalid = [
    "1_000",
    ".",
    "' '",
    " ",
    "",
    "0,0",
    "one.zero",
    ".zero",
    "one.",
    "0..0",
    "1.0s",
    "1.0e^1000",
  ]

  for value in invalid {
    let result = tail_run(ctx, root, ["--sleep-interval", value])?

    assert result.status == 1
    assert result.stdout.is_empty()
    assert result.stderr.starts_with("tail: invalid number of seconds: "), result.stderr
    assert result.stderr.ends_with("\nTry 'tail --help' for more information.\n"), result.stderr
    if value == "" {
      assert result.stderr == "tail: invalid number of seconds: ''\nTry 'tail --help' for more information.\n", result.stderr
    }
  }
}

test test_tail_zero_terminated_lines_and_non_utf8_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?

  assert tail_run(ctx, root, ["-z", "-n", "2"], b"a\0b\0c\0")?.stdout == b"b\0c\0"
  assert tail_run(ctx, root, ["-n", "1"], b"\xff\r\n\x80\r\n")?.stdout == b"\x80\r\n"
  assert tail_run(ctx, root, ["-c", "2"], b"\xff\xfe\xfd")?.stdout == b"\xfe\xfd"
}

test test_tail_headers_and_open_errors { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?
  fp"{root}/one".write(b"1\n")
  fp"{root}/two words".write(b"2\n")
  fp"{root}/dir".mkdir()

  assert tail_run(ctx, root, ["one", "two words"])?.stdout == b"==> one <==\n1\n\n==> 'two words' <==\n2\n"
  assert tail_run(ctx, root, ["-q", "one", "one"])?.stdout == b"1\n1\n"
  assert tail_run(ctx, root, ["-v", "-"], b"s\n")?.stdout == b"==> 'standard input' <==\ns\n"

  let result = tail_run(ctx, root, ["one", "missing", "dir"])?
  assert result.status == 1
  assert result.stdout == b"==> one <==\n1\n\n==> dir <==\n"
  assert result.stderr == "tail: cannot open 'missing' for reading: No such file or directory\ntail: error reading 'dir': Is a directory\n", result.stderr
}

test test_tail_zero_count_does_not_read { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?

  assert tail_run(ctx, root, ["-n0", "missing"])?.status == 0
  assert tail_run(ctx, root, ["-c", "0", "missing"])?.stdout == b""
}

test test_tail_follow_of_untailable_inputs_matches_gnu { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?
  fp"{root}/dir".mkdir()

  let piped = tail_run(ctx, root, ["-f"], b"foo\n")?
  assert piped.status == 0
  assert piped.stdout == b"foo\n"
  assert piped.stderr == ""

  let dirs = tail_run(ctx, root, ["-f", "dir"])?
  assert dirs.status == 1
  assert dirs.stderr == "tail: error reading 'dir': Is a directory\ntail: dir: cannot follow end of this type of file; giving up on this name\ntail: no files remaining\n", dirs.stderr

  let by_name = tail_run(ctx, root, ["-F", "-"])?
  assert by_name.status == 1
  assert by_name.stderr == "tail: cannot follow '-' by name\n", by_name.stderr

  let abbreviated = tail_run(ctx, root, ["--follow=n", "-"])?
  assert abbreviated.stderr == "tail: cannot follow '-' by name\n", abbreviated.stderr
}

test test_tail_pid_follow_does_not_block_on_fifo_open { |ctx|
  let root = test.temp_dir(ctx, name: "tail-pid-fifo")?
  let fifo = fp"{root}/fifo"
  fs.mkfifo(fifo, 0o600)
  let sleeper = spawn run sleep 30 ?
  defer { sleeper.cancel(signal: "KILL", kill_after: 0ms)? }

  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let timeout = fp"{ctx.core_dir}/timeout.xsh"
  let tail = fp"{ctx.core_dir}/tail.xsh"
  let argv = [
    ctx.xsh_bin.display(),
    timeout.display(),
    ".2",
    ctx.xsh_bin.display(),
    tail.display(),
    "-f",
    "-s.01",
    f"--pid={sleeper.pid}",
    fifo.display(),
  ]
  let plan = process.command_argv(
    ctx.xsh_bin,
    argv,
    root,
    {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"},
    b"",
    stdout,
    stderr,
    timeout: 2s,
  )
  let status = process.run(plan)?

  assert status.exit_code()? == 124
  assert stdout.read_bytes()?.is_empty()
  assert stderr.read_text()? == ""
}

test test_tail_follow_with_a_dead_pid_exits_after_initial_output { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?
  fp"{root}/log".write(b"1\n2\n")

  let done = tail_run(ctx, root, ["-f", "--pid=2147483647", "log"])?
  assert done.status == 0
  assert done.stdout == b"1\n2\n"
}

test test_tail_follows_descriptor_appends_and_zero_count { |ctx|
  let root = test.temp_dir(ctx, name: "tail-follow")?
  let file = fp"{root}/log"
  let other = fp"{root}/other"
  file.write(b"initial\n")
  other.write(b"other initial\n")
  let timeout = fp"{ctx.core_dir}/timeout.xsh"
  let tail = fp"{ctx.core_dir}/tail.xsh"
  let cases: List[FollowCase] = [
    {args: ["-f", "-s.02", "log"], expected: b"initial\nlog appended\n"},
    {args: ["-q", "-n0", "-f", "-s.02", "log", "other"], expected: b"other appended\nlog appended\n"},
  ]

  for case in cases {
    let writer = spawn run sh -c "sleep 0.1; printf 'other appended\\n' >> \"$2\"; sleep 0.1; printf 'log appended\\n' >> \"$1\"" sh $file.display() $other.display() ?
    defer writer.cancel(signal: "KILL", kill_after: 0ms)?

    let stdout = fp"{root}/stdout"
    let stderr = fp"{root}/stderr"
    let argv = [ctx.xsh_bin.display(), timeout.display(), "-k", ".1", ".5", ctx.xsh_bin.display(), tail.display(), @case.args]
    let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, b"", stdout, stderr, timeout: 2s)
    let status = process.run(plan)?

    let exit_code = status.exit_code()?
    let output = stdout.read_bytes()?
    let diagnostic = stderr.read_text()?
    assert exit_code == 124, f"exit={exit_code} stderr={diagnostic}"
    assert output == case.expected
    assert diagnostic == ""
    assert (wait writer?).exited_with(0)
  }
}

test test_tail_follow_descriptor_reports_truncation_and_reads_from_start { |ctx|
  let root = test.temp_dir(ctx, name: "tail-descriptor-truncation")?
  let log = fp"{root}/log"
  log.write(b"1\n2\n3\n4\n5\n")
  let timeout = fp"{ctx.core_dir}/timeout.xsh"
  let tail = fp"{ctx.core_dir}/tail.xsh"
  let writer = spawn run sh -c "sleep 0.2; printf 'hi\\n' > \"$1\"" sh $log.display() ?
  defer writer.cancel(signal: "KILL", kill_after: 0ms)?

  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), timeout.display(), "-k", ".1", "1.2", ctx.xsh_bin.display(), tail.display(), "-f", "-s.02", "log"]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, b"", stdout, stderr, timeout: 3s)
  let status = process.run(plan)?

  assert status.exit_code()? == 124, f"stderr={stderr.read_text()?}"
  assert stdout.read_text()? == "1\n2\n3\n4\n5\nhi\n"
  assert stderr.read_text()? == "tail: log: file truncated\n", stderr.read_text()?
  assert (wait writer?).exited_with(0)
}

test test_tail_descriptor_retry_waits_for_a_missing_file { |ctx|
  let root = test.temp_dir(ctx, name: "tail-descriptor-retry")?
  let log = fp"{root}/log"
  let timeout = fp"{ctx.core_dir}/timeout.xsh"
  let tail = fp"{ctx.core_dir}/tail.xsh"
  let writer = spawn run sh -c "sleep 0.2; printf 'one\\n' > \"$1\"; sleep 0.2; printf 'two\\n' >> \"$1\"" sh $log.display() ?
  defer writer.cancel(signal: "KILL", kill_after: 0ms)?

  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), timeout.display(), "-k", ".1", "1.2", ctx.xsh_bin.display(), tail.display(), "-f", "--retry", "-s.02", "log"]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, b"", stdout, stderr, timeout: 3s)
  let status = process.run(plan)?

  assert status.exit_code()? == 124, f"stderr={stderr.read_text()?}"
  assert stdout.read_text()? == "one\ntwo\n"
  assert stderr.read_text()? == "tail: warning: --retry only effective for the initial open\ntail: cannot open 'log' for reading: No such file or directory\ntail: 'log' has appeared;  following new file\n", stderr.read_text()?
  assert (wait writer?).exited_with(0)
}

test test_tail_follow_name_banners_for_names_created_after_failed_opens { |ctx|
  let root = test.temp_dir(ctx, name: "tail-name-created-later")?
  let first = fp"{root}/log1"
  let second = fp"{root}/log2"
  let timeout = fp"{ctx.core_dir}/timeout.xsh"
  let tail = fp"{ctx.core_dir}/tail.xsh"
  let writer = spawn run sh -c "sleep 0.2; printf 'ping\\n' > \"$1\"; sleep 0.2; printf 'pong\\n' > \"$2\"" sh $first.display() $second.display() ?
  defer writer.cancel(signal: "KILL", kill_after: 0ms)?

  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), timeout.display(), "-k", ".1", "1.2", ctx.xsh_bin.display(), tail.display(), "-F", "-s.02", "log1", "log2"]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, b"", stdout, stderr, timeout: 3s)
  let status = process.run(plan)?

  assert status.exit_code()? == 124, f"stderr={stderr.read_text()?}"
  assert stdout.read_text()? == "\n==> log1 <==\nping\n\n==> log2 <==\npong\n", stdout.read_text()?
  assert stderr.read_text()? == "tail: cannot open 'log1' for reading: No such file or directory\ntail: cannot open 'log2' for reading: No such file or directory\ntail: 'log1' has appeared;  following new file\ntail: 'log2' has appeared;  following new file\n", stderr.read_text()?
  assert (wait writer?).exited_with(0)
}

test test_tail_validates_follow_options { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?

  assert tail_run(ctx, root, ["--pid=-1", "-f"])?.stderr == "tail: invalid PID: '-1'\n"
  assert tail_run(ctx, root, ["-s", "1.0s", "-"])?.stderr == "tail: invalid number of seconds: '1.0s'\nTry 'tail --help' for more information.\n"
  assert tail_run(ctx, root, ["--max-unchanged-stats=x", "-"])?.stderr == "tail: invalid maximum number of unchanged stats between opens: 'x'\n"
  assert tail_run(ctx, root, ["-s.1", "-"], b"a\n")?.stdout == b"a\n"

  let hint = tail_run(ctx, root, ["--follow=x", "-"])?
  assert hint.status == 1
  assert hint.stderr.starts_with(
    "tail: invalid argument 'x' for '--follow'\nValid arguments are:\n  - 'descriptor'\n  - 'name'\n",
  )

  let warned = tail_run(ctx, root, ["--retry", "--pid=1", "-"], b"a\n")?
  assert warned.stdout == b"a\n"
  assert warned.stderr == "tail: warning: --retry ignored; --retry is useful only when following\ntail: warning: PID ignored; --pid=PID is useful only when following\n", warned.stderr
}

test test_tail_getopt_diagnostics_and_help { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?

  let bad = tail_run(ctx, root, ["--definitely-invalid"])?
  assert bad.status == 1
  assert bad.stderr == "tail: unrecognized option '--definitely-invalid'\nTry 'tail --help' for more information.\n", bad.stderr

  assert tail_run(ctx, root, ["---presume-input-pipe", "-n1"], b"a\nb\n")?.stdout == b"b\n"
  assert "Usage: tail [OPTION]... [FILE]..." in tail_run(ctx, root, ["--help"])?.stdout as Str
  assert tail_run(ctx, root, ["--version"])?.stdout.starts_with(b"tail")
}

test test_tail_follow_name_switches_banners_between_files { |ctx|
  let root = test.temp_dir(ctx, name: "tail-follow-name")?
  let log = fp"{root}/log"
  let other = fp"{root}/other"
  log.write(b"initial\n")
  other.write(b"other initial\n")
  let timeout = fp"{ctx.core_dir}/timeout.xsh"
  let tail = fp"{ctx.core_dir}/tail.xsh"
  let writer = spawn run sh -c "sleep 0.1; printf 'other appended\\n' >> \"$2\"; sleep 0.1; printf 'log appended\\n' >> \"$1\"" sh $log.display() $other.display() ?
  defer writer.cancel(signal: "KILL", kill_after: 0ms)?

  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), timeout.display(), "-k", ".1", ".5", ctx.xsh_bin.display(), tail.display(), "-F", "-s.02", "log", "other"]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, b"", stdout, stderr, timeout: 2s)
  let status = process.run(plan)?

  assert status.exit_code()? == 124, f"stderr={stderr.read_text()?}"
  assert stdout.read_text()? == "==> log <==\ninitial\n\n==> other <==\nother initial\nother appended\n\n==> log <==\nlog appended\n"
  assert stderr.read_text()? == ""
}

type RemovalCase = {name: Str, extra: List[Str], stderr: Str}

# The directory holding a followed name is removed and recreated. GNU's inotify
# backend reports the removal once and then polls; with --disable-inotify it
# reports nothing beyond the polling messages.
test test_tail_follow_name_reports_removed_directory_once { |ctx|
  let cases: List[RemovalCase] = [
    {
      name: "tail-dir-removal",
      extra: [],
      stderr: "tail: 'd/f' has become inaccessible: No such file or directory\ntail: directory containing watched file was removed\ntail: inotify cannot be used, reverting to polling\ntail: 'd/f' has appeared;  following new file\n",
    },
    {
      name: "tail-dir-removal-polling",
      extra: ["---disable-inotify"],
      stderr: "tail: 'd/f' has become inaccessible: No such file or directory\ntail: 'd/f' has appeared;  following new file\n",
    },
  ]
  let timeout = fp"{ctx.core_dir}/timeout.xsh"
  let tail = fp"{ctx.core_dir}/tail.xsh"

  for case in cases {
    let root = test.temp_dir(ctx, name: case.name)?
    let dir = fp"{root}/d"
    let file = fp"{dir}/f"
    dir.mkdir()?
    file.write(b"foo\n")
    let writer = spawn run sh -c "sleep 0.2; rm -f \"$1\"; sleep 0.2; rmdir \"$2\"; sleep 0.2; mkdir \"$2\"; printf 'bar\\n' > \"$1\"" sh $file.display() $dir.display() ?
    defer writer.cancel(signal: "KILL", kill_after: 0ms)?

    let stdout = fp"{root}/stdout"
    let stderr = fp"{root}/stderr"
    let argv = [ctx.xsh_bin.display(), timeout.display(), "-k", ".1", "1.5", ctx.xsh_bin.display(), tail.display(), "-F", "-s.02", "--max-unchanged-stats=1", "d/f", @case.extra]
    let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, b"", stdout, stderr, timeout: 4s)
    let status = process.run(plan)?

    assert status.exit_code()? == 124, f"exit={status.exit_code()?} stderr={stderr.read_text()?}"
    assert stdout.read_text()? == "foo\nbar\n", case.name
    assert stderr.read_text()? == case.stderr, f"{case.name}: {stderr.read_text()?}"
    assert (wait writer?).exited_with(0)
  }
}

test test_tail_warns_that_retry_is_only_effective_for_the_initial_open { |ctx|
  let root = test.temp_dir(ctx, name: "tail-retry-warning")?
  fp"{root}/log".write(b"x\n")
  let timeout = fp"{ctx.core_dir}/timeout.xsh"
  let tail = fp"{ctx.core_dir}/tail.xsh"
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let argv = [ctx.xsh_bin.display(), timeout.display(), ".3", ctx.xsh_bin.display(), tail.display(), "--follow=descriptor", "--retry", "log"]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, b"", stdout, stderr, timeout: 2s)
  let status = process.run(plan)?

  assert status.exit_code()? == 124
  assert stdout.read_text()? == "x\n"
  assert stderr.read_text()? == "tail: warning: --retry only effective for the initial open\n", stderr.read_text()?
}

test test_tail_warning_write_failure_exits_with_status_one { |ctx|
  if ! p"/dev/full".exists() {
    test.skip("/dev/full is not available")
  }

  let root = test.temp_dir(ctx, name: "tail-warning-full")?
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/tail.xsh".display(), "--pid=0", "/dev/null"]
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, b"", fp"{root}/stdout", p"/dev/full")
  let status = process.run(plan)?

  assert status.exit_code()? == 1
}

type DebugCase = {args: List[Str], stderr: Str}

test test_tail_debug_reports_the_follow_implementation_like_gnu { |ctx|
  let root = test.temp_dir(ctx, name: "tail-debug")?
  fp"{root}/n20".write(numbers(1, 20))
  fs.mkfifo(fp"{root}/fifo", 0o600)

  let plain = tail_run(ctx, root, ["--debug", "n20"])?
  assert plain.status == 0
  assert plain.stdout == numbers(11, 20)
  assert plain.stderr == "", "--debug only reports when following"

  let stdin_pipe = tail_run(ctx, root, ["--debug", "-f", "--pid=2147483647"], b"a\n")?
  assert stdin_pipe.stdout == b"a\n"
  assert stdin_pipe.stderr == "", "a piped standard input is never followed, so nothing is reported"

  let by_descriptor = tail_run(ctx, root, ["--debug", "-f", "--pid=2147483647", "n20"])?
  assert by_descriptor.stdout == numbers(11, 20)
  assert by_descriptor.stderr == "tail: using notification mode\n", by_descriptor.stderr

  let by_name = tail_run(ctx, root, ["--debug", "-F", "--pid=2147483647", "n20"])?
  assert by_name.stderr == "tail: using notification mode\n", by_name.stderr

  let fifo = tail_run(ctx, root, ["--debug", "-f", "--pid=2147483647", "fifo"])?
  assert fifo.status == 0
  assert fifo.stderr == "tail: using notification mode\n", fifo.stderr

  let disabled = tail_run(ctx, root, ["---disable-inotify", "--debug", "-f", "--pid=2147483647", "n20"])?
  assert disabled.stderr == "tail: using polling mode\n", disabled.stderr

  let missing = tail_run(ctx, root, ["--debug", "-f", "--pid=2147483647", "missing"])?
  assert missing.status == 1
  assert missing.stderr == "tail: cannot open 'missing' for reading: No such file or directory\ntail: using polling mode\ntail: no files remaining\n", missing.stderr

  let device = tail_run(ctx, root, ["--debug", "-f", "--pid=2147483647", "/dev/null"])?
  assert device.status == 0
  assert device.stderr == "tail: using polling mode\n", device.stderr

  let named_device = tail_run(ctx, root, ["--debug", "--follow=name", "--pid=2147483647", "/dev/null"])?
  assert named_device.stderr == "tail: using polling mode\n", named_device.stderr
}

test test_tail_debug_reports_blocking_and_polling_while_following { |ctx|
  let root = test.temp_dir(ctx, name: "tail-debug-follow")?
  fp"{root}/n20".write(numbers(1, 20))
  let timeout = fp"{ctx.core_dir}/timeout.xsh"
  let tail = fp"{ctx.core_dir}/tail.xsh"

  let cases: List[DebugCase] = [
    {args: ["--debug", "-f", "/dev/null"], stderr: "tail: using blocking mode\n"},
    {args: ["---disable-inotify", "--debug", "-f", "/dev/null"], stderr: "tail: using blocking mode\n"},
    {args: ["--debug", "-f", "n20", "/dev/null"], stderr: "tail: using polling mode\n"},
  ]

  for case in cases {
    let stdout = fp"{root}/stdout"
    let stderr = fp"{root}/stderr"
    let argv = [ctx.xsh_bin.display(), timeout.display(), ".2", ctx.xsh_bin.display(), tail.display()].extend(case.args)
    let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, b"", stdout, stderr, timeout: 2s)
    let status = process.run(plan)?

    assert status.exit_code()? == 124, f"exit={status.exit_code()?} stderr={stderr.read_text()?}"
    assert stderr.read_text()? == case.stderr, stderr.read_text()?
  }

  let help = tail_run(ctx, root, ["--help"])?.stdout as Str
  assert "      --debug       indicate which --follow implementation is used\n" in help
}
