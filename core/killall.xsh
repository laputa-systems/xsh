#!/bin/xsh
use lib.procps

type Options = {signal: Str, quiet: Bool, verbose: Bool, ignore_case: Bool, regexp: Bool, user: Str?, help: Bool, operands: List[Str]}
proc main(...argv: List[Str]) [process, error] {
  var normalized: List[Str] = []
  var option_words = true
  for word in argv {
    if word == "--" { option_words = false }
    let signal_name = word.split("") |> drop(1).join("")
    if option_words and word.starts_with("-") and ! word.starts_with("--") and word.byte_len() > 1 and process.signal(signal_name) is Ok(_) { normalized = normalized.push("--signal").push(signal_name) } else { normalized = normalized.push(word) }
  }
  let opts: Options = cli.applet(normalized, {gnu: {status: 2}, signal: {form: "-s --signal SIGNAL", default: "TERM"}, quiet: {form: "-q --quiet", default: false}, verbose: {form: "-v --verbose", default: false}, ignore_case: {form: "-I --ignore-case", default: false}, regexp: {form: "-r --regexp", default: false}, user: {form: "-u --user USER"}, help: {form: "--help", default: false}, operands: {form: "...NAME"}})?
  if opts.help { print "Usage: killall [-qvIr] [-s SIGNAL] [-u USER] NAME..."; return }
  if opts.operands.is_empty() { eprint "killall: process name required"; exit 2 }
  let signal = process.signal(opts.signal)?
  let self_pid = process.current_pid()?
  let rows: List[ProcessEntry] = process.list()? |> collect
  var failed = false
  var signaled: List[Int] = []
  for name in opts.operands {
    let base = procps.selector()
    let selected = procps.select(rows, {...base, users: if opts.user == null { [] } else { [opts.user] }, names: if opts.regexp { [] } else { [name] }, pattern: if opts.regexp { name } else { null }, ignore_case: opts.ignore_case}, self_pid)?
    var matched = false
    for row in selected {
      matched = true
      continue when row.pid in signaled
      if let Err(failure) = process.kill(row.pid, signal.name) {
        if ! opts.quiet { eprint f"killall: {row.pid}: {failure.message}" }
        failed = true
      } else {
        signaled = signaled.push(row.pid)
        if opts.verbose { eprint f"Killed {row.command}({row.pid}) with signal {signal.name}" }
      }
    }
    if ! matched { failed = true; if ! opts.quiet { eprint f"{name}: no process found" } }
  }
  if failed { exit 1 }
}
