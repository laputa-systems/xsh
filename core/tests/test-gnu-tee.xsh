use support.uu as uu

proc wrapped(s: uu.Scene, code: Str, words: List[Path], stdin: Path? = null, input: Bytes = b"", timeout: Duration = 20s) [fs, process, env, error] -> uu.Ran {
  let out = uu.at(s, "wrapped-out")
  let err = uu.at(s, "wrapped-err")
  let argv = [p"/bin/sh", p"-c", Path(code), p"tee-wrapper"].extend(words)
  let plan = if let source = stdin {
    process.command_argv(p"/bin/sh", argv, s.root, {LC_ALL: "C"}, source, out, err, timeout: timeout)
  } else {
    process.command_argv(p"/bin/sh", argv, s.root, {LC_ALL: "C"}, input, out, err, timeout: timeout)
  }
  let status = process.run(plan)?.exit_code()?
  {util: "tee", args: [], status: status, stdout: out.read_bytes()?, stderr: err.read_bytes()?}
}

proc line_count(data: Bytes) [error] -> Int { data.utf8()?.split("\n").len() - 1 }

# origin: gnu tee/append.log
test test_gnu_tee_append_log { |ctx|
  let s = uu.scene(ctx)?
  let files = [f"{n}" for n in range(1, 14)]
  for option in ["-a", "--append"] {
    uu.write(s, "inp", "line 1\n")?
    let initial = uu.invoke_from_path(s, "tee", files, uu.at(s, "inp"))?
    uu.succeeds(initial)
    uu.stdout_is(initial, "line 1\n")
    for name in files { uu.file_is(s, name, "line 1\n") }
    uu.write(s, "inp", "line 2\n")?
    var args: List[Str] = []
    for name in files { args += [option, name] }
    let appended = uu.invoke_from_path(s, "tee", args, uu.at(s, "inp"))?
    uu.succeeds(appended)
    uu.stdout_is(appended, "line 2\n")
    for name in files { uu.file_is(s, name, "line 1\nline 2\n") }
  }
}

# origin: gnu tee/write-eagain.log
test test_gnu_tee_write_eagain_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "exp", "a\n")?
  let words = uu.argv(s, "tee", [p"file1"])?
  let trace = uu.at(s, "trace")
  # File identity selects the same output even when the implementation writes
  # through a duplicated descriptor rather than its original descriptor number.
  let injected = words[..3].extend([p"strace", p"-f", p"-qqq", p"-o", trace,
    p"-P", uu.at(s, "file1"), p"-e", p"trace=write", p"-e", p"inject=write:error=EAGAIN:when=1"]).extend(words[3..])
  let r = wrapped(s, r"""exec "$@"; """, injected, stdin: uu.at(s, "exp"), timeout: 10s)
  uu.succeeds(r)
  uu.stdout_only(r, "a\n")
  uu.file_is(s, "file1", "a\n")
  assert "INJECTED" in trace.read_text()? and "EAGAIN" in trace.read_text()?, "EAGAIN must reach the file output"
}

# origin: gnu tee/tee.log
test test_gnu_tee_tee_log { |ctx|
  test.timeout(ctx, 120s)
  let s = uu.scene(ctx)?
  uu.write(s, "sample", "line\n")?
  let delayed = wrapped(s, r"""(printf '%s\n' 1; sleep 0.1; printf '%s\n' 2) | "$@"; """, uu.argv(s, "tee", [])?)
  uu.succeeds(delayed)
  uu.stdout_is(delayed, "1\n2\n")
  let unbuffered = wrapped(s, r"""(printf '%s' a; for delay in 0.1 0.2 0.4 0.8 1.6 3.2 6.4; do sleep "$delay"; [ -s tee_output ] && exit 0; done; touch no_output) | "$@" > tee_output; [ ! -e no_output ]; """, uu.argv(s, "tee", [])?)
  uu.succeeds(unbuffered)
  for count in [0, 1, 2, 12, 13] {
    let files = [f"{n}" for n in range(1, count + 1)]
    for name in files { uu.remove(s, name)? }
    let r = uu.invoke_from_path(s, "tee", files, uu.at(s, "sample"))?
    uu.succeeds(r)
    uu.stdout_is(r, "line\n")
    for name in files { uu.file_is(s, name, "line\n") }
  }
  let dash = uu.invoke_from_path(s, "tee", ["-"], uu.at(s, "sample"))?
  uu.succeeds(dash)
  uu.stdout_only(dash, "line\n")
  uu.file_is(s, "-", "line\n")
  let no_output = wrapped(s, r"""yes | "$@" > /dev/full; """, uu.argv(s, "tee", [p"/dev/full"])?, timeout: 10s)
  uu.fails_with_code(no_output, 1)
  assert line_count(no_output.stderr) == 2
  let many_lines = [f"{n}\n" for n in range(1, 10001)].join("")
  uu.write(s, "multi_read", many_lines)?
  let continuing = uu.invoke_from_path(s, "tee", ["/dev/full", "out2"], uu.at(s, "multi_read"), stdout: uu.at(s, "out1"))?
  uu.fails_with_code(continuing, 1)
  uu.file_is(s, "out1", many_lines)
  uu.file_is(s, "out2", many_lines)
  assert line_count(continuing.stderr) == 1
  let failed_stdout = uu.invoke_from_path(s, "tee", ["out1", "out2"], uu.at(s, "multi_read"), stdout: p"/dev/full")?
  uu.fails_with_code(failed_stdout, 1)
  uu.file_is(s, "out1", many_lines)
  uu.file_is(s, "out2", many_lines)
  assert line_count(failed_stdout.stderr) == 1
  let idle = wrapped(s, r"""(for delay in 0.1 0.2 0.4 0.8 1.6 3.2 6.4; do sleep "$delay"; [ -f tee.exited ] && exit 0; done) | { "$@" && touch tee.exited; } | :; """, uu.argv(s, "tee", [p"-p"])?, timeout: 10s)
  assert line_count(idle.stderr) == 0
  assert uu.exists(s, "tee.exited")?
  uu.touch(s, "file.ro")?
  uu.set_mode(s, "file.ro", 0o444)?
  uu.fails_with_code(uu.invoke(s, "tee", ["-p", "file.ro"])?, 1)
  uu.mkfifo(s, "fifo")?
  let nonblocking = wrapped(s, r"""(sleep 0.1; dd of=/dev/null status=none) < fifo & reader=$!; dd count=20 bs=100K if=/dev/zero status=none | { dd count=0 oflag=nonblock status=none; "$@"; } > fifo; result=$?; wait "$reader"; exit "$result"; """, uu.argv(s, "tee", [])?, timeout: 10s)
  uu.succeeds(nonblocking)
  let probe = wrapped(s, r"""dd count=1 if=fifo of=/dev/null status=none & reader=$!; yes > fifo; result=$?; wait "$reader"; exit "$result"; """, [], timeout: 10s)
  let sigpipe = probe.status
  for row in [
    {args: ["./e/noent"], status: sigpipe, errors: 1},
    {args: ["-p"], status: 0, errors: 0},
    {args: ["--output-error=warn"], status: 1, errors: 1},
    {args: ["--output-error=exit", "/dev/null"], status: 1, errors: 1},
    {args: ["--output-error=exit", "./e/noent"], status: 1, errors: 1},
    {args: ["--output-error=exit-nopipe"], status: 0, errors: 0},
  ] {
    let r = wrapped(s, r"""dd count=1 if=fifo of=/dev/null status=none & reader=$!; yes | "$@" > fifo; result=$?; wait "$reader"; exit "$result"; """, uu.argv(s, "tee", [Path(word) for word in row.args])?, timeout: 10s)
    uu.fails_with_code(r, row.status)
    assert line_count(r.stderr) == row.errors
  }
}
