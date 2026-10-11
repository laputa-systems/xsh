use support.uu

# origin: busybox pidof/pidof (exit with error)
test test_bb_pidof_pidof_exit_with_error_503ff3da { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "pidof", ["veryunlikelyoccuringbinaryname"], timeout: 5s)?
  uu.fails_with_code(r, 1)
  uu.no_output(r)
}

# origin: busybox pidof/pidof this
test test_bb_pidof_pidof_this_48b653be { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "pidof.tests", "#!/bin/sh\nprintf '%s\\n' \"$$\"\n\"$@\"\nresult=$?\nexit \"$result\"\n")?
  uu.set_mode(s, "pidof.tests", 0o755)?
  let argv = [uu.at(s, "pidof.tests")].extend(uu.argv(s, "pidof", [p"pidof.tests"])?)
  let out = uu.at(s, "out")
  let err = uu.at(s, "err")
  let command = process.command_argv(uu.at(s, "pidof.tests"), argv, s.root, {}, b"", out, err, timeout: 5s)
  assert process.run(command)?.exit_code()? == 0
  let lines = out.read_text()?.trim().split("\n")
  assert lines.len() == 2
  assert lines[0] in lines[1].split(" ")
  assert err.read_bytes()? == b""
}
