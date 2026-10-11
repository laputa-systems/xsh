#!/bin/xsh
use lib.gnu
use lib.date_dircolors as colors

pure term_matches(pattern: Str, value: Str) -> Bool {
  var regex_text = "^"
  var bracket = false
  for char in pattern {
    if char == "*" and ! bracket { regex_text = f"{regex_text}.*" } else if char == "?" and ! bracket { regex_text = f"{regex_text}." } else if char == "[" { bracket = true; regex_text = f"{regex_text}[" } else if char == "]" { bracket = false; regex_text = f"{regex_text}]" } else if char == "!" and bracket { regex_text = f"{regex_text}^" } else if ! bracket and ".+(){}^$|\\".find(char) != null { regex_text = f"{regex_text}\\{char}" } else { regex_text = f"{regex_text}{char}" }
  }
  if bracket { return false }
  match regex.compile(f"{regex_text}$") {
    Ok(found) => found.matches(value)
    Err(failure) => false
  }
}

pure color_key(key: Str) -> Str {
  match key.lower() {
    "normal" | "norm" => "no"
    "file" => "fi"
    "reset" => "rs"
    "dir" => "di"
    "link" | "lnk" | "symlink" => "ln"
    "multihardlink" => "mh"
    "fifo" | "pipe" => "pi"
    "sock" => "so"
    "door" => "do"
    "blk" | "block" => "bd"
    "chr" | "char" => "cd"
    "orphan" => "or"
    "missing" => "mi"
    "setuid" => "su"
    "setgid" => "sg"
    "capability" => "ca"
    "sticky_other_writable" | "owt" => "tw"
    "other_writable" | "owr" => "ow"
    "sticky" => "st"
    "exec" => "ex"
    "left" | "leftcode" => "lc"
    "right" | "rightcode" => "rc"
    "end" | "endcode" => "ec"
    "clrtoeol" => "cl"
    else => ""
  }
}

pure shell_output_field(text: Str) -> Str {
  var output = ""
  var needs_escape = true
  for char in text {
    if char == "'" {
      output = f"{output}'\\''"
      needs_escape = true
    } else if char == "\\" or char == "^" {
      output = f"{output}{char}"
      needs_escape = !needs_escape
    } else if char == ":" or char == "=" {
      if needs_escape { output = f"{output}\\" }
      output = f"{output}{char}"
      needs_escape = true
    } else {
      output = f"{output}{char}"
      needs_escape = true
    }
  }
  output
}

# Operands that are not UTF-8 reach the text parser as NUL markers; these helpers
# recover the raw bytes for opening files and for naming them in messages.
proc display_name(name: Bytes) [env] -> Str {
  match name.utf8() {
    Ok(text) => text
    Err(_) => gnu.quote_bytes(name, always: false)
  }
}

proc extra_operand(name: Bytes) [process, env] -> Unit {
  gnu.usage_error(f"extra operand {gnu.quote_bytes(name)}")
}

# Long options in the order GNU lists their possibilities; `--help` and
# `--version` are included so that their abbreviations resolve like the rest.
const LONG_OPTIONS = ["--bourne-shell", "--c-shell", "--csh", "--help", "--print-database", "--print-ls-colors", "--sh", "--version"]

# The long options an argument names: an exact name, or every name it prefixes
# (getopt_long accepts any unambiguous abbreviation).
pure long_candidates(arg: Str) -> List[Str] {
  if arg in LONG_OPTIONS { return [arg] }
  [name for name in LONG_OPTIONS if name.starts_with(arg)]
}

# Aliases of one option (`--csh` and `--c-shell`) are not ambiguous with each
# other, as in getopt_long; the action is what distinguishes options.
pure long_action(name: Str) -> Str {
  match name {
    "--sh" | "--bourne-shell" => "bourne"
    "--csh" | "--c-shell" => "csh"
    "--print-database" => "print-database"
    "--print-ls-colors" => "print-ls-colors"
    "--help" => "help"
    else => "version"
  }
}

proc is_directory(name: Bytes) [fs, error] -> Result[Bool] {
  Path.parse_bytes(name)?.is_dir()
}

proc read_database(name: Bytes) [fs, error, io] -> Result[Bytes, Error] {
  return io.stdin_bytes() when name == b"-"

  Path.parse_bytes(name)?.read_bytes()
}

proc main(...argv: List[Bytes]) [process, env, io, fs, error] {
  let prepared = gnu.prepare_arguments(argv)
  var shell = ""
  var database = false
  var display = false
  var file = ""
  var operands = false
  for arg in prepared.text {
    if ! operands and arg == "--" { operands = true; continue }
    if ! operands and arg.starts_with("--") {
      let found = long_candidates(arg)
      if found.is_empty() { gnu.usage_error(f"unrecognized option {gnu.quote(arg)}") }
      let action = long_action(found[0])
      if [name for name in found if long_action(name) != action].len() > 0 {
        gnu.usage_error(f"option '{arg}' is ambiguous; possibilities: {[f"'{name}'" for name in found].join(" ")}")
      }
      match action {
        "bourne" => shell = "b"
        "csh" => shell = "c"
        "print-database" => database = true
        "print-ls-colors" => display = true
        "help" => {
          gnu.help("Usage: dircolors [OPTION]... [FILE]\nOutput commands to set LS_COLORS.\n  -b, --sh, --bourne-shell  Bourne shell output\n  -c, --csh, --c-shell      C shell output\n  -p, --print-database      output default database\n      --print-ls-colors     display color codes\nFILE defaults to the built-in database; '-' reads standard input.")
          return
        }
        "version" => {
          gnu.version("dircolors")
          return
        }
      }
    } else if ! operands and arg.starts_with("-") and arg != "-" {
      for flag in arg.byte_slice(1) {
        match flag {
          "b" => shell = "b"
          "c" => shell = "c"
          "p" => database = true
          else => gnu.usage_error(f"invalid option -- {gnu.quote(flag)}")
        }
      }
    } else {
      if file != "" { extra_operand(gnu.argument_bytes(arg, prepared.raw)) }
      file = arg
    }
  }
  if database and display { gnu.usage_error("options --print-database and --print-ls-colors are mutually exclusive") }
  if shell != "" and (database or display) { gnu.usage_error("the options to output non shell syntax,\nand to select a shell syntax are mutually exclusive") }
  if database {
    if file != "" { extra_operand(gnu.argument_bytes(file, prepared.raw)) }
    gnu.write_text(colors.DATABASE)
    return
  }
  if shell == "" and ! display {
    let selected = env.get_or("SHELL", "") ?? ""
    if selected == "" { gnu.error("no SHELL environment variable, and no shell type option given"); exit 1 }
    shell = if fp"{selected}".basename() == "csh" or fp"{selected}".basename() == "tcsh" { "c" } else { "b" }
  }
  var input = colors.DATABASE
  let file_bytes = gnu.argument_bytes(file, prepared.raw)
  let file_name = display_name(file_bytes)
  if file != "" {
    if file != "-" and (is_directory(file_bytes) ?? false) { gnu.error(f"{gnu.quote_bytes(file_bytes, always: false)}: read error: Is a directory"); exit 1 }
    match read_database(file_bytes) {
      Ok(data) => {
        match data.utf8() {
          Ok(value) => input = value
          else => { gnu.error("database is not UTF-8"); exit 1 }
        }
      }
      Err(failure) => { gnu.error(f"{gnu.quote_bytes(file_bytes, always: false)}: {gnu.strerror(failure)}"); exit 1 }
    }
  }
  let term = env.get_or("TERM", "none") ?? "none"
  let colorterm = env.get_or("COLORTERM", "") ?? ""
  var selectors = false
  var selected = true
  var entries = false
  var output = ""
  var line_number = 0
  for original in input.lines() {
    line_number += 1
    var line = original.trim()
    if line == "" or line.starts_with("#") { continue }
    let words = line.replace("\t", with: " ").split(" ") |> where . != ""
    if words.len() < 2 { gnu.error(f"{file_name}:{line_number}: invalid line;  missing second token"); exit 1 }
    let key = words[0]
    var value = line.byte_slice(key.byte_len()).trim()
    if value.starts_with("#") { gnu.error(f"{file_name}:{line_number}: invalid line;  missing second token"); exit 1 }
    let value_comment_at = value.find("#") ?? -1
    if value_comment_at >= 0 { value = value.byte_slice(0, value_comment_at).trim() }
    if value == "" { gnu.error(f"{file_name}:{line_number}: invalid line;  missing second token"); exit 1 }
    if key.lower() == "term" or key.lower() == "colorterm" {
      if ! selectors or entries { selected = false; entries = false; selectors = true }
      if term_matches(value, if key.lower() == "term" { term } else { colorterm }) { selected = true }
      continue
    }
    entries = true
    if ! selected { continue }
    if key.lower() == "options" or key.lower() == "color" or key.lower() == "eightbit" { continue }
    let named = color_key(key)
    let code = if key.starts_with(".") { f"*{key}" } else if key.starts_with("*") { key } else { named }
    if code == "" { gnu.error(f"{file_name}:{line_number}: unrecognized keyword {gnu.quote(key)}"); exit 1 }
    if display { output = f"{output}\x1b[{value}m{code}\t{value}\x1b[0m\n" } else {
      let escaped_code = shell_output_field(code)
      let escaped_value = shell_output_field(value)
      output = f"{output}{escaped_code}={escaped_value}:"
    }
  }
  if display { gnu.write_text(output) } else if shell == "c" { gnu.write_text(f"setenv LS_COLORS '{output}'\n") } else { gnu.write_text(f"LS_COLORS='{output}';\nexport LS_COLORS\n") }
}
