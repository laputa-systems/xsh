##! Transcribed from the MIT-licensed uutils stdbuf integration tests.

use support.uu as uu

# A FIFO keeps stderr nonterminal and pipe-backed while stdout stays a separate stream.
proc pipe_stderr(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let captures = test.temp_dir(s.ctx, name: "stdbuf-pipe")?
  let out = fp"{captures}/stdout"
  let err = fp"{captures}/stderr"
  let fifo = fp"{captures}/pipe"
  fs.mkfifo(fifo, 0o600)?
  let words = uu.argv(s, "stdbuf", [Path(word) for word in args])?
  let argv = [p"/bin/sh", p"-c", Path(r"""fifo=$1; shift; cat "$fifo" >&2 & reader=$!; "$@" 2>"$fifo"; rc=$?; wait "$reader" || exit; exit "$rc"; """), p"stdbuf-pipe", fifo].extend(words)
  let status = process.run(process.command_argv(p"/bin/sh", argv, s.root, stdout: out, stderr: err, timeout: 5s))?.exit_code()?
  Ok({util: "stdbuf", args: args, status: status, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

# origin: uutils test_stdbuf::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_stdbuf_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = uu.scene(ctx)?
  let r = pipe_stderr(s, ["-o", "6pq", "head"])?
  uu.fails_with_code(r, 125)
  uu.stderr_only(r, "stdbuf: invalid suffix in -o argument '6pq'\n")
}

# origin: uutils test_stdbuf::invalid_input
test test_uu_stdbuf_invalid_input { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "stdbuf", ["-/"])?, 125)
}

# origin: uutils test_stdbuf::test_no_such
test test_uu_stdbuf_no_such { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "stdbuf", ["-o1", "no_such"])?
  uu.fails_with_code(r, 127)
  uu.stderr_contains(r, "No such file or directory")
}

# origin: uutils test_stdbuf::test_permission
test test_uu_stdbuf_permission { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "stdbuf", ["-o1", "."])?
  uu.fails_with_code(r, 126)
  uu.stderr_contains(r, "Permission denied")
}

# origin: uutils test_stdbuf::test_stdbuf_invalid_mode_fails
test test_uu_stdbuf_stdbuf_invalid_mode_fails { |ctx|
  for option in ["--input", "--output", "--error"] {
    let short = if option == "--input" { "-i" } else if option == "--output" { "-o" } else { "-e" }
    {
      let s = uu.scene(ctx)?
      let r = uu.invoke(s, "stdbuf", [option, "1024R", "head"])?
      uu.fails_with_code(r, 125)
      uu.stderr_only(r, f"stdbuf: {short} argument '1024R' too large\n")
    }
    {
      let s = uu.scene(ctx)?
      let r = uu.invoke(s, "stdbuf", [option, "1Y", "head"])?
      uu.fails_with_code(r, 125)
      uu.stderr_contains(r, f"stdbuf: {short} argument '1Y' too large")
    }
  }
}

# origin: uutils test_stdbuf::test_stdbuf_line_buffering_stdin_fails
test test_uu_stdbuf_stdbuf_line_buffering_stdin_fails { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "stdbuf", ["-i", "L", "head"])?
  uu.fails(r)
  uu.stderr_only(r, "stdbuf: line buffering standard input is meaningless\nTry 'stdbuf --help' for more information.\n")
}

# origin: uutils test_stdbuf::test_stdbuf_no_tmpdir_leak
test test_uu_stdbuf_stdbuf_no_tmpdir_leak { |ctx|
  let s = uu.scene(ctx)?
  let dedicated = test.temp_dir(ctx, name: "stdbuf-tmpdir")?
  for _ in range(5) {
    let result = uu.invoke(s, "stdbuf", ["-oL", "true"], vars: {TMPDIR: dedicated}, timeout: 5s)?
  }
  let leaked = fs.children(dedicated)? |> where { |entry| entry.path.is_dir()? } |> count()
  assert leaked == 0
}
