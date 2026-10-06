#!/bin/xsh
use lib.procps

type Options = {single: Bool, omit: Str?, separator: Str, help: Bool, operands: List[Str]}
proc main(...argv: List[Str]) [process, error] {
  let opts: Options = cli.applet(argv, {gnu: {status: 2}, single: {form: "-s --single-shot", default: false}, omit: {form: "-o --omit-pid PID"}, separator: {form: "-S --separator TEXT", default: " "}, help: {form: "--help", default: false}, operands: {form: "...PROGRAM"}})?
  if opts.help { print "Usage: pidof [-s] [-o PID,...] [-S SEP] PROGRAM..."; return }
  if opts.operands.is_empty() { eprint "pidof: program name required"; exit 2 }
  let omitted = if opts.omit == null { [] } else { procps.ids(opts.omit)? }
  let self_pid = process.current_pid()?
  let rows: List[ProcessEntry] = process.list()? |> collect
  var found: List[Int] = []
  for name in opts.operands {
    for row in rows |> sort-by(desc: true) .pid {
      if row.pid != self_pid and row.pid not in omitted and (row.command == name or row.argv0 == name or fp"{row.argv0}".name() == name) {
        if row.pid not in found { found = found.push(row.pid) }
        break when opts.single
      }
    }
  }
  if found.is_empty() { exit 1 }
  let output = [f"{pid}" for pid in found].join(opts.separator)
  print $output
}
