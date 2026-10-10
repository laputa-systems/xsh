type Ran = {status: Int, stdout: Str, stderr: Str}

const HELP = """Usage: taskset [options] [mask | cpu-list] [pid|cmd [args...]]


Show or change the CPU affinity of a process.

Options:
 -a, --all-tasks         operate on all the tasks (threads) for a given pid
 -p, --pid               operate on existing given pid
 -c, --cpu-list          display and specify cpus in list format
 -h, --help              display this help
 -V, --version           display version

The default behavior is to run a new command:
    taskset 03 sshd -b 1024
You can retrieve the mask of an existing task:
    taskset -p 700
Or set it:
    taskset -p 03 700
List format uses a comma-separated list instead of a mask:
    taskset -pc 0,3,7-11 700
Ranges in list format can take a stride argument:
    e.g. 0-31:2 is equivalent to mask 0x55555555

For more details see taskset(1).
"""

# Runs core/taskset.xsh by its real path, so the invoked name is `taskset` and
# `lib.gnu` resolves beside it, capturing both streams.
proc taskset_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "taskset")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/taskset.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err)
  let status = process.run(plan)?

  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

# A command that prints the CPUs its own process may use, one list of numbers.
proc reporter(ctx: TestContext) [fs, process, error] -> Result[List[Str]] {
  let root = test.temp_dir(ctx, name: "reporter")?
  let script = fp"{root}/report.xsh"
  script.write("print json.encode(process.affinity()?)?\n")

  Ok([ctx.xsh_bin.display(), script.display()])
}

# The same command as taskset itself, asked for the affinity of this process.
proc self_query(ctx: TestContext, flags: Str) -> List[Str] {
  [ctx.xsh_bin.display(), fp"{ctx.core_dir}/taskset.xsh".display(), flags, "0"]
}

pure hex_mask(cpus: List[Int]) -> Str {
  var top = 0

  for id in cpus {
    if id > top {
      top = id
    }
  }

  var digits = ""
  var nibble = top / 4

  while nibble >= 0 {
    var value = 0

    for id in cpus {
      if id / 4 == nibble {
        value += [1, 2, 4, 8][id % 4]
      }
    }

    digits = f"{digits}{"0123456789abcdef".byte_slice(value, length: 1)}"
    nibble -= 1
  }

  digits
}

pure has_all(allowed: List[Int], wanted: List[Int]) -> Bool {
  for id in wanted {
    if ! (id in allowed) {
      return false
    }
  }

  true
}

type ListCase = {input: Str, cpus: List[Int], shown: Str}

# Each pair was printed by util-linux taskset for the same CPUs.
const LIST_CASES = [
  {input: "0,2,4", cpus: [0, 2, 4], shown: "0-4:2"},
  {input: "0-3:2,5,8-9", cpus: [0, 2, 5, 8, 9], shown: "0,2-8:3,9"},
  {input: "0,1,3,5,7", cpus: [0, 1, 3, 5, 7], shown: "0,1-7:2"},
  {input: "0,5,10,11,12,13,20", cpus: [0, 5, 10, 11, 12, 13, 20], shown: "0-10:5,11-13,20"},
  {input: "0,2,4,7,9,11", cpus: [0, 2, 4, 7, 9, 11], shown: "0-4:2,7-11:2"},
  {input: "0,1,3,4,6,7", cpus: [0, 1, 3, 4, 6, 7], shown: "0,1,3,4,6,7"},
  {input: "0,3,6,7,8,9", cpus: [0, 3, 6, 7, 8, 9], shown: "0-6:3,7-9"},
  {input: "0,2,4,6,8,10,12,14,15,16,17", cpus: [0, 2, 4, 6, 8, 10, 12, 14, 15, 16, 17], shown: "0-14:2,15-17"},
  {input: "1,6,11,16,21,23,25,26,31", cpus: [1, 6, 11, 16, 21, 23, 25, 26, 31], shown: "1-21:5,23,25,26,31"},
  {input: "0,1,3,4,8,12,16,19,20,24,28", cpus: [0, 1, 3, 4, 8, 12, 16, 19, 20, 24, 28], shown: "0,1,3,4-16:4,19,20-28:4"},
  {input: "0-1,4-5", cpus: [0, 1, 4, 5], shown: "0,1,4,5"},
  {input: "0-3,8-11", cpus: [0, 1, 2, 3, 8, 9, 10, 11], shown: "0-3,8-11"},
  {input: "1-31:2", cpus: [1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27, 29, 31], shown: "1-31:2"},
]

test test_taskset_reports_the_affinity_of_a_process_as_mask_and_list { |ctx|
  let cpus = process.affinity()?
  let pid = process.current_pid()?

  let mask = taskset_run(ctx, ["-p", f"{pid}"])?
  assert mask.status == 0, mask.stderr
  assert mask.stderr == ""
  assert mask.stdout == f"pid {pid}'s current affinity mask: {hex_mask(cpus)}\n", mask.stdout

  let list = taskset_run(ctx, ["-pc", f"{pid}"])?
  assert list.status == 0, list.stderr

  # The allowed CPUs of a test host are usually one range; pin that spelling.
  let last = cpus[-1]

  if cpus == [id for id in range(last + 1)] {
    assert list.stdout == f"pid {pid}'s current affinity list: {if last == 0 { "0" } else { f"0-{last}" }}\n"
  } else {
    assert list.stdout.starts_with(f"pid {pid}'s current affinity list: "), list.stdout
  }

  # `--pid --cpu-list` is `-pc`, and pid 0 is the process asked.
  let long = taskset_run(ctx, ["--cpu-list", "--pid", "0"])?
  assert long.status == 0
  assert long.stdout.ends_with("'s current affinity list: " + list.stdout.split(": ")[1])
}

test test_taskset_changes_the_affinity_of_another_process { |ctx|
  let cpus = process.affinity()?

  if cpus.len() < 2 {
    test.skip("the process may use one CPU, so there is nothing to restrict")
  }

  let child = process.spawn(process.command_argv("sh", ["sh", "-c", "exec sleep 60"]))?
  defer process.kill(child.pid, signal: "KILL")

  let changed = taskset_run(ctx, ["-p", hex_mask([cpus[0]]), f"{child.pid}"])?
  assert changed.status == 0, changed.stderr
  assert changed.stdout == f"pid {child.pid}'s current affinity mask: {hex_mask(cpus)}\npid {child.pid}'s new affinity mask: {hex_mask([cpus[0]])}\n", changed.stdout
  assert process.affinity(child.pid)? == [cpus[0]]
  assert process.affinity()? == cpus

  let both = [cpus[0], cpus[1]]
  let listed = taskset_run(ctx, ["-pc", f"{cpus[1]},{cpus[0]}", f"{child.pid}"])?
  assert listed.status == 0, listed.stderr
  assert process.affinity(child.pid)? == both

  # `-a` visits every task of the process; a single-threaded child has one.
  let all = taskset_run(ctx, ["-a", "-p", f"{child.pid}"])?
  assert all.status == 0
  assert all.stdout == f"pid {child.pid}'s current affinity mask: {hex_mask(both)}\n", all.stdout
}

test test_taskset_runs_a_command_under_a_mask_or_a_list { |ctx|
  let cpus = process.affinity()?
  let command = reporter(ctx)?

  let by_mask = taskset_run(ctx, [hex_mask([cpus[0]])].extend(command))?
  assert by_mask.status == 0, by_mask.stderr
  assert by_mask.stdout == f"[{cpus[0]}]\n", by_mask.stdout

  let by_prefix = taskset_run(ctx, [f"0x{hex_mask([cpus[0]])}"].extend(command))?
  assert by_prefix.stdout == f"[{cpus[0]}]\n"

  let by_list = taskset_run(ctx, ["-c", f"{cpus[0]}"].extend(command))?
  assert by_list.status == 0, by_list.stderr
  assert by_list.stdout == f"[{cpus[0]}]\n"

  # The command's own arguments are never read as options.
  let echoed = taskset_run(ctx, ["-c", f"{cpus[0]}", "echo", "-p", "-c", "x"])?
  assert echoed.stdout == "-p -c x\n"
}

test test_taskset_list_output_uses_ranges_and_strides_as_util_linux_does { |ctx|
  let allowed = process.affinity()?
  var ran = 0

  for case in LIST_CASES {
    if ! has_all(allowed, case.cpus) {
      continue
    }

    let result = taskset_run(ctx, ["-c", case.input].extend(self_query(ctx, "-pc")))?
    assert result.status == 0, result.stderr
    assert result.stdout.ends_with(f"'s current affinity list: {case.shown}\n"), f"{case.input}: {result.stdout}"
    ran += 1
  }

  if ran == 0 {
    test.skip("the process may not use CPUs 0 through 4")
  }
}

test test_taskset_mask_parsing_accepts_prefixes_commas_and_case { |ctx|
  let allowed = process.affinity()?
  let cases = [
    {mask: "5", cpus: [0, 2]},
    {mask: "0x5", cpus: [0, 2]},
    {mask: "05", cpus: [0, 2]},
    {mask: "FF", cpus: [0, 1, 2, 3, 4, 5, 6, 7]},
    {mask: "aF", cpus: [0, 1, 2, 3, 5, 7]},
    {mask: "1,2", cpus: [1, 4]},
    {mask: "3,", cpus: [0, 1]},
  ]

  for case in cases {
    if ! has_all(allowed, case.cpus) {
      continue
    }

    let result = taskset_run(ctx, [case.mask].extend(self_query(ctx, "-p")))?
    assert result.status == 0, f"{case.mask}: {result.stderr}"
    assert result.stdout.ends_with(f"'s current affinity mask: {hex_mask(case.cpus)}\n"), f"{case.mask}: {result.stdout}"
  }
}

test test_taskset_mask_and_list_syntax_errors_name_the_text { |ctx|
  for mask in ["zz", "g", ",3", "0X3", "3 ", " 3", "+3", "0x,3", "3,,"] {
    let result = taskset_run(ctx, [mask, "true"])?
    assert result.status == 1, mask
    assert result.stdout == ""
    assert result.stderr == f"taskset: failed to parse CPU mask: {mask}\n", result.stderr
  }

  for list in ["1-0", "0,,1", "0-3:0", "3:", " 3", "3 ", "a-b", "5:2", ",", "", "1-2-3", "0-5:2:1", "1-", "+1", "0x1", "1,", ",1"] {
    let result = taskset_run(ctx, ["-c", list, "true"])?
    assert result.status == 1, list
    assert result.stdout == ""
    assert result.stderr == f"taskset: failed to parse CPU list: {list}\n", result.stderr
  }
}

test test_taskset_refused_masks_report_the_kernels_error { |ctx|
  for mask in ["0", "0x", "00"] {
    let result = taskset_run(ctx, [mask, "true"])?
    assert result.status == 1, mask
    assert result.stderr.starts_with("taskset: failed to set pid "), result.stderr
    assert result.stderr.ends_with("'s affinity: Invalid argument\n"), result.stderr
  }

  # A CPU number past what any kernel configures leaves no CPU to run on.
  let beyond = taskset_run(ctx, ["-c", "1000000", "true"])?
  assert beyond.status == 1
  assert beyond.stderr.ends_with("'s affinity: Invalid argument\n"), beyond.stderr

  # Numbers the kernel has no bit for are dropped, not refused.
  let cpus = process.affinity()?
  let mixed = taskset_run(ctx, ["-c", f"{cpus[0]},1000000", "true"])?
  assert mixed.status == 0, mixed.stderr
}

test test_taskset_pid_and_operand_errors { |ctx|
  let usage = "Try 'taskset --help' for more information.\n"

  for args in [[], ["-c"], ["-c", "0"], ["3"], ["-pc", "0", "1", "2"], ["-p", "1", "2", "3"]] {
    let result = taskset_run(ctx, args)?
    assert result.status == 1, args.join(" ")
    assert result.stdout == ""
    assert result.stderr == f"taskset: bad usage\n{usage}", result.stderr
  }

  let invalid = [
    ["-p", "abc", "taskset: invalid PID argument: 'abc'\n"],
    ["-p", "1x", "taskset: invalid PID argument: '1x'\n"],
    ["-p", "5 ", "taskset: invalid PID argument: '5 '\n"],
    ["-p", "0x5", "taskset: invalid PID argument: '0x5'\n"],
    ["-p", "5.0", "taskset: invalid PID argument: '5.0'\n"],
    ["-p", "", "taskset: invalid PID argument: ''\n"],
    ["-p", "4294967296", "taskset: invalid PID argument: '4294967296': Result not representable\n"],
    ["-p", "2147483648", "taskset: invalid PID argument: '2147483648': Result not representable\n"],
    ["-pc", "99999999999999999999", "taskset: invalid PID argument: '99999999999999999999': Result not representable\n"],
  ]

  for case in invalid {
    let result = taskset_run(ctx, [case[0], case[1]])?
    assert result.status == 1, case[1]
    assert result.stderr == case[2], result.stderr
  }

  # With no operand the last word of the command line is read as the pid.
  let bare = taskset_run(ctx, ["-p"])?
  assert bare.status == 1
  assert bare.stderr == "taskset: invalid PID argument: '-p'\n"

  let missing = taskset_run(ctx, ["-p", "2147483647"])?
  assert missing.status == 1
  assert missing.stderr == "taskset: failed to get pid 2147483647's affinity: No such process\n", missing.stderr

  let set_missing = taskset_run(ctx, ["-pc", "0", "2147483647"])?
  assert set_missing.status == 1
  assert set_missing.stderr == "taskset: failed to get pid 2147483647's affinity: No such process\n"

  # An `-a` walk of a process without tasks has nothing to report.
  let none = taskset_run(ctx, ["-a", "-p", "2147483647"])?
  assert none.status == 0
  assert none.stdout == ""
  assert none.stderr == ""
}

test test_taskset_a_plus_sign_and_leading_zeros_are_pids { |ctx|
  let pid = process.current_pid()?
  let plus = taskset_run(ctx, ["-p", f"+{pid}"])?
  assert plus.status == 0, plus.stderr
  assert plus.stdout.starts_with(f"pid {pid}'s current affinity mask: ")
}

test test_taskset_stops_reading_options_at_the_first_operand { |ctx|
  let cpus = process.affinity()?
  let mask = hex_mask([cpus[0]])

  let result = taskset_run(ctx, [mask, "-c", "0", "true"])?
  assert result.status == 127
  assert result.stderr == "taskset: failed to execute -c: No such file or directory\n", result.stderr
}

test test_taskset_missing_and_unrunnable_commands { |ctx|
  let cpus = process.affinity()?
  let missing = taskset_run(ctx, ["-c", f"{cpus[0]}", "/definitely/not/here"])?
  assert missing.status == 127
  assert missing.stderr == "taskset: failed to execute /definitely/not/here: No such file or directory\n", missing.stderr

  let directory = taskset_run(ctx, ["-c", f"{cpus[0]}", "/"])?
  assert directory.status == 126
  assert directory.stderr == "taskset: failed to execute /: Permission denied\n", directory.stderr
}

test test_taskset_command_status_is_the_exit_status { |ctx|
  let cpus = process.affinity()?
  assert taskset_run(ctx, ["-c", f"{cpus[0]}", "sh", "-c", "exit 7"])?.status == 7
  assert taskset_run(ctx, ["-c", f"{cpus[0]}", "false"])?.status == 1
}

test test_taskset_option_errors_use_getopt_wording_and_exit_1 { |ctx|
  let short = taskset_run(ctx, ["-x"])?
  assert short.status == 1
  assert short.stderr == "taskset: invalid option -- 'x'\nTry 'taskset --help' for more information.\n", short.stderr

  let long = taskset_run(ctx, ["--nope"])?
  assert long.status == 1
  assert long.stderr == "taskset: unrecognized option '--nope'\nTry 'taskset --help' for more information.\n", long.stderr
}

test test_taskset_help_and_version_go_to_stdout { |ctx|
  for flag in ["-h", "--help", "--he"] {
    let help = taskset_run(ctx, [flag])?
    assert help.status == 0
    assert help.stderr == ""
    assert help.stdout == HELP, help.stdout
  }

  let version = taskset_run(ctx, ["-V"])?
  assert version.status == 0
  assert version.stdout.starts_with("taskset ")
}
