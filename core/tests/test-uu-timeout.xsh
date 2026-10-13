##! Native ports of the uutils timeout integration tests.
use support.uu

# timeout may terminate itself with KILL when it signals the entire process group.
# Shell status preserves that observable result as 128 plus the signal number.
proc invoke(s: uu.Scene, args: List[Str], limit: Duration = 15s) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let plan = uu.command(s, "timeout", args, timeout: limit)?
  let status = process.run(plan)?
  Ok({util: "timeout", args: args, status: status.shell_code()?, stdout: uu.read(s, ".uu-stdout")?, stderr: uu.read(s, ".uu-stderr")?})
}

proc usage_error(r: uu.Ran, message: Str) {
  uu.stderr_only(r, f"timeout: {message}\nTry 'timeout --help' for more information.\n")
}

# Send the same external signal after the upstream Linux 100 ms delay, then
# require the controller to finish before inspecting its captured child output.
proc interrupt(s: uu.Scene, args: List[Str], signal: Str) [fs, process, env, time, error] -> Result[uu.Ran, Error] {
  let plan = uu.command(s, "timeout", args, timeout: 8s)?
  let child = spawn plan?
  time.sleep(100ms)
  let child_pid = child.pid
  process.kill(child_pid, signal)
  let status = wait child?
  test.error_kind(process.kill(child_pid, "0"), "process-missing")
  Ok({util: "timeout", args: args, status: status.shell_code()?, stdout: uu.read(s, ".uu-stdout")?, stderr: uu.read(s, ".uu-stderr")?})
}

# origin: uutils test_timeout::test_command_cannot_invoke
test test_uu_timeout_command_cannot_invoke { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(invoke(s, ["1", "/"])?, 126)
}

# origin: uutils test_timeout::test_command_empty_args
test test_uu_timeout_command_empty_args { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke(s, ["", ""])?
  uu.fails(r)
  uu.stderr_contains(r, "timeout: invalid time interval ''")
}

# origin: uutils test_timeout::test_command_not_found
test test_uu_timeout_command_not_found { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(invoke(s, ["1", "/this/command/definitely/does/not/exist"])?, 127)
}

# origin: uutils test_timeout::test_command_with_args
test test_uu_timeout_command_with_args { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke(s, ["1700", "echo", "-n", "abcd"])?
  uu.succeeds(r)
  uu.stdout_only(r, "abcd")
}

# origin: uutils test_timeout::test_dont_overflow
test test_uu_timeout_dont_overflow { |ctx|
  let s = uu.scene(ctx)?
  for args in [["9223372036854775808d", "sleep", "0"], ["-k", "9223372036854775808d", "10", "sleep", "0"]] {
    let r = invoke(s, args)?
    uu.succeeds(r)
    uu.no_output(r)
  }
}

# origin: uutils test_timeout::test_dont_underflow
test test_uu_timeout_dont_underflow { |ctx|
  let s = uu.scene(ctx)?
  for interval in [".0000000001", "1e-100", "1e-18172487393827593258"] {
    let r = invoke(s, [interval, "sleep", "1"])?
    uu.fails_with_code(r, 124)
    uu.no_output(r)
  }
}

# origin: uutils test_timeout::test_foreground
test test_uu_timeout_foreground { |ctx|
  let s = uu.scene(ctx)?
  for flag in ["-f", "--foreground"] {
    let r = invoke(s, [flag, ".1", "sleep", "10"])?
    uu.fails_with_code(r, 124)
    uu.no_output(r)
  }
}

# origin: uutils test_timeout::test_foreground_signal0_kill_after
test test_uu_timeout_foreground_signal0_kill_after { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(invoke(s, ["--foreground", "-s0", "-k.1", ".1", "sleep", "10"])?, 137)
}

# origin: uutils test_timeout::test_forward_sigint_to_child
test test_uu_timeout_forward_sigint_to_child { |ctx|
  let s = uu.scene(ctx)?
  let r = interrupt(s, ["10", "sh", "-c", "trap 'echo got_int; exit 42' INT; sleep 5"], "INT")?
  uu.stdout_contains(r, "got_int")
}

# origin: uutils test_timeout::test_hex_timeout_ending_with_d
test test_uu_timeout_hex_timeout_ending_with_d { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke(s, ["0x0.1d", "sleep", "10"], 1s)?
  uu.fails_with_code(r, 124)
  uu.no_output(r)
}

# origin: uutils test_timeout::test_invalid_arg
test test_uu_timeout_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(invoke(s, ["--definitely-invalid"])?, 125)
}

# origin: uutils test_timeout::test_invalid_kill_after
test test_uu_timeout_invalid_kill_after { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke(s, ["-k", "xyz", "1", "sleep", "0"])?
  uu.fails_with_code(r, 125)
  usage_error(r, "invalid time interval 'xyz'")
}

# origin: uutils test_timeout::test_invalid_signal
test test_uu_timeout_invalid_signal { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke(s, ["-s", "invalid", "1", "sleep", "0"])?
  uu.fails(r)
  usage_error(r, "'invalid': invalid signal")
}

# origin: uutils test_timeout::test_invalid_time_interval::case_1_alphabetic
test test_uu_timeout_test_invalid_time_interval_case_1_alphabetic { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke(s, ["xyz", "sleep", "0"])?
  uu.fails_with_code(r, 125)
  usage_error(r, "invalid time interval 'xyz'")
}

# origin: uutils test_timeout::test_kill_after_long
test test_uu_timeout_kill_after_long { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke(s, ["--kill-after=1", "1", "sleep", "0"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_timeout::test_kill_after_preserves_timeout_exit_without_preserve_status
test test_uu_timeout_kill_after_preserves_timeout_exit_without_preserve_status { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke(s, ["-k", "1", "1", "sleep", "10"])?
  uu.fails_with_code(r, 124)
  uu.no_output(r)
}

# origin: uutils test_timeout::test_kill_subprocess
test test_uu_timeout_kill_subprocess { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke(s, ["1", "sh", "-c", "trap 'echo inside_trap' TERM; sleep 5"])?
  uu.fails_with_code(r, 124)
  uu.stdout_contains(r, "inside_trap")
}

# origin: uutils test_timeout::test_negative_interval
test test_uu_timeout_negative_interval { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke(s, ["--", "-1", "sleep", "0"])?
  uu.fails(r)
  usage_error(r, "invalid time interval '-1'")
}

# origin: uutils test_timeout::test_preserve_status
test test_uu_timeout_preserve_status { |ctx|
  let s = uu.scene(ctx)?
  for flag in ["-p", "--preserve-status"] {
    let r = invoke(s, [flag, ".1", "sleep", "10"])?
    uu.fails_with_code(r, 143)
    uu.no_output(r)
  }
}

# origin: uutils test_timeout::test_preserve_status_even_when_send_signal
test test_uu_timeout_preserve_status_even_when_send_signal { |ctx|
  let s = uu.scene(ctx)?
  for signal in ["CONT", "cOnT", "SIGcont"] {
    let r = invoke(s, ["-s", signal, "--preserve-status", ".1", "sleep", "1"])?
    uu.succeeds(r)
    uu.no_output(r)
  }
}

# origin: uutils test_timeout::test_realtime_signal_names
test test_uu_timeout_realtime_signal_names { |ctx|
  let s = uu.scene(ctx)?
  for item in [{signal: "RTMAX", name: "RTMAX"}, {signal: "RTMIN", name: "RTMIN"}, {signal: "SIGRTMAX", name: "RTMAX"}] {
    let r = invoke(s, ["-v", "-s", item.signal, ".1", "sleep", "1"])?
    uu.fails(r)
    uu.stderr_contains(r, f"sending signal {item.name} to command")
  }
}

# origin: uutils test_timeout::test_subcommand_return_code
test test_uu_timeout_subcommand_return_code { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(invoke(s, ["1", "true"])?)
  uu.fails_with_code(invoke(s, ["1", "false"])?, 1)
}

# origin: uutils test_timeout::test_terminate_child_on_receiving_terminate
test test_uu_timeout_terminate_child_on_receiving_terminate { |ctx|
  let s = uu.scene(ctx)?
  let r = interrupt(s, ["10", "sh", "-c", "trap 'echo child received TERM' TERM; sleep 5"], "TERM")?
  uu.fails_with_code(r, 143)
  uu.stdout_contains(r, "child received TERM")
}

# origin: uutils test_timeout::test_verbose
test test_uu_timeout_verbose { |ctx|
  let s = uu.scene(ctx)?
  for flag in ["-v", "--verbose"] {
    let term = invoke(s, [flag, ".1", "sleep", "1"])?
    uu.fails(term)
    uu.no_stdout(term)
    uu.stderr_contains(term, "timeout: sending signal TERM to command")
    let kill = invoke(s, [flag, "-s0", "-k.1", ".1", "sleep", "1"])?
    uu.fails(kill)
    uu.no_stdout(kill)
    uu.stderr_contains(kill, "timeout: sending signal 0 to command")
    uu.stderr_contains(kill, "timeout: sending signal KILL to command")
  }
}

# origin: uutils test_timeout::test_with_background_child
test test_uu_timeout_with_background_child { |ctx|
  let s = uu.scene(ctx)?
  let r = invoke(s, [".5", "sh", "-c", "sleep .1 & sleep 2"])?
  uu.fails_with_code(r, 124)
  uu.no_stdout(r)
}

# origin: uutils test_timeout::test_zero_timeout
test test_uu_timeout_zero_timeout { |ctx|
  let s = uu.scene(ctx)?
  let first = invoke(s, ["-v", "0", "sleep", ".1"])?
  uu.succeeds(first)
  uu.no_output(first)
  let second = invoke(s, ["-v", "0", "-s0", "-k0", "sleep", ".1"])?
  uu.fails_with_code(second, 127)
  uu.stderr_only(second, "timeout: failed to run command '-s0': No such file or directory\n")
}
