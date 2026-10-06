#!/bin/xsh
use lib.gnu
use lib.fileattrs
use lib.capability

type Options = {recursive: Bool, verbose: Bool, rootid: Bool, help: Bool, files: List[Str]}
proc main(...argv: List[Str]) [fs, process, env, io, error] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    recursive: {form: "-r", default: false}, verbose: {form: "-v", default: false}, rootid: {form: "-n", default: false},
    help: {form: "-h --help", default: false, stop: true}, files: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: getcap [-r] [-v] [-n] FILE...\nDisplay file capabilities; -r descends directories, -v includes files without capabilities, -n displays namespace root IDs.\n"); return }
  if opts.files.is_empty() { gnu.missing_operand() }
  var pending = opts.files
  var success = true
  while ! pending.is_empty() {
    let name = pending[0]; pending = pending[1..]
    let target = fp"{name}"
    let metadata = fs.stat(target, follow_symlinks: false)
    if let Err(failure) = metadata { fileattrs.report(name, failure); success = false; continue }
    if metadata?.kind == "dir" {
      if opts.recursive {
        let children = fs.children(target)
        if let Err(failure) = children { fileattrs.report(name, failure); success = false; continue }
        pending = collect { for child in children? { yield f"{name}/{child.name}" } }.extend(pending)
      }
      if opts.verbose { gnu.write_text(name + " (Not a regular file)\n") }
      continue
    }
    if metadata?.kind != "file" { if opts.verbose { gnu.write_text(name + " (Not a regular file)\n") }; continue }
    let payload = fs.xattr_get(target, "security.capability", follow_symlinks: false)
    if let Err(failure) = payload {
      if failure.errno == 61 or failure.errno == 93 { if opts.verbose { gnu.write_text(name + "\n") }; continue }
      fileattrs.report(name, failure); success = false; continue
    }
    let record = capability.decode(payload?)
    if let Err(failure) = record { fileattrs.report(name, failure); success = false; continue }
    let value = record?
    var text = name + " " + capability.format(value)
    if opts.rootid and value.rootid != null { text += f" [rootid={value.rootid ?? 0}]" }
    gnu.write_text(text + "\n")
  }
  if ! success { exit 1 }
}
