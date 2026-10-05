#!/bin/xsh
use lib.gnu
use lib.perm

type Options = {userspec: Str?, groups: Str?, skip_chdir: Bool, help: Bool, version: Bool, operands: List[Str]}

proc main(...argv: List[Str]) [fs, error, process, env, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 125, permute: false},
    userspec: {form: "--userspec USER:GROUP"},
    groups: {form: "--groups G_LIST"},
    skip_chdir: {form: "--skip-chdir", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    operands: {form: "...ARG"},
  })?
  if opts.help {
    gnu.help("Usage: chroot [OPTION] NEWROOT [COMMAND [ARG]...]\nRun COMMAND with root directory set to NEWROOT.\n      --userspec=USER:GROUP\n      --groups=G_LIST\n      --skip-chdir\n      --help\n      --version")
    return
  }
  if opts.version { gnu.version("chroot"); return }
  if opts.operands.is_empty() { gnu.missing_operand(125) }
  let new_root = fp"{opts.operands[0]}"
  if opts.skip_chdir {
    match fs.stat(new_root, follow_symlinks: true) {
      Ok(meta) => {
        let root = fs.stat(p"/", follow_symlinks: true)?
        if root.dev != meta.dev or root.ino != meta.ino {
          gnu.error("option --skip-chdir only permitted if NEWROOT is old '/' directory")
          exit 125
        }
      }
      Err(failure) => { gnu.cannot("change root directory to", f"{new_root}", failure); exit 125 }
    }
  }
  if opts.userspec != null or opts.groups != null {
    gnu.error("credential transitions are not supported by the native process API")
    exit 125
  }
  if let Err(failure) = linux.chroot(new_root) {
    gnu.cannot("change root directory to", f"{new_root}", failure)
    exit 125
  }
  let words = if opts.operands.len() > 1 { opts.operands[1..] } else { [env.get_or("SHELL", "/bin/sh") ?? "/bin/sh", "-i"] }
  if opts.skip_chdir {
    launch(words)
  } else {
    let working_dir = p"/"
    cd $working_dir { launch(words) }
  }
}

# Relative commands and PATH entries resolve in the new working directory.
proc launch(words: List[Str]) [fs, error, process, env, io] {
  let command = words[0]
  if command.find("/") != null {
    if ! (fp"{command}".exists() ?? false) {
      gnu.error(f"failed to run command {gnu.quote(command)}: No such file or directory")
      exit 127
    }
  } else if let Err(failure) = process.which(command) {
    let missing = failure is NotFound
    gnu.error(f"failed to run command {gnu.quote(command)}: {if missing { "No such file or directory" } else { "Permission denied" }}")
    exit if missing { 127 } else { 126 }
  }
  let plan = process.command_argv(command, words)
  if let Err(failure) = unix.exec(plan) {
    let reason = if failure is PermissionDenied or failure.message == "executable exists but is not executable" { "Permission denied" } else { gnu.strerror(failure) }
    gnu.error(f"failed to run command {gnu.quote(command)}: {reason}")
    exit if (failure.errno ?? gnu.errno(failure)) == 2 { 127 } else { 126 }
  }
}
