#!/bin/xsh
use lib.gnu
use lib.fs_misc

type Options = {mode: Str?, help: Bool, version: Bool, paths: List[Str]}

proc main(...argv: List[Str]) [fs, process, env, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1, unsupported: {"-Z": "security labels require a native security context API", "--context": "security labels require a native security context API"}},
    mode: {form: "-m --mode MODE"},
    help: {form: "--help", default: false, stop: true},
    version: {form: "--version", default: false, stop: true},
    paths: {form: "...NAME"},
  })?
  if opts.help { gnu.help("Usage: mkfifo [OPTION]... NAME...\nCreate named pipes.\n  -m, --mode=MODE  set permission bits\n"); return }
  if opts.version { gnu.version("mkfifo"); return }
  if opts.paths.is_empty() { gnu.missing_operand() }
  var mode = 0o666
  if opts.mode != null {
    let parsed = fs_misc.mode_for(opts.mode ?? "", 0o666, false)
    if let Err(failure) = parsed { gnu.error(f"invalid mode {gnu.quote(opts.mode ?? "")}"); exit 1 }
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
