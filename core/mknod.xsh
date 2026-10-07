#!/bin/xsh
use lib.gnu
use lib.fs_misc

type Options = {mode: Str?, help: Bool, version: Bool, operands: List[Str]}
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
  # Keep supported long-option abbreviations while matching unknown-option diagnostics.
  var option_at = 0
  while option_at < argv.len() {
    let arg = argv[option_at]
    break when arg == "--"
    if arg.starts_with("--") {
      let option_name = arg.split("=")[0]
      var recognized = false
      for option in ["--mode", "--help", "--version"] {
        if option.starts_with(arg) or (option == "--mode" and option.starts_with(option_name)) { recognized = true }
      }
      if ! recognized { gnu.error(f"unexpected argument {gnu.quote(arg)} found"); exit 1 }
    }
    option_at += 1
  }
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1, unsupported: {"-Z": "security labels require a native security context API", "--context": "security labels require a native security context API"}},
    mode: {form: "-m --mode MODE"},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    operands: {form: "...ARG"},
  })?
  if opts.help { gnu.help("Usage: mknod [OPTION]... NAME TYPE [MAJOR MINOR]\nCreate a special file.\n  -m, --mode=MODE  set permission bits\nTYPE is b (block), c or u (character), or p (FIFO).\n"); return }
  if opts.version { gnu.version("mknod"); return }
  let args = opts.operands
  if args.len() < 2 { if args.is_empty() { gnu.missing_operand() } else { gnu.missing_operand_after(args[0]) } }
  let kind = match bytes.from_text(args[1]).byte_at(0) ?? 0 { 112 => "fifo", 98 => "block", 99 => "char", 117 => "char", else => "" }
  if kind == "" { gnu.usage_error(f"invalid device type {gnu.quote(args[1])}") }
  let count = if kind == "fifo" { 2 } else { 4 }
  if args.len() < count {
    gnu.error(f"missing operand after {gnu.quote(args[-1])}")
    eprint "Special files require major and minor device numbers."
    gnu.try_help()
    exit 1
  }
  if args.len() > count {
    gnu.error(f"extra operand {gnu.quote(args[count])}")
    if kind == "fifo" { eprint "Fifos do not have major and minor device numbers." }
    gnu.try_help()
    exit 1
  }
  var major = 0
  var minor = 0
  if kind != "fifo" {
    let a = args[2].parse_int()
    let b = args[3].parse_int()
    if a is Err(_) or ((a ?? -1) < 0 or (a ?? -1) > 4294967295) { gnu.error(f"invalid value {gnu.quote(args[2])}"); exit 1 }
    if b is Err(_) or ((b ?? -1) < 0 or (b ?? -1) > 4294967295) { gnu.error(f"invalid value {gnu.quote(args[3])}"); exit 1 }
    major = a?
    minor = b?
  }
  var mode = 0o666
  if opts.mode != null {
    let parsed = fs_misc.mode_for(opts.mode ?? "", mode, false, umask: fs.umask()?)
    if parsed is Err(_) { mode_error(argv, opts.mode ?? ""); exit 1 }
    mode = parsed?
    if mode > 0o777 { gnu.error("mode must specify only file permission bits"); exit 1 }
  }
  if let Err(failure) = fs.mknod(fp"{args[0]}", kind, mode, major: major, minor: minor) { gnu.name_error(args[0], failure); exit 1 }
  if opts.mode != null { if let Err(failure) = fp"{args[0]}".chmod(mode) { gnu.name_error(args[0], failure); exit 1 } }
}
