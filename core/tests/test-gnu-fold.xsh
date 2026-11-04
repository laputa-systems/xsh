use support.uu

const BUFFER = 262144
const UTF8 = "fr_FR.UTF-8"

proc fold_file(s: uu.Scene, name: Str, input: Bytes, args: List[Str], expected: Bytes) [fs, process, env, error] {
  uu.write_bytes(s, name, input)
  let r = uu.invoke(s, "fold", [@args, name], vars: {LC_ALL: UTF8}, timeout: 30s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, expected)
  Ok()
}

# origin: gnu fold/fold-characters.log
test test_gnu_fold_fold_characters_log { |ctx|
  let s = uu.scene(ctx)?
  fold_file(s, "input1", bytes.from_text("뉐뉐뉐\n"), ["-w", "5"], bytes.from_text("뉐뉐\n뉐\n"))
  fold_file(s, "input1", bytes.from_text("뉐뉐뉐\n"), ["--characters", "-w", "5"], bytes.from_text("뉐뉐뉐\n"))
  let fullwidth = ["：" for n in range(50)].join("") + "\n"
  let columns = [["：" for n in range(5)].join("") + "\n" for row in range(10)].join("")
  let characters = [["：" for n in range(10)].join("") + "\n" for row in range(5)].join("")
  fold_file(s, "input2", bytes.from_text(fullwidth), ["-w", "10"], bytes.from_text(columns))
  fold_file(s, "input2", bytes.from_text(fullwidth), ["--characters", "-w", "10"], bytes.from_text(characters))
  let edge = ["a" for n in range(BUFFER - 1)].join("") + "뉐" + ["a" for n in range(100)].join("") + "\n"
  uu.write(s, "input3", edge)
  let boundary = uu.invoke(s, "fold", ["--characters", "input3"], vars: {LC_ALL: UTF8})?
  uu.succeeds(boundary)
  uu.no_stderr(boundary)
  let lines = boundary.stdout.utf8()?.lines()
  let tail = [lines[index] + "\n" for index in range(lines.len() - 4, lines.len())].join("")
  let expected = ["a" for n in range(80)].join("") + "\n" + ["a" for n in range(63)].join("") + "뉐" + ["a" for n in range(16)].join("") + "\n" + ["a" for n in range(80)].join("") + "\naaaa\n"
  assert tail == expected
  let malformed = b"\xff|\xed\xba\xad|\xc2\x89|\xed\xa6\xbf\xed\xbf\xbf|\0\n"
  let invalid = uu.invoke(s, "fold", [], stdin: malformed, vars: {LC_ALL: UTF8})?
  uu.succeeds(invalid)
  uu.stdout_only_bytes(invalid, malformed)
  let incomplete = uu.invoke(s, "fold", [], stdin: b"\xc3", vars: {LC_ALL: UTF8})?
  uu.succeeds(incomplete)
  assert incomplete.stdout.len() == 1
  uu.no_stderr(incomplete)
}

# origin: gnu fold/fold-nbsp.log
test test_gnu_fold_fold_nbsp_log { |ctx|
  let s = uu.scene(ctx)?
  fold_file(s, "input1", bytes.from_text("abcdefghijklmnop qrstuvwxyz\n"), ["--spaces", "--width", "10"], bytes.from_text("abcdefghij\nklmnop qrs\ntuvwxyz\n"))
  fold_file(s, "input2", bytes.from_text("abcdefghijklmnop  qrstuvwxyz\n"), ["--spaces", "--width", "10"], bytes.from_text("abcdefghij\nklmnop  qr\nstuvwxyz\n"))
}

# origin: gnu fold/fold-spaces.log
test test_gnu_fold_fold_spaces_log { |ctx|
  let s = uu.scene(ctx)?
  fold_file(s, "input1", bytes.from_text("abcdefghijklmnop qrstuvwxyz\n"), ["--spaces", "--width", "10"], bytes.from_text("abcdefghij\nklmnop \nqrstuvwxyz\n"))
  fold_file(s, "input2", bytes.from_text("abcdefghijklmnop  qrstuvwxyz\n"), ["--spaces", "--width", "10"], bytes.from_text("abcdefghij\nklmnop  \nqrstuvwxyz\n"))
}

# origin: gnu fold/multiple-files.log
test test_gnu_fold_multiple_files_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file1", "a\n")
  uu.write(s, "file2", "b\n")
  let r = uu.invoke(s, "fold", ["file1", "missing", "file2"])?
  uu.fails_with_code(r, 1)
  uu.stdout_is(r, "a\nb\n")
  uu.stderr_is(r, "fold: missing: No such file or directory\n")
}

# Apply the address-space limit after the isolation launcher has finished. Its
# compilation is outside the memory budget of the applet selected by uu.
pure constrained(words: List[Path], kib: Int) -> List[Path] {
  let script = p"ulimit -v \"$1\" || exit; shift; exec \"$@\"; "
  [words[0], words[1], words[2], p"/bin/sh", p"-c", script, p"fold-limit", fp"{kib}", @[words[index] for index in range(3, words.len())]]
}

proc limited(s: uu.Scene, words: List[Path], kib: Int, input: Path, output: Path, diagnostic: Path, vars: Record) [process, env, error] -> Result[Bool, Error] {
  let args = constrained(words, kib)
  let command = process.command_argv(args[0], args, s.root, vars, input, output, diagnostic)
  Ok(process.run(command)?.exited_with(0))
}

proc zero_width_file(ctx: TestContext, name: Str, data: Bytes, args: List[Str]) [fs, process, env, error] {
  let s = uu.scene(ctx)?
  uu.write_bytes(s, name, data)
  let r = uu.invoke(s, "fold", args + [name], vars: {LC_ALL: "", LC_CTYPE: UTF8, LANG: "C"}, timeout: 600s)?
  uu.succeeds(r)
  uu.no_stderr(r)
  let line_count = r.stdout.utf8()?.split("\n").len() - 1
  assert line_count == (if args.is_empty() { 0 } else { BUFFER * 2 / 80 })
}

# origin: gnu fold/fold-zero-width.log
test test_gnu_fold_zero_width_nul_columns_log { |ctx|
  test.timeout(ctx, 600s)
  zero_width_file(ctx, "nul", bytes.zero(BUFFER * 2)?, [])
}

test test_gnu_fold_zero_width_nul_characters_log { |ctx|
  test.timeout(ctx, 600s)
  zero_width_file(ctx, "nul", bytes.zero(BUFFER * 2)?, ["--characters"])
}

test test_gnu_fold_zero_width_unicode_columns_log { |ctx|
  test.timeout(ctx, 600s)
  zero_width_file(ctx, "inp", bytes.from_text(["​" for n in range(BUFFER * 2)].join("")), [])
}

test test_gnu_fold_zero_width_unicode_characters_log { |ctx|
  test.timeout(ctx, 600s)
  zero_width_file(ctx, "inp", bytes.from_text(["​" for n in range(BUFFER * 2)].join("")), ["--characters"])
}

proc fold_argv(s: uu.Scene, script: Path, args: List[Path], vars: Record) [fs, error] -> Result[List[Path], Error] {
  let words = uu.argv(s, "fold", args, vars:)?
  Ok([@words[..4], script, @words[5..]])
}

proc full_output(s: uu.Scene, script: Path, character: Str, allowance: Int) [fs, process, env, net, error] {
  let ready = uu.at(s, "ready.fifo")
  fs.mkfifo(ready, 0o600)
  defer ready.remove()
  let ready_fd = unix.open_fd(ready, nonblock: true)?
  defer unix.close_fd(ready_fd)
  let vars = {LC_ALL: "", LC_CTYPE: UTF8, LANG: "C", XSH_FOLD_READY: ready.display()}
  let words = constrained(fold_argv(s, script, [], vars)?, allowance)
  let diagnostic = uu.at(s, "err")
  let shell = p"tr '\\0' \"$1\" </dev/zero | { shift; \"$@\" > /dev/full; }"
  let args = [p"sh", p"-c", shell, p"fold-stream", fp"{character}", @words]
  let consumer = spawn process.command_argv(p"/bin/sh", args, s.root, vars, b"", uu.at(s, "out"), diagnostic)?
  loop {
    let events = unix.poll_fd(ready_fd, ["readable"], timeout_ms: 20)?
    if "readable" in events {
      assert unix.read_fd(ready_fd, 1)? == b"R", "fold runtime readiness was not acknowledged"
      break
    }
    assert ! ("error" in events or "invalid" in events or "hangup" in events), "fold exited before runtime readiness"
    if let completed = process.wait_timeout([consumer], 0ms)? {
      assert false, f"fold exited before runtime readiness with status {completed.status.shell_code()?}: {diagnostic.read_text()?}"
    }
  }
  let completed = process.wait_timeout([consumer], 10s)?
  assert completed != null, "fold did not diagnose the full output device within ten seconds after runtime readiness"
  let status = completed.status
  assert status.exited_with(1), f"fold write-error status: {status.shell_code()?}"
  let errors = diagnostic.read_text()?
  assert "No space left on device" in errors, f"input {character}, limit {allowance} KiB: {errors}"
}

test test_gnu_fold_zero_width_full_output_log { |ctx|
  test.timeout(ctx, 600s)
  let s = uu.scene(ctx)?
  let script = uu.at(s, "fold.xsh")
  let source = fp"{ctx.core_dir}/fold.xsh".read_text()?
  script.write(source + r"""
if let Ok(ready) = e"XSH_FOLD_READY" {
  let fd = unix.open_fd(Path(ready), write: true)?
  defer unix.close_fd(fd)
  assert unix.write_fd(fd, b"R")? == 1
}
""")
  uu.at(s, "lib").symlink(to: fp"{ctx.core_dir}/lib")
  let stream_locale = {LC_ALL: "", LC_CTYPE: UTF8, LANG: "C"}
  let empty_words = fold_argv(s, script, [p"/dev/null"], stream_locale)?
  var minimum = 0
  var probe_results = []
  let candidates = [@[n * 5000 for n in range(1, 11)], 100000, 200000, 400000, 800000]
  for candidate in candidates {
    let code = limited(s, empty_words, candidate, p"/dev/null", p"/dev/null", uu.at(s, "probe.err"), stream_locale)?
    probe_results += [code]
    if code { minimum = candidate; break }
  }
  assert minimum > 0, f"fold startup did not fit through 800000 KiB; probes {json.encode(probe_results)?}; last diagnostic {uu.read_text(s, "probe.err")?}"
  var previous = minimum
  for n in range(1, minimum / 1000) {
    let decrement = n * 1000
    let candidate = minimum - decrement
    let code = limited(s, empty_words, candidate, p"/dev/null", p"/dev/null", uu.at(s, "probe.err"), stream_locale)?
    break unless code
    previous = candidate
  }
  let allowance = previous + 4 + 6000
  for character in ["\\n", "\\0", "\\303"] {
    full_output(s, script, character, allowance)
  }
}
