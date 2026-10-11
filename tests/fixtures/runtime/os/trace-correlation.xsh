let helper = fp"{args[0]}"
let ready = fp"{args[1]}"

on USR1 --pre-cancel=0ms [time, error] {
  time.sleep(50ms)?
  exit 0
}

let command = process.command_argv(
  helper,
  ["os-probe", "trap-and-wait", fp"{args[1]}".display(), fp"{args[2]}".display(), "USR1"],
)

let h = spawn command?
# The marker is written after the child installs its signal handler; process
# creation alone does not guarantee that a forwarded signal can be caught.
let started = time.now()
while ! ready.exists()? {
  if time.now() - started > 5000 {
    h.cancel(signal: "KILL", kill_after: 0ms)?
    eprint "the signal fixture did not become ready"
    exit 1
  }
  time.sleep(10ms)?
}
let _sender = process.spawn(process.command_argv(helper, ["os-probe", "signal-parent-after", "USR1", "50"]))?
let _status = wait h
