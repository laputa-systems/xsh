#!/bin/xsh
use lib.gnu

type Options = {terminal: Str?, no_init: Bool, help: Bool, version: Bool, types: List[Str]}

proc main(...argv: List[Str]) [env, process, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    terminal: {form: "-T TERM"},
    no_init: {form: "-I", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
    types: {form: "...TERM"},
  })?
  if opts.help { gnu.help("Usage: reset [-T TERM] [-I] [TERM]\nRestore sane terminal modes and initialize an ANSI terminal.\n-I omits terminal initialization sequences."); return }
  if opts.version { gnu.version("reset"); return }
  if opts.types.len() > 1 { gnu.extra_operand(opts.types[1]) }
  let terminal = opts.terminal ?? opts.types.get(0) ?? env.get_or("TERM", "") ?? ""
  if ! (terminal.starts_with("xterm") or terminal.starts_with("screen") or terminal.starts_with("tmux") or terminal.starts_with("rxvt") or terminal in ["linux", "ansi", "vt100", "vt102", "vt220"]) {
    gnu.error(f"unsupported terminal type {gnu.quote(terminal)}")
    exit 1
  }
  var changed = false
  # Try inherited terminal descriptors without opening or changing another
  # device. The first terminal supplies the attributes that sane mode preserves.
  for fd in [0, 1, 2] {
    if let Ok(attrs) = unix.tty_attrs(fd) {
      let sane = unix.tty_mode(attrs, "sane")?
      if let Err(failure) = unix.set_tty_attrs(sane, fd: fd, when: "flush") { gnu.name_error("terminal", failure); exit 1 }
      changed = true
      break
    }
  }
  if !changed { gnu.error("cannot find an inherited terminal descriptor"); exit 1 }
  if !opts.no_init { gnu.write_text("\x1bc\x1b[?25h") }
}
