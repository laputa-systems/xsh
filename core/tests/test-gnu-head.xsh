use support.uu

# The shell owns shared descriptors, pipes, and resource limits. The applet
# launch remains identical when these tests run against the reference tool.
proc wrapped(s: uu.Scene, args: List[Str], setup: Str, timeout: Duration = 10s) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let launch = uu.argv(s, "head", [Path(arg) for arg in args])?
  let words = [p"/bin/sh", p"-c", Path(setup), p"head-boundary"].extend(launch)
  let out = uu.at(s, ".wrapped-out")
  let err = uu.at(s, ".wrapped-err")
  let status = process.run(process.command_argv(p"/bin/sh", words, s.root, {LC_ALL: "C", TZ: "UTC"}, b"", out, err, timeout: timeout))?
  Ok({util: "head", args: args, status: status.shell_code()?, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

proc file_case(s: uu.Scene, args: List[Str], input: Bytes, expected: Bytes) [fs, process, env, error] {
  uu.write_bytes(s, "input", input)?
  let r = uu.invoke(s, "head", args.extend(["input"]))?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, expected)
}

proc all_inputs(s: uu.Scene, args: List[Str], input: Bytes, expected: Bytes) [fs, process, env, error] {
  file_case(s, args, input, expected)
  let redirected = uu.invoke_from_path(s, "head", args, uu.at(s, "input"))?
  uu.succeeds(redirected)
  uu.stdout_only_bytes(redirected, expected)
  let piped = wrapped(s, args, "cat input | \"$@\"")?
  uu.succeeds(piped)
  uu.stdout_only_bytes(piped, expected)
}

# Find startup's memory requirement separately from the eight-megabyte allowance
# for processing data. A script runtime can need a larger startup budget than
# the reference executable while still requiring bounded tail storage.
proc memory_limit(s: uu.Scene) [fs, process, env, error] -> Result[Int, Error] {
  var working = 0
  for limit in [5000 * n for n in range(1, 11)].extend([50000 * n for n in range(2, 21)]) {
    let r = wrapped(s, ["-c-1", "/dev/null"], f"ulimit -c 0; ulimit -v {limit} || exit; exec \"$@\"")?
    if r.status == 0 { working = limit; break }
  }
  assert working != 0, "head startup did not fit a one-gigabyte virtual-memory limit"
  var previous = working
  for reduction in range(1, working / 1000) {
    let candidate = working - reduction * 1000
    let r = wrapped(s, ["-c-1", "/dev/null"], f"ulimit -c 0; ulimit -v {candidate} || exit; exec \"$@\"")?
    if r.status != 0 { return Ok(previous + 4) }
    previous = candidate
  }
  Ok(previous + 4)
}

# origin: gnu head/head-c.log
test test_gnu_head_head_c_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "abc\n")?
  let sequential = wrapped(s, ["-c1"], "exec 3<input; \"$@\" <&3 || exit; \"$@\" <&3")?
  uu.succeeds(sequential)
  uu.stdout_only_bytes(sequential, b"ab")
  uu.write(s, "input", "abc\ndef\n")?
  let positioned = wrapped(s, ["-c-4"], "exec 3<input; dd bs=1 skip=1 count=0 <&3 2>/dev/null || exit; exec \"$@\" <&3")?
  uu.succeeds(positioned)
  uu.stdout_only_bytes(positioned, b"bc\n")
  let budget = memory_limit(s)? + 8000
  for count in ["9223372036854775807", "18446744073709551616000"] {
    let r = wrapped(s, [f"--bytes=-{count}"], f"ulimit -c 0; ulimit -v {budget} || exit; exec \"$@\" </dev/null")?
    uu.succeeds(r)
    uu.no_output(r)
  }
  for source in [p"/proc/version", p"/sys/kernel/profiling"] {
    if !source.exists()? { continue }
    let content = source.read_bytes()?
    uu.write_bytes(s, "copy", content)?
    let regular = uu.invoke(s, "head", ["-c", "-1", "copy"])?
    let virtual = uu.invoke(s, "head", ["-c", "-1", source.display()])?
    uu.succeeds(regular)
    uu.succeeds(virtual)
    assert regular.stdout == virtual.stdout
    uu.no_stderr(virtual)
  }
}

# origin: gnu head/head-elide-tail.log
test test_gnu_head_head_elide_tail_log { |ctx|
  test.timeout(ctx, 5m)
  let s = uu.scene(ctx)?
  let first_boundary = ["a" for _ in range(0, 8192 - 3)].join("")
  let second_boundary = ["a" for _ in range(0, 2 * 8192 - 2)].join("")
  for case in [
    {option: "--bytes=-2", input: b"a\n", output: b""},
    {option: "--bytes=-2", input: b"a", output: b""},
    {option: "--bytes=-2", input: b"abc", output: b"a"},
    {option: "--bytes=-2", input: bytes.from_text(f"{first_boundary}\nbcd"), output: bytes.from_text(f"{first_boundary}\nb")},
    {option: "--bytes=-2", input: bytes.from_text(f"{second_boundary}bcd"), output: bytes.from_text(f"{second_boundary}b")},
    {option: "--lines=-1", input: b"", output: b""},
    {option: "--lines=-1", input: b"a\n", output: b""},
    {option: "--lines=-1", input: b"a", output: b""},
    {option: "--lines=-1", input: b"a\nb", output: b"a\n"},
    {option: "--lines=-1", input: b"a\nb\n", output: b"a\n"},
    {option: "--lines=-0", input: b"a\nb\n", output: b"a\nb\n"},
    {option: "--lines=-0", input: b"a\nb", output: b"a\nb"},
  ] { file_case(s, [case.option], case.input, case.output) }
  let alphabet = b"abcdefghijklmnopqrst"
  for size in range(0, 21) {
    let input = alphabet[..size]
    for elided in range(0, 21) {
      let retained = if elided < size { size - elided } else { 0 }
      for options in [[], ["---presume-input-pipe"]] {
        file_case(s, [f"--bytes=-{elided}"].extend(options), input, input[..retained])
      }
    }
  }
  let lines = b"a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\nm\nn\no\np\nq\nr\ns\nt\nu"
  for size in range(0, 22) {
    let length = if 2 * size > lines.len() { lines.len() } else { 2 * size }
    let input = lines[..length]
    for elided in range(0, 22) {
      let retained = if elided < size { size - elided } else { 0 }
      let output_length = if 2 * retained > input.len() { input.len() } else { 2 * retained }
      for options in [[], ["---presume-input-pipe"]] {
        file_case(s, [f"--lines=-{elided}"].extend(options), input, input[..output_length])
      }
    }
  }
}

# origin: gnu head/head-n0.log
test test_gnu_head_head_n0_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "dir")?
  uu.write(s, "file", "a\n")?
  for name in ["dir", "file"] {
    for option in ["-n", "-c"] {
      let r = uu.invoke(s, "head", [option, "0", name])?
      uu.succeeds(r)
      uu.no_output(r)
    }
  }
  for names in [["dir", "file"], ["file", "dir"]] {
    for option in ["-n", "-c"] {
      let quiet = uu.invoke(s, "head", ["-q", option, "0"].extend(names))?
      uu.succeeds(quiet)
      uu.no_output(quiet)
      let headers = uu.invoke(s, "head", [option, "0"].extend(names))?
      uu.succeeds(headers)
      uu.stdout_only(headers, f"==> {names[0]} <==\n\n==> {names[1]} <==\n")
    }
  }
  for missing in [["missing1"], ["missing1", "missing2"]] {
    let diagnostic = [f"head: cannot open '{name}' for reading: No such file or directory\n" for name in missing].join("")
    for option in ["-n", "-c"] {
      let r = uu.invoke(s, "head", [option, "0"].extend(missing))?
      uu.fails_with_code(r, 1)
      uu.stderr_only(r, diagnostic)
    }
  }
}

# origin: gnu head/head-pos.log
test test_gnu_head_head_pos_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "input", "a\nb\n")?
  for count in ["-1", "1"] {
    let r = wrapped(s, ["-n", count], "exec 3<input; \"$@\" <&3 >/dev/null || exit; cat <&3")?
    uu.succeeds(r)
    uu.stdout_only_bytes(r, b"b\n")
  }
  uu.write(s, "input", [f"{n}\n" for n in range(1, 70001)].join(""))?
  let large = wrapped(s, ["-n-50000"], "exec 3<input; \"$@\" <&3 >/dev/null || exit; wc -l <&3")?
  uu.succeeds(large)
  uu.stdout_only_bytes(large, b"50000\n")
}

# origin: gnu head/head-write-error.log
test test_gnu_head_head_write_error_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write_bytes(s, "bigseek", bytes.concat([b"y\n" for _ in range(0, 5 * 1024 * 1024)]))?
  let diagnostic = "head: error writing 'standard output': No space left on device\n"
  for unit in ["lines", "bytes"] {
    for count in [0, 1] {
      let piped = wrapped(s, [f"--{unit}=-{count}"], "yes | \"$@\" >/dev/full")?
      uu.fails_with_code(piped, 1)
      uu.stderr_only(piped, diagnostic)
      let seekable = uu.invoke(s, "head", [f"--{unit}=-{count}", "bigseek"], stdout: p"/dev/full", timeout: 10s)?
      uu.fails_with_code(seekable, 1)
      uu.stderr_only(seekable, diagnostic)
    }
  }
}

# origin: gnu head/head.log
test test_gnu_head_head_log { |ctx|
  let s = uu.scene(ctx)?
  for input in [b"", b"a", b"\n", b"a\n"] { all_inputs(s, [], input, input) }
  let nine = b"1\n2\n3\n4\n5\n6\n7\n8\n9\n"
  let ten = bytes.concat([nine, b"0\n"])
  all_inputs(s, [], ten, ten)
  all_inputs(s, [], nine, nine)
  all_inputs(s, [], bytes.concat([ten, b"b\n"]), ten)
  all_inputs(s, ["-1"], b"1\n2\n", b"1\n")
  all_inputs(s, ["-1c"], b"", b"")
  all_inputs(s, ["-1c"], b"12", b"1")
  all_inputs(s, ["-14c"], b"1234567890abcdefg", b"1234567890abcd")
  let numbers = bytes.from_text([f"{n}\n" for n in range(0, 601)].join(""))
  for option in ["-2b", "-1k"] { all_inputs(s, [option], numbers, numbers[..1024]) }
  all_inputs(s, ["-n", "2048m"], b"a\n", b"a\n")
  all_inputs(s, [], b"a\0a\n", b"a\0a\n")
  let newlines = b"\n\n\n\n\n\n\n\n\n\n\n\n"
  for options in [["-08"], ["-010"], ["-n", "08"], ["-c", "08"]] {
    let count = if options == ["-010"] { 10 } else { 8 }
    all_inputs(s, options, newlines, newlines[..count])
  }
  all_inputs(s, ["-z", "-n", "1"], b"x\0y", b"x\0")
  all_inputs(s, ["-z", "-n", "2"], b"x\0y", b"x\0y")
}

# origin: gnu head/quote-headers.log
test test_gnu_head_quote_headers_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "\n")?
  uu.touch(s, "normal")?
  let r = uu.invoke(s, "head", ["-n1", "\n", "normal"])?
  uu.succeeds(r)
  uu.stdout_only(r, "==> ''$'\\n' <==\n\n==> normal <==\n")
}
