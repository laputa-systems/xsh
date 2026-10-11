type Ran = {status: Int, stdout: Str, stderr: Str}

# Runs core/nice.xsh by its real path (so the invoked name is `nice` and
# `lib.gnu` resolves beside it), capturing both streams to files.
proc nice_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "nice")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/nice.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err)
  let status = process.run(plan)?

  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

# A command that reports the niceness it runs with: another xsh process.
proc reporter(ctx: TestContext) [fs, process, error] -> Result[List[Str]] {
  let root = test.temp_dir(ctx, name: "reporter")?
  let script = fp"{root}/report.xsh"
  script.write("print f\"{process.priority()?}\"\n")

  Ok([ctx.xsh_bin.display(), script.display()])
}

pure clamp(value: Int) -> Int {
  return -20 when value < -20
  return 19 when value > 19

  value
}

proc reported(ctx: TestContext, options: List[Str]) [fs, process, error] -> Result[Str] {
  let result = nice_run(ctx, options.extend(reporter(ctx)?))?

  assert result.status == 0, result.stderr
  assert result.stderr == "" or result.stderr.ends_with("Permission denied\n"), result.stderr

  Ok(result.stdout.trim())
}

test test_nice_without_a_command_prints_the_current_niceness { |ctx|
  let result = nice_run(ctx, [])?
  assert result.status == 0
  assert result.stderr == ""
  assert result.stdout == f"{process.priority()?}\n"
}

test test_nice_default_adjustment_is_ten { |ctx|
  assert reported(ctx, [])? == f"{clamp(process.priority()? + 10)}"
}

test test_nice_adjusts_by_the_given_amount_in_every_spelling { |ctx|
  let base = process.priority()?
  let up = f"{clamp(base + 3)}"

  for spelling in [
    ["-n", "3"],
    ["-n3"],
    ["-n", "+3"],
    ["--adjustment=3"],
    ["--adjustment", "3"],
    ["--adj=3"],
    ["--a", "3"],
    ["-3"],
    ["-+3"],
    ["-n", " 3"],
  ] {
    assert reported(ctx, spelling)? == up, spelling.join(" ")
  }
}

test test_nice_legacy_double_dash_is_a_negative_adjustment { |ctx|
  # Lowering niceness needs privilege, so the result is only checked against
  # what the process could have reached either way.
  let base = process.priority()?
  let result = nice_run(ctx, ["--2"].extend(reporter(ctx)?))?
  assert result.status == 0, result.stderr

  let seen = result.stdout.trim().parse_int() ?? 99
  assert seen == clamp(base - 2) or seen == base, f"niceness {seen}"
}

test test_nice_last_adjustment_wins_and_options_may_repeat { |ctx|
  let base = process.priority()?
  assert reported(ctx, ["-n", "1", "-n", "2"])? == f"{clamp(base + 2)}"
  assert reported(ctx, ["-4", "-n", "1"])? == f"{clamp(base + 1)}"
  assert reported(ctx, ["-n", "1", "-4"])? == f"{clamp(base + 4)}"
}

test test_nice_out_of_range_adjustments_are_clamped_not_rejected { |ctx|
  let base = process.priority()?
  assert reported(ctx, ["-n", "99999999999999999999999999999999999999999999999999"])? == "19"
  assert reported(ctx, ["-n", "0000000000000000000000000000000005"])? == f"{clamp(base + 5)}"

  let low = nice_run(ctx, ["-n", "-9999999999"].extend(reporter(ctx)?))?
  assert low.status == 0, low.stderr
}

test test_nice_invalid_adjustment_is_reported_without_a_hint { |ctx|
  for bad in ["x", "5x", "-2+4", "4 ", "", "1.5", "0x10"] {
    let result = nice_run(ctx, ["-n", bad, "true"])?
    assert result.status == 125, bad
    assert result.stdout == ""
    assert result.stderr == f"nice: invalid adjustment '{bad}'\n", result.stderr
  }
}

test test_nice_adjustment_without_a_command_is_a_usage_error { |ctx|
  for args in [["-n", "19"], ["-5"], ["--adjustment=1"]] {
    let result = nice_run(ctx, args)?
    assert result.status == 125, args.join(" ")
    assert result.stderr == "nice: a command must be given with an adjustment\nTry 'nice --help' for more information.\n", result.stderr
  }
}

test test_nice_option_errors_exit_125_with_getopt_wording { |ctx|
  let short = nice_run(ctx, ["-x"])?
  assert short.status == 125
  assert short.stderr == "nice: invalid option -- 'x'\nTry 'nice --help' for more information.\n", short.stderr

  let long = nice_run(ctx, ["--invalid"])?
  assert long.status == 125
  assert long.stderr == "nice: unrecognized option '--invalid'\nTry 'nice --help' for more information.\n", long.stderr

  let missing = nice_run(ctx, ["-n", "1", "-n"])?
  assert missing.status == 125
  assert missing.stderr == "nice: option requires an argument -- 'n'\nTry 'nice --help' for more information.\n", missing.stderr

  let long_missing = nice_run(ctx, ["--adjustment"])?
  assert long_missing.status == 125
  assert long_missing.stderr == "nice: option '--adjustment' requires an argument\nTry 'nice --help' for more information.\n", long_missing.stderr
}

test test_nice_command_arguments_are_not_options { |ctx|
  let echoed = nice_run(ctx, ["-n", "19", "echo", "-n", "a", "b"])?
  assert echoed.status == 0
  assert echoed.stdout == "a b"

  let bare = nice_run(ctx, ["echo", "-n", "a"])?
  assert bare.stdout == "a"

  let ended = nice_run(ctx, ["-n", "1", "--", "echo", "-n", "x"])?
  assert ended.stdout == "x"

  let plain = nice_run(ctx, ["--", "echo", "plain"])?
  assert plain.stdout == "plain\n"
}

test test_nice_replaces_itself_so_the_command_status_is_the_exit_status { |ctx|
  assert nice_run(ctx, ["-n", "0", "sh", "-c", "exit 7"])?.status == 7
  assert nice_run(ctx, ["true"])?.status == 0
  assert nice_run(ctx, ["false"])?.status == 1
}

test test_nice_missing_and_unrunnable_commands { |ctx|
  let missing = nice_run(ctx, ["/definitely/not/here"])?
  assert missing.status == 127
  assert missing.stderr == "nice: '/definitely/not/here': No such file or directory\n", missing.stderr

  # A missing name is distinct from a denied search through an inherited PATH.
  let search_dir = test.temp_dir(ctx, name: "empty-command-path")?
  let named = env ({PATH: search_dir}) { nice_run(ctx, ["no-such-command-anywhere"])? }?
  assert named.status == 127
  assert named.stderr == "nice: 'no-such-command-anywhere': No such file or directory\n", named.stderr

  let directory = nice_run(ctx, ["/"])?
  assert directory.status == 126
  assert directory.stderr == "nice: '/': Permission denied\n", directory.stderr
}

test test_nice_refused_change_warns_and_still_runs_the_command { |ctx|
  if unix.id()?.euid == 0 {
    test.skip("root may lower niceness; no refusal to observe")
  }

  let result = nice_run(ctx, ["-n", "-20", "echo", "ran"])?
  assert result.status == 0
  assert result.stdout == "ran\n"
  assert result.stderr == "nice: cannot set niceness: Permission denied\n", result.stderr
}

test test_nice_help_and_version_go_to_stdout { |ctx|
  let help = nice_run(ctx, ["--help"])?
  assert help.status == 0
  assert help.stderr == ""
  assert help.stdout.starts_with("Usage: nice [OPTION] [COMMAND [ARG]...]")

  let version = nice_run(ctx, ["--version"])?
  assert version.status == 0
  assert version.stdout.starts_with("nice ")
}

test test_nice_command_tokens_that_look_like_adjustments_survive { |ctx|
  let result = nice_run(ctx, ["-n", "0", "printf", "%s|%s", "--19", "-n3"])?
  assert result.status == 0
  assert result.stdout == "--19|-n3"
  assert result.stderr == ""
}

test test_nice_nonexecutable_path_match_is_126 { |ctx|
  let root = test.temp_dir(ctx, name: "path-permission")?
  let blocked = fp"{root}/blocked"
  blocked.write("printf must-not-run\n")
  blocked.chmod(384)?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/nice.xsh"
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), script.display(), "blocked"], root, {PATH: root, LC_ALL: "C"}, b"", out, err)
  assert process.run(plan)?.shell_code()? == 126
  assert out.read_text()? == ""
  assert err.read_text()? == "nice: 'blocked': Permission denied\n"
}

# The privilege warning must be written; when it cannot be, the command is not run.
test test_nice_failed_advisory_write_stops_the_command { |ctx|
  if unix.id()?.euid == 0 { test.skip("an unprivileged user is needed for a refused niceness change") }
  if ! p"/dev/full".exists() { test.skip("/dev/full is not available") }
  let root = test.temp_dir(ctx, name: "nice-full")?
  let out = fp"{root}/stdout"
  let status = process.run(process.command_argv(ctx.xsh_bin,
    [ctx.xsh_bin.display(), fp"{ctx.core_dir}/nice.xsh".display(), "-n", "-1", "nice"],
    root, {LC_ALL: "C"}, b"", out, p"/dev/full"))?
  assert status.exited_with(125)
  assert out.read_text()? == ""
}
