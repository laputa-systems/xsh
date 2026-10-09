#!/bin/xsh
use lib.gnu

const USAGE = """Usage: sync [OPTION]... [FILE]...
Synchronize cached writes to persistent storage.

  -d, --data         sync only file data, not metadata
  -f, --file-system  sync the filesystems that contain FILEs
      --help         display this help and exit
      --version      output version information and exit
"""

type SyncOptions = {data: Bool, filesystem: Bool, help: Bool, version: Bool, files: List[Str]}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: SyncOptions = cli.applet(
    argv,
    {
      gnu: {status: 1},
      data: {form: "-d --data", default: false},
      filesystem: {form: "-f --file-system", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?
  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("sync"); return }

  if opts.data and opts.files.len() == 0 {
    gnu.error("--data needs at least one argument")
    exit 1
  }
  if opts.files.len() == 0 {
    fs.sync()?
    return
  }

  var failed = false
  for name in opts.files {
    let target = fp"{name}"
    let metadata = fs.stat(target)
    if let Err(failure) = metadata {
      gnu.error(f"error opening {gnu.quote(name)}: {gnu.strerror(failure)}")
      failed = true
      continue
    }
    let meta = if let Ok(value) = metadata { value } else { fs.stat(target)? }
    if opts.data and meta.kind == "fifo" {
      gnu.error(f"error syncing {gnu.quote(name)}: Invalid input")
      failed = true
      continue
    }
    let result = if opts.filesystem {
      # fs.sync flushes all mounted filesystems, which is stronger than
      # syncing only the filesystem containing this operand.
      fs.sync()
    } else {
      fs.fsync(target)
    }
    if let Err(failure) = result {
      gnu.error(f"error opening {gnu.quote(name)}: {gnu.strerror(failure)}")
      failed = true
    }
  }
  if failed { exit 1 }
}
