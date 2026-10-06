#!/bin/xsh

type Options = {verbose: Bool, help: Bool, operands: List[Str]}
proc main(...argv: List[Str]) [fs, process, error] {
  let opts: Options = cli.applet(argv, {gnu: {status: 2}, verbose: {form: "-v --verbose", default: false}, help: {form: "--help", default: false}, operands: {form: "...FILE"}})?
  if opts.help { print "Usage: fuser [-v] FILE..."; return }
  if opts.operands.is_empty() { eprint "fuser: filename required"; exit 2 }
  let files: List[LinuxOpenFile] = linux.open_files()? |> collect
  var matched = false
  for name in opts.operands {
    let metadata = fs.stat(fp"{name}", follow_symlinks: true)?
    let rows = files |> where .dev == metadata.dev and .inode == metadata.ino
    let pids = [row.pid for row in rows].to_set().to_list()
    if ! pids.is_empty() {
      matched = true
      eprint f"{name}:"
      let output = [f"{pid}" for pid in pids].join(" ")
      print $output
      if opts.verbose { for row in rows { eprint f"{row.pid} {row.command} {row.type} {row.fd}" } }
    }
  }
  if ! matched { exit 1 }
}
