#!/bin/xsh
# This front end imports nothing. A name that links to it resolves `use` beside
# the link, so an import would stop every link outside the applet directory
# from loading; its messages are therefore written here rather than by lib.gnu.

# Must equal lib.gnu's VERSION, which this file cannot import.
const VERSION = "0.0.1"

const USAGE = """Usage: coreutils --coreutils-prog=PROGRAM_NAME [PARAMETERS]...
Execute the PROGRAM_NAME built-in program with the given PARAMETERS.

      --help
         display this help and exit
      --version
         output version information and exit

Built-in programs:
"""

# The applets beside this front end. A `.xsh` suffix is the source-tree spelling
# of an applet; directories, this front end, and names with other dots such as
# README.md are not applets.
proc applet_names(dir: Path) [fs, error] -> Result[List[Str]] {
  var names: List[Str] = []

  for entry in fs.children(dir)? {
    let name = if entry.name.ends_with(".xsh") { entry.name.byte_slice(0, entry.name.byte_len() - 4) } else { entry.name }

    if entry.kind != "dir" and name != "coreutils" and (name.find(".") ?? -1) < 0 {
      names += [name]
    }
  }

  names |> sort
}

# The file that runs PROGRAM, or null when the name cannot be an applet. A name
# with a slash, a NUL, or a leading dot could leave the directory, so it is
# refused. A missing path is a failed metadata read, so a missing candidate
# counts as no applet rather than as an error.
proc applet_file(dir: Path, program: Str) [fs, error] -> Result[Path?] {
  return Ok(null) when program == "" or "/" in program or "\0" in program or program.starts_with(".")

  let installed = fp"{dir}/{program}"

  if installed.is_file() ?? false {
    return Ok(installed)
  }

  let source = fp"{dir}/{program}.xsh"

  if source.is_file() ?? false {
    return Ok(source)
  }

  Ok(null)
}

proc try_help() [process] {
  eprint f"Try 'coreutils --help' for more information."
}

proc main(...argv: List[Bytes]) [process, env, error, io, fs] {
  let invoked = process.script_path() ?? p"xsh"
  let dir = invoked.parent()
  var program = invoked.basename()

  if program.ends_with(".xsh") and program.byte_len() > 4 {
    program = program.byte_slice(0, program.byte_len() - 4)
  }

  # A non-UTF-8 argument reads as a NUL marker, which no applet name can match.
  # `words` and `operands` stay in step: operands keep the raw bytes forwarded
  # to the applet.
  var words: List[Str] = []
  var operands = argv

  for arg in argv {
    words += [arg.utf8() ?? "\0"]
  }

  # Invoked under its own name (a link such as `yes`), the front end runs that
  # applet directly; only the bare `coreutils` name takes a program operand.
  if program == "coreutils" {
    if words.is_empty() or words[0] == "--" {
      try_help()
      exit 1
    }

    if words[0] == "--help" {
      let listing = applet_names(dir)?
      print f"{USAGE} {listing.join(" ")}\n\nUse: 'coreutils --coreutils-prog=PROGRAM_NAME --help' for individual program help."
      return
    }

    if words[0] == "--version" {
      print f"coreutils (XSH core) {VERSION}"
      return
    }

    if words[0].starts_with("--coreutils-prog=") {
      program = words[0].byte_slice(17)
    } else if words[0].starts_with("--") {
      eprint f"coreutils: unrecognized option '{words[0]}'"
      try_help()
      exit 1
    } else if words[0].starts_with("-") and words[0] != "-" {
      eprint f"coreutils: invalid option -- '{words[0].byte_slice(1, length: 1)}'"
      try_help()
      exit 1
    } else {
      program = words[0]
    }

    words = words[1..]
    operands = operands[1..]
  }

  guard let file = applet_file(dir, program)? else {
    let shown = if "\0" in program { "<non-UTF-8 name>" } else { program }
    eprint f"coreutils: unknown program '{shown}'"
    exit 1
  }

  var forwarded: List[Path] = [file]

  for operand in operands {
    forwarded += [Path.parse_bytes(operand)?]
  }

  let plan = process.command_argv(file, forwarded)

  if let Err(failure) = unix.exec(plan) {
    eprint f"coreutils: cannot run '{file.display()}': {failure.message}"
    exit 126
  }
}
