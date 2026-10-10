type Ran = {status: Int, stdout: Str, stderr: Str}

# The help text starts with a blank line, which a literal cannot begin with;
# the assertion adds it back.
const HELP_BODY = """Usage:
 ionice [options] -p <pid>...
 ionice [options] -P <pgid>...
 ionice [options] -u <uid>...
 ionice [options] <command>

Show or change the I/O-scheduling class and priority of a process.

Options:
 -c, --class <class>    name or number of scheduling class,
                          0: none, 1: realtime, 2: best-effort, 3: idle
 -n, --classdata <num>  priority (0..7) in the specified scheduling class,
                          only for the realtime and best-effort classes
 -p, --pid <pid>...     act on these already running processes
 -P, --pgid <pgrp>...   act on already running processes in these groups
 -t, --ignore           ignore failures
 -u, --uid <uid>...     act on already running processes owned by these users

 -h, --help             display this help
 -V, --version          display version

For more details see ionice(1).
"""

# Runs core/ionice.xsh by its real path, so the invoked name is `ionice` and
# `lib.gnu` resolves beside it, capturing both streams.
proc ionice_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "ionice")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/ionice.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err)
  let status = process.run(plan)?

  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

# A command that prints the I/O priority of the calling thread, which `exec`
# carried over from the applet.
proc reporter(ctx: TestContext) [fs, process, error] -> Result[List[Str]] {
  let root = test.temp_dir(ctx, name: "reporter")?
  let script = fp"{root}/report.xsh"
  script.write("let info = process.io_priority()?\nprint f\"{info.class} {info.level}\"\n")

  Ok([ctx.xsh_bin.display(), script.display()])
}

test test_ionice_without_arguments_prints_the_current_priority { |ctx|
  process.set_io_priority(0, "best-effort", 5, "process")?
  let result = ionice_run(ctx, [])?
  assert result.status == 0, result.stderr
  assert result.stderr == ""
  assert result.stdout == "best-effort: prio 5\n", result.stdout
}

test test_ionice_runs_a_command_under_a_class_and_level { |ctx|
  let command = reporter(ctx)?

  let named = ionice_run(ctx, ["-c", "best-effort", "-n", "3"].extend(command))?
  assert named.status == 0, named.stderr
  assert named.stdout == "best-effort 3\n", named.stdout

  let numbered = ionice_run(ctx, ["-c2", "-n7"].extend(command))?
  assert numbered.stdout == "best-effort 7\n"

  let long = ionice_run(ctx, ["--class=BEST-EFFORT", "--classdata", "1"].extend(command))?
  assert long.stdout == "best-effort 1\n", long.stdout

  # A level alone means best-effort, and a class alone level 4.
  let level = ionice_run(ctx, ["-n", "2"].extend(command))?
  assert level.stdout == "best-effort 2\n"

  let class = ionice_run(ctx, ["-c", "2"].extend(command))?
  assert class.stdout == "best-effort 4\n"

  let idle = ionice_run(ctx, ["-c", "idle"].extend(command))?
  assert idle.status == 0, idle.stderr
  assert idle.stdout.starts_with("idle "), idle.stdout

  let plain = ionice_run(ctx, command)?
  assert plain.stdout == "best-effort 4\n"
}

test test_ionice_ignored_class_data_warns { |ctx|
  let command = reporter(ctx)?

  let none = ionice_run(ctx, ["-c", "0", "-n", "3"].extend(command))?
  assert none.status == 0, none.stderr
  assert none.stderr == "ionice: ignoring given class data for none class\n"
  assert none.stdout == "none 0\n"

  let idle = ionice_run(ctx, ["-c", "3", "-n", "3"].extend(command))?
  assert idle.status == 0, idle.stderr
  assert idle.stderr == "ionice: ignoring given class data for idle class\n"
  assert idle.stdout.starts_with("idle ")

  let quiet = ionice_run(ctx, ["-c", "none"].extend(command))?
  assert quiet.stderr == ""
  assert quiet.stdout == "none 0\n"
}

test test_ionice_reads_and_changes_other_processes { |ctx|
  let handle = process.spawn(process.command {
    new_session = true
    run sleep 60
  })?
  let child = handle.pid
  defer process.kill(child, signal: "KILL")

  let changed = ionice_run(ctx, ["-c", "2", "-n", "5", "-p", f"{child}"])?
  assert changed.status == 0, changed.stderr
  assert changed.stdout == ""
  assert process.io_priority(child)? == {class: "best-effort", level: 5}

  let shown = ionice_run(ctx, ["-p", f"{child}"])?
  assert shown.stdout == "best-effort: prio 5\n", shown.stdout

  # Further pids follow the first, each answered in turn.
  let several = ionice_run(ctx, ["-p", f"{child}", f"{child}"])?
  assert several.stdout == "best-effort: prio 5\nbest-effort: prio 5\n", several.stdout

  let idle = ionice_run(ctx, ["-c", "idle", "-p", f"{child}"])?
  assert idle.status == 0, idle.stderr
  assert ionice_run(ctx, ["-p", f"{child}"])?.stdout == "idle\n"

  let none = ionice_run(ctx, ["-c", "none", "--pid", f"{child}"])?
  assert none.status == 0, none.stderr
  assert ionice_run(ctx, ["-p", f"{child}"])?.stdout == "none: prio 0\n"
}

test test_ionice_changes_a_process_group { |ctx|
  let handle = process.spawn(process.command {
    new_session = true
    run sleep 60
  })?
  let child = handle.pid
  defer process.kill(child, signal: "KILL")

  # The child leads a session of its own, so its pid is its group.
  let changed = ionice_run(ctx, ["-c", "2", "-n", "6", "-P", f"{child}"])?
  assert changed.status == 0, changed.stderr
  assert ionice_run(ctx, ["-P", f"{child}"])?.stdout == "best-effort: prio 6\n"
  assert ionice_run(ctx, ["--pgid", f"{child}"])?.stdout == "best-effort: prio 6\n"
  assert process.io_priority(child)? == {class: "best-effort", level: 6}
}

test test_ionice_reads_a_users_priority { |ctx|
  let uid = unix.id()?.uid
  let result = ionice_run(ctx, ["-u", f"{uid}"])?
  assert result.status == 0, result.stderr
  assert result.stdout.lines().len() == 1
  assert result.stdout.starts_with("none: prio ") or result.stdout.starts_with("best-effort: prio ") or result.stdout.starts_with("realtime: prio ") or result.stdout == "idle\n", result.stdout

  # A user without processes is the kernel's "no such process".
  let nobody = ionice_run(ctx, ["-u", "4000000000"])?
  assert nobody.status == 1
  assert nobody.stderr == "ionice: ioprio_get failed: No such process\n", nobody.stderr
}

test test_ionice_failures_follow_util_linux_wording { |ctx|
  let handle = process.spawn(process.command {
    new_session = true
    run sleep 60
  })?
  let child = handle.pid
  defer process.kill(child, signal: "KILL")

  let missing = ionice_run(ctx, ["-p", "2147483647"])?
  assert missing.status == 1
  assert missing.stdout == ""
  assert missing.stderr == "ionice: ioprio_get failed: No such process\n", missing.stderr

  let change = ionice_run(ctx, ["-c", "2", "-p", "2147483647"])?
  assert change.status == 1
  assert change.stderr == "ionice: ioprio_set failed: No such process\n", change.stderr

  # `-t` ignores a failed change but not a failed read, and reports nothing.
  let tolerant = ionice_run(ctx, ["-t", "-c", "2", "-p", "2147483647"])?
  assert tolerant.status == 0
  assert tolerant.stderr == ""

  let read = ionice_run(ctx, ["-t", "-p", "2147483647"])?
  assert read.status == 1

  # The first pid is answered before a later bad one is read.
  let partial = ionice_run(ctx, ["-p", f"{child}", "true"])?
  assert partial.status == 1
  assert partial.stdout.lines().len() == 1
  assert partial.stderr == "ionice: invalid PID argument: 'true'\n", partial.stderr
}

test test_ionice_argument_errors { |ctx|
  let hint = "Try 'ionice --help' for more information.\n"
  let errors = [
    {args: ["-c", "foo", "true"], stderr: "ionice: unknown scheduling class: 'foo'\n"},
    {args: ["-c", "", "true"], stderr: "ionice: unknown scheduling class: ''\n"},
    {args: ["-c", "2x", "true"], stderr: "ionice: invalid class argument: '2x'\n"},
    {args: ["-c", "99999999999", "true"], stderr: "ionice: invalid class argument: '99999999999': Result not representable\n"},
    {args: ["-c", "3", "-n", "abc", "true"], stderr: "ionice: invalid class data argument: 'abc'\n"},
    {args: ["-n", "99999999999", "true"], stderr: "ionice: invalid class data argument: '99999999999': Result not representable\n"},
    {args: ["-p", "abc"], stderr: "ionice: invalid PID argument: 'abc'\n"},
    {args: ["-p", "99999999999"], stderr: "ionice: invalid PID argument: '99999999999': Result not representable\n"},
    {args: ["-P", "x"], stderr: "ionice: invalid PGID argument: 'x'\n"},
    {args: ["-u", "root"], stderr: "ionice: invalid UID argument: 'root'\n"},
    {args: ["-u", "4294967296"], stderr: "ionice: invalid UID argument: '4294967296': Result not representable\n"},
    {args: ["-p", "1", "-P", "1"], stderr: "ionice: can handle only one of pid, pgid or uid at once\n"},
    {args: ["-p", "1", "-p", "2"], stderr: "ionice: can handle only one of pid, pgid or uid at once\n"},
    {args: ["-c", "2", "-n", "4", "-p", "1", "-u", "1"], stderr: "ionice: can handle only one of pid, pgid or uid at once\n"},
  ]

  for case in errors {
    let result = ionice_run(ctx, case.args)?
    assert result.status == 1, case.args.join(" ")
    assert result.stdout == ""
    assert result.stderr == case.stderr, f"{case.args.join(" ")}: {result.stderr}"
  }

  # Nothing to act on is a usage error, with the hint.
  let usage = ionice_run(ctx, ["-c", "2"])?
  assert usage.status == 1
  assert usage.stderr == f"ionice: bad usage\n{hint}", usage.stderr
}

test test_ionice_unknown_classes_are_passed_to_the_kernel { |ctx|
  let refused = ionice_run(ctx, ["-c", "4", "true"])?
  assert refused.status == 1
  assert refused.stderr == "ionice: unknown prio class 4\nionice: ioprio_set failed: Invalid argument\n", refused.stderr

  # `-t` hides both the warning and the failure.
  let tolerant = ionice_run(ctx, ["-t", "-c", "9", "true"])?
  assert tolerant.status == 0
  assert tolerant.stderr == ""
}

test test_ionice_level_wider_than_its_field_changes_the_class { |ctx|
  # The kernel packs class and level into one number, so 8192 is the idle
  # class's first value; util-linux does the same sum.
  let result = ionice_run(ctx, ["-n", "8192"].extend(reporter(ctx)?))?
  assert result.status == 0, result.stderr
  assert result.stdout.starts_with("idle "), result.stdout

  let negative = ionice_run(ctx, ["-c", "2", "-n", "-1", "true"])?
  assert negative.status == 1
  assert negative.stderr == "ionice: ioprio_set failed: Invalid argument\n", negative.stderr
}

test test_ionice_real_time_class_needs_privilege { |ctx|
  let result = ionice_run(ctx, ["-c", "1", "true"])?

  if result.status == 0 {
    test.skip("this process may use the real-time I/O class")
  }

  assert result.status == 1
  assert result.stderr == "ionice: ioprio_set failed: Operation not permitted\n", result.stderr
}

test test_ionice_command_errors_and_status { |ctx|
  let missing = ionice_run(ctx, ["-c", "2", "/definitely/not/here"])?
  assert missing.status == 127
  assert missing.stderr == "ionice: failed to execute /definitely/not/here: No such file or directory\n", missing.stderr

  let directory = ionice_run(ctx, ["/"])?
  assert directory.status == 126
  assert directory.stderr == "ionice: failed to execute /: Permission denied\n", directory.stderr

  assert ionice_run(ctx, ["-c", "2", "sh", "-c", "exit 7"])?.status == 7

  # The command's own words are never read as options.
  let echoed = ionice_run(ctx, ["-c", "2", "echo", "-p", "-c", "9"])?
  assert echoed.stdout == "-p -c 9\n"
}

test test_ionice_option_errors_use_getopt_wording_and_exit_1 { |ctx|
  let short = ionice_run(ctx, ["-x"])?
  assert short.status == 1
  assert short.stderr == "ionice: invalid option -- 'x'\nTry 'ionice --help' for more information.\n", short.stderr

  let missing = ionice_run(ctx, ["-p"])?
  assert missing.status == 1
  assert missing.stderr == "ionice: option requires an argument -- 'p'\nTry 'ionice --help' for more information.\n", missing.stderr
}

test test_ionice_help_and_version_go_to_stdout { |ctx|
  for flag in ["-h", "--help"] {
    let help = ionice_run(ctx, [flag])?
    assert help.status == 0
    assert help.stderr == ""
    assert help.stdout == f"\n{HELP_BODY}", help.stdout
  }

  let version = ionice_run(ctx, ["-V"])?
  assert version.status == 0
  assert version.stdout.starts_with("ionice ")
}
