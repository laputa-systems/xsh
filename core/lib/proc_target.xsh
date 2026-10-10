##! The procfs files that name another process's namespaces, root directory and
##! working directory: what a program opens or enters to act inside a target.

## The handle of the `kind` namespace (`user`, `mnt`, `net`, ...) of `pid`.
export pure namespace_file(pid: Int, kind: Str) -> Path {
  fp"/proc/{pid}/ns/{kind}"
}

## The root directory of `pid`, as that process sees it.
export pure root_dir(pid: Int) -> Path {
  fp"/proc/{pid}/root"
}

## The working directory of `pid`.
export pure cwd_dir(pid: Int) -> Path {
  fp"/proc/{pid}/cwd"
}
