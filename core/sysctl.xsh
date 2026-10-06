#!/bin/xsh
use lib.gnu
use lib.system_control as control

type Operation = {key: Str, value: Str?, ignore_failure: Bool}

proc main(...argv: List[Str]) {
  let opts = cli.applet(argv, {
    gnu: {unsupported: {"-a": "sysctl enumeration is not available", "--all": "sysctl enumeration is not available", "-A": "sysctl enumeration is not available", "--system": "system configuration precedence and glob expansion are not available", "-r": "sysctl pattern enumeration is not available", "--pattern": "sysctl pattern enumeration is not available"}},
    write: {form: "-w --write", default: false}, values: {form: "-n --values", default: false}, names: {form: "-N --names", default: false}, quiet: {form: "-q --quiet", default: false}, binary: {form: "-b --binary", default: false}, ignore: {form: "-e --ignore", default: false},
    load: {form: "-p --load[=FILE]", optional_default: "/etc/sysctl.conf"},
    help: {form: "-h --help", default: false, stop: true}, version: {form: "-V --version", default: false, stop: true}, operands: {form: "...KEY"},
  })?
  if opts.help { gnu.help("Usage: sysctl [-n|-N] [-q] [-e] [-w] KEY[=VALUE]... | -p[FILE]\nRead and write named kernel parameters."); return }
  if opts.version { gnu.version("sysctl"); return }
  if opts.values and opts.names { gnu.usage_error("--values and --names are mutually exclusive") }
  if opts.binary and opts.names { gnu.usage_error("--binary and --names are mutually exclusive") }
  var operations: List[Operation] = []
  if opts.load != null {
    let files = if opts.operands.is_empty() { [fp"{opts.load}"] } else { [fp"{opts.load}"].extend(collect { for name in opts.operands { yield fp"{name}" } }) }
    for item in control.load_assignments(files)? { operations += [{key: item.key, value: item.value, ignore_failure: item.ignore_failure}] }
  } else {
    if opts.operands.is_empty() { gnu.usage_error("no variables specified") }
    for operand in opts.operands {
      if "=" in operand {
        let item = control.assignment(operand)?
        operations += [{key: item.key, value: item.value, ignore_failure: false}]
      } else {
        if opts.write { gnu.usage_error(f"expected key=value: {operand}") }
        let item = control.assignment(operand + "=")?
        operations += [{key: item.key, value: null, ignore_failure: false}]
      }
    }
  }
  var failed = false
  for item in operations {
    let outcome = if item.value == null { linux.sysctl_get(item.key) } else {
      match linux.sysctl_set(item.key, item.value ?? "") { Ok(_) => Ok(item.value ?? ""), Err(failure) => Err(failure) }
    }
    match outcome {
      Ok(value) => {
        if opts.quiet and item.value != null { continue }
        let text = if opts.names { item.key } else if opts.values or opts.binary { value } else { f"{item.key} = {value}" }
        gnu.write_text(text + (if opts.binary { "" } else { "\n" }))
      }
      Err(failure) => {
        if item.ignore_failure or (opts.ignore and gnu.errno(failure) == 2) { continue }
        gnu.error(failure.message)
        failed = true
      }
    }
  }
  if failed { exit 1 }
}
