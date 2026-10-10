#!/bin/xsh
use lib.gnu
use lib.diffutils
use lib.search

type Options = {unified: Bool, context: Str?, long_context: Str?, brief: Bool, report_same: Bool, text: Bool, ignore_space_change: Bool, ignore_blank: Bool, label: Str?, recursive: Bool, new_file: Bool, exclude: List[Str], exclude_from: List[Str], help: Bool, version: Bool, files: List[Str]}

# Everything a comparison needs besides the two names. `switches` is the option
# text GNU diff repeats in the `diff -r a/f b/f` line it prints before the
# output of each file compared inside a directory.
type Settings = {opts: Options, wanted: Int, labels: List[Str], switches: Str, patterns: List[Str]}

# One operand or directory entry after `stat`. `kind` is the `fs.stat` kind,
# plus "stdin" for `-` and "absent" for a missing path that `-N` treats as an
# empty file or directory. `identity` is device and inode, so a directory
# reached again through a symlink is recognised.
type Probe = {name: Str, kind: Str, size: Int, identity: Str}

pure binary(data: Bytes) -> Bool {
  for index in range(data.len()) { if data.byte_at(index) == 0 { return true } }
  data.utf8() is Err(_)
}

pure labels(argv: List[Str]) -> List[Str] {
  var values: List[Str] = []
  var index = 0
  while index < argv.len() {
    let arg = argv[index]
    if arg == "--" { break }
    if arg.starts_with("--label=") { values += [arg.byte_slice(8)] } else if arg == "--label" or arg == "-L" { index += 1; values += [argv.get(index) ?? ""] } else if arg.starts_with("-L") { values += [arg.byte_slice(2)] }
    index += 1
  }
  values
}

# Whether option token `arg` takes its value from the next argv word: a long
# option written without `=`, or a short cluster whose value-taking letter is
# last (an attached value, as in `-x*.o`, ends the cluster earlier).
pure takes_next_word(arg: Str) -> Bool {
  if arg.starts_with("--") { return arg in ["--label", "--exclude", "--exclude-from"] }
  let letters = arg.byte_slice(1)
  for index in range(letters.byte_len()) {
    if letters.byte_slice(index, length: 1) in ["L", "x", "X", "U"] { return index == letters.byte_len() - 1 }
  }
  false
}

# The command-line options as typed, for the per-file `diff` line.
pure switch_text(argv: List[Str]) -> Str {
  var text = ""
  var index = 0
  while index < argv.len() {
    let arg = argv[index]
    if arg == "--" { break }
    if arg.starts_with("-") and arg != "-" {
      text += " " + arg
      if takes_next_word(arg) { index += 1; text += " " + (argv.get(index) ?? "") }
    }
    index += 1
  }
  text
}

pure join(dir: Str, name: Str) -> Str {
  if dir == "" or dir.ends_with("/") { dir + name } else { dir + "/" + name }
}

pure worse(first: Int, second: Int) -> Int {
  if first > second { first } else { second }
}

# GNU's wording for a file type in "File A is a T while file B is a T".
pure type_name(item: Probe) -> Str {
  match item.kind {
    "file" => if item.size == 0 { "regular empty file" } else { "regular file" }
    "dir" => "directory"
    "symlink" => "symbolic link"
    "fifo" => "fifo"
    "socket" => "socket"
    "block" => "block special file"
    "char" => "character special file"
    _ => "weird file"
  }
}

pure excluded(patterns: List[Str], name: Str) -> Bool {
  let raw = bytes.from_text(name)
  for pattern in patterns { if search.glob(pattern, raw) { return true } }
  false
}

proc probe(cfg: Settings, name: Str) [fs, error] -> Result[Probe, Error] {
  if name == "-" { return Ok({name: name, kind: "stdin", size: 0, identity: ""}) }
  match fs.stat(fp"{name}", follow_symlinks: true) {
    Ok(info) => Ok({name: name, kind: info.kind, size: info.size, identity: f"{info.dev}:{info.ino}"})
    Err(failure) => if cfg.opts.new_file and gnu.errno(failure) == 2 { Ok({name: name, kind: "absent", size: 0, identity: ""}) } else { Err(failure) }
  }
}

proc read_side(item: Probe) [fs, io, error] -> Result[Bytes, Error] {
  if item.kind == "absent" { return Ok(b"") }
  gnu.read_operand(item.name)
}

proc native_unified(left: Bytes, right: Bytes, context: Int) [fs, error] -> Result[Str, Error] {
  let scratch = fs.tempdir()?
  defer scratch.close()
  scratch.write(p"original", left)
  scratch.write(p"modified", right)
  let root = scratch.host_path()?
  Ok(diff.unified(fp"{root}/original", fp"{root}/modified", context: context)?.text)
}

# Compare two byte strings and print the result in the selected format.
# `entry` marks a comparison reached through a directory: those print the
# `diff` line first, and `-L` labels apply only to command-line files.
# Returns 0 (same), 1 (differ), or 2 (trouble).
proc compare_contents(cfg: Settings, left_name: Str, right_name: Str, left: Bytes, right: Bytes, entry: Bool) [fs, io, process, env, error] -> Int {
  let opts = cfg.opts
  if left == right {
    if opts.report_same { gnu.write_text(f"Files {left_name} and {right_name} are identical\n") }
    return 0
  }
  let ignoring = opts.ignore_space_change or opts.ignore_blank
  if opts.brief and !ignoring { gnu.write_text(f"Files {left_name} and {right_name} differ\n"); return 1 }
  if !opts.text and (binary(left) or binary(right)) { gnu.write_text(f"Binary files {left_name} and {right_name} differ\n"); return 1 }
  var rendered = ""
  if ignoring {
    if left.utf8() is Err(_) or right.utf8() is Err(_) { gnu.error("ignoring white space requires UTF-8 text"); return 2 }
    match diffutils.unified_ignoring(left.utf8() ?? "", right.utf8() ?? "", cfg.wanted, opts.ignore_space_change, opts.ignore_blank) {
      Ok(text) => rendered = text
      Err(failure) => { gnu.error(gnu.strerror(failure)); return 2 }
    }
  } else {
    match native_unified(left, right, cfg.wanted) {
      Ok(text) => rendered = text
      Err(failure) => { gnu.error(gnu.strerror(failure)); return 2 }
    }
  }
  if rendered == "" {
    if opts.report_same { gnu.write_text(f"Files {left_name} and {right_name} are identical\n") }
    return 0
  }
  if opts.brief { gnu.write_text(f"Files {left_name} and {right_name} differ\n"); return 1 }
  if entry { gnu.write_text(f"diff{cfg.switches} {left_name} {right_name}\n") }
  if opts.unified or opts.context != null or opts.long_context != null {
    let first_label = if entry { left_name } else { cfg.labels.get(0) ?? left_name }
    let second_label = if entry { right_name } else { cfg.labels.get(1) ?? right_name }
    let lines = rendered.lines().collect()
    gnu.write_text(f"--- {first_label}\n+++ {second_label}\n")
    for index in range(2, lines.len()) { gnu.write_text(lines[index] + "\n") }
  } else { gnu.write_text(diffutils.normal(rendered)) }
  1
}

# Names inside one directory side, in byte order, minus excluded names. An
# absent side (`-N`) is an empty directory.
proc entry_names(cfg: Settings, side: Probe) [fs, error] -> Result[List[Str], Error] {
  var names: List[Str] = []
  if side.kind == "absent" { return Ok(names) }
  for child in fs.children(fp"{side.name}", stat: false, ordered: true)? {
    if !excluded(cfg.patterns, child.name) { names += [child.name] }
  }
  Ok(names)
}

# Compare two directories entry by entry in name order. Recursion into
# subdirectories happens in `compare_known` under `-r`. `trail` holds the
# identity pairs of the directories being compared above this one.
proc diff_dirs(cfg: Settings, left: Probe, right: Probe, trail: List[Str]) [fs, io, process, env, error] -> Int {
  let left_listing = entry_names(cfg, left)
  if let Err(failure) = left_listing { gnu.name_error(left.name, failure); return 2 }
  let right_listing = entry_names(cfg, right)
  if let Err(failure) = right_listing { gnu.name_error(right.name, failure); return 2 }
  let left_names = left_listing ?? []
  let right_names = right_listing ?? []
  var status = 0
  var left_at = 0
  var right_at = 0
  while left_at < left_names.len() or right_at < right_names.len() {
    var name = ""
    if left_at >= left_names.len() { name = right_names[right_at] } else if right_at >= right_names.len() { name = left_names[left_at] } else if left_names[left_at] <= right_names[right_at] { name = left_names[left_at] } else { name = right_names[right_at] }
    let in_left = left_at < left_names.len() and left_names[left_at] == name
    let in_right = right_at < right_names.len() and right_names[right_at] == name
    if in_left { left_at += 1 }
    if in_right { right_at += 1 }
    if (in_left and in_right) or cfg.opts.new_file {
      status = worse(status, compare_entry(cfg, join(left.name, name), join(right.name, name), true, trail))
    } else if in_left {
      gnu.write_text(f"Only in {left.name}: {name}\n")
      status = worse(status, 1)
    } else {
      gnu.write_text(f"Only in {right.name}: {name}\n")
      status = worse(status, 1)
    }
  }
  status
}

# Compare two probed operands that are already past command-line directory
# redirection.
proc compare_known(cfg: Settings, left: Probe, right: Probe, entry: Bool, trail: List[Str]) [fs, io, process, env, error] -> Int {
  let left_dir = left.kind == "dir" or (left.kind == "absent" and right.kind == "dir")
  let right_dir = right.kind == "dir" or (right.kind == "absent" and left.kind == "dir")
  if left_dir and right_dir {
    if entry and !cfg.opts.recursive {
      gnu.write_text(f"Common subdirectories: {left.name} and {right.name}\n")
      return 0
    }
    let pair = left.identity + "|" + right.identity
    if pair in trail {
      gnu.error(f"{gnu.quote_maybe(left.name)}: recursive directory loop")
      return 2
    }
    return diff_dirs(cfg, left, right, trail + [pair])
  }
  if left_dir or right_dir {
    gnu.write_text(f"File {left.name} is a {type_name(left)} while file {right.name} is a {type_name(right)}\n")
    return 1
  }
  let plain = ["file", "stdin", "absent"]
  if !(left.kind in plain) or !(right.kind in plain) {
    if left.kind != right.kind and left.kind != "absent" and right.kind != "absent" {
      gnu.write_text(f"File {left.name} is a {type_name(left)} while file {right.name} is a {type_name(right)}\n")
    } else {
      if !(left.kind in plain) { gnu.write_text(f"File {left.name} is not a regular file or directory and was skipped\n") }
      if !(right.kind in plain) { gnu.write_text(f"File {right.name} is not a regular file or directory and was skipped\n") }
    }
    return 1
  }
  let first = read_side(left)
  if let Err(failure) = first { gnu.name_error(left.name, failure); return 2 }
  let second = read_side(right)
  if let Err(failure) = second { gnu.name_error(right.name, failure); return 2 }
  compare_contents(cfg, left.name, right.name, first ?? b"", second ?? b"", entry)
}

# Probe both names and compare them as a directory entry.
proc compare_entry(cfg: Settings, left_name: Str, right_name: Str, entry: Bool, trail: List[Str]) [fs, io, process, env, error] -> Int {
  let left = probe(cfg, left_name)
  let right = probe(cfg, right_name)
  var failed = false
  if let Err(failure) = left { gnu.name_error(left_name, failure); failed = true }
  if let Err(failure) = right { gnu.name_error(right_name, failure); failed = true }
  if failed { return 2 }
  match left {
    Ok(first) => match right {
      Ok(second) => compare_known(cfg, first, second, entry, trail)
      Err(_) => 2
    }
    Err(_) => 2
  }
}

# Command-line operands: a directory against a file compares the file with the
# same-named file inside the directory, as `cp` and `mv` resolve a target.
proc compare_operands(cfg: Settings, left_name: Str, right_name: Str) [fs, io, process, env, error] -> Int {
  let left = probe(cfg, left_name)
  let right = probe(cfg, right_name)
  var failed = false
  if let Err(failure) = left { gnu.name_error(left_name, failure); failed = true }
  if let Err(failure) = right { gnu.name_error(right_name, failure); failed = true }
  if failed { return 2 }
  let first = match left { Ok(item) => item, Err(_) => {name: "", kind: "absent", size: 0, identity: ""} }
  let second = match right { Ok(item) => item, Err(_) => {name: "", kind: "absent", size: 0, identity: ""} }
  let first_dir = first.kind == "dir"
  let second_dir = second.kind == "dir"
  if first_dir == second_dir { return compare_known(cfg, first, second, false, []) }
  let directory = if first_dir { first } else { second }
  let other = if first_dir { second } else { first }
  if other.kind == "stdin" { gnu.error("cannot compare '-' to a directory"); return 2 }
  if other.kind == "absent" { return compare_known(cfg, first, second, false, []) }
  let inside = join(directory.name, fp"{other.name}".name())
  match probe(cfg, inside) {
    Ok(moved) => if first_dir { compare_known(cfg, moved, second, false, []) } else { compare_known(cfg, first, moved, false, []) }
    Err(failure) => { gnu.name_error(inside, failure); 2 }
  }
}

proc main(...argv: List[Str]) [fs, io, process, env, error] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 2},
    unified: {form: "-u", default: false},
    context: {form: "-U LINES"},
    long_context: {form: "--unified[=LINES]", optional_default: "3"},
    brief: {form: "-q --brief", default: false},
    report_same: {form: "-s --report-identical-files", default: false},
    text: {form: "-a --text", default: false},
    label: {form: "-L --label LABEL"},
    ignore_case: {form: "-i --ignore-case", unsupported: true},
    ignore_space: {form: "-w --ignore-all-space", unsupported: true},
    ignore_space_change: {form: "-b --ignore-space-change", default: false},
    ignore_blank: {form: "-B --ignore-blank-lines", default: false},
    recursive: {form: "-r --recursive", default: false},
    new_file: {form: "-N --new-file", default: false},
    exclude: {form: "-x --exclude PAT", repeated: true},
    exclude_from: {form: "-X --exclude-from FILE", repeated: true},
    context_diff: {form: "-c --context[=LINES]", unsupported: true},
    ed: {form: "-e --ed", unsupported: true},
    side: {form: "-y --side-by-side", unsupported: true},
    color: {form: "--color[=WHEN]", unsupported: true},
    help: {form: "--help", default: false, stop: true},
    version: {form: "-v --version", default: false, stop: true},
    files: {form: "...FILE"},
  })?
  if opts.help { gnu.help("Usage: diff [OPTION]... FILE1 FILE2\nCompare two files or directories in normal or unified format. '-' reads stdin.\n-u, -U NUM, --unified[=NUM]  unified output with NUM context lines\n-q, --brief  report only whether files differ\n-s, --report-identical-files\n-a, --text  compare as UTF-8 text\n-L, --label LABEL  replace a displayed filename\n-r, --recursive  compare subdirectories recursively\n-N, --new-file  treat absent files as empty\n-x, --exclude PAT  exclude entries whose name matches PAT\n-X, --exclude-from FILE  exclude entries matching patterns listed in FILE"); return }
  if opts.version { gnu.version("diff"); return }
  if opts.files.len() < 2 { gnu.missing_operand(2) }
  if opts.files.len() > 2 { gnu.extra_operand(opts.files[2], 2) }
  var context_text = opts.context ?? opts.long_context ?? "3"
  var argument = 0
  while argument < argv.len() {
    let word = argv[argument]
    if word == "--" { break }
    if word == "-U" { argument += 1; context_text = argv.get(argument) ?? "" } else if word.starts_with("-U") { context_text = word.byte_slice(2) } else if word.starts_with("--unified=") { context_text = word.byte_slice(10) } else if word == "--unified" { context_text = "3" }
    argument += 1
  }
  let context = context_text.parse_int() ?? -1
  if context < 0 { gnu.usage_error("invalid context length", 2) }
  var patterns = opts.exclude
  for listing in opts.exclude_from {
    let data = gnu.read_operand(listing)
    if let Err(failure) = data { gnu.name_error(listing, failure); exit 2 }
    for line in ((data ?? b"").utf8() ?? "").lines() {
      if line != "" { patterns += [line] }
    }
  }
  let left_name = opts.files[0]
  let right_name = opts.files[1]
  if left_name == "-" and right_name == "-" { return }
  let wanted = if opts.unified or opts.context != null or opts.long_context != null { context } else { 0 }
  let cfg: Settings = {opts: opts, wanted: wanted, labels: labels(argv), switches: switch_text(argv), patterns: patterns}
  let status = compare_operands(cfg, left_name, right_name)
  if status != 0 { exit status }
}
