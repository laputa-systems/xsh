##! Owns the raw terminal session shared by XSH terminal applications.
##!
##! Entry saves the input attributes, applies raw mode, and enters the
##! alternate screen. The restore contract has two callers: `leave` for normal
##! scope cleanup and `restore` for a signal hook, where a deferred call is not
##! guaranteed. Unconsumed input can be handed to a parent through an inherited
##! descriptor named by `XSH_TUI_INPUT_FD`.
use lib.gnu

const ENTER_SCREEN = "\u{1b}[?1049h\u{1b}[?25l\u{1b}[?2004h\u{1b}[2J\u{1b}[H"

## Preserves the input attributes replaced by raw mode.
export type Session = {attrs: UnixTtyAttrs}

## Names the terminal's row and column dimensions.
export type Size = {rows: Int, cols: Int}

## Returns every sequence needed to leave the screen and input modes.
export pure cleanup_sequences() -> Str {
  "\u{1b}[0m\u{1b}[?2026l\u{1b}[?2004l\u{1b}[?25h\u{1b}[?1049l"
}

## Reads the current terminal window size.
export proc size() [process, error] -> Result[Size, Error] {
  let measured = unix.window_size(1)?
  Ok({rows: measured.rows, cols: measured.cols})
}

## Saves terminal attributes, enters raw mode, and opens the alternate screen.
## A failed screen write restores the saved attributes before the applet exits.
export proc enter() [env, process, io, error] -> Result[Session, Error] {
  let attrs = unix.tty_attrs(0)?
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
  gnu.write_text(ENTER_SCREEN)
  transferred = true
  Ok({attrs: attrs})
}

## Restores the terminal after normal scope exit, reporting the first failure.
export proc leave(session: Session) [process, io, error] -> Result[Unit, Error] {
  defer { gnu.write_text(cleanup_sequences()) }
  unix.set_tty_attrs(session.attrs, fd: 0)
}

## Restores the terminal from a signal hook without relying on a deferred call.
## Every step is attempted even when an earlier one fails, and no failure exits.
export proc restore(session: Session) [process, io] -> Unit {
  let _ = unix.set_tty_attrs(session.attrs, fd: 0)
  let _ = io.write_stdout(cleanup_sequences())
  let _ = io.flush_stdout()
}

## Closes synchronized output on errors while a frame is being written.
export proc present(frame: Str) [io] {
  defer { gnu.write_text("\u{1b}[?2026l") }
  gnu.write_text("\u{1b}[?2026h")
  gnu.write_text(frame)
}

## Writes bytes the decoder did not consume to the parent's descriptor.
## `XSH_TUI_INPUT_FD` names an inherited writable descriptor; without it the
## call is a no-op and returns false. Returns true after every byte is written.
export proc hand_off(data: Bytes) [env, process, net, error] -> Result[Bool, Error] {
  let configured = env.get_or("XSH_TUI_INPUT_FD", "") ?? ""
  return Ok(false) when configured == "" or data.is_empty()
  guard let fd = configured.parse_int() else {
    return Err(error.failure("XSH_TUI_INPUT_FD is not a descriptor number"))
  }
  if fd < 0 {
    return Err(error.failure("XSH_TUI_INPUT_FD is not a descriptor number"))
  }
  var written = 0
  while written < data.len() {
    let count = unix.write_fd(fd, data[written..])?
    if count <= 0 {
      return Err(error.failure("XSH_TUI_INPUT_FD accepted no bytes"))
    }
    written += count
  }
  Ok(true)
}
