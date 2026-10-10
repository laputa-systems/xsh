#!/bin/xsh
use lib.gnu
use lib.fs_misc

type Options = {mode: Str?, security_context: Bool, context: List[Str], help: Bool, version: Bool, paths: List[Str]}
type ModeSource = {index: Int, start: Int}

pure mode_source(argv: List[Str]) -> ModeSource {
  for index in range(argv.len()) {
    let arg = argv[index]
    if arg in ["-m", "--mode"] { return {index: index + 1, start: 0} }
    if arg.starts_with("--mode=") { return {index: index, start: 7} }
    if arg.starts_with("-m") and arg.byte_len() > 2 { return {index: index, start: 2} }
  }
  {index: 0, start: 0}
}

pure bad_mode_offset(mode: Str) -> Int {
  for index in range(mode.byte_len()) {
    let char = mode.byte_slice(index, length: 1)
    if char not in "ugoa+-=rwxXst01234567,0123456789" { return index }
  }
  mode.byte_len() - 1
}

proc mode_error(argv: List[Str], mode: Str) [env, process, error] {
  let message = f"invalid mode {gnu.quote(mode)}"
  gnu.error(message)
  let requested = env.get_or("UUTILS_DIAG", "") ?? ""
  return when requested == "never"
  if requested != "always" and ! unix.isatty(2) { return }
  let source = mode_source(argv)
  let program = gnu.prog()
  let command = argv.join(" ")
  var column = 0
  for index in range(source.index) { column += argv[index].byte_len() + 1 }
  column += source.start + bad_mode_offset(mode)
  let spacing = [" " for _ in range(column)].join("")
  eprint f"   ╭─[ {program}:1:{column + 1} ]"
  eprint "   │"
  eprint f" 1 │ {command}"
  eprint f"   │ {spacing}─┬"
  eprint f"   │ {spacing} ╰─ invalid mode character"
  eprint "───╯"
}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    mode: {form: "-m --mode MODE"},
    # Without SELinux or SMACK there is no label to set, so -Z and a bare --context change nothing.
    # argv cannot carry NUL, so the bare --context value is NUL and is distinct from --context=.
    security_context: {form: "-Z", default: false},
    context: {form: "--context[=CTX]", repeated: true, optional_default: "\0"},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...NAME"},
  })?
  for context in opts.context {
    # GNU reports each explicit label while parsing, so these lines precede any later diagnostic.
    if context != "\0" { gnu.error("warning: ignoring --context; it requires an SELinux/SMACK-enabled kernel") }
  }
  if opts.help { gnu.help("Usage: mkfifo [OPTION]... NAME...\nCreate named pipes.\n  -m, --mode=MODE  set permission bits\n"); return }
  if opts.version { gnu.version("mkfifo"); return }
  if opts.paths.is_empty() { gnu.missing_operand() }
  var mode = 0o666
  if opts.mode != null {
    let parsed = fs_misc.mode_for(opts.mode ?? "", 0o666, false, umask: fs.umask()?)
    if let Err(failure) = parsed { mode_error(argv, opts.mode ?? ""); exit 1 }
    mode = parsed?
    if mode > 0o777 { gnu.error("mode must specify only file permission bits"); exit 1 }
  }
  var failed = false
  for name in opts.paths {
    let made = fs.mkfifo(fp"{name}", mode)
    if let Err(failure) = made { gnu.cannot("create fifo", name, failure); failed = true } else if opts.mode != null {
      if let Err(failure) = fp"{name}".chmod(mode) { gnu.cannot("set permissions of", name, failure); failed = true }
    }
  }
  if failed { exit 1 }
}
