#!/bin/xsh
error AppletError = Usage(message: Str) : Usage

type DateOptions = {utc: Bool, operands: List[Str]}

proc main(...argv: List[Str]) [time, error] {
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

  print (time.format(time.now(), format, utc: opts.utc)?)
}
