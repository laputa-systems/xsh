#!/bin/xsh
use lib.gnu

type Options = {terminal: Str?, no_scrollback: Bool, help: Bool, version: Bool}

proc main(...argv: List[Str]) [env, process, error, io] {
  let opts: Options = cli.applet(argv, {
    gnu: {status: 1},
    terminal: {form: "-T TERM"},
    no_scrollback: {form: "-x", default: false},
    help: {form: "--help", default: false, stop: true},
    version: {form: "-V --version", default: false, stop: true},
  })?
  if opts.help { gnu.help("Usage: clear [-T TERM] [-x]\nClear an ANSI terminal. -x preserves its scrollback."); return }
  if opts.version { gnu.version("clear"); return }
  let terminal = opts.terminal ?? env.get_or("TERM", "") ?? ""
  if terminal == "" { gnu.error("TERM environment variable not set."); exit 1 }
  if ! (terminal.starts_with("xterm") or terminal.starts_with("screen") or terminal.starts_with("tmux") or terminal.starts_with("rxvt") or terminal in ["linux", "ansi", "vt100", "vt102", "vt220"]) {
    gnu.error(f"unsupported terminal type {gnu.quote(terminal)}")
    exit 1
  }
  gnu.write_text("\x1b[H\x1b[2J" + (if opts.no_scrollback { "" } else { "\x1b[3J" }))
}
