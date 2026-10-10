type Ran = {status: Int, stdout: Str, stderr: Str}

# Runs core/kill.xsh by its real path (so the invoked name is `kill` and
# `lib.gnu` resolves beside it), capturing both streams to files. `out` lets a
# test point stdout at a device such as /dev/full.
proc kill_run(ctx: TestContext, args: List[Str], out: Path? = null) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "kill")?
  let stdout_path = out ?? fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/kill.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(
    ctx.xsh_bin,
    argv,
    root,
    {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""},
    b"",
    stdout_path,
    err,
  )
  let status = process.run(plan)?
  let text = if out == null { stdout_path.read_bytes()?.utf8() ?? "" } else { "" }

  Ok({status: status.exit_code()?, stdout: text, stderr: err.read_text()?})
}

pure last_signal() -> Int {
  let table = process.signals()
  table[-1].number
}

# Signals a fresh `sleep 30` through the applet and returns how it ended.
proc kill_sleeper(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Status] {
  let child = spawn run sleep 30 ?
  let result = kill_run(ctx, [word.replace("PID", with: f"{child.pid}") for word in args])?

  assert result.status == 0, result.stderr
  assert result.stderr == ""

  let status = wait child?

  Ok(status)
}

test test_kill_default_signal_is_term { |ctx|
  let status = kill_sleeper(ctx, ["PID"])?
  assert status.signaled()
  assert status.signal_number()? == 15
}

test test_kill_signal_in_every_spelling { |ctx|
  for spelling in [
    ["-s", "9", "PID"],
    ["-s", "KILL", "PID"],
    ["-s", "KiLl", "PID"],
    ["-s", "SIGKILL", "PID"],
    ["-s", "SiGKiLl", "PID"],
    ["--signal=KILL", "PID"],
    ["--sig=9", "PID"],
    ["-n", "9", "PID"],
    ["-9", "PID"],
    ["-KILL", "PID"],
    ["-Kill", "PID"],
    ["-SIGKILL", "PID"],
  ] {
    let status = kill_sleeper(ctx, spelling)?
    assert status.signaled(), spelling.join(" ")
    assert status.signal_number()? == 9, spelling.join(" ")
  }
}

test test_kill_realtime_signals_by_offset { |ctx|
  let table = process.signals()
  let last = table[-1]
  if last.name != "RTMAX" {
    test.skip("the host has no real-time signals")
  }

  let rtmin = process.signal("RTMIN")?.number
  # A signal the parent shell left ignored would not end the sleeper.
  if process.signal_action("RTMIN+7")? != "default" {
    test.skip("real-time signals are ignored in this environment")
  }

  let up = kill_sleeper(ctx, ["-s", "SIGRTMIN+7", "PID"])?
  assert up.signal_number()? == rtmin + 7
  let down = kill_sleeper(ctx, ["-s", "RTMAX-7", "PID"])?
  assert down.signal_number()? == last.number - 7
  let bare = kill_sleeper(ctx, ["-s", "RTMIN", "PID"])?
  assert bare.signal_number()? == rtmin
}

test test_kill_signal_zero_only_probes { |ctx|
  let child = spawn run sleep 30 ?
  for spelling in [["-0"], ["-s", "0"], ["-s", "EXIT"]] {
    let result = kill_run(ctx, spelling.extend([f"{child.pid}"]))?
    assert result.status == 0, spelling.join(" ")
    assert result.stderr == ""
  }

  assert kill_run(ctx, ["-0", "0"])?.status == 0, "the caller's own process group exists"

  let missing = kill_run(ctx, ["-0", "999999999"])?
  assert missing.status == 1
  assert missing.stderr == "kill: sending signal to 999999999 failed: No such process\n", missing.stderr

  child.cancel(signal: "KILL")
}

test test_kill_negative_operand_signals_a_process_group { |ctx|
  let child = spawn run sleep 30 ?
  assert process.group_id(child.pid)? == child.pid

  let result = kill_run(ctx, ["-s", "TERM", "--", f"-{child.pid}"])?
  assert result.status == 0, result.stderr

  let status = wait child?
  assert status.signal_number()? == 15
}

test test_kill_negative_pid_needs_no_separator { |ctx|
  let child = spawn run sleep 30 ?
  let result = kill_run(ctx, ["-TERM", f"-{child.pid}"])?
  assert result.status == 0, result.stderr
  assert result.stderr == ""

  let status = wait child?
  assert status.signal_number()? == 15
}

test test_kill_reports_each_bad_operand_and_continues { |ctx|
  let child = spawn run sleep 30 ?
  let result = kill_run(ctx, ["abc", "999999999", f"{child.pid}", "1x", "99999999999"])?
  assert result.status == 1
  assert result.stderr == "kill: 'abc': invalid process id\nkill: sending signal to 999999999 failed: No such process\nkill: '1x': invalid process id\nkill: '99999999999': invalid process id\n", result.stderr

  let status = wait child?
  assert status.signal_number()? == 15, "operands after a failure are still signaled"
}

test test_kill_every_process_is_refused_not_signaled { |ctx|
  let result = kill_run(ctx, ["-s", "0", "--", "-1"])?
  assert result.status == 1
  assert result.stderr == "kill: '-1': signaling every process is not supported\n", result.stderr
}

test test_kill_without_a_process_id_is_a_usage_error { |ctx|
  for args in [[], ["-1"], ["-9"], ["-TERM"], ["-s", "TERM"], ["--signal=HUP"]] {
    let result = kill_run(ctx, args)?
    assert result.status == 1, args.join(" ")
    assert result.stdout == ""
    assert result.stderr == "kill: no process ID specified\nTry 'kill --help' for more information.\n", result.stderr
  }
}

test test_kill_rejects_invalid_signals_before_sending { |ctx|
  let child = spawn run sleep 30 ?
  for args in [
    ["-s", "IAMNOTASIGNAL"],
    ["-s", "IaMnOtAsIgNaL"],
    ["-s", "65"],
    ["-s", " 9"],
    ["-s", "+9"],
    ["-s", "0x9"],
    ["-s", ""],
  ] {
    let result = kill_run(ctx, args.extend([f"{child.pid}"]))?
    assert result.status == 1, args.join(" ")
    assert result.stderr.ends_with(": invalid signal\n"), result.stderr
  }

  # An out-of-range obsolete number is a bad signal, never a negative pid.
  for bad in ["-65", "-129", "-NOPESIG", "-SIGNOPE"] {
    let result = kill_run(ctx, [bad, f"{child.pid}"])?
    assert result.status == 1, bad
    assert result.stderr == f"kill: '{bad.byte_slice(1)}': invalid signal\n", result.stderr
  }

  let still = kill_run(ctx, ["-0", f"{child.pid}"])?
  assert still.status == 0, "the target survived every rejected request"
  child.cancel(signal: "KILL")
}

test test_kill_lowercase_obsolete_names_are_options { |ctx|
  let result = kill_run(ctx, ["-kill", "123"])?
  assert result.status == 1
  assert result.stderr == "kill: unexpected argument '-kill' found\nTry 'kill --help' for more information.\n", result.stderr
}

test test_kill_signal_conflicts_with_listing { |ctx|
  for args in [["-s", "EXIT", "1", "-l"], ["-s", "EXIT", "1", "-t"], ["-9", "-l"], ["-t", "-s", "9"]] {
    let result = kill_run(ctx, args)?
    assert result.status == 1, args.join(" ")
    assert result.stderr == "kill: cannot combine signal with -l or -t\nTry 'kill --help' for more information.\n", result.stderr
  }
}

test test_kill_list_and_table_are_exclusive { |ctx|
  for args in [["-l", "-t"], ["-t", "--list"]] {
    let result = kill_run(ctx, args)?
    assert result.status == 1, args.join(" ")
    assert result.stderr == "kill: cannot combine -l and -t\nTry 'kill --help' for more information.\n", result.stderr
  }
}

test test_kill_capital_l_lists_like_lowercase { |ctx|
  let lower = kill_run(ctx, ["-l"])?
  let upper = kill_run(ctx, ["-L"])?
  assert upper.status == 0, upper.stderr
  assert upper.stdout == lower.stdout
  assert upper.stdout.starts_with("EXIT\n")
}

test test_kill_list_prints_one_name_per_line_from_exit { |ctx|
  let result = kill_run(ctx, ["-l"])?
  assert result.status == 0
  assert result.stdout.ends_with("\n")
  let names = result.stdout.trim().split("\n")
  assert names[0] == "EXIT"
  for expected in ["HUP", "INT", "KILL", "TERM", "CHLD", "RTMIN", "RTMAX"] {
    assert expected in names, expected
  }

  assert names == [entry.name for entry in process.signals()]
  assert kill_run(ctx, ["--list"])?.stdout == result.stdout
}

test test_kill_table_numbers_and_names { |ctx|
  let result = kill_run(ctx, ["-t"])?
  assert result.status == 0
  let lines = result.stdout.lines()
  assert lines[0] == " 0 EXIT"
  assert " 9 KILL" in lines
  assert "15 TERM" in lines
  assert lines.len() == process.signals().len()
  assert kill_run(ctx, ["--table"])?.stdout == result.stdout
}

test test_kill_list_converts_between_names_and_numbers { |ctx|
  assert kill_run(ctx, ["-l", "9"])?.stdout == "KILL\n"
  assert kill_run(ctx, ["-l", "KILL"])?.stdout == "9\n"
  assert kill_run(ctx, ["-l", "KiLl"])?.stdout == "9\n"
  assert kill_run(ctx, ["-l", "SIGTERM"])?.stdout == "15\n"
  assert kill_run(ctx, ["-l", "--list", "INT", "KILL"])?.stdout == "2\n9\n"
  assert kill_run(ctx, ["-l", "IO"])?.stdout == "29\n"
  assert kill_run(ctx, ["-l", "SIGIO"])?.stdout == "29\n"
  assert kill_run(ctx, ["-l", "0"])?.stdout == "EXIT\n"
  assert kill_run(ctx, ["-l", "--", "KILL"])?.stdout == "9\n"
  assert kill_run(ctx, ["-l", "RTMAX"])?.stdout == f"{last_signal()}\n"
  # Numbers without a name print as themselves.
  assert kill_run(ctx, ["-l", "32", "33"])?.stdout == "32\n33\n"
}

test test_kill_list_accepts_wait_statuses { |ctx|
  assert kill_run(ctx, ["-l", "143"])?.stdout == "TERM\n"
  assert kill_run(ctx, ["-l", "137"])?.stdout == "KILL\n"
  assert kill_run(ctx, ["-l", "128", "256", "2304"])?.stdout == "EXIT\nEXIT\nEXIT\n"
  for status in ["111", "384", "65", "99", "1x", " 9", "+9"] {
    let result = kill_run(ctx, ["-l", status])?
    assert result.status == 1, status
    assert result.stdout == ""
    assert result.stderr == f"kill: '{status}': invalid signal\n", result.stderr
  }
}

test test_kill_list_continues_after_an_invalid_signal { |ctx|
  let result = kill_run(ctx, ["-l", "IAMNOTASIGNAL", "INT", "KILL"])?
  assert result.status == 1
  assert result.stdout == "2\n9\n"
  assert result.stderr == "kill: 'IAMNOTASIGNAL': invalid signal\n", result.stderr
}

test test_kill_list_names_every_number_up_to_the_last_named_signal { |ctx|
  let last = last_signal()
  let numbers = [f"{n}" for n in range(last + 1)]
  let result = kill_run(ctx, ["-l", "--"].extend(numbers))?
  assert result.status == 0, result.stderr
  assert result.stdout.trim().split("\n").len() == last + 1
}

test test_kill_list_write_errors_are_reported { |ctx|
  for args in [["-l"], ["-l", "TERM"], ["--list", "9"], ["--table"]] {
    let result = kill_run(ctx, args, out: /dev/full)?
    assert result.status == 1, args.join(" ")
    assert result.stderr == "kill: write error: No space left on device\n", result.stderr
  }
}

test test_kill_option_errors_use_getopt_wording { |ctx|
  let unknown = kill_run(ctx, ["--definitely-invalid"])?
  assert unknown.status == 1
  assert unknown.stderr == "kill: unrecognized option '--definitely-invalid'\nTry 'kill --help' for more information.\n", unknown.stderr

  let missing = kill_run(ctx, ["-s"])?
  assert missing.status == 1
  assert missing.stderr == "kill: option requires an argument -- 's'\nTry 'kill --help' for more information.\n", missing.stderr
}

test test_kill_help_and_version_go_to_stdout { |ctx|
  let help = kill_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stderr == ""
  assert help.stdout.starts_with("Usage: kill [-s SIGNAL | -SIGNAL] PID...")

  let version = kill_run(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("kill ")
}

test test_kill_realtime_aliases_list_and_probe_the_same_signal { |ctx|
  let number = process.signal("RTMIN+1")?.number
  let listed = kill_run(ctx, ["-l", "SIGRTMIN+1", f"{number}"])?
  assert listed.status == 0
  assert listed.stdout == f"{number}\nRTMIN+1\n"
  let sleeper = spawn run sleep 30 ?
  let probe = kill_run(ctx, ["-s", "0", f"{sleeper.pid}"])?
  assert probe.status == 0
  let status = kill_sleeper(ctx, ["-s", "SIGRTMIN+1", "PID"])?
  assert status.signal_number()? == number
  sleeper.cancel()
}
