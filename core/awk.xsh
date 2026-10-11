#!/bin/xsh
use lib.awk as interpreter

# The usage and version texts are GNU awk's own, so help and error output
# match the reference implementation byte for byte.
const USAGE = """Usage: awk [POSIX or GNU style options] -f progfile [--] file ...
Usage: awk [POSIX or GNU style options] [--] 'program' file ...
POSIX options:		GNU long options: (standard)
	-f progfile		--file=progfile
	-F fs			--field-separator=fs
	-v var=val		--assign=var=val
Short options:		GNU long options: (extensions)
	-b			--characters-as-bytes
	-c			--traditional
	-C			--copyright
	-d[file]		--dump-variables[=file]
	-D[file]		--debug[=file]
	-e 'program-text'	--source='program-text'
	-E file			--exec=file
	-g			--gen-pot
	-h			--help
	-i includefile		--include=includefile
	-I			--trace
	-k			--csv
	-l library		--load=library
	-L[fatal|invalid|no-ext]	--lint[=fatal|invalid|no-ext]
	-M			--bignum
	-N			--use-lc-numeric
	-n			--non-decimal-data
	-o[file]		--pretty-print[=file]
	-O			--optimize
	-p[file]		--profile[=file]
	-P			--posix
	-r			--re-interval
	-s			--no-optimize
	-S			--sandbox
	-t			--lint-old
	-V			--version

To report bugs, use the `gawkbug' program.
For full instructions, see the node `Bugs' in `gawk.info'
which is section `Reporting Problems and Bugs' in the
printed version.  This same information may be found at
https://www.gnu.org/software/gawk/manual/html_node/Bugs.html.
PLEASE do NOT try to report bugs by posting in comp.lang.awk,
or by using a web forum such as Stack Overflow.

Source code for gawk may be obtained from
https://ftp.gnu.org/gnu/gawk/gawk-5.3.2.tar.gz

gawk is a pattern scanning and processing language.
By default it reads standard input and writes standard output.

Examples:
	awk '{ sum += $1 }; END { print sum }' file
	awk -F: '{ print $1 }' /etc/passwd
"""

const VERSION = """GNU Awk 5.3.2, API 4.0
Copyright (C) 1989, 1991-2025 Free Software Foundation.

This program is free software; you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation; either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program. If not, see http://www.gnu.org/licenses/.
"""

const LONG_OPTIONS = ["assign", "load", "include", "source", "file", "exec", "field-separator", "characters-as-bytes", "traditional", "copyright", "dump-variables", "debug", "gen-pot", "help", "trace", "csv", "lint", "bignum", "use-lc-numeric", "non-decimal-data", "pretty-print", "optimize", "profile", "posix", "re-interval", "no-optimize", "sandbox", "lint-old", "version"]
const OPTIONS_WITH_VALUE = ["assign", "load", "include", "source", "file", "exec", "field-separator"]
const SHORT_LONG: Map[Str] = {"v": "assign", "l": "load", "i": "include", "e": "source", "f": "file", "E": "exec", "F": "field-separator", "b": "characters-as-bytes", "c": "traditional", "C": "copyright", "d": "dump-variables", "D": "debug", "g": "gen-pot", "h": "help", "I": "trace", "k": "csv", "L": "lint", "M": "bignum", "N": "use-lc-numeric", "n": "non-decimal-data", "o": "pretty-print", "O": "optimize", "p": "profile", "P": "posix", "r": "re-interval", "s": "no-optimize", "S": "sandbox", "t": "lint-old", "V": "version"}
const SHORT_WITH_VALUE = ["v", "l", "i", "e", "f", "E", "F"]
const SHORT_OPTIONAL_VALUE = ["d", "D", "L", "o", "p"]

# A reserved word or builtin function cannot be the target of a -v assignment.
const RESERVED = ["BEGIN", "END", "function", "func", "if", "else", "while", "for", "do", "break", "continue", "next", "nextfile", "exit", "return", "delete", "in", "getline", "print", "printf", "length", "substr", "index", "split", "sub", "gsub", "match", "sprintf", "sin", "cos", "atan2", "exp", "log", "sqrt", "int", "rand", "srand", "tolower", "toupper", "system", "close", "fflush", "and", "or", "xor", "lshift", "rshift", "compl", "strtonum", "systime", "strftime", "mktime", "gensub", "asort", "asorti", "patsplit", "typeof", "isarray"]

# A program source: its diagnostic label ("cmd. line" or the file name) and text.
type Source = {label: Str, text: Str}
type Settings = {
  pieces: List[Source],
  assignments: List[Str],
  operands: List[Str],
  sandbox: Bool,
  version: Bool,
  help: Bool,
}

proc usage_failure(prefix: Str) [io] -> Unit {
  let _ = io.write_stderr(prefix + USAGE)
  let _ = io.flush_stderr()
  exit 1
}

# GNU awk looks program files up on AWKPATH and retries each name with ".awk"
# appended; a name with a slash is taken as it stands.
proc find_program(name: Str) [fs, env] -> Str? {
  var candidates: List[Str] = []
  if "/" in name { candidates = [name, f"{name}.awk"] } else {
    let search = env.get("AWKPATH") ?? ".:/usr/local/share/awk"
    for directory in search.split(":") {
      let base = if directory.is_empty() { "." } else { directory }
      candidates += [f"{base}/{name}", f"{base}/{name}.awk"]
    }
  }
  for candidate in candidates {
    match fs.stat(fp"{candidate}", follow_symlinks: true) {
      Ok(info) => { if info.kind == "file" { return candidate } }
      Err(_) => {}
    }
  }
  null
}

# A program file read from disk, and where it was found.
type Loaded = {source: Source, found: Str}

# Source text of -f, -i and -E program files; "-" is standard input.
proc read_program(name: Str, kind: Str) [fs, io, env, error] -> Loaded {
  if name == "-" {
    let text = io.stdin_text() ?? ""
    return {source: {label: "-", text: text}, found: "-"}
  }
  let found = find_program(name)
  if found == null {
    eprint f"awk: fatal: cannot open {kind} `{name}' for reading: No such file or directory"
    exit 2
  }
  match fp"{found ?? name}".read_text() {
    Ok(text) => { return {source: {label: name, text: text}, found: found ?? name} }
    Err(failure) => {
      var reason = failure.message
      let cut = reason.find(" (os error")
      if cut != null { reason = reason.byte_slice(0, length: cut ?? 0) }
      var tail = reason
      loop {
        let at = tail.find(": ")
        if at == null { break }
        tail = tail.byte_slice((at ?? 0) + 2)
      }
      eprint f"awk: fatal: cannot open {kind} `{name}' for reading: {tail}"
      exit 2
    }
  }
}

# The operand of a `@directive "name"` line, or null when the line is not one.
pure directive_operand(line: Str, directive: Str) -> Str? {
  let trimmed = line.trim()
  if ! trimmed.starts_with(directive) { return null }
  let rest = trimmed.byte_slice(directive.byte_len())
  if rest.is_empty() or ! (rest.starts_with(" ") or rest.starts_with("\t")) { return null }
  let quoted = rest.trim()
  if quoted.byte_len() < 2 or ! quoted.starts_with("\"") or ! quoted.ends_with("\"") { return null }
  let name = quoted.byte_slice(1, length: quoted.byte_len() - 2)
  if "\"" in name { return null }
  name
}

pure newlines(count: Int) -> Str { ["\n" for _ in range(count)].join("") }

type Expansion = {sources: List[Source], included: List[Str]}

# Replaces every `@include "file"` line of a program text by the file's text,
# keeping the line numbers of the text after it; a file already included is
# skipped, and `@load` names a library this implementation cannot provide.
proc expand_includes(source: Source, seen: List[Str]) [fs, io, env, error] -> Expansion {
  let lines = source.text.split("\n")
  var sources: List[Source] = []
  var included = seen
  var chunk_start = 0
  var label_here = source.label
  for index in range(lines.len()) {
    let library = directive_operand(lines[index], "@load")
    if library != null {
      eprint f"awk: {source.label}:{index + 1}: error: cannot open shared library `{library ?? ""}' for reading: No such file or directory"
      exit 1
    }
    let wanted = directive_operand(lines[index], "@include")
    if wanted == null { continue }
    let name = wanted ?? ""
    let found = find_program(name)
    if found == null {
      eprint f"awk: {source.label}:{index + 1}: error: cannot open source file `{name}' for reading: No such file or directory"
      exit 1
    }
    let before = lines[chunk_start..index].join("\n")
    if ! before.trim().is_empty() { sources += [{label: label_here, text: newlines(chunk_start) + before + "\n"}] }
    chunk_start = index + 1
    # Once a command-line program has included a file, GNU awk names the
    # whole program text, not "cmd. line", in the messages for what follows.
    if source.label == "cmd. line" { label_here = source.text }
    if (found ?? name) in included { continue }
    included += [found ?? name]
    let loaded = read_program(name, "source file")
    let inner = expand_includes(loaded.source, included)
    sources += inner.sources
    included = inner.included
  }
  if chunk_start == 0 { return {sources: [source], included: included} }
  let rest = lines[chunk_start..].join("\n")
  if ! rest.trim().is_empty() { sources += [{label: label_here, text: newlines(chunk_start) + rest}] }
  {sources: sources, included: included}
}

# One option occurrence: its canonical long name and value (null when it takes
# none).
type Given = {name: Str, value: Str?}

proc main(...args: List[Str]) [fs, process, env, error, io, time] {
  var settings: Settings = {pieces: [], assignments: [], operands: [], sandbox: false, version: false, help: false}
  var index = 0
  var program_given = false
  var seen: List[Given] = []
  var include_found: List[Str] = []
  var program_found: List[Str] = []
  while index < args.len() {
    let argument = args[index]
    if argument == "--" { index += 1; break }
    if ! argument.starts_with("-") or argument == "-" { break }
    index += 1
    if argument.starts_with("--") {
      var body = argument.byte_slice(2)
      var value: Str? = null
      let equals = body.find("=")
      if equals != null {
        value = body.byte_slice((equals ?? 0) + 1)
        body = body.byte_slice(0, length: equals ?? 0)
      }
      let candidates = [name for name in LONG_OPTIONS if name.starts_with(body)]
      var option = ""
      if body in LONG_OPTIONS { option = body } else if candidates.len() == 1 { option = candidates[0] } else {
        usage_failure("")
      }
      if option in OPTIONS_WITH_VALUE and value == null {
        if index >= args.len() { usage_failure(f"awk: option '--{option}' requires an argument\n") }
        value = args[index]
        index += 1
      }
      seen += [{name: option, value: value}]
    } else {
      var at = 1
      while at < argument.byte_len() {
        let letter = argument.byte_slice(at, length: 1)
        at += 1
        if letter not in SHORT_LONG { usage_failure("") }
        let option = SHORT_LONG.get(letter) ?? ""
        var value: Str? = null
        if letter in SHORT_WITH_VALUE {
          if at < argument.byte_len() { value = argument.byte_slice(at); at = argument.byte_len() } else {
            if index >= args.len() { usage_failure(f"awk: option requires an argument -- {letter}\n") }
            value = args[index]
            index += 1
          }
        } else if letter in SHORT_OPTIONAL_VALUE and at < argument.byte_len() {
          value = argument.byte_slice(at)
          at = argument.byte_len()
        }
        seen += [{name: option, value: value}]
      }
    }
  }
  # --posix and --traditional narrow the language; --posix wins when both are given.
  var mode = 0
  for entry in seen {
    if entry.name == "posix" { mode = 2 } else if entry.name == "traditional" and mode == 0 { mode = 1 }
  }
  for entry in seen {
    let option = entry.name
    let given = entry.value ?? ""
    if option == "version" { settings.version = true } else if option == "help" { settings.help = true } else if option == "source" {
      let expanded = expand_includes({label: "cmd. line", text: given}, include_found)
      settings.pieces += expanded.sources
      include_found = expanded.included
      program_given = true
    } else if option == "file" or option == "exec" {
      let loaded = read_program(given, "source file")
      if loaded.found in include_found { eprint f"awk: fatal: cannot include `{given}' and use it as a program file"; exit 2 }
      program_found += [loaded.found]
      let expanded = expand_includes(loaded.source, include_found)
      settings.pieces += expanded.sources
      include_found = expanded.included
      program_given = true
    } else if option == "include" {
      let loaded = read_program(given, "source file")
      if loaded.found in program_found { eprint f"awk: fatal: cannot include `{given}' and use it as a program file"; exit 2 }
      if loaded.found not in include_found {
        include_found += [loaded.found]
        let expanded = expand_includes(loaded.source, include_found)
        settings.pieces += expanded.sources
        include_found = expanded.included
      }
    } else if option == "load" {
      eprint f"awk: fatal: cannot open shared library `{given}' for reading: No such file or directory"
      exit 2
    } else if option == "assign" {
      let equals = given.find("=")
      if equals == null {
        usage_failure(f"awk: `{given}' argument to `-v' not in `var=value' form\n\n")
      }
      let name = given.byte_slice(0, length: equals ?? 0)
      if name in RESERVED { eprint f"awk: fatal: cannot use gawk builtin `{name}' as variable name"; exit 2 }
      if ! interpreter.binding_name(name) { eprint f"awk: fatal: `{name}' is not a legal variable name"; exit 2 }
      settings.assignments += [given]
    } else if option == "field-separator" {
      # Traditional awk reads -F t as a tab.
      settings.assignments += [if mode == 1 and given == "t" { "FS=\t" } else { f"FS={given}" }]
    } else if option == "sandbox" { settings.sandbox = true } else if option == "bignum" {
      eprint "awk: warning: -M ignored: MPFR/GMP support not compiled in"
    } else if option in ["traditional", "posix", "re-interval", "no-optimize", "optimize", "lint", "lint-old", "use-lc-numeric"] {
      # Accepted for compatibility; this implementation has one mode.
    } else {
      eprint f"awk: fatal: option `--{option}' is not supported"
      exit 2
    }
  }
  if settings.version {
    io.write_stdout(VERSION)?
    return
  }
  if settings.help {
    io.write_stdout(USAGE)?
    return
  }
  if ! program_given {
    if index >= args.len() { usage_failure("") }
    let expanded = expand_includes({label: "cmd. line", text: args[index]}, include_found)
    settings.pieces += expanded.sources
    include_found = expanded.included
    index += 1
  }
  let operands = args[index..]
  match interpreter.execute(settings.pieces, operands, settings.assignments, settings.sandbox, mode) {
    Ok(status) => { exit status }
    Err(interpreter.AwkError.Syntax {message}) => {
      let _ = io.write_stderr(f"awk: {message}\n")
      let _ = io.flush_stderr()
      exit 1
    }
    Err(interpreter.AwkError.Fatal {message, pending}) => {
      let _ = io.write_stdout(pending)
      let _ = io.flush_stdout()
      let _ = io.write_stderr(f"awk: {message}\n")
      let _ = io.flush_stderr()
      exit 2
    }
    Err(interpreter.AwkError.Plain {message, pending}) => {
      let _ = io.write_stdout(pending)
      let _ = io.flush_stdout()
      let _ = io.write_stderr(f"awk: {message}\n")
      let _ = io.flush_stderr()
      exit 2
    }
    Err(failure) => {
      let _ = io.write_stderr(f"awk: fatal: {failure.message}\n")
      let _ = io.flush_stderr()
      exit 2
    }
  }
}
