let ready = fp"${ARGV[0]}"
let leaked = fp"${ARGV[1]}"
let helper = fp"${ARGV[2]}"
let command = process.command_argv(helper, ["os-probe", "group-leak", ready.display(), leaked.display()])
process.run(command)?
