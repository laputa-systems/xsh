##! Kernel module command argument handling and conventional text output.

use gnu

const VERSION = "kmod (XSH core) 0.0.1"

proc common(arg: Str, usage: Str) -> Bool {
  if arg == "-h" or arg == "--help" { gnu.help(usage); return true }
  if arg == "-V" or arg == "--version" { gnu.write_text(f"{VERSION}\n"); return true }
  false
}

proc unsupported(arg: Str) {
  gnu.error(f"option {gnu.quote(arg)} is not supported")
  exit 1
}

pure module_name(text: Str) -> Str {
  var name = fp"{text}".basename()
  for suffix in [".ko.zst", ".ko.xz", ".ko.gz", ".ko"] {
    if name.ends_with(suffix) { name = name.byte_slice(0, name.byte_len() - suffix.byte_len()); break }
  }
  name.replace("-", with: "_")
}

# The kernel parameter parser understands double quotes around values. Keep
# whitespace inside one argv item attached to its parameter, without a shell.
pure parameters(values: List[Str]) -> Str {
  var output: List[Str] = []
  for value in values {
    let pieces = value.split("=", maxsplit: 1)
    if pieces.len() == 2 {
      let escaped = pieces[1].replace("\\", with: "\\\\").replace("\"", with: "\\\"")
      if escaped.find(" ") != null or escaped.find("\t") != null or escaped.find("\"") != null { output += [f"{pieces[0]}=\"{escaped}\""] } else { output += [value] }
    } else { output += [value] }
  }
  output.join(" ")
}

# Split only complete groups of recognized boolean short options. Values of
# options such as -Ffield and parameter argv items keep their original bytes.
pure short_options(argv: List[Str], boolean_flags: Str) -> List[Str] {
  var output: List[Str] = []
  for arg in argv {
    if arg.starts_with("-") and ! arg.starts_with("--") and arg.byte_len() > 2 {
      var pieces: List[Str] = []
      var recognized = true
      for char in arg.byte_slice(1) {
        if boolean_flags.find(char) == null { recognized = false }
        pieces += [f"-{char}"]
      }
      output += if recognized { pieces } else { [arg] }
    } else { output += [arg] }
  }
  output
}

proc failure(message: Str, problem: Error) {
  gnu.error(f"{message}: {gnu.strerror(problem)}")
}

## Print loaded module names, sizes, reference counts and dependent names.
export proc lsmod(argv: List[Str]) {
  for arg in argv {
    if common(arg, "Usage: lsmod [--help] [--version]\nList currently loaded kernel modules.") { return }
    gnu.usage_error(f"unexpected argument {gnu.quote(arg)}")
  }
  match linux.modules() {
    Ok(modules) => {
      gnu.write_text("Module                  Size  Used by\n")
      for item in modules {
        let dependents = if item.used_by.is_empty() { "" } else { f" {item.used_by.join(",")}" }
        gnu.write_text(f"{item.name:<19} {item.size:>8} {item.ref_count:>2}{dependents}\n")
      }
    }
    Err(problem) => { failure("could not get list of modules", problem); exit 1 }
  }
}

## Print selected or complete ordered kernel module metadata fields.
export proc modinfo(raw: List[Str]) {
  let argv = short_options(raw, "adlpn0")
  var selected = ""
  var delimiter = "\n"
  var names: List[Str] = []
  var index = 0
  var operands = false
  while index < argv.len() {
    let arg = argv[index]; index += 1
    if ! operands and arg == "--" { operands = true; continue }
    if ! operands and common(arg, "Usage: modinfo [OPTIONS] MODULE...\n  -a, --author        show author\n  -d, --description   show description\n  -l, --license       show license\n  -p, --parameters    show parameters\n  -n, --filename      show filename\n  -F, --field FIELD   show one field\n  -0, --null          terminate fields with NUL") { return }
    if ! operands and (arg == "-0" or arg == "--null") { delimiter = "\0"; continue }
    if ! operands and (arg == "-a" or arg == "--author") { selected = "author"; continue }
    if ! operands and (arg == "-d" or arg == "--description") { selected = "description"; continue }
    if ! operands and (arg == "-l" or arg == "--license") { selected = "license"; continue }
    if ! operands and (arg == "-p" or arg == "--parameters") { selected = "parm"; continue }
    if ! operands and (arg == "-n" or arg == "--filename") { selected = "filename"; continue }
    if ! operands and (arg == "-F" or arg == "--field" or arg.starts_with("--field=") or (arg.starts_with("-F") and arg.byte_len() > 2)) {
      if arg.starts_with("--field=") { selected = arg.byte_slice(8) } else if arg.starts_with("-F") and arg.byte_len() > 2 { selected = arg.byte_slice(2) } else { if index >= argv.len() { gnu.usage_error(f"option {gnu.quote(arg)} requires an argument") }; selected = argv[index]; index += 1 }
      continue
    }
    if ! operands and arg.starts_with("-") { unsupported(arg) }
    names += [arg]
  }
  if names.is_empty() { gnu.missing_operand() }
  var ok = true
  for name in names {
    match linux.modinfo(name) {
      Ok(info) => {
        if selected == "filename" { gnu.write_text(f"{info.filename}{delimiter}") } else if selected == "name" { gnu.write_text(f"{info.name}{delimiter}") } else {
          if selected == "" { gnu.write_text(f"filename:       {info.filename}{delimiter}") }
          var parameter_index = 0
          for field in info.fields {
            var value = field.value
            if field.name == "parm" and parameter_index < info.params.len() {
              let param = info.params[parameter_index]
              let kind = if param.type == "" { "" } else { f" ({param.type})" }
              value = f"{param.name}:{param.description}{kind}"
              parameter_index += 1
            }
            if selected == "" and field.name != "parmtype" { let label = f"{field.name}:"; gnu.write_text(f"{label:<16}{value}{delimiter}") } else if field.name == selected { gnu.write_text(f"{value}{delimiter}") }
          }
        }
      }
      Err(problem) => { failure(f"ERROR: Module {gnu.quote(name)} not found", problem); ok = false }
    }
  }
  if ! ok { exit 1 }
}

## Insert one module file, forwarding kernel parameters as one typed value.
export proc insmod(argv: List[Str]) {
  var operands: List[Str] = []
  var positional = false
  for arg in argv {
    if ! positional and arg == "--" { positional = true; continue }
    if ! positional and operands.is_empty() {
      if common(arg, "Usage: insmod [OPTIONS] FILE [PARAMETERS...]\nInsert a kernel module file.") { return }
      if arg.starts_with("-") { unsupported(arg) }
    }
    operands += [arg]
  }
  if operands.is_empty() { gnu.missing_operand() }
  let args = operands |> drop(1)
  if let Err(problem) = linux.insmod(fp"{operands[0]}", params: parameters(args)) {
    failure(f"ERROR: could not insert module {operands[0]}", problem)
    exit 1
  }
}

## Remove each requested module, with optional kernel forced removal.
export proc rmmod(argv: List[Str]) {
  var force = false
  var verbose = false
  var names: List[Str] = []
  var operands = false
  for arg in short_options(argv, "fv") {
    if ! operands and arg == "--" { operands = true; continue }
    if ! operands and common(arg, "Usage: rmmod [OPTIONS] MODULE...\n  -f, --force     force removal\n  -v, --verbose   print each removal") { return }
    if ! operands and (arg == "-f" or arg == "--force") { force = true; continue }
    if ! operands and (arg == "-v" or arg == "--verbose") { verbose = true; continue }
    if ! operands and arg.starts_with("-") { unsupported(arg) }
    names += [arg]
  }
  if names.is_empty() { gnu.missing_operand() }
  var ok = true
  for name in names {
    let normalized = module_name(name)
    if verbose { gnu.write_text(f"rmmod {normalized}\n") }
    if let Err(problem) = linux.rmmod(normalized, force:) { failure(f"ERROR: could not remove module {normalized}", problem); ok = false }
  }
  if ! ok { exit 1 }
}

## Resolve module dependencies before insertion, or print the native plan.
export proc modprobe(argv: List[Str]) {
  var dry_run = false
  var remove = false
  var all = false
  var quiet = false
  var show = false
  var verbose = false
  var operands: List[Str] = []
  var positional = false
  for arg in short_options(argv, "nDvqar") {
    if ! positional and arg == "--" { positional = true; continue }
    if ! positional and operands.is_empty() {
      if common(arg, "Usage: modprobe [OPTIONS] MODULE [PARAMETERS...]\n  -r, --remove          remove modules and unused dependencies\n  -a, --all             process all named modules\n  -q, --quiet           suppress errors\n  -n, --dry-run         do not change loaded modules\n  -D, --show-depends    print insertion commands\n  -v, --verbose         print insertion commands") { return }
      if arg == "-r" or arg == "--remove" { remove = true; continue }
      if arg == "-a" or arg == "--all" { all = true; continue }
      if arg == "-q" or arg == "--quiet" { quiet = true; continue }
      if arg == "-n" or arg == "--dry-run" { dry_run = true; continue }
      if arg == "-D" or arg == "--show-depends" { show = true; dry_run = true; continue }
      if arg == "-v" or arg == "--verbose" { verbose = true; continue }
      if arg.starts_with("-") { unsupported(arg) }
    }
    operands += [arg]
  }
  if operands.is_empty() { gnu.missing_operand() }
  let args = operands |> drop(1)
  let params = if remove or all { "" } else { parameters(args) }
  let names = if remove or all { operands } else { [operands[0]] }
  var ok = true
  for name in names {
    var resolved = true
    if dry_run or verbose {
      match linux.module_plan(name, params:, remove:) {
        Ok(plan) => {
          if show or verbose {
            for item in plan {
              if verbose and ! show and item.loaded and ! remove { continue }
              if remove { gnu.write_text(f"rmmod {item.name}\n") } else { let suffix = if item.params == "" { "" } else { f" {item.params}" }; gnu.write_text(f"insmod {item.filename}{suffix}\n") }
            }
          }
        }
        Err(problem) => { if ! quiet { failure(f"FATAL: Module {name} could not be resolved", problem) }; resolved = false; ok = false }
      }
    }
    if ! dry_run and resolved {
      if let Err(problem) = linux.modprobe(name, params:, remove:) { if ! quiet { failure(f"ERROR: could not process {name}", problem) }; ok = false }
    }
  }
  if ! ok { exit 1 }
}

## Generate native module dependency indices for one kernel release.
export proc depmod(argv: List[Str]) {
  var version = ""
  for arg in argv {
    if common(arg, "Usage: depmod [-a|--all] [VERSION]\nGenerate kernel module dependency indices.") { return }
    if arg == "-a" or arg == "--all" { continue }
    if arg.starts_with("-") { unsupported(arg) }
    if version != "" { gnu.extra_operand(arg) }
    if ! rx"^[0-9]+\.[0-9]+".matches(arg) { gnu.usage_error(f"invalid kernel version {gnu.quote(arg)}") }
    version = arg
  }
  if let Err(problem) = linux.depmod(version) { failure("ERROR: could not generate module dependencies", problem); exit 1 }
}
