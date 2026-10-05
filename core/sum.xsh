#!/bin/xsh
use lib.gnu

type Options = {bsd: Bool, sysv: Bool, help: Bool, version: Bool, files: List[Str]}

pure padded(value: Int, width: Int, fill: Str) -> Str {
  var result = f"{value}"
  while result.byte_len() < width { result = fill + result }
  result
}

proc main(...argv: List[Str]) [fs, io, error, process, env] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    bsd: {form: "-r", default: false},
    sysv: {form: "-s --sysv", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    files: {form: "...FILE"},
  })?
  if opts.help {
    gnu.help("Usage: sum [OPTION]... [FILE]...\nPrint checksum and block counts for each FILE.\n  -r       BSD checksum, 1024-byte blocks (default)\n  -s, --sysv  System V checksum, 512-byte blocks\nWith no FILE or when FILE is -, read standard input.")
    return
  }
  if opts.version { gnu.version("sum"); return }
  var sysv = false
  for arg in argv {
    if arg == "--" { break }
    if arg == "--sysv" { sysv = true }
    if arg.starts_with("-") and !arg.starts_with("--") {
      for index in range(1, arg.byte_len()) {
        let flag = arg.byte_slice(index, length: 1)
        if flag == "s" { sysv = true }
        if flag == "r" { sysv = false }
      }
    }
  }
  let algorithm = if sysv { "sysv" } else { "bsd" }
  let files = if opts.files.is_empty() { ["-"] } else { opts.files }
  var failed = false
  for name in files {
    let result = if name == "-" { hash.checksum_stdin(algorithm) } else { hash.checksum(fp"{name}", algorithm) }
    match result {
      Ok(value) => {
        let block_size = if sysv { 512 } else { 1024 }
        let blocks = (value.size + block_size - 1) / block_size
        let suffix = if name == "-" { "" } else { f" {name}" }
        let checksum = if sysv { f"{value.checksum}" } else { padded(value.checksum, 5, "0") }
        let count = if sysv { f"{blocks}" } else { padded(blocks, 5, " ") }
        gnu.write_text(f"{checksum} {count}{suffix}\n")
      }
      Err(failure) => { gnu.name_error(name, failure); failed = true }
    }
  }
  if failed { exit 1 }
}
