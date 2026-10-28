##! Test support: binds the network namespace of this process onto a file so
##! the namespace outlives the process. Argument: the file.
proc main(...argv: List[Str]) [fs, process, error] {
  linux.mount(b"/proc/self/ns/net", fp"{argv[0]}", options: ["bind"])
}
