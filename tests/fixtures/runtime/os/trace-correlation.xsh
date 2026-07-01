let helper = fp"${ARGV[0]}"

on USR1 --pre-cancel=0ms [time, error] {
  time.sleep(50ms)?
  abort(0)
}

let command = process.command_argv(
  helper,
  ["os-probe", "trap-and-wait", fp"${ARGV[1]}".display(), fp"${ARGV[2]}".display(), "USR1"],
)

let h = spawn command?
let _sender = process.spawn(process.command_argv(helper, ["os-probe", "signal-parent-after", "USR1", "50"]))?
let _status = wait h
