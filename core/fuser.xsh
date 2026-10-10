#!/bin/xsh

type Options = {verbose: Bool, silent: Bool, mount: Bool, kill: Bool, all: Bool, namespace: Str, signal: Str, help: Bool, operands: List[Str]}
type Holder = {pid: Int, command: Str, user: Str, access: Str}

# One access string per holder, in psmisc column order: file (f, or F when it
# is open for writing), root, cwd, executable, memory map.
pure access_string(files: List[LinuxOpenFile]) -> Str {
  var file = "."
  var root = "."
  var cwd = "."
  var exe = "."
  for entry in files {
    if entry.fd_label == "rtd" { root = "r" } else if entry.fd_label == "cwd" { cwd = "c" } else if entry.fd_label == "txt" { exe = "e" } else if entry.access == "w" or entry.access == "u" { file = "F" } else if file == "." { file = "f" }
  }
  f"{file}{root}{cwd}{exe}."
}

# Plain output shows only the non-file letters; f and F are verbose-only.
pure plain_letters(access: Str) -> Str {
  access.split("") |> drop(1) |> where . != "." |> collect |> join("")
}

proc main(...argv: List[Str]) [fs, process, io, error] {
  var normalized: List[Str] = []
  var option_words = true
  for word in argv {
    if word == "--" { option_words = false }
    let signal_name = word.split("") |> drop(1).join("")
    if option_words and word.starts_with("-") and ! word.starts_with("--") and word.byte_len() > 1 and process.signal(signal_name) is Ok(_) { normalized = normalized.push("--signal").push(signal_name) } else { normalized = normalized.push(word) }
  }
  let opts: Options = cli.applet(normalized, {gnu: {status: 1}, verbose: {form: "-v --verbose", default: false}, silent: {form: "-s --silent", default: false}, mount: {form: "-m --mount", default: false}, kill: {form: "-k --kill", default: false}, all: {form: "-a --all", default: false}, namespace: {form: "-n --namespace SPACE", default: "file"}, signal: {form: "--signal SIGNAL", default: "KILL"}, help: {form: "--help", default: false}, operands: {form: "...NAME"}})?
  if opts.help { print "Usage: fuser [-vsmka] [-SIGNAL] [-n file|tcp|udp] NAME..."; return }
  if opts.operands.is_empty() { eprint "No process specification given"; exit 1 }
  if opts.namespace not in ["file", "tcp", "udp"] { eprint f"Invalid namespace name {opts.namespace}"; exit 1 }
  let requested = process.signal(opts.signal)
  if let Err(_) = requested { eprint f"{opts.signal}: unknown signal"; exit 1 }
  let signal = requested?
  let self_pid = process.current_pid()?
  let snapshot: List[ProcessEntry] = process.list()? |> collect
  let files: List[LinuxOpenFile] = linux.open_files()? |> collect
  if opts.verbose and ! opts.silent { io.write_stderr("                     USER        PID ACCESS COMMAND\n")? }
  var matched = false
  var failed = false
  for operand in opts.operands {
    var protocol = opts.namespace
    var spec = operand
    if opts.namespace == "file" and rx"^[0-9]+/(tcp|udp)$".matches(operand) {
      let parts = operand.split("/")
      spec = parts[0]
      protocol = parts[1]
    }
    var pids: List[Int] = []
    var rows: List[LinuxOpenFile] = []
    if protocol == "file" {
      let metadata = fs.stat(fp"{operand}", follow_symlinks: true)
      if let Err(_) = metadata { eprint f"Specified filename {operand} does not exist."; continue }
      let target = metadata?
      rows = if opts.mount { files |> where .dev == target.dev |> collect } else { files |> where .dev == target.dev and .inode == target.ino |> collect }
      pids = [row.pid for row in rows].to_set().to_list() |> sort-by .
    } else {
      let port = spec.parse_int()
      if let Err(_) = port { eprint f"Cannot resolve local port {spec}: unsupported port syntax"; exit 1 }
      let listeners = process.port(port?)? |> where .protocol.starts_with(protocol) |> collect
      pids = [row.pid for row in listeners].to_set().to_list() |> sort-by .
    }
    let label = if opts.namespace == "file" and protocol == "file" { f"{operand}:" } else { f"{spec}/{protocol}:" }
    continue when pids.is_empty() and ! opts.all
    matched = matched or ! pids.is_empty()
    if ! opts.silent {
      var table: List[Holder] = []
      var letters = ""
      for pid in pids {
        let mine = rows |> where .pid == pid |> collect
        let access = if protocol == "file" { access_string(mine) } else { "F...." }
        let named = snapshot |> where .pid == pid |> collect
        table = table.push({pid: pid, command: if named.is_empty() { "?" } else { named[0].command }, user: if named.is_empty() { "(unknown)" } else { named[0].user }, access: access})
        letters = letters + plain_letters(access)
        io.write_stdout(f"{pid:>6}")?
      }
      if table.is_empty() { io.write_stderr(f"{label}\n")? } else if opts.verbose {
        var first = true
        for holder in table {
          io.write_stderr(f"{if first { label } else { "" }:<20} {holder.user:<8} {holder.pid:>6} {holder.access} {holder.command}\n")?
          first = false
        }
      } else { io.write_stderr(f"{label:<20}{letters}\n")? }
      io.flush_stdout()?
      io.flush_stderr()?
    }
    if opts.kill {
      for pid in pids {
        continue when pid == self_pid
        if let Err(failure) = process.kill(pid, signal.name) { eprint f"Kill process {pid} failed: {failure.message}"; failed = true }
      }
    }
  }
  if ! matched or failed { exit 1 }
}
