let ready = fp"{args[0]}"
let leaked = fp"{args[1]}"
let helper = fp"{args[2]}"
let command = process.command_argv(helper, ["os-probe", "fork-new-session-leak", ready.display(), leaked.display()])
process.run(command)?
