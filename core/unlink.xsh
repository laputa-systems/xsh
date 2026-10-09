#!/bin/xsh
use lib.gnu

const USAGE = """Usage: unlink FILE
Call the unlink function to remove the specified FILE.

      --help     display this help and exit
      --version  output version information and exit
"""

type UnlinkOptions = {help: Bool, version: Bool, operands: List[Str]}

pure raw_for(argv: List[Str], raw: List[Bytes], name: Str) -> Bytes {
  for index in range(argv.len()) {
    if argv[index] == name { return raw[index] }
  }
  bytes.from_text(name)
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let raw_args = cli.argv_bytes()
  let opts: UnlinkOptions = cli.applet(
    argv,
    {
      gnu: {status: 1, permute: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      operands: {form: "...FILE"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }

  if opts.version {
    gnu.version("unlink")
    return
  }

  if opts.operands.len() == 0 {
    gnu.missing_operand()
  }

  if opts.operands.len() > 1 {
    gnu.extra_operand(opts.operands[1])
  }

  let name = opts.operands[0]
  let raw_name = raw_for(argv, raw_args, name)
  let target = Path.parse_bytes(raw_name)?
  let quoted = gnu.quote_bytes(raw_name)

  match fs.stat(target) {
    Err(failure) => {
      gnu.error(f"cannot unlink {quoted}: {gnu.strerror(failure)}")
      exit 1
    }
    Ok(meta) if meta.kind == "dir" => {
      gnu.error(f"cannot unlink {quoted}: Is a directory")
      exit 1
    }
    Ok(_) => {}
  }

  match target.remove() {
    Ok(_) => return
    Err(failure) => {
      gnu.error(f"cannot unlink {quoted}: {gnu.strerror(failure)}")
      exit 1
    }
  }
}
