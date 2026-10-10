type Ran = {status: Int, stdout: Str, stderr: Str}

# The help text starts with a blank line, which a literal cannot begin with;
# the assertion adds it back.
const HELP_BODY = """Usage:
 prlimit [options] [--<resource>=<limit>] [-p PID]
 prlimit [options] [--<resource>=<limit>] COMMAND

Show or change the resource limits of a process.

Options:
 -p, --pid <pid>        process id
 -o, --output <list>    define which output columns to use
     --noheadings       don't print headings
     --raw              use the raw output format
     --verbose          verbose output
 -h, --help             display this help
 -V, --version          display version

Resources:
 -c, --core             maximum size of core files created
 -d, --data             maximum size of a process's data segment
 -e, --nice             maximum nice priority allowed to raise
 -f, --fsize            maximum size of files written by the process
 -i, --sigpending       maximum number of pending signals
 -l, --memlock          maximum size a process may lock into memory
 -m, --rss              maximum resident set size
 -n, --nofile           maximum number of open files
 -q, --msgqueue         maximum bytes in POSIX message queues
 -r, --rtprio           maximum real-time scheduling priority
 -s, --stack            maximum stack size
 -t, --cpu              maximum amount of CPU time in seconds
 -u, --nproc            maximum number of user processes
 -v, --as               size of virtual memory
 -x, --locks            maximum number of file locks
 -y, --rttime           CPU time in microseconds a process scheduled
                        under real-time scheduling

Arguments:
 <limit> is defined as a range soft:hard, soft:, :hard or a value to
         define both limits (e.g. -e=0:10 -r=:10).

Available output columns:
 DESCRIPTION  resource description
    RESOURCE  resource name
        SOFT  soft limit
        HARD  hard limit (ceiling)
       UNITS  units

For more details see prlimit(1).
"""

# Runs core/prlimit.xsh by its real path, so the invoked name is `prlimit` and
# `lib.gnu` resolves beside it, capturing both streams.
proc prlimit_run(ctx: TestContext, args: List[Str]) [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "prlimit")?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/prlimit.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C", XSH_EXECUTION_PHRASE: ""}, b"", out, err)
  let status = process.run(plan)?

  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

# A command that prints `name [soft,hard]` for each named resource, with null
# for an unlimited bound.
proc reporter(ctx: TestContext, names: List[Str]) [fs, process, error] -> Result[List[Str]] {
  let root = test.temp_dir(ctx, name: "reporter")?
  let script = fp"{root}/report.xsh"
  script.write(r"""for name in args {
  let limit = process.rlimit(name)?
  print f"{name} {json.encode([limit.soft, limit.hard])?}"
}
""")

  Ok([ctx.xsh_bin.display(), script.display()].extend(names))
}

const NAMES = ["AS", "CORE", "CPU", "DATA", "FSIZE", "LOCKS", "MEMLOCK", "MSGQUEUE", "NICE", "NOFILE", "NPROC", "RSS", "RTPRIO", "RTTIME", "SIGPENDING", "STACK"]

const RESOURCES = ["as", "core", "cpu", "data", "fsize", "locks", "memlock", "msgqueue", "nice", "nofile", "nproc", "rss", "rtprio", "rttime", "sigpending", "stack"]

# The tests run `sleep` itself rather than a shell that execs it: these limits
# can be lower than a starting shell needs. The spawn can return while the
# child is still executing the program, and a limit set then would fail the
# exec, so the name the kernel reports tells when `sleep` is really running.
proc wait_for_sleep(pid: Int) [fs, time, error] {
  for _ in range(500) {
    if let Ok(name) = fp"/proc/{pid}/comm".read_text() {
      if name.trim() == "sleep" {
        return
      }
    }

    time.sleep(10ms)?
  }
}

# Gives a process limits that are the same on every host, using only values
# below the current ones so that no privilege is needed: 100 + position for the
# soft limit, 200 + position for the hard limit. NICE and RTPRIO start at 0
# for ordinary processes and cannot be raised, so they are left as they are.
proc fix_limits(pid: Int) [fs, time, process, error] -> Result[Bool] {
  wait_for_sleep(pid)

  var at = 0

  for name in RESOURCES {
    if name != "nice" and name != "rtprio" {
      process.set_rlimit(name, soft: 100 + at, hard: 200 + at, pid: pid)?
    }

    at += 1
  }

  Ok(true)
}

# A limit as the table prints it.
pure shown(limit: Int?) -> Str {
  if let value = limit { f"{value}" } else { "unlimited" }
}

test test_prlimit_lists_every_limit_of_a_process_in_a_table { |ctx|
  let handle = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  let child = handle.pid
  defer process.kill(child, signal: "KILL")

  if let Err(failure) = fix_limits(child) {
    test.skip(f"cannot lower this process's limits: {failure.message}")
  }

  let result = prlimit_run(ctx, ["-p", f"{child}"])?
  assert result.status == 0, result.stderr
  assert result.stderr == ""

  let lines = result.stdout.lines()
  assert lines.len() == 17, result.stdout
  assert lines[0].starts_with("RESOURCE   DESCRIPTION"), lines[0]
  assert lines[0].squeeze(" ").ends_with(" SOFT HARD UNITS"), lines[0]

  let descriptions = [
    ["address space limit", "bytes"],
    ["max core file size", "bytes"],
    ["CPU time", "seconds"],
    ["max data size", "bytes"],
    ["max file size", "bytes"],
    ["max number of file locks held", "locks"],
    ["max locked-in-memory address space", "bytes"],
    ["max bytes in POSIX mqueues", "bytes"],
    ["max nice prio allowed to raise", ""],
    ["max number of open files", "files"],
    ["max number of processes", "processes"],
    ["max resident set size", "bytes"],
    ["max real-time priority", ""],
    ["timeout for real-time tasks", "microsecs"],
    ["max number of pending signals", "signals"],
    ["max stack size", "bytes"],
  ]

  for index in range(16) {
    let line = lines[index + 1]
    let limit = process.rlimit(RESOURCES[index], pid: child)?
    let row = NAMES[index]

    assert line.starts_with(row), line
    assert line.byte_slice(row.byte_len()).trim().starts_with(descriptions[index][0]), line
    # Columns are padded, so compare with single spaces.
    assert line.squeeze(" ").ends_with(f" {shown(limit.soft)} {shown(limit.hard)} {descriptions[index][1]}"), line
  }

  # The values this test set are the ones listed.
  assert lines[1].squeeze(" ").ends_with(" 100 200 bytes"), lines[1]
  assert lines[10].squeeze(" ").ends_with(" 109 209 files"), lines[10]
  assert lines[16].squeeze(" ").ends_with(" 115 215 bytes"), lines[16]
}

test test_prlimit_shows_the_requested_resources_in_command_line_order { |ctx|
  let handle = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  let child = handle.pid
  defer process.kill(child, signal: "KILL")

  if let Err(failure) = fix_limits(child) {
    test.skip(f"cannot lower this process's limits: {failure.message}")
  }

  let shown = prlimit_run(ctx, ["--nofile", "--core", "-s", f"--pid={child}"])?
  assert shown.status == 0, shown.stderr
  assert shown.stdout == """RESOURCE DESCRIPTION              SOFT HARD UNITS
NOFILE   max number of open files  109  209 files
CORE     max core file size        101  201 bytes
STACK    max stack size            115  215 bytes
""", shown.stdout

  let short = prlimit_run(ctx, ["-n", "-c", "-s", "-p", f"{child}"])?
  assert short.stdout == shown.stdout

  # A letter takes the rest of its word, so `-ncs` is one malformed limit.
  let cluster = prlimit_run(ctx, ["-ncs", "-p", f"{child}"])?
  assert cluster.status == 1
  assert cluster.stderr == "prlimit: failed to parse NOFILE limit\n", cluster.stderr
}

test test_prlimit_output_columns_headings_and_raw_form { |ctx|
  let handle = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  let child = handle.pid
  defer process.kill(child, signal: "KILL")

  if let Err(failure) = fix_limits(child) {
    test.skip(f"cannot lower this process's limits: {failure.message}")
  }

  let pid = f"{child}"

  let columns = prlimit_run(ctx, ["-o", "RESOURCE,SOFT,HARD", "--nofile", "--cpu", "-p", pid])?
  assert columns.stdout == "RESOURCE SOFT HARD\nNOFILE    109  209\nCPU       102  202\n", columns.stdout

  # Names are not case sensitive, the last -o wins, and a right-aligned
  # column ends the line at its width.
  let lower = prlimit_run(ctx, ["-o", "resource,hard", "-o", "soft", "--nofile", "-p", pid])?
  assert lower.stdout == "SOFT\n 109\n", lower.stdout

  let hard_first = prlimit_run(ctx, ["--output=hard,soft", "--nofile", "-p", pid])?
  assert hard_first.stdout == "HARD SOFT\n 209  109\n", hard_first.stdout

  let bare = prlimit_run(ctx, ["--noheadings", "--nofile", "--cpu", "-p", pid])?
  assert bare.stdout == "NOFILE max number of open files 109 209 files\nCPU    CPU time                 102 202 seconds\n", bare.stdout

  # The raw form has no padding and writes blanks as \x20; an empty unit
  # leaves the separator behind.
  let nice = process.rlimit("nice", pid: child)?
  let raw = prlimit_run(ctx, ["--raw", "--nofile", "--nice", "-p", pid])?
  let nice_row = f"NICE max\\x20nice\\x20prio\\x20allowed\\x20to\\x20raise {shown(nice.soft)} {shown(nice.hard)} \n"
  assert raw.stdout == "RESOURCE DESCRIPTION SOFT HARD UNITS\nNOFILE max\\x20number\\x20of\\x20open\\x20files 109 209 files\n" + nice_row, raw.stdout

  let raw_bare = prlimit_run(ctx, ["--raw", "--noheadings", "-o", "RESOURCE,HARD", "--cpu", "--core", "-p", pid])?
  assert raw_bare.stdout == "CPU 202\nCORE 201\n", raw_bare.stdout

  # `--verbose` adds lines only when a limit is changed.
  let verbose = prlimit_run(ctx, ["--verbose", "--nofile", "-p", pid])?
  assert verbose.stdout == "RESOURCE DESCRIPTION              SOFT HARD UNITS\nNOFILE   max number of open files  109  209 files\n"
}

test test_prlimit_sets_limits_before_running_a_command { |ctx|
  let command = reporter(ctx, ["nofile", "core"])?

  let both = prlimit_run(ctx, ["--nofile=10:20", "--core=0"].extend(command))?
  assert both.status == 0, both.stderr
  assert both.stdout == "nofile [10,20]\ncore [0,0]\n", both.stdout

  # One number sets both bounds; `soft:` and `:hard` leave the other alone.
  let one = prlimit_run(ctx, ["--nofile=10"].extend(command))?
  assert one.stdout.starts_with("nofile [10,10]\n"), one.stdout

  let chained = prlimit_run(ctx, ["--nofile=10:20", ctx.xsh_bin.display(), fp"{ctx.core_dir}/prlimit.xsh".display(), "--nofile=5:"].extend(command))?
  assert chained.status == 0, chained.stderr
  assert chained.stdout.starts_with("nofile [5,20]\n"), chained.stdout

  let hard = prlimit_run(ctx, ["--nofile=6:20", ctx.xsh_bin.display(), fp"{ctx.core_dir}/prlimit.xsh".display(), "--nofile=:8"].extend(command))?
  assert hard.status == 0, hard.stderr
  assert hard.stdout.starts_with("nofile [6,8]\n"), hard.stdout

  # Short options take the limit attached, with or without an equals sign.
  let short = prlimit_run(ctx, ["-n10:20", "-c=0"].extend(command))?
  assert short.status == 0, short.stderr
  assert short.stdout == "nofile [10,20]\ncore [0,0]\n"

  # Several settings of one resource apply in order.
  let again = prlimit_run(ctx, ["--cpu=50:60", "--cpu=40:50", "--cpu=:45"].extend(reporter(ctx, ["cpu"])?))?
  assert again.status == 0, again.stderr
  assert again.stdout == "cpu [40,45]\n", again.stdout
}

test test_prlimit_unlimited_values_and_verbose_reports { |ctx|
  let handle = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  let child = handle.pid
  defer process.kill(child, signal: "KILL")

  # `unlimited` and -1 are the same limit, and a lower bound needs no privilege.
  let cpu_before = process.rlimit("cpu", pid: child)?

  if cpu_before.hard != null {
    test.skip("the process's CPU hard limit is finite, so it cannot be raised")
  }

  let pid = f"{child}"
  let lowered = prlimit_run(ctx, ["--verbose", "--cpu=100:200", "-p", pid])?
  assert lowered.status == 0, lowered.stderr
  assert lowered.stdout == f"New CPU limit for pid {child}: <100:200>\n", lowered.stdout
  assert process.rlimit("cpu", pid: child)?.soft == 100

  let command = reporter(ctx, ["core"])?
  let unlimited = prlimit_run(ctx, ["--core=unlimited"].extend(command))?
  assert unlimited.status == 0, unlimited.stderr
  assert unlimited.stdout == "core [null,null]\n", unlimited.stdout

  let minus = prlimit_run(ctx, ["--core=0:-1"].extend(command))?
  assert minus.status == 0, minus.stderr
  assert minus.stdout == "core [0,null]\n", minus.stdout
}

test test_prlimit_shows_a_command_the_limits_it_will_run_with { |ctx|
  # With no limit named and a command, the full table is printed first.
  let result = prlimit_run(ctx, ["echo", "ran"])?
  assert result.status == 0, result.stderr

  let lines = result.stdout.lines()
  assert lines.len() == 18, result.stdout
  assert lines[0].starts_with("RESOURCE   DESCRIPTION"), lines[0]
  assert lines[17] == "ran"

  for index in range(16) {
    assert lines[index + 1].starts_with(NAMES[index]), lines[index + 1]
  }
}

test test_prlimit_limit_text_errors { |ctx|
  let ceiling = fp"/proc/sys/fs/nr_open".read_text()?.trim()

  let cases = [
    {args: ["--nofile=abc", "true"], stderr: "prlimit: failed to parse NOFILE limit\n"},
    {args: ["--nofile=1:2:3", "true"], stderr: "prlimit: failed to parse NOFILE limit\n"},
    {args: ["--nofile=", "true"], stderr: "prlimit: failed to parse NOFILE limit\n"},
    {args: ["--nofile=:", "true"], stderr: "prlimit: failed to parse NOFILE limit\n"},
    {args: ["--nofile=0x10", "true"], stderr: "prlimit: failed to parse NOFILE limit\n"},
    {args: ["--nofile=10k", "true"], stderr: "prlimit: failed to parse NOFILE limit\n"},
    {args: ["--nofile=UNLIMITED:5", "true"], stderr: "prlimit: failed to parse NOFILE limit\n"},
    {args: ["--nofile=5 ", "true"], stderr: "prlimit: failed to parse NOFILE limit\n"},
    {args: ["--cpu=99999999999999999999", "true"], stderr: "prlimit: failed to parse CPU limit\n"},
    {args: ["-vn", "true"], stderr: "prlimit: failed to parse AS limit\n"},
    {args: ["--nofile=300:200", "true"], stderr: "prlimit: the soft limit NOFILE cannot exceed the hard limit\n"},
    {args: ["--cpu=300:200", "true"], stderr: "prlimit: the soft limit CPU cannot exceed the hard limit\n"},
    {args: ["--nofile=unlimited", "true"], stderr: f"prlimit: the NOFILE resource limit is not allowed to exceed {ceiling} (fs.nr_open)\n"},
    {args: ["--nofile=5:-1", "true"], stderr: f"prlimit: the NOFILE resource limit is not allowed to exceed {ceiling} (fs.nr_open)\n"},
    {args: ["--nofile=18446744073709551615", "true"], stderr: f"prlimit: the NOFILE resource limit is not allowed to exceed {ceiling} (fs.nr_open)\n"},
  ]

  for case in cases {
    let result = prlimit_run(ctx, case.args)?
    assert result.status == 1, case.args.join(" ")
    assert result.stdout == ""
    assert result.stderr == case.stderr, f"{case.args.join(" ")}: {result.stderr}"
  }

  # Blanks and a plus sign before the digits are what strtoul would accept.
  let command = reporter(ctx, ["nofile"])?
  let spaced = prlimit_run(ctx, ["--nofile= 5"].extend(command))?
  assert spaced.stdout.starts_with("nofile [5,5]\n"), spaced.stderr
  let plus = prlimit_run(ctx, ["--nofile=+6:+7"].extend(command))?
  assert plus.stdout.starts_with("nofile [6,7]\n"), plus.stderr
  let zeros = prlimit_run(ctx, ["--nofile=007"].extend(command))?
  assert zeros.stdout.starts_with("nofile [7,7]\n"), zeros.stderr
}

test test_prlimit_process_and_column_errors { |ctx|
  let cases = [
    {args: ["--pid", "abc"], stderr: "prlimit: invalid PID argument: 'abc'\n"},
    {args: ["-p", "1x"], stderr: "prlimit: invalid PID argument: '1x'\n"},
    {args: ["-p", ""], stderr: "prlimit: invalid PID argument: ''\n"},
    {args: ["-p", "0"], stderr: "prlimit: invalid PID argument: '0': Result not representable\n"},
    {args: ["-p", "-5"], stderr: "prlimit: invalid PID argument: '-5': Result not representable\n"},
    {args: ["-p", "4294967296"], stderr: "prlimit: invalid PID argument: '4294967296': Result not representable\n"},
    {args: ["-p", "1", "true"], stderr: "prlimit: options --pid and COMMAND are mutually exclusive\n"},
    {args: ["-p", "1", "--nofile=3", "true"], stderr: "prlimit: options --pid and COMMAND are mutually exclusive\n"},
    {args: ["-p", "2147483647"], stderr: "prlimit: failed to get the AS resource limit: No such process\n"},
    {args: ["--nofile", "-p", "2147483647"], stderr: "prlimit: failed to get the NOFILE resource limit: No such process\n"},
    {args: ["--nofile=100", "-p", "2147483647"], stderr: "prlimit: failed to set the NOFILE resource limit: No such process\n"},
    {args: ["--nofile=100:", "-p", "2147483647"], stderr: "prlimit: failed to get the NOFILE resource limit: No such process\n"},
    {args: ["-o", "NOPE", "--nofile"], stderr: "prlimit: unknown column: NOPE\n"},
    {args: ["-o", "RESOURCE,NOPE", "--nofile"], stderr: "prlimit: unknown column: NOPE\n"},
    {args: ["-o", "NOPE,RESOURCE", "--nofile"], stderr: "prlimit: unknown column: NOPE,RESOURCE\n"},
    {args: ["-o", "RESOURCE,", "--nofile"], stderr: "prlimit: unknown column: RESOURCE,\n"},
    {args: ["-o", "RESOURCE SOFT", "--nofile"], stderr: "prlimit: unknown column: RESOURCE SOFT\n"},
    {args: ["--nofile=100", "/definitely/not/here"], stderr: "prlimit: failed to execute /definitely/not/here: No such file or directory\n"},
  ]

  for case in cases {
    let result = prlimit_run(ctx, case.args)?
    assert result.status in [1, 127], case.args.join(" ")
    assert result.stdout == "", case.args.join(" ")
    assert result.stderr == case.stderr, f"{case.args.join(" ")}: {result.stderr}"
  }

  let silent = prlimit_run(ctx, ["-o", "", "--nofile"])?
  assert silent.status == 1
  assert silent.stdout == ""
  assert silent.stderr == ""
}

test test_prlimit_raising_a_hard_limit_needs_privilege { |ctx|
  if unix.id()?.euid == 0 {
    test.skip("root may raise hard limits")
  }

  let handle = process.spawn(process.command_argv("sleep", ["sleep", "60"]))?
  let child = handle.pid
  defer process.kill(child, signal: "KILL")

  # A limit that no process can raise without privilege, since the nice
  # ceiling of an ordinary process is already 0.
  let denied = prlimit_run(ctx, ["--verbose", "--nice=30:30", "-p", f"{child}"])?

  if denied.status == 0 {
    test.skip("this process may raise its nice ceiling")
  }

  assert denied.status == 1
  assert denied.stdout == f"New NICE limit for pid {child}: <30:30>\n", denied.stdout
  assert denied.stderr == "prlimit: failed to set the NICE resource limit: Operation not permitted\n", denied.stderr
}

test test_prlimit_command_arguments_are_not_options { |ctx|
  let cases = [
    {args: ["--nofile=100", "printf", "%s|%s", "-n", "--core=0"], stdout: "-n|--core=0"},
    {args: ["--core=0", "sh", "-c", "exit 7"], stdout: ""},
  ]

  let echoed = prlimit_run(ctx, cases[0].args)?
  assert echoed.stdout == cases[0].stdout, echoed.stdout

  assert prlimit_run(ctx, cases[1].args)?.status == 7

  # A lone `-` is an operand, so it starts the command.
  let dash = prlimit_run(ctx, ["--nofile=100", "-"])?
  assert dash.status == 127
  assert dash.stderr == "prlimit: failed to execute -: No such file or directory\n", dash.stderr

  let directory = prlimit_run(ctx, ["--nofile=100", "/"])?
  assert directory.status == 126
  assert directory.stderr == "prlimit: failed to execute /: Permission denied\n", directory.stderr
}

test test_prlimit_option_errors_use_getopt_wording_and_exit_1 { |ctx|
  let hint = "Try 'prlimit --help' for more information.\n"
  let cases = [
    {args: ["-Z"], stderr: f"prlimit: invalid option -- 'Z'\n{hint}"},
    {args: ["--nope"], stderr: f"prlimit: unrecognized option '--nope'\n{hint}"},
    {args: ["-p"], stderr: f"prlimit: option requires an argument -- 'p'\n{hint}"},
    {args: ["-o"], stderr: f"prlimit: option requires an argument -- 'o'\n{hint}"},
    {args: ["--pid"], stderr: f"prlimit: option '--pid' requires an argument\n{hint}"},
    {args: ["--output"], stderr: f"prlimit: option '--output' requires an argument\n{hint}"},
    {args: ["--raw=1"], stderr: f"prlimit: option '--raw' doesn't allow an argument\n{hint}"},
    {args: ["--noheadings=1"], stderr: f"prlimit: option '--noheadings' doesn't allow an argument\n{hint}"},
    {args: ["--n"], stderr: f"prlimit: option '--n' is ambiguous; possibilities: '--nice' '--nofile' '--noheadings' '--nproc'\n{hint}"},
    {args: ["--no"], stderr: f"prlimit: option '--no' is ambiguous; possibilities: '--nofile' '--noheadings'\n{hint}"},
  ]

  for case in cases {
    let result = prlimit_run(ctx, case.args)?
    assert result.status == 1, case.args.join(" ")
    assert result.stdout == ""
    assert result.stderr == case.stderr, f"{case.args.join(" ")}: {result.stderr}"
  }

  # An unambiguous prefix names the option, and an exact name beats a longer
  # one that starts with it.
  let command = reporter(ctx, ["nofile"])?
  let prefix = prlimit_run(ctx, ["--nofi=9"].extend(command))?
  assert prefix.status == 0, prefix.stderr
  assert prefix.stdout.starts_with("nofile [9,9]\n"), prefix.stdout

  let exact = prlimit_run(ctx, ["--raw", "--noheadings", "--cpu", "-o", "RESOURCE"])?
  assert exact.stdout == "CPU\n"
}

test test_prlimit_help_and_version_go_to_stdout_before_later_words_are_read { |ctx|
  for args in [["-h"], ["--help"], ["--help", "--nope"], ["-hV"], ["--he", "-Z"]] {
    let help = prlimit_run(ctx, args)?
    assert help.status == 0, args.join(" ")
    assert help.stderr == ""
    assert help.stdout == f"\n{HELP_BODY}", help.stdout
  }

  let version = prlimit_run(ctx, ["-V"])?
  assert version.status == 0
  assert version.stdout.starts_with("prlimit ")

  let first = prlimit_run(ctx, ["-V", "--help"])?
  assert first.stdout.starts_with("prlimit ")
}
