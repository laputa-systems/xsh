##! Owns the monitor's raw terminal session and deterministic display capabilities.
##!
##! Capability selection is a pure function of captured environment values and
##! window size; terminal mutation stays behind the session boundary.
use lib.gnu

## Names the largest ANSI color range the captured terminal advertises.
export enum ColorDepth: Str {
  NoColor = "none",
  Ansi16 = "ansi16",
  Ansi256 = "ansi256",
  TrueColor = "truecolor",
}

## Chooses printable symbols according to the captured locale encoding.
export enum SymbolSet: Str {
  Ascii = "ascii",
  Unicode = "unicode",
}

## Captures the environment and geometry used to choose display capabilities.
export type Inputs = {
  term: Str,
  colorterm: Str,
  no_color: Bool,
  lc_all: Str,
  lc_ctype: Str,
  lang: Str,
  rows: Int,
  cols: Int,
}

## Describes the terminal's usable color, encoding, symbols, and dimensions.
export type Capabilities = {
  color_depth: ColorDepth,
  utf8: Bool,
  symbols: SymbolSet,
  rows: Int,
  cols: Int,
  minimum_size: Bool,
}

## Retains terminal state and the exact inputs used for capability selection.
export type Session = {attrs: UnixTtyAttrs, inputs: Inputs, capabilities: Capabilities}

pure locale(inputs: Inputs) -> Str {
  return inputs.lc_all when inputs.lc_all != ""
  return inputs.lc_ctype when inputs.lc_ctype != ""
  inputs.lang
}

## Selects only capabilities implied by the supplied terminal environment.
export pure detect(inputs: Inputs) -> Capabilities {
  let term = inputs.term.lower()
  let colorterm = inputs.colorterm.lower()
  let locale_name = locale(inputs).lower()
  let utf8 = locale_name.find("utf-8") != null or locale_name.find("utf8") != null
  let color_depth: ColorDepth = if inputs.no_color or term == "" or term == "dumb" {
    .NoColor
  } else if colorterm == "truecolor" or colorterm == "24bit" or term.ends_with("-direct") {
    .TrueColor
  } else if term.find("256color") != null {
    .Ansi256
  } else {
    .Ansi16
  }
  {
    color_depth: color_depth,
    utf8: utf8,
    symbols: if utf8 { .Unicode } else { .Ascii },
    rows: inputs.rows,
    cols: inputs.cols,
    minimum_size: inputs.rows >= 24 and inputs.cols >= 80,
  }
}

proc environment_inputs(rows: Int, cols: Int) [env] -> Inputs {
  {
    term: env.get_or("TERM", "") ?? "",
    colorterm: env.get_or("COLORTERM", "") ?? "",
    no_color: env.get("NO_COLOR") is Ok(_),
    lc_all: env.get_or("LC_ALL", "") ?? "",
    lc_ctype: env.get_or("LC_CTYPE", "") ?? "",
    lang: env.get_or("LANG", "") ?? "",
    rows: rows,
    cols: cols,
  }
}

pure cleanup_sequences() -> Str {
  "\u{1b}[0m\u{1b}[?2026l\u{1b}[?2004l\u{1b}[?25h\u{1b}[?1049l"
}

pure usable_inputs(inputs: Inputs) -> Bool {
  inputs.rows > 0 and inputs.cols > 0 and inputs.rows <= 2000 and inputs.cols <= 2000 and
    inputs.term.byte_len() <= 256 and inputs.colorterm.byte_len() <= 256 and
    inputs.lc_all.byte_len() <= 256 and inputs.lc_ctype.byte_len() <= 256 and inputs.lang.byte_len() <= 256
}

proc start(selected: Inputs?) [env, process, io, error] -> Result[Session, Error] {
  if ! unix.isatty(0) or ! unix.isatty(1) {
    return Err(error.failure("btop requires interactive stdin and stdout terminals"))
  }
  let size = unix.window_size(1)?
  if size.rows <= 0 or size.cols <= 0 {
    return Err(error.failure("btop could not read a usable terminal size"))
  }
  let inputs = if selected == null { environment_inputs(size.rows, size.cols) } else { selected ?? environment_inputs(size.rows, size.cols) }
  if ! usable_inputs(inputs) {
    return Err(error.failure("btop terminal inputs exceed their bounds"))
  }
  let attrs = unix.tty_attrs(0)?
  let capabilities = detect(inputs)
  let raw = unix.tty_mode(attrs, "raw")?
  var transferred = false
  var output_may_be_active = false
  defer {
    if ! transferred {
      defer { unix.set_tty_attrs(attrs, fd: 0) }
      if output_may_be_active {
        gnu.write_text(cleanup_sequences())
      }
    }
  }
  unix.set_tty_attrs(raw, fd: 0)
  output_may_be_active = true
  gnu.write_text("\u{1b}[?1049h\u{1b}[?25l\u{1b}[?2004h\u{1b}[2J\u{1b}[H")
  transferred = true
  Ok({attrs: attrs, inputs: inputs, capabilities: capabilities})
}

## Enters raw input using the current process environment and terminal size.
export proc begin() [env, process, io, error] -> Result[Session, Error] {
  start(null)
}

## Enters raw input using explicitly captured replay environment values.
export proc begin_with(inputs: Inputs) [env, process, io, error] -> Result[Session, Error] {
  start(inputs)
}

## Restores terminal state even when a mode-reset write fails.
export proc finish(session: Session) [process, io, error] -> Result[Unit, Error] {
  defer { gnu.write_text(cleanup_sequences()) }
  unix.set_tty_attrs(session.attrs, fd: 0)
  return
}

## Closes synchronized output on errors while a frame is being written.
export proc present(frame: Str) [io] {
  defer { gnu.write_text("\u{1b}[?2026l") }
  gnu.write_text("\u{1b}[?2026h")
  gnu.write_text(frame)
}
