#!/bin/xsh
use lib.gnu

type Options = {help: Bool, version: Bool, files: List[Str]}

# Dependency paths are inspected as files; target code and interpreters are
# never run, so this report has paths rather than runtime load addresses.
proc resolve_library(name: Str, origin: Path, search: Str) [fs, env, error, process] -> Result[Path?] {
  if name.find("/") != null {
    let target = fp"{name}"
    return Ok(if target.exists()? { target } else { null })
  }
  let environment = env.get_or("LD_LIBRARY_PATH", "") ?? ""
  let paths = (environment + ":" + search).split(":").extend(["/lib", "/usr/local/lib", "/usr/lib", "/lib64", "/usr/lib64", "/lib/aarch64-linux-gnu", "/usr/lib/aarch64-linux-gnu", "/lib/x86_64-linux-gnu", "/usr/lib/x86_64-linux-gnu"])
  for directory in paths {
    if directory == "" { continue }
    let expanded = directory.replace(r"${ORIGIN}", with: origin.display()).replace(r"$ORIGIN", with: origin.display())
    if expanded.find("$") != null { gnu.error(f"unsupported loader path expansion {gnu.quote(expanded)}"); return Ok(null) }
    let candidate = fp"{expanded}/{name}"
    if candidate.exists()? { return Ok(candidate) }
  }
  Ok(null)
}

proc main(...argv: List[Str]) [fs, error, process, env, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    verbose: {form: "-v --verbose", unsupported: true},
    relocations: {form: "-r --function-relocs", unsupported: true},
    data_relocations: {form: "-d --data-relocs", unsupported: true},
    detect_unused: {form: "-u --unused", unsupported: true},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    files: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: ldd [OPTION]... FILE...\nInspect ELF interpreter and dependency paths without executing targets.\nLoad addresses and relocation checks are unavailable in this static report."); return }
  if opts.version { gnu.version("ldd"); return }
  if opts.files.is_empty() { gnu.missing_operand() }
  var failed = false
  for name in opts.files {
    if opts.files.len() > 1 { gnu.write_text(f"{name}:\n") }
    let inspected = elf.inspect(fp"{name}")
    if let Err(failure) = inspected { gnu.name_error(name, failure); failed = true; continue }
    let initial = inspected?
    if initial.type == "not-elf" or (initial.type != "shared" and initial.type != "executable") {
      gnu.error(f"{name}: not a dynamic executable")
      failed = true
      continue
    }
    if initial.needed.is_empty() and initial.interpreter == "" { gnu.write_text("\tstatically linked\n"); continue }
    if initial.interpreter != "" { gnu.write_text(f"\t{initial.interpreter}\n") }
    var pending = [fp"{name}"]
    var visited: List[Str] = []
    var index = 0
    while index < pending.len() {
      let object = pending[index]
      index += 1
      let info = elf.inspect(object)?
      let search = if info.runpath != "" { info.runpath } else { info.rpath }
      for dependency in info.needed {
        if dependency in visited { continue }
        visited += [dependency]
        let resolved = resolve_library(dependency, object.resolve()?.parent(), search)?
        if let resolved_path = resolved {
          gnu.write_text(f"\t{dependency} => {resolved_path}\n")
          let checked = elf.inspect(resolved_path)
          if let Ok(child) = checked {
            if child.type != "not-elf" { pending += [resolved_path] } else { gnu.error(f"{resolved_path}: not an ELF library"); failed = true }
          } else if let Err(failure) = checked { gnu.name_error(resolved_path.display(), failure); failed = true }
        } else {
          gnu.write_text(f"\t{dependency} => not found\n")
          failed = true
        }
      }
    }
  }
  if failed { exit 1 }
}
