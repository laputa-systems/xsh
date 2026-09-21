##! Embedded implementation of the public `text` module.
# Internal implementation module. The public contract (names, parameters,
# purity, effects, and docs) stays in the standard API registry; only the
# wrapping and field-selection policy lives here.
#
# Primitive splitting, searching, replacement, Unicode case conversion,
# translation, byte indexing/views, and numeric parsing all stay native.

# The one rejection `wrap` reports.
#
# A declared error variant reports `Family.Variant` unless its payload carries
# a string `kind` field, so `kind` is what keeps the baseline spelling
# `text-wrap` visible to callers.
error TextWrapError = Width(kind: Str, message: Str)

# A rejected width, carrying the wrapper's own kind.
pure wrap_error(message: Str) -> TextWrapError {
  return TextWrapError.Width(kind: "text-wrap", message: message)
}

## Select the fields of `text`.
##
## An empty `delimiter` — the default — selects runs of Unicode whitespace, so
## leading, trailing, and repeated whitespace contribute no fields and the
## result is the words of `text`. Any other delimiter is taken literally and
## empty fields between two adjacent delimiters are dropped, so a delimiter
## that never occurs yields the whole `text` as a single field.
export pure fields(text: Str, delimiter: Str = "") -> List[Str] {
  if delimiter.byte_len() == 0 {
    return text.words()
  }
  return [field for field in text.split(delimiter) if field.byte_len() > 0]
}

# The largest character boundary at or before `end` in `text`.
#
# Only `byte_slice` needs this: it accepts character boundaries alone, and the
# window that has to reach `width + 1` scalar values can end inside a
# multi-byte scalar. No scalar is longer than four bytes, so at most three
# continuation bytes are stepped back over, and nothing is decoded - only the
# one byte that would end the window is classified.
pure boundary_at(text: Str, end: Int) -> Int {
  var cut = end
  while cut > 0 and text.byte_at(cut, -1) >= 128 and text.byte_at(cut, -1) < 192 {
    cut = cut - 1
  }
  return cut
}

# The byte offset of scalar `index` in `text`, counted from the start, and the
# length of `text` when it holds fewer scalars.
#
# `split` on the empty separator with `maxsplit` yields one element per scalar
# up to the limit, then the rest of `text` as the element after them, so the
# remainder's byte length is what to subtract from the whole. `Str` has no
# scalar indexing; this is what stands in for it.
pure scalar_offset(text: Str, index: Int) -> Int {
  return text.byte_len() - text.split("", maxsplit: index).get(index, "").byte_len()
}

# The next wrapped piece of the normalized line `norm`, starting at byte
# `start`.
#
# A piece ends at the last separator at or before column `width`, counted in
# Unicode scalar values. That is exactly where the baseline's greedy fill
# stops: it starts a new line as soon as the word that follows would not fit,
# and no word is wider than `width` once overlong words are cut.
#
# The window is four bytes per column plus one column, which reaches `width + 1`
# scalar values as long as the line still holds them. `scalar_offset` then names
# the byte offset of that column, and `split` on a single space names the last
# separator at or before it, so the piece is the text up to that separator. A
# word wider than `width` has no separator inside those columns; it is cut at
# exactly `width` scalars instead.
#
# The returned piece ends its line exactly when it ends at the end of `norm`:
# every other piece is followed by the single-space separator it was cut at.
pure wrap_piece(norm: Str, start: Int, width: Int) -> Str {
  let remaining = norm.byte_len() - start
  if width >= remaining {
    return norm.byte_slice(start)
  }
  let head = norm.byte_slice(start, boundary_at(norm, start + 4 * (width + 1)) - start)
  if head.count_chars() <= width {
    return head
  }
  let window = head.byte_slice(0, scalar_offset(head, width + 1))
  let parts = window.split(" ")
  if parts.len() == 1 {
    return head.byte_slice(0, scalar_offset(head, width))
  }
  let trailing = parts.get(parts.len() - 1, "").byte_len()
  return window.byte_slice(0, window.byte_len() - trailing - 1)
}

## Wrap `text` into the lines of at most `width` columns it occupies.
##
## Columns count Unicode scalar values. Every line of `text` wraps on its own:
## runs of Unicode whitespace separate words, each line takes greedily the words
## that fit, and a word wider than `width` is cut into pieces of exactly `width`
## scalars. An output line holds single spaces between the words it was built
## from, an empty input line contributes one empty output line, and text ending
## with a newline contributes one empty output line after its last line. Empty
## `text` wraps to no lines at all.
##
## A `width` of zero or less is rejected with kind `text-wrap` and message
## `width must be positive`.
##
## The declared return type is `Any` rather than the registry's `List[Str]`
## because that rejection is a runtime error rather than a returned value: the
## lowered runtime raises `Err` out of a function whose declared kind is not a
## `Result`, which is what keeps the rejection's kind, message, and call-site
## span identical to the baseline's native route.
##
## Wrapped lines are collected per page and each finished page becomes one
## string in the page list. A piece is one output line, so a list holding every
## piece would be as long as the text has lines; a page keeps the intermediate
## list bounded and the pieces it holds are joined once, into a single entry of
## the (short) page list.
export pure wrap(text: Str, width: Int) -> Any {
  if width <= 0 {
    return Err(wrap_error("width must be positive"))
  }
  if text.byte_len() == 0 {
    return []
  }
  var pages: List[Str] = []
  var page: List[Str] = []
  for line in text.lines() {
    let norm = line.words().join(" ")
    var start = 0
    var last = false
    while !last {
      let piece = wrap_piece(norm, start, width)
      page = page.push(piece)
      if page.len() >= 64 {
        pages = pages.push(page.join("\n"))
        page = []
      }
      # A piece ends at a separator or at exactly `width` scalars of one word.
      # Only a separator has to be stepped over; the byte after such a piece is
      # always that separator.
      let end = start + piece.byte_len()
      last = end >= norm.byte_len()
      if norm.byte_at(end, 0) == 32 {
        start = end + 1
      } else {
        start = end
      }
    }
  }
  if page.len() > 0 {
    pages = pages.push(page.join("\n"))
  }
  var joined = pages.join("\n")
  if text.ends_with("\n") {
    joined = joined + "\n"
  }
  return joined.split("\n")
}
