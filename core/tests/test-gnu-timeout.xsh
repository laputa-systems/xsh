use support.uu as uu

# origin: gnu timeout/timeout-blocked.log
test test_gnu_timeout_timeout_blocked_log { |ctx|
  let s = uu.scene(ctx)?
  let adapter = uu.at(s, "block-alarm.xsh")
  adapter.write(r"""proc main(...words: List[Bytes]) {
  let argv = [Path.parse_bytes(word)? for word in words]
  var environment: Map[Str, Bytes] = {}
  for entry in env.entries()? {
    if let Ok(key) = entry.name.utf8() { environment[key] = entry.value }
  }
  let command = process.command_argv(argv[0], argv)
  unix.exec_env(command, environment, block_signals: ["ALRM"])?
}
""")?
  let words = uu.argv(s, "timeout", [p".1", p"sleep", p"10"])?
  let masked = words[..3].extend([s.ctx.xsh_bin, adapter]).extend(words[3..])
  let status = process.run(process.command_argv(masked[0], masked, s.root,
    stdout: uu.at(s, "out"), stderr: uu.at(s, "err"), timeout: 5s))?
  assert status.exited_with(124), uu.read_text(s, "err")?
  uu.file_is(s, "out", "")
  uu.file_is(s, "err", "")
}

# origin: gnu timeout/timeout-large-parameters.log
test test_gnu_timeout_timeout_large_parameters_log { |ctx|
  let s = uu.scene(ctx)?
  let maximum = "1.189731495357231765e+4932"
  let overflow = uu.invoke(s, "timeout", ["9223372036854775808", "sleep", "3"], timeout: 10s)?
  if overflow.status == 124 { test.skip("kernel timer overflow caused an immediate deadline") }
  for duration in ["4294967296", "9223372036854775808d", "2.34e+5d", maximum] {
    uu.succeeds(uu.invoke(s, "timeout", [duration, "sleep", "0"], timeout: 5s)?)
  }
  uu.fails_with_code(uu.invoke(s, "timeout", ["--", "-" + maximum, "sleep", "0"], timeout: 5s)?, 125)
}

# origin: gnu timeout/timeout-parameters.log
test test_gnu_timeout_timeout_parameters_log { |ctx|
  let s = uu.scene(ctx)?
  for args in [
    ["invalid", "sleep", "0"],
    [" -0.1", "sleep", "0"],
    [" -1e-10000", "sleep", "0"],
    ["--kill-after=invalid", "1", "sleep", "0"],
    ["42D", "sleep", "0"],
    ["--signal=invalid", "1", "sleep", "0"],
  ] { uu.fails_with_code(uu.invoke(s, "timeout", args, timeout: 5s)?, 125) }
  for duration in ["10.34", "9.999999999"] {
    uu.succeeds(uu.invoke(s, "timeout", [duration, "sleep", "0"], timeout: 5s)?)
  }
  uu.fails_with_code(uu.invoke(s, "timeout", ["1e-10000", "sleep", "10"], timeout: 5s)?, 124)
  if uu.invoke(s, "env", ["."])?.status == 126 {
    uu.fails_with_code(uu.invoke(s, "timeout", ["10", "."], timeout: 5s)?, 126)
  }
  uu.fails_with_code(uu.invoke(s, "timeout", ["10", "no_such"], timeout: 5s)?, 127)
}
