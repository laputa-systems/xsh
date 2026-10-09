#!/bin/xsh
use lib.gnu as gnu

const USAGE = """Usage: mkdir [OPTION]... DIRECTORY...
Create the DIRECTORY(ies), if they do not already exist.

  -m, --mode=MODE   set file mode (as in chmod), not a=rwx - umask
  -p, --parents     make parent directories as needed
  -v, --verbose     print a message for each created directory
  -Z, --context     set the SELinux security context of each created directory
      --help        display this help and exit
      --version     output version information and exit
"""

type MkdirOptions = {
  parents: Bool,
  mode: Str,
  verbose: Bool,
  help: Bool,
  version: Bool,
  directories: List[Str],
}

pure octal_mode(spec: Str) -> Int? {
  return null when spec == ""
  var mode = 0
  for ch in spec {
    let digit = ch.parse_int() ?? -1
    if digit < 0 or digit > 7 { return null }
    mode = mode * 8 + digit
    if mode > 4095 { return null }
  }
  mode
}

pure symbolic_class_mask(who: Str, mask: Int) -> Int {
  let classes = if who == "" or "a" in who { "ugo" } else { who }
  var result = 0
  if "u" in classes {
    result = result + 0o4000
    for bit in [0o400, 0o200, 0o100] {
      if (who != "" and "a" not in who) or mask.bit_and(bit) == 0 { result = result + bit }
    }
  }
  if "g" in classes {
    result = result + 0o2000
    for bit in [0o40, 0o20, 0o10] {
      if (who != "" and "a" not in who) or mask.bit_and(bit) == 0 { result = result + bit }
    }
  }
  if "o" in classes {
    result = result + 0o1000
    for bit in [0o4, 0o2, 0o1] {
      if (who != "" and "a" not in who) or mask.bit_and(bit) == 0 { result = result + bit }
    }
  }
  result
}

pure symbolic_permissions(perms: Str, who: Str, mask: Int) -> Int? {
  let classes = if who == "" or "a" in who { "ugo" } else { who }
  let restricted = who == "" or "a" in who
  var result = 0

  for perm in perms {
    if perm not in "rwxXst" { return null }
    if "u" in classes {
      let bit = match perm { "r" => 0o400, "w" => 0o200, "x" => 0o100, "X" => 0o100, "s" => 0o4000, else => 0 }
      if bit != 0 and (! restricted or mask.bit_and(bit) == 0) { result = result + bit }
    }
    if "g" in classes {
      let bit = match perm { "r" => 0o40, "w" => 0o20, "x" => 0o10, "X" => 0o10, "s" => 0o2000, else => 0 }
      if bit != 0 and (! restricted or mask.bit_and(bit) == 0) { result = result + bit }
    }
    if "o" in classes {
      let bit = match perm { "r" => 0o4, "w" => 0o2, "x" => 0o1, "X" => 0o1, "t" => 0o1000, else => 0 }
      if bit != 0 and (! restricted or mask.bit_and(bit) == 0) { result = result + bit }
    }
  }
  result
}

pure parse_symbolic_mode(spec: Str, mask: Int) -> Int? {
  var mode = 0o777
  for clause in spec.split(",") {
    var who = ""
    var op = ""
    var perms = ""
    for ch in clause {
      if op == "" and ch in "ugoa" {
        who = f"{who}{ch}"
      } else if op == "" and ch in "+-=" {
        op = ch
      } else {
        perms = f"{perms}{ch}"
      }
    }
    if op == "" { return null }
    for ch in who {
      if ch not in "ugoa" { return null }
    }
    let maybe_permissions = symbolic_permissions(perms, who, mask)
    if maybe_permissions == null { return null }
    let perms_value = maybe_permissions ?? 0
    let clear = symbolic_class_mask(who, mask)
    if op == "+" {
      mode = mode.bit_or(perms_value)
    } else if op == "-" {
      mode = mode.clear_bits(perms_value)
    } else {
      mode = mode.clear_bits(clear).bit_or(perms_value)
    }
  }
  mode
}

pure mode_value(spec: Str, mask: Int) -> Int? {
  if "+" in spec or "-" in spec or "=" in spec {
    return parse_symbolic_mode(spec, mask)
  }
  octal_mode(spec)
}

pure invalid_mode_index(spec: Str) -> Int? {
  var index = 0
  for ch in spec {
    if ch not in "ugoa+-=rwxXst," { return index }
    index += 1
  }
  null
}

pure mode_option_given(argv: List[Str], posixly_correct: Bool) -> Bool {
  var options = true
  for arg in argv {
    if options and arg == "--" { options = false; continue }
    if ! options { continue }
    if posixly_correct and (! arg.starts_with("-") or arg == "-") { options = false; continue }
    if arg == "-m" or arg == "--mode" { return true }
    if arg.starts_with("--") {
      let equal_at = arg.find("=")
      let name = if equal_at == null { arg } else { arg.byte_slice(0, length: equal_at ?? 0) }
      if "--mode".starts_with(name) { return true }
    } else if arg.starts_with("-") and arg != "-" {
      for option in arg.byte_slice(1) {
        if option == "m" { return true }
      }
    }
  }
  false
}

proc invalid_mode(spec: Str) [process, env, io] -> Unit {
  if let index = invalid_mode_index(spec) {
    if unix.isatty(2) {
      let column = index + 4
      gnu.error(f"invalid mode {gnu.quote_value(spec)}: invalid operator")
      eprint f"╭─[ mkdir:1:{column} ]"
      eprint f"1 │ -m {spec}"
      var padding = ""
      for _ in range(column - 1) { padding = f"{padding} " }
      eprint f"  │ {padding}^"
      eprint "╰─"
      exit 1
    }
  }
  gnu.usage_error(f"invalid mode {gnu.quote_value(spec)}")
}

pure directory_prefixes(item: Str) -> List[Str] {
  var paths: List[Str] = []
  var current = if item.starts_with("/") { "/" } else { "" }
  for component in item.split("/") {
    continue when component == ""
    current = if current == "" { component } else if current == "/" { f"/{component}" } else { f"{current}/{component}" }
    continue when component == "." or component == ".."
    paths += [current]
  }
  paths
}

proc create_directory(item: Str, parents: Bool, mask: Int, requested_mode: Int?) [fs, error] -> Result[List[Str]] {
  let target = fp"{item}"
  if ! parents {
    target.mkdir(parents: false)?
    if let mode = requested_mode { target.chmod(mode)? }
    return Ok([item])
  }

  var made: List[Str] = []
  let final_path = target.normalize()
  let parent_mode = 0o777.clear_bits(mask).bit_or(0o300)
  for prefix in directory_prefixes(item) {
    let directory = fp"{prefix}"
    if directory.exists()? { continue }
    match directory.mkdir(parents: false) {
      Ok(_) => {
        made += [prefix]
        let is_final = directory.normalize() == final_path
        if is_final {
          if let mode = requested_mode { directory.chmod(mode)? }
        } else {
          let inherited = fs.stat(directory)?.mode.bit_and(0o2000)
          directory.chmod(parent_mode.bit_or(inherited))?
        }
      }
      Err(_) => {
        match target.mkdir(parents: true) {
          Ok(_) => return Ok(made),
          Err(failure) => return Err(failure),
        }
      }
    }
  }
  target.mkdir(parents: true)?
  Ok(made)
}

proc main(...argv: List[Str]) [fs, process, env, io, error] {
  let opts: MkdirOptions = cli.applet(
    argv,
    {
      gnu: {
        status: 1,
        unsupported: {
          "-Z": "SELinux security contexts are not available",
          "--context": "SELinux security contexts are not available",
        },
      },
      parents: {form: "-p --parents", default: false},
      mode: {form: "-m --mode MODE", default: ""},
      verbose: {form: "-v --verbose", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      directories: {form: "...DIRECTORY"},
    },
  )?

  if opts.help {
    gnu.help(USAGE)
    return
  }
  if opts.version {
    gnu.version("mkdir")
    return
  }
  if opts.directories.len() == 0 {
    gnu.missing_operand()
  }

  let mask = fs.umask()?
  let mode_given = mode_option_given(argv, (env.get_or("POSIXLY_CORRECT", "") ?? "") != "")
  let mode: Int? = if mode_given { mode_value(opts.mode, mask) } else { null }
  if mode_given and mode == null {
    invalid_mode(opts.mode)
  }

  var had_error = false
  for item in opts.directories {
    if item == "" {
      gnu.error("cannot create directory '': No such file or directory")
      had_error = true
      continue
    }

    match create_directory(item, opts.parents, mask, mode) {
      Ok(made) => {
        if opts.verbose {
          for created in made {
            gnu.write_text(f"mkdir: created directory {gnu.quote(created)}\n")
          }
        }
      }
      Err(failure) => {
        gnu.error(f"cannot create directory {gnu.quote(item)}: {gnu.strerror(failure)}")
        had_error = true
      }
    }
  }

  exit 1 when had_error
}
