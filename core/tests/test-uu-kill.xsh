##! Transcribed signal listing and delivery tests from the uutils coreutils suite.

use support.uu as uu

# The owned handle kills and reaps a failed test's target; sleep's natural
# thirty-second exit also bounds a missing delivery before the signal assertion.
proc target() [process, error] -> Result[ProcessHandle, Error] {
  let child = spawn run sleep 30 ?
  Ok(child)
}

proc signaled(child: ProcessHandle, number: Int) [process, error] {
  let status = wait child?
  assert status.signal_number()? == number
}

# Read the kernel mask because a libc can reserve numbers that GNU names.
# One nibble avoids overflowing an Int when signal 64 is ignored.
proc ignored(number: Int) [fs, error] -> Result[Bool, Error] {
  for line in p"/proc/self/status".lines()? {
    if line.starts_with("SigIgn:") {
      let mask = line.fields()[1]
      let offset = (number - 1) / 4
      let digit = mask.byte_slice(mask.byte_len() - 1 - offset, length: 1)
      let value = f"0x{digit}".parse_int()?
      let shift = [1, 2, 4, 8][(number - 1) % 4]
      return Ok(value / shift % 2 == 1)
    }
  }
  assert false, "the process status has no ignored-signal mask"
  Ok(false)
}

# The test runner and applet can use different libcs, so read these bounds from
# Bash, which is linked to the target container's libc.
proc realtime_bound(name: Str) [process, error] -> Result[Int, Error] {
  let result = run.capture --text "bash" "-c" f"kill -l {name}"
  assert result.status.exited_with(0), result.stderr
  Ok(result.stdout.trim().parse_int()?)
}

# origin: uutils test_kill::test_invalid_arg
test test_uu_kill_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
  
}

# origin: uutils test_kill::test_kill_list_all_signals
test test_uu_kill_kill_list_all_signals { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-l"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "KILL")
  uu.stdout_contains(r, "TERM")
  uu.stdout_contains(r, "HUP")
  uu.stdout_contains(r, "EXIT")
  
}

# origin: uutils test_kill::test_kill_list_all_signals_as_table
test test_uu_kill_kill_list_all_signals_as_table { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-t"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "KILL")
  uu.stdout_contains(r, "TERM")
  uu.stdout_contains(r, "HUP")
  uu.stdout_contains(r, "EXIT")
  
}

# origin: uutils test_kill::test_kill_list_contains_realtime_signals
test test_uu_kill_kill_list_contains_realtime_signals { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-l"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "RTMIN")
  uu.stdout_contains(r, "RTMAX")
  
}

# origin: uutils test_kill::test_kill_table_contains_realtime_signals
test test_uu_kill_kill_table_contains_realtime_signals { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-t"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "RTMIN")
  uu.stdout_contains(r, "RTMAX")
  
}

# origin: uutils test_kill::test_kill_list_final_new_line
test test_uu_kill_kill_list_final_new_line { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-l"])?
  uu.succeeds(r)
  
  assert rx"\n$".matches(r.stdout.utf8()?)
}

# origin: uutils test_kill::test_kill_table_starts_at_0
test test_uu_kill_kill_table_starts_at_0 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-t"])?
  uu.succeeds(r)
  
  assert rx"^\s?0\sEXIT".matches(r.stdout.utf8()?)
}

# origin: uutils test_kill::test_kill_table_lists_all_vertically
test test_uu_kill_kill_table_lists_all_vertically { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-t"])?
  uu.succeeds(r)
  
  let signals = collect { for line in r.stdout.utf8()?.split("\n") { let fields = line.trim().split(" "); yield fields[1] when fields.len() > 1 } }
  assert "KILL" in signals
  assert "TERM" in signals
  assert "HUP" in signals
  assert "EXIT" in signals
}

# origin: uutils test_kill::test_kill_list_all_vertically
test test_uu_kill_kill_list_all_vertically { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-l"])?
  uu.succeeds(r)
  
  let signals = r.stdout.utf8()?.split("\n")
  assert "KILL" in signals
  assert "TERM" in signals
  assert "HUP" in signals
  assert "EXIT" in signals
}

# origin: uutils test_kill::test_kill_list_one_signal_from_number
test test_uu_kill_kill_list_one_signal_from_number { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-l", "9"])?
  uu.succeeds(r)
  uu.stdout_only(r, "KILL\n")
}

# origin: uutils test_kill::test_kill_list_rtmax_from_name
test test_uu_kill_kill_list_rtmax_from_name { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-l", "RTMAX"])?
  uu.succeeds(r)
  uu.stdout_only(r, "64\n")
}

# origin: uutils test_kill::test_kill_list_sigio_alias
test test_uu_kill_kill_list_sigio_alias { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "kill", ["-l", "IO"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "29\n")
  let r1 = uu.invoke(s, "kill", ["-l", "SIGIO"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "29\n")
}

# origin: uutils test_kill::test_kill_list_one_signal_from_name
test test_uu_kill_kill_list_one_signal_from_name { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-l", "KILL"])?
  uu.succeeds(r)
  
  assert rx"\b9\b".matches(r.stdout.utf8()?)
}

# origin: uutils test_kill::test_kill_list_one_signal_ignore_case
test test_uu_kill_kill_list_one_signal_ignore_case { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-l", "KiLl"])?
  uu.succeeds(r)
  
  assert rx"\b9\b".matches(r.stdout.utf8()?)
}

# origin: uutils test_kill::test_kill_list_one_signal_from_invalid_number
test test_uu_kill_kill_list_one_signal_from_invalid_number { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-l", "99"])?
  uu.fails(r)
  uu.stderr_contains(r, "'99': invalid signal")
  
}

# origin: uutils test_kill::test_kill_list_unknown_must_match_input_case
test test_uu_kill_kill_list_unknown_must_match_input_case { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-l", "IaMnOtAsIgNaL"])?
  uu.fails(r)
  uu.stderr_contains(r, "IaMnOtAsIgNaL")
  
}

# origin: uutils test_kill::test_kill_list_two_signal_from_name
test test_uu_kill_kill_list_two_signal_from_name { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-l", "INT", "KILL"])?
  uu.succeeds(r)
  
  assert rx"\d\n\d".matches(r.stdout.utf8()?)
}

# origin: uutils test_kill::test_kill_list_three_signal_first_unknown
test test_uu_kill_kill_list_three_signal_first_unknown { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-l", "IAMNOTASIGNAL", "INT", "KILL"])?
  uu.fails(r)
  uu.stderr_contains(r, "'IAMNOTASIGNAL': invalid signal")
  
  assert rx"\d\n\d".matches(r.stdout.utf8()?)
}

# origin: uutils test_kill::test_kill_set_bad_signal_name
test test_uu_kill_kill_set_bad_signal_name { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-s", "IAMNOTASIGNAL"])?
  uu.fails(r)
  uu.stderr_contains(r, "'IAMNOTASIGNAL': invalid signal")
  
}

# origin: uutils test_kill::test_kill_accepts_sigio_alias_for_sending
test test_uu_kill_kill_accepts_sigio_alias_for_sending { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "kill", ["--signal=SIGIO", "999999999"])?
  uu.fails_with_code(r0, 1)
  uu.stderr_contains(r0, "kill: '999999999': No such process")
  assert !("invalid signal" in r0.stderr.utf8()?)
  let r1 = uu.invoke(s, "kill", ["-SIGIO", "999999999"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "kill: '999999999': No such process")
  assert !("invalid signal" in r1.stderr.utf8()?)
}

# origin: uutils test_kill::test_kill_with_default_signal
test test_uu_kill_kill_with_default_signal { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", [f"{child.pid}"])?
  uu.succeeds(r)
  signaled(child, 15)
}

# origin: uutils test_kill::test_kill_with_signal_number_old_form
test test_uu_kill_kill_with_signal_number_old_form { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-9", f"{child.pid}"])?
  uu.succeeds(r)
  signaled(child, 9)
}

# origin: uutils test_kill::test_kill_with_signal_prefixed_name_old_form
test test_uu_kill_kill_with_signal_prefixed_name_old_form { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-SIGKILL", f"{child.pid}"])?
  uu.succeeds(r)
  signaled(child, 9)
}

# origin: uutils test_kill::test_kill_with_signal_number_new_form
test test_uu_kill_kill_with_signal_number_new_form { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-s", "9", f"{child.pid}"])?
  uu.succeeds(r)
  signaled(child, 9)
}

# origin: uutils test_kill::test_kill_with_signal_name_new_form
test test_uu_kill_kill_with_signal_name_new_form { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-s", "KILL", f"{child.pid}"])?
  uu.succeeds(r)
  signaled(child, 9)
}

# origin: uutils test_kill::test_kill_with_signal_name_new_form_ignore_case
test test_uu_kill_kill_with_signal_name_new_form_ignore_case { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-s", "KiLl", f"{child.pid}"])?
  uu.succeeds(r)
  signaled(child, 9)
}

# origin: uutils test_kill::test_kill_with_signal_prefixed_name_new_form
test test_uu_kill_kill_with_signal_prefixed_name_new_form { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-s", "SIGKILL", f"{child.pid}"])?
  uu.succeeds(r)
  signaled(child, 9)
}

# origin: uutils test_kill::test_kill_with_signal_prefixed_name_new_form_ignore_case
test test_uu_kill_kill_with_signal_prefixed_name_new_form_ignore_case { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-s", "SiGKiLl", f"{child.pid}"])?
  uu.succeeds(r)
  signaled(child, 9)
}

# origin: uutils test_kill::test_kill_with_signal_number_hidden_compatibility_option
test test_uu_kill_kill_with_signal_number_hidden_compatibility_option { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-n", "9", f"{child.pid}"])?
  uu.succeeds(r)
  signaled(child, 9)
}

# origin: uutils test_kill::test_kill_with_signal_name_old_form
test test_uu_kill_kill_with_signal_name_old_form { |ctx|
  let s = uu.scene(ctx)?
  for arg in ["-Kill", "-KILL"] {
    let child = target()?
    let r = uu.invoke(s, "kill", [arg, f"{child.pid}"])?
    uu.succeeds(r)
    signaled(child, 9)
  }
}

# origin: uutils test_kill::test_kill_with_lower_case_signal_name_old_form
test test_uu_kill_kill_with_lower_case_signal_name_old_form { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-kill", f"{child.pid}"])?
  uu.fails(r)
  uu.stderr_contains(r, "invalid option -- 'k'")
  child.cancel(signal: "KILL")
}

# origin: uutils test_kill::test_kill_with_signal_name_new_form_unknown_must_match_input_case
test test_uu_kill_kill_with_signal_name_new_form_unknown_must_match_input_case { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-s", "IaMnOtAsIgNaL", f"{child.pid}"])?
  uu.fails(r)
  uu.stderr_contains(r, "'IaMnOtAsIgNaL': invalid signal")
  child.cancel(signal: "KILL")
}

# origin: uutils test_kill::test_kill_with_signal_exit_new_form
test test_uu_kill_kill_with_signal_exit_new_form { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-s", "EXIT", f"{child.pid}"])?
  uu.succeeds(r)
  child.cancel(signal: "KILL")
}

# origin: uutils test_kill::test_kill_with_signal_and_list
test test_uu_kill_kill_with_signal_and_list { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-s", "EXIT", f"{child.pid}", "-l"])?
  uu.fails(r)
  child.cancel(signal: "KILL")
}

# origin: uutils test_kill::test_kill_with_signal_and_table
test test_uu_kill_kill_with_signal_and_table { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-s", "EXIT", f"{child.pid}", "-t"])?
  uu.fails(r)
  child.cancel(signal: "KILL")
}

# origin: uutils test_kill::test_kill_with_list_lower_bits
test test_uu_kill_kill_with_list_lower_bits { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "kill", ["-l", "128"])?
  uu.succeeds(r0)
  uu.stdout_contains(r0, "EXIT")
  
  let r1 = uu.invoke(s, "kill", ["-l", "143"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "TERM")
  
  let r2 = uu.invoke(s, "kill", ["-l", "256"])?
  uu.succeeds(r2)
  uu.stdout_contains(r2, "EXIT")
  
  let r3 = uu.invoke(s, "kill", ["-l", "2304"])?
  uu.succeeds(r3)
  uu.stdout_contains(r3, "EXIT")
  
}

# origin: uutils test_kill::test_kill_with_list_lower_bits_unrecognized
test test_uu_kill_kill_with_list_lower_bits_unrecognized { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "kill", ["-l", "111"])?
  uu.fails(r0)
  
  let r1 = uu.invoke(s, "kill", ["-l", "384"])?
  uu.fails(r1)
  
}

# origin: uutils test_kill::test_kill_with_list_unnamed_signal_numbers
test test_uu_kill_kill_with_list_unnamed_signal_numbers { |ctx|
  let s = uu.scene(ctx)?
  let r32 = uu.invoke(s, "kill", ["-l", "32"])?
  uu.succeeds(r32)
  uu.stdout_only(r32, "32\n")
  let r33 = uu.invoke(s, "kill", ["-l", "33"])?
  uu.succeeds(r33)
  uu.stdout_only(r33, "33\n")
}

# origin: uutils test_kill::test_kill_with_list_all_signal_numbers_up_to_last_named_signal
test test_uu_kill_kill_with_list_all_signal_numbers_up_to_last_named_signal { |ctx|
  let s = uu.scene(ctx)?
  let listed = uu.invoke(s, "kill", ["-l"])?
  uu.succeeds(listed)
  
  let names = listed.stdout.utf8()?.lines()
  let last_signal_name = names[-1]
  let converted = uu.invoke(s, "kill", ["-l", "--", last_signal_name])?
  uu.succeeds(converted)
  let last_signal_number = converted.stdout.utf8()?.trim().parse_int()?
  let args = ["-l", "--"].extend([f"{n}" for n in range(last_signal_number + 1)])
  let r = uu.invoke(s, "kill", args)?
  uu.succeeds(r)
}

# origin: uutils test_kill::test_kill_list_signals_write_error_is_reported
test test_uu_kill_kill_list_signals_write_error_is_reported { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "kill", ["-l"], stdout: p"/dev/full")?
  uu.fails(r0)
  uu.stderr_is(r0, "kill: write error: No space left on device\n")
  let r1 = uu.invoke(s, "kill", ["-l", "TERM"], stdout: p"/dev/full")?
  uu.fails(r1)
  uu.stderr_is(r1, "kill: write error: No space left on device\n")
  let r2 = uu.invoke(s, "kill", ["--list", "9"], stdout: p"/dev/full")?
  uu.fails(r2)
  uu.stderr_is(r2, "kill: write error: No space left on device\n")
  let r3 = uu.invoke(s, "kill", ["--table"], stdout: p"/dev/full")?
  uu.fails(r3)
  uu.stderr_is(r3, "kill: write error: No space left on device\n")
}

# origin: uutils test_kill::test_kill_no_pid_provided
test test_uu_kill_kill_no_pid_provided { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", [])?
  uu.fails(r)
  uu.stderr_contains(r, "no process ID specified")
  
}

# origin: uutils test_kill::test_kill_signal_only_no_pid
test test_uu_kill_kill_signal_only_no_pid { |ctx|
  let s = uu.scene(ctx)?
  let r0 = uu.invoke(s, "kill", ["-1"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "no process ID specified")
  
  let r1 = uu.invoke(s, "kill", ["-9"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "no process ID specified")
  
  let r2 = uu.invoke(s, "kill", ["-TERM"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "no process ID specified")
  
}

# origin: uutils test_kill::test_kill_signal_zero_process
test test_uu_kill_kill_signal_zero_process { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-0", f"{child.pid}"])?
  uu.succeeds(r)
  child.cancel(signal: "KILL")
}

# origin: uutils test_kill::test_kill_signal_zero_new_form
test test_uu_kill_kill_signal_zero_new_form { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-s", "0", f"{child.pid}"])?
  uu.succeeds(r)
  child.cancel(signal: "KILL")
}

# origin: uutils test_kill::test_kill_signal_zero_nonexistent
test test_uu_kill_kill_signal_zero_nonexistent { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-0", "999999999"])?
  uu.fails(r)
  
}

# origin: uutils test_kill::test_kill_signal_zero_current_process_group
test test_uu_kill_kill_signal_zero_current_process_group { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "kill", ["-0", "0"])?
  uu.succeeds(r)
  
}

# origin: uutils test_kill::test_kill_realtime_signal
test test_uu_kill_kill_realtime_signal { |ctx|
  let s = uu.scene(ctx)?
  let rtmin = realtime_bound("RTMIN")?
  if ignored(rtmin) { test.skip("the real-time signal is inherited as ignored") }
  let child = target()?
  let r = uu.invoke(s, "kill", ["-s", "RTMIN", f"{child.pid}"])?
  uu.succeeds(r)
  signaled(child, rtmin)
}

# origin: uutils test_kill::test_kill_with_rtmax_offset
test test_uu_kill_kill_with_rtmax_offset { |ctx|
  let s = uu.scene(ctx)?
  let rtmax = realtime_bound("RTMAX")?
  let number = rtmax - 7
  if ignored(number) { test.skip("the real-time signal is inherited as ignored") }
  let child = target()?
  let r = uu.invoke(s, "kill", ["-s", "SIGRTMAX-7", f"{child.pid}"])?
  uu.succeeds(r)
  signaled(child, number)
}

# origin: uutils test_kill::test_kill_with_rtmin_offset
test test_uu_kill_kill_with_rtmin_offset { |ctx|
  let s = uu.scene(ctx)?
  let rtmin = realtime_bound("RTMIN")?
  let number = rtmin + 7
  if ignored(number) { test.skip("the real-time signal is inherited as ignored") }
  let child = target()?
  let r = uu.invoke(s, "kill", ["-s", "SIGRTMIN+7", f"{child.pid}"])?
  uu.succeeds(r)
  signaled(child, number)
}

# Encoded process statuses also select a signal for the old-form option.
test test_uu_kill_encoded_signal_status { |ctx|
  let s = uu.scene(ctx)?
  let child = target()?
  let r = uu.invoke(s, "kill", ["-129", f"{child.pid}"])?
  uu.succeeds(r)
  signaled(child, 1)
}
