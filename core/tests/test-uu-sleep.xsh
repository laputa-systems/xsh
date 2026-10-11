##! Native ports of the uutils sleep integration tests.

use support.uu as uu

# Process measurements use a monotonic elapsed counter and keep both streams.
proc timed_sleep(s: uu.Scene, args: List[Str], minimum_ns: Int) [fs, process, env, time, error] {
  let timing = time.measure(uu.command(s, "sleep", args)?)?
  assert timing.status.exited_with(0)
  assert timing.wall_ns >= minimum_ns
  assert uu.at(s, ".uu-stdout").read_bytes()? == b""
  assert uu.at(s, ".uu-stderr").read_bytes()? == b""
}

# A long sleep must remain alive until the caller's signal terminates it.
proc signaled_sleep(s: uu.Scene, args: List[Str], signal: Str, expected: Int) [fs, process, env, time, error] {
  let plan = uu.command(s, "sleep", args, timeout: 10s)?
  let child = spawn plan?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  time.sleep(100ms)?
  process.kill(child.pid, signal: signal)?
  let completed = process.wait_timeout([child], 10s)?
  assert completed != null, "sleep did not terminate after its signal"
  assert completed.status.signaled()
  assert completed.status.signal_number()? == expected
  assert uu.at(s, ".uu-stdout").read_bytes()? == b""
  assert uu.at(s, ".uu-stderr").read_bytes()? == b""
}

# origin: uutils test_sleep::test_sleep_no_suffix
test test_uu_sleep_sleep_no_suffix { |ctx|
  let s = uu.scene(ctx)?
  timed_sleep(s, ["0.1"], 100000000)
}

# origin: uutils test_sleep::test_sleep_s_suffix
test test_uu_sleep_sleep_s_suffix { |ctx|
  let s = uu.scene(ctx)?
  timed_sleep(s, ["0.1s"], 100000000)
}

# origin: uutils test_sleep::test_sleep_m_suffix
test test_uu_sleep_sleep_m_suffix { |ctx|
  let s = uu.scene(ctx)?
  timed_sleep(s, ["0.01m"], 600000000)
}

# origin: uutils test_sleep::test_sleep_h_suffix
test test_uu_sleep_sleep_h_suffix { |ctx|
  let s = uu.scene(ctx)?
  timed_sleep(s, ["0.0001h"], 360000000)
}

# origin: uutils test_sleep::test_sleep_sum_duration_same_suffix
test test_uu_sleep_sleep_sum_duration_same_suffix { |ctx|
  let s = uu.scene(ctx)?
  timed_sleep(s, ["0.1s", "0.1s"], 200000000)
}

# origin: uutils test_sleep::test_sleep_sum_duration_different_suffix
test test_uu_sleep_sleep_sum_duration_different_suffix { |ctx|
  let s = uu.scene(ctx)?
  timed_sleep(s, ["0.1s", "0.01m"], 700000000)
}

# origin: uutils test_sleep::test_sleep_sum_duration_many
test test_uu_sleep_sleep_sum_duration_many { |ctx|
  let s = uu.scene(ctx)?
  timed_sleep(s, ["0.1s", "0.1s", "0.3s", "0.4s"], 900000000)
}

# origin: uutils test_sleep::test_invalid_time_interval
test test_uu_sleep_invalid_time_interval { |ctx|
  let s = uu.scene(ctx)?
  let text = uu.invoke(s, "sleep", ["xyz"])?
  uu.fails(text)
  uu.stderr_is(text, "sleep: invalid time interval 'xyz'\nTry 'sleep --help' for more information.\n")
  let negative = uu.invoke(s, "sleep", ["--", "-1"])?
  uu.fails(negative)
  uu.stderr_is(negative, "sleep: invalid time interval '-1'\nTry 'sleep --help' for more information.\n")
}

# origin: uutils test_sleep::test_negative_interval
test test_uu_sleep_negative_interval { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "-1"])?
  uu.fails(r)
  uu.stderr_is(r, "sleep: invalid time interval '-1'\nTry 'sleep --help' for more information.\n")
}

# origin: uutils test_sleep::test_sleep_negative_duration
test test_uu_sleep_sleep_negative_duration { |ctx|
  let s = uu.scene(ctx)?
  for arg in ["-1", "-1s", "-1m", "-1h", "-1d"] {
    uu.fails(uu.invoke(s, "sleep", [arg])?)
  }
}

# origin: uutils test_sleep::test_sleep_zero_duration
test test_uu_sleep_sleep_zero_duration { |ctx|
  let s = uu.scene(ctx)?
  for arg in ["0", "0s", "0m", "0h", "0d"] {
    let r = uu.invoke(s, "sleep", [arg])?
    uu.succeeds(r)
    uu.stdout_only(r, "")
  }
}

# origin: uutils test_sleep::test_sleep_wrong_time
test test_uu_sleep_sleep_wrong_time { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["0.1s", "abc"])?
  uu.fails(r)
}

# origin: uutils test_sleep::test_sleep_when_multiple_input_some_with_error_then_shows_all_errors
test test_uu_sleep_sleep_when_multiple_input_some_with_error_then_shows_all_errors { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["abc", "100000.0", "1years", " "], timeout: 10s)?
  uu.fails(r)
  uu.stderr_is(r, "sleep: invalid time interval 'abc'\nsleep: invalid time interval '1years'\nsleep: invalid time interval ' '\nTry 'sleep --help' for more information.\n")
}

# origin: uutils test_sleep::test_sleep_stops_after_sigsegv
test test_uu_sleep_sleep_stops_after_sigsegv { |ctx|
  let s = uu.scene(ctx)?
  signaled_sleep(s, ["100"], "SEGV", 11)
}

# origin: uutils test_sleep::test_sleep_stops_after_sigbus
test test_uu_sleep_sleep_stops_after_sigbus { |ctx|
  let s = uu.scene(ctx)?
  signaled_sleep(s, ["100"], "BUS", 7)
}

# origin: uutils test_sleep::test_sleep_when_single_input_exceeds_max_duration_then_no_error
test test_uu_sleep_sleep_when_single_input_exceeds_max_duration_then_no_error { |ctx|
  let s = uu.scene(ctx)?
  signaled_sleep(s, ["18446744073709551616"], "KILL", 9)
}

# origin: uutils test_sleep::test_sleep_when_multiple_inputs_exceed_max_duration_then_no_error
test test_uu_sleep_sleep_when_multiple_inputs_exceed_max_duration_then_no_error { |ctx|
  let s = uu.scene(ctx)?
  signaled_sleep(s, ["18446744073709551615", "1"], "KILL", 9)
}

# origin: uutils test_sleep::test_uchild_when_kill_and_timeout_higher_than_kill_time_then_no_panic
test test_uu_sleep_uchild_when_kill_and_timeout_higher_than_kill_time_then_no_panic { |ctx|
  let s = uu.scene(ctx)?
  let plan = uu.command(s, "sleep", ["20.0"], timeout: 60s)?
  let child = spawn plan?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  process.kill(child.pid, signal: "KILL")?
  let completed = process.wait_timeout([child], 5s)?
  assert completed != null, "sleep was still alive after kill within five seconds"
}

# origin: uutils test_sleep::test_ucommand_when_run_with_timeout_higher_then_execution_time_then_no_panic
test test_uu_sleep_ucommand_when_run_with_timeout_higher_then_execution_time_then_no_panic { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["1.0"], timeout: 60s)?
  uu.succeeds(r)
}

# origin: uutils test_sleep::test_invalid_duration::case_1_negative
test test_uu_sleep_test_invalid_duration_case_1_negative { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "-0x1"])?
  uu.fails(r)
  uu.stderr_is(r, "sleep: invalid time interval '-0x1'\nTry 'sleep --help' for more information.\n")
}

# origin: uutils test_sleep::test_invalid_duration::case_2_negative_suffix
test test_uu_sleep_test_invalid_duration_case_2_negative_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "-0x1s"])?
  uu.fails(r)
  uu.stderr_is(r, "sleep: invalid time interval '-0x1s'\nTry 'sleep --help' for more information.\n")
}

# origin: uutils test_sleep::test_invalid_duration::case_3_negative_frac_suffix
test test_uu_sleep_test_invalid_duration_case_3_negative_frac_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "-0x0.1s"])?
  uu.fails(r)
  uu.stderr_is(r, "sleep: invalid time interval '-0x0.1s'\nTry 'sleep --help' for more information.\n")
}

# origin: uutils test_sleep::test_invalid_duration::case_4_wrong_capitalization
test test_uu_sleep_test_invalid_duration_case_4_wrong_capitalization { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "infD"])?
  uu.fails(r)
  uu.stderr_is(r, "sleep: invalid time interval 'infD'\nTry 'sleep --help' for more information.\n")
}

# origin: uutils test_sleep::test_invalid_duration::case_5_wrong_capitalization
test test_uu_sleep_test_invalid_duration_case_5_wrong_capitalization { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "INFD"])?
  uu.fails(r)
  uu.stderr_is(r, "sleep: invalid time interval 'INFD'\nTry 'sleep --help' for more information.\n")
}

# origin: uutils test_sleep::test_invalid_duration::case_6_wrong_capitalization
test test_uu_sleep_test_invalid_duration_case_6_wrong_capitalization { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "iNfD"])?
  uu.fails(r)
  uu.stderr_is(r, "sleep: invalid time interval 'iNfD'\nTry 'sleep --help' for more information.\n")
}

# origin: uutils test_sleep::test_sleep_when_input_has_leading_whitespace_then_no_error::case_1_whitespace_prefix
test test_uu_sleep_test_sleep_when_input_has_leading_whitespace_then_no_error_case_1_whitespace_prefix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", [" 0.1s"], timeout: 10s)?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_sleep::test_sleep_when_input_has_leading_whitespace_then_no_error::case_2_multiple_whitespace_prefix
test test_uu_sleep_test_sleep_when_input_has_leading_whitespace_then_no_error_case_2_multiple_whitespace_prefix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["   0.1s"], timeout: 10s)?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_sleep::test_sleep_when_input_has_only_whitespace_then_error::case_1_only_space
test test_uu_sleep_test_sleep_when_input_has_only_whitespace_then_error_case_1_only_space { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", [" "], timeout: 10s)?
  uu.fails(r)
  uu.stderr_is(r, "sleep: invalid time interval ' '\nTry 'sleep --help' for more information.\n")
}

# origin: uutils test_sleep::test_sleep_when_input_has_trailing_whitespace_then_error::case_1_whitespace_suffix
test test_uu_sleep_test_sleep_when_input_has_trailing_whitespace_then_error_case_1_whitespace_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["0.1s "], timeout: 10s)?
  uu.fails(r)
  uu.stderr_is(r, "sleep: invalid time interval '0.1s '\nTry 'sleep --help' for more information.\n")
}

# origin: uutils test_sleep::test_valid_hex_duration::case_1_int
test test_uu_sleep_test_valid_hex_duration_case_1_int { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "0x0"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_sleep::test_valid_hex_duration::case_2_negative_zero
test test_uu_sleep_test_valid_hex_duration_case_2_negative_zero { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "-0x0"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_sleep::test_valid_hex_duration::case_3_int_suffix
test test_uu_sleep_test_valid_hex_duration_case_3_int_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "0x0s"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_sleep::test_valid_hex_duration::case_4_int_suffix
test test_uu_sleep_test_valid_hex_duration_case_4_int_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "0x0h"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_sleep::test_valid_hex_duration::case_5_frac
test test_uu_sleep_test_valid_hex_duration_case_5_frac { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "0x0.1"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_sleep::test_valid_hex_duration::case_6_frac_suffix
test test_uu_sleep_test_valid_hex_duration_case_6_frac_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "0x0.1s"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_sleep::test_valid_hex_duration::case_7_frac_suffix
test test_uu_sleep_test_valid_hex_duration_case_7_frac_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "0x0.001h"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_sleep::test_valid_hex_duration::case_8_scientific
test test_uu_sleep_test_valid_hex_duration_case_8_scientific { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "0x1.0p-3"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# origin: uutils test_sleep::test_valid_hex_duration::case_9_scientific_suffix
test test_uu_sleep_test_valid_hex_duration_case_9_scientific_suffix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "sleep", ["--", "0x1.0p-4s"])?
  uu.succeeds(r)
  uu.no_output(r)
}

# Measuring the owner process uses monotonic elapsed time while it waits for
# its live sleep child, then kills and reaps that child before returning.
proc timed_wait_child(s: uu.Scene, limit: Str, minimum_ns: Int) [fs, process, env, time, error] {
  let script = uu.at(s, "wait-owner.xsh")
  let source = r"""proc main(...argv: List[Str]) [fs, process, error] {
  let plan = process.command_argv(argv[0], argv, p".", {}, b"", p"sleep-out", p"sleep-err")
  let child = spawn plan?
  defer child.cancel(signal: "KILL", kill_after: 0ms)
  let completed = process.wait_timeout([child], WAIT_LIMIT)?
  assert completed == null, "sleep finished before the wait interval elapsed"
  process.kill(child.pid, signal: "KILL")?
  let killed = process.wait_timeout([child], 5s)?
  assert killed != null
  assert killed.status.signal_number()? == 9
  assert p"sleep-out".read_bytes()? == b""
  assert p"sleep-err".read_bytes()? == b""
}
"""
  script.write(source.replace("WAIT_LIMIT", with: limit))?
  let sleep_words = uu.argv(s, "sleep", [p"10.0"])?
  let words = [s.ctx.xsh_bin, script, p"--"].extend(sleep_words)
  let timing = time.measure(process.command_argv(s.ctx.xsh_bin, words, s.root, {}, b"", uu.at(s, "owner-out"), uu.at(s, "owner-err")))?
  assert timing.status.exited_with(0), uu.at(s, "owner-err").read_text()?
  assert timing.wall_ns >= minimum_ns
  assert uu.at(s, "owner-out").read_bytes()? == b""
  assert uu.at(s, "owner-err").read_bytes()? == b""
}

test test_sleep_child_remains_alive_through_one_second_wait { |ctx|
  let s = uu.scene(ctx)?
  timed_wait_child(s, "1s", 1000000000)
}

test test_sleep_child_remains_alive_through_one_point_one_second_wait { |ctx|
  let s = uu.scene(ctx)?
  timed_wait_child(s, "1100ms", 1100000000)
}
