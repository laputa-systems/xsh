#!/bin/xsh
use lib.gnu

const USAGE = """Usage: {prog} [OPTION]... [FILE]...
List information about the FILEs (the current directory by default).
Sort entries alphabetically if none of -cftuvSUX nor --sort is specified.

Mandatory arguments to long options are mandatory for short options too.
  -a, --all                  do not ignore entries starting with .
  -A, --almost-all           do not list implied . and ..
      --author               with -l, print the author of each file
  -b, --escape               print C-style escapes for nongraphic characters
      --block-size=SIZE      with -l, scale sizes by SIZE when printing them;
                               e.g., '--block-size=M'; see SIZE format below
  -B, --ignore-backups       do not list implied entries ending with ~
  -c                         with -lt: sort by, and show, ctime (time of last
                               change of file status information);
                               with -l: show ctime and sort by name;
                               otherwise: sort by ctime, newest first
  -C                         list entries by columns
      --color[=WHEN]         color the output WHEN; more info below
  -d, --directory            list directories themselves, not their contents
  -D, --dired                generate output designed for Emacs' dired mode
  -f                         list all entries in directory order
  -F, --classify[=WHEN]      append indicator (one of */=>@|) to entries WHEN
      --file-type            likewise, except do not append '*'
      --format=WORD          across -x, commas -m, horizontal -x, long -l,
                               single-column -1, verbose -l, vertical -C
      --full-time            like -l --time-style=full-iso
  -g                         like -l, but do not list owner
      --group-directories-first
                             group directories before files;
                               can be augmented with a --sort option, but any
                               use of --sort=none (-U) disables grouping
  -G, --no-group             in a long listing, don't print group names
  -h, --human-readable       with -l and -s, print sizes like 1K 234M 2G etc.
      --si                   likewise, but use powers of 1000 not 1024
  -H, --dereference-command-line
                             follow symbolic links listed on the command line
      --dereference-command-line-symlink-to-dir
                             follow each command line symbolic link
                               that points to a directory
      --hide=PATTERN         do not list implied entries matching shell PATTERN
                               (overridden by -a or -A)
      --hyperlink[=WHEN]     hyperlink file names WHEN
      --indicator-style=WORD
                             append indicator with style WORD to entry names:
                               none (default), slash (-p),
                               file-type (--file-type), classify (-F)
  -i, --inode                print the index number of each file
  -I, --ignore=PATTERN       do not list implied entries matching shell PATTERN
  -k, --kibibytes            default to 1024-byte blocks for file system usage;
                               used only with -s and per directory totals
  -l                         use a long listing format
  -L, --dereference          when showing file information for a symbolic
                               link, show information for the file the link
                               references rather than for the link itself
  -m                         fill width with a comma separated list of entries
  -n, --numeric-uid-gid      like -l, but list numeric user and group IDs
  -N, --literal              print entry names without quoting
  -o                         like -l, but do not list group information
  -p, --indicator-style=slash
                             append / indicator to directories
  -q, --hide-control-chars   print ? instead of nongraphic characters
      --show-control-chars   show nongraphic characters as-is (the default,
                               unless program is 'ls' and output is a terminal)
  -Q, --quote-name           enclose entry names in double quotes
      --quoting-style=WORD   use quoting style WORD for entry names:
                               literal, locale, shell, shell-always,
                               shell-escape, shell-escape-always, c, escape
                               (overrides QUOTING_STYLE environment variable)
  -r, --reverse              reverse order while sorting
  -R, --recursive            list subdirectories recursively
  -s, --size                 print the allocated size of each file, in blocks
  -S                         sort by file size, largest first
      --sort=WORD            sort by WORD instead of name: none (-U), size (-S),
                               time (-t), version (-v), extension (-X), width
  -t                         sort by time, newest first; see --time
      --time=WORD            change the default of using modification times;
                               access time (-u): atime, access, use;
                               change time (-c): ctime, status;
                               birth time: birth, creation;
                             with -l, WORD determines which time to show;
                             with --sort=time, sort by WORD (newest first)
      --time-style=TIME_STYLE
                             time/date format with -l; see TIME_STYLE below
  -T, --tabsize=COLS         assume tab stops at each COLS instead of 8
  -u                         with -lt: sort by, and show, access time;
                               with -l: show access time and sort by name;
                               otherwise: sort by access time, newest first
  -U                         do not sort; list entries in directory order
  -v                         natural sort of (version) numbers within text
  -w, --width=COLS           set output width to COLS.  0 means no limit
  -x                         list entries by lines instead of by columns
  -X                         sort alphabetically by entry extension
  -Z, --context              print any security context of each file
      --zero                 end each output line with NUL, not newline
  -1                         list one file per line
      --help        display this help and exit
      --version     output version information and exit

The SIZE argument is an integer and optional unit (example: 10K is 10*1024).
Units are K,M,G,T,P,E,Z,Y,R,Q (powers of 1024) or KB,MB,... (powers of 1000).
Binary prefixes can be used, too: KiB=K, MiB=M, and so on.

The TIME_STYLE argument can be full-iso, long-iso, iso, locale, or +FORMAT.
FORMAT is interpreted like in date(1).  If FORMAT is FORMAT1<newline>FORMAT2,
then FORMAT1 applies to non-recent files and FORMAT2 to recent files.
TIME_STYLE prefixed with 'posix-' takes effect only outside the POSIX locale.
Also the TIME_STYLE environment variable sets the default style to use.

The WHEN argument defaults to 'always' and can also be 'auto' or 'never'.

Using color to distinguish file types is disabled both by default and
with --color=never.  With --color=auto, ls emits color codes only when
standard output is connected to a terminal.  The LS_COLORS environment
variable can change the settings.  Use the dircolors(1) command to set it.

Exit status:
 0  if OK,
 1  if minor problems (e.g., cannot access subdirectory),
 2  if serious trouble (e.g., cannot access command-line argument).
"""

# The full set of settings the options and environment decide. Fields that
# start null are "not given": the last option wins, and the default depends on
# whether stdout is a terminal.
type Cfg = {
  format: Str,
  format_set: Bool,
  ignore_mode: Str,
  sort: Str?,
  reverse: Bool,
  time: Str,
  time_given: Bool,
  recursive: Bool,
  directory: Bool,
  inode: Bool,
  size: Bool,
  owner: Bool,
  group: Bool,
  author: Bool,
  numeric: Bool,
  context: Bool,
  deref: Str?,
  indicator: Str,
  quoting: Str?,
  hide_control: Bool,
  color: Str?,
  hyperlink: Str,
  dired: Bool,
  zero: Bool,
  width: Int?,
  tabsize: Int?,
  human: Str,
  block_size: Int?,
  file_human: Str,
  file_block_size: Int?,
  kibibytes: Bool,
  ignores: List[Str],
  hides: List[Str],
  dirs_first: Bool,
  time_style: Str?,
}

# arg: 0 none, 1 required, 2 optional (long options only).
type Opt = {short: Str, long: Str, arg: Int, id: Str}

const OPTS: List[Opt] = [
  {short: "a", long: "all", arg: 0, id: "all"},
  {short: "A", long: "almost-all", arg: 0, id: "almost_all"},
  {short: "", long: "author", arg: 0, id: "author"},
  {short: "b", long: "escape", arg: 0, id: "escape"},
  {short: "", long: "block-size", arg: 1, id: "block_size"},
  {short: "B", long: "ignore-backups", arg: 0, id: "ignore_backups"},
  {short: "c", long: "", arg: 0, id: "ctime"},
  {short: "C", long: "", arg: 0, id: "columns"},
  {short: "", long: "color", arg: 2, id: "color"},
  {short: "d", long: "directory", arg: 0, id: "directory"},
  {short: "D", long: "dired", arg: 0, id: "dired"},
  {short: "f", long: "", arg: 0, id: "f"},
  {short: "F", long: "", arg: 0, id: "classify_short"},
  {short: "", long: "classify", arg: 2, id: "classify"},
  {short: "", long: "file-type", arg: 0, id: "file_type"},
  {short: "", long: "format", arg: 1, id: "format"},
  {short: "", long: "full-time", arg: 0, id: "full_time"},
  {short: "g", long: "", arg: 0, id: "g"},
  {short: "", long: "group-directories-first", arg: 0, id: "group_dirs_first"},
  {short: "G", long: "no-group", arg: 0, id: "no_group"},
  {short: "h", long: "human-readable", arg: 0, id: "human"},
  {short: "", long: "si", arg: 0, id: "si"},
  {short: "H", long: "dereference-command-line", arg: 0, id: "deref_cmdline"},
  {short: "", long: "dereference-command-line-symlink-to-dir", arg: 0, id: "deref_cmdline_dir"},
  {short: "", long: "hide", arg: 1, id: "hide"},
  {short: "", long: "hyperlink", arg: 2, id: "hyperlink"},
  {short: "", long: "indicator-style", arg: 1, id: "indicator_style"},
  {short: "i", long: "inode", arg: 0, id: "inode"},
  {short: "I", long: "ignore", arg: 1, id: "ignore"},
  {short: "k", long: "kibibytes", arg: 0, id: "kibibytes"},
  {short: "l", long: "", arg: 0, id: "long"},
  {short: "L", long: "dereference", arg: 0, id: "deref_all"},
  {short: "m", long: "", arg: 0, id: "commas"},
  {short: "n", long: "numeric-uid-gid", arg: 0, id: "numeric"},
  {short: "N", long: "literal", arg: 0, id: "literal"},
  {short: "o", long: "", arg: 0, id: "o"},
  {short: "p", long: "", arg: 0, id: "slash"},
  {short: "q", long: "hide-control-chars", arg: 0, id: "hide_control"},
  {short: "", long: "show-control-chars", arg: 0, id: "show_control"},
  {short: "Q", long: "quote-name", arg: 0, id: "quote_name"},
  {short: "", long: "quoting-style", arg: 1, id: "quoting_style"},
  {short: "r", long: "reverse", arg: 0, id: "reverse"},
  {short: "R", long: "recursive", arg: 0, id: "recursive"},
  {short: "s", long: "size", arg: 0, id: "size"},
  {short: "S", long: "", arg: 0, id: "sort_size"},
  {short: "", long: "sort", arg: 1, id: "sort"},
  {short: "t", long: "", arg: 0, id: "sort_time"},
  {short: "", long: "time", arg: 1, id: "time"},
  {short: "", long: "time-style", arg: 1, id: "time_style"},
  {short: "T", long: "tabsize", arg: 1, id: "tabsize"},
  {short: "u", long: "", arg: 0, id: "atime"},
  {short: "U", long: "", arg: 0, id: "sort_none"},
  {short: "v", long: "", arg: 0, id: "sort_version"},
  {short: "w", long: "width", arg: 1, id: "width"},
  {short: "x", long: "", arg: 0, id: "across"},
  {short: "X", long: "", arg: 0, id: "sort_extension"},
  {short: "Z", long: "context", arg: 0, id: "context"},
  {short: "", long: "zero", arg: 0, id: "zero"},
  {short: "1", long: "", arg: 0, id: "single"},
  {short: "", long: "help", arg: 0, id: "help"},
  {short: "", long: "version", arg: 0, id: "version"},
]

const QUOTING_NAMES = ["literal", "shell", "shell-always", "shell-escape", "shell-escape-always", "c", "c-maybe", "escape", "locale", "clocale"]
const WHEN_NAMES = ["always", "yes", "force", "never", "no", "none", "auto", "tty", "if-tty"]
const WHEN_VALUES = ["always", "always", "always", "never", "never", "never", "auto", "auto", "auto"]
const FORMAT_NAMES = ["verbose", "long", "commas", "horizontal", "across", "vertical", "single-column"]
const FORMAT_VALUES = ["long", "long", "commas", "across", "across", "columns", "single-column"]
const SORT_NAMES = ["none", "size", "time", "version", "extension", "width", "name"]
const TIME_NAMES = ["atime", "access", "use", "ctime", "status", "birth", "creation", "mtime"]
const TIME_VALUES = ["atime", "atime", "atime", "ctime", "ctime", "birth", "birth", "mtime"]
const INDICATOR_NAMES = ["none", "slash", "file-type", "classify"]

# GNU argmatch: an exact name wins, otherwise a prefix that all candidates with
# the same value agree on. `values` parallels `names`.
proc argmatch(text: Str, names: List[Str], values: List[Str], option: Str) [process, env] -> Str {
  for index in range(names.len()) {
    return values[index] when names[index] == text
  }

  let found = [index for index in range(names.len()) if names[index].starts_with(text)]

  if found.len() > 0 {
    let first = values[found[0]]

    if [index for index in found if values[index] != first].len() == 0 {
      return first
    }
  }

  let kind = if found.len() == 0 { "invalid" } else { "ambiguous" }
  gnu.error(f"{kind} argument {gnu.quote_value(text)} for {gnu.quote_value(option)}")
  eprint "Valid arguments are:"
  var line = ""
  var last = ""

  for index in range(names.len()) {
    let name = gnu.quote_value(names[index])

    if values[index] == last and line != "" {
      line = f"{line}, {name}"
    } else {
      if line != "" {
        eprint $line
      }

      line = f"  - {name}"
      last = values[index]
    }
  }

  eprint $line
  gnu.try_help()
  exit 1
}

# `strtoul` with base 0 over the whole text: decimal, 0x hex, 0 octal. A value
# too large for Int clamps (GNU treats an overflowing width as unlimited).
pure parse_c_unsigned(text: Str) -> Int? {
  var digits = if text.starts_with("+") { text[1..] } else { text }
  var base = 10

  if digits.starts_with("0x") or digits.starts_with("0X") {
    base = 16
    digits = digits[2..]
  } else if digits.starts_with("0") and digits.byte_len() > 1 {
    base = 8
    digits = digits[1..]
  }

  return null when digits == ""

  var value = 0
  let limit = 100000000000000000

  for ch in digits {
    let found = "0123456789abcdef".find(ch.lower())
    return null when found == null or found >= base

    if value < limit {
      value = value * base + found
    }
  }

  value
}

type Block = {ok: Bool, human: Str, size: Int}

# GNU human_options for a --block-size style argument.
pure parse_block_size(text: Str) -> Block {
  return {ok: true, human: "human", size: 1} when text == "human-readable"
  return {ok: true, human: "si", size: 1} when text == "si"

  let parts = rx"^'?([0-9]*)([EGKMPTYZRQegkmptyzrq]?)(iB|B)?$".captures(text)

  return {ok: false, human: "", size: 0} when parts.len() == 0 or (parts[1] == "" and parts[2] == "") or (parts[3] != "" and parts[2] == "")

  var size = if parts[1] == "" { 1 } else { parts[1].parse_int() ?? 0 }
  let unit = parts[2].upper()
  let base = if parts[3] == "B" { 1000 } else { 1024 }

  if unit != "" {
    for _ in range(("KMGTPEZYRQ".find(unit) ?? 0) + 1) {
      size *= base
    }
  }

  {ok: size > 0, human: "", size: size}
}

proc block_size_option(c: Cfg, text: Str) [process, env] -> Cfg {
  let parsed = parse_block_size(text)

  if ! parsed.ok {
    gnu.error(f"invalid --block-size argument {gnu.quote_value(text)}")
    exit 2
  }

  {...c, human: parsed.human, block_size: parsed.size, file_human: parsed.human, file_block_size: parsed.size}
}

proc line_option(text: Str, what: Str) [process, env] -> Int {
  let value = parse_c_unsigned(text)

  if value == null {
    gnu.error(f"invalid {what}: {gnu.quote_value(text)}")
    exit 2
  }

  value
}

proc when_option(v: Str?, option: Str, tty: Bool) [process, env] -> Bool {
  return true when v == null

  let word = argmatch(v ?? "", WHEN_NAMES, WHEN_VALUES, option)

  word == "always" or (word == "auto" and tty)
}

# One option, in command-line order. `tty` is whether stdout is a terminal.
proc apply(c: Cfg, id: Str, v: Str?, tty: Bool) [process, env] -> Cfg {
  let arg = v ?? ""

  match id {
    "all" => {...c, ignore_mode: "all"}
    "almost_all" => {...c, ignore_mode: "dotdot"}
    "author" => {...c, author: true}
    "escape" => {...c, quoting: "escape"}
    "block_size" => block_size_option(c, arg)
    "ignore_backups" => {...c, ignores: c.ignores + ["*~", ".*~"]}
    "ctime" => {...c, time: "ctime", time_given: true}
    "atime" => {...c, time: "atime", time_given: true}
    "columns" => {...c, format_set: true, format: "columns"}
    "across" => {...c, format_set: true, format: "across"}
    "commas" => {...c, format_set: true, format: "commas"}
    "long" => {...c, format_set: true, format: "long"}
    "single" => {...c, format_set: true, format: if c.format == "long" and c.format_set { "long" } else { "single-column" }}
    "color" => {...c, color: if when_option(v, "--color", tty) { "always" } else { "never" }}
    "directory" => {...c, directory: true}
    "dired" => {...c, format_set: true, format: "long", hyperlink: "never", dired: true}
    "f" => {...c, ignore_mode: "all", sort: "none"}
    "classify_short" => {...c, indicator: "classify"}
    "classify" => {...c, indicator: if when_option(v, "--classify", tty) { "classify" } else { "none" }}
    "file_type" => {...c, indicator: "file-type"}
    "slash" => {...c, indicator: "slash"}
    "indicator_style" => {...c, indicator: argmatch(arg, INDICATOR_NAMES, INDICATOR_NAMES, "--indicator-style")}
    "format" => {...c, format_set: true, format: argmatch(arg, FORMAT_NAMES, FORMAT_VALUES, "--format")}
    "full_time" => {...c, format_set: true, format: "long", time_style: "full-iso"}
    "g" => {...c, format_set: true, format: "long", owner: false}
    "o" => {...c, format_set: true, format: "long", group: false}
    "numeric" => {...c, format_set: true, format: "long", numeric: true}
    "group_dirs_first" => {...c, dirs_first: true}
    "no_group" => {...c, group: false}
    "human" => {...c, human: "human", block_size: 1, file_human: "human", file_block_size: 1}
    "si" => {...c, human: "si", block_size: 1, file_human: "si", file_block_size: 1}
    "deref_cmdline" => {...c, deref: "cmdline"}
    "deref_cmdline_dir" => {...c, deref: "cmdline_dir"}
    "deref_all" => {...c, deref: "always"}
    "hide" => {...c, hides: c.hides + [arg]}
    "ignore" => {...c, ignores: c.ignores + [arg]}
    "hyperlink" => {...c, hyperlink: if when_option(v, "--hyperlink", tty) { "always" } else { "never" }}
    "inode" => {...c, inode: true}
    "kibibytes" => {...c, kibibytes: true}
    "literal" => {...c, quoting: "literal"}
    "quote_name" => {...c, quoting: "c"}
    "quoting_style" => {...c, quoting: argmatch(arg, QUOTING_NAMES, QUOTING_NAMES, "--quoting-style")}
    "hide_control" => {...c, hide_control: true}
    "show_control" => {...c, hide_control: false}
    "reverse" => {...c, reverse: true}
    "recursive" => {...c, recursive: true}
    "size" => {...c, size: true}
    "sort_size" => {...c, sort: "size"}
    "sort_time" => {...c, sort: "time"}
    "sort_none" => {...c, sort: "none"}
    "sort_version" => {...c, sort: "version"}
    "sort_extension" => {...c, sort: "extension"}
    "sort" => {...c, sort: argmatch(arg, SORT_NAMES, SORT_NAMES, "--sort")}
    "time" => {...c, time: argmatch(arg, TIME_NAMES, TIME_VALUES, "--time"), time_given: true}
    "time_style" => {...c, time_style: arg}
    "tabsize" => {...c, tabsize: line_option(arg, "tab size")}
    "width" => {...c, width: line_option(arg, "line width")}
    "context" => {...c, context: true}
    "zero" => {...c, zero: true, format: if c.format_set { c.format } else { "single-column" }, quoting: "literal", hide_control: false, color: "never"}
    else => c
  }
}

# Result of option parsing: the settings and the file operands.
type Parsed = {cfg: Cfg, operands: List[Str]}

proc usage_failure(message: Str) [process, env] -> Unit {
  gnu.usage_error(message, 2)
}

# GNU getopt_long over `argv`, applying each option as it is met so the last
# of several conflicting options wins and errors appear in command-line order.
proc parse_args(argv: List[Str], start: Cfg, tty: Bool) [process, env, io] -> Parsed {
  var cfg = start
  var operands: List[Str] = []
  let posix = env.get("POSIXLY_CORRECT") is Ok(_)
  var only_operands = false
  var index = 0

  while index < argv.len() {
    let arg = argv[index]
    index += 1

    if only_operands or arg == "-" or ! arg.starts_with("-") {
      operands += [arg]
      if posix {
        only_operands = true
      }

      continue
    }

    if arg == "--" {
      only_operands = true
      continue
    }

    if arg.starts_with("--") {
      let body = arg[2..]
      let equals = body.find("=")
      let name = if equals == null { body } else { body.byte_slice(0, equals ?? 0) }
      let attached: Str? = if equals == null { null } else { body.byte_slice((equals ?? 0) + 1) }
      var matches = [opt for opt in OPTS if opt.long == name]

      if matches.len() == 0 {
        matches = [opt for opt in OPTS if opt.long != "" and opt.long.starts_with(name)]
      }

      let ids = [opt.id for opt in matches]

      if matches.len() == 0 {
        usage_failure(f"unrecognized option '--{name}'")
      }

      if ids.len() > 1 and [id for id in ids if id != ids[0]].len() > 0 {
        let names = [f"'--{opt.long}'" for opt in matches |> sort-by .long]
        usage_failure(f"option '--{name}' is ambiguous; possibilities: {names.join(" ")}")
      }

      let opt = matches[0]
      var value: Str? = attached

      if opt.arg == 0 and attached != null {
        usage_failure(f"option '--{opt.long}' doesn't allow an argument")
      }

      if opt.arg == 1 and attached == null {
        if index >= argv.len() {
          usage_failure(f"option '--{opt.long}' requires an argument")
        }

        value = argv[index]
        index += 1
      }

      cfg = dispatch(cfg, opt.id, value, tty)
      continue
    }

    var position = 1
    let total = arg.count_chars()

    while position < total {
      let letter = arg[position..position + 1]
      position += 1
      let found = [opt for opt in OPTS if opt.short == letter]

      if found.len() == 0 {
        usage_failure(f"invalid option -- '{letter}'")
      }

      let opt = found[0]
      var value: Str? = null

      if opt.arg == 1 {
        if position < total {
          value = arg[position..]
          position = total
        } else if index < argv.len() {
          value = argv[index]
          index += 1
        } else {
          usage_failure(f"option requires an argument -- '{letter}'")
        }
      }

      cfg = dispatch(cfg, opt.id, value, tty)
    }
  }

  {cfg: cfg, operands: operands}
}

# --help and --version end the program; everything else changes the settings.
proc dispatch(c: Cfg, id: Str, v: Str?, tty: Bool) [process, env, io] -> Cfg {
  if id == "help" {
    gnu.help(USAGE.replace("{prog}", gnu.prog()))
    exit 0
  }

  if id == "version" {
    gnu.version(gnu.prog())
    exit 0
  }

  apply(c, id, v, tty)
}

proc env_text(name: Str) [env] -> Str? {
  match env.get(name) {
    Ok(text) => text
    Err(_) => null
  }
}

# The first non-empty of LC_ALL, then the category, then LANG.
proc locale_value(category: Str) [env] -> Str {
  for name in ["LC_ALL", category, "LANG"] {
    let found = env_text(name) ?? ""

    return found when found != ""
  }

  ""
}

proc utf8_locale() [env] -> Bool {
  let lower = locale_value("LC_CTYPE").lower()

  lower.find("utf-8") != null or lower.find("utf8") != null
}

proc hard_time_locale() [env] -> Bool {
  let value = locale_value("LC_TIME")

  ! (value == "" or value == "C" or value == "POSIX")
}

proc stdout_is_tty() [process] -> Bool {
  unix.tty_attrs(1) is Ok(_)
}

# Whole units of `to`-byte blocks for `n` blocks of `from` bytes, rounded up,
# or with a K/M/G suffix and one decimal below 10 when `mode` is "human"
# (powers of 1024) or "si" (powers of 1000), as gnulib human_readable.
pure human_readable(n: Int, from: Int, to: Int, mode: Str) -> Str {
  let num = n * from

  return f"{(num + to - 1) / to}" when mode == ""

  let base = if mode == "si" { 1000 } else { 1024 }
  let suffixes = ["", if mode == "si" { "k" } else { "K" }, "M", "G", "T", "P", "E", "Z", "Y", "R", "Q"]
  var power = 0
  var unit = to

  while power < 10 and num / unit >= base {
    unit *= base
    power += 1
  }

  var whole = num / unit
  let rest = num % unit
  var tenths = 0

  if power > 0 and whole < 10 {
    tenths = (rest * 10 + unit - 1) / unit

    if tenths == 10 {
      whole += 1
      tenths = 0
    }
  } else if rest > 0 {
    whole += 1
  }

  if whole >= base and power < 10 {
    whole = 1
    tenths = 0
    power += 1
  }

  if power > 0 and whole < 10 {
    return f"{whole}.{tenths}{suffixes[power]}"
  }

  f"{whole}{suffixes[power]}"
}

type Tz = {name: Str, offset: Int}

proc load_tz() [env] -> Tz {
  let value = env_text("TZ") ?? ""
  let parts = rx"^<?([A-Za-z0-9+-]+?)>?([+-]?)([0-9]{1,2})(?::([0-9]{2}))?(?::([0-9]{2}))?$".captures(value)

  if parts.len() > 0 {
    let seconds = (parts[3].parse_int() ?? 0) * 3600 + (parts[4].parse_int() ?? 0) * 60 + (parts[5].parse_int() ?? 0)

    return {name: parts[1], offset: if parts[2] == "-" { seconds } else { -seconds }}
  }

  {name: "UTC", offset: 0}
}

type Civil = {year: Int, month: Int, day: Int, hour: Int, minute: Int, second: Int, weekday: Int, yearday: Int}

pure floor_div(a: Int, b: Int) -> Int {
  let q = a / b

  if a % b != 0 and (a < 0) != (b < 0) { q - 1 } else { q }
}

pure civil_from_seconds(seconds: Int) -> Civil {
  let days = floor_div(seconds, 86400)
  let rest = seconds - days * 86400
  let z = days + 719468
  let era = floor_div(z, 146097)
  let doe = z - era * 146097
  let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365
  let doy = doe - (365 * yoe + yoe / 4 - yoe / 100)
  let mp = (5 * doy + 2) / 153
  let day = doy - (153 * mp + 2) / 5 + 1
  let month = if mp < 10 { mp + 3 } else { mp - 9 }
  let year = yoe + era * 400 + (if month <= 2 { 1 } else { 0 })
  let weekday = ((days + 4) % 7 + 7) % 7
  let leap = (year % 4 == 0 and year % 100 != 0) or year % 400 == 0
  let before = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334]
  let yearday = before[month - 1] + day + (if leap and month > 2 { 1 } else { 0 })

  {
    year: year,
    month: month,
    day: day,
    hour: rest / 3600,
    minute: rest % 3600 / 60,
    second: rest % 60,
    weekday: weekday,
    yearday: yearday,
  }
}

const MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"]
const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"]

pure pad_number(value: Int, width: Int, fill: Str) -> Str {
  var text = f"{value}"

  while text.byte_len() < width {
    text = f"{fill}{text}"
  }

  text
}

# strftime for the conversions `ls --time-style=+FORMAT` can reasonably use.
# An unknown conversion is copied through unchanged, as glibc does.
pure format_time(format: Str, seconds: Int, nanos: Int, tz: Tz) -> Str {
  let local = seconds + tz.offset
  let t = civil_from_seconds(local)
  var out = ""
  var at = 0
  let total = format.byte_len()

  while at < total {
    let ch = format.byte_slice(at, length: 1)

    if ch != "%" {
      let start = at

      while at < total and format.byte_slice(at, length: 1) != "%" {
        at += 1
      }

      out = f"{out}{format.byte_slice(start, length: at - start)}"
      continue
    }

    let origin = at
    at += 1
    var flag = ""
    var upper = false

    while at < total and "-_0^#".find(format.byte_slice(at, length: 1)) != null {
      let f = format.byte_slice(at, length: 1)

      if f == "^" {
        upper = true
      } else if f != "#" {
        flag = f
      }

      at += 1
    }

    var width = -1

    while at < total and "0123456789".find(format.byte_slice(at, length: 1)) != null {
      let digit = "0123456789".find(format.byte_slice(at, length: 1)) ?? 0
      width = (if width < 0 { 0 } else { width }) * 10 + digit
      at += 1
    }

    while at < total and "EO".find(format.byte_slice(at, length: 1)) != null {
      at += 1
    }

    if at >= total {
      out = f"{out}{format.byte_slice(origin)}"
      break
    }

    let conv = format.byte_slice(at, length: 1)
    at += 1
    var text = ""
    var numeric = false
    var digits = 2
    var fill = "0"
    var value = 0
    let hour12 = if t.hour % 12 == 0 { 12 } else { t.hour % 12 }

    match conv {
      "Y" => {
        numeric = true
        value = t.year
        digits = 1
      }
      "C" => {
        numeric = true
        value = floor_div(t.year, 100)
      }
      "y" => {
        numeric = true
        value = (t.year % 100 + 100) % 100
      }
      "m" => {
        numeric = true
        value = t.month
      }
      "d" => {
        numeric = true
        value = t.day
      }
      "e" => {
        numeric = true
        value = t.day
        fill = " "
      }
      "H" => {
        numeric = true
        value = t.hour
      }
      "k" => {
        numeric = true
        value = t.hour
        fill = " "
      }
      "I" => {
        numeric = true
        value = hour12
      }
      "l" => {
        numeric = true
        value = hour12
        fill = " "
      }
      "M" => {
        numeric = true
        value = t.minute
      }
      "S" => {
        numeric = true
        value = t.second
      }
      "j" => {
        numeric = true
        value = t.yearday
        digits = 3
      }
      "u" => {
        numeric = true
        value = if t.weekday == 0 { 7 } else { t.weekday }
        digits = 1
      }
      "w" => {
        numeric = true
        value = t.weekday
        digits = 1
      }
      "U" => {
        numeric = true
        value = (t.yearday - 1 + 7 - t.weekday) / 7
      }
      "W" => {
        numeric = true
        value = (t.yearday - 1 + 7 - (t.weekday + 6) % 7) / 7
      }
      "s" => {
        numeric = true
        value = seconds
        digits = 1
      }
      "N" => text = pad_number(nanos, 9, "0")
      "a" => text = WEEKDAYS[t.weekday].byte_slice(0, length: 3)
      "A" => text = WEEKDAYS[t.weekday]
      "b" | "h" => text = MONTHS[t.month - 1].byte_slice(0, length: 3)
      "B" => text = MONTHS[t.month - 1]
      "p" => text = if t.hour < 12 { "AM" } else { "PM" }
      "P" => text = if t.hour < 12 { "am" } else { "pm" }
      "Z" => text = tz.name
      "z" => {
        let magnitude = if tz.offset < 0 { -tz.offset } else { tz.offset }
        text = f"{if tz.offset < 0 { "-" } else { "+" }}{pad_number(magnitude / 3600, 2, "0")}{pad_number(magnitude % 3600 / 60, 2, "0")}"
      }
      "n" => text = "\n"
      "t" => text = "\t"
      "%" => text = "%"
      "F" => text = f"{t.year}-{pad_number(t.month, 2, "0")}-{pad_number(t.day, 2, "0")}"
      "T" | "X" => text = f"{pad_number(t.hour, 2, "0")}:{pad_number(t.minute, 2, "0")}:{pad_number(t.second, 2, "0")}"
      "R" => text = f"{pad_number(t.hour, 2, "0")}:{pad_number(t.minute, 2, "0")}"
      "D" | "x" => text = f"{pad_number(t.month, 2, "0")}/{pad_number(t.day, 2, "0")}/{pad_number(t.year % 100, 2, "0")}"
      "r" => text = f"{pad_number(hour12, 2, "0")}:{pad_number(t.minute, 2, "0")}:{pad_number(t.second, 2, "0")} {if t.hour < 12 { "AM" } else { "PM" }}"
      "c" => text = f"{WEEKDAYS[t.weekday].byte_slice(0, length: 3)} {MONTHS[t.month - 1].byte_slice(0, length: 3)} {pad_number(t.day, 2, " ")} {pad_number(t.hour, 2, "0")}:{pad_number(t.minute, 2, "0")}:{pad_number(t.second, 2, "0")} {t.year}"
      else => {
        out = f"{out}{format.byte_slice(origin, length: at - origin)}"
        continue
      }
    }

    if numeric {
      let use_fill = if flag == "-" { "" } else if flag == "_" { " " } else if flag == "0" { "0" } else { fill }
      let w = if width >= 0 { width } else if flag == "-" { 0 } else { digits }
      text = if use_fill == "" { f"{value}" } else { pad_number(value, w, use_fill) }
    } else if width > 0 {
      text = pad_number(0, 0, "") + text
    }

    if upper {
      text = text.upper()
    }

    out = f"{out}{text}"
  }

  out
}

# Characters that never need quoting in any style (except as listed below).
const SAFE_NAME = rx"^[A-Za-z0-9_.,+%@/\]:-][A-Za-z0-9_.,+%@/\]:{}#~-]*$"

pure octal3(byte: Int) -> Str {
  f"\\{byte / 64}{byte / 8 % 8}{byte % 8}"
}

# One input character per entry: its bytes, whether it is printable, and its
# display width. In a unibyte locale every byte is a character.
type Units = {chars: List[Bytes], printable: List[Bool], widths: List[Int]}

pure code_point_width(cp: Int) -> Int {
  return 0 when cp >= 768 and cp <= 879
  return 2 when (cp >= 4352 and cp <= 4447) or (cp >= 11904 and cp <= 42191) or (cp >= 44032 and cp <= 55203)
  return 2 when (cp >= 63744 and cp <= 64255) or (cp >= 65072 and cp <= 65135) or (cp >= 65280 and cp <= 65376)
  return 2 when (cp >= 65504 and cp <= 65510) or (cp >= 127744 and cp <= 128591) or (cp >= 129280 and cp <= 129535)
  return 2 when cp >= 131072 and cp <= 262141

  1
}

pure decode_units(raw: Bytes, utf8: Bool) -> Units {
  var chars: List[Bytes] = []
  var printable: List[Bool] = []
  var widths: List[Int] = []
  var at = 0
  let total = raw.len()

  while at < total {
    let byte = raw.byte_at(at) ?? 0
    var size = 1
    var shown = byte >= 32 and byte < 127
    var width = 1

    if byte >= 128 and utf8 {
      let need = if byte >= 194 and byte <= 223 { 2 } else if byte >= 224 and byte <= 239 { 3 } else if byte >= 240 and byte <= 244 { 4 } else { 0 }

      if need > 0 and at + need <= total and raw[at..at + need].utf8() is Ok(_) {
        size = need
        let second = (raw.byte_at(at + 1) ?? 128) - 128
        let third = (raw.byte_at(at + 2) ?? 128) - 128
        let fourth = (raw.byte_at(at + 3) ?? 128) - 128
        let cp = if need == 2 {
          (byte - 192) * 64 + second
        } else if need == 3 {
          (byte - 224) * 4096 + second * 64 + third
        } else {
          (byte - 240) * 262144 + second * 4096 + third * 64 + fourth
        }

        shown = ! (cp >= 128 and cp <= 159) and cp != 8232 and cp != 8233 and cp != 65534 and cp != 65535
        width = code_point_width(cp)
      } else {
        shown = false
      }
    } else if byte >= 128 {
      shown = false
    }

    chars += [raw[at..at + size]]
    printable += [shown]
    widths += [width]
    at += size
  }

  {chars: chars, printable: printable, widths: widths}
}

pure display_width(raw: Bytes, utf8: Bool) -> Int {
  return raw.len() when ! utf8

  guard let text = raw.utf8() else {
    return raw.len()
  }

  return raw.len() when text.count_chars() == raw.len()

  var total = 0

  for width in decode_units(raw, true).widths {
    total += width
  }

  total
}

type QuoteStyle = {name: Str, utf8: Bool, curly: Bool, extra: Str, space: Bool}

# The escape letter for control bytes with a C name.
pure escape_letter(byte: Int) -> Str {
  return "a" when byte == 7
  return "b" when byte == 8
  return "f" when byte == 12
  return "n" when byte == 10
  return "r" when byte == 13
  return "t" when byte == 9
  return "v" when byte == 11

  ""
}

# Backslash styles (escape, c, c-maybe, locale, clocale): `left` and `right`
# are the quote marks; `elide` is c-maybe's "only quote when something needed
# escaping". Returns null when eliding and nothing needed it.
pure backslash_quote(units: Units, left: Str, right: Str, elide: Bool, extra: Str, space: Bool) -> Str? {
  var body = ""
  var escaped = false
  let stored = if elide { "" } else { right }
  let total = units.chars.len()

  for index in range(total) {
    let chunk = units.chars[index]
    let byte = chunk.byte_at(0) ?? 0
    let letter = if chunk.len() == 1 { escape_letter(byte) } else { "" }
    let text = chunk.utf8() ?? ""

    if letter != "" {
      return null when elide
      body = f"{body}\\{letter}"
    } else if chunk.len() == 1 and byte == 92 {
      body = f"{body}{if elide { "\\" } else { "\\\\" }}"
    } else if ! units.printable[index] {
      return null when elide
      escaped = true
      body = f"{body}{octal_bytes(chunk)}"
    } else if (stored != "" and text == stored) or (extra != "" and extra.find(text) != null) or (space and text == " ") {
      return null when elide
      body = f"{body}\\{text}"
    } else {
      body = f"{body}{text}"
    }
  }

  if elide { body } else { f"{left}{body}{right}" }
}

pure octal_bytes(chunk: Bytes) -> Str {
  var out = ""

  for byte in chunk {
    out = f"{out}{octal3(byte)}"
  }

  out
}

const SHELL_SPECIAL = " !\"$&()*;<=>?[\\^`|'"
const COMPAT_PUNCT = "%+,-./:]_@' "

pure ascii_text(chunk: Bytes) -> Str {
  if chunk.len() == 1 { chunk.utf8() ?? "" } else { "" }
}

# The shell styles. `escapes` is shell-escape (non-printables become $'...'),
# `always` is shell-always / shell-escape-always. A name that needs nothing is
# returned as it is unless `always`.
pure shell_quote(raw: Bytes, units: Units, escapes: Bool, always: Bool, extra: Str) -> Bytes {
  let total = units.chars.len()
  var force = always or total == 0

  if ! force {
    for index in range(total) {
      let chunk = units.chars[index]
      let byte = chunk.byte_at(0) ?? 0
      let text = ascii_text(chunk)
      let tricky = text != "" and (SHELL_SPECIAL.find(text) != null or (extra != "" and extra.find(text) != null))
      let lone = total == 1 and (text == "{" or text == "}")
      let lead = index == 0 and (text == "#" or text == "~")
      let control = chunk.len() == 1 and (byte == 9 or byte == 10 or byte == 13)
      let escaped = escapes and (! units.printable[index] or (chunk.len() == 1 and escape_letter(byte) != ""))

      if tricky or lone or lead or control or escaped {
        force = true
        break
      }
    }
  }

  return raw when ! force

  var pieces: List[Bytes] = [b"'"]
  var inside = false
  var quote_seen = false
  var compat = true

  for index in range(total) {
    let chunk = units.chars[index]
    let byte = chunk.byte_at(0) ?? 0
    let text = ascii_text(chunk)
    let letter = if chunk.len() == 1 { escape_letter(byte) } else { "" }

    if escapes and (! units.printable[index] or letter != "") {
      compat = false

      if ! inside {
        pieces += [b"'$'"]
        inside = true
      }

      pieces += [bytes.from_text(if letter != "" { f"\\{letter}" } else { octal_bytes(chunk) })]
      continue
    }

    if inside {
      pieces += [b"''"]
      inside = false
    }

    if text == "'" {
      quote_seen = true
      pieces += [b"'\\''"]
    } else {
      pieces += [chunk]
    }

    let plain = text != "" and (rx"^[A-Za-z0-9]$".matches(text) or COMPAT_PUNCT.find(text) != null)
    let special = (index == 0 and (text == "#" or text == "~")) or (total == 1 and (text == "{" or text == "}"))

    if ! (plain or special or (chunk.len() > 1 and units.printable[index])) {
      compat = false
    }
  }

  return bytes.concat([b"\"", raw, b"\""]) when quote_seen and compat

  pieces += [b"'"]
  bytes.concat(pieces)
}

# GNU quotearg for ls. `extra` lists characters that must be quoted too (the
# colon of directory headers, the indicator characters).
pure quote_raw(raw: Bytes, qs: QuoteStyle) -> Bytes {
  let style = qs.name

  return raw when style == "literal"

  let text = raw.utf8() ?? ""

  if text != "" and SAFE_NAME.matches(text) and (qs.extra == "" or [c for c in qs.extra if text.find(c) != null].len() == 0) {
    return match style {
      "shell-always" | "shell-escape-always" => bytes.concat([b"'", raw, b"'"])
      "c" => bytes.concat([b"\"", raw, b"\""])
      "locale" => bytes.from_text(f"{if qs.curly { "‘" } else { "'" }}{text}{if qs.curly { "’" } else { "'" }}")
      "clocale" => bytes.from_text(f"{if qs.curly { "‘" } else { "\"" }}{text}{if qs.curly { "’" } else { "\"" }}")
      else => raw
    }
  }

  let units = decode_units(raw, qs.utf8)

  match style {
    "shell" => shell_quote(raw, units, false, false, qs.extra)
    "shell-always" => shell_quote(raw, units, false, true, qs.extra)
    "shell-escape" => shell_quote(raw, units, true, false, qs.extra)
    "shell-escape-always" => shell_quote(raw, units, true, true, qs.extra)
    "escape" => bytes.from_text(backslash_quote(units, "", "", false, qs.extra, qs.space) ?? "")
    "c" => bytes.from_text(backslash_quote(units, "\"", "\"", false, qs.extra, false) ?? "")
    "c-maybe" => match backslash_quote(units, "\"", "\"", true, qs.extra, false) {
      null => bytes.from_text(backslash_quote(units, "\"", "\"", false, qs.extra, false) ?? "")
      else => raw
    }
    "locale" => bytes.from_text(backslash_quote(units, if qs.curly { "‘" } else { "'" }, if qs.curly { "’" } else { "'" }, false, qs.extra, false) ?? "")
    "clocale" => bytes.from_text(backslash_quote(units, if qs.curly { "‘" } else { "\"" }, if qs.curly { "’" } else { "\"" }, false, qs.extra, false) ?? "")
    else => raw
  }
}

# `?` for every character that is not printable (ls -q on the literal and
# shell styles).
pure hide_controls(raw: Bytes, utf8: Bool) -> Bytes {
  let units = decode_units(raw, utf8)
  var pieces: List[Bytes] = []

  for index in range(units.chars.len()) {
    pieces += [if units.printable[index] { units.chars[index] } else { b"?" }]
  }

  bytes.concat(pieces)
}

const INDICATOR_CODES = ["lc", "rc", "ec", "rs", "no", "fi", "di", "ln", "pi", "so", "bd", "cd", "mi", "or", "ex", "do", "su", "sg", "st", "ow", "tw", "ca", "mh", "cl"]
const DEFAULT_COLORS: Map[Str] = {
  lc: "\u{1b}[",
  rc: "m",
  rs: "0",
  di: "01;34",
  ln: "01;36",
  pi: "33",
  so: "01;35",
  bd: "01;33",
  cd: "01;33",
  ex: "01;32",
  do: "01;35",
  su: "37;41",
  sg: "30;43",
  st: "37;44",
  ow: "34;42",
  tw: "30;42",
  ca: "30;41",
  cl: "\u{1b}[K",
}

type ExtColor = {ext: Str, seq: Str, exact: Bool}

# `ok` is false when LS_COLORS could not be parsed (colors are then off).
type Colors = {ok: Bool, ind: Map[Str], exts: List[ExtColor], referent: Bool, unknown: List[Str]}

# GNU get_funky_string: backslash escapes and ^X control notation.
pure unescape_color(value: Str) -> Str {
  return value when value.find("\\") == null and value.find("^") == null

  var out = ""
  let chars = [ch for ch in value]
  var at = 0

  while at < chars.len() {
    let ch = chars[at]
    at += 1

    if ch == "^" and at < chars.len() {
      let next = chars[at]
      at += 1
      out = f"{out}{if next == "?" { "\u{7f}" } else { control_char(next) }}"
    } else if ch == "\\" and at < chars.len() {
      let next = chars[at]
      at += 1

      match next {
        "a" => out = f"{out}\u{7}"
        "b" => out = f"{out}\u{8}"
        "e" => out = f"{out}\u{1b}"
        "f" => out = f"{out}\u{c}"
        "n" => out = f"{out}\n"
        "r" => out = f"{out}\r"
        "t" => out = f"{out}\t"
        "v" => out = f"{out}\u{b}"
        "?" => out = f"{out}\u{7f}"
        "_" => out = f"{out} "
        "x" | "X" => {
          var number = 0
          var count = 0

          while at < chars.len() and count < 2 and "0123456789abcdefABCDEF".find(chars[at]) != null {
            number = number * 16 + ("0123456789abcdef".find(chars[at].lower()) ?? 0)
            at += 1
            count += 1
          }

          out = f"{out}{byte_char(number)}"
        }
        "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" => {
          var number = "01234567".find(next) ?? 0
          var count = 1

          while at < chars.len() and count < 3 and "01234567".find(chars[at]) != null {
            number = number * 8 + ("01234567".find(chars[at]) ?? 0)
            at += 1
            count += 1
          }

          out = f"{out}{byte_char(number)}"
        }
        else => out = f"{out}{next}"
      }
    } else {
      out = f"{out}{ch}"
    }
  }

  out
}

pure byte_char(number: Int) -> Str {
  (bytes.from_ints([number % 128]) ?? b"").utf8() ?? ""
}

pure control_char(ch: Str) -> Str {
  let at = " !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_".find(ch) ?? 64
  byte_char((at + 32) % 32)
}

pure ls_color_parse(text: Str) -> Colors {
  var ind = DEFAULT_COLORS
  var exts: List[ExtColor] = []
  var referent = false
  var failed = false
  var unknown: List[Str] = []

  for entry in text.split(":") {
    continue when entry == ""

    let equals = entry.find("=")

    if entry.starts_with("*") {
      if equals == null {
        failed = true
        break
      }

      exts = [{ext: unescape_color(entry.byte_slice(1, length: (equals ?? 0) - 1)), seq: unescape_color(entry.byte_slice((equals ?? 0) + 1)), exact: false}] + exts
    } else {
      if equals != 2 {
        failed = true
        break
      }

      let label = entry.byte_slice(0, length: 2)
      let value = unescape_color(entry.byte_slice(3))

      if label in INDICATOR_CODES {
        ind = ind.set(label, value)

        if label == "ln" and value == "target" {
          referent = true
        }
      } else {
        unknown += [label]
      }
    }
  }

  var kept: List[ExtColor] = []

  for entry in exts {
    if [seen for seen in kept if seen.ext == entry.ext].len() == 0 {
      kept += [entry]
    }
  }

  let marked = [
    {
      ext: entry.ext,
      seq: entry.seq,
      exact: [other for other in kept if other.ext != entry.ext and other.ext.lower() == entry.ext.lower() and other.seq != entry.seq].len() > 0,
    }
    for entry in kept
  ]

  {ok: ! failed, ind: ind, exts: marked, referent: referent, unknown: unknown}
}

const ZERO_STAT: FsStat = {
  atime_ns: 0,
  birth_ns: null,
  blksize: 0,
  blocks_512: 0,
  ctime_ns: 0,
  dev: 0,
  gid: 0,
  ino: 0,
  kind: "",
  mode: 0,
  mtime_ns: 0,
  nlink: 0,
  rdev: 0,
  size: 0,
  uid: 0,
}

# Everything `ls` knows about one listed name. `raw` is the name as the
# filesystem spells it, `q` its quoted form for display and `qw` the width of
# that. `ok` says `st` was read; without it only `kind` is known.
type File = {
  raw: Bytes,
  name: Str,
  path: Path,
  ok: Bool,
  st: FsStat,
  kind: Str,
  arg: Bool,
  q: Bytes,
  qw: Int,
  quoted: Bool,
  link: Bytes?,
  linkok: Bool,
  linkmode: Int,
  linkkind: Str,
}

# Everything the options and environment settle before any file is read.
type Ctx = {
  cfg: Cfg,
  format: Str,
  utf8: Bool,
  qs: QuoteStyle,
  dir_qs: QuoteStyle,
  color: Bool,
  colors: Colors,
  hyper: Bool,
  host: Str,
  cwd: Str,
  line_length: Int,
  tabsize: Int,
  eol: Str,
  needs_stat: Bool,
  deref: Str,
  indicator: Str,
  align: Bool,
  tz: Tz,
  now: Int,
  time_fmt: List[Str],
  human: Str,
  block_size: Int,
  file_human: Str,
  file_block_size: Int,
  check_symlink: Bool,
  link_stat: Bool,
  sort: Str,
  qmark: Bool,
  ignore_globs: List[Glob],
  hide_globs: List[Glob],
}

pure kind_letter(kind: Str) -> Str {
  match kind {
    "dir" => "d"
    "symlink" => "l"
    "fifo" => "p"
    "char" => "c"
    "block" => "b"
    "socket" => "s"
    "file" => "-"
    else => "?"
  }
}

pure join_raw(dir: Bytes, name: Bytes) -> Bytes {
  return name when dir.len() == 0

  if dir.byte_at(dir.len() - 1) == 47 {
    bytes.concat([dir, name])
  } else {
    bytes.concat([dir, b"/", name])
  }
}

pure raw_path(raw: Bytes) -> Path {
  Path.parse_bytes(raw) ?? Path("")
}

pure lossy(raw: Bytes) -> Str {
  raw.utf8() ?? (Path.parse_bytes(raw) ?? Path("")).display()
}

# The last path component of `path`'s bytes.
pure base_raw(raw: Bytes) -> Bytes {
  var at = raw.len()

  while at > 0 and raw.byte_at(at - 1) != 47 {
    at -= 1
  }

  raw[at..]
}

pure mode_has(mode: Int, bit: Int) -> Bool {
  mode / bit % 2 == 1
}

# A File for a command-line operand or a directory entry. `dir` is empty for
# operands. `hint` is the entry kind from the directory read, if any.
type Gobbled = {file: File?, status: Int}

proc follow_stat(target: Path, follow: Bool) [fs] -> Result[FsStat] {
  fs.stat(target, follow)
}

proc gobble(ctx: Ctx, name: Bytes, dir: Bytes, arg: Bool, hint: Str) [fs, process, env] -> Gobbled {
  let full = if dir.len() == 0 or dir == b"." { name } else { join_raw(dir, name) }
  let target = raw_path(full)
  let display = lossy(full)
  var st = ZERO_STAT
  var ok = false
  var kind = hint
  var failed = false

  if arg or ctx.needs_stat or hint == "" {
    var result = follow_stat(target, ctx.deref == "always")

    if arg and (ctx.deref == "cmdline" or ctx.deref == "cmdline_dir") {
      result = follow_stat(target, true)

      if ctx.deref == "cmdline_dir" {
        let need_lstat = match result {
          Ok(found) => found.kind != "dir"
          Err(problem) => gnu.errno(problem) == 2
        }

        if need_lstat {
          result = follow_stat(target, false)
        }
      }
    }

    match result {
      Ok(found) => {
        st = found
        ok = true
        kind = found.kind
      }
      Err(problem) => {
        gnu.cannot_access(display, problem)

        if arg {
          return {file: null, status: 2}
        }

        failed = true
      }
    }
  }

  var link: Bytes? = null
  var linkok = false
  var linkmode = 0
  var linkkind = ""

  if ok and kind == "symlink" and (ctx.format == "long" or ctx.check_symlink) {
    if let Ok(pointed) = target.readlink() {
      link = pointed.bytes()
      let target_raw = pointed.bytes()
      let resolved = if target_raw.byte_at(0) == 47 { target_raw } else { join_raw(base_dir(full), target_raw) }

      if ctx.link_stat {
        if let Ok(found) = follow_stat(raw_path(resolved), true) {
          linkok = true
          linkmode = found.mode
          linkkind = found.kind
        }
      }
    }
  }

  let shown = quote_name(ctx, name, ctx.qs)
  let file: File = {
    raw: name,
    name: lossy(name),
    path: target,
    ok: ok,
    st: st,
    kind: kind,
    arg: arg,
    q: shown.q,
    qw: shown.w,
    quoted: shown.quoted,
    link: link,
    linkok: linkok,
    linkmode: linkmode,
    linkkind: linkkind,
  }

  {file: file, status: if failed { 1 } else { 0 }}
}

# The directory part of `raw` as the kernel would resolve a relative link
# target: everything before the last slash, or "." with none.
pure base_dir(raw: Bytes) -> Bytes {
  var at = raw.len()

  while at > 0 and raw.byte_at(at - 1) != 47 {
    at -= 1
  }

  return b"." when at == 0
  return b"/" when at == 1

  raw[..at - 1]
}

type Shown = {q: Bytes, w: Int, quoted: Bool}

# A name quoted for display: GNU quote_name_buf, including the question-mark
# pass for ls -q and the width it takes on screen.
pure quote_name(ctx: Ctx, name: Bytes, qs: QuoteStyle) -> Shown {
  var out = quote_raw(name, qs)
  let style = qs.name
  let width_text = ctx.qmark and (style == "literal" or style == "shell" or style == "shell-always")

  if width_text {
    out = hide_controls(out, qs.utf8)
  }

  {q: out, w: display_width(out, qs.utf8), quoted: out != name}
}

# A shell pattern as an anchored regular expression: * ? [..] [!..] and
# backslash escapes. An unclosed [ is a plain character.
pure glob_regex(pattern: Str) -> Str {
  let chars = [ch for ch in pattern]
  let total = chars.len()
  var out = "^"
  var at = 0

  while at < total {
    let ch = chars[at]
    at += 1

    if ch == "*" {
      out = f"{out}(?s:.*)"
    } else if ch == "?" {
      out = f"{out}(?s:.)"
    } else if ch == "\\" and at < total {
      out = f"{out}{regex_escape(chars[at])}"
      at += 1
    } else if ch == "[" {
      var close = at

      if close < total and (chars[close] == "!" or chars[close] == "^") {
        close += 1
      }

      if close < total and chars[close] == "]" {
        close += 1
      }

      while close < total and chars[close] != "]" {
        close += 1
      }

      if close >= total {
        out = f"{out}\\["
      } else {
        var body = ""
        var inner = at

        if chars[inner] == "!" or chars[inner] == "^" {
          body = "^"
          inner += 1
        }

        while inner < close {
          let c = chars[inner]
          inner += 1

          if c == "[" and inner < close and chars[inner] == ":" {
            var end = inner

            while end < close and chars[end] != "]" {
              end += 1
            }

            body = f"{body}[{chars[inner..end].join("")}]"
            inner = end + 1
          } else if c == "\\" or c == "[" or c == "&" or c == "~" or c == "|" {
            body = f"{body}\\{c}"
          } else if c == "]" and inner == at + 1 {
            body = f"{body}\\]"
          } else {
            body = f"{body}{c}"
          }
        }

        out = f"{out}[{body}]"
        at = close + 1
      }
    } else {
      out = f"{out}{regex_escape(ch)}"
    }
  }

  f"{out}$"
}

pure regex_escape(ch: Str) -> Str {
  if ".+(){}|^$*?[]\\-".find(ch) != null { f"\\{ch}" } else { ch }
}

# A compiled pattern; `dot` is whether it starts with a literal period, which
# the leading period of a name must match (fnmatch FNM_PERIOD).
type Glob = {rx: Regex, dot: Bool}

proc compile_glob(pattern: Str) [process, env] -> Glob {
  let made = regex.compile(glob_regex(pattern))

  guard let compiled = made else {
    return {rx: rx"^$", dot: false}
  }

  {rx: compiled, dot: pattern.starts_with(".") or pattern.starts_with("\\.")}
}

pure glob_matches(globs: List[Glob], name: Str) -> Bool {
  for glob in globs {
    continue when name.starts_with(".") and ! glob.dot

    return true when glob.rx.matches(name)
  }

  false
}

# gnulib filevercmp: version-aware comparison of file names.
pure ver_order(text: Str, at: Int, len: Int) -> Int {
  return -1 when at >= len

  let c = text.byte_at(at) ?? 0

  return 0 when c >= 48 and c <= 57
  return c when (c >= 65 and c <= 90) or (c >= 97 and c <= 122)
  return -1 when c == 126

  c + 256
}

pure ver_digit(text: Str, at: Int) -> Bool {
  let c = text.byte_at(at) ?? 0

  c >= 48 and c <= 57
}

pure ver_rev_cmp(a: Str, a_len: Int, b: Str, b_len: Int) -> Int {
  var pa = 0
  var pb = 0

  while pa < a_len or pb < b_len {
    var first_diff = 0

    while (pa < a_len and ! ver_digit(a, pa)) or (pb < b_len and ! ver_digit(b, pb)) {
      let ca = ver_order(a, pa, a_len)
      let cb = ver_order(b, pb, b_len)

      return ca - cb when ca != cb

      pa += 1
      pb += 1
    }

    while a.byte_at(pa) == 48 {
      pa += 1
    }

    while b.byte_at(pb) == 48 {
      pb += 1
    }

    while ver_digit(a, pa) and ver_digit(b, pb) {
      if first_diff == 0 {
        first_diff = (a.byte_at(pa) ?? 0) - (b.byte_at(pb) ?? 0)
      }

      pa += 1
      pb += 1
    }

    return 1 when ver_digit(a, pa)
    return -1 when ver_digit(b, pb)
    return first_diff when first_diff != 0
  }

  0
}

# The length of the name without its file suffix, per gnulib file_prefixlen.
pure ver_prefix_len(text: Str) -> Int {
  let total = text.byte_len()
  var prefix = 0
  var at = 0

  while at < total {
    at += 1
    prefix = at

    while at + 1 < total and text.byte_at(at) == 46 and ver_alpha_tilde(text.byte_at(at + 1) ?? 0) {
      at += 2

      while at < total and ver_alnum_tilde(text.byte_at(at) ?? 0) {
        at += 1
      }
    }
  }

  prefix
}

pure ver_alpha_tilde(c: Int) -> Bool {
  (c >= 65 and c <= 90) or (c >= 97 and c <= 122) or c == 126
}

pure ver_alnum_tilde(c: Int) -> Bool {
  (c >= 48 and c <= 57) or ver_alpha_tilde(c)
}

pure file_ver_cmp(a: Str, b: Str) -> Int {
  let a_len = a.byte_len()
  let b_len = b.byte_len()

  return if b_len == 0 { 0 } else { -1 } when a_len == 0
  return 1 when b_len == 0

  if a.starts_with(".") {
    return -1 when ! b.starts_with(".")

    let a_dot = a_len == 1
    let b_dot = b_len == 1

    return if b_dot { 0 } else { -1 } when a_dot
    return 1 when b_dot

    let a_dotdot = a == ".."
    let b_dotdot = b == ".."

    return if b_dotdot { 0 } else { -1 } when a_dotdot
    return 1 when b_dotdot
  } else if b.starts_with(".") {
    return 1
  }

  let a_prefix = ver_prefix_len(a)
  let b_prefix = ver_prefix_len(b)
  let first = ver_rev_cmp(a, a_prefix, b, b_prefix)

  return first when first != 0 or (a_prefix == a_len and b_prefix == b_len)

  ver_rev_cmp(a, a_len, b, b_len)
}

pure version_less_equal(a: File, b: File) -> Bool {
  let order = file_ver_cmp(a.name, b.name)

  if order != 0 { order < 0 } else { a.name <= b.name }
}

pure merge_version(items: List[File]) -> List[File] {
  return items when items.len() <= 1

  let middle = items.len() / 2
  let left = merge_version(items[..middle])
  let right = merge_version(items[middle..])
  var out: List[File] = []
  var l = 0
  var r = 0

  while l < left.len() and r < right.len() {
    if version_less_equal(left[l], right[r]) {
      out += [left[l]]
      l += 1
    } else {
      out += [right[r]]
      r += 1
    }
  }

  out + left[l..] + right[r..]
}

pure extension_of(name: Str) -> Str {
  let pieces = name.split(".")

  return "" when pieces.len() < 2 or name == ".."

  let tail = pieces[pieces.len() - 1]
  let cut = name.byte_len() - tail.byte_len() - 1

  if cut <= 0 { "" } else { name.byte_slice(cut) }
}

pure file_time(ctx: Ctx, f: File) -> Int {
  match ctx.cfg.time {
    "ctime" => f.st.ctime_ns
    "atime" => f.st.atime_ns
    "birth" => f.st.birth_ns ?? -1
    else => f.st.mtime_ns
  }
}

pure by_name(files: List[File]) -> List[File] {
  files |> sort-by .name
}

pure reversed(files: List[File]) -> List[File] {
  let total = files.len()

  [files[total - 1 - at] for at in range(total)]
}

pure sort_files(ctx: Ctx, files: List[File]) -> List[File] {
  return files when ctx.sort == "none" or files.len() < 2

  var sorted = by_name(files)

  match ctx.sort {
    "size" => sorted = sorted |> sort-by { |f| 0 - f.st.size }
    "time" => sorted = sorted |> sort-by { |f| 0 - file_time(ctx, f) }
    "extension" => sorted = sorted |> sort-by { |f| extension_of(f.name) }
    "width" => sorted = sorted |> sort-by { |f| f.name.count_chars() }
    "version" => sorted = merge_version(sorted)
    else => sorted = sorted
  }

  if ctx.cfg.reverse {
    sorted = reversed(sorted)
  }

  if ctx.cfg.dirs_first {
    sorted = [f for f in sorted if f.kind == "dir"] + [f for f in sorted if f.kind != "dir"]
  }

  sorted
}

pure ind_get(colors: Colors, code: Str) -> Str? {
  match colors.ind.get(code) {
    Ok(text) => text
    Err(_) => null
  }
}

# GNU is_colored: set, and not the "no attributes" codes 0 and 00.
pure is_colored(colors: Colors, code: Str) -> Bool {
  let seq = ind_get(colors, code)

  seq != null and seq != "" and seq != "0" and seq != "00"
}

# lc rs rc: the reset ls writes before the first colored output and after
# every colored name (or `ec` when the user defines one).
pure prep_text(colors: Colors) -> Str {
  let end = ind_get(colors, "ec")

  if end != null {
    return end ?? ""
  }

  f"{ind_get(colors, "lc") ?? ""}{ind_get(colors, "rs") ?? ""}{ind_get(colors, "rc") ?? ""}"
}

# Text for an indicator that may be the first color output.
pure with_first(colors: Colors, used: Bool, text: Str) -> Str {
  if used { text } else { f"{prep_text(colors)}{text}" }
}

# The sequence ls would start a name with (GNU print_color_indicator), or null
# when it has none. `target` colors the name after "->" of a long listing.
pure color_for(ctx: Ctx, f: File, target: Bool) -> Str? {
  let colors = ctx.colors
  let followed = colors.referent and f.linkok and ! target
  let kind = if target { f.linkkind } else if followed { f.linkkind } else { f.kind }
  let mode = if target { f.linkmode } else if followed { f.linkmode } else { f.st.mode }
  let name = if target { lossy(f.link ?? b"") } else { f.name }
  var code = "fi"

  if target and ! f.linkok and is_colored(colors, "mi") {
    code = "mi"
  } else if kind == "file" {
    if mode_has(mode, 2048) and is_colored(colors, "su") {
      code = "su"
    } else if mode_has(mode, 1024) and is_colored(colors, "sg") {
      code = "sg"
    } else if mode.bit_and(73) != 0 and is_colored(colors, "ex") {
      code = "ex"
    } else if f.st.nlink > 1 and is_colored(colors, "mh") {
      code = "mh"
    }
  } else if kind == "dir" {
    code = "di"

    if mode_has(mode, 512) and mode_has(mode, 2) and is_colored(colors, "tw") {
      code = "tw"
    } else if mode_has(mode, 2) and is_colored(colors, "ow") {
      code = "ow"
    } else if mode_has(mode, 512) and is_colored(colors, "st") {
      code = "st"
    }
  } else if kind == "symlink" {
    code = "ln"
  } else if kind == "fifo" {
    code = "pi"
  } else if kind == "socket" {
    code = "so"
  } else if kind == "block" {
    code = "bd"
  } else if kind == "char" {
    code = "cd"
  } else {
    code = "or"
  }

  if code == "ln" and ! f.linkok and ! target {
    if colors.referent or is_colored(colors, "or") {
      code = "or"
    }
  }

  if code == "fi" {
    let lowered = name.lower()

    for ext in colors.exts {
      continue when ext.ext.byte_len() > name.byte_len()

      let hit = if ext.exact { name.ends_with(ext.ext) } else { lowered.ends_with(ext.ext.lower()) }

      if hit {
        return ext.seq
      }
    }
  }

  ind_get(colors, code)
}

pure rpad(text: Str, width: Int) -> Str {
  let gap = width - text.count_chars()

  if gap > 0 { f"{spaces(gap)}{text}" } else { text }
}

pure lpad(text: Str, width: Int) -> Str {
  let gap = width - text.count_chars()

  if gap > 0 { f"{text}{spaces(gap)}" } else { text }
}

pure spaces(count: Int) -> Str {
  var out = ""

  for _ in range(count) {
    out = f"{out} "
  }

  out
}

type Widths = {
  inode: Int,
  blocks: Int,
  owner: Int,
  group: Int,
  context: Int,
  nlink: Int,
  size: Int,
  major: Int,
  minor: Int,
  users: Map[Str],
  groups: Map[Str],
}

pure digits(n: Int) -> Int {
  f"{n}".byte_len()
}

pure blocks_text(ctx: Ctx, f: File) -> Str {
  if f.ok { human_readable(f.st.blocks_512, 512, ctx.block_size, ctx.human) } else { "?" }
}

pure is_device(f: File) -> Bool {
  f.ok and (f.kind == "char" or f.kind == "block")
}

pure max_of(a: Int, b: Int) -> Int {
  if a > b { a } else { b }
}

# Column widths for a set of files that are printed together, and the owner
# and group names they need (looked up once per distinct id).
proc listing_widths(ctx: Ctx, files: List[File]) [fs] -> Widths {
  var w: Widths = {inode: 0, blocks: 0, owner: 0, group: 0, context: 0, nlink: 0, size: 0, major: 0, minor: 0, users: {}, groups: {}}
  let long = ctx.format == "long"
  let cfg = ctx.cfg
  var users = w.users
  var groups = w.groups
  var size_width = 0

  for f in files {
    continue when ! f.ok

    if cfg.inode {
      w.inode = max_of(w.inode, digits(f.st.ino))
    }

    if cfg.size or long {
      w.blocks = max_of(w.blocks, blocks_text(ctx, f).byte_len())
    }

    continue when ! long

    w.nlink = max_of(w.nlink, digits(f.st.nlink))

    if cfg.owner or cfg.author {
      let key = f"{f.st.uid}"

      if ! cfg.numeric and ! (key in users) {
        users = users.set(key, user_name(f.st.uid))
      }

      let known = if cfg.numeric { "" } else { users[key] }
      w.owner = max_of(w.owner, (if known == "" { key } else { known }).count_chars())
    }

    if cfg.group {
      let key = f"{f.st.gid}"

      if ! cfg.numeric and ! (key in groups) {
        groups = groups.set(key, group_name(f.st.gid))
      }

      let known = if cfg.numeric { "" } else { groups[key] }
      w.group = max_of(w.group, (if known == "" { key } else { known }).count_chars())
    }

    if cfg.context {
      w.context = max_of(w.context, 1)
    }

    if is_device(f) {
      w.major = max_of(w.major, digits(fs.dev_major(f.st.rdev)))
      w.minor = max_of(w.minor, digits(fs.dev_minor(f.st.rdev)))
    } else {
      size_width = max_of(size_width, human_readable(f.st.size, 1, ctx.file_block_size, ctx.file_human).byte_len())
    }
  }

  w.users = users
  w.groups = groups
  w.size = if w.major > 0 { max_of(size_width, w.major + 2 + w.minor) } else { size_width }
  w
}

proc user_name(uid: Int) [fs] -> Str {
  match user.by_uid(uid) {
    Ok(found) => found.name
    Err(_) => ""
  }
}

proc group_name(gid: Int) [fs] -> Str {
  match group.by_gid(gid) {
    Ok(found) => found.name
    Err(_) => ""
  }
}

pure mode_string(f: File) -> Str {
  let letter = kind_letter(f.kind)

  return f"{letter}?????????" when ! f.ok

  let mode = f.st.mode
  let user_x = if mode_has(mode, 2048) { if mode_has(mode, 64) { "s" } else { "S" } } else if mode_has(mode, 64) { "x" } else { "-" }
  let group_x = if mode_has(mode, 1024) { if mode_has(mode, 8) { "s" } else { "S" } } else if mode_has(mode, 8) { "x" } else { "-" }
  let other_x = if mode_has(mode, 512) { if mode_has(mode, 1) { "t" } else { "T" } } else if mode_has(mode, 1) { "x" } else { "-" }
  let r1 = if mode_has(mode, 256) { "r" } else { "-" }
  let w1 = if mode_has(mode, 128) { "w" } else { "-" }
  let r2 = if mode_has(mode, 32) { "r" } else { "-" }
  let w2 = if mode_has(mode, 16) { "w" } else { "-" }
  let r3 = if mode_has(mode, 4) { "r" } else { "-" }
  let w3 = if mode_has(mode, 2) { "w" } else { "-" }

  f"{letter}{r1}{w1}{user_x}{r2}{w2}{group_x}{r3}{w3}{other_x}"
}

# The character ls -F / -p / --file-type appends, or "".
pure type_indicator(ctx: Ctx, ok: Bool, kind: Str, mode: Int) -> Str {
  let style = ctx.indicator

  return "" when style == "none"

  if kind == "file" {
    return if ok and style == "classify" and mode.bit_and(73) != 0 { "*" } else { "" }
  }

  return "/" when kind == "dir"
  return "" when style == "slash"

  match kind {
    "symlink" => "@"
    "fifo" => "|"
    "socket" => "="
    else => ""
  }
}

pure url_escape(raw: Bytes) -> Str {
  var out = ""
  let hex = "0123456789abcdef"

  for byte in raw {
    let keep = (byte >= 48 and byte <= 57) or (byte >= 65 and byte <= 90) or (byte >= 97 and byte <= 122) or byte == 47 or byte == 45 or byte == 95 or byte == 46 or byte == 126

    if keep {
      out = f"{out}{(bytes.from_ints([byte]) ?? b"").utf8() ?? ""}"
    } else {
      out = f"{out}%{hex.byte_slice(byte / 16, length: 1)}{hex.byte_slice(byte % 16, length: 1)}"
    }
  }

  out
}

pure hyperlink_start(ctx: Ctx, full: Bytes) -> Bytes {
  let absolute = if full.byte_at(0) == 47 { full } else { join_raw(bytes.from_text(ctx.cwd), full) }
  let clean = raw_path(absolute).normalize().bytes()

  bytes.from_text(f"\u{1b}]8;;file://{ctx.host}{url_escape(clean)}\u{1b}\\")
}

const HYPERLINK_END = b"\x1b]8;;\x1b\\"

type NamePiece = {pre: Bytes, name: Bytes, post: Bytes, used: Bool, w: Int}

# GNU print_name_with_quoting: the color start, an alignment space, the
# hyperlink, the quoted name, and the closing sequences. `col` is where the
# name starts on its line.
pure name_piece(ctx: Ctx, f: File, target: Bool, pad0: Bool, used0: Bool, col: Int) -> NamePiece {
  let shown = if target { quote_name(ctx, f.link ?? b"", ctx.qs) } else { Shown(f.q, f.qw, f.quoted) }
  let pad = pad0 and (target or ! f.quoted)
  let colors = ctx.colors
  var used = used0
  var pre = ""
  var post = ""

  if ctx.color {
    let seq = color_for(ctx, f, target)
    let norm = is_colored(colors, "no")

    if seq != null {
      let restore = if norm { prep_text(colors) } else { "" }
      pre = with_first(colors, used, f"{restore}{ind_get(colors, "lc") ?? ""}{seq ?? ""}{ind_get(colors, "rc") ?? ""}")
      used = true
    }

    if seq != null or norm {
      post = with_first(colors, used, prep_text(colors))
      used = true

      let cl = ind_get(colors, "cl")

      if ctx.line_length > 0 and cl != null and col / ctx.line_length != (col + shown.w - 1) / ctx.line_length {
        post = f"{post}{cl ?? ""}"
      }
    }
  }

  var before = bytes.from_text(f"{pre}{if pad { " " } else { "" }}")
  var after = b""

  if ctx.hyper {
    let full = if target {
      join_raw(base_dir(f.path.bytes()), f.link ?? b"")
    } else {
      f.path.bytes()
    }

    before = bytes.concat([before, hyperlink_start(ctx, full)])
    after = HYPERLINK_END
  }

  {pre: before, name: shown.q, post: bytes.concat([after, bytes.from_text(post)]), used: used, w: shown.w + (if pad { 1 } else { 0 })}
}

pure norm_text(ctx: Ctx, used: Bool) -> Str {
  if ctx.color and is_colored(ctx.colors, "no") {
    with_first(ctx.colors, used, f"{ind_get(ctx.colors, "lc") ?? ""}{ind_get(ctx.colors, "no") ?? ""}{ind_get(ctx.colors, "rc") ?? ""}")
  } else {
    ""
  }
}

pure frills_text(ctx: Ctx, f: File, wd: Widths) -> Str {
  let cfg = ctx.cfg
  var out = ""

  if cfg.inode {
    out = f"{rpad(if f.ok { f"{f.st.ino}" } else { "?" }, wd.inode)} "
  }

  if cfg.size {
    out = f"{out}{rpad(blocks_text(ctx, f), wd.blocks)} "
  }

  if cfg.context {
    out = f"{out}{lpad("?", wd.context)} "
  }

  out
}

pure indicator_for(ctx: Ctx, f: File) -> Str {
  type_indicator(ctx, f.ok, f.kind, f.st.mode)
}

# The width of a name with its inode, block count, and type indicator.
pure name_len(ctx: Ctx, f: File, wd: Widths, pad: Bool) -> Int {
  frills_text(ctx, f, wd).byte_len() + f.qw + (if pad and ! f.quoted { 1 } else { 0 }) + indicator_for(ctx, f).byte_len()
}

type FilePiece = {bytes: Bytes, used: Bool}

pure file_piece(ctx: Ctx, f: File, wd: Widths, pad: Bool, used0: Bool, col: Int) -> FilePiece {
  let lead = norm_text(ctx, used0)
  let used1 = used0 or lead != ""
  let frills = frills_text(ctx, f, wd)
  let np = name_piece(ctx, f, false, pad, used1, col + frills.byte_len())

  {bytes: bytes.concat([bytes.from_text(f"{lead}{frills}"), np.pre, np.name, np.post, bytes.from_text(indicator_for(ctx, f))]), used: np.used}
}

# GNU indent(): spaces, or a tab where a tab stop is crossed.
pure indent_text(from: Int, to: Int, tabsize: Int) -> Str {
  var out = ""
  var at = from

  while at < to {
    if tabsize != 0 and to / tabsize > (at + 1) / tabsize {
      out = f"{out}\t"
      at += tabsize - at % tabsize
    } else {
      out = f"{out} "
      at += 1
    }
  }

  out
}

type Layout = {bytes: Bytes, used: Bool}

# Commas, and columns/across with no width limit: names joined by a
# separator, wrapping at the line length.
pure print_separated(ctx: Ctx, files: List[File], wd: Widths, pad: Bool, used0: Bool, sep: Str) -> Layout {
  var chunks: List[Bytes] = []
  var used = used0
  var pos = 0

  for index in range(files.len()) {
    let f = files[index]
    let len = if ctx.line_length > 0 { name_len(ctx, f, wd, pad) } else { 0 }

    if index != 0 {
      if ctx.line_length == 0 or pos + len + 2 <= ctx.line_length {
        pos += 2
        chunks += [bytes.from_text(f"{sep} ")]
      } else {
        pos = 0
        chunks += [bytes.from_text(f"{sep}\n")]
      }
    }

    let piece = file_piece(ctx, f, wd, pad, used, pos)
    chunks += [piece.bytes]
    used = piece.used
    pos += len
  }

  chunks += [bytes.from_text(ctx.eol)]

  {bytes: bytes.concat(chunks), used: used}
}

pure print_grid(ctx: Ctx, files: List[File], wd: Widths, pad: Bool, used0: Bool, by_columns: Bool) -> Layout {
  let total = files.len()
  let lens = [name_len(ctx, f, wd, pad) for f in files]
  let line_length = ctx.line_length
  let max_cols = max_of(1, if line_length / 3 < total { line_length / 3 } else { total })
  var valid = [true for _ in range(max_cols)]
  var line_len = [(i + 1) * 3 for i in range(max_cols)]
  var col_arr = [[3 for _ in range(i + 1)] for i in range(max_cols)]

  for filesno in range(total) {
    for i in range(max_cols) {
      continue when ! valid[i]

      let idx = if by_columns { filesno / ((total + i) / (i + 1)) } else { filesno % (i + 1) }
      let real = lens[filesno] + (if idx == i { 0 } else { 2 })

      if col_arr[i][idx] < real {
        line_len[i] += real - col_arr[i][idx]
        col_arr[i][idx] = real
        valid[i] = line_len[i] <= line_length
      }
    }
  }

  var cols = max_cols

  while cols > 1 and ! valid[cols - 1] {
    cols -= 1
  }

  let widths = col_arr[cols - 1]
  var chunks: List[Bytes] = []
  var used = used0

  if by_columns {
    let rows = total / cols + (if total % cols != 0 { 1 } else { 0 })

    for row in range(rows) {
      var col = 0
      var filesno = row
      var pos = 0

      loop {
        let piece = file_piece(ctx, files[filesno], wd, pad, used, pos)
        chunks += [piece.bytes]
        used = piece.used
        let name_length = lens[filesno]
        let max_length = widths[col]
        col += 1
        filesno += rows

        break when filesno >= total

        chunks += [bytes.from_text(indent_text(pos + name_length, pos + max_length, ctx.tabsize))]
        pos += max_length
      }

      chunks += [bytes.from_text(ctx.eol)]
    }
  } else {
    var pos = 0
    var name_length = lens[0]
    var max_length = widths[0]
    let first = file_piece(ctx, files[0], wd, pad, used, 0)
    chunks += [first.bytes]
    used = first.used

    for filesno in range(1, total) {
      let col = filesno % cols

      if col == 0 {
        chunks += [bytes.from_text(ctx.eol)]
        pos = 0
      } else {
        chunks += [bytes.from_text(indent_text(pos + name_length, pos + max_length, ctx.tabsize))]
        pos += max_length
      }

      let piece = file_piece(ctx, files[filesno], wd, pad, used, pos)
      chunks += [piece.bytes]
      used = piece.used
      name_length = lens[filesno]
      max_length = widths[col]
    }

    chunks += [bytes.from_text(ctx.eol)]
  }

  {bytes: bytes.concat(chunks), used: used}
}

pure floor_nanos(t: Int) -> Int {
  floor_div(t, 1000000000)
}

# The timestamp column for a long listing.
pure time_text(ctx: Ctx, f: File) -> Str {
  let expected = format_time(ctx.time_fmt[0], floor_nanos(ctx.now), 0, ctx.tz).count_chars()

  return rpad("?", expected) when ! f.ok or (ctx.cfg.time == "birth" and f.st.birth_ns == null)

  let t = file_time(ctx, f)
  let seconds = floor_nanos(t)
  let recent = ctx.now - 15778476000000000 < t and t < ctx.now

  format_time(ctx.time_fmt[if recent { 1 } else { 0 }], seconds, t - seconds * 1000000000, ctx.tz)
}

pure owner_text(ctx: Ctx, wd: Widths, f: File, by_group: Bool) -> Str {
  let width = if by_group { wd.group } else { wd.owner }

  return f"{lpad("?", width)} " when ! f.ok

  let id = if by_group { f.st.gid } else { f.st.uid }
  let key = f"{id}"
  let known = if ctx.cfg.numeric { "" } else if by_group { wd.groups.get(key) ?? "" } else { wd.users.get(key) ?? "" }

  if known == "" { f"{rpad(key, width)} " } else { f"{lpad(known, width)} " }
}

type LongLine = {bytes: Bytes, used: Bool, start: Int, end: Int}

proc long_line(ctx: Ctx, f: File, wd: Widths, pad: Bool, used0: Bool) [fs] -> LongLine {
  let cfg = ctx.cfg
  let lead = norm_text(ctx, used0)
  var head = f"{lead}{if cfg.dired { "  " } else { "" }}"

  if cfg.inode {
    head = f"{head}{rpad(if f.ok { f"{f.st.ino}" } else { "?" }, wd.inode)} "
  }

  if cfg.size {
    head = f"{head}{rpad(blocks_text(ctx, f), wd.blocks)} "
  }

  head = f"{head}{mode_string(f)} {rpad(if f.ok { f"{f.st.nlink}" } else { "?" }, wd.nlink)} "

  if cfg.owner {
    head = f"{head}{owner_text(ctx, wd, f, false)}"
  }

  if cfg.group {
    head = f"{head}{owner_text(ctx, wd, f, true)}"
  }

  if cfg.author {
    head = f"{head}{owner_text(ctx, wd, f, false)}"
  }

  if cfg.context {
    head = f"{head}{lpad("?", wd.context)} "
  }

  if is_device(f) {
    let major = fs.dev_major(f.st.rdev)
    let minor = fs.dev_minor(f.st.rdev)
    let blanks = wd.size - (wd.major + 2 + wd.minor)

    head = f"{head}{rpad(f"{major}", wd.major + max_of(0, blanks))}, {rpad(f"{minor}", wd.minor)} "
  } else {
    let size = if f.ok { human_readable(f.st.size, 1, ctx.file_block_size, ctx.file_human) } else { "?" }
    head = f"{head}{rpad(size, wd.size)} "
  }

  head = f"{head}{time_text(ctx, f)} "

  let used1 = used0 or lead != ""
  let np = name_piece(ctx, f, false, pad, used1, head.count_chars())
  var pieces: List[Bytes] = [bytes.from_text(head), np.pre, np.name, np.post]
  var used = np.used

  if f.kind == "symlink" {
    if f.link != null {
      let target = name_piece(ctx, f, true, false, used, head.count_chars() + np.w + 4)
      used = target.used
      pieces += [b" -> ", target.pre, target.name, target.post]
      pieces += [bytes.from_text(type_indicator(ctx, true, f.linkkind, f.linkmode))]
    }
  } else {
    pieces += [bytes.from_text(indicator_for(ctx, f))]
  }

  let start = head.byte_len() + np.pre.len()

  {bytes: bytes.concat(pieces), used: used, start: start, end: start + np.name.len()}
}

type Dirent = {raw: Bytes, kind: Str}

# A directory entry's raw name: the lossy text unless it holds an undecodable
# byte, in which case the path's own bytes.
pure entry_raw(name: Str, entry_path: Path) -> Bytes {
  if name.find("\u{fffd}") == null { bytes.from_text(name) } else { base_raw(entry_path.bytes()) }
}

type Pending = {raw: Bytes, arg: Bool, marker: Bool}

type Rendered = {bytes: Bytes, used: Bool, dired: List[Int]}

proc render_files(ctx: Ctx, files: List[File], used0: Bool) [fs] -> Rendered {
  let cfg = ctx.cfg
  var wd = listing_widths(ctx, files)

  if ctx.format == "commas" {
    wd = {...wd, inode: 0, blocks: 0, context: 0}
  }

  let pad = ctx.align and [f for f in files if f.quoted].len() > 0
  var chunks: List[Bytes] = []
  var used = used0
  var dired: List[Int] = []

  match ctx.format {
    "long" => {
      var offset = 0

      for f in files {
        let line = long_line(ctx, f, wd, pad, used)
        used = line.used
        chunks += [line.bytes, bytes.from_text(ctx.eol)]

        if cfg.dired {
          dired += [offset + line.start, offset + line.end]
        }

        offset += line.bytes.len() + 1
      }
    }
    "single-column" => {
      for f in files {
        let piece = file_piece(ctx, f, wd, pad, used, 0)
        used = piece.used
        chunks += [piece.bytes, bytes.from_text(ctx.eol)]
      }
    }
    "commas" => {
      let laid = print_separated(ctx, files, wd, pad, used, ",")
      used = laid.used
      chunks += [laid.bytes]
    }
    else => {
      let laid = if ctx.line_length == 0 {
        print_separated(ctx, files, wd, pad, used, " ")
      } else {
        print_grid(ctx, files, wd, pad, used, ctx.format == "columns")
      }

      used = laid.used
      chunks += [laid.bytes]
    }
  }

  {bytes: bytes.concat(chunks), used: used, dired: dired}
}

pure file_ignored(ctx: Ctx, name: Str) -> Bool {
  let mode = ctx.cfg.ignore_mode
  let hidden = name.starts_with(".") and (mode == "default" or name == "." or name == "..")

  (mode != "all" and hidden) or (mode == "default" and glob_matches(ctx.hide_globs, name)) or glob_matches(ctx.ignore_globs, name)
}

proc take_entry(ctx: Ctx, dir: Bytes, raw: Bytes, kind: Str) [fs, process, env] -> Gobbled {
  if file_ignored(ctx, lossy(raw)) {
    return {file: null, status: 0}
  }

  gobble(ctx, raw, dir, false, kind)
}

proc flush(out: List[Bytes]) [process, env, io] -> Unit {
  if out.len() > 0 {
    gnu.write_bytes(bytes.concat(out))
  }
}

proc list_all(ctx: Ctx, operands: List[Str]) [fs, process, env, io, error] -> Int {
  let cfg = ctx.cfg
  let eol = bytes.from_text(ctx.eol)
  let indent = if cfg.dired { "  " } else { "" }
  var status = 0
  var out: List[Bytes] = []
  var pos = 0
  var used = false
  var dired: List[Int] = []
  var subdired: List[Int] = []
  var first = true
  var print_name = true
  var pending: List[Pending] = []
  var active: List[Str] = []
  var files: List[File] = []

  if operands.len() == 0 {
    if cfg.directory {
      let g = gobble(ctx, b".", b"", true, "")
      status = max_of(status, g.status)

      if let found = g.file {
        files += [found]
      }
    } else {
      pending = [Pending(raw: b".", arg: true, marker: false)]
    }
  } else {
    for operand in operands {
      let g = gobble(ctx, bytes.from_text(operand), b"", true, "")
      status = max_of(status, g.status)

      if let found = g.file {
        files += [found]
      }
    }
  }

  if files.len() > 0 {
    files = sort_files(ctx, files)

    if ! cfg.directory {
      pending = [Pending(raw: f.raw, arg: true, marker: false) for f in files if f.kind == "dir"]
      files = [f for f in files if f.kind != "dir"]
    }
  }

  if files.len() > 0 {
    let r = render_files(ctx, files, used)
    used = r.used
    dired += [pos + x for x in r.dired]
    out += [r.bytes]
    pos += r.bytes.len()

    if pending.len() > 0 {
      out += [b"\n"]
      pos += 1
    }
  } else if operands.len() <= 1 and pending.len() == 1 {
    print_name = false
  }

  while pending.len() > 0 {
    let dir = pending[0]
    pending = pending[1..]

    if dir.marker {
      active = active[..active.len() - 1]
      continue
    }

    let display = lossy(dir.raw)
    let target = raw_path(dir.raw)

    let opened = fs.children(target)

    guard let lister = opened else { |problem|
      gnu.cannot("open directory", display, problem)
      status = max_of(status, if dir.arg { 2 } else { 1 })
      continue
    }

    if cfg.recursive {
      match fs.stat(target, true) {
        Ok(found) => {
          let key = f"{found.dev}:{found.ino}"

          if key in active {
            gnu.error(f"{gnu.quote_maybe(display)}: not listing already-listed directory")
            status = 2
            continue
          }

          active += [key]
        }
        Err(problem) => {
          gnu.cannot("determine device and inode of", display, problem)
          status = max_of(status, if dir.arg { 2 } else { 1 })
          continue
        }
      }
    }

    if cfg.recursive or print_name {
      if ! first {
        out += [b"\n"]
        pos += 1
      }

      first = false

      let shown = quote_name(ctx, dir.raw, ctx.dir_qs)
      let start = if ctx.hyper { hyperlink_start(ctx, dir.raw) } else { b"" }
      let before = bytes.concat([bytes.from_text(indent), start])
      let after = bytes.concat([if ctx.hyper { HYPERLINK_END } else { b"" }, b":\n"])

      subdired += [pos + before.len(), pos + before.len() + shown.q.len()]
      out += [before, shown.q, after]
      pos += before.len() + shown.q.len() + after.len()
    }

    var listing: List[File] = []
    var total = 0
    var names: List[Dirent] = []

    if cfg.ignore_mode == "all" {
      names = [Dirent(b".", "dir"), Dirent(b"..", "dir")]
    }

    for entry in names {
      let g = take_entry(ctx, dir.raw, entry.raw, entry.kind)
      status = max_of(status, g.status)

      if let found = g.file {
        listing += [found]
        total += found.st.blocks_512
      }
    }

    for entry in lister {
      let g = take_entry(ctx, dir.raw, entry_raw(entry.name, entry.path), entry.kind)
      status = max_of(status, g.status)

      if let found = g.file {
        listing += [found]
        total += found.st.blocks_512
      }
    }

    listing = sort_files(ctx, listing)

    if cfg.recursive {
      let subs = [Pending(raw: join_raw(dir.raw, f.raw), arg: false, marker: false) for f in listing if f.kind == "dir" and f.name != "." and f.name != ".."]
      pending = subs + [Pending(raw: b"", arg: false, marker: true)] + pending
    }

    if ctx.format == "long" or cfg.size {
      let line = bytes.from_text(f"{indent}total {human_readable(total, 512, ctx.block_size, ctx.human)}\n")
      out += [line]
      pos += line.len()
    }

    if listing.len() > 0 {
      let r = render_files(ctx, listing, used)
      used = r.used
      dired += [pos + x for x in r.dired]
      out += [r.bytes]
      pos += r.bytes.len()
    }

    flush(out)
    out = []
  }

  flush(out)

  if cfg.dired {
    var trailer = ""

    if dired.len() > 0 {
      trailer = f"//DIRED// {[f"{x}" for x in dired].join(" ")}\n"
    }

    if subdired.len() > 0 {
      trailer = f"{trailer}//SUBDIRED// {[f"{x}" for x in subdired].join(" ")}\n"
    }

    gnu.write_text(f"{trailer}//DIRED-OPTIONS// --quoting-style={ctx.qs.name}\n")
  }

  status
}

const TIME_STYLE_NAMES = ["full-iso", "long-iso", "iso", "locale"]

# A name that equals one of `names` or is a unique prefix of one; null if none.
pure quiet_match(text: Str, names: List[Str]) -> Str? {
  for name in names {
    return name when name == text
  }

  let found = [name for name in names if name.starts_with(text)]

  if found.len() == 1 { found[0] } else { null }
}

# The [old, recent] strftime formats for the long listing.
proc time_formats(cfg: Cfg) [process, env] -> List[Str] {
  var style = cfg.time_style ?? env_text("TIME_STYLE") ?? "locale"
  var locale_only = false

  while style.starts_with("posix-") {
    if ! hard_time_locale() {
      locale_only = true
    }

    style = style[6..]
  }

  return ["%b %e  %Y", "%b %e %H:%M"] when locale_only

  if style.starts_with("+") {
    let lines = style[1..].split("\n")

    if lines.len() > 2 {
      gnu.error(f"invalid time style format {gnu.quote_value(style[1..])}")
      exit 2
    }

    return if lines.len() == 2 { lines } else { [lines[0], lines[0]] }
  }

  match quiet_match(style, TIME_STYLE_NAMES) {
    "full-iso" => ["%Y-%m-%d %H:%M:%S.%N %z", "%Y-%m-%d %H:%M:%S.%N %z"]
    "long-iso" => ["%Y-%m-%d %H:%M", "%Y-%m-%d %H:%M"]
    "iso" => ["%Y-%m-%d ", "%m-%d %H:%M"]
    "locale" => ["%b %e  %Y", "%b %e %H:%M"]
    else => {
      gnu.error(f"invalid argument {gnu.quote_value(style)} for 'time style'")
      eprint "Valid arguments are:"
      eprint "  - [posix-]full-iso"
      eprint "  - [posix-]long-iso"
      eprint "  - [posix-]iso"
      eprint "  - [posix-]locale"
      eprint "  - +FORMAT"
      gnu.try_help()
      exit 2
    }
  }
}

proc build_ctx(cfg0: Cfg, tty: Bool) [fs, process, env, time, io] -> Ctx {
  var cfg = cfg0
  let utf8 = utf8_locale()

  if cfg.dired and (cfg.format != "long" or cfg.hyperlink == "always") {
    cfg = {...cfg, dired: false}
  }

  if cfg.dired and cfg.zero {
    gnu.error("--dired and --zero are incompatible")
    exit 2
  }

  var line_length = 80
  let columns = env_text("COLUMNS") ?? ""

  if cfg.width == null and columns != "" {
    let parsed = parse_c_unsigned(columns)

    if parsed == null or (parsed ?? 0) <= 0 {
      gnu.error(f"ignoring invalid width in environment variable COLUMNS: {gnu.quote_value(columns)}")
    } else {
      line_length = parsed ?? 80
    }
  }

  line_length = cfg.width ?? line_length
  var tabsize = if tty { 0 } else { 8 }
  let tab_env = env_text("TABSIZE") ?? ""

  if cfg.tabsize == null and tab_env != "" {
    let parsed = parse_c_unsigned(tab_env)

    if parsed == null {
      gnu.error(f"ignoring invalid tab size in environment variable TABSIZE: {gnu.quote_value(tab_env)}")
    } else {
      tabsize = parsed ?? 8
    }
  }

  tabsize = cfg.tabsize ?? tabsize

  let style = cfg.quoting ?? (if tty and gnu.prog() == "ls" { "shell-escape" } else { "literal" })
  let extra = match cfg.indicator {
    "classify" => "*=>@|"
    "file-type" => "=>@|"
    else => ""
  }
  let curly = utf8
  let qs: QuoteStyle = {name: style, utf8: utf8, curly: curly, extra: extra, space: style == "escape"}
  let dir_qs: QuoteStyle = {name: style, utf8: utf8, curly: curly, extra: ":", space: false}

  var human = cfg.human
  var block_size = cfg.block_size ?? 0
  var file_human = cfg.file_human
  var file_block_size = cfg.file_block_size ?? 1

  if cfg.block_size == null {
    let default_size = if env.get("POSIXLY_CORRECT") is Ok(_) { 512 } else { 1024 }
    let ls_spec = env_text("LS_BLOCK_SIZE")
    let block_env = env_text("BLOCK_SIZE")
    let spec: Str? = if ls_spec != null { ls_spec } else if block_env != null { block_env } else { env_text("BLOCKSIZE") }
    block_size = default_size
    human = ""
    file_human = ""
    file_block_size = 1

    if let text = spec {
      let parsed = parse_block_size(text)

      if parsed.ok {
        human = parsed.human
        block_size = parsed.size
      }
    }

    if ls_spec != null or block_env != null {
      file_human = human
      file_block_size = block_size
    }

    if cfg.kibibytes {
      block_size = 1024
    }
  }

  var colors: Colors = {ok: true, ind: DEFAULT_COLORS, exts: [], referent: false, unknown: []}
  var color = cfg.color == "always"

  if color {
    let spec = env_text("LS_COLORS") ?? ""

    if spec != "" {
      colors = ls_color_parse(spec)

      for label in colors.unknown {
        gnu.error(f"unrecognized prefix: {gnu.quote_value(label)}")
      }

      if ! colors.ok {
        gnu.error("unparsable value for LS_COLORS environment variable")
        color = false
      }
    }
  }

  let long = cfg.format == "long"
  let deref = cfg.deref ?? (if cfg.directory or cfg.indicator == "classify" or long { "never" } else { "cmdline_dir" })
  let sort = cfg.sort ?? (if cfg.time_given and ! long { "time" } else { "name" })
  let check_symlink = color and (is_colored(colors, "or") or (is_colored(colors, "ex") and colors.referent) or (is_colored(colors, "mi") and long))
  let link_stat = cfg.indicator == "classify" or cfg.indicator == "file-type" or check_symlink
  let needs_stat = long or cfg.inode or cfg.size or cfg.context or sort == "size" or sort == "time" or color or cfg.indicator != "none" or cfg.recursive or cfg.dirs_first or cfg.hyperlink == "always"
  let hyper = cfg.hyperlink == "always"
  let align = cfg.format != "commas" and (style == "shell" or style == "shell-escape")
  var host = ""
  var cwd = ""

  if hyper {
    host = system.hostname() ?? ""
    cwd = (fs.cwd() ?? Path("/")).display()
  }

  let formats = if long { time_formats(cfg) } else { ["%b %e  %Y", "%b %e %H:%M"] }
  let ignore_globs = [compile_glob(pattern) for pattern in cfg.ignores]
  let hide_globs = [compile_glob(pattern) for pattern in cfg.hides]

  {
    cfg: cfg,
    format: cfg.format,
    utf8: utf8,
    qs: qs,
    dir_qs: dir_qs,
    color: color,
    colors: colors,
    hyper: hyper,
    host: host,
    cwd: cwd,
    line_length: line_length,
    tabsize: tabsize,
    eol: if cfg.zero { "\0" } else { "\n" },
    needs_stat: needs_stat,
    deref: deref,
    indicator: cfg.indicator,
    align: align,
    tz: load_tz(),
    now: time.now() * 1000000 + 999999,
    time_fmt: formats,
    human: human,
    block_size: block_size,
    file_human: file_human,
    file_block_size: file_block_size,
    check_symlink: check_symlink,
    link_stat: link_stat,
    sort: sort,
    qmark: cfg.hide_control,
    ignore_globs: ignore_globs,
    hide_globs: hide_globs,
  }
}

proc main(...argv: List[Str]) [fs, process, env, error, io, time] {
  let prog = gnu.prog()
  let tty = stdout_is_tty()
  var quoting: Str? = null

  match env.get("QUOTING_STYLE") {
    Ok(text) => {
      quoting = quiet_match(text, QUOTING_NAMES)

      if quoting == null {
        eprint f"{prog}: ignoring invalid value of environment variable QUOTING_STYLE: {gnu.quote_value(text)}"
      }
    }
    Err(problem) => {
      if problem.message.find("UTF-8") != null {
        let raw = (env.Path.QUOTING_STYLE ?? Path("")).bytes()
        eprint f"{prog}: ignoring invalid value of environment variable QUOTING_STYLE: {gnu.quote_value_bytes(raw)}"
      }
    }
  }

  if quoting == null and (prog == "dir" or prog == "vdir") {
    quoting = "escape"
  }

  let start: Cfg = {
    format_set: false,
    format: if prog == "dir" { "columns" } else if prog == "vdir" { "long" } else if tty { "columns" } else { "single-column" },
    ignore_mode: "default",
    sort: null,
    reverse: false,
    time: "mtime",
    time_given: false,
    recursive: false,
    directory: false,
    inode: false,
    size: false,
    owner: true,
    group: true,
    author: false,
    numeric: false,
    context: false,
    deref: null,
    indicator: "none",
    quoting: quoting,
    hide_control: prog == "ls" and tty,
    color: null,
    hyperlink: "never",
    dired: false,
    zero: false,
    width: null,
    tabsize: null,
    human: "",
    block_size: null,
    file_human: "",
    file_block_size: null,
    kibibytes: false,
    ignores: [],
    hides: [],
    dirs_first: false,
    time_style: null,
  }

  let parsed = parse_args(argv, start, tty)
  let ctx = build_ctx(parsed.cfg, tty)
  let status = list_all(ctx, parsed.operands)

  if status != 0 {
    exit status
  }
}
