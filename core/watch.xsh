#!/bin/xsh
use lib.gnu

type Options = {interval: Str, no_title: Bool, exec: Bool, exit_error: Bool, exit_change: Bool, help: Bool, operands: List[Str]}
proc main(...argv: List[Str]) [process, env, time, error, io] {
  let opts: Options = cli.applet(argv, {gnu: {status: 1, permute: false}, interval: {form: "-n --interval SECONDS", default: "2"}, no_title: {form: "-t --no-title", default: false}, exec: {form: "-x --exec", default: false}, exit_error: {form: "-e --errexit", default: false}, exit_change: {form: "-g --chgexit", default: false}, help: {form: "--help", default: false}, operands: {form: "...COMMAND"}})?
  if opts.help { print "Usage: watch -x [-n SECONDS] [-t] [-e] [-g] COMMAND [ARG...]"; return }
  if ! opts.exec { eprint "watch: shell command strings are unsupported; use -x for an argument vector"; exit 1 }
  if opts.operands.is_empty() { eprint "watch: command required"; exit 1 }
  let parsed = opts.interval.parse_float()
  if let Err(_) = parsed { eprint f"watch: failed to parse argument: '{opts.interval}': Invalid argument"; exit 1 }
  # procps clamps a small or negative interval to its 0.1 second floor.
  let interval = if parsed? < 0.1 { 0.1 } else { parsed? }
  let command = opts.operands[0]
  let command_args = opts.operands |> drop(1)
  var previous: Str? = null
  while true {
    let captured = run.capture --text $command @command_args
    gnu.write_text("\u{1b}[H\u{1b}[2J")
    if ! opts.no_title { print f"Every {interval.format(1)}s: {opts.operands.join(" ")}\n" }
    gnu.write_text(captured.stdout)
    gnu.write_text(captured.stderr)
    io.flush_stdout()?
    if opts.exit_error and ! captured.status.exited_with(0) { exit 1 }
    let combined = captured.stdout + captured.stderr
    # The first run only establishes the baseline; a later differing run ends the loop.
    return when opts.exit_change and previous != null and previous != combined
    previous = combined
    time.sleep(time.millis((interval * 1000.0).ceil()?))
  }
}
