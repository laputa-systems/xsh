let helper = fp"${ARGV[0]}"
let ready = fp"${ARGV[1]}"
let command = process.command_argv(helper, ["os-probe", "ready-sleep", ready.display()])
let h = spawn command?

while ! ready.exists()? {
  time.sleep(10ms)?
}

h.cancel(signal: "TERM", kill_after: 0ms)?
