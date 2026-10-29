##! Test support: replaces itself with a program that dies with its parent.
##! Arguments: the program, then the program's whole argv.
proc main(...argv: List[Str]) [process, error] {
  linux.set_parent_death_signal(9)?
  unix.exec(process.command_argv(argv[0], argv[1..]))?
}
