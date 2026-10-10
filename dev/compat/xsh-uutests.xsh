#!/bin/xsh
# uutils multicall contract for XSH: `xsh-uutests <utility> <args...>`.
#
# The uutils integration suite runs `$UUTESTS_BINARY_PATH <utility> <args...>`
# for every command under test, some of them under a tiny RLIMIT_NOFILE. A
# /bin/sh adapter cannot even open its own script at that limit, so the adapter
# is an XSH script: the staged copy is run by the interpreter named on its
# shebang line and needs no shell.
#
# stage.py copies this file to STAGE/xsh-uutests with the interpreter on the
# shebang line and STAGE replaced by the absolute stage path. The uutils test
# framework clears the environment (only LC_ALL, TZ, PATH and LD_PRELOAD
# survive), so the stage cannot come from a variable. This source file is a
# template and is not runnable until stage.py has filled the stage in.
#
# It is a dispatcher, not a second implementation: it execs the staged applet
# for <utility> with argv, stdio, cwd, environment and signals untouched, and
# never rewrites arguments or output. A utility with no staged applet exits 127
# with a diagnostic, so a missing command is a visible failure rather than a
# fall-through to a host binary.

const STAGE = "@STAGE@"

proc fail(status: Int, message: Str) [io, error] {
  io.write_stderr(f"xsh-uutests: {message}\n")
  exit status
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  if argv.is_empty() {
    io.write_stderr("usage: xsh-uutests UTILITY [ARG...]\n")
    exit 2
  }

  let name = argv[0]
  let util = match name.utf8() {
    Ok(text) => text
    Err(_) => ""
  }

  if util == "" or util == "." or util == ".." or util.find("/") != null {
    fail(2, f"invalid utility name '{util}'")
  }

  let program = fp"{STAGE}/bin/{util}"
  let usable = match fs.stat(program, follow_symlinks: true) {
    Ok(info) => info.kind != "dir" and (fs.access(program, execute: true) ?? false)
    Err(_) => false
  }

  if ! usable {
    fail(127, f"XSH provides no applet for '{util}'")
  }

  # A runaway applet must fail its own test rather than exhaust the host: the
  # address-space cap is read from the stage (written by stage.py from
  # XSH_COMPAT_MEM_KB, default 3 GiB; "unlimited" disables it). A cap the host
  # refuses is ignored, as the limit is a safety net and not part of the
  # contract under test.
  let limit = match fp"{STAGE}/mem-limit-kb".read_text() {
    Ok(text) => text.trim()
    Err(_) => "3145728"
  }

  if limit != "unlimited" {
    if let Ok(kib) = limit.parse_int() {
      match process.set_rlimit("as", soft: kib * 1024, hard: kib * 1024) {
        Ok(_) => {}
        Err(_) => {}
      }
    }
  }

  var words: List[Path] = [program]

  for argument in argv[1..] {
    words += [Path.parse_bytes(argument)?]
  }

  # The uutils usage_error helper expects the multicall hint `Try '<binary>
  # <util> --help'`; applets print `Try 'PHRASE --help'` with PHRASE taken from
  # this variable.
  let plan = process.command_argv(program, words, env: {XSH_EXECUTION_PHRASE: f"{STAGE}/xsh-uutests {util}"})

  if let Err(failure) = unix.exec(plan) {
    fail(126, f"cannot run '{util}': {failure.message}")
  }
}
