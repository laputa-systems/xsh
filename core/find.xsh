#!/bin/xsh
use lib.gnu
use lib.search
use lib.date_parse as dates
use lib.find_format as fmt
use lib.find_match as match_name

# One node of the parsed expression. Operators use `left` and `right`; every
# other kind carries the fields its predicate needs: `text` the pattern or
# letter, `compare` the +/-/= sense of a numeric argument (1, -1, 0), `number`
# and `bound` its operands, `argv` a command or type list, `file` an output
# file and `segments` a compiled -printf format.
type Node = {kind: Str, text: Str, flag: Bool, compare: Int, number: Int, bound: Int, left: Int, right: Int, argv: List[Str], file: Str, segments: List[fmt.Segment]}

# Settings for the whole run plus the positional ones captured while parsing:
# the regex dialect and the day-start origin apply to the tests parsed after
# them, and `last_token`/`last_test` feed the diagnostics that name them.
type Options = {follow: Int, minimum: Int, maximum: Int, depth: Bool, device: Bool, warn: Bool, start_ns: Int, day_start_ns: Int, regex_type: Str, last_token: Str, last_test: Str}

type Parsed = {nodes: List[Node], root: Int, at: Int, opts: Options}
type Deferred = {node: Int, name: Str, directory: Path}
type Decision = {matched: Bool, prune: Bool, quit: Bool, failed: Bool, deferred: List[Deferred]}
type Visit = {quit: Bool, failed: Bool, deferred: List[Deferred]}
type Ancestor = {identity: Str, path: Path}
type Run = {nodes: List[Node], root: Int, follow: Int, opts: Options, stat_needed: Bool}
type Count = {compare: Int, number: Int}
type Stat = {atime_ns: Int, birth_ns: Int?, blksize: Int, blocks_512: Int, ctime_ns: Int, dev: Int, gid: Int, ino: Int, kind: Str, mode: Int, mtime_ns: Int, nlink: Int, rdev: Int, size: Int, uid: Int}

const NEEDS_ARGUMENT = ["-name","-iname","-path","-ipath","-wholename","-iwholename","-lname","-ilname","-regex","-iregex","-regextype","-type","-xtype","-size","-mtime","-atime","-ctime","-mmin","-amin","-cmin","-newer","-anewer","-cnewer","-perm","-user","-group","-uid","-gid","-links","-inum","-samefile","-used","-maxdepth","-mindepth","-printf","-fprint","-fprint0","-fls","-fstype"]
const NO_ARGUMENT = ["-true","-false","-print","-print0","-prune","-quit","-delete","-empty","-ls","-nouser","-nogroup","-readable","-writable","-executable","-depth","-d","-mount","-xdev","-noleaf","-daystart","-follow","-warn","-nowarn","-ignore_readdir_race","-noignore_readdir_race"]
const GLOBAL_OPTIONS = ["-depth","-d","-maxdepth","-mindepth","-mount","-xdev","-noleaf","-ignore_readdir_race","-noignore_readdir_race"]
const POSITIONAL_OPTIONS = ["-regextype","-daystart","-follow","-warn","-nowarn"]
const ACTIONS = ["print","print0","printf","fprint","fprint0","fprintf","ls","fls","delete","exec","execdir","ok","okdir","exec+","execdir+"]
const DAY_NS = 86400000000000
const MINUTE_NS = 60000000000
const BATCH_LIMIT = 131072

# Symbolic modes are changes applied to an initially empty mode, with no
# umask filtering. Conditional execute is evaluated separately for directories.
pure permission_mode(spec: Str, directory: Bool) -> Int? {
  var numeric_mode = 0
  var octal = spec != ""
  for digit in spec {
    let number = digit.parse_int() ?? -1
    if number < 0 or number > 7 { octal = false; break }
    numeric_mode = numeric_mode * 8 + number
  }
  if octal { return if numeric_mode > 0o7777 { null } else { numeric_mode } }
  var mode = 0
  for clause in spec.split(",") {
    var at = 0
    var who = ""
    while at < clause.byte_len() and clause.byte_slice(at,1) in "ugoa" { who = f"{who}{clause.byte_slice(at,1)}"; at += 1 }
    if who == "" or "a" in who { who = "ugo" }
    if at >= clause.byte_len() { return null }
    while at < clause.byte_len() {
      let operation = clause.byte_slice(at,1); at += 1
      if operation not in ["+","-","="] { return null }
      let start = at
      while at < clause.byte_len() and clause.byte_slice(at,1) not in ["+","-","="] { at += 1 }
      let permissions = clause.byte_slice(start,at - start)
      var ordinary = 0
      var bits = 0
      if permissions in ["u","g","o"] {
        ordinary = mode / (if permissions == "u" { 64 } else if permissions == "g" { 8 } else { 1 }) % 8
      } else {
        for permission in permissions {
          match permission {
            "r" => ordinary = ordinary.bit_or(4)
            "w" => ordinary = ordinary.bit_or(2)
            "x" => ordinary = ordinary.bit_or(1)
            "X" => { if directory or mode.bit_and(0o111) != 0 { ordinary = ordinary.bit_or(1) } }
            "s" => { if "u" in who { bits = bits.bit_or(0o4000) }; if "g" in who { bits = bits.bit_or(0o2000) } }
            "t" => { if "o" in who { bits = bits.bit_or(0o1000) } }
            _ => return null
          }
        }
      }
      var mask = 0
      if "u" in who { bits = bits.bit_or(ordinary * 64); mask = mask.bit_or(0o4700) }
      if "g" in who { bits = bits.bit_or(ordinary * 8); mask = mask.bit_or(0o2070) }
      if "o" in who { bits = bits.bit_or(ordinary); mask = mask.bit_or(0o1007) }
      if operation == "+" { mode = mode.bit_or(bits) } else if operation == "-" { mode = mode.clear_bits(bits) } else { mode = mode.clear_bits(mask).bit_or(bits) }
    }
  }
  mode
}

pure compare_number(actual: Int, compare: Int, expected: Int) -> Bool {
  if compare > 0 { actual > expected } else if compare < 0 { actual < expected } else { actual == expected }
}

# Split a leading `+` or `-` from a decimal argument; null unless the rest is
# a non-negative decimal.
pure signed_number(text: Str) -> Count? {
  var compare = 0
  var digits = text
  if text.starts_with("+") { compare = 1; digits = text.byte_slice(1) } else if text.starts_with("-") { compare = -1; digits = text.byte_slice(1) }
  if digits == "" { return null }
  for digit in digits { if (digit.parse_int() ?? -1) < 0 { return null } }
  let number = digits.parse_int() ?? -1
  if number < 0 { return null }
  {compare: compare, number: number}
}

pure all_digits(text: Str) -> Bool {
  if text == "" { return false }
  for digit in text { if (digit.parse_int() ?? -1) < 0 { return false } }
  true
}

# A bare non-negative decimal, as -maxdepth and -mindepth require.
pure plain_number(text: Str) -> Int? {
  if text == "" { return null }
  for digit in text { if (digit.parse_int() ?? -1) < 0 { return null } }
  let number = text.parse_int() ?? -1
  if number < 0 { null } else { number }
}

pure blank_node(kind: Str) -> Node {
  {kind: kind, text: "", flag: false, compare: 0, number: 0, bound: 0, left: -1, right: -1, argv: [], file: "", segments: []}
}

pure with_node(parsed: Parsed, node: Node, at: Int, opts: Options) -> Parsed {
  var nodes = parsed.nodes
  nodes += [node]
  {nodes: nodes, root: nodes.len() - 1, at: at, opts: opts}
}

pure is_binary_operator(token: Str) -> Bool {
  token in ["-o","-or","-a","-and",","]
}

pure regex_flavor(kind: Str) -> Str {
  match kind {
    "emacs" | "findutils-default" => "emacs"
    "awk" | "gnu-awk" | "posix-awk" | "egrep" | "posix-egrep" | "posix-extended" => "extended"
    _ => "basic"
  }
}

# The expression that must match a whole path: the pattern grouped between
# anchors in the engine's own syntax.
pure whole_pattern(pattern: Str, flavor: Str) -> Str {
  match flavor {
    "emacs" => f"^({match_name.emacs_to_extended(pattern)})$"
    "extended" => f"^({pattern})$"
    _ => f"^\\({pattern}\\)$"
  }
}

pure snapshot(found: Stat) -> fmt.Meta {
  {kind: found.kind, size: found.size, uid: found.uid, gid: found.gid, ino: found.ino, dev: found.dev, nlink: found.nlink, mode: found.mode, atime_ns: found.atime_ns, mtime_ns: found.mtime_ns, ctime_ns: found.ctime_ns, blocks_512: found.blocks_512}
}

# A date argument as nanoseconds since the epoch, saturating for years the
# nanosecond range cannot hold so that far-future dates still compare.
proc time_reference(text: Str, start_ns: Int) [process, env, error, time] -> Result[Int, Error] {
  let base = dates.instant_from_ns(start_ns)
  match dates.parse_instant(text, base: base) {
    Ok(instant) => {
      if instant.seconds > 9223372035 { return Ok(9223372036854775807) }
      if instant.seconds < -9223372035 { return Ok(-9223372036854775807) }
      Ok(instant.seconds * 1000000000 + instant.nanoseconds)
    }
    Err(_) => { search.reject(f"I cannot figure out how to interpret {gnu.quote_value(text)} as a date or time")?; Ok(0) }
  }
}

# The metadata of a reference file named in an expression. Missing files are
# an error at parse time, before anything is printed.
proc stat_reference(name: Str, follow: Bool) [fs, process, env, error] -> Result[fmt.Meta, Error] {
  match fs.stat(fp"{name}", follow_symlinks: follow) {
    Ok(found) => Ok(snapshot(found))
    Err(failure) => { search.reject(f"{gnu.quote(name)}: {gnu.strerror(failure)}")?; Ok(snapshot(fs.stat(p".")?)) }
  }
}

# Create or truncate an output file when the expression is parsed, as GNU
# find does, so a run that matches nothing still leaves an empty file.
proc prepare_output(name: Str) [fs, process, env, error] -> Result[Unit, Error] {
  if name in ["/dev/stdout","/dev/stderr"] { return Ok() }
  if let Err(failure) = fp"{name}".write(b"") { search.reject(f"{gnu.quote(name)}: {gnu.strerror(failure)}")? }
  Ok()
}

proc parse_exec(args: List[Str], parsed: Parsed, token: Str, opts: Options) [process, env, error] -> Result[Parsed, Error] {
  var at = parsed.at + 1
  var argv: List[Str] = []
  var plus = false
  var found = false
  var braces = 0
  while at < args.len() {
    let word = args[at]
    at += 1
    if word == ";" { found = true; break }
    if word == "+" and token in ["-exec","-execdir"] and ! argv.is_empty() and argv[argv.len() - 1] == "{}" { found = true; plus = true; break }
    if word.find("{}") != null { braces += 1 }
    argv += [word]
  }
  if ! found { search.reject(f"missing argument to `{token}'")? }
  if argv.is_empty() { search.reject(f"invalid argument `;' to `{token}'")? }
  if plus and braces > 1 { search.reject("Only one instance of {} is supported with -exec ... +")? }
  var node = blank_node(f"{token.byte_slice(1)}{if plus { "+" } else { "" }}")
  node.argv = argv
  Ok(with_node(parsed, node, at, opts))
}

# Parse one test, action, option or parenthesised group.
proc primary(args: List[Str], parsed: Parsed) [fs, io, process, env, error, time] -> Result[Parsed, Error] {
  let start = parsed.at
  if start >= args.len() { search.reject("invalid expression")? }
  let token = args[start]
  var at = start + 1
  var opts = parsed.opts
  if token == "(" {
    if at < args.len() and args[at] == ")" { search.reject("invalid expression; empty parentheses are not allowed.")? }
    if at >= args.len() { search.reject("invalid expression; I was expecting to find a ')' somewhere but did not see one.")? }
    let inner = comma_expression(args, {nodes: parsed.nodes, root: -1, at: at, opts: opts})?
    if inner.at >= args.len() or args[inner.at] != ")" { search.reject("invalid expression; I was expecting to find a ')' somewhere but did not see one.")? }
    return Ok({nodes: inner.nodes, root: inner.root, at: inner.at + 1, opts: inner.opts})
  }
  if token in ["!","-not"] {
    if at < args.len() and args[at] == ")" { search.reject(f"expected an expression between '{token}' and ')'")? }
    if at >= args.len() { search.reject(f"expected an expression after '{token}'")? }
    let inner = primary(args, {nodes: parsed.nodes, root: -1, at: at, opts: opts})?
    var node = blank_node("not")
    node.left = inner.root
    return Ok(with_node(inner, node, inner.at, inner.opts))
  }
  if token == ")" { search.reject("invalid expression; I was expecting to find a ')' somewhere but did not see one.")? }
  if is_binary_operator(token) { search.reject(f"invalid expression; you have used a binary operator '{token}' with nothing before it.")? }
  if token == "--help" { gnu.help(HELP); exit 0 }
  if token == "--version" { gnu.version("find"); exit 0 }
  if token.starts_with("-newer") and token.byte_len() == 8 {
    let left = token.byte_slice(6, 1)
    let right = token.byte_slice(7, 1)
    if left not in ["a","B","c","m"] or right not in ["a","B","c","m","t"] { search.reject(f"invalid predicate `{token}'")? }
    if at >= args.len() { search.reject(f"missing argument to `{token}'")? }
    var node = blank_node("newerxy")
    node.text = f"{left}{right}"
    let reference = args[at]
    at += 1
    if right == "t" {
      node.number = time_reference(reference, opts.start_ns)?
    } else {
      let found = stat_reference(reference, opts.follow == 2)?
      node.number = match right { "a" => found.atime_ns; "c" => found.ctime_ns; _ => found.mtime_ns }
    }
    opts.last_token = token
    opts.last_test = token
    return Ok(with_node(parsed, node, at, opts))
  }
  if token not in NEEDS_ARGUMENT and token not in NO_ARGUMENT and token not in ["-exec","-execdir","-ok","-okdir","-fprintf"] {
    if token.starts_with("-") { search.reject(f"unknown predicate `{token}'")? }
    var message = f"paths must precede expression: `{token}'"
    if opts.last_token != "" { message = f"{message}\n{gnu.prog()}: possible unquoted pattern after predicate `{opts.last_token}'?" }
    search.reject(message)?
  }
  if token in ["-exec","-execdir","-ok","-okdir"] {
    opts.last_token = token
    opts.last_test = token
    return parse_exec(args, parsed, token, opts)
  }
  if token == "-fprintf" {
    if at >= args.len() { search.reject(f"missing argument to `{token}'")? }
    if at + 1 >= args.len() { search.reject(invalid_argument(args[at], token))? }
    var node = blank_node("fprintf")
    node.file = args[at]
    node.segments = fmt.compile(bytes.from_text(args[at + 1]))?
    prepare_output(node.file)?
    opts.last_token = token
    opts.last_test = token
    return Ok(with_node(parsed, node, at + 2, opts))
  }
  var value = ""
  if token in NEEDS_ARGUMENT {
    if at >= args.len() { search.reject(f"missing argument to `{token}'")? }
    value = args[at]
    at += 1
  }
  var node = blank_node(token.byte_slice(1))
  node.text = value
  match token {
    "-true" => node.kind = "true"
    "-false" => node.kind = "false"
    "-name" | "-iname" => {
      node.kind = "name"
      node.flag = token == "-iname"
      if opts.warn and value.find("/") != null {
        eprint f"{gnu.prog()}: warning: '{token}' matches against basenames only, but the given pattern contains a directory separator ('/'), thus the expression will evaluate to false all the time.  Did you mean '{if token == "-iname" { "-iwholename" } else { "-wholename" }}'?"
      }
    }
    "-path" | "-ipath" | "-wholename" | "-iwholename" => { node.kind = "path"; node.flag = token in ["-ipath","-iwholename"] }
    "-lname" | "-ilname" => { node.kind = "lname"; node.flag = token == "-ilname" }
    "-regex" | "-iregex" => {
      node.kind = "regex"
      node.flag = token == "-iregex"
      let flavor = regex_flavor(opts.regex_type)
      node.argv = [flavor]
      node.text = whole_pattern(value, flavor)
      if let Err(failure) = regex.find_bytes(node.text, b"", extended: flavor != "basic", ignore_case: node.flag) {
        let reason = if failure.message.find("Missing ')'") != null { "Unmatched ( or \\(" } else if failure.message.find("Missing ']'") != null { "Unmatched [, [^, [:, [., or [=" } else { "Invalid regular expression" }
        search.reject(f"failed to compile regular expression '{value}': {reason}")?
      }
    }
    "-regextype" => {
      let known = ["findutils-default","awk","ed","egrep","emacs","gnu-awk","grep","posix-awk","posix-basic","posix-egrep","posix-extended","posix-minimal-basic","sed"]
      if value not in known {
        search.reject(f"Unknown regular expression type '{value}'; valid types are 'findutils-default', 'awk', 'ed', 'egrep', 'emacs', 'gnu-awk', 'grep', 'posix-awk', 'posix-basic', 'posix-egrep', 'posix-extended', 'posix-minimal-basic', 'sed'.")?
      }
      opts.regex_type = value
      node.kind = "true"
    }
    "-type" | "-xtype" => {
      node.kind = token.byte_slice(1)
      var kinds: List[Str] = []
      var expect_letter = true
      for char in value {
        if expect_letter {
          let kind = match char { "f" => "file"; "d" => "dir"; "l" => "symlink"; "p" => "fifo"; "s" => "socket"; "b" => "block"; "c" => "char"; "D" => "door"; _ => "" }
          if kind == "" { search.reject(f"Unknown argument to {token}: {char}")? }
          if kind in kinds { search.reject(f"Duplicate file type '{char}' in the argument list to {token}.")? }
          kinds += [kind]
          expect_letter = false
        } else {
          if char != "," { search.reject(f"Must separate multiple arguments to {token} using: ','")? }
          expect_letter = true
        }
      }
      if expect_letter { search.reject(f"Last file type in list argument to {token} is missing, i.e., list is ending on: ','")? }
      node.argv = kinds
    }
    "-size" => {
      if value == "" { search.reject("invalid null argument to -size")? }
      var digits = value
      var unit = 512
      let last = value.byte_slice(value.byte_len() - 1)
      if last in ["b","c","w","k","M","G","T","P"] {
        unit = match last { "b" => 512; "c" => 1; "w" => 2; "k" => 1024; "M" => 1048576; "G" => 1073741824; "T" => 1099511627776; _ => 1125899906842624 }
        digits = value.byte_slice(0, value.byte_len() - 1)
      } else if (last.parse_int() ?? -1) < 0 { search.reject(f"invalid -size type `{last}'")? }
      let counted = signed_number(digits)
      if counted == null { search.reject(f"Invalid argument `{value}' to -size")? }
      node.compare = (counted ?? {compare: 0, number: 0}).compare
      node.number = (counted ?? {compare: 0, number: 0}).number
      node.bound = unit
    }
    "-mtime" | "-atime" | "-ctime" | "-mmin" | "-amin" | "-cmin" | "-used" => {
      let counted = signed_number(value)
      if counted == null { search.reject(invalid_argument(value, token))? }
      node.compare = (counted ?? {compare: 0, number: 0}).compare
      node.number = (counted ?? {compare: 0, number: 0}).number
      node.kind = "time"
      node.text = if token == "-used" { "used" } else { token.byte_slice(1, 1) }
      node.flag = token in ["-mmin","-amin","-cmin"]
      node.bound = if opts.day_start_ns != 0 { opts.day_start_ns } else { opts.start_ns }
    }
    "-newer" | "-anewer" | "-cnewer" => {
      let found = stat_reference(value, opts.follow == 2)?
      node.kind = "newerxy"
      node.text = match token { "-newer" => "mm"; "-anewer" => "am"; _ => "cm" }
      node.number = found.mtime_ns
    }
    "-perm" => {
      var spec = value
      var mode_kind = "exact"
      if spec.starts_with("-") { mode_kind = "all"; spec = spec.byte_slice(1) } else if spec.starts_with("/") { mode_kind = "any"; spec = spec.byte_slice(1) }
      let file_mode = permission_mode(spec, false)
      let dir_mode = permission_mode(spec, true)
      if file_mode == null or dir_mode == null { search.reject(f"invalid file mode '{value}'")? }
      node.text = mode_kind
      node.number = file_mode ?? 0
      node.bound = dir_mode ?? 0
      if mode_kind == "any" and node.number == 0 and node.bound == 0 {
        eprint f"{gnu.prog()}: warning: you have specified a mode pattern {value} (which is equivalent to /000). The meaning of -perm /000 was changed in 2007 to be consistent with -perm -000; that is, while it used to match no files, it now matches all files. This warning message will be removed in a future findutils release."
      }
    }
    "-uid" | "-gid" | "-links" | "-inum" => {
      let counted = signed_number(value)
      if counted == null { search.reject(f"non-numeric argument to {token}: '{value}'")? }
      node.kind = "count"
      node.text = token.byte_slice(1)
      node.compare = (counted ?? {compare: 0, number: 0}).compare
      node.number = (counted ?? {compare: 0, number: 0}).number
    }
    "-user" => {
      node.kind = "count"
      node.text = "uid"
      let numeric = plain_number(value)
      if numeric != null { node.number = numeric ?? 0 } else if all_digits(value) {
        search.reject(f"invalid user name or UID argument to -user: '{value}'")?
      } else if let Ok(account) = user.lookup(value) { node.number = account.uid } else {
        search.reject(f"invalid user name or UID argument to -user: '{value}'")?
      }
    }
    "-group" => {
      node.kind = "count"
      node.text = "gid"
      let numeric = plain_number(value)
      if numeric != null { node.number = numeric ?? 0 } else if all_digits(value) {
        search.reject(f"invalid group name or GID argument to -group: '{value}'")?
      } else if let Ok(account) = group.lookup(value) { node.number = account.gid } else {
        search.reject(f"invalid group name or GID argument to -group: '{value}'")?
      }
    }
    "-samefile" => {
      let found = stat_reference(value, opts.follow == 2)?
      node.number = found.ino
      node.bound = found.dev
    }
    "-fstype" => node.kind = "fstype"
    "-printf" => node.segments = fmt.compile(bytes.from_text(value))?
    "-fprint" | "-fprint0" | "-fls" => { node.kind = token.byte_slice(1); node.file = value; prepare_output(value)? }
    "-depth" | "-d" => { opts.depth = true; node.kind = "true" }
    "-mount" | "-xdev" => { opts.device = true; node.kind = "true" }
    "-noleaf" | "-ignore_readdir_race" | "-noignore_readdir_race" => node.kind = "true"
    "-follow" => { opts.follow = 2; node.kind = "true" }
    "-warn" => { opts.warn = true; node.kind = "true" }
    "-nowarn" => { opts.warn = false; node.kind = "true" }
    "-daystart" => {
      let fields = time.to_calendar(opts.start_ns, false)?
      opts.day_start_ns = time.from_calendar(fields.year, fields.month, fields.day + 1, 0, 0, 0, utc: false, normalize: true)?
      node.kind = "true"
    }
    "-maxdepth" | "-mindepth" => {
      let number = plain_number(value)
      if number == null { search.reject(f"Expected a positive decimal integer argument to {token}, but got '{value}'")? }
      if token == "-maxdepth" { opts.maximum = number ?? -1 } else { opts.minimum = number ?? 0 }
      node.kind = "true"
    }
    _ => {}
  }
  if opts.warn and token in GLOBAL_OPTIONS and opts.last_test != "" {
    eprint f"{gnu.prog()}: warning: you have specified the global option {token} after the argument {opts.last_test}, but global options are not positional, i.e., {token} affects tests specified before it as well as those specified after it.  Please specify global options before other arguments."
  }
  opts.last_token = token
  if token not in GLOBAL_OPTIONS and token not in POSITIONAL_OPTIONS { opts.last_test = token }
  Ok(with_node(parsed, node, at, opts))
}

# A command-line word begins the expression when it is an option such as
# -name, or a lone `(` or `!`; a lone `-`, `)` and `,` are file names.
pure starts_expression(word: Str) -> Bool {
  (word.starts_with("-") and word != "-") or word in ["(", "!"]
}

pure invalid_argument(value: Str, option: Str) -> Str {
  f"invalid argument `{value}' to `{option}'"
}

# Explicit and implicit conjunction share one precedence level above -o,
# which binds tighter than the comma operator.
proc and_expression(args: List[Str], parsed: Parsed) [fs, io, process, env, error, time] -> Result[Parsed, Error] {
  var current = primary(args, parsed)?
  while current.at < args.len() and args[current.at] != ")" and args[current.at] not in ["-o","-or",","] {
    var at = current.at
    if args[at] in ["-a","-and"] {
      at += 1
      if at < args.len() and args[at] == ")" { search.reject(f"expected an expression between '{args[current.at]}' and ')'")? }
      if at >= args.len() { search.reject(f"expected an expression after '{args[current.at]}'")? }
    }
    let right = primary(args, {nodes: current.nodes, root: -1, at: at, opts: current.opts})?
    var node = blank_node("and")
    node.left = current.root
    node.right = right.root
    current = with_node(right, node, right.at, right.opts)
  }
  Ok(current)
}

proc or_expression(args: List[Str], parsed: Parsed) [fs, io, process, env, error, time] -> Result[Parsed, Error] {
  var current = and_expression(args, parsed)?
  while current.at < args.len() and args[current.at] in ["-o","-or"] {
    let operator = args[current.at]
    let at = current.at + 1
    if at < args.len() and args[at] == ")" { search.reject(f"expected an expression between '{operator}' and ')'")? }
    if at >= args.len() { search.reject(f"expected an expression after '{operator}'")? }
    let right = and_expression(args, {nodes: current.nodes, root: -1, at: at, opts: current.opts})?
    var node = blank_node("or")
    node.left = current.root
    node.right = right.root
    current = with_node(right, node, right.at, right.opts)
  }
  Ok(current)
}

proc comma_expression(args: List[Str], parsed: Parsed) [fs, io, process, env, error, time] -> Result[Parsed, Error] {
  var current = or_expression(args, parsed)?
  while current.at < args.len() and args[current.at] == "," {
    let at = current.at + 1
    if at < args.len() and args[at] == ")" { search.reject("expected an expression between ',' and ')'")? }
    if at >= args.len() { search.reject("expected an expression after ','")? }
    let right = or_expression(args, {nodes: current.nodes, root: -1, at: at, opts: current.opts})?
    var node = blank_node("comma")
    node.left = current.root
    node.right = right.root
    current = with_node(right, node, right.at, right.opts)
  }
  Ok(current)
}

# One time test. Days count whole 24-hour periods back from the origin, so an
# age of 3.5 days is "3"; minutes round up, so 5.5 minutes ago is "6". `-used`
# compares the access time with the status-change time.
pure time_test(node: Node, meta: fmt.Meta) -> Bool {
  if node.text == "used" {
    return compare_number((meta.atime_ns - meta.ctime_ns) / DAY_NS, node.compare, node.number)
  }
  let stamp = if node.text == "a" { meta.atime_ns } else if node.text == "c" { meta.ctime_ns } else { meta.mtime_ns }
  let age = node.bound - stamp
  if node.flag {
    if node.compare > 0 { return age > node.number * MINUTE_NS }
    if node.compare < 0 { return age < node.number * MINUTE_NS }
    return age > (node.number - 1) * MINUTE_NS and age <= node.number * MINUTE_NS
  }
  if node.compare > 0 { return age > (node.number + 1) * DAY_NS }
  if node.compare < 0 { return age < node.number * DAY_NS }
  age > node.number * DAY_NS and age <= (node.number + 1) * DAY_NS
}

pure replace_marker(word: Bytes, value: Bytes) -> Bytes {
  var out: List[Bytes] = []
  var at = 0
  while at < word.len() {
    let found = search.find_fixed(word, b"{}", offset: at)
    if found == null { out += [word[at..]]; break }
    let offset = found ?? word.len()
    out += [word[at..offset], value]
    at = offset + 2
  }
  bytes.concat(out)
}

# Output goes to the process streams for the /dev/stdout and /dev/stderr
# names, and is appended to the file otherwise, which several actions may
# share.
proc write_output(name: Str, data: Bytes) [fs, io, process, env, error] -> Result[Unit, Error] {
  if name == "/dev/stdout" { gnu.write_bytes(data); return Ok() }
  if name == "/dev/stderr" { io.write_stderr(data.utf8() ?? "")?; io.flush_stderr()?; return Ok() }
  let target = fp"{name}"
  let size = match fs.stat(target) { Ok(found) => found.size; Err(_) => 0 }
  let _ = bytes.write_at(target, size, data, create: true)?
  Ok()
}

# Why a command could not be started, in the words of the exec failure: a
# missing file, or one that exists but cannot be run.
proc start_failure(command: Path) [fs, process] -> Str {
  let raw = command.bytes()
  var has_slash = false
  for at in range(raw.len()) { if raw.byte_at(at) == 47 { has_slash = true } }
  if has_slash {
    return match fs.stat(command) { Ok(_) => "Permission denied"; Err(failure) => gnu.strerror(failure) }
  }
  if process.which(command.display()) is Err(_) { "No such file or directory" } else { "Permission denied" }
}

# Run a command and report whether it exited 0. A command that cannot be
# started is reported by find, as the forked child does in GNU find, and is a
# false result rather than a failure of the whole run.
proc command_succeeds(words: List[Path], directory: Path?) [fs, process, env, error] -> Result[Bool, Error] {
  let command = words[0]
  let plan = if directory != null { process.command_argv(command, words, directory ?? p".") } else { process.command_argv(command, words) }
  let status = process.run(plan)?
  if status.signaled() {
    gnu.error(f"{gnu.quote_bytes(command.bytes())} terminated by signal {status.signal_number() ?? 0}")
    return Ok(false)
  }
  if ! status.exited() {
    gnu.error(f"{gnu.quote_bytes(command.bytes())}: {start_failure(command)}")
    return Ok(false)
  }
  Ok(status.exited_with(0))
}

proc dot_name(raw: Bytes) [error] -> Result[Path, Error] {
  Ok(Path.parse_bytes(bytes.concat([b"./", search.basename_bytes(raw)]))?)
}

proc evaluate(target: Path, meta: fmt.Meta, depth: Int, start: Path, plan: Run, index: Int) [fs, io, process, env, error, time] -> Result[Decision, Error] {
  let node = plan.nodes[index]
  if node.kind in ["and","or","comma","not"] {
    let lhs = evaluate(target, meta, depth, start, plan, node.left)?
    if node.kind == "not" { return Ok({matched: ! lhs.matched, prune: lhs.prune, quit: lhs.quit, failed: lhs.failed, deferred: lhs.deferred}) }
    if lhs.quit or (node.kind == "and" and ! lhs.matched) or (node.kind == "or" and lhs.matched) { return Ok(lhs) }
    let rhs = evaluate(target, meta, depth, start, plan, node.right)?
    return Ok({matched: rhs.matched, prune: lhs.prune or rhs.prune, quit: lhs.quit or rhs.quit, failed: lhs.failed or rhs.failed, deferred: lhs.deferred + rhs.deferred})
  }
  var matched = true
  var prune = false
  var quit = false
  var failed = false
  var deferred: List[Deferred] = []
  let raw = target.bytes()
  let entry: fmt.Entry = {path: target, start: start, depth: depth, meta: meta}
  match node.kind {
    "true" => matched = true
    "false" => matched = false
    "name" => matched = match_name.glob(node.text, match_name.basename(raw), insensitive: node.flag)
    "path" => matched = match_name.glob(node.text, raw, insensitive: node.flag)
    "lname" => {
      matched = false
      if meta.kind == "symlink" {
        if let Ok(link) = target.readlink() { matched = match_name.glob(node.text, link.bytes(), insensitive: node.flag) }
      }
    }
    "type" => matched = meta.kind in node.argv
    "xtype" => {
      var kind = meta.kind
      let followed = plan.follow == 2 or (plan.follow == 1 and depth == 0)
      if ! followed and meta.kind == "symlink" {
        kind = match fs.stat(target, follow_symlinks: true) { Ok(found) => found.kind; Err(_) => "symlink" }
      } else if followed {
        kind = match fs.stat(target, follow_symlinks: false) { Ok(found) => found.kind; Err(_) => meta.kind }
      }
      matched = kind in node.argv
    }
    "count" => {
      let actual = match node.text { "uid" => meta.uid; "gid" => meta.gid; "inum" => meta.ino; _ => meta.nlink }
      matched = compare_number(actual, node.compare, node.number)
    }
    "size" => matched = compare_number((meta.size + node.bound - 1) / node.bound, node.compare, node.number)
    "time" => matched = time_test(node, meta)
    "newerxy" => {
      let stamp = match node.text.byte_slice(0, 1) { "a" => meta.atime_ns; "c" => meta.ctime_ns; _ => meta.mtime_ns }
      matched = stamp > node.number
    }
    "perm" => {
      let mode = if meta.kind == "dir" { node.bound } else { node.number }
      let bits = meta.mode.bit_and(0o7777)
      matched = if node.text == "all" { bits.bit_and(mode) == mode } else if node.text == "any" { mode == 0 or bits.bit_and(mode) != 0 } else { bits == mode }
    }
    "regex" => {
      matched = false
      let spans = regex.find_bytes(node.text, raw, extended: node.argv != ["basic"], ignore_case: node.flag)?
      for span in spans { if span.start == 0 and span.end == raw.len() { matched = true } }
    }
    "empty" => {
      matched = false
      if meta.kind == "dir" {
        match fs.children(target, stat: false) {
          Ok(listing) => matched = listing.collect().is_empty()
          Err(failure) => { gnu.error(f"{gnu.quote_bytes(raw)}: {gnu.strerror(failure)}"); failed = true }
        }
      } else { matched = meta.kind == "file" and meta.size == 0 }
    }
    "readable" => matched = fs.access(target, read: true) ?? false
    "writable" => matched = fs.access(target, write: true) ?? false
    "executable" => matched = fs.access(target, execute: true) ?? false
    "nouser" => matched = user.by_uid(meta.uid) is Err(_)
    "nogroup" => matched = group.by_gid(meta.gid) is Err(_)
    "samefile" => matched = meta.ino == node.number and meta.dev == node.bound
    "fstype" => matched = match fs.mount_for(target) { Ok(found) => found.fstype == node.text; Err(_) => false }
    "print" | "print0" => gnu.write_bytes(bytes.concat([raw, if node.kind == "print0" { b"\0" } else { b"\n" }]))
    "printf" => gnu.write_bytes(fmt.render(node.segments, entry)?)
    "fprint" | "fprint0" => write_output(node.file, bytes.concat([raw, if node.kind == "fprint0" { b"\0" } else { b"\n" }]))?
    "fprintf" => write_output(node.file, fmt.render(node.segments, entry)?)?
    "ls" => gnu.write_bytes(fmt.ls_line(entry, plan.opts.start_ns)?)
    "fls" => write_output(node.file, fmt.ls_line(entry, plan.opts.start_ns)?)?
    "prune" => prune = ! plan.opts.depth
    "quit" => quit = true
    "delete" => {
      let removal = if meta.kind == "dir" { target.remove_dir() } else { target.unlink() }
      if let Err(failure) = removal {
        gnu.error(f"cannot delete {gnu.quote_bytes(raw)}: {gnu.strerror(failure)}")
        failed = true
        matched = false
      }
    }
    "exec+" | "execdir+" => {
      let in_directory = node.kind == "execdir+"
      let name = if in_directory { dot_name(raw)?.display() } else { target.display() }
      deferred = [{node: index, name: name, directory: if in_directory { target.parent() } else { p"." }}]
    }
    "exec" | "execdir" | "ok" | "okdir" => {
      let in_directory = node.kind in ["execdir","okdir"]
      let value = if in_directory { dot_name(raw)?.bytes() } else { raw }
      var words: List[Path] = []
      for word in node.argv { words += [Path.parse_bytes(replace_marker(bytes.from_text(word), value))?] }
      var proceed = true
      if node.kind in ["ok","okdir"] {
        io.write_stderr(f"< {node.argv[0]} ... {raw.utf8() ?? gnu.quote_bytes(raw)} > ? ")?
        io.flush_stderr()?
        let answer = io.stdin_line() ?? ""
        proceed = answer.starts_with("y") or answer.starts_with("Y")
      }
      matched = false
      if proceed {
        io.flush_stdout()?
        let directory: Path? = if in_directory { target.parent() } else { null }
        matched = command_succeeds(words, directory)?
      }
    }
    _ => search.reject(f"unsupported predicate {node.kind}")?
  }
  Ok({matched: matched, prune: prune, quit: quit, failed: failed, deferred: deferred})
}

# Metadata of an entry as the traversal sees it. Following symlinks falls back
# to the link itself when its target is missing, as fts does, so a dangling
# link is still listed; other errors (a link loop) are reported.
proc stat_entry(target: Path, depth: Int, follow: Int) [fs, error] -> Result[fmt.Meta, Error] {
  let following = follow == 2 or (follow == 1 and depth == 0)
  if ! following { return Ok(snapshot(fs.stat(target, follow_symlinks: false)?)) }
  match fs.stat(target, follow_symlinks: true) {
    Ok(found) => Ok(snapshot(found))
    Err(failure) => {
      if gnu.errno(failure) == 2 { return Ok(snapshot(fs.stat(target, follow_symlinks: false)?)) }
      Err(failure)
    }
  }
}

proc visit(target: Path, depth: Int, hint: Str, ancestors: List[Ancestor], device: Int?, start: Path, plan: Run) [fs, io, process, env, error, time] -> Result[Visit, Error] {
  var failed = false
  var deferred: List[Deferred] = []
  let opts = plan.opts
  # Below the starting points the type from the directory listing is enough
  # unless some test reads more, which keeps names listable in a directory
  # that cannot be searched, as GNU find does.
  let known = hint in ["file","dir","symlink","fifo","socket","block","char"]
  let lazy = depth > 0 and ! plan.stat_needed and known and (plan.follow != 2 or (hint != "symlink" and hint != "dir"))
  let meta = if lazy { {kind: hint, size: 0, uid: 0, gid: 0, ino: 0, dev: 0, nlink: 0, mode: 0, atime_ns: 0, mtime_ns: 0, ctime_ns: 0, blocks_512: 0} } else { stat_entry(target, depth, plan.follow)? }
  let identity = f"{meta.dev}:{meta.ino}"
  if meta.kind == "dir" and ! lazy {
    for ancestor in ancestors {
      if ancestor.identity == identity {
        gnu.error(f"File system loop detected; the following directory is part of the cycle: {gnu.quote_bytes(target.bytes())}")
        return Ok({quit: false, failed: true, deferred: []})
      }
    }
  }
  var decision: Decision = {matched: false, prune: false, quit: false, failed: false, deferred: []}
  if ! opts.depth and depth >= opts.minimum {
    decision = evaluate(target, meta, depth, start, plan, plan.root)?
    deferred += decision.deferred
    failed = failed or decision.failed
    if decision.quit { return Ok({quit: true, failed: failed, deferred: deferred}) }
  }
  var descend = meta.kind == "dir" and ! decision.prune and (opts.maximum < 0 or depth < opts.maximum)
  if descend and opts.device and device != null and meta.dev != device { descend = false }
  if descend {
    match fs.children(target, stat: false) {
      Ok(entries) => {
        for entry in entries {
          let leaf = search.child(target, search.basename_bytes(entry.path.bytes()))?
          let below = visit(leaf, depth + 1, entry.kind, ancestors + [{identity: identity, path: target}], device ?? meta.dev, start, plan)
          match below {
            Ok(result) => { failed = failed or result.failed; deferred += result.deferred; if result.quit { return Ok({quit: true, failed: failed, deferred: deferred}) } }
            Err(failure) => { gnu.error(f"{gnu.quote_bytes(leaf.bytes())}: {gnu.strerror(failure)}"); failed = true }
          }
        }
      }
      Err(failure) => { gnu.error(f"{gnu.quote_bytes(target.bytes())}: {gnu.strerror(failure)}"); failed = true }
    }
  }
  if opts.depth and depth >= opts.minimum {
    decision = evaluate(target, meta, depth, start, plan, plan.root)?
    deferred += decision.deferred
    failed = failed or decision.failed
  }
  Ok({quit: decision.quit, failed: failed, deferred: deferred})
}

# Run the deferred `-exec ... {} +` command lines: one batch per -execdir
# directory, split where the argument bytes would pass the line limit.
proc run_batches(plan: Run, deferred: List[Deferred], kinds: List[Str]) [fs, io, process, env, error] -> Result[Bool, Error] {
  var failed = false
  for index in range(plan.nodes.len()) {
    let node = plan.nodes[index]
    if node.kind not in kinds { continue }
    var initial: List[Path] = []
    var initial_size = 0
    for word in node.argv[..node.argv.len() - 1] { initial += [fp"{word}"]; initial_size += word.byte_len() + 1 }
    var batch: List[Path] = []
    var directory: Path? = null
    var size = 0
    for item in deferred {
      if item.node != index { continue }
      let cwd: Path? = if node.kind == "execdir+" { item.directory } else { null }
      let name = fp"{item.name}"
      let needed = name.bytes().len() + 1
      if ! batch.is_empty() and (cwd != directory or initial_size + size + needed > BATCH_LIMIT) {
        io.flush_stdout()?
        if ! command_succeeds(initial + batch, directory)? { failed = true }
        batch = []
        size = 0
      }
      directory = cwd
      batch += [name]
      size += needed
    }
    if ! batch.is_empty() {
      io.flush_stdout()?
      if ! command_succeeds(initial + batch, directory)? { failed = true }
    }
  }
  Ok(failed)
}

proc main(...args: List[Str]) {
  var follow = 0
  var at = 0
  let start_ns = time.now() * 1000000
  while at < args.len() {
    let token = args[at]
    if token in ["-H","-L","-P"] { follow = if token == "-L" { 2 } else if token == "-H" { 1 } else { 0 }; at += 1; continue }
    if token == "-D" {
      if at + 1 >= args.len() { eprint f"{gnu.prog()}: Missing argument after the -D option.\nTry 'find --help' for more information."; exit 1 }
      for name in args[at + 1].split(",") {
        if name in ["exec","opt","rates","search","stat","time","tree","all","help"] { gnu.error(f"debug option {name} is not supported"); exit 1 }
        gnu.error(f"Ignoring unrecognised debug flag {gnu.quote_value(name)}")
      }
      at += 2
      continue
    }
    if token.starts_with("-O") {
      let digits = token.byte_slice(2)
      if digits == "" { gnu.error("The -O option must be immediately followed by a decimal integer"); exit 1 }
      if plain_number(digits) == null { gnu.error("Please specify a decimal number immediately after -O"); exit 1 }
      at += 1
      continue
    }
    if token == "--" { at += 1; break }
    break
  }
  var roots: List[Path] = []
  var last_token = ""
  var read_failed = false
  if at < args.len() and args[at] == "-files0-from" {
    if at + 1 >= args.len() { gnu.error("missing argument to `-files0-from'"); exit 1 }
    let source = args[at + 1]
    at += 2
    last_token = "-files0-from"
    let listed = if source == "-" { io.stdin_bytes() } else { fp"{source}".read_bytes() }
    if let Err(failure) = listed { gnu.cannot_open(source, failure); exit 1 }
    let label = if source == "-" { "(standard input)" } else { source }
    var number = 0
    for name in search.records(listed ?? b"", 0) {
      number += 1
      if name.is_empty() { gnu.error(f"{gnu.quote(label)}:{number}: invalid zero-length file name"); read_failed = true; continue }
      match Path.parse_bytes(name) { Ok(entry) => roots += [entry]; Err(failure) => { gnu.error(failure.message); exit 1 } }
    }
    if roots.is_empty() { exit if read_failed { 1 } else { 0 } }
  } else {
    while at < args.len() and ! starts_expression(args[at]) { roots += [fp"{args[at]}"]; at += 1 }
    if roots.is_empty() { roots = [p"."] }
  }
  var expression_args: List[Str] = args[at..]
  if expression_args.is_empty() { expression_args = ["-true"] }
  let initial: Options = {follow: follow, minimum: 0, maximum: -1, depth: false, device: false, warn: false, start_ns: start_ns, day_start_ns: 0, regex_type: "emacs", last_token: last_token, last_test: ""}
  let outcome = try {
    let parsed = comma_expression(expression_args, {nodes: [], root: -1, at: 0, opts: initial})?
    if parsed.at < expression_args.len() { search.reject("you have too many ')'")? }
    parsed
  }
  if let Err(failure) = outcome { gnu.error(failure.message); exit 1 }
  let parsed = outcome ?? {nodes: [], root: -1, at: 0, opts: initial}
  var nodes = parsed.nodes
  var root = parsed.root
  var final_opts = parsed.opts
  var has_action = false
  var deleting = false
  var pruning = false
  for node in nodes {
    if node.kind in ACTIONS { has_action = true }
    if node.kind == "delete" { deleting = true }
    if node.kind == "prune" { pruning = true }
  }
  if deleting {
    if pruning and ! final_opts.depth {
      gnu.error("The -delete action automatically turns on -depth, but -prune does nothing when -depth is in effect.  If you want to carry on anyway, just explicitly use the -depth option.")
      exit 1
    }
    final_opts.depth = true
  }
  if ! has_action {
    nodes += [blank_node("print")]
    var join = blank_node("and")
    join.left = root
    join.right = nodes.len() - 1
    nodes += [join]
    root = nodes.len() - 1
  }
  var stat_needed = final_opts.device
  for node in nodes {
    if node.kind in ["count","size","time","newerxy","perm","empty","samefile","nouser","nogroup","fstype","ls","fls"] { stat_needed = true }
    if node.kind in ["printf","fprintf"] and fmt.needs_stat(node.segments) { stat_needed = true }
  }
  let plan: Run = {nodes: nodes, root: root, follow: final_opts.follow, opts: final_opts, stat_needed: stat_needed}
  var failed = read_failed
  var deferred: List[Deferred] = []
  for target in roots {
    var quit = false
    match visit(target, 0, "", [], null, target, plan) {
      Ok(result) => { failed = failed or result.failed; deferred += result.deferred; quit = result.quit }
      Err(failure) => { gnu.error(f"{gnu.quote_bytes(target.bytes())}: {gnu.strerror(failure)}"); failed = true }
    }
    # -execdir lines never span starting points; -exec lines do.
    match run_batches(plan, deferred, ["execdir+"]) {
      Ok(any_failed) => { failed = failed or any_failed }
      Err(failure) => { gnu.error(failure.message); exit 1 }
    }
    var kept: List[Deferred] = []
    for item in deferred { if plan.nodes[item.node].kind != "execdir+" { kept += [item] } }
    deferred = kept
    if quit { break }
  }
  match run_batches(plan, deferred, ["exec+"]) {
    Ok(any_failed) => { failed = failed or any_failed }
    Err(failure) => { gnu.error(failure.message); exit 1 }
  }
  if let Err(failure) = io.flush_stdout() { gnu.error(failure.message); exit 1 }
  exit if failed { 1 } else { 0 }
}


# The text of --help.
const HELP = r"""
  Usage: find [-H] [-L] [-P] [-Olevel] [-D debugopts] [path...] [expression]

  Default path is the current directory; default expression is -print.
  Expression may consist of: operators, options, tests, and actions.

  Operators (decreasing precedence; -and is implicit where no others are given):
        ( EXPR )   ! EXPR   -not EXPR   EXPR1 -a EXPR2   EXPR1 -and EXPR2
        EXPR1 -o EXPR2   EXPR1 -or EXPR2   EXPR1 , EXPR2

  Positional options (always true):
        -daystart -follow -nowarn -regextype -warn

  Normal options (always true, specified before other expressions):
        -depth -files0-from FILE -maxdepth LEVELS -mindepth LEVELS
        -mount -noleaf -xdev -ignore_readdir_race -noignore_readdir_race

  Tests (N can be +N or -N or N):
        -amin N -anewer FILE -atime N -cmin N -cnewer FILE -context CONTEXT
        -ctime N -empty -false -fstype TYPE -gid N -group NAME -ilname PATTERN
        -iname PATTERN -inum N -iwholename PATTERN -iregex PATTERN
        -links N -lname PATTERN -mmin N -mtime N -name PATTERN -newer FILE
        -newerXY REFERENCE -nouser -nogroup -path PATTERN -perm [-/]MODE
        -regex PATTERN -readable -writable -executable
        -wholename PATTERN -size N[bcwkMG] -true -type [bcdpflsD] -uid N
        -used N -user NAME -xtype [bcdpfls]

  Actions:
        -delete -print0 -printf FORMAT -fprintf FILE FORMAT -print 
        -fprint0 FILE -fprint FILE -ls -fls FILE -prune -quit
        -exec COMMAND ; -exec COMMAND {} + -ok COMMAND ;
        -execdir COMMAND ; -execdir COMMAND {} + -okdir COMMAND ;

  Other common options:
        --help                   display this help and exit
        --version                output version information and exit


  In -newerXY, XY stands for the combination [aBcm][aBcmt]; see find(1).

  Valid arguments for -D:
        exec, opt, rates, search, stat, time, tree, all, help
  Use '-D help' for a description of the options, or see find(1).

  Please see also the documentation at https://www.gnu.org/software/findutils/.
  You can report (and track progress on fixing) bugs in the "find"
  program via the GNU findutils bug-reporting page at
  https://savannah.gnu.org/bugs/?group=findutils or, if
  you have no web access, by sending email to <bug-findutils@gnu.org>.
  """
