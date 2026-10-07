#!/bin/xsh
use lib.gnu
use lib.search as search

type RmOptions = {recursive: Bool, force: Bool, directory: Bool, verbose: Bool, progress: Bool, one_file_system: Bool, preserve_root: Str, no_preserve_root: Bool, interactive_always: Bool, interactive_once: Bool, interactive: Str?, help: Bool, version: Bool, targets: List[Str]}
type RawArgument = {marker: Str, value: Bytes}
type RmArguments = {argv: List[Str], raw: List[RawArgument], presume_input_tty: Bool}
type Removal = {ok: Bool, removed: Bool, write_error: Error?}

pure prepare_arguments(argv: List[Bytes], posix: Bool) -> RmArguments {
  var text: List[Str] = []
  var raw: List[RawArgument] = []
  var options = true
  var presume_input_tty = false
  for index in range(argv.len()) {
    let argument = argv[index]
    if options and argument == b"---presume-input-tty" {
      presume_input_tty = true
      continue
    }
    match argument.utf8() {
      Ok(value) => text += [value]
      Err(_) => {
        let marker = f"\0rm-raw-argument-{index}\0"
        text += [marker]
        raw += [{marker: marker, value: argument}]
      }
    }
    if argument == b"--" or (posix and options and (argument == b"-" or ! argument.starts_with(b"-"))) { options = false }
  }
  {argv: text, raw: raw, presume_input_tty: presume_input_tty}
}

pure argument_bytes(value: Str, raw: List[RawArgument]) -> Bytes {
  for argument in raw { if argument.marker == value { return argument.value } }
  bytes.from_text(value)
}

pure operand_label(raw: Bytes) -> Bytes {
  var end = raw.len()
  while end > 0 and raw.byte_at(end - 1) == 47 { end -= 1 }
  return b"/" when end == 0 and raw != b""
  let trimmed = raw[..end]
  let base = search.basename_bytes(trimmed)
  if end < raw.len() and (base == b"." or base == b"..") { bytes.concat([trimmed, b"/"]) } else { trimmed }
}

proc confirm(question: Str) [process, io, error] -> Bool {
  io.write_stderr(f"{gnu.prog()}: {question}? ")?
  io.flush_stderr()?
  let reply = io.stdin_line()?
  reply.trim().lower().starts_with("y")
}

pure file_kind(kind: Str, size: Int) -> Str {
  if kind == "file" { if size == 0 { "regular empty file" } else { "regular file" } } else if kind == "symlink" { "symbolic link" } else if kind == "dir" { "directory" } else if kind == "char" { "character special file" } else if kind == "block" { "block special file" } else if kind == "fifo" { "fifo" } else { kind }
}

# Flush before the next prompt. Return write failures so removal continues
# and the caller reports a broken output stream once after processing operands.
proc report_removed(name: Bytes, directory: Bool) [process, env, io, error] -> Result[Unit] {
  let verb = if directory { "removed directory" } else { "removed" }
  io.write_stdout(f"{verb} {gnu.quote_bytes(name)}\n")?
  io.flush_stdout()
}

proc report_progress(name: Bytes, opts: RmOptions) [process, env, io, error] -> Result[Unit] {
  return when ! opts.progress or opts.verbose or unix.tty_attrs(2) is Err(_)
  io.write_stderr(f"\rRemoving {gnu.quote_bytes(name)}... done\n")?
  io.flush_stderr()
}

# Classify with lstat: a symbolic link to a directory is removed as a link,
# and recursive traversal never crosses into its referent.
proc remove_target(target: Path, name: Bytes, opts: RmOptions, device: Int, interactive: Bool, automatic: Bool, presume_input_tty: Bool) [fs, process, env, error, io] -> Removal {
  guard let meta = fs.stat(target) else { |failure|
    return {ok: true, removed: true, write_error: null} when opts.force and gnu.errno(failure) == 2
    gnu.error(f"cannot remove {gnu.quote_bytes(name)}: {gnu.strerror(failure)}")
    return {ok: false, removed: false, write_error: null}
  }
  if opts.one_file_system and meta.kind == "dir" and meta.dev != device {
    gnu.error(f"skipping {gnu.quote_bytes(name)}, since it's on a different device")
    return {ok: true, removed: false, write_error: null}
  }
  var success = true
  var write_error: Error? = null
  var protected = false
  if ! opts.force and meta.kind != "symlink" and (interactive or (automatic and (unix.isatty(0) or presume_input_tty))) {
    match fs.access(target, write: true) {
      Ok(writable) => protected = ! writable
      Err(failure) => { gnu.error(f"cannot remove {gnu.quote_bytes(name)}: {gnu.strerror(failure)}")
        return {ok: false, removed: false, write_error: null} }
    }
  }
  let protection = if protected { "write-protected " } else { "" }
  if meta.kind == "dir" and opts.recursive {
    guard let children = fs.children(target, stat: false) else { |failure|
      if gnu.errno(failure) == 13 {
        if interactive {
          return {ok: true, removed: false, write_error: null} when ! confirm(f"attempt removal of inaccessible directory {gnu.quote_bytes(name)}")
        }
        if target.remove_dir() is Ok(_) {
          if opts.verbose {
            if let Err(output_failure) = report_removed(name, true) { write_error = output_failure }
          }
          if let Err(output_failure) = report_progress(name, opts) {
            if write_error == null { write_error = output_failure }
          }
          return {ok: true, removed: true, write_error: write_error}
        }
      }
      gnu.error(f"cannot remove {gnu.quote_bytes(name)}: {gnu.strerror(failure)}")
      return {ok: false, removed: false, write_error: null}
    }
    let entries = children.collect()
    if (interactive or protected) and ! entries.is_empty() {
      return {ok: true, removed: false, write_error: null} when ! confirm(f"descend into {protection}directory {gnu.quote_bytes(name)}")
    }
    var retained = false
    for child in entries {
      let base = search.basename_bytes(child.path.bytes())
      let child_name = if name == b"/" { bytes.concat([b"/", base]) } else { bytes.concat([name, b"/", base]) }
      let child_target = search.child(target, base)?
      let result = remove_target(child_target, child_name, opts, device, interactive, automatic, presume_input_tty)
      if ! result.ok { success = false }
      if let failure = result.write_error { if write_error == null { write_error = failure } }
      if ! result.removed { retained = true }
    }
    return {ok: success, removed: false, write_error: write_error} when retained

  }
  if meta.kind == "dir" and ! opts.recursive and ! opts.directory {
    gnu.error(f"cannot remove {gnu.quote_bytes(name)}: Is a directory")
    return {ok: false, removed: false, write_error: null}
  }
  if interactive or protected {
    var question = f"remove {protection}{file_kind(meta.kind, meta.size)} {gnu.quote_bytes(name)}"
    if meta.kind == "dir" and ! opts.recursive {
      if let Ok(accessible) = fs.access(target, read: true, execute: true) {
        if ! accessible { question = f"attempt removal of inaccessible directory {gnu.quote_bytes(name)}" }
      }
    }
    return {ok: true, removed: false, write_error: write_error} when ! confirm(question)
  }
  let removed = if meta.kind == "dir" { target.remove_dir() } else { target.remove(missing_ok: false) }
  match removed {
    Ok(_) => {
      if opts.verbose {
        if let Err(failure) = report_removed(name, meta.kind == "dir") {
          if write_error == null { write_error = failure }
        }
      }
      if let Err(failure) = report_progress(name, opts) {
        if write_error == null { write_error = failure }
      }
    }
    Err(failure) => {
      if ! (opts.force and gnu.errno(failure) == 2) {
        gnu.error(f"cannot remove {gnu.quote_bytes(name)}: {gnu.strerror(failure)}")
        success = false
      }
    }
  }
  {ok: success, removed: success, write_error: write_error}
}

# An invalid option can be a real dash-prefixed filename. Stop at the first
# parse problem so a later operand never changes the reported error.
proc dash_filename_hint(argv: List[Str]) [fs, process, env, error] {
  let posix = env.get("POSIXLY_CORRECT") is Ok(_)
  let long_names = ["force", "interactive", "recursive", "dir", "one-file-system", "preserve-root", "no-preserve-root", "verbose", "help", "version", "-presume-input-tty", "progress"]
  for raw in argv {
    break when raw in ["--", "--help", "--version"]
    if raw.starts_with("--") {
      let name = raw.byte_slice(2).split("=")[0]
      var matches = []
      for candidate in long_names { if candidate.starts_with(name) { matches += [candidate] } }
      if name in long_names { matches = [name] }
      break when matches.len() != 1
      let option = matches[0]
      break when option in ["help", "version"]
      break when option == "no-preserve-root" and name != option
      break when "=" in raw and option not in ["interactive", "preserve-root"]
      continue
    }
    if raw == "-" or ! raw.starts_with("-") {
      break when posix
      continue
    }
    for letter in raw.byte_slice(1) {
      if letter not in "dfgiIrvR" {
        if fs.stat(fp"{raw}") is Ok(_) {
          gnu.error(f"invalid option -- {gnu.quote_value(letter)}")
          eprint f"Try '{gnu.phrase()} ./{gnu.quote_maybe(raw)}' to remove the file {gnu.quote(raw)}."
          gnu.try_help()
          exit 1
        }
        return
      }
    }
  }
}

proc main(...argv: List[Bytes]) [fs, process, env, error, io] {
  let posix = env.get("POSIXLY_CORRECT") is Ok(_)
  let prepared = prepare_arguments(argv, posix)
  dash_filename_hint(prepared.argv)
  let opts: RmOptions = cli.applet(prepared.argv, {
    gnu: {status: 1},
    interactive_always: {form: "-i", default: false, conflicts: ["force", "interactive_once", "interactive"]},
    interactive_once: {form: "-I", default: false, conflicts: ["force", "interactive_always", "interactive"]},
    interactive: {form: "--interactive[=WHEN]", optional_default: "always", conflicts: ["force", "interactive_always", "interactive_once"]},
    recursive: {form: "-r -R --recursive", default: false},
    force: {form: "-f --force", default: false, conflicts: ["interactive_always", "interactive_once", "interactive"]},
    directory: {form: "-d --dir", default: false},
    progress: {form: "-g --progress", default: false},
    one_file_system: {form: "--one-file-system", default: false},
    preserve_root: {form: "--preserve-root[=WHEN]", default: "root", optional_default: "root", conflicts: ["no_preserve_root"]},
    no_preserve_root: {form: "--no-preserve-root", default: false, conflicts: ["preserve_root"]},
    verbose: {form: "-v --verbose", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    targets: {form: "...FILE"},
  })?
  if opts.help {
    gnu.help("Usage: rm [OPTION]... FILE...\nRemove files and directories.\n  -f, --force  ignore nonexistent files\n  -i  prompt for each removal\n  -I  prompt once for recursive removal or more than three operands\n      --interactive[=WHEN]  never, once, or always\n  -r, -R, --recursive  remove directory contents\n  -d, --dir  remove empty directories\n  -v, --verbose  report removals\n  -g, --progress  display a progress indicator on a terminal\n      --one-file-system  skip directories on a different device\n      --preserve-root[=all]  protect /; all also rejects mount-point operands\n      --no-preserve-root  allow recursive operation on /\n")
    return
  }
  if opts.version { gnu.version("rm")
    return }
  for token in cli.tokens(prepared.argv)? {
    break when posix and token.kind == "operand"
    if token.kind == "long" and token.name != "no-preserve-root" and "no-preserve-root".starts_with(token.name) {
      gnu.error("you may not abbreviate the --no-preserve-root option")
      exit 1
    }
  }
  if opts.preserve_root not in ["root", "all"] {
    gnu.usage_error(f"unrecognized --preserve-root argument: {gnu.quote_value(opts.preserve_root)}")
  }
  let preserve_root = ! opts.no_preserve_root
  let force = opts.force
  var prompt_mode = if opts.interactive_always { 2 } else if opts.interactive_once { 1 } else { 0 }
  let automatic = ! force and ! opts.interactive_always and ! opts.interactive_once and opts.interactive == null
  if let choice = opts.interactive {
    if choice in ["never", "no", "none"] { prompt_mode = 0 } else if choice == "once" { prompt_mode = 1 } else if choice in ["always", "yes"] { prompt_mode = 2 } else { gnu.usage_error(f"invalid argument {gnu.quote_value(choice)} for 'interactive'") }
  }
  let targets: List[Bytes] = collect { for target in opts.targets { yield argument_bytes(target, prepared.raw) } }
  if targets.is_empty() {
    return when force
    gnu.missing_operand()
  }
  if prompt_mode == 1 and (opts.recursive or targets.len() > 3) {
    let count = targets.len()
    let noun = if count == 1 { "argument" } else { "arguments" }
    let recursive = if opts.recursive { " recursively" } else { "" }
    return when ! confirm(f"remove {count} {noun}{recursive}")
  }
  var success = true
  var write_error: Error? = null
  let root = fs.stat(p"/")?
  for name in targets {
    let target = Path.parse_bytes(name)?
    let base = search.basename_bytes(target.bytes())
    if base == b"." or base == b".." {
      gnu.error(f"refusing to remove '.' or '..' directory: skipping {gnu.quote_bytes(operand_label(name))}")
      success = false
      continue
    }
    if opts.recursive {
      if let Ok(meta) = fs.stat(target) {
        if preserve_root and meta.kind == "dir" and meta.dev == root.dev and meta.ino == root.ino {
          let same = if target.bytes() == b"/" { "" } else { " (same as '/')" }
          gnu.error(f"it is dangerous to operate recursively on {gnu.quote_bytes(name)}{same}")
          gnu.error("use --no-preserve-root to override this failsafe")
          success = false
          continue
        }
      }
    }
    if opts.preserve_root == "all" {
      if let Ok(meta) = fs.stat(target) {
        if let Ok(parent) = fs.stat(target.parent(), follow_symlinks: true) {
          if meta.dev != parent.dev {
            gnu.error(f"skipping {gnu.quote_bytes(name)}, since it's on a different device")
            success = false
            continue
          }
        }
      }
    }
    let device = match fs.stat(target) { Ok(meta) => meta.dev, Err(_) => 0 }
    let result = remove_target(target, operand_label(name), opts, device, prompt_mode == 2, automatic, prepared.presume_input_tty)
    if ! result.ok { success = false }
    if let failure = result.write_error { if write_error == null { write_error = failure } }
  }
  if let failure = write_error { gnu.write_failed(failure) }
  if ! success { exit 1 }
}
