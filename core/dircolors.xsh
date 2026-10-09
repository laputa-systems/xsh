#!/bin/xsh
use lib.gnu as gnu

const DEFAULT_DATABASE = r"""# Configuration file for dircolors, a utility to help you set the
# LS_COLORS environment variable used by GNU ls with the --color option.
# The keywords COLOR, OPTIONS, and EIGHTBIT (honored by the
# slackware version of dircolors) are recognized but ignored.
# Global config options can be specified before TERM or COLORTERM entries
# Below are TERM or COLORTERM entries, which can be glob patterns, which
# restrict following config to systems with matching environment variables.
COLORTERM ?*
TERM Eterm
TERM alacritty*
TERM ansi
TERM *color*
TERM con[0-9]*x[0-9]*
TERM cons25
TERM console
TERM cygwin
TERM *direct*
TERM dtterm
TERM foot
TERM gnome
TERM hurd
TERM jfbterm
TERM konsole
TERM kterm
TERM linux
TERM linux-c
TERM mlterm
TERM putty
TERM rxvt*
TERM screen*
TERM st
TERM terminator
TERM tmux*
TERM vt100
TERM wezterm*
TERM xterm*
# Below are the color init strings for the basic file types.
# One can use codes for 256 or more colors supported by modern terminals.
# The default color codes use the capabilities of an 8 color terminal
# with some additional attributes as per the following codes:
# Attribute codes:
# 00=none 01=bold 04=underscore 05=blink 07=reverse 08=concealed
# Text color codes:
# 30=black 31=red 32=green 33=yellow 34=blue 35=magenta 36=cyan 37=white
# Background color codes:
# 40=black 41=red 42=green 43=yellow 44=blue 45=magenta 46=cyan 47=white
#NORMAL 00 # no color code at all
#FILE 00 # regular file: use no color at all
RESET 0
DIR 01;34
LINK 01;36
MULTIHARDLINK 00
FIFO 40;33
SOCK 01;35
DOOR 01;35
BLK 40;33;01
CHR 40;33;01
ORPHAN 40;31;01
MISSING 00
SETUID 37;41
SETGID 30;43
CAPABILITY 00
STICKY_OTHER_WRITABLE 30;42
OTHER_WRITABLE 34;42
STICKY 37;44
EXEC 01;32
# List any file extensions like '.gz' or '.tar' that you would like ls
# to color below. Put the extension, a space, and the color init string.
.tar 01;31
.tgz 01;31
.arc 01;31
.arj 01;31
.taz 01;31
.lha 01;31
.lz4 01;31
.lzh 01;31
.lzma 01;31
.tlz 01;31
.txz 01;31
.tzo 01;31
.t7z 01;31
.zip 01;31
.z 01;31
.dz 01;31
.gz 01;31
.lrz 01;31
.lz 01;31
.lzo 01;31
.xz 01;31
.zst 01;31
.tzst 01;31
.bz2 01;31
.bz 01;31
.tbz 01;31
.tbz2 01;31
.tz 01;31
.deb 01;31
.rpm 01;31
.jar 01;31
.war 01;31
.ear 01;31
.sar 01;31
.rar 01;31
.alz 01;31
.ace 01;31
.zoo 01;31
.cpio 01;31
.7z 01;31
.rz 01;31
.cab 01;31
.wim 01;31
.swm 01;31
.dwm 01;31
.esd 01;31
.avif 01;35
.jpg 01;35
.jpeg 01;35
.mjpg 01;35
.mjpeg 01;35
.gif 01;35
.bmp 01;35
.pbm 01;35
.pgm 01;35
.ppm 01;35
.tga 01;35
.xbm 01;35
.xpm 01;35
.tif 01;35
.tiff 01;35
.png 01;35
.svg 01;35
.svgz 01;35
.mng 01;35
.pcx 01;35
.mov 01;35
.mpg 01;35
.mpeg 01;35
.m2v 01;35
.mkv 01;35
.webm 01;35
.webp 01;35
.ogm 01;35
.mp4 01;35
.m4v 01;35
.mp4v 01;35
.vob 01;35
.qt 01;35
.nuv 01;35
.wmv 01;35
.asf 01;35
.rm 01;35
.rmvb 01;35
.flc 01;35
.avi 01;35
.fli 01;35
.flv 01;35
.gl 01;35
.dl 01;35
.xcf 01;35
.xwd 01;35
.yuv 01;35
.cgm 01;35
.emf 01;35
.ogv 01;35
.ogx 01;35
.aac 00;36
.au 00;36
.flac 00;36
.m4a 00;36
.mid 00;36
.midi 00;36
.mka 00;36
.mp3 00;36
.mpc 00;36
.ogg 00;36
.ra 00;36
.wav 00;36
.oga 00;36
.opus 00;36
.spx 00;36
.xspf 00;36
*~ 00;90
*# 00;90
.bak 00;90
.old 00;90
.orig 00;90
.part 00;90
.rej 00;90
.swp 00;90
.tmp 00;90
.dpkg-dist 00;90
.dpkg-old 00;90
.ucf-dist 00;90
.ucf-new 00;90
.ucf-old 00;90
.rpmnew 00;90
.rpmorig 00;90
.rpmsave 00;90
# Subsequent TERM or COLORTERM entries, can be used to add / override
# config specific to those matching environment variables.
"""
const SHELL_USAGE = "Usage: dircolors [OPTION]... [FILE]\n\nOutput commands to set the LS_COLORS environment variable.\n\n  -b, --sh, --bourne-shell  output Bourne shell commands\n  -c, --csh, --c-shell     output C shell commands\n  -p, --print-database     output the default database\n      --print-ls-colors    output the color mapping for FILE\n      --help               display this help and exit\n      --version            output version information and exit"

type OutputFormat = {shell: Str}

pure white(c: Str) -> Bool { c == " " or c == "\t" }

pure purify(line: Str) -> Str {
  let at = line.find("#") ?? -1
  if at == 0 { return "" }
  if at > 0 and white(line.byte_slice(at - 1, length: 1)) {
    return line.byte_slice(0, length: at - 1).trim()
  }
  line.trim()
}

pure escaped_regex(text: Str) -> Str {
  if ".+(){}|^$*?[]\\-".find(text) != null { f"\\{text}" } else { text }
}

pure glob_regex(pattern: Str) -> Str? {
  let chars = [c for c in pattern]
  var out = "^"
  var at = 0
  while at < chars.len() {
    let c = chars[at]
    at += 1
    if c == "*" { out = f"{out}.*" } else if c == "?" { out = f"{out}." } else if c == "\\" and at < chars.len() { out = f"{out}{escaped_regex(chars[at])}"; at += 1 } else if c == "[" {
      var close = at
      if close < chars.len() and (chars[close] == "!" or chars[close] == "^") { close += 1 }
      while close < chars.len() and chars[close] != "]" { close += 1 }
      if close >= chars.len() { return null }
      var body = ""
      var inner = at
      if inner < close and (chars[inner] == "!" or chars[inner] == "^") { body = "^"; inner += 1 }
      while inner < close {
        let item = chars[inner]
        if item == "\\" or item == "[" { body = f"{body}\\{item}" } else { body = f"{body}{item}" }
        inner += 1
      }
      out = f"{out}[{body}]"
      at = close + 1
    } else { out = f"{out}{escaped_regex(c)}" }
  }
  f"{out}$"
}

proc glob_matches(pattern: Str, value: Str) [process, env] -> Bool {
  if let source = glob_regex(pattern) {
    if let Ok(compiled) = regex.compile(source) { return compiled.matches(value) }
  }
  false
}

pure escape_shell(value: Str) -> Str {
  var out = ""
  var previous = " "
  for c in value {
    if c == "'" { out = f"{out}'\\''" } else if c == ":" and previous != "\\" { out = f"{out}\\:" } else { out = f"{out}{c}" }
    previous = c
  }
  out
}

pure attribute_code(key: Str) -> Str? {
  match key.lower() {
    "normal" | "norm" => "no",
    "file" => "fi",
    "reset" => "rs",
    "dir" => "di",
    "link" | "lnk" | "symlink" => "ln",
    "orphan" => "or",
    "missing" => "mi",
    "fifo" | "pipe" => "pi",
    "sock" => "so",
    "blk" | "block" => "bd",
    "chr" | "char" => "cd",
    "door" => "do",
    "exec" => "ex",
    "left" | "leftcode" => "lc",
    "right" | "rightcode" => "rc",
    "end" | "endcode" => "ec",
    "suid" | "setuid" => "su",
    "sgid" | "setgid" => "sg",
    "sticky" => "st",
    "other_writable" | "owr" => "ow",
    "sticky_other_writable" | "owt" => "tw",
    "capability" => "ca",
    "multihardlink" => "mh",
    "clrtoeol" => "cl",
    _ => null,
  }
}

pure display_entry(code: Str, value: Str) -> Str {
  f"\u{001b}[{value}m{code}\t{value}\u{001b}[0m\n"
}

pure format_prefix(fmt: Str) -> Str {
  if fmt == "csh" { "setenv LS_COLORS '" } else if fmt == "display" { "" } else { "LS_COLORS='" }
}

pure format_suffix(fmt: Str) -> Str {
  if fmt == "csh" { "'" } else if fmt == "display" { "" } else { "';\nexport LS_COLORS" }
}

proc parse_database(text: Str, fmt: Str, source_name: Str, all: Bool, term: Str, colorterm: Str) [process, env] -> Result[Str, Str] {
  let lines = text.lines()
  var out = format_prefix(fmt)
  var state = "global"
  var saw_colorterm_match = false

  for index in range(lines.len()) {
    var line = purify(lines[index])
    continue when line == ""
    line = escape_shell(line)
    var split_at = -1
    var pos = 0
    while pos < line.byte_len() {
      let c = line.byte_slice(pos, length: 1)
      if white(c) { split_at = pos; break }
      pos += 1
    }
    if split_at < 0 { return Err(f"{source_name}:{index + 1}: missing token") }
    let key = line.byte_slice(0, length: split_at)
    var val_at = split_at
    while val_at < line.byte_len() and white(line.byte_slice(val_at, length: 1)) { val_at += 1 }
    let value = line.byte_slice(val_at)
    if value == "" { return Err(f"{source_name}:{index + 1}: missing token") }
    let lower = key.lower()

    if all and (lower == "term" or lower == "colorterm") { continue }

    if ! all and lower == "term" {
      if glob_matches(value, term) { state = "matched" } else if state == "global" { state = "pass" }
      continue
    }
    if ! all and lower == "colorterm" {
      let matches = if value == "?*" { colorterm != "" } else { glob_matches(value, colorterm) }
      if matches { state = "matched"; saw_colorterm_match = true } else if ! saw_colorterm_match and state == "global" { state = "pass" }
      continue
    }
    if all or state != "pass" {
      if lower == "color" or lower == "options" or lower == "eightbit" { continue }
      if key.starts_with(".") or key.starts_with("*") {
        let entry = if key.starts_with(".") { f"*{key}" } else { key }
        if fmt == "display" { out = f"{out}{display_entry(entry, value)}" } else { out = f"{out}{entry}={value}:" }
        continue
      }
      if let code = attribute_code(key) {
        if fmt == "display" { out = f"{out}{display_entry(code, value)}" } else { out = f"{out}{code}={value}:" }
      } else { return Err(f"{source_name}:{index + 1}: unrecognized keyword {gnu.quote_value(key)}") }
    }
    if state == "matched" { state = "continue" }
  }

  if fmt == "display" and out.ends_with("\n") { out = out.byte_slice(0, length: out.byte_len() - 1) }
  Ok(f"{out}{format_suffix(fmt)}")
}

proc default_ls_colors(fmt: Str) [process, env] -> Result[Str, Str] {
  parse_database(DEFAULT_DATABASE, fmt, "<internal>", true, "", "")
}

proc explicit_shell(args: List[Str]) -> Str? {
  var selected = ""
  for arg in args {
    if arg == "--sh" or arg == "--bourne-shell" {
      selected = "shell"
    } else if arg == "--csh" or arg == "--c-shell" {
      selected = "csh"
    } else if arg.starts_with("-") and ! arg.starts_with("--") {
      let short = arg.byte_slice(1)
      for char in short {
        if char == "b" { selected = "shell" }
        if char == "c" { selected = "csh" }
      }
    }
  }
  if selected == "" { null } else { selected }
}

proc choose_format(args: List[Str], display: Bool) [env] -> Str? {
  if let selected = explicit_shell(args) { return selected }
  return "display" when display
  let shell = env.get_or("SHELL", "") ?? ""
  return null when shell == ""
  let parts = shell.split("/")
  let name = parts[parts.len() - 1]
  if name == "csh" or name == "tcsh" { "csh" } else { "shell" }
}

proc main(...argv: List[Str]) [process, fs, io, env, error] {
  let options = cli.applet(
    argv,
    {
      gnu: {prog: "dircolors"},
      bourne: {form: "-b --sh --bourne-shell", default: false},
      c_shell: {form: "-c --csh --c-shell", default: false},
      print_database: {form: "-p --print-database", default: false},
      print_ls_colors: {form: "--print-ls-colors", default: false},
      help: {form: "--help", default: false, stop: true},
      version: {form: "--version", default: false, stop: true},
      files: {form: "...FILE"},
    },
  )?

  if options.help { gnu.help(SHELL_USAGE); return }
  if options.version { gnu.version("dircolors"); return }
  if ((options.bourne or options.c_shell) and (options.print_database or options.print_ls_colors)) or (options.print_database and options.print_ls_colors) {
    gnu.usage_error("options selecting a shell and output mode are mutually exclusive")
  }
  if options.print_database and options.files.len() > 0 { gnu.extra_operand(options.files[0]) }
  if options.files.len() > 1 { gnu.extra_operand(options.files[1]) }

  if options.print_database {
    gnu.write_text(f"{DEFAULT_DATABASE.trim()}\n")
    return
  }

  let chosen = choose_format(argv, options.print_ls_colors)
  if chosen == null { gnu.error("no SHELL environment variable, and no shell type option given"); exit 1 }
  let fmt = chosen ?? "shell"

  let output = if options.files.len() == 0 {
    match default_ls_colors(fmt) {
      Ok(text) => text,
      Err(message) => { gnu.error(message); exit 1 },
    }
  } else {
    let source_name = options.files[0]
    let input = if source_name == "-" {
      match io.stdin_text() { Ok(text) => text, Err(_) => { gnu.error("error reading '-'"); exit 1 } }
    } else {
      let file = fp"{source_name}"
      match file.metadata() {
        Ok(meta) if meta.kind == "dir" => { gnu.error(f"expected file, got directory {gnu.quote(source_name)}"); exit 1 },
        _ => {},
      }
      match file.read_text() {
        Ok(text) => text,
        Err(failure) => { gnu.cannot_open(source_name, failure); exit 1 },
      }
    }
    let term = env.get_or("TERM", "none") ?? "none"
    let colorterm = env.get_or("COLORTERM", "") ?? ""
    match parse_database(input, fmt, source_name, false, term, colorterm) {
      Ok(text) => text,
      Err(message) => { gnu.error(message); exit 1 },
    }
  }
  gnu.write_text(f"{output}\n")
}
