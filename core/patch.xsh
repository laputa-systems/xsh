#!/bin/xsh
use lib.gnu
use lib.patch_parse
use lib.patch_apply

type Options = {
  backup: Bool,
  backup_if_mismatch: Bool,
  no_backup_if_mismatch: Bool,
  prefix: Str?,
  basename_prefix: Str?,
  suffix: Str?,
  version_control: Str?,
  context: Bool,
  ed: Bool,
  normal: Bool,
  unified: Bool,
  directory: Str?,
  remove_empty: Bool,
  force: Bool,
  batch: Bool,
  fuzz: Str,
  input: Str?,
  ignore_whitespace: Bool,
  forward: Bool,
  output: Str?,
  strip: Str?,
  reject_file: Str?,
  reverse: Bool,
  quiet: Bool,
  verbose: Bool,
  dry_run: Bool,
  binary: Bool,
  posix: Bool,
  follow_symlinks: Bool,
  set_time: Bool,
  set_utc: Bool,
  get: Str?,
  quoting_style: Str?,
  ifdef: Str?,
  merge: Str?,
  reject_format: Str?,
  read_only: Str?,
  help: Bool,
  version: Bool,
  files: List[Str],
}

# How one file patch ended. `next` is the patch line to scan from; `output` is
# the patched content; `rejected` holds the hunks that did not apply, already
# in the direction they were tried; `fatal_line` is nonzero for a malformed
# patch and `reversed` tells whether the patch was applied backwards.
type Outcome = {
  output: List[Bytes],
  rejected: List[patch_parse.Hunk],
  total: Int,
  mismatch: Bool,
  skipped: Bool,
  next: Int,
  fatal_line: Int,
  fatal_text: Bytes,
  reversed: Bool,
}

type PatchStatus = {status: Int, next: Int}

type Skipped = {total: Int, next: Int}

type Candidate = {name: Str, exists: Bool}

const HELP = """
Usage: patch [OPTION]... [ORIGFILE [PATCHFILE]]

Input options:

  -p NUM  --strip=NUM  Strip NUM leading components from file names.
  -F LINES  --fuzz LINES  Set the fuzz factor to LINES for inexact matching.
  -l  --ignore-whitespace  Ignore white space changes between patch and input.

  -c  --context  Interpret the patch as a context difference.
  -e  --ed  Interpret the patch as an ed script.
  -n  --normal  Interpret the patch as a normal difference.
  -u  --unified  Interpret the patch as a unified difference.

  -N  --forward  Ignore patches that appear to be reversed or already applied.
  -R  --reverse  Assume patches were created with old and new files swapped.

  -i PATCHFILE  --input=PATCHFILE  Read patch from PATCHFILE instead of stdin.

Output options:

  -o FILE  --output=FILE  Output patched files to FILE.
  -r FILE  --reject-file=FILE  Output rejects to FILE.

  -D NAME  --ifdef=NAME  Make merged if-then-else output using NAME.
  --merge  Merge using conflict markers instead of creating reject files.
  -E  --remove-empty-files  Remove output files that are empty after patching.

  -Z  --set-utc  Set times of patched files, assuming diff uses UTC (GMT).
  -T  --set-time  Likewise, assuming local time.

  --quoting-style=WORD   output file names using quoting style WORD.
    Valid WORDs are: literal, shell, shell-always, c, escape.
    Default is taken from QUOTING_STYLE env variable, or 'shell' if unset.

Backup and version control options:

  -b  --backup  Back up the original contents of each file.
  --backup-if-mismatch  Back up if the patch does not match exactly.
  --no-backup-if-mismatch  Back up mismatches only if otherwise requested.

  -V STYLE  --version-control=STYLE  Use STYLE version control.
\tSTYLE is either 'simple', 'numbered', or 'existing'.
  -B PREFIX  --prefix=PREFIX  Prepend PREFIX to backup file names.
  -Y PREFIX  --basename-prefix=PREFIX  Prepend PREFIX to backup file basenames.
  -z SUFFIX  --suffix=SUFFIX  Append SUFFIX to backup file names.

  -g NUM  --get=NUM  Get files from RCS etc. if positive; ask if negative.

Miscellaneous options:

  -t  --batch  Ask no questions; skip bad-Prereq patches; assume reversed.
  -f  --force  Like -t, but ignore bad-Prereq patches, and assume unreversed.
  -s  --quiet  --silent  Work silently unless an error occurs.
  --verbose  Output extra information about the work being done.
  --dry-run  Do not actually change any files; just print what would happen.
  --posix  Conform to the POSIX standard.

  -d DIR  --directory=DIR  Change the working directory to DIR first.
  --reject-format=FORMAT  Create 'context' or 'unified' rejects.
  --binary  Read and write data in binary mode.
  --read-only=BEHAVIOR  How to handle read-only input files: 'ignore' that they
                        are read-only, 'warn' (default), or 'fail'.

  -v  --version  Output version info.
  --help  Output this help.

Report bugs to <bug-patch@gnu.org>.
"""

# A fatal GNU patch error: `patch: **** MESSAGE` and exit status 2.
proc fatal(message: Str) [process, env, io] {
  eprint f"{gnu.prog()}: **** {message}"
  exit 2
}

# A fatal error that names a system failure: `patch: **** MESSAGE : STRERROR`.
proc pfatal(message: Str, failure: Error) [process, env, io] {
  eprint f"{gnu.prog()}: **** {message} : {gnu.strerror(failure)}"
  exit 2
}

proc say(text: Str) [process, env, io] {
  gnu.write_text(text)
}

# Progress messages go to stderr while the patched file itself is written to
# standard output, so they cannot mix with it.
proc note(opts: Options, text: Str) [process, env, io, error] {
  if opts.output == "-" {
    io.write_stderr(text)?
    io.flush_stderr()?
  } else { say(text) }
}

# A file name as GNU patch prints it: shell style unless `--quoting-style`
# or QUOTING_STYLE chooses another.
proc q(opts: Options, name: Str) [env] -> Str {
  let style = opts.quoting_style ?? env.get_or("QUOTING_STYLE", "shell") ?? "shell"
  if style == "literal" { return name }
  if style == "shell-always" { return gnu.quote(name) }
  if style == "c" or style == "escape" {
    var out = ""
    for char in name {
      if char == "\"" and style == "c" { out += "\\\"" } else if char == "\\" { out += "\\\\" } else if char == "\n" { out += "\\n" } else if char == "\t" { out += "\\t" } else { out += char }
    }
    return if style == "c" { "\"" + out + "\"" } else { out }
  }
  gnu.quote_maybe(name)
}

pure plural(count: Int) -> Str {
  if count == 1 { "" } else { "s" }
}

pure basename_of(name: Str) -> Str {
  let parts = name.split("/")
  parts[parts.len() - 1]
}

pure dirname_of(name: Str) -> Str {
  let at = name.byte_len() - basename_of(name).byte_len()
  name.byte_slice(0, at)
}

pure component_count(name: Str) -> Int {
  var count = 0
  for part in name.split("/") { if part != "" { count += 1 } }
  count
}

# GNU patch's choice among the old, new, and index names: an existing file
# beats a missing one, then fewer path components, then a shorter base name,
# then a shorter whole name; the first listed wins a full tie.
pure better(candidate: Candidate, other: Candidate) -> Bool {
  if candidate.exists != other.exists { return candidate.exists }
  let a = component_count(candidate.name)
  let b = component_count(other.name)
  if a != b { return a < b }
  let c = basename_of(candidate.name).byte_len()
  let d = basename_of(other.name).byte_len()
  if c != d { return c < d }
  candidate.name.byte_len() < other.name.byte_len()
}

proc file_exists(name: Str) [fs] -> Bool {
  match fs.stat(fp"{name}", follow_symlinks: true) {
    Ok(_) => true
    Err(_) => false
  }
}

# The patched file's content as bytes, or empty when it does not exist.
proc read_input(name: Str) [fs, error] -> Result[Bytes, Error] {
  if !file_exists(name) { return Ok(b"") }
  fp"{name}".read_bytes()
}

# A unified range: the line count is omitted for one line, and an empty range
# names the line before it.
pure range_text(first: Int, count: Int) -> Str {
  if count == 1 { f"{first}" } else if count == 0 { f"{first - 1},0" } else { f"{first},{count}" }
}

# A body line as GNU patch writes it into a reject file: the line bytes
# exactly as read, so a line that had no terminator runs into the next one.
pure line_text(kind: Str, line: Bytes) -> Bytes {
  bytes.concat([bytes.from_text(kind), line])
}

# One failed hunk as unified-diff text.
pure unified_reject(hunk: patch_parse.Hunk) -> Bytes {
  var parts: List[Bytes] = [bytes.from_text(f"@@ -{range_text(hunk.old_first, hunk.old_count)} +{range_text(hunk.new_first, hunk.new_count)} @@"), hunk.function, b"\n"]
  for index in range(hunk.kinds.len()) {
    let kind = hunk.kinds[index]
    parts += [line_text(if kind == 0 { " " } else if kind == 1 { "-" } else { "+" }, hunk.texts[index])]
  }
  bytes.concat(parts)
}

pure context_range(first: Int, count: Int) -> Str {
  if count == 0 { return f"{first - 1}" }
  if count == 1 { return f"{first}" }
  f"{first},{first + count - 1}"
}

# One failed hunk as context-diff text. A hunk read from a normal diff has no
# context lines and prints the bare ranges GNU patch prints for such hunks.
pure context_reject(hunk: patch_parse.Hunk, from_normal: Bool) -> Bytes {
  var old_lines: List[Bytes] = []
  var new_lines: List[Bytes] = []
  let kinds = hunk.kinds
  # Within one change group, removed and added lines show as changed (`!`)
  # when the group has both, otherwise as `-` or `+`.
  var marks: List[Str] = []
  var index = 0
  while index < kinds.len() {
    if kinds[index] == 0 {
      marks += [" "]
      index += 1
      continue
    }
    var end = index
    var removed = 0
    var added = 0
    while end < kinds.len() and kinds[end] != 0 {
      if kinds[end] == 1 { removed += 1 } else { added += 1 }
      end += 1
    }
    for k in range(end - index) {
      if removed > 0 and added > 0 and !from_normal { marks += ["!"] } else if kinds[index + k] == 1 { marks += ["-"] } else { marks += ["+"] }
    }
    index = end
  }
  for at in range(kinds.len()) {
    let mark = marks[at]
    let text = hunk.texts[at]
    let note = b""
    if kinds[at] != 2 {
      old_lines += [bytes.concat([bytes.from_text(if mark == "!" { "! " } else if mark == "-" { "- " } else { "  " }), text, note])]
    }
    if kinds[at] != 1 {
      new_lines += [bytes.concat([bytes.from_text(if mark == "!" { "! " } else if mark == "+" { "+ " } else { "  " }), text, note])]
    }
  }
  var parts: List[Bytes] = [bytes.from_text("***************"), hunk.function, b"\n"]
  let old_range = if from_normal and hunk.old_count == 0 { "0" } else { context_range(hunk.old_first, hunk.old_count) }
  let new_range = if from_normal and hunk.new_count == 0 { "0" } else { context_range(hunk.new_first, hunk.new_count) }
  let old_tail = if from_normal { "" } else { " ****" }
  let new_tail = if from_normal { " -----" } else { " ----" }
  parts += [bytes.from_text(f"*** {old_range}{old_tail}\n")]
  parts += old_lines
  parts += [bytes.from_text(f"--- {new_range}{new_tail}\n")]
  parts += new_lines
  bytes.concat(parts)
}

# The order GNU patch declares its long options in. getopt lists the
# candidates of an ambiguous abbreviation in this order.
const LONG_OPTION_ORDER = ["backup", "prefix", "context", "directory", "ifdef", "ed", "remove-empty-files", "force", "fuzz", "get", "input", "ignore-whitespace", "normal", "forward", "output", "strip", "reject-file", "reverse", "quiet", "silent", "batch", "set-time", "unified", "version", "version-control", "debug", "basename-prefix", "suffix", "set-utc", "dry-run", "verbose", "binary", "help", "backup-if-mismatch", "no-backup-if-mismatch", "posix", "quoting-style", "reject-format", "read-only", "follow-symlinks", "merge"]

# Reorder the candidates in an `is ambiguous; possibilities:` message.
pure reorder_candidates(message: Str) -> Str {
  let marker = "; possibilities: "
  let at = message.find(marker) ?? -1
  if at < 0 { return message }
  let head = message.byte_slice(0, at + marker.byte_len())
  var names: List[Str] = []
  for part in message.byte_slice(at + marker.byte_len()).split(" ") {
    if part != "" { names += [part] }
  }
  var ordered: List[Str] = []
  for known in LONG_OPTION_ORDER {
    for name in names { if name == f"'--{known}'" { ordered += [name] } }
  }
  for name in names { if !(name in ordered) { ordered += [name] } }
  head + ordered.join(" ")
}

# Resolve a `-V` argument to none, simple, existing, or numbered; abbreviations
# are accepted when they name one style. Anything else is "".
pure version_style(text: Str) -> Str {
  let table = [
    {name: "none", style: "none"}, {name: "off", style: "none"},
    {name: "simple", style: "simple"}, {name: "never", style: "simple"},
    {name: "existing", style: "existing"}, {name: "nil", style: "existing"},
    {name: "numbered", style: "numbered"}, {name: "t", style: "numbered"},
  ]
  for entry in table { if entry.name == text { return entry.style } }
  var found = ""
  for entry in table {
    if text != "" and entry.name.starts_with(text) {
      if found != "" and found != entry.style { return "" }
      found = entry.style
    }
  }
  found
}

# The last of the format switches wins, as with getopt processing in order.
pure chosen_format(argv: List[Str]) -> Str {
  var format = "any"
  for argument in argv {
    if argument == "--" { break }
    if argument.starts_with("--") {
      let name = argument.byte_slice(2).split("=")[0]
      if name != "" {
        for long in ["unified", "context", "normal", "ed"] {
          if long.starts_with(name) and (name == long or (name.byte_len() > 1 and name != "no")) {
            format = if long == "unified" { "unified" } else if long == "context" { "context" } else if long == "normal" { "normal" } else { "ed" }
          }
        }
      }
    } else if argument.starts_with("-") and argument != "-" {
      for letter in argument.byte_slice(1) {
        if letter == "u" { format = "unified" } else if letter == "c" { format = "context" } else if letter == "n" { format = "normal" } else if letter == "e" { format = "ed" }
        # Letters that take a value end the cluster.
        if letter in ["p", "F", "d", "D", "i", "o", "r", "B", "Y", "z", "V", "g", "x"] { break }
      }
    }
  }
  format
}

# GNU patch prints its own prefix before the usage hint, which the option
# parser's message lacks.
proc usage_failure(message: Str) [process, env, io] {
  let first = reorder_candidates(message.split("\n")[0])
  if first != "" { eprint $first }
  eprint f"{gnu.prog()}: Try '{gnu.prog()} --help' for more information."
  exit 2
}

proc main(...argv: List[Str]) [fs, io, process, env, error] {
  let parsed = cli.applet(argv, {
    gnu: {status: 2, unsupported: {"-x": "debug output is not available", "--debug": "debug output is not available"}},
    backup: {form: "-b --backup", default: false},
    backup_if_mismatch: {form: "--backup-if-mismatch", default: false},
    no_backup_if_mismatch: {form: "--no-backup-if-mismatch", default: false},
    prefix: {form: "-B --prefix PREFIX"},
    basename_prefix: {form: "-Y --basename-prefix PREFIX"},
    suffix: {form: "-z --suffix SUFFIX"},
    version_control: {form: "-V --version-control STYLE"},
    context: {form: "-c --context", default: false},
    ed: {form: "-e --ed", default: false},
    normal: {form: "-n --normal", default: false},
    unified: {form: "-u --unified", default: false},
    directory: {form: "-d --directory DIR"},
    ifdef: {form: "-D --ifdef NAME"},
    remove_empty: {form: "-E --remove-empty-files", default: false},
    force: {form: "-f --force", default: false},
    batch: {form: "-t --batch", default: false},
    fuzz: {form: "-F --fuzz LINES", default: "2"},
    get: {form: "-g --get NUM"},
    input: {form: "-i --input PATCHFILE"},
    ignore_whitespace: {form: "-l --ignore-whitespace", default: false},
    forward: {form: "-N --forward", default: false},
    output: {form: "-o --output FILE"},
    strip: {form: "-p --strip NUM"},
    reject_file: {form: "-r --reject-file FILE"},
    reverse: {form: "-R --reverse", default: false},
    quiet: {form: "-s --quiet --silent", default: false},
    verbose: {form: "--verbose", default: false},
    dry_run: {form: "--dry-run", default: false},
    binary: {form: "--binary", default: false},
    posix: {form: "--posix", default: false},
    merge: {form: "--merge[=STYLE]", optional_default: "merge"},
    follow_symlinks: {form: "--follow-symlinks", default: false},
    set_time: {form: "-T --set-time", default: false},
    set_utc: {form: "-Z --set-utc", default: false},
    quoting_style: {form: "--quoting-style WORD"},
    reject_format: {form: "--reject-format FORMAT"},
    read_only: {form: "--read-only BEHAVIOR"},
    help: {form: "--help", default: false, stop: true},
    version: {form: "-v --version", default: false, stop: true},
    files: {form: "...FILE"},
  })
  let opts: Options = match parsed {
    Ok(record) => record
    Err(failure) => {
      usage_failure(failure.message)
      exit 2
    }
  }
  if opts.help { gnu.help(HELP); return }
  if opts.version { gnu.version("patch"); return }
  if opts.files.len() > 2 { usage_failure(f"{gnu.prog()}: {opts.files[2]}: extra operand") }
  var strip = -1
  if let text = opts.strip {
    let count = text.parse_int() ?? -2
    if count == -2 { fatal(f"strip count {text} is not a number") }
    if count < 0 { fatal(f"strip count {text} is negative") }
    strip = count
  }
  let max_fuzz = opts.fuzz.parse_int() ?? -1
  if max_fuzz < 0 { fatal(f"fuzz factor {opts.fuzz} is not a number") }
  if let style = opts.version_control {
    if version_style(style) == "" {
      eprint f"{gnu.prog()}: invalid argument '{style}' for '--version-control or -V option'"
      eprint "Valid arguments are:\n  - 'none', 'off'\n  - 'simple', 'never'\n  - 'existing', 'nil'\n  - 'numbered', 't'"
      exit 2
    }
  }
  if let format = opts.reject_format {
    if format != "context" and format != "unified" { usage_failure("") }
  }
  if let behavior = opts.read_only {
    if behavior != "ignore" and behavior != "warn" and behavior != "fail" { usage_failure("") }
  }
  if let style = opts.quoting_style {
    if !(style in ["literal", "shell", "shell-always", "c", "escape"]) {
      usage_failure(f"{gnu.prog()}: invalid argument '{style}' for 'quoting style'")
    }
  }
  if let count = opts.get {
    let number = count.parse_int() ?? -1000000
    if number == -1000000 { fatal(f"get option value {count} is not a number") }
    if number != 0 {
      eprint f"{gnu.prog()}: option '--get' is not supported: files cannot be checked out of version control"
      exit 2
    }
  }
  if let dir = opts.directory {
    match fs.stat(fp"{dir}", follow_symlinks: true) {
      Ok(info) => { if info.kind != "dir" { fatal(f"Can't change to directory {dir} : Not a directory") } }
      Err(failure) => pfatal(f"Can't change to directory {dir}", failure)
    }
    cd fp"{dir}" {
      run_patches(opts, argv, strip, max_fuzz)
    }
  } else {
    run_patches(opts, argv, strip, max_fuzz)
  }
}

proc run_patches(opts: Options, opts_argv: List[Str], strip: Int, max_fuzz: Int) [fs, io, process, env, error] {
  let input_name = opts.files.get(1) ?? opts.input ?? "-"
  var patch_bytes = b""
  if input_name == "-" { patch_bytes = io.stdin_bytes()? } else {
    match fp"{input_name}".read_bytes() {
      Ok(data) => { patch_bytes = data }
      Err(failure) => pfatal(f"Can't open patch file {input_name}", failure)
    }
  }
  let lines = patch_parse.split_lines(patch_bytes)
  if lines.is_empty() { return }
  let allowed = chosen_format(opts_argv)
  var named: Str? = null
  if opts.files.len() > 0 { named = opts.files[0] }
  var position = 0
  var status = 0
  var seen = 0
  loop {
    let scan = patch_parse.scan(lines, position, allowed)
    if scan.kind == "none" {
      if seen == 0 { fatal("Only garbage was found in the patch input.") }
      if opts.verbose { say("Hmm...  Ignoring the trailing garbage.\n") }
      break
    }
    if opts.verbose { announce(lines, scan, seen == 0) }
    if scan.cr_header and !opts.binary { say("(Stripping trailing CRs from patch; use --binary to disable.)\n") }
    seen += 1
    let outcome = one_patch(opts, strip, max_fuzz, lines, scan, named)
    if outcome.status > status { status = outcome.status }
    position = outcome.next
    if position >= lines.len() { break }
  }
  if opts.verbose { say("done\n") }
  exit status
}

# The `--verbose` description of the patch about to be applied.
proc announce(lines: List[Bytes], scan: patch_parse.Scan, first: Bool) [process, env, io, error] {
  let what = if scan.kind == "context" { "a new-style context diff" } else if scan.kind == "normal" { "a normal diff" } else if scan.kind == "ed" { "an ed script" } else { "a unified diff" }
  let lead = if first { "Looks like" } else { "The next patch looks like" }
  say(f"Hmm...  {lead} {what} to me...\n")
  if scan.body > scan.start {
    var shown = ""
    for index in range(scan.start, scan.body) { shown += "|" + patch_parse.body_text(lines[index]) + "\n" }
    say(f"The text leading up to this was:\n--------------------------\n{shown}--------------------------\n")
  }
}

# A file name taken from a header, after `-p`. Absolute names and names that
# climb out with `..` are refused, as GNU patch refuses them.
proc usable_name(raw: Str, strip: Int) [io, process, env, error] -> Str {
  let stripped = patch_parse.strip_name(raw, strip)
  if stripped == "" { return "" }
  var dangerous = stripped.starts_with("/")
  for part in stripped.split("/") { if part == ".." { dangerous = true } }
  if dangerous {
    say(f"Ignoring potentially dangerous file name {stripped}\n")
    return ""
  }
  stripped
}

# Whether a directory component of `name` is a symbolic link.
proc crosses_symlink(name: Str) [fs, error] -> Bool {
  var prefix = ""
  let parts = name.split("/")
  for index in range(parts.len() - 1) {
    prefix = if prefix == "" { parts[index] } else { prefix + "/" + parts[index] }
    if prefix == "" { continue }
    match fs.stat(fp"{prefix}", follow_symlinks: false) {
      Ok(info) => {
        if info.kind == "symlink" {
          # A link that stays inside the tree is followed; one that points
          # elsewhere makes the name invalid.
          let destination = fp"{prefix}".readlink()?.display()
          if destination.starts_with("/") { return true }
          for part in destination.split("/") { if part == ".." { return true } }
        }
      }
      Err(_) => {}
    }
  }
  false
}

proc one_patch(opts: Options, strip: Int, max_fuzz: Int, lines: List[Bytes], scan: patch_parse.Scan, named: Str?) [fs, io, process, env, error] -> PatchStatus {
  let old_raw = scan.old_name
  let new_raw = scan.new_name
  let git = scan.git
  let first = read_one(lines, scan.body, scan.kind, false)
  var creating = false
  var deleting = false
  if let hunk = first.hunk {
    creating = hunk.old_count == 0 and hunk.old_start == 0
    deleting = hunk.new_count == 0 and hunk.new_start == 0
  }
  var candidates: List[Candidate] = []
  for raw in [old_raw, new_raw] {
    if raw.present and raw.name != "/dev/null" {
      let stripped = usable_name(raw.name, strip)
      if stripped != "" { candidates += [{name: stripped, exists: file_exists(stripped)}] }
    }
  }
  if candidates.is_empty() and git.present {
    for raw in [git.old_path, git.new_path] {
      if raw != "" and raw != "/dev/null" {
        let stripped = usable_name(raw, strip)
        if stripped != "" { candidates += [{name: stripped, exists: file_exists(stripped)}] }
      }
    }
  }
  if scan.index_name != "" and candidates.is_empty() {
    let stripped = usable_name(scan.index_name, strip)
    if stripped != "" { candidates += [{name: stripped, exists: file_exists(stripped)}] }
  }
  var chosen = ""
  var found = false
  var source = ""
  var action = ""
  if git.present and git.rename_from != "" and git.rename_to != "" {
    source = if opts.reverse { git.rename_to } else { git.rename_from }
    chosen = if opts.reverse { git.rename_from } else { git.rename_to }
    action = "renamed"
    found = true
  } else if git.present and git.copy_from != "" and git.copy_to != "" {
    source = git.copy_from
    chosen = git.copy_to
    action = "copied"
    found = true
  } else if let given = named { chosen = given; found = true } else {
    var best: Candidate = {name: "", exists: false}
    var have = false
    for candidate in candidates {
      if !have or better(candidate, best) { best = candidate; have = true }
    }
    if have and (best.exists or creating or deleting or (git.present and (git.new_file_mode != "" or git.deleted_file_mode != ""))) { chosen = best.name; found = true }
  }
  if !found {
    let first_line = scan.body + 1
    var shown = ""
    for index in range(scan.start, scan.body) { shown += "|" + patch_parse.body_text(lines[index]) + "\n" }
    let skipped = skip_hunks(lines, scan)
    let hint = if opts.strip == null { "Perhaps you should have used the -p or --strip option?\n" } else { "Perhaps you used the wrong -p or --strip option?\n" }
    if !opts.quiet or opts.force or opts.batch {
      say(f"can't find file to patch at input line {first_line}\n{hint}")
    }
    say(f"The text leading up to this was:\n--------------------------\n{shown}--------------------------\n")
    if opts.force or opts.batch {
      say("No file to patch.  Skipping patch.\n")
    } else {
      say("File to patch: \nSkip this patch? [y] \n")
      if !opts.quiet { say("Skipping patch.\n") }
    }
    say(f"{skipped.total} out of {skipped.total} hunk{plural(skipped.total)} ignored\n")
    return {status: 1, next: skipped.next}
  }
  if git.present and git.binary {
    say(f"File {chosen}: git binary diffs are not supported.\n")
    return {status: 1, next: scan.body}
  }
  if crosses_symlink(chosen) or (source != "" and crosses_symlink(source)) {
    say(f"Invalid file name {chosen} -- skipping patch\n")
    return {status: 1, next: skip_hunks(lines, scan).next}
  }
  if source != "" and !file_exists(source) {
    say("Cannot rename file without two valid file names\n")
    return {status: 1, next: skip_hunks(lines, scan).next}
  }
  let old_missing = (old_raw.present and (old_raw.name == "/dev/null" or patch_parse.is_epoch(old_raw.stamp))) or (git.present and git.new_file_mode != "")
  let new_missing = (new_raw.present and (new_raw.name == "/dev/null" or patch_parse.is_epoch(new_raw.stamp))) or (git.present and git.deleted_file_mode != "")
  let target: Target = {read: if source != "" { source } else { chosen }, write: chosen, action: action, mode: git_mode(git)}
  apply_file(opts, strip, max_fuzz, lines, scan, target, {creating: creating or (git.present and git.new_file_mode != ""), deleting: deleting or (git.present and git.deleted_file_mode != ""), old_missing: old_missing, new_missing: new_missing})
}

# The permission bits a Git header asks for, or -1 when it names none.
pure git_mode(git: patch_parse.GitHeader) -> Int {
  var text = ""
  if git.new_file_mode != "" { text = git.new_file_mode } else if git.new_mode != "" { text = git.new_mode }
  if text == "" { return -1 }
  var value = 0
  for digit in text { value = value * 8 + (digit.parse_int() ?? 0) }
  value
}

proc skip_hunks(lines: List[Bytes], scan: patch_parse.Scan) -> Skipped {
  var position = scan.body
  var total = 0
  loop {
    if position >= lines.len() { break }
    let read = read_one(lines, position, scan.kind, false)
    if read.hunk == null or read.bad_line > 0 { break }
    total += 1
    position = read.next
  }
  {total: total, next: position}
}

pure read_one(lines: List[Bytes], position: Int, kind: Str, strip_cr: Bool) -> patch_parse.HunkRead {
  let none: patch_parse.HunkRead = {hunk: null, next: position, bad_line: 0, bad_text: b"", truncated: false}
  if position >= lines.len() { return none }
  let text = patch_parse.body_text(lines[position])
  if kind == "unified" or kind == "git" {
    if !text.starts_with("@@ ") { return none }
    return patch_parse.read_unified(lines, position, strip_cr)
  }
  if kind == "context" {
    if !text.starts_with("***************") { return none }
    return patch_parse.read_context(lines, position, strip_cr)
  }
  if kind == "normal" {
    if !rx"^\d+(,\d+)?[acd]\d+(,\d+)?$".matches(text) { return none }
    return patch_parse.read_normal(lines, position, strip_cr)
  }
  none
}

# Whether `word` occurs in the file with no letter, digit, or underscore
# touching either end, as GNU patch's Prereq check requires.
pure contains_word(content: Bytes, word: Str) -> Bool {
  let text = content.utf8() ?? ""
  var from = 0
  loop {
    let at = text.find(word, from) ?? -1
    if at < 0 { return false }
    let before = if at == 0 { " " } else { text.byte_slice(at - 1, length: 1) }
    let end = at + word.byte_len()
    let after = if end >= text.byte_len() { " " } else { text.byte_slice(end, length: 1) }
    if !rx"[A-Za-z0-9_]".matches(before) and !rx"[A-Za-z0-9_]".matches(after) { return true }
    from = at + 1
  }
  false
}

# What the patch says about the file's existence, in the patch's own
# direction: whether its first hunk starts from or ends at an empty file, and
# whether the old or new header name is `/dev/null` or an epoch timestamp.
type Facts = {creating: Bool, deleting: Bool, old_missing: Bool, new_missing: Bool}

# Where a file patch reads and writes. `action` is "renamed" or "copied"
# when the Git header moves the file; `mode` is the full Git mode or -1.
type Target = {read: Str, write: Str, action: Str, mode: Int}

proc apply_file(opts: Options, strip: Int, max_fuzz: Int, lines: List[Bytes], scan: patch_parse.Scan, target: Target, facts: Facts) [fs, io, process, env, error] -> PatchStatus {
  let name = target.read
  var entry_kind = ""
  match fs.stat(fp"{name}", follow_symlinks: false) {
    Ok(info) => { entry_kind = info.kind }
    Err(_) => {}
  }
  var exists = false
  var blocked = ""
  if entry_kind != "" {
    var kind = entry_kind
    if entry_kind == "symlink" and opts.follow_symlinks {
      match fs.stat(fp"{name}", follow_symlinks: true) {
        Ok(info) => { kind = info.kind }
        Err(_) => { kind = "dangling" }
      }
    }
    if kind != "file" and kind != "dangling" {
      blocked = f"File {q(opts, name)} is not a regular file -- refusing to patch\n"
    } else if kind == "file" {
      exists = true
      let writable = fs.access(fp"{name}", write: true) ?? true
      if !writable and opts.read_only != "ignore" {
        if opts.read_only == "fail" {
          blocked = f"File {q(opts, name)} is read-only; refusing to patch\n"
        } else {
          say(f"File {q(opts, name)} is read-only; trying to patch anyway\n")
        }
      }
    }
  }
  var original = b""
  var original_mtime = -1
  var original_mode = -1
  if blocked == "" and exists {
    let original_info = fs.stat(fp"{name}", follow_symlinks: true)?
    original_mtime = original_info.mtime_ns
    original_mode = original_info.mode % 4096
    match fp"{name}".read_bytes() {
      Ok(data) => { original = data }
      Err(failure) => pfatal(f"Can't open file {name}", failure)
    }
  }
  if scan.prereq != "" and blocked == "" {
    let words = scan.prereq.words()
    if words.len() > 1 { say("Prereq: with multiple words at line 0 of patch\n") }
    let wanted = words[0]
    if !contains_word(original, wanted) {
      if opts.force {
        say(f"Warning: this file doesn't appear to be the {wanted} version -- patching anyway.\n")
      } else if opts.batch {
        fatal(f"This file doesn't appear to be the {wanted} version -- aborting.")
      } else {
        say(f"This file doesn't appear to be the {wanted} version -- patch anyway? [n] \n")
        fatal("aborted")
      }
    }
  }
  var reverse = opts.reverse
  # A patch that creates a file that is there already, or deletes one that is
  # not, is probably reversed or already applied.
  let creating = if reverse { facts.deleting } else { facts.creating }
  let deleting = if reverse { facts.creating } else { facts.deleting }
  let old_missing = if reverse { facts.new_missing } else { facts.old_missing }
  let lead = if opts.reverse { "The next patch, when reversed, would" } else { "The next patch would" }
  var prompt = ""
  if blocked != "" {
    say(blocked)
  } else if !exists and deleting {
    prompt = f"{lead} delete the file {q(opts, name)},\nwhich does not exist!  "
  } else if exists and original.len() > 0 and old_missing and creating {
    prompt = f"{lead} create the file {q(opts, name)},\nwhich already exists!  "
  }
  var skip_all = false
  if prompt != "" {
    if opts.force {
      note(opts, prompt + "Applying it anyway.\n")
    } else if opts.batch {
      note(opts, prompt + (if opts.reverse { "Ignoring -R.\n" } else { "Assuming -R.\n" }))
      reverse = !reverse
    } else if opts.forward {
      note(opts, prompt + "Skipping patch.\n")
      skip_all = true
    } else {
      note(opts, prompt + (if opts.reverse { "Ignore -R? [n] \n" } else { "Assume -R? [n] \n" }) + "Apply anyway? [n] \nSkipping patch.\n")
      skip_all = true
    }
  }
  if skip_all {
    let skipped = skip_hunks(lines, scan)
    note(opts, f"{skipped.total} out of {skipped.total} hunk{plural(skipped.total)} ignored\n")
    return {status: 1, next: skipped.next}
  }
  let input_lines = patch_parse.split_lines(original)
  let outname = opts.output ?? target.write
  if scan.kind == "context" and read_one(lines, scan.body, scan.kind, false).bad_line == -1 {
    fatal("unexpected end of file in patch")
  }
  let symlink = target.mode >= 0 and target.mode / 4096 == 10
  if !opts.quiet and blocked == "" and scan.kind != "ed" {
    let verb = if opts.dry_run { "checking" } else { "patching" }
    var tail = ""
    if target.action != "" { tail = f" ({target.action} from {q(opts, name)})" } else if opts.output != null { tail = f" (read from {name})" }
    note(opts, f"{verb} {if symlink { "symbolic link" } else { "file" }} {q(opts, outname)}{tail}\n")
  }
  var outcome: Outcome = {output: [], rejected: [], total: 0, mismatch: false, skipped: false, next: 0, fatal_line: 0, fatal_text: b"", reversed: false}
  if scan.kind == "ed" and blocked == "" {
    # The ed script runs against the whole file; nothing can be rejected.
    let edited = patch_parse.run_ed(lines, scan.body, input_lines)
    if edited.failed != "" {
      eprint f"{gnu.prog()}: **** /bin/ed FAILED"
      exit 2
    }
    outcome = {output: edited.output, rejected: [], total: 0, mismatch: false, skipped: false, next: lines.len(), fatal_line: 0, fatal_text: b"", reversed: false}
  } else {
    outcome = run_hunks(opts, max_fuzz, lines, scan, input_lines, reverse, facts, reverse != opts.reverse, blocked != "")
  }
  if outcome.fatal_line == -1 { fatal("unexpected end of file in patch") }
  if outcome.fatal_line > 0 {
    eprint f"{gnu.prog()}: **** malformed patch at line {outcome.fatal_line}: {outcome.fatal_text.utf8() ?? ""}"
    exit 2
  }
  if opts.merge != null and outcome.rejected.len() > 0 {
    fatal("--merge cannot write conflict markers; hunks that do not apply are not merged")
  }
  let failed = outcome.rejected.len()
  let reject_name = opts.reject_file ?? f"{outname}.rej"
  let content = bytes.concat(outcome.output)
  var status = 0
  if !opts.dry_run and !outcome.skipped {
    if outname == "-" { gnu.write_bytes(content) } else {
      let removes = if outcome.reversed { facts.old_missing and facts.creating } else { facts.new_missing and facts.deleting }
      let posix = opts.posix or (env.get_or("POSIXLY_CORRECT", "\u{1}unset") ?? "\u{1}unset") != "\u{1}unset"
      # A file that was not there is only backed up, empty, after a failure.
      let mismatched = failed > 0 or (outcome.mismatch and exists)
      let mismatch_backup = mismatched and !opts.no_backup_if_mismatch and (opts.backup_if_mismatch or !posix)
      if opts.output == null and (opts.backup or mismatch_backup) {
        let says_missing = facts.old_missing or facts.new_missing or facts.creating or facts.deleting
        make_backup(opts, name, exists, original, original_mode, says_missing)?
      }
      if content.len() == 0 and (opts.remove_empty or removes) {
        if exists and opts.output == null { remove_file(name)? }
      } else {
        if removes {
          note(opts, f"Not deleting file {q(opts, name)} as content differs from patch\n")
          status = 1
        }
        if opts.output != null or (exists and (content != original or target.write != target.read)) or (!exists and (content.len() > 0 or failed == 0)) {
          let parent = dirname_of(outname)
          if parent != "" and !file_exists(parent) { fp"{parent}".mkdir(true)? }
          if symlink {
            let link = content.utf8() ?? ""
            let text = if link.ends_with("\n") { link.byte_slice(0, link.byte_len() - 1) } else { link }
            fp"{outname}".symlink(to: fp"{text}")?
          } else if opts.output != null {
            fp"{outname}".write(content, 384)?
          } else if exists and outname == name {
            fp"{outname}".write_atomic(content)?
          } else {
            fp"{outname}".write(content)?
          }
        }
        if target.mode >= 0 and !symlink and opts.output == null {
          fp"{outname}".chmod(without_bits(target.mode % 4096, fs.umask()?))?
        }
        if target.action == "renamed" and target.read != target.write and opts.output == null and failed == 0 {
          remove_file(target.read)?
        }
      }
    }
  }
  if (opts.set_time or opts.set_utc) and failed == 0 and !opts.dry_run and !outcome.skipped and outname != "-" {
    let old_stamp = patch_parse.parse_stamp(if outcome.reversed { scan.new_name.stamp } else { scan.old_name.stamp })
    let new_stamp = patch_parse.parse_stamp(if outcome.reversed { scan.old_name.stamp } else { scan.new_name.stamp })
    let seen = old_stamp.valid and exists and original_mtime != old_stamp.seconds * 1000000000 + old_stamp.nanos
    if seen {
      note(opts, f"Not setting time of file {q(opts, outname)} (time mismatch)\n")
    } else if outcome.mismatch {
      note(opts, f"Not setting time of file {q(opts, outname)} (contents mismatch)\n")
    } else if new_stamp.valid and !(new_stamp.seconds == 0 and new_stamp.nanos == 0) and file_exists(outname) {
      fs.set_times(fp"{outname}", atime_sec: new_stamp.seconds, atime_nsec: new_stamp.nanos, mtime_sec: new_stamp.seconds, mtime_nsec: new_stamp.nanos)?
    }
  }
  if failed > 0 {
    let word = if outcome.skipped { "ignored" } else { "FAILED" }
    var message = f"{failed} out of {outcome.total} hunk{plural(outcome.total)} {word}"
    if !opts.dry_run {
      message += f" -- saving rejects to file {q(opts, reject_name)}"
      write_rejects(opts, scan, outcome, reject_name, strip)?
    }
    note(opts, message + "\n")
    return {status: 1, next: outcome.next}
  }
  {status: status, next: outcome.next}
}

# `mode` with every bit that is set in `mask` cleared.
pure without_bits(mode: Int, mask: Int) -> Int {
  var result = mode
  var bit = 1
  for _ in range(12) {
    if mask / bit % 2 == 1 and result / bit % 2 == 1 { result -= bit }
    bit *= 2
  }
  result
}

# Remove a file and then any directories left empty above it.
proc remove_file(name: Str) [fs, error] -> Result[Unit, Error] {
  fp"{name}".remove()?
  var parent = dirname_of(name)
  while parent != "" {
    let trimmed = if parent.ends_with("/") { parent.byte_slice(0, parent.byte_len() - 1) } else { parent }
    if trimmed == "" { break }
    if fp"{trimmed}".remove_dir() is Err(_) { break }
    parent = dirname_of(trimmed)
  }
  Ok()
}

# The highest number of an existing `NAME.~N~` numbered backup beside
# `target`, or 0 when there is none.
proc highest_numbered(target: Str) [fs, error] -> Int {
  let parent = dirname_of(target)
  let directory = if parent == "" { "." } else { parent }
  let stem = basename_of(target)
  var highest = 0
  if !file_exists(directory) { return 0 }
  for entry in fs.children(fp"{directory}")? {
    if entry.name.starts_with(stem + ".~") and entry.name.ends_with("~") {
      let middle = entry.name.byte_slice(stem.byte_len() + 2, entry.name.byte_len() - stem.byte_len() - 3)
      let value = middle.parse_int() ?? -1
      if value > highest { highest = value }
    }
  }
  highest
}

# GNU patch's backup file name: version control style from -V or the
# environment, then prefixes and suffix. A prefix without an explicit suffix
# names the backup by the prefix alone.
proc backup_name(opts: Options, name: Str) [fs, env, error] -> Str {
  var style = version_style(opts.version_control ?? "")
  if style == "" { style = version_style(env.get_or("PATCH_VERSION_CONTROL", "") ?? "") }
  if style == "" { style = version_style(env.get_or("VERSION_CONTROL", "") ?? "") }
  let affixed = opts.prefix != null or opts.basename_prefix != null
  let stem = f"{opts.prefix ?? ""}{dirname_of(name)}{opts.basename_prefix ?? ""}{basename_of(name)}"
  let suffix = opts.suffix ?? (if affixed { "" } else { ".orig" })
  let highest = highest_numbered(stem)
  var numbered = style == "numbered"
  if style == "existing" or style == "" or style == "none" { numbered = highest > 0 }
  if numbered { return f"{stem}.~{highest + 1}~" }
  stem + suffix
}

# Copy the original aside. A file that is missing gets an empty backup when
# the patch itself says the file does not exist (it creates or deletes it);
# otherwise there is nothing to copy and the user is told.
proc make_backup(opts: Options, name: Str, exists: Bool, original: Bytes, mode: Int, says_missing: Bool) [fs, io, process, env, error] -> Result[Unit, Error] {
  if !exists and !says_missing {
    say(f"Cannot stat file {name}, skipping backup\n")
    return Ok()
  }
  let target = backup_name(opts, name)
  let parent = dirname_of(target)
  if parent != "" and !file_exists(parent) { fp"{parent}".mkdir(true)? }
  if mode >= 0 { fp"{target}".write(original, mode) } else { fp"{target}".write(original) }
}

proc write_rejects(opts: Options, scan: patch_parse.Scan, outcome: Outcome, reject_name: Str, strip: Int) [fs, io, process, env, error] -> Result[Unit, Error] {
  let as_context = if opts.reject_format != null { opts.reject_format == "context" } else { scan.kind != "unified" and scan.kind != "git" }
  var parts: List[Bytes] = []
  let old_header = if outcome.reversed { scan.new_name } else { scan.old_name }
  let new_header = if outcome.reversed { scan.old_name } else { scan.new_name }
  var header_old = "/dev/null"
  var header_new = "/dev/null"
  if scan.kind != "normal" {
    header_old = reject_name_of(old_header, strip) + (if old_header.stamp != "" { "\t" + old_header.stamp } else { "" })
    header_new = reject_name_of(new_header, strip) + (if new_header.stamp != "" { "\t" + new_header.stamp } else { "" })
  }
  if as_context {
    parts += [bytes.from_text(f"*** {header_old}\n--- {header_new}\n")]
    for hunk in outcome.rejected { parts += [context_reject(hunk, scan.kind == "normal")] }
  } else {
    parts += [bytes.from_text(f"--- {header_old}\n+++ {header_new}\n")]
    for hunk in outcome.rejected { parts += [unified_reject(hunk)] }
  }
  fp"{reject_name}".write(bytes.concat(parts))
}

# A rejected hunk is written in terms of the partly patched file, so its line
# numbers move by the net size change of the hunks already applied.
pure shifted(hunk: patch_parse.Hunk, by: Int) -> patch_parse.Hunk {
  var out = hunk
  out.old_first += by
  out.new_first += by
  out
}

# The name a reject header shows for a patch header name: `/dev/null` stays,
# anything else appears after `-p` stripping.
pure reject_name_of(header: patch_parse.HeaderName, strip: Int) -> Str {
  if header.name == "/dev/null" or !header.present { return header.name }
  let stripped = patch_parse.strip_name(header.name, strip)
  if stripped == "" { header.name } else { stripped }
}

# Apply every hunk of the file patch to `input_lines` and report the result.
proc run_hunks(opts: Options, max_fuzz: Int, lines: List[Bytes], scan: patch_parse.Scan, input_lines: List[Bytes], start_reversed: Bool, facts: Facts, assumed: Bool, start_skip: Bool) [io, process, env, error] -> Outcome {
  var reverse = start_reversed
  var output: List[Bytes] = []
  var rejected: List[patch_parse.Hunk] = []
  var total = 0
  var mismatch = assumed
  var skip = start_skip
  var position = scan.body
  var in_offset = 0
  var last_frozen = 0
  var out_offset = 0
  var copied = 0
  let loose = opts.ignore_whitespace
  let strip_cr = scan.cr_header and !opts.binary
  let inkeys = patch_apply.keys(input_lines, loose)
  var detected = assumed
  loop {
    if position >= lines.len() { break }
    let read = read_one(lines, position, scan.kind, strip_cr)
    if read.bad_line != 0 {
      return {output: output, rejected: rejected, total: total, mismatch: mismatch, skipped: skip, next: position, fatal_line: read.bad_line, fatal_text: read.bad_text, reversed: reverse}
    }
    let parsed = read.hunk
    if parsed == null { break }
    position = read.next
    total += 1
    var hunk: patch_parse.Hunk = parsed ?? patch_parse.EMPTY_HUNK
    if reverse { hunk = patch_apply.reversed(hunk) }
    if skip {
      if opts.verbose { note(opts, f"Hunk #{total} ignored at {hunk.old_first + out_offset}.\n") }
      rejected += [shifted(hunk, out_offset)]
      continue
    }
    var pattern = patch_apply.keys(patch_apply.pattern(hunk), loose)
    let context = if hunk.prefix < hunk.suffix { hunk.suffix } else { hunk.prefix }
    let limit = if max_fuzz < context { max_fuzz } else { context }
    var fuzz = 0
    var where = 0
    while fuzz <= limit {
      where = patch_apply.locate(inkeys, pattern, hunk, fuzz, in_offset, last_frozen)
      if where > 0 { break }
      if total == 1 and !opts.force and !detected {
        let other = patch_apply.reversed(hunk)
        let other_pattern = patch_apply.keys(patch_apply.pattern(other), loose)
        let found = patch_apply.locate(inkeys, other_pattern, other, fuzz, in_offset, last_frozen)
        if found > 0 {
          detected = true
          let heading = if reverse { "Unreversed patch detected!  " } else { "Reversed (or previously applied) patch detected!  " }
          if opts.forward {
            note(opts, heading + "Skipping patch.\n")
            skip = true
          } else if opts.batch {
            note(opts, heading + (if reverse { "Ignoring -R.\n" } else { "Assuming -R.\n" }))
            mismatch = true
            reverse = !reverse
            hunk = other
            pattern = other_pattern
            where = found
          } else {
            note(opts, heading + (if reverse { "Ignore -R? [n] \n" } else { "Assume -R? [n] \n" }))
            note(opts, "Apply anyway? [n] \nSkipping patch.\n")
            skip = true
          }
          break
        }
      }
      fuzz += 1
    }
    if skip {
      if opts.verbose { note(opts, f"Hunk #{total} ignored at {hunk.old_first + out_offset}.\n") }
      rejected += [shifted(hunk, out_offset)]
      continue
    }
    # A hunk that creates the file cannot apply to one that has content.
    if where > 0 and pattern.is_empty() and !input_lines.is_empty() and total == 1 and (if reverse { facts.new_missing } else { facts.old_missing }) { where = 0 }
    if where == 0 {
      mismatch = true
      var blame = ""
      if !opts.quiet {
        # GNU patch blames line endings when the first input line and the
        # first hunk line end differently.
        let input_crlf = !input_lines.is_empty() and input_lines[0].ends_with(b"\r\n")
        let hunk_crlf = !hunk.texts.is_empty() and hunk.texts[0].ends_with(b"\r\n")
        if input_crlf != hunk_crlf { blame = " (different line endings)" }
        note(opts, f"Hunk #{total} FAILED at {hunk.old_first + out_offset}{blame}.\n")
      }
      rejected += [shifted(hunk, out_offset)]
      continue
    }
    let offset = where - hunk.old_first
    if fuzz > 0 or offset != 0 { mismatch = true }
    if !opts.quiet and (fuzz > 0 or offset != 0 or opts.verbose) {
      var message = f"Hunk #{total} succeeded at {where + out_offset}"
      if fuzz > 0 { message += f" with fuzz {fuzz}" }
      if offset != 0 { message += f" (offset {offset} line{plural(offset)})" }
      note(opts, message + ".\n")
    }
    # Copy untouched input up to the hunk, then merge its lines.
    let begin = where - 1
    var index = copied
    while index < begin and index < input_lines.len() {
      output += [input_lines[index]]
      index += 1
    }
    var at = begin
    var step = 0
    while step < hunk.kinds.len() {
      let kind = hunk.kinds[step]
      if kind == 0 {
        if at < input_lines.len() { output += [input_lines[at]] }
        at += 1
        step += 1
        continue
      }
      var end = step
      var removed: List[Bytes] = []
      var added: List[Bytes] = []
      while end < hunk.kinds.len() and hunk.kinds[end] != 0 {
        if hunk.kinds[end] == 1 { removed += [hunk.texts[end]] } else { added += [hunk.texts[end]] }
        end += 1
      }
      at += removed.len()
      step = end
      if let name = opts.ifdef {
        # Merged output: the old text under #ifndef, the new under #else.
        if !removed.is_empty() and !added.is_empty() {
          output += [bytes.from_text(f"#ifndef {name}\n")] + removed + [b"#else\n"] + added + [b"#endif\n"]
        } else if !added.is_empty() {
          output += [bytes.from_text(f"#ifdef {name}\n")] + added + [b"#endif\n"]
        } else {
          output += [bytes.from_text(f"#ifndef {name}\n")] + removed + [b"#endif\n"]
        }
      } else {
        output += added
      }
    }
    copied = at
    last_frozen = at
    out_offset += hunk.new_count - hunk.old_count
    in_offset = offset
  }
  var index = copied
  while index < input_lines.len() {
    output += [input_lines[index]]
    index += 1
  }
  {output: output, rejected: rejected, total: total, mismatch: mismatch, skipped: skip, next: position, fatal_line: 0, fatal_text: b"", reversed: reverse}
}
