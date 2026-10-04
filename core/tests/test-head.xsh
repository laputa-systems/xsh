type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs core/head.xsh by its real path inside `root`, capturing both streams.
proc head_run(ctx: TestContext, root: Path, args: List[Str], input: Bytes = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/head.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

test test_head_lines_bytes_and_obsolete_counts { |ctx|
  let root = test.temp_dir(ctx, name: "head")?
  let twenty = bytes.concat([bytes.from_text(f"{n}\n") for n in range(1, 21)])
  fp"{root}/n20".write(twenty)?

  assert head_run(ctx, root, ["n20"])?.stdout == bytes.concat([bytes.from_text(f"{n}\n") for n in range(1, 11)])
  assert head_run(ctx, root, ["-n2", "n20"])?.stdout == b"1\n2\n"
  assert head_run(ctx, root, ["--lines=1", "n20"])?.stdout == b"1\n"
  assert head_run(ctx, root, ["-3"], twenty)?.stdout == b"1\n2\n3\n"
  assert head_run(ctx, root, ["-c", "5", "n20"])?.stdout == b"1\n2\n3"
  assert head_run(ctx, root, ["-1c"], b"abc")?.stdout == b"a"
  assert head_run(ctx, root, ["-n", "3", "-c", "2", "n20"])?.stdout == b"1\n", "the last of -n and -c wins"
  assert head_run(ctx, root, ["-n", "0", "n20"])?.stdout == b""
}

test test_head_elides_the_tail_with_a_leading_minus { |ctx|
  let root = test.temp_dir(ctx, name: "head")?

  assert head_run(ctx, root, ["-n", "-1"], b"x\ny")?.stdout == b"x\n"
  assert head_run(ctx, root, ["-n", "-1"], b"x\ny\n")?.stdout == b"x\n"
  assert head_run(ctx, root, ["-c", "-3"], b"abcdefgh")?.stdout == b"abcde"
  assert head_run(ctx, root, ["-c", "-20"], b"abc")?.stdout == b""
  assert head_run(ctx, root, ["--lines=-0"], b"a\nb")?.stdout == b"a\nb"
  assert head_run(ctx, root, ["--bytes=-0"], b"qwerty")?.stdout == b"qwerty"
  assert head_run(ctx, root, ["-n", "-116265256266241262252526"], b"a\n")?.stdout == b""
}

test test_head_counts_accept_gnu_suffixes_and_reject_others { |ctx|
  let root = test.temp_dir(ctx, name: "head")?

  assert head_run(ctx, root, ["-n", "2048m"], b"a\n")?.stdout == b"a\n"
  assert head_run(ctx, root, ["-c", "1b"], bytes.concat([b"x" for _ in range(600)]))?.stdout.len() == 512
  assert head_run(ctx, root, ["-c", "1kB"], bytes.concat([b"x" for _ in range(1100)]))?.stdout.len() == 1000
  assert head_run(ctx, root, ["-c", "+5"], b"abcdefgh")?.stdout == b"abcde"

  for suffix in ["g", "t", "R2", "1fb"] {
    let result = head_run(ctx, root, ["-c", f"2{suffix}"], b"x")?
    assert result.status == 1, suffix
    assert result.stderr == f"head: invalid number of bytes: '2{suffix}'\n", result.stderr
  }

  let lines = head_run(ctx, root, ["-n", "00x"])?
  assert lines.stderr == "head: invalid number of lines: '00x'\n", lines.stderr
}

test test_head_zero_terminated_lines { |ctx|
  let root = test.temp_dir(ctx, name: "head")?

  assert head_run(ctx, root, ["-z", "-n", "1"], b"x\0y")?.stdout == b"x\0"
  assert head_run(ctx, root, ["-z", "-n", "2"], b"x\0y")?.stdout == b"x\0y"
  assert head_run(ctx, root, ["-z", "-n", "-1"], b"x\0y\0z\0")?.stdout == b"x\0y\0"
  assert head_run(ctx, root, ["-5zv"], b"1\0002\0003\0004\0005\0006")?.stdout == b"==> 'standard input' <==\n1\0002\0003\0004\0005\0"
}

test test_head_keeps_non_utf8_bytes_and_crlf { |ctx|
  let root = test.temp_dir(ctx, name: "head")?
  let input = b"\xfc\x80\xaf\r\nb\xff\r\nc"

  assert head_run(ctx, root, ["-n", "2"], input)?.stdout == b"\xfc\x80\xaf\r\nb\xff\r\n"
  assert head_run(ctx, root, ["-c", "6"], input)?.stdout == b"\xfc\x80\xaf\r\nb"
  assert head_run(ctx, root, ["-n", "-1"], input)?.stdout == b"\xfc\x80\xaf\r\nb\xff\r\n"
}

test test_head_headers_quote_names_and_follow_the_open_result { |ctx|
  let root = test.temp_dir(ctx, name: "head")?
  fp"{root}/plain".write(b"p\n")?
  fp"{root}/two words".write(b"w\n")?
  fp"{root}/dir".mkdir()?

  assert head_run(ctx, root, ["-n1", "plain", "two words"])?.stdout == b"==> plain <==\np\n\n==> 'two words' <==\nw\n"
  assert head_run(ctx, root, ["-q", "plain", "plain"])?.stdout == b"p\np\n"
  assert head_run(ctx, root, ["-v", "plain"])?.stdout == b"==> plain <==\np\n"
  assert head_run(ctx, root, ["-v", "-q", "plain", "plain"])?.stdout == b"p\np\n", "the last of -q and -v wins"
  assert head_run(ctx, root, ["plain", "-", "plain"], b"s\n")?.stdout == b"==> plain <==\np\n\n==> 'standard input' <==\ns\n\n==> plain <==\np\n"

  let result = head_run(ctx, root, ["-c", "5", "dir", "plain", "missing"])?
  assert result.status == 1
  assert result.stdout == b"==> dir <==\n\n==> plain <==\np\n"
  assert result.stderr == "head: error reading 'dir': Is a directory\nhead: cannot open 'missing' for reading: No such file or directory\n", result.stderr
}

test test_head_zero_count_reads_nothing { |ctx|
  let root = test.temp_dir(ctx, name: "head")?
  fp"{root}/dir".mkdir()?

  assert head_run(ctx, root, ["-c", "0", "dir"])?.status == 0
  assert head_run(ctx, root, ["-n", "0", "dir"])?.stdout == b""
  assert head_run(ctx, root, ["-c", "0", "dir", "dir"])?.stdout == b"==> dir <==\n\n==> dir <==\n"
}

test test_head_getopt_diagnostics_and_help { |ctx|
  let root = test.temp_dir(ctx, name: "head")?

  let bad = head_run(ctx, root, ["--definitely-invalid"])?
  assert bad.status == 1
  assert bad.stderr == "head: unrecognized option '--definitely-invalid'\nTry 'head --help' for more information.\n", bad.stderr

  let late = head_run(ctx, root, ["-n", "1", "-5"])?
  assert late.stderr == "head: invalid option -- '5'\nTry 'head --help' for more information.\n", late.stderr

  assert head_run(ctx, root, ["---presume-input-pipe", "-n1"], b"a\nb\n")?.stdout == b"a\n"
  assert "Usage: head [OPTION]... [FILE]..." in head_run(ctx, root, ["--help"])?.stdout.utf8()?
  assert head_run(ctx, root, ["--version"])?.stdout.starts_with(b"head")
}
