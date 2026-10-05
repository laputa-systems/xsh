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
    "leftcode" => "lc"
    "rightcode" => "rc"
    "endcode" => "ec"
    else => ""
  }
}

proc main(...argv: List[Str]) [process, env, io, fs, error] {
  var shell = ""
  var database = false
  var display = false
  var file = ""
  var operands = false
  for arg in argv {
    if ! operands and arg == "--" { operands = true; continue }
    if ! operands and arg == "--help" {
      gnu.help("Usage: dircolors [OPTION]... [FILE]\nOutput commands to set LS_COLORS.\n  -b, --sh, --bourne-shell  Bourne shell output\n  -c, --csh, --c-shell      C shell output\n  -p, --print-database      output default database\n      --print-ls-colors     display color codes\nFILE defaults to the built-in database; '-' reads standard input.")
      return
    }
    if ! operands and arg == "--version" { gnu.version("dircolors"); return }
    if ! operands and arg.starts_with("--") {
      match arg {
        "--sh" | "--bourne-shell" => shell = "b"
        "--csh" | "--c-shell" => shell = "c"
        "--print-database" => database = true
        "--print-ls-colors" => display = true
        else => gnu.usage_error(f"unrecognized option {gnu.quote(arg)}")
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
      if file != "" { gnu.extra_operand(arg) }
      file = arg
    }
  }
  if (shell != "" and (database or display)) or (database and display) { gnu.usage_error("options are mutually exclusive") }
  if database {
    if file != "" { gnu.extra_operand(file) }
    gnu.write_text(colors.DATABASE)
    return
  }
  if shell == "" and ! display {
    let selected = env.get_or("SHELL", "") ?? ""
    if selected == "" { gnu.error("no SHELL environment variable, and no shell type option given"); exit 1 }
    shell = if fp"{selected}".basename() == "csh" or fp"{selected}".basename() == "tcsh" { "c" } else { "b" }
  }
  var input = colors.DATABASE
  if file != "" {
    match gnu.read_operand(file) {
      Ok(data) => {
        match data.utf8() {
          Ok(value) => input = value
          else => { gnu.error("database is not UTF-8"); exit 1 }
        }
      }
      Err(failure) => { gnu.name_error(file, failure); exit 1 }
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
    let comment_at = line.find(" #") ?? -1
    if comment_at >= 0 { line = line.byte_slice(0, comment_at).trim() }
    let words = line.replace("\t", with: " ").split(" ") |> where . != ""
    if words.len() < 2 { gnu.error(f"{file}:{line_number}: invalid line; missing second token"); exit 1 }
    let key = words[0]
    let value = line.byte_slice(key.byte_len()).trim()
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
    if code == "" { gnu.error(f"{file}:{line_number}: unrecognized keyword {gnu.quote(key)}"); exit 1 }
    if display { output = f"{output}\x1b[{value}m{code}\t{value}\x1b[0m\n" } else {
      let escaped_code = code.replace("'", with: "'\\''").replace(":", with: "\\:")
      let escaped_value = value.replace("'", with: "'\\''").replace(":", with: "\\:")
      output = f"{output}{escaped_code}={escaped_value}:"
    }
  }
  if display { gnu.write_text(output) } else if shell == "c" { gnu.write_text(f"setenv LS_COLORS '{output}'\n") } else { gnu.write_text(f"LS_COLORS='{output}';\nexport LS_COLORS\n") }
}
