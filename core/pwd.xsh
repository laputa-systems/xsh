#!/bin/xsh
use lib.gnu

type Options = {logical: Bool, physical: Bool, help: Bool, version: Bool, operands: List[Str]}

# A deleted cwd has no matching entry in its parent; other lookup failures
# retain their OS reason.
proc cwd_error(failure: Error) [process, env] {
  if gnu.errno(failure) == 2 {
    gnu.error("couldn't find directory entry in '..' with matching i-node")
  } else {
    gnu.error(f"failed to get current directory: {gnu.strerror(failure)}")
  }
  exit 1
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    logical: {form: "-L --logical", default: false},
    physical: {form: "-P --physical", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    operands: {form: "...ARG"},
  })?
  if opts.help { gnu.help("Usage: pwd [OPTION]...\nPrint the current working directory.\n  -L, --logical   use PWD if valid\n  -P, --physical  resolve symbolic links\n"); return }
  if opts.version { gnu.version("pwd"); return }
  if ! opts.operands.is_empty() { gnu.error("ignoring non-option arguments") }
  var logical = opts.logical or (env.get("POSIXLY_CORRECT") is Ok(_) and ! opts.physical)
  if opts.physical { logical = false }
  for arg in argv {
    break when arg == "--"
    if arg == "--logical" { logical = true } else if arg == "--physical" { logical = false } else if arg.starts_with("-") and ! arg.starts_with("--") {
      for flag in arg { if flag == "L" { logical = true } else if flag == "P" { logical = false } }
    }
  }
  let physical = fs.cwd()
  if let Err(failure) = physical { cwd_error(failure) }
  let physical_path = physical?
  var value = physical_path.display()
  if value == "." {
    let resolved = physical_path.resolve()
    if let Err(failure) = resolved { cwd_error(failure) }
    value = resolved?.display()
  }
  if logical {
    let pwd = env.get_or("PWD", "") ?? ""
    let parts = pwd.split("/")
    if pwd.starts_with("/") and "." not in parts and ".." not in parts {
      let given = fs.stat(fp"{pwd}", follow_symlinks: true)
      let current = fs.stat(p".", follow_symlinks: true)?
      if let Ok(info) = given { if info.dev == current.dev and info.ino == current.ino { value = pwd } }
    }
  }
  gnu.write_text(value + "\n")
}
