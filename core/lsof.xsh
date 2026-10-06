#!/bin/xsh
use lib.procps

type Options = {pids: Str?, users: Str?, terse: Bool, command: Str?, help: Bool, operands: List[Str]}
pure file_type(kind: Str) -> Str {
  match kind { "character" => "CHR"; "block" => "BLK"; "directory" => "DIR"; "file" => "REG"; "pipe" => "FIFO"; "socket" => "sock"; else => kind }
}

proc main(...argv: List[Str]) [fs, process, error] {
  let opts: Options = cli.applet(argv, {gnu: {status: 2}, pids: {form: "-p PID"}, users: {form: "-u USER"}, terse: {form: "-t", default: false}, command: {form: "-c COMMAND"}, help: {form: "--help", default: false}, operands: {form: "...FILE"}})?
  if opts.help { print "Usage: lsof [-p PID,...] [-u USER,...] [-c COMMAND] [-t] [FILE...]"; return }
  let base = procps.selector()
  let selector: procps.Selector = {...base, pids: if opts.pids == null { [] } else { procps.ids(opts.pids)? }, users: (opts.users ?? "").split(",") |> where . != "", pattern: if opts.command == null { null } else { f"^{opts.command}" }}
  let snapshot: List[ProcessEntry] = process.list()? |> collect
  let selected = procps.select(snapshot, selector, 0)?
  let wanted = [fs.stat(fp"{name}", follow_symlinks: true)? for name in opts.operands]
  var found: List[Int] = []
  if ! opts.terse { print "COMMAND PID USER FD TYPE NODE NAME" }
  for row in selected {
    let files = linux.open_files(row.pid)?
    for file in files {
      if ! wanted.is_empty() {
        let matches = wanted |> where file.dev == .dev and file.inode == .ino
        continue when matches.is_empty()
      }
      if row.pid not in found { found = found.push(row.pid); if opts.terse { print $row.pid } }
      if ! opts.terse { print f"{row.command} {row.pid} {row.user} {file.fd_label}{file.access} {file_type(file.type)} {file.inode} {file.path}" }
    }
  }
  if found.is_empty() { exit 1 }
}
