#!/bin/xsh
use lib.gnu
use lib.awk as interpreter

const USAGE = """Usage: awk [-F separator] [-v name=value] [-f program-file] [program] [file ...]
Execute an AWK pattern-action program. No files, or -, reads standard input.
  -F separator     set the input field separator
  -v name=value    set a variable before BEGIN
  -f program-file  read program source (repeatable)
      --help       display this help
      --version    display version
"""

proc main(...args: List[Str]) {
  var separator = " "
  var variables: List[Str] = []
  var sources: List[Str] = []
  var operands: List[Str] = []
  var index = 0
  var options = true
  while index < args.len() {
    let argument = args[index]
    if options and argument == "--" {
      options = false
    } else if options and argument == "--help" {
      print $USAGE
      return
    } else if options and argument == "--version" {
      print "awk (XSH core)"
      return
    } else if options and (argument == "-F" or argument == "-v" or argument == "-f") {
      index += 1
      if index >= args.len() {
        gnu.usage_error(f"option requires an argument -- '{argument}'", status: 2)
      }
      let value = args[index]
      if argument == "-F" { separator = value } else if argument == "-v" { variables += [value] } else { sources += [if value == "-" { io.stdin_text()? } else { fp"{value}".read_text()? }] }
    } else if options and argument.starts_with("-F") {
      separator = argument[2..]
    } else if options and argument.starts_with("-v") {
      variables += [argument[2..]]
    } else if options and argument.starts_with("-f") {
      let name = argument[2..]
      sources += [if name == "-" { io.stdin_text()? } else { fp"{name}".read_text()? }]
    } else if options and argument.starts_with("-") and argument != "-" {
      gnu.usage_error(f"unrecognized option '{argument}'", status: 2)
    } else {
      operands += [argument]
      options = false
    }
    index += 1
  }
  var program = sources.join("\n")
  if sources.is_empty() {
    if operands.is_empty() {
      gnu.usage_error("no program supplied", status: 2)
    }
    program = operands[0]
    operands = operands[1..]
  }
  var has_input = false
  for name in operands {
    if ! rx"^[A-Za-z_][A-Za-z_0-9]*=".matches(name) { has_input = true }
  }
  if ! has_input { operands += ["-"] }
  var inputs: List[Str] = []
  for name in operands {
    inputs += [if rx"^[A-Za-z_][A-Za-z_0-9]*=".matches(name) { "" } else if name == "-" { io.stdin_text()? } else { fp"{name}".read_text()? }]
  }
  match interpreter.execute(program, inputs, operands, variables, separator) {
    Ok(result) => {
      gnu.write_text(result.stdout)
      exit result.status
    }
    Err(error) => {
      eprint f"awk: {error.message}"
      exit 2
    }
  }
}
