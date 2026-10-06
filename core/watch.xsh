#!/bin/xsh
use lib.gnu

type Options = {interval: Str, no_title: Bool, exec: Bool, exit_error: Bool, help: Bool, operands: List[Str]}
proc main(...argv: List[Str]) [process, env, time, error, io] {
  let opts: Options = cli.applet(argv, {gnu: {status: 2, permute: false}, interval: {form: "-n --interval SECONDS", default: "2"}, no_title: {form: "-t --no-title", default: false}, exec: {form: "-x --exec", default: false}, exit_error: {form: "-e --errexit", default: false}, help: {form: "--help", default: false}, operands: {form: "...COMMAND"}})?
  if opts.help { print "Usage: watch -x [-n SECONDS] [-t] [-e] COMMAND [ARG...]"; return }
  if ! opts.exec { eprint "watch: shell command strings are unsupported; use -x for an argument vector"; exit 2 }
  if opts.operands.is_empty() { eprint "watch: command required"; exit 2 }
  let interval = opts.interval.parse_float()?
  if interval <= 0.0 { eprint "watch: interval must be positive"; exit 2 }
  let command = opts.operands[0]
  let command_args = opts.operands |> drop(1)
  while true {
    let captured = run.capture --text $command @command_args
    gnu.write_text("\u{1b}[H\u{1b}[2J")
    if ! opts.no_title { print f"Every {interval}s: {opts.operands.join(" ")}\n" }
    gnu.write_text(captured.stdout)
    gnu.write_text(captured.stderr)
    io.flush_stdout()?
    if opts.exit_error and ! captured.status.exited_with(0) { exit 1 }
    time.sleep(time.millis((interval * 1000.0).ceil()?))
  }
}
