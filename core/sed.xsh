#!/bin/xsh
use lib.gnu
use lib.sed as editing

proc main(...argv: List[Str]) [fs, error, io, process, env] {
  var quiet = false
  var extended = false
  var inplace = false
  var suffix = ""
  var scripts: List[Str] = []
  var files: List[Str] = []
  var options = true
  var at = 0
  while at < argv.len() {
    let arg = argv[at]
    at += 1
    if options and arg == "--" { options = false; continue }
    if options and arg == "--help" {
      gnu.help("Usage: sed [OPTION]... {script-only-if-no-other-script} [FILE]...\n  -n, --quiet          suppress automatic printing\n  -e SCRIPT            add editing commands\n  -f FILE              read editing commands\n  -E, -r               extended regular expressions\n  -i[SUFFIX]           edit files in place, optionally keeping a backup")
      return
    }
    if options and (arg == "--expression" or arg == "--file" or arg.starts_with("--expression=") or arg.starts_with("--file=")) {
      let file_option = arg == "--file" or arg.starts_with("--file=")
      var value = ""
      if "=" in arg {
        let offset = if file_option { 7 } else { 13 }
        value = arg.byte_slice(offset)
      } else {
        if at >= argv.len() { gnu.error(f"option requires an argument -- '{arg}'"); exit 1 }
        value = argv[at]
        at += 1
      }
      if file_option { scripts += [editing.read_script(value)] } else { scripts += [value] }
      continue
    }
    if options and arg == "--version" { gnu.version("sed"); return }
    if options and (arg == "--quiet" or arg == "--silent") { quiet = true; continue }
    if options and arg == "--regexp-extended" { extended = true; continue }
    if options and (arg == "--in-place" or arg.starts_with("--in-place=")) {
      inplace = true
      suffix = if arg == "--in-place" { "" } else { arg.byte_slice(11) }
      continue
    }
    if options and arg.starts_with("-") and arg != "-" {
      var flag = 1
      while flag < arg.byte_len() {
        let ch = arg.byte_slice(flag, 1)
        flag += 1
        if ch == "n" { quiet = true } else if ch == "E" or ch == "r" { extended = true } else if ch == "i" { inplace = true; suffix = arg.byte_slice(flag); break } else if ch == "e" or ch == "f" {
          var value = arg.byte_slice(flag)
          if value == "" {
            if at >= argv.len() { gnu.error(f"option requires an argument -- '{ch}'"); exit 1 }
            value = argv[at]
            at += 1
          }
          if ch == "e" { scripts += [value] } else { scripts += [editing.read_script(value)] }
          break
        } else { gnu.error(f"invalid option -- '{ch}'"); exit 1 }
      }
      continue
    }
    if scripts.is_empty() { scripts += [arg] } else { files += [arg] }
  }
  if scripts.is_empty() { gnu.error("missing editing script"); exit 1 }
  let script = scripts.join("\n")
  # Validate every command before reading input or replacing a file.
  let validated = editing.edit(script, [], quiet, extended)
  var failed = false
  if inplace {
    if files.is_empty() { gnu.error("no input files"); exit 1 }
    for name in files {
      if name == "-" { gnu.error("cannot edit standard input in place"); exit 1 }
      let source = fp"{name}"
      let input = match gnu.read_operand(name) {
        Ok(data) => data
        Err(failure) => { gnu.cannot_open(name, failure); failed = true; continue }
      }
      let output = editing.edit(script, [input], quiet, extended)
      if suffix != "" {
        let backup = if "*" in suffix { fp"{suffix.replace("*", with: name)}" } else { fp"{name}{suffix}" }
        if let Err(failure) = source.copy(to: backup, overwrite: true) { gnu.cannot_open(backup.display(), failure, mode: "writing"); exit 2 }
      }
      if let Err(failure) = source.write_atomic(output) { gnu.cannot_open(name, failure, mode: "writing"); exit 2 }
    }
  } else {
    var inputs: List[Bytes] = []
    if files.is_empty() { inputs = [editing.read_input("-")] }
    for name in files {
      let data = match gnu.read_operand(name) {
        Ok(value) => value
        Err(failure) => { gnu.cannot_open(name, failure); failed = true; continue }
      }
      inputs += [data]
    }
    gnu.write_bytes(editing.edit(script, inputs, quiet, extended))
  }
  if failed { exit 2 }
}
