#!/bin/xsh
use lib.gnu as gnu

const USAGE = """Usage:
 mountpoint [-qd] /path/to/directory
 mountpoint -x /dev/device

Check whether a directory or file is a mountpoint.

Options:
 -q, --quiet quiet mode - don't print anything
 --nofollow do not follow symlink
 -d, --fs-devno print maj:min device number of the filesystem
 -x, --devno print maj:min device number of the block device
 --show print mountpoint for given path

 -h, --help display this help
 -V, --version display version

For more details see mountpoint(1).
"""

type Options = {
  quiet: Bool,
  nofollow: Bool,
  fs_devno: Bool,
  devno: Bool,
  show: Bool,
  help: Bool,
  version: Bool,
  operands: List[Str],
}

proc write_error_bytes(data: Bytes) [process, env, io] {
  if let Err(failure) = io.write_stderr_bytes(data) {
    gnu.error(f"write error: {gnu.strerror(failure)}")
    exit 1
  }
  if let Err(failure) = io.flush_stderr() {
    gnu.error(f"write error: {gnu.strerror(failure)}")
    exit 1
  }
}

proc bad_usage() [process, env] {
  gnu.error("bad usage")
  gnu.try_help()
  exit 1
}

proc report_path_error(name: Bytes, failure: Error) [process, env, io] {
  write_error_bytes(bytes.concat([
    bytes.from_text(f"{gnu.prog()}: "), name,
    bytes.from_text(f": {gnu.strerror(failure)}\n"),
  ]))
}

proc report_non_block(name: Bytes) [process, env, io] {
  write_error_bytes(bytes.concat([
    bytes.from_text(f"{gnu.prog()}: "), name,
    b": not a block device\n",
  ]))
}

pure device_number(device: Int) -> Str {
  f"{fs.dev_major(device)}:{fs.dev_minor(device)}"
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  let prepared = gnu.prepare_arguments(argv)
  let opts: Options = cli.applet(prepared.text, {
    gnu: {status: 1},
    quiet: {form: "-q --quiet", default: false},
    nofollow: {form: "--nofollow", default: false},
    fs_devno: {form: "-d --fs-devno", default: false},
    devno: {form: "-x --devno", default: false},
    show: {form: "--show", default: false},
    help: {form: "-h --help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    operands: {form: "...FILE"},
  })?

  if opts.help { gnu.help(USAGE); return }
  if opts.version { gnu.version("mountpoint"); return }
  if opts.operands.len() != 1 { bad_usage() }
  if opts.nofollow and opts.devno {
    gnu.error("--devno and --nofollow are mutually exclusive")
    exit 1
  }

  let name = gnu.argument_bytes(opts.operands[0], prepared.raw)
  let path = Path.parse_bytes(name)?
  let metadata = match fs.stat(path, follow_symlinks: ! opts.nofollow) {
    Ok(value) => value
    Err(failure) => {
      if ! opts.quiet { report_path_error(name, failure) }
      exit 1
    }
  }

  if opts.devno {
    if metadata.kind != "block" {
      if ! opts.quiet { report_non_block(name) }
      exit 32
    }
    gnu.write_text(f"{device_number(metadata.rdev)}\n")
    return
  }

  if opts.nofollow and metadata.kind == "symlink" {
    if ! opts.quiet {
      gnu.write_bytes(bytes.concat([name, b" is not a mountpoint\n"]))
    }
    exit 32
  }

  let resolved = match path.resolve() {
    Ok(value) => value
    Err(failure) => {
      if ! opts.quiet { report_path_error(name, failure) }
      exit 1
    }
  }
  let mount = match fs.mount_for(resolved) {
    Ok(value) => value
    Err(failure) => {
      if ! opts.quiet { report_path_error(name, failure) }
      exit 1
    }
  }

  if opts.show {
    gnu.write_text(f"{mount.mounted_on.display()}\n")
    return
  }
  if resolved != mount.mounted_on {
    if ! opts.quiet { gnu.write_bytes(bytes.concat([name, b" is not a mountpoint\n"])) }
    exit 32
  }
  if opts.fs_devno {
    gnu.write_text(f"{device_number(mount.device)}\n")
  } else if ! opts.quiet {
    gnu.write_bytes(bytes.concat([name, b" is a mountpoint\n"]))
  }
}
