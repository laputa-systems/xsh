#!/bin/xsh
use lib.gnu
use lib.fileattrs

type Options = {recursive: Bool, all: Bool, directory: Bool, long: Bool, generation: Bool, project: Bool, verbose: Bool, help: Bool, files: List[Str]}
enum ListingKind { Operand, Attributes, Recursive, End }
type Listing = {name: Str, kind: ListingKind}

proc print_flags(name: Str, opts: Options) [fs, process, env, error, io] -> Bool {
  let target = fp"{name}"
  let metadata = fs.stat(target, follow_symlinks: false)
  if let Err(failure) = metadata { gnu.error(f"{gnu.strerror(failure)} while trying to stat {name}"); return false }
  if metadata?.kind not in ["file", "dir"] { gnu.error(f"Operation not supported While reading flags on {name}"); return false }
  let found = linux.file_attrs(target)
  if let Err(failure) = found { gnu.error(f"{gnu.strerror(failure)} While reading flags on {name}"); return false }
  var prefix = ""
  if opts.project {
    let project = linux.file_project(target)
    if let Err(failure) = project { gnu.error(f"{gnu.strerror(failure)} While reading project on {name}"); return false }
    prefix = fileattrs.field(f"{project?}", 5, left: false) + " "
  }
  if opts.generation {
    let generation = linux.file_version(target)
    if let Err(failure) = generation { gnu.error(f"{gnu.strerror(failure)} While reading version on {name}"); return false }
    prefix += fileattrs.field(f"{generation?}", 10) + " "
  }
  let text = fileattrs.flag_text(found?.flags, long: opts.long)
  if opts.long { gnu.write_text(prefix + fileattrs.field(name, 28) + " " + text + "\n") } else { gnu.write_text(prefix + text + " " + name + "\n") }
  true
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    recursive: {form: "-R", default: false},
    all: {form: "-a", default: false},
    directory: {form: "-d", default: false},
    long: {form: "-l", default: false},
    project: {form: "-p", default: false},
    generation: {form: "-v", default: false},
    verbose: {form: "-V", default: false},
    help: {form: "--help", default: false, stop: true},
    files: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: lsattr [-RVadlpv] [FILE...]\nList Linux inode flags, directory contents by default.\n  -R  recurse\n  -a  include hidden entries\n  -d  list directories themselves\n  -l  use descriptive flag names\n  -v  show generation numbers\n  -p  show project IDs\n  -V  print version information\n"); return }
  if opts.verbose { eprint (gnu.version_text("lsattr")) }
  let files = if opts.files.is_empty() { ["."] } else { opts.files }
  var pending: List[Listing] = []
  var success = true
  for name in files { pending += [{name: name, kind: .Operand}] }
  while ! pending.is_empty() {
    let listing = pending[0]
    pending = pending[1..]
    if listing.kind == .End { gnu.write_text("\n"); continue }
    if listing.kind == .Attributes {
      if ! print_flags(listing.name, opts) { success = false }
      continue
    }
    let info = fs.stat(fp"{listing.name}", follow_symlinks: false)
    if let Err(failure) = info { gnu.error(f"{gnu.strerror(failure)} while trying to stat {listing.name}"); success = false; continue }
    if info?.kind != "dir" or (opts.directory and listing.kind == .Operand) {
      if ! print_flags(listing.name, opts) { success = false }
      continue
    }
    if listing.kind == .Recursive { gnu.write_text(f"\n{listing.name}:\n") }
    let children = fs.children(fp"{listing.name}")
    if let Err(failure) = children { fileattrs.report(listing.name, failure); success = false; continue }
    var entries: List[Listing] = []
    if opts.all {
      entries += [{name: listing.name + "/.", kind: .Attributes}, {name: listing.name + "/..", kind: .Attributes}]
    }
    for child in children? {
      continue when ! opts.all and child.name.starts_with(".")
      let name = f"{listing.name}/{child.name}"
      entries += [{name: name, kind: .Attributes}]
      if opts.recursive and child.kind == "dir" {
        entries += [{name: name, kind: .Recursive}, {name: name, kind: .End}]
      }
    }
    pending = entries.extend(pending)
  }
  if ! success { exit 1 }
}
