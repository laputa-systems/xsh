#!/bin/xsh
use lib.gnu
use lib.proc_target as target_files

const USAGE = """Usage: nsenter [options] [<program> [<argument>...]]

Run a program with namespaces of other processes.

Options:
 -t, --target <PID>            target process to get namespaces from
 -m, --mount[=<file>]          enter mount namespace
 -u, --uts[=<file>]            enter UTS namespace (hostname etc)
 -i, --ipc[=<file>]            enter System V IPC namespace
 -n, --net[=<file>]            enter network namespace
 -p, --pid[=<file>]            enter pid namespace
 -C, --cgroup[=<file>]         enter cgroup namespace
 -U, --user[=<file>]           enter user namespace
 -T, --time[=<file>]           enter time namespace
 -S, --setuid <uid>            set uid in entered namespace
 -G, --setgid <gid>            set gid in entered namespace
     --preserve-credentials    do not set the uid and gid to 0 when entering
                               a user namespace
 -r, --root[=<dir>]            set the root directory
 -w, --wd[=<dir>]              set the working directory
 -F, --no-fork                 do not fork before exec'ing <program>

 -h, --help                    display this help
 -V, --version                 display version
"""

type Options = {
  target: Str?,
  user: Str?, cgroup: Str?, ipc: Str?, uts: Str?, net: Str?, pid: Str?, mnt: Str?, time: Str?,
  setuid: Str?, setgid: Str?, preserve: Bool, root: Str?, wd: Str?, no_fork: Bool,
  help: Bool, version: Bool, operands: List[Str],
}

type Entry = {name: Str, file: Str?}

# A short option's optional value attaches directly, so `-m=FILE` arrives as
# `=FILE`; the long form `--mount=FILE` arrives as `FILE`.
pure value_of(given: Str?) -> Str? {
  if let text = given {
    return text.byte_slice(1) when text.starts_with("=")
    return text
  }
  null
}

pure parse_number(text: Str) -> Int? {
  return null when text.starts_with("+")
  if let Ok(number) = text.parse_int() { number } else { null }
}

proc parse_id(text: Str, what: Str) [process, env] -> Int {
  let parsed = parse_number(text)
  if parsed == null {
    gnu.error(f"failed to parse {what}: {gnu.quote_value(text)}")
    exit 1
  }
  let id = parsed
  if id < 0 or id > 4294967295 {
    gnu.error(f"failed to parse {what}: {gnu.quote_value(text)}: Result not representable")
    exit 1
  }
  id
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
    target: {form: "-t --target PID"},
    mnt: {form: "-m --mount[=FILE]", optional_default: ""},
    uts: {form: "-u --uts[=FILE]", optional_default: ""},
    ipc: {form: "-i --ipc[=FILE]", optional_default: ""},
    net: {form: "-n --net[=FILE]", optional_default: ""},
    pid: {form: "-p --pid[=FILE]", optional_default: ""},
    cgroup: {form: "-C --cgroup[=FILE]", optional_default: ""},
    user: {form: "-U --user[=FILE]", optional_default: ""},
    time: {form: "-T --time[=FILE]", optional_default: ""},
    setuid: {form: "-S --setuid UID"},
    setgid: {form: "-G --setgid GID"},
    preserve: {form: "--preserve-credentials", default: false},
    root: {form: "-r --root[=DIR]", optional_default: ""},
    wd: {form: "-w --wd[=DIR]", optional_default: ""},
    no_fork: {form: "-F --no-fork", default: false},
    help: {form: "-h --help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    operands: {form: "...COMMAND"},
  })?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("nsenter"); return }

  var target: Int? = null
  if let text = opts.target {
    let parsed = parse_number(text)
    if parsed == null or parsed <= 0 {
      gnu.error("invalid PID argument")
      exit 1
    }
    target = parsed
  }
  var uid: Int? = null
  var gid: Int? = null
  if let text = opts.setuid { uid = parse_id(text, "uid") }
  if let text = opts.setgid { gid = parse_id(text, "gid") }

  # The order the kernel permits entering in: the user namespace first, since
  # it decides the permission to enter the others.
  let order: List[Entry] = [
    {name: "user", file: value_of(opts.user)},
    {name: "cgroup", file: value_of(opts.cgroup)},
    {name: "ipc", file: value_of(opts.ipc)},
    {name: "uts", file: value_of(opts.uts)},
    {name: "net", file: value_of(opts.net)},
    {name: "pid", file: value_of(opts.pid)},
    {name: "mnt", file: value_of(opts.mnt)},
    {name: "time", file: value_of(opts.time)},
  ]
  var join: List[Path] = []
  var entered: List[Str] = []
  var needs_target = false
  for entry in order {
    if let file = entry.file {
      entered += [entry.name]
      if file == "" {
        needs_target = true
        if let pid = target {
          join += [target_files.namespace_file(pid, entry.name)]
        }
      } else {
        join += [fp"{file}"]
      }
    }
  }
  if entered.is_empty() {
    gnu.error("no namespace specified")
    exit 1
  }
  if needs_target and target == null {
    gnu.error("no target PID specified")
    exit 1
  }

  let wd_file = value_of(opts.wd)
  let root_file = value_of(opts.root)
  var cwd_path: Path? = null
  var root_path: Path? = null
  if let file = root_file {
    if file != "" {
      root_path = fp"{file}"
    } else if let pid = target {
      root_path = target_files.root_dir(pid)
    } else {
      gnu.error("neither filename nor target pid supplied for root")
      exit 1
    }
  }
  if let file = wd_file {
    if file != "" {
      cwd_path = fp"{file}"
    } else if let pid = target {
      cwd_path = target_files.cwd_dir(pid)
    } else {
      gnu.error("neither filename nor target pid supplied for cwd")
      exit 1
    }
  }

  # Entering a user namespace gives the process the root of that namespace
  # unless the caller keeps its own credentials or names others.
  let user_entered = "user" in entered
  var set_uid: Int? = uid
  var set_gid: Int? = gid
  var drop_groups = false
  if user_entered and !opts.preserve {
    if set_uid == null { set_uid = 0 }
    if set_gid == null { set_gid = 0 }
    drop_groups = true
  }

  # A pid or time namespace applies to children, so the command is started in
  # one more process unless told not to.
  let fork = ("pid" in entered or "time" in entered) and !opts.no_fork

  let words = if opts.operands.is_empty() { [env.get_or("SHELL", "/bin/sh") ?? "/bin/sh"] } else { opts.operands }
  check_launch(words[0])
  io.flush_stdout()
  let plan = process.command_argv(words[0], words)
  let outcome = linux.run_in_namespaces(
    plan,
    join:,
    fork:,
    root: root_path,
    cwd: cwd_path,
    uid: set_uid,
    gid: set_gid,
    drop_groups:,
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
