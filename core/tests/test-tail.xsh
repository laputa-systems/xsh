type Ran = {status: Int, stdout: Bytes, stderr: Str}

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
  assert digits.stderr == "tail: option used in invalid context -- 5\nTry 'tail --help' for more information.\n", digits.stderr

  let plus = tail_run(ctx, root, ["+cl"])?
  assert plus.stderr == "tail: cannot open '+cl' for reading: No such file or directory\n", plus.stderr
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

test test_tail_follow_of_a_live_file_is_rejected_unless_the_pid_is_gone { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?
  fp"{root}/log".write(b"1\n2\n")

  let live = tail_run(ctx, root, ["-f", "log"])?
  assert live.status == 1
  assert live.stdout == b"1\n2\n"
  assert "cannot follow 'log'" in live.stderr

  let done = tail_run(ctx, root, ["-f", "--pid=2147483647", "log"])?
  assert done.status == 0
  assert done.stdout == b"1\n2\n"
}

test test_tail_validates_follow_options { |ctx|
  let root = test.temp_dir(ctx, name: "tail")?

  assert tail_run(ctx, root, ["--pid=-1", "-f"])?.stderr == "tail: invalid PID: '-1'\n"
  assert tail_run(ctx, root, ["-s", "1.0s", "-"])?.stderr == "tail: invalid number of seconds: '1.0s'\n"
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
