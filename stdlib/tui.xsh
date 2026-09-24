##! Embedded implementation of the public `tui` module.
# Internal implementation module. The public contract (names, parameters,
# purity, effects, and docs) stays in the standard API registry; only the
# escape-sequence table lives here.
#
# `tui.left_pad`, `tui.right_pad`, and `tui.read_secret` stay native. Padding
# retains its width scan, while secret input owns terminal mode and descriptor
# lifetime.

## Return the SGR reset sequence, ending all active styling.
export pure reset() -> Str {
  return "\u{1b}[0m"
}

## Return the SGR bold-intensify sequence.
export pure bold() -> Str {
  return "\u{1b}[1m"
}

## Return the SGR faint-intensify sequence.
export pure dim() -> Str {
  return "\u{1b}[2m"
}

## Return the SGR foreground sequence for red.
export pure red() -> Str {
  return "\u{1b}[31m"
}

## Return the SGR foreground sequence for green.
export pure green() -> Str {
  return "\u{1b}[32m"
}

## Return the SGR foreground sequence for yellow.
export pure yellow() -> Str {
  return "\u{1b}[33m"
}

## Return the SGR foreground sequence for blue.
export pure blue() -> Str {
  return "\u{1b}[34m"
}

## Return the SGR foreground sequence for magenta.
export pure magenta() -> Str {
  return "\u{1b}[35m"
}

## Return the SGR foreground sequence for cyan.
export pure cyan() -> Str {
  return "\u{1b}[36m"
}

## Return the SGR foreground sequence for white.
export pure white() -> Str {
  return "\u{1b}[37m"
}

## Return the SGR bright-black foreground sequence, the conventional gray.
export pure gray() -> Str {
  return "\u{1b}[90m"
}

## Return the sequence that erases the whole display.
export pure clear() -> Str {
  return "\u{1b}[2J"
}

## Return the sequence that moves the cursor to the home position.
export pure home() -> Str {
  return "\u{1b}[H"
}

## Return the sequence that erases the whole current line.
export pure erase_line() -> Str {
  return "\u{1b}[2K"
}

## Return the sequence that hides the cursor.
export pure hide_cursor() -> Str {
  return "\u{1b}[?25l"
}

## Return the sequence that shows the cursor again.
export pure show_cursor() -> Str {
  return "\u{1b}[?25h"
}
