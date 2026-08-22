#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type DateOptions = {utc: Bool, operands: List[Str]}

proc main(...argv: List[Str]) [process, error] {
  let opts: DateOptions = cli.applet(
    argv,
    {
      utc: {
        form: "-u",
        default: false,
      },
      operands: {
        form: "...ARG",
      },
    },
  )?
  if opts.operands.len() > 1 {
    return Err(AppletError.Usage("date: expected at most one format operand"))
  }

  let format_arg = opts.operands.get(0, "+%a %b %d %H:%M:%S %Y")
  let format = if format_arg.starts_with("+") { format_arg.replace("+", "") } else { format_arg }
  let host_format = format.replace("%:z", "%z")
  let date_argv = if opts.utc {
    ["date", "-u", f"+${host_format}"]
  } else {
    ["date", f"+${host_format}"]
  }
  let status = process.run(process.command_argv("date", date_argv))?

  if status.ok {
    return
  }

  if status.exited() {
    abort(status.exit_code()?)
  }

  return Err(AppletError.Usage("date: command was signaled"))
}
