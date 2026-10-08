type Ran = {status: Int, stdout: Bytes, stderr: Str}

# Runs core/split.xsh by its real path inside `root`, capturing both streams.
proc split_run(ctx: TestContext, root: Path, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let out = fp"{root}/.out"
  let err = fp"{root}/.err"
  let argv = [ctx.xsh_bin.display(), fp"{ctx.core_dir}/split.xsh".display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {XSH_EXECUTION_PHRASE: "", LC_ALL: "C"}, input, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?, stderr: err.read_text()?})
}

proc piece(root: Path, name: Str) [fs, error] -> Result[Bytes] {
  fp"{root}/{name}".read_bytes()?
}

test test_split_lines_bytes_and_line_bytes { |ctx|
  let root = test.temp_dir(ctx, name: "split")?
  fp"{root}/five".write("1\n2\n3\n4\n5\n")

  assert split_run(ctx, root, ["-l", "2", "five"])?.status == 0
  assert piece(root, "xaa")? == b"1\n2\n"
  assert piece(root, "xab")? == b"3\n4\n"
  assert piece(root, "xac")? == b"5\n"
  assert ! fp"{root}/xad".exists()?

  assert split_run(ctx, root, ["-b", "3", "five", "b-"])?.status == 0
  assert piece(root, "b-aa")? == b"1\n2"
  assert piece(root, "b-ab")? == b"\n3\n"

  assert split_run(ctx, root, ["-C", "3"], b"1\n2222\n3\n4")?.status == 0
  assert piece(root, "xaa")? == b"1\n"
  assert piece(root, "xab")? == b"222"
  assert piece(root, "xac")? == b"2\n"
  assert piece(root, "xad")? == b"3\n"
  assert piece(root, "xae")? == b"4"
}

test test_split_obsolete_line_count_and_conflicts { |ctx|
  let root = test.temp_dir(ctx, name: "split")?
  fp"{root}/five".write("1\n2\n3\n4\n5\n")

  assert split_run(ctx, root, ["-2", "five", "o-"])?.status == 0
  assert piece(root, "o-aa")? == b"1\n2\n"
  assert split_run(ctx, root, ["-d2", "five", "d-"])?.status == 0
  assert piece(root, "d-00")? == b"1\n2\n", "digits inside a cluster are the line count"

  let both = split_run(ctx, root, ["-l", "2", "-2", "five"])?
  assert both.status == 1
  assert both.stderr == "split: cannot split in more than one way\n", both.stderr

  let value = split_run(ctx, root, ["--lines", "-200", "five"])?
  assert value.stderr == "split: invalid number of lines: '-200'\n", value.stderr
}

test test_split_suffix_styles_and_widening { |ctx|
  let root = test.temp_dir(ctx, name: "split")?
  fp"{root}/abc".write("abc")

  assert split_run(ctx, root, ["-b", "1", "-d", "abc", "n"])?.status == 0
  assert piece(root, "n00")? == b"a"
  assert split_run(ctx, root, ["-b", "1", "-d", "--hex-suffixes=a", "abc", "h"])?.status == 0
  assert piece(root, "h0a")? == b"a", "the last suffix option wins and --hex-suffixes=N starts at N"
  assert piece(root, "h0c")? == b"c"
  assert split_run(ctx, root, ["-b", "1", "-a", "3", "--additional-suffix=.txt", "abc", "s"])?.status == 0
  assert piece(root, "saaa.txt")? == b"a"

  let long = bytes.concat([b"a" for _ in range(651)])
  assert split_run(ctx, root, ["-b", "1", "-"], long)?.status == 0
  assert piece(root, "xyz")? == b"a"
  assert piece(root, "xzaaa")? == b"a", "the suffix widens after xyz"

  let exhausted = split_run(ctx, root, ["-b", "1", "-a", "1", "-"], b"abcdefghijklmnopqrstuvwxyz0")?
  assert exhausted.status == 1
  assert exhausted.stderr == "split: output file suffixes exhausted\n", exhausted.stderr
}

test test_split_number_chunks { |ctx|
  let root = test.temp_dir(ctx, name: "split")?
  fp"{root}/az".write("abcdefghijklmnopqrstuvwxyz\n")
  fp"{root}/five".write("1\n2\n3\n4\n5\n")

  assert split_run(ctx, root, ["-n", "5", "az", "c-"])?.status == 0
  assert piece(root, "c-aa")? == b"abcdef"
  assert piece(root, "c-ae")? == b"wxyz\n"

  assert split_run(ctx, root, ["-n", "3/5", "az"])?.stdout == b"mnopq"
  assert split_run(ctx, root, ["-n", "l/2", "five", "l-"])?.status == 0
  assert piece(root, "l-aa")? == b"1\n2\n3\n"
  assert piece(root, "l-ab")? == b"4\n5\n"
  assert split_run(ctx, root, ["-n", "r/2", "five", "r-"])?.status == 0
  assert piece(root, "r-aa")? == b"1\n3\n5\n"
  assert split_run(ctx, root, ["-n", "r/2/3", "five"])?.stdout == b"2\n5\n"
  assert split_run(ctx, root, ["-e", "-n", "7", "-", "e-"], b"abc")?.status == 0
  assert ! fp"{root}/e-ad".exists()?, "-e drops empty chunks"

  let bad = split_run(ctx, root, ["-n", "10/5", "az"])?
  assert bad.stderr == "split: invalid chunk number: '10'\n", bad.stderr
  let zero = split_run(ctx, root, ["-n", "l/0", "az"])?
  assert zero.stderr == "split: invalid number of chunks: '0'\n", zero.stderr
  let width = split_run(ctx, root, ["-n", "100", "-a", "1", "az"])?
  assert width.stderr == "split: the suffix length needs to be at least 2\n", width.stderr
}

test test_split_separator_verbose_filter_and_errors { |ctx|
  let root = test.temp_dir(ctx, name: "split")?

  assert split_run(ctx, root, ["--lines=2", "-t", ";", "-", "t-"], b"1;2;3;4;5;")?.status == 0
  assert piece(root, "t-aa")? == b"1;2;"
  assert piece(root, "t-ac")? == b"5;"

  let verbose = split_run(ctx, root, ["-b", "2", "--verbose", "-", "v-"], b"abcd")?
  assert verbose.stdout == b"creating file 'v-aa'\ncreating file 'v-ab'\n"

  assert split_run(ctx, root, ["--filter=cat > $FILE.out", "-l", "1", "-", "f-"], b"x\ny\n")?.status == 0
  assert piece(root, "f-aa.out")? == b"x\n"
  assert split_run(ctx, root, ["--filter=exit 3", "-"], b"x\n")?.status == 1

  assert split_run(ctx, root, ["-b", "2", "--filter=cat > $FILE.out", "-", "b-"], b"abcd")?.status == 0
  assert piece(root, "b-aa.out")? == b"ab"
  assert piece(root, "b-ab.out")? == b"cd"

  let missing = split_run(ctx, root, ["nosuch"])?
  assert missing.status == 1
  assert missing.stderr == "split: cannot open 'nosuch' for reading: No such file or directory\n", missing.stderr

  let separator = split_run(ctx, root, ["--separator=xx", "-"], b"a")?
  assert separator.stderr == "split: multi-character separator 'xx'\n", separator.stderr
  let invalid = split_run(ctx, root, ["-b", "1024W", "-"], b"a")?
  assert invalid.stderr == "split: invalid number of bytes: '1024W'\n", invalid.stderr

  fp"{root}/xaa".write("keep")
  assert split_run(ctx, root, [], b"")?.status == 0
  assert piece(root, "xaa")? == b"keep", "empty input creates no output and leaves existing files alone"
}

test test_split_filter_round_robin_stops_when_filters_exit { |ctx|
  let root = test.temp_dir(ctx, name: "split-filter-round-robin")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let timeout = fp"{ctx.core_dir}/timeout.xsh"
  let split = fp"{ctx.core_dir}/split.xsh"
  let command = r"""yes | "$1" "$2" 1 "$1" "$3" --filter='head -c1 >$FILE.out' -n r/2 -"""
  let plan = process.command_argv(
    p"/bin/sh",
    ["sh", "-c", command, "split-filter-round-robin", ctx.xsh_bin.display(), timeout.display(), split.display()],
    root,
    {LC_ALL: "C", TMPDIR: root, XSH_EXECUTION_PHRASE: ""},
    b"",
    stdout,
    stderr,
    timeout: 3s,
  )
  let status = process.run(plan)?

  assert status.exited_with(0), stderr.read_text()?
  assert piece(root, "xaa.out")? == b"y"
  assert piece(root, "xab.out")? == b"y"
}

test test_split_filter_byte_chunks_keep_processing_until_timeout { |ctx|
  let root = test.temp_dir(ctx, name: "split-filter-byte-chunks")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let timeout = fp"{ctx.core_dir}/timeout.xsh"
  let split = fp"{ctx.core_dir}/split.xsh"
  let command = r"""yes | "$1" "$2" -k .2 .5 "$1" "$3" -b 1000 --filter='printf x >>$FILE.started; sleep .02; head -c1 >/dev/null' -"""
  let plan = process.command_argv(
    p"/bin/sh",
    ["sh", "-c", command, "split-filter-byte-chunks", ctx.xsh_bin.display(), timeout.display(), split.display()],
    root,
    {LC_ALL: "C", TMPDIR: root, XSH_EXECUTION_PHRASE: ""},
    b"",
    stdout,
    stderr,
    timeout: 2s,
  )
  let status = process.run(plan)?

  assert status.exited_with(124), stderr.read_text()?
  assert stderr.read_text()? == "", stderr.read_text()?
  assert fp"{root}/xaa.started".exists()?
  assert fp"{root}/xab.started".exists()?
}

test test_split_non_utf8_input_path { |ctx|
  let root = test.temp_dir(ctx, name: "split-raw-input")?
  let input_name = b"input-\xff"
  let input = Path.parse_bytes(bytes.concat([root.bytes(), b"/", input_name]))?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let command = r"""name=$(printf 'input-\377'); exec "$1" "$2" "$name" """
  input.write("A\nB\n")

  let status = process.run(process.command_argv(
    p"/bin/sh",
    ["sh", "-c", command, "split-raw-input", ctx.xsh_bin.display(), fp"{ctx.core_dir}/split.xsh".display()],
    root,
    {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""},
    b"",
    stdout,
    stderr,
    timeout: 5s,
  ))?

  assert status.exited_with(0), stderr.read_text()?
  assert piece(root, "xaa")? == b"A\nB\n"
}

test test_split_non_utf8_prefix_and_additional_suffix { |ctx|
  let root = test.temp_dir(ctx, name: "split-raw-names")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let command = r"""prefix=$(printf 'p\377'); suffix=$(printf '\376'); exec "$1" "$2" -b 1 --additional-suffix "$suffix" input.txt "$prefix" """
  fp"{root}/input.txt".write("AB")

  let status = process.run(process.command_argv(
    p"/bin/sh",
    ["sh", "-c", command, "split-raw-names", ctx.xsh_bin.display(), fp"{ctx.core_dir}/split.xsh".display()],
    root,
    {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""},
    b"",
    stdout,
    stderr,
    timeout: 5s,
  ))?
  let first = Path.parse_bytes(bytes.concat([root.bytes(), b"/p\xffaa\xfe"]))?
  let second = Path.parse_bytes(bytes.concat([root.bytes(), b"/p\xffab\xfe"]))?

  assert status.exited_with(0), stderr.read_text()?
  assert first.read_bytes()? == b"A"
  assert second.read_bytes()? == b"B"
}

test test_split_missing_separator_and_invalid_obsolete_cluster { |ctx|
  let root = test.temp_dir(ctx, name: "split-options")?

  let missing = split_run(ctx, root, ["-t"], b"a\n")?
  assert missing.stderr.find("a value is required for '--separator <SEP>'") != null, missing.stderr
  assert missing.status == 1, missing.stderr

  let literal = split_run(ctx, root, ["--", "--", "-t"])?
  assert literal.stderr.find("cannot open '-t'") != null, literal.stderr

  let invalid = split_run(ctx, root, ["-2fb", "input"], b"a\n")?
  assert invalid.stderr.find("unexpected argument '-f' found") != null, invalid.stderr
  assert invalid.status == 1, invalid.stderr
}

test test_split_numbered_virtual_input_is_rejected_before_read { |ctx|
  let root = test.temp_dir(ctx, name: "split-device")?
  let stdout = fp"{root}/stdout"
  let stderr = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/split.xsh"
  for number in ["3", "l/3"] {
    # Bound both allocation and time if size validation regresses. The cap
    # leaves room for the debug interpreter's preparation stack.
    let status = process.run(process.command_argv(p"/bin/sh",
      ["sh", "-c", "ulimit -v 524288; exec \"$@\"", "split-device-probe",
        ctx.xsh_bin.display(), "--", script.display(), "-n", number, "/dev/zero"],
      root, {LC_ALL: "C"}, b"", stdout, stderr, timeout: 3s))?
    assert status.exited_with(1), "numbered virtual input must fail before its endless byte stream is read"
    assert stderr.read_text()? == "split: /dev/zero: cannot determine file size\n"
    assert ! fp"{root}/xaa".exists()?
  }
}
