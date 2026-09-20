##! Embedded implementation of the public `tui` module.
# Internal implementation module. The public contract (names, parameters,
# purity, effects, and docs) stays in the standard API registry; only the
# escape-sequence table and the visible-width padding algorithm live here.
#
# `tui.read_secret` has no implementation here and stays native: it owns raw
# stdin reads, terminal mode changes, and descriptor lifetime.

# Width of a span that contains no `ESC` byte: every Unicode scalar value
# except CR and LF contributes one. `Str` has no scalar indexing, so the CR/LF
# exclusion is a single deleting `translate` and the remainder is counted by the
# kernel `count_chars`.
pure plain_width(span: Str) -> Int {
  return span.translate("\r\n", "").count_chars()
}

# A run of `count` spaces, or the empty string when `count` is not positive.
#
# There is no `Str.repeat`, and appending one space per iteration would copy the
# whole accumulator every time (quadratic in `count`), so the run is grown by
# doubling and then cut back to exactly `count` bytes.
pure space_run(count: Int) -> Str {
  var block = " "
  while block.byte_len() < count {
    block = block + block
  }
  return block.byte_slice(0, count)
}

# Visible width of `text`, counted in Unicode scalar values - not display cells
# and not graphemes. `ESC` followed immediately by `[` opens a CSI sequence that
# contributes nothing through its first byte in `@`..`~`, or through the end of
# the text when no such byte follows. CR and LF contribute nothing; every other
# scalar value contributes one.
#
# Every byte this scan branches on is ASCII (`ESC`, `[`, and the `@`..`~`
# terminator range), and UTF-8 continuation bytes are all `0x80`..`0xBF`, so
# byte positions and character boundaries coincide and the scan can work in
# bytes. Plain spans between escape sequences are measured by the kernel
# `count_chars`, so the loop runs once per escape sequence rather than once per
# byte or scalar value, and each span is sliced out of the original text instead
# of being rebuilt.
pure visible_width(text: Str) -> Int {
  let length = text.byte_len()
  var total = 0
  var pos = 0
  while pos < length {
    let escape = text.find("\u{1b}", pos)
    if escape < 0 {
      total = total + plain_width(text.byte_slice(pos, length - pos))
      break
    }
    if escape > pos {
      total = total + plain_width(text.byte_slice(pos, escape - pos))
    }
    if escape + 1 < length and text.byte_at(escape + 1, 0) == 91 {
      var index = escape + 2
      var end = length
      while index < length {
        let byte = text.byte_at(index, 0)
        if byte >= 64 and byte <= 126 {
          end = index + 1
          break
        }
        index = index + 1
      }
      pos = end
    } else {
      # A lone `ESC` is an ordinary scalar value and keeps its width.
      total = total + 1
      pos = escape + 1
    }
  }
  return total
}

# Pad `text` to `width` visible columns on the `left` when requested, or on the
# right otherwise. A negative `width` clamps to zero, and text that already
# reaches `width` is returned unchanged and byte-identical.
pure pad(text: Str, width: Int, left: Bool) -> Str {
  var target = width
  if target < 0 {
    target = 0
  }
  let visible = visible_width(text)
  if visible >= target {
    return text
  }
  let filler = space_run(target - visible)
  if left {
    return filler + text
  }
  return text + filler
}

## Pad `text` to `width` visible columns with leading spaces.
##
## Width counts Unicode scalar values, with ANSI escape sequences and CR/LF
## contributing nothing, so styled text pads to its visible extent rather than
## its byte length. A negative `width` clamps to zero, and text already at least
## `width` wide is returned unchanged.
export pure left_pad(text: Str, width: Int) -> Str {
  return pad(text, width, true)
}

## Pad `text` to `width` visible columns with trailing spaces.
##
## Width counts Unicode scalar values, with ANSI escape sequences and CR/LF
## contributing nothing, so styled text pads to its visible extent rather than
## its byte length. A negative `width` clamps to zero, and text already at least
## `width` wide is returned unchanged.
export pure right_pad(text: Str, width: Int) -> Str {
  return pad(text, width, false)
}

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
