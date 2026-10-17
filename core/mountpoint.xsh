#!/bin/xsh
use lib.gnu as gnu
use lib.util_linux_io as ul_io

const USAGE = """Usage:
 mountpoint [-qd] /path/to/directory
 mountpoint -x /dev/device

Check whether a directory or file is a mountpoint.

Options:
 -q, --quiet        quiet mode - don't print anything
     --nofollow     do not follow symlink
 -d, --fs-devno     print maj:min device number of the filesystem
 -x, --devno        print maj:min device number of the block device
     --show         print mountpoint for given path

 -h, --help         display this help
 -V, --version      display version

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

proc parse_options(argv: List[Str]) [process, env] -> Options {
  var opts: Options = {quiet: false, nofollow: false, fs_devno: false, devno: false, show: false, help: false, version: false, operands: []}
  var at = 0
  while at < argv.len() {
    let word = argv[at]
    at += 1
    if word == "--" {
      return {...opts, operands: opts.operands.extend(argv[at..])}
    }
    if ! word.starts_with("-") or word == "-" {
      opts = {...opts, operands: opts.operands.extend([word])}
      continue
    }
    var flags: List[Str] = []
    if word.starts_with("--") {
      let body = word.byte_slice(2)
      let typed = body.split("=")[0]
      let matches = [name for name in ["quiet", "nofollow", "fs-devno", "devno", "show", "help", "version"] if name.starts_with(typed)]
      if matches.is_empty() { gnu.usage_error(f"unrecognized option: {body}") }
      if matches.len() > 1 { gnu.usage_error(f"option is ambiguous: {body}") }
      if "=" in body { gnu.usage_error(f"option does not take an argument: {matches[0]}") }
      flags = matches
    } else {
      flags = [flag for flag in word.byte_slice(1)]
    }
    for flag in flags {
      match flag {
        "q" | "quiet" => opts = {...opts, quiet: true}
        "nofollow" => opts = {...opts, nofollow: true}
        "d" | "fs-devno" => opts = {...opts, fs_devno: true}
        "x" | "devno" => opts = {...opts, devno: true}
        "show" => opts = {...opts, show: true}
        "h" | "help" => return {...opts, help: true}
        "V" | "version" => return {...opts, version: true}
        else => gnu.usage_error(f"unrecognized option: {flag}")
      }
    }
  }
  opts
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  if process.signal_action("PIPE")? == "default" {
    process.set_signal_action("PIPE", "default")
  }

  let prepared = gnu.prepare_arguments(argv)
  let opts = parse_options(prepared.text)

  if opts.help { ul_io.help(f"\n{USAGE}"); return }
  if opts.version { ul_io.version("mountpoint"); return }
  if opts.operands.len() != 1 { bad_usage() }
  if opts.nofollow and opts.devno {
    gnu.error("--devno and --nofollow are mutually exclusive")
    exit 1
  }

  let name = gnu.argument_bytes(opts.operands[0], prepared.raw)
  let path_value = Path.parse_bytes(name)?
  let metadata = match fs.stat(path_value, follow_symlinks: ! opts.nofollow) {
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
    ul_io.write_text(f"{device_number(metadata.rdev)}\n")
    return
  }

  if opts.nofollow and metadata.kind == "symlink" {
    if ! opts.quiet {
      ul_io.write_bytes(bytes.concat([name, b" is not a mountpoint\n"]))
    }
    exit 32
  }

  let resolved = match path_value.resolve() {
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
    ul_io.write_bytes(bytes.concat([mount.mounted_on.bytes(), b"\n"]))
    return
  }
  if resolved != mount.mounted_on {
    if ! opts.quiet { ul_io.write_bytes(bytes.concat([name, b" is not a mountpoint\n"])) }
    exit 32
  }
  if opts.fs_devno {
    ul_io.write_text(f"{device_number(mount.device)}\n")
  } else if ! opts.quiet {
    ul_io.write_bytes(bytes.concat([name, b" is a mountpoint\n"]))
  }
}
