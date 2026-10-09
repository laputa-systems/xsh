#!/bin/xsh
use lib.gnu

const USAGE = """Usage: chroot [OPTION] NEWROOT [COMMAND [ARG]...]
Run COMMAND with root directory set to NEWROOT.
If no COMMAND is given, run the value of the SHELL environment variable or /bin/sh.

      --skip-chdir  do not change working directory to '/'
      --help        display this help and exit
      --version     output version information and exit
"""

type ChrootOptions = {skip_chdir: Bool, help: Bool, version: Bool, operands: List[Str]}

proc main(...argv: List[Str]) [fs, process, env, error, io] -> Result[Int] {
  let opts: ChrootOptions = cli.applet(
    argv,
    {
      gnu: {
        status: 125,
        permute: false,
        unsupported: {
          "--userspec": "setting the command user is not available",
          "--groups": "setting supplementary groups is not available",
        },
      },
      skip_chdir: {form: "--skip-chdir", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      operands: {form: "...ARG"},
    },
  )?

  if opts.help { gnu.help(USAGE); return Ok(0) }
  if opts.version { gnu.version("chroot"); return Ok(0) }
  if opts.operands.len() == 0 { gnu.missing_operand(125) }

  let new_root = fp"{opts.operands[0]}"
  if opts.skip_chdir {
    let old_root = fs.stat(p"/", true)?
    match fs.stat(new_root, true) {
      Ok(meta) if meta.dev == old_root.dev and meta.ino == old_root.ino => {}
      _ => {
        gnu.error("option --skip-chdir only permitted if NEWROOT is old '/'")
        return Ok(125)
      }
    }
  }
  let root_stat = match fs.stat(new_root, true) {
    Ok(meta) => meta
    Err(failure) => {
      gnu.error(f"cannot chroot to {gnu.quote(new_root.display())}: {gnu.strerror(failure)}")
      return Ok(125)
    }
  }
  if root_stat.kind != "dir" {
    gnu.error(f"cannot chroot to {gnu.quote(new_root.display())}: Not a directory")
    return Ok(125)
  }

  match linux.chroot(new_root) {
    Err(failure) => {
      gnu.error(f"cannot chroot to {gnu.quote(new_root.display())}: {gnu.strerror(failure)}")
      return Ok(125)
    }
    Ok(_) => {}
  }

  let command = if opts.operands.len() > 1 {
    opts.operands[1]
  } else {
    env.get_or("SHELL", "/bin/sh")?
  }
  let command_args = if opts.operands.len() > 1 { opts.operands |> drop(1) } else { [command] }
  let command_cwd = if opts.skip_chdir { fs.cwd()? } else { p"/" }
  let plan = process.command_argv(command, command_args, cwd: command_cwd)
  match process.run(plan) {
    Ok(status) => Ok(status.exit_code()?)
    Err(failure) => {
      let code = if failure.errno == 13 { 126 } else { 127 }
      gnu.error(f"failed to run command {gnu.quote(command)}: {gnu.strerror(failure)}")
      Ok(code)
    }
  }
}

exit main(@args)?
