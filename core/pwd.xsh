#!/bin/xsh
use lib.gnu

const USAGE = """Usage: pwd [OPTION]...
Print the full filename of the current working directory.

  -L, --logical  use PWD from the environment, even if it contains symlinks
  -P, --physical resolve all symlinks in the current directory
      --help     display this help and exit
      --version  output version information and exit
"""

type PwdOptions = {logical: Bool, physical: Bool, help: Bool, version: Bool, operands: List[Str]}

proc logical_pwd(cwd: Path) [fs, env] -> Path {
  let text = env.get_or("PWD", "") ?? ""
  return cwd when text == "" or ! text.starts_with("/")

  let candidate = fp"{text}"
  if let Ok(expected) = fs.stat(cwd) {
    if let Ok(found) = fs.stat(candidate) {
      return candidate when expected.dev == found.dev and expected.ino == found.ino
    }
  }

  cwd
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: PwdOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      logical: {form: "-L --logical", default: false},
      physical: {form: "-P --physical", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      operands: {form: "...ARG"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }
  if opts.version {
    gnu.version("pwd")
    return
  }

  if opts.operands.len() > 0 { gnu.error("ignoring non-option arguments") }
  let cwd_result = fs.cwd()
  if let Err(failure) = cwd_result {
    gnu.error(f"failed to get current directory: {gnu.strerror(failure)}")
    exit 1
  }
  let cwd = if let Ok(directory) = cwd_result { directory } else { p"/" }
  let posix = (env.get_or("POSIXLY_CORRECT", "") ?? "") != ""
  let use_logical = opts.logical or (! opts.physical and posix)
  let output = if use_logical { logical_pwd(cwd) } else { cwd.resolve()? }
  gnu.write_text(f"{output.display()}\n")
}
