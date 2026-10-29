##! Test support: runs one program inside the network namespace behind a bound
##! namespace file, and dies with its parent. Arguments: the namespace file, the
##! program, its arguments.
proc main(...argv: List[Str]) [process, error] {
  linux.set_parent_death_signal(9)?
  let plan = process.command_argv(argv[1], argv[2..])
  let status = linux.run_in_namespaces(plan, join: [fp"{argv[0]}"])?
  exit status.shell_code()?
}
