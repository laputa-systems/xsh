#!/bin/xsh
use lib.gnu

const USAGE = """Usage: unshare [options] [<program> [<argument>...]]

Run a program with some namespaces unshared from the parent.

Options:
 -m, --mount               unshare mounts namespace
 -u, --uts                 unshare UTS namespace (hostname etc)
 -i, --ipc                 unshare System V IPC namespace
 -n, --net                 unshare network namespace
 -p, --pid                 unshare pid namespace
 -U, --user                unshare user namespace
 -C, --cgroup              unshare cgroup namespace
 -T, --time                unshare time namespace

 --mount-proc[=<dir>]      mount proc filesystem first (implies --mount)
 --propagation slave|shared|private|unchanged
                           modify mount propagation in mount namespace

 -r, --map-root-user       map current user to root (implies --user)
 -f, --fork                fork before launching <program>

 -h, --help                display this help
 -V, --version             display version
"""

type Options = {
  mnt: Bool, uts: Bool, ipc: Bool, net: Bool, pid: Bool, user: Bool, cgroup: Bool, time: Bool,
  mnt_file: Str?, uts_file: Str?, ipc_file: Str?, net_file: Str?, pid_file: Str?,
  user_file: Str?, cgroup_file: Str?, time_file: Str?,
  mount_proc: Str?, propagation: Str?, map_root: Bool, fork: Bool,
  help: Bool, version: Bool, operands: List[Str],
}

# The files `--mount=FILE` and friends would keep a namespace alive in. The
# namespace must be bound into the filesystem of the parent's mount namespace,
# which only a process outside the new namespaces can do, so it is refused
# rather than silently ignored.
pure persistent_file(opts: Options) -> Str? {
  let named = [
    {flag: "--mount", file: opts.mnt_file}, {flag: "--uts", file: opts.uts_file},
    {flag: "--ipc", file: opts.ipc_file}, {flag: "--net", file: opts.net_file},
    {flag: "--pid", file: opts.pid_file}, {flag: "--user", file: opts.user_file},
    {flag: "--cgroup", file: opts.cgroup_file}, {flag: "--time", file: opts.time_file},
  ]
  for entry in named {
    if let file = entry.file { return f"{entry.flag}={file}" }
  }
  null
}

# Resolve a launch failure before starting so the conventional 126 and 127
# statuses and the util-linux wording hold even though the command runs in a
# child.
proc check_launch(command: Str) [fs, process, env] {
  if command.find("/") != null {
    let target = fp"{command}"
    match fs.stat(target, follow_symlinks: true) {
      Ok(meta) => {
        if meta.kind != "file" or meta.mode.bit_and(0o111) == 0 {
          gnu.error(f"failed to execute {command}: Permission denied")
          exit 126
        }
      }
      Err(failure) => {
        gnu.error(f"failed to execute {command}: {gnu.strerror(failure)}")
        exit if (failure.errno ?? gnu.errno(failure)) == 2 { 127 } else { 126 }
      }
    }
  } else if let Err(failure) = process.which(command) {
    let missing = failure is NotFound
    gnu.error(f"failed to execute {command}: {if missing { "No such file or directory" } else { "Permission denied" }}")
    exit if missing { 127 } else { 126 }
  }
}

# A command that died from a signal is reported the way the util-linux tool
# does: by dying from the same signal, so the caller sees a signal death rather
# than an exit status. The runtime's own handler is replaced first.
proc relay_signal(status: Status) [process, error] {
  return when !status.signaled()
  let number = status.signal_number() ?? 0
  for entry in process.signals() {
    if entry.number == number {
      let _ = process.set_signal_action(entry.name, "default")
      let _ = process.kill(process.current_pid()?, entry.name)
      return
    }
  }
}

proc main(...argv: List[Str]) [fs, error, process, env, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1, permute: false},
    mnt: {form: "-m", default: false},
    uts: {form: "-u", default: false},
    ipc: {form: "-i", default: false},
    net: {form: "-n", default: false},
    pid: {form: "-p", default: false},
    user: {form: "-U", default: false},
    cgroup: {form: "-C", default: false},
    time: {form: "-T", default: false},
    mnt_file: {form: "--mount[=FILE]", optional_default: ""},
    uts_file: {form: "--uts[=FILE]", optional_default: ""},
    ipc_file: {form: "--ipc[=FILE]", optional_default: ""},
    net_file: {form: "--net[=FILE]", optional_default: ""},
    pid_file: {form: "--pid[=FILE]", optional_default: ""},
    user_file: {form: "--user[=FILE]", optional_default: ""},
    cgroup_file: {form: "--cgroup[=FILE]", optional_default: ""},
    time_file: {form: "--time[=FILE]", optional_default: ""},
    mount_proc: {form: "--mount-proc[=DIR]", optional_default: "/proc"},
    propagation: {form: "--propagation TYPE"},
    map_root: {form: "-r --map-root-user", default: false},
    fork: {form: "-f --fork", default: false},
    help: {form: "-h --help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    operands: {form: "...COMMAND"},
  })?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("unshare"); return }

  # An empty optional value (`--mount=`) is a file named "", which the kernel
  # would refuse; any value at all asks for persistence.
  if let named = persistent_file(opts) {
    gnu.error(f"{named}: persistent namespace files are not supported")
    exit 1
  }

  let mode = opts.propagation ?? "unchanged"
  if mode not in ["private", "shared", "slave", "unchanged"] {
    gnu.error(f"unsupported propagation mode: {mode}")
    exit 1
  }

  let mount_proc = opts.mount_proc
  let kinds: List[Str] = collect {
    yield "mnt" when opts.mnt or mount_proc != null
    yield "uts" when opts.uts
    yield "ipc" when opts.ipc
    yield "net" when opts.net
    yield "pid" when opts.pid
    yield "user" when opts.user or opts.map_root
    yield "cgroup" when opts.cgroup
    yield "time" when opts.time
  }

  # A new mount namespace starts private unless told otherwise, so mounts made
  # inside it never propagate out to the parent.
  let propagation = if "mnt" in kinds {
    if opts.propagation == null { "private" } else { mode }
  } else {
    "unchanged"
  }

  let words = if opts.operands.is_empty() { [env.get_or("SHELL", "/bin/sh") ?? "/bin/sh"] } else { opts.operands }
  check_launch(words[0])
  io.flush_stdout()
  let plan = process.command_argv(words[0], words)
  let outcome = linux.run_in_namespaces(
    plan,
    unshare: kinds,
    map_root_user: opts.map_root,
    propagation:,
    mount_proc: if let dir = mount_proc { fp"{dir}" } else { null },
    fork: opts.fork,
  )
  match outcome {
    Ok(status) => {
      if status.kind == "exec" {
        gnu.error(f"failed to execute {words[0]}")
        exit 126
      }
      relay_signal(status)
      exit status.shell_code()?
    }
    Err(failure) => {
      gnu.error(failure.message)
      exit 1
    }
  }
}
