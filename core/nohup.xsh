#!/bin/xsh
use lib.gnu

const USAGE = """Usage: nohup COMMAND [ARG]...
Run COMMAND, ignoring hangup signals.

If standard input is a terminal, redirect it from an unreadable file.
If standard output is a terminal, append output to 'nohup.out' in the
current directory, or '$HOME/nohup.out' if the current directory is not
writable. If standard error is a terminal, redirect it to standard output.

      --help        display this help and exit
      --version     output version information and exit
"""

type NohupOptions = {help: Bool, version: Bool, command: List[Str]}

proc command_status(command: Str) [fs, process] -> Int {
  if command.find("/") != null {
    let target = fp"{command}"

    return 127 when ! (target.exists() ?? false)
    let launch = match target.metadata() {
      Ok(entry) => if entry.kind == "dir" or ! entry.executable { 126 } else { 0 },
      Err(_) => 126,
    }
    return launch
  }

  match process.which(command) {
    Ok(_) => 0
    Err(_) => 127
  }
}

proc output_path(ignoring_input: Bool) [fs, process, env, error] -> Path {
  let cwd_output = fp"nohup.out"
  let cwd_existed = cwd_output.exists()?
  let message = if ignoring_input { "ignoring input and appending output to" } else { "appending output to" }

  if ! cwd_existed {
    if let Err(_) = fs.mknod(cwd_output, "file", 0o600) {
      let home = env.get("HOME")?
      let fallback = fp"{home}/nohup.out"

      if ! fallback.exists()? {
        fs.mknod(fallback, "file", 0o600)?
      }

      gnu.error(f"{message} {gnu.quote(fallback.display())}")
      return fallback
    }
  }

  gnu.error(f"{message} {gnu.quote(cwd_output.display())}")
  cwd_output
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let failure_status = if let Ok(_) = env.get("POSIXLY_CORRECT") { 127 } else { 125 }

  if failure_status == 127 {
    for argument in argv {
      if argument == "--" { break }
      if argument == "--help" or argument == "--version" { break }
      if argument == "-" or ! argument.starts_with("-") { break }

      if argument.starts_with("--") {
        gnu.error(f"unrecognized option {gnu.quote_value(argument)}")
      } else {
        gnu.error(f"invalid option -- {gnu.quote_value(argument.byte_slice(1, length: 1))}")
      }

      gnu.try_help()
      exit 127
    }
  }

  let opts: NohupOptions = cli.applet(
    argv,
    {
      gnu: {status: 125, permute: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      command: {form: "...COMMAND"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("nohup")
    return
  }

  if opts.command.len() == 0 {
    gnu.missing_operand(failure_status)
  }

  let command = opts.command[0]
  let launch = command_status(command)

  if launch != 0 {
    let reason = if launch == 127 { "No such file or directory" } else { "Permission denied" }
    gnu.error(f"failed to run command {gnu.quote(command)}: {reason}")
    exit launch
  }

  let stdin_terminal = unix.isatty(0)
  let stdin = if stdin_terminal { /dev/null } else { /dev/stdin }
  let stdout_terminal = unix.isatty(1)
  let stderr_terminal = unix.isatty(2)
  var stdout: Path? = null
  var stderr: Path? = null

  if stdout_terminal {
    stdout = output_path(stdin_terminal)
  } else if stdin_terminal {
    gnu.error("ignoring input")
  }

  if stderr_terminal {
    stderr = if stdout_terminal { stdout } else { /dev/stdout }
  }

  var execution_target = command
  var execution_argv = opts.command

  if stdin_terminal {
    execution_target = "/bin/sh"
    execution_argv = [
      "/bin/sh",
      "-c",
      "exec 0>&1; exec \"$@\"",
      "nohup",
    ].extend(opts.command)
  }

  let plan = process.command_argv(
    execution_target,
    execution_argv,
    cwd: fs.cwd()?,
    stdin: stdin,
    stdout: stdout ?? /dev/stdout,
    stderr: stderr ?? /dev/stderr,
    stdout_append: stdout_terminal,
    stderr_append: stdout_terminal and stderr_terminal,
    ignore_hup: true,
  )

  match process.run(plan) {
    Ok(status) => exit status.shell_code()?
    Err(failure) => {
      let code = if (failure.errno ?? 0) == 13 { 126 } else { 127 }
      gnu.error(f"failed to run command {gnu.quote(command)}: {gnu.strerror(failure)}")
      exit code
    }
  }
}
