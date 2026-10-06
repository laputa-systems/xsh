#!/bin/xsh
use lib.gnu
use lib.system_control as control

proc main(...argv: List[Str]) {
  let opts = cli.applet(argv, {
    gnu: {unsupported: {"-c": "read-and-clear kernel log is not available", "--read-clear": "read-and-clear kernel log is not available", "-C": "clearing the kernel log is not available", "--clear": "clearing the kernel log is not available", "-D": "console log control is not available", "--console-off": "console log control is not available", "-E": "console log control is not available", "--console-on": "console log control is not available", "-n": "console log level control is not available", "--console-level": "console log level control is not available", "-r": "raw kernel log metadata is not available", "--raw": "raw kernel log metadata is not available", "-x": "kernel log metadata decoding is not available", "--decode": "kernel log metadata decoding is not available", "-T": "kernel log timestamps are not available", "--ctime": "kernel log timestamps are not available", "-l": "kernel log priority filtering is not available", "--level": "kernel log priority filtering is not available", "-f": "kernel log facility filtering is not available", "--facility": "kernel log facility filtering is not available", "-w": "kernel log following is not available", "--follow": "kernel log following is not available", "-s": "kernel log buffer sizing is not available", "--buffer-size": "kernel log buffer sizing is not available", "--json": "typed kernel log metadata is not available", "--color": "kernel log colors are not available"}},
    no_time: {form: "-t --notime", default: false},
    help: {form: "-h --help", default: false, stop: true}, version: {form: "-V --version", default: false, stop: true}, operands: {form: "...ARG"},
  })?
  if opts.help { gnu.help("Usage: dmesg [-t]\nDisplay available kernel log messages."); return }
  if opts.version { gnu.version("dmesg"); return }
  if ! opts.operands.is_empty() { gnu.extra_operand(opts.operands[0]) }
  for message in linux.dmesg()? {
    let text = control.kernel_message(message, opts.no_time)
    gnu.write_text(text + "\n")
  }
}
