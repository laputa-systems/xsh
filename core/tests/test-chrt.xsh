type Ran = {status: Int, stdout: Str, stderr: Str}

const HELP = """Show or change the real-time scheduling attributes of a process.

Set policy:
 chrt [options] [<priority>] <command> [<argument>...]
 chrt --pid <policy-option> [options] [<priority>] <PID>

Get policy:
 chrt --pid <PID>

Policy options:
 -b, --batch          set policy to SCHED_BATCH
 -d, --deadline       set policy to SCHED_DEADLINE
 -e, --ext            set policy to SCHED_EXT
 -f, --fifo           set policy to SCHED_FIFO
 -i, --idle           set policy to SCHED_IDLE
 -o, --other          set policy to SCHED_OTHER
 -r, --rr             set policy to SCHED_RR (default)

Scheduling options:
 -R, --reset-on-fork       set reset-on-fork flag
 -T, --sched-runtime <ns>  runtime parameter for DEADLINE
 -P, --sched-period <ns>   period parameter for DEADLINE
 -D, --sched-deadline <ns> deadline parameter for DEADLINE

Other options:
 -a, --all-tasks      operate on all the tasks (threads) for a given pid
 -m, --max            show min and max valid priorities
 -p, --pid            operate on existing given pid
 -v, --verbose        display status information

 -h, --help           display this help
 -V, --version        display version

For more details see chrt(1).
"""

# Runs core/chrt.xsh by its real path, so the invoked name is `chrt` and
# `lib.gnu` resolves beside it, capturing both streams.
proc chrt_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "chrt")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/chrt.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err)
  let status = process.run(plan)?

  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

# A command that prints its own pid and the scheduler state of its main thread.
# The script itself runs on a thread the runtime starts, and a thread created
# under reset-on-fork starts with the flag cleared, so thread 0 would not show
# the flag the command was started with.
proc reporter(ctx: TestContext) [fs, process, error] -> Result[List[Str]] {
  let root = test.temp_dir(ctx, name: "reporter")?
  let script = fp"{root}/report.xsh"
  script.write("let pid = process.current_pid()?\nlet info = process.scheduler(pid)?\nprint f\"{pid} {info.policy} {info.priority} {info.reset_on_fork} {info.runtime_ns}\"\n")

  Ok([ctx.xsh_bin.display(), script.display()])
}

# Real-time policies are refused without CAP_SYS_NICE or an rtprio limit.
proc can_use_real_time() [process, error] -> Result[Bool] {
  Ok(unix.id()?.euid == 0 or (process.rlimit("rtprio")?.soft ?? 0) > 0)
}

test test_chrt_max_lists_every_policys_priority_range { |ctx|
  let result = chrt_run(ctx, ["-m"])?
  assert result.status == 0, result.stderr
  assert result.stderr == ""

  let lines = result.stdout.lines()
  assert lines.len() == 7
  assert lines[0] == "SCHED_OTHER min/max priority\t: 0/0"
  assert lines[1] == "SCHED_FIFO min/max priority\t: 1/99"
  assert lines[2] == "SCHED_RR min/max priority\t: 1/99"
  assert lines[3] == "SCHED_BATCH min/max priority\t: 0/0"
  assert lines[4] == "SCHED_IDLE min/max priority\t: 0/0"
  assert lines[5] == "SCHED_DEADLINE min/max priority\t: 0/0"
  # Only a kernel built with sched_ext knows the last policy.
  assert lines[6] in ["SCHED_EXT min/max priority\t: 0/0", "SCHED_EXT not supported?"], lines[6]

  # `-m` wins over everything else on the line.
  let combined = chrt_run(ctx, ["-v", "-f", "-m", "-p", "1"])?
  assert combined.stdout == result.stdout
}

test test_chrt_reports_the_policy_of_a_process { |ctx|
  let handle = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  let child = handle.pid
  defer process.kill(child, signal: "KILL")

  let result = chrt_run(ctx, ["-p", f"{child}"])?
  assert result.status == 0, result.stderr

  let lines = result.stdout.lines()
  assert lines.len() == 3, result.stdout
  assert lines[0] == f"pid {child}'s current scheduling policy: SCHED_OTHER"
  assert lines[1] == f"pid {child}'s current scheduling priority: 0"
  assert lines[2].starts_with(f"pid {child}'s current runtime parameter: "), lines[2]

  let long = chrt_run(ctx, ["--pid", f"{child}"])?
  assert long.stdout == result.stdout

  let own = chrt_run(ctx, ["-p", "0"])?
  assert own.status == 0
  assert own.stdout.lines()[0].ends_with("'s current scheduling policy: SCHED_OTHER")
}

test test_chrt_changes_the_policy_of_another_process { |ctx|
  let handle = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  let child = handle.pid
  defer process.kill(child, signal: "KILL")

  # A fair policy takes no priority argument.
  let batch = chrt_run(ctx, ["-b", "-p", f"{child}"])?
  assert batch.status == 0, batch.stderr
  assert batch.stdout == ""
  assert process.scheduler(child)?.policy == "batch"

  let other = chrt_run(ctx, ["-o", "-p", "0", f"{child}"])?
  assert other.status == 0, other.stderr
  assert process.scheduler(child)?.policy == "other"

  let verbose = chrt_run(ctx, ["-v", "-b", "-p", "0", f"{child}"])?
  assert verbose.status == 0, verbose.stderr

  let lines = verbose.stdout.lines()
  assert lines.len() == 6, verbose.stdout
  assert lines[0] == f"pid {child}'s current scheduling policy: SCHED_OTHER"
  assert lines[1] == f"pid {child}'s current scheduling priority: 0"
  assert lines[2].starts_with(f"pid {child}'s current runtime parameter: ")
  assert lines[3] == f"pid {child}'s new scheduling policy: SCHED_BATCH"
  assert lines[4] == f"pid {child}'s new scheduling priority: 0"
  assert lines[5].starts_with(f"pid {child}'s new runtime parameter: ")

  # Idle has no time slice to report, and the flag shows after a bar.
  let idle = chrt_run(ctx, ["-v", "-R", "-i", "-p", "0", f"{child}"])?
  assert idle.status == 0, idle.stderr
  assert idle.stdout.lines()[3] == f"pid {child}'s new scheduling policy: SCHED_IDLE|SCHED_RESET_ON_FORK"
  assert idle.stdout.lines().len() == 5

  let info = process.scheduler(child)?
  assert info.policy == "idle" and info.reset_on_fork

  let after = chrt_run(ctx, ["-p", f"{child}"])?
  assert after.stdout == f"pid {child}'s current scheduling policy: SCHED_IDLE|SCHED_RESET_ON_FORK\npid {child}'s current scheduling priority: 0\n", after.stdout
}

test test_chrt_runs_a_command_under_the_policy { |ctx|
  let command = reporter(ctx)?

  let batch = chrt_run(ctx, ["-b", "0"].extend(command))?
  assert batch.status == 0, batch.stderr
  assert batch.stdout.trim().split(" ")[1] == "batch"
  assert batch.stdout.trim().split(" ")[3] == "false"

  # A fair policy may leave out the priority; the command then starts the line.
  let bare = chrt_run(ctx, ["-i"].extend(command))?
  assert bare.status == 0, bare.stderr
  assert bare.stdout.trim().split(" ")[1] == "idle"

  let forked = chrt_run(ctx, ["-R", "-o", "0"].extend(command))?
  assert forked.status == 0, forked.stderr
  assert forked.stdout.trim().split(" ")[3] == "true"

  # `-v` names the new policy with the pid the command will have.
  let verbose = chrt_run(ctx, ["-v", "-b", "0"].extend(command))?
  assert verbose.status == 0, verbose.stderr

  let lines = verbose.stdout.lines()
  let pid = lines[1].split(" ")[0]
  assert lines.len() == 2, verbose.stdout
  assert lines[0] == f"pid {pid}'s new scheduling policy: SCHED_BATCH", lines[0]
}

test test_chrt_time_slice_reaches_the_fair_policies { |ctx|
  let handle = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  let child = handle.pid
  defer process.kill(child, signal: "KILL")

  let result = chrt_run(ctx, ["-T", "3000000", "-b", "-p", "0", f"{child}"])?
  assert result.status == 0, result.stderr
  assert process.scheduler(child)?.policy == "batch"

  if process.scheduler(child)?.runtime_ns != 3000000 {
    test.skip("the kernel has no per-task time slice")
  }

  let shown = chrt_run(ctx, ["-p", f"{child}"])?
  assert shown.stdout.lines()[2] == f"pid {child}'s current runtime parameter: 3000000", shown.stdout

  # Changing the policy without a time restores the default slice.
  let reset = chrt_run(ctx, ["-b", "-p", "0", f"{child}"])?
  assert reset.status == 0, reset.stderr
  assert process.scheduler(child)?.runtime_ns != 3000000
}

test test_chrt_real_time_policies_need_privilege { |ctx|
  if can_use_real_time()? {
    test.skip("this process may use real-time policies")
  }

  let denied = chrt_run(ctx, ["-f", "10", "true"])?
  assert denied.status == 1
  assert denied.stderr == "chrt: failed to set pid 0's policy: Operation not permitted\n", denied.stderr

  let handle = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  let child = handle.pid
  defer process.kill(child, signal: "KILL")

  let other = chrt_run(ctx, ["-r", "-p", "5", f"{child}"])?
  assert other.status == 1
  assert other.stderr == f"chrt: failed to set pid {child}'s policy: Operation not permitted\n", other.stderr
}

test test_chrt_priority_arguments_are_checked_against_the_policy { |ctx|
  let range = "see --max for valid range\n"

  let cases = [
    {args: ["-o", "5", "true"], stderr: "chrt: unsupported priority value for the policy: 5: " + range},
    {args: ["-b", "5", "true"], stderr: "chrt: unsupported priority value for the policy: 5: " + range},
    {args: ["-i", "1", "true"], stderr: "chrt: unsupported priority value for the policy: 1: " + range},
    {args: ["-f", "0", "true"], stderr: "chrt: unsupported priority value for the policy: 0: " + range},
    {args: ["-f", "100", "true"], stderr: "chrt: unsupported priority value for the policy: 100: " + range},
    {args: ["-r", "100", "true"], stderr: "chrt: unsupported priority value for the policy: 100: " + range},
    {args: ["-p", "0", "2147483647"], stderr: "chrt: unsupported priority value for the policy: 0: " + range},
    {args: ["-d", "-T", "1000000", "-P", "3000000", "5", "true"], stderr: "chrt: unsupported priority value for the policy: 5: " + range},
  ]

  for case in cases {
    let result = chrt_run(ctx, case.args)?
    assert result.status == 1, case.args.join(" ")
    assert result.stdout == ""
    assert result.stderr == case.stderr, result.stderr
  }
}

test test_chrt_operand_errors { |ctx|
  let hint = "Try 'chrt --help' for more information.\n"

  for args in [[], ["-p"], ["-f"], ["-o"], ["-P", "5", "-o"]] {
    let result = chrt_run(ctx, args)?
    assert result.status == 1, args.join(" ")
    assert result.stderr == f"chrt: too few arguments\n{hint}", result.stderr
  }

  let errors = [
    {args: ["true"], stderr: "chrt: policy SCHED_RR requires a priority argument\n"},
    {args: ["-f", "true"], stderr: "chrt: policy SCHED_FIFO requires a priority argument\n"},
    {args: ["-f", "abc", "true"], stderr: "chrt: policy SCHED_FIFO requires a priority argument\n"},
    {args: ["-f", "1x", "true"], stderr: "chrt: policy SCHED_FIFO requires a priority argument\n"},
    {args: ["-r", "-p", "2147483647"], stderr: "chrt: policy SCHED_RR requires a priority argument\n"},
    {args: ["-f", "-p", "5"], stderr: "chrt: policy SCHED_FIFO requires a priority argument\n"},
    {args: ["10"], stderr: "chrt: no command or priority specified\n"},
    {args: ["-f", "10"], stderr: "chrt: no command or priority specified\n"},
    {args: ["-f", "99999999999", "true"], stderr: "chrt: invalid priority argument: '99999999999': Result not representable\n"},
    {args: ["-f", "-p", "abc", "1"], stderr: "chrt: invalid priority argument: 'abc'\n"},
    {args: ["-p", "abc"], stderr: "chrt: invalid PID argument: 'abc'\n"},
    {args: ["-p", "4294967296"], stderr: "chrt: invalid PID argument: '4294967296': Result not representable\n"},
    {args: ["-T", "abc", "-o", "0", "true"], stderr: "chrt: invalid runtime argument: 'abc'\n"},
    {args: ["-T", "-5", "-o", "0", "true"], stderr: "chrt: invalid runtime argument: '-5': Result not representable\n"},
    {args: ["-T", "100000000000000000000", "-d", "true"], stderr: "chrt: invalid runtime argument: '100000000000000000000': Result not representable\n"},
    {args: ["-P", "x", "-d", "true"], stderr: "chrt: invalid period argument: 'x'\n"},
    {args: ["-D", "x", "-d", "true"], stderr: "chrt: invalid deadline argument: 'x'\n"},
    {args: ["-P", "5", "-o", "0", "true"], stderr: "chrt: --sched-{deadline,period} options are supported for SCHED_DEADLINE only\n"},
    {args: ["-D", "5", "-b", "0", "true"], stderr: "chrt: --sched-{deadline,period} options are supported for SCHED_DEADLINE only\n"},
    {args: ["-T", "5", "-f", "1", "true"], stderr: "chrt: --sched-runtime option is supported for SCHED_OTHER, SCHED_BATCH and SCHED_DEADLINE\n"},
    {args: ["-T", "5", "-i", "0", "true"], stderr: "chrt: --sched-runtime option is supported for SCHED_OTHER, SCHED_BATCH and SCHED_DEADLINE\n"},
    {args: ["-p", "2147483647"], stderr: "chrt: failed to get pid 2147483647's policy: No such process\n"},
    {args: ["-v", "-f", "-p", "100", "2147483647"], stderr: "chrt: failed to get pid 2147483647's policy: No such process\n"},
    {args: ["-o", "-p", "0", "2147483647"], stderr: "chrt: failed to set pid 2147483647's policy: No such process\n"},
    {args: ["-o", "0", "/definitely/not/here"], stderr: "chrt: failed to execute /definitely/not/here: No such file or directory\n"},
  ]

  for case in errors {
    let result = chrt_run(ctx, case.args)?
    assert result.status in [1, 127], case.args.join(" ")
    assert result.stdout == "", case.args.join(" ")
    assert result.stderr == case.stderr, f"{case.args.join(" ")}: {result.stderr}"
  }
}

test test_chrt_deadline_reservations_reach_the_kernel_unchanged { |ctx|
  # A runtime below the kernel's minimum is refused whoever asks, so the
  # kernel's EINVAL shows that the three times were passed on.
  let refused = chrt_run(ctx, ["-d", "-T", "1", "-D", "2", "-P", "3", "true"])?
  assert refused.status == 1
  assert refused.stderr == "chrt: failed to set pid 0's policy: Invalid argument\n", refused.stderr

  let zero = chrt_run(ctx, ["-d", "0", "true"])?
  assert zero.status == 1
  assert zero.stderr == "chrt: failed to set pid 0's policy: Invalid argument\n"

  # A runtime alone leaves deadline and period at zero.
  let runtime_only = chrt_run(ctx, ["-d", "-T", "1000000", "0", "true"])?
  assert runtime_only.status == 1
  assert runtime_only.stderr == "chrt: failed to set pid 0's policy: Invalid argument\n"
}

test test_chrt_all_tasks_needs_the_process_to_exist { |ctx|
  # A query of a missing process lists no tasks and reports nothing.
  let query = chrt_run(ctx, ["-a", "-p", "2147483647"])?
  assert query.status == 0
  assert query.stdout == ""
  assert query.stderr == ""

  let change = chrt_run(ctx, ["-a", "-o", "-p", "0", "2147483647"])?
  assert change.status == 1
  assert change.stderr == "chrt: cannot obtain the list of tasks: No such file or directory\n", change.stderr

  let handle = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  let child = handle.pid
  defer process.kill(child, signal: "KILL")

  let each = chrt_run(ctx, ["-a", "-p", f"{child}"])?
  assert each.status == 0
  assert each.stdout.lines().len() == 3
}

test test_chrt_stops_reading_options_at_the_first_operand { |ctx|
  let missing = chrt_run(ctx, ["-o", "0", "-p", "1"])?
  assert missing.status == 127
  assert missing.stderr == "chrt: failed to execute -p: No such file or directory\n", missing.stderr

  let directory = chrt_run(ctx, ["-o", "0", "/"])?
  assert directory.status == 126
  assert directory.stderr == "chrt: failed to execute /: Permission denied\n", directory.stderr
}

test test_chrt_command_arguments_and_status_pass_through { |ctx|
  let echoed = chrt_run(ctx, ["-o", "0", "echo", "-p", "5", "-f"])?
  assert echoed.stdout == "-p 5 -f\n"

  assert chrt_run(ctx, ["-o", "0", "sh", "-c", "exit 7"])?.status == 7
  assert chrt_run(ctx, ["-b", "false"])?.status == 1
}

test test_chrt_option_errors_use_getopt_wording_and_exit_1 { |ctx|
  let short = chrt_run(ctx, ["-x"])?
  assert short.status == 1
  assert short.stderr == "chrt: invalid option -- 'x'\nTry 'chrt --help' for more information.\n", short.stderr

  let missing = chrt_run(ctx, ["-T"])?
  assert missing.status == 1
  assert missing.stderr == "chrt: option requires an argument -- 'T'\nTry 'chrt --help' for more information.\n", missing.stderr
}

test test_chrt_help_and_version_go_to_stdout { |ctx|
  for flag in ["-h", "--help"] {
    let help = chrt_run(ctx, [flag])?
    assert help.status == 0
    assert help.stderr == ""
    assert help.stdout == HELP, help.stdout
  }

  let version = chrt_run(ctx, ["-V"])?
  assert version.status == 0
  assert version.stdout.starts_with("chrt ")
}
