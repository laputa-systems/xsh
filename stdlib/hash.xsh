##! Embedded implementation of the public `hash` module.
# Internal implementation module. The public contract (names, parameters,
# purity, effects, and docs) stays in the standard API registry; only the
# checksum-line interpretation lives here.
#
# The accepted dialect is the GNU coreutils checksum line, `<hex>  <path>` or
# `<hex> *<path>`. Digest creation, the MD5/SHA/CRC implementations, digest
# representation, encoding, and file hashing all stay native.

# The one error kind this parser reports.
#
# A declared error variant reports `Family.Variant` unless its payload carries
# a string `kind` field, so `kind` is what keeps the baseline spelling
# `checksum-line` visible to callers. Every rejection shares that kind; the
# message is what separates them.
error HashCheckLineError = Line(kind: Str, message: Str)

# A rejected checksum line carrying the parser's own kind.
pure check_line_error(message: Str) -> HashCheckLineError {
  return HashCheckLineError.Line(kind: "checksum-line", message: message)
}

# Whether every byte of a digest field is a hexadecimal digit.
#
# The test uses the retained translation kernel rather than a byte loop: a
# helper call per byte would make a long digest cost an interpreted step per
# character. The baseline accepts `0-9a-fA-F`, and `translate` deletes exactly
# those, so an empty remainder is the same predicate. An empty field passes,
# because the baseline rejects emptiness as an incomplete line before it
# validates hexadecimal digits.
pure is_hex_text(text: Str) -> Bool {
  return text.lower().translate("0123456789abcdef", "").byte_len() == 0
}

# Drop every trailing carriage return, not just one.
#
# The baseline trims the line before it looks for a separator, so a CRLF
# checksum file parses exactly like an LF one. The count is found first so the
# text is copied at most once.
pure strip_trailing_cr(line: Str) -> Str {
  var end = line.byte_len()
  while end > 0 and line.byte_at(end - 1, 0) == 13 {
    end = end - 1
  }
  if end == line.byte_len() {
    return line
  }
  return line.byte_slice(0, end)
}

## Parse one checksum-file verification line.
##
## Accepts `<hex>  <path>` and `<hex> *<path>`. A double-space separator wins
## over a space-star separator even when the space-star pair appears earlier,
## because the baseline searches for the two separators in that order over the
## whole line. Every trailing carriage return is ignored.
##
## The digest field is checked for hexadecimal digits before it is lowercased,
## so a rejected line keeps reporting its original spelling, and the accepted
## digest is always lowercase. The path field is every byte after the separator
## minus one optional leading `*`. `binary` reports whether the separator
## itself carried the marker, so a `*` that begins a double-space path is
## stripped without setting it.
##
## An empty digest field or an empty path field is an incomplete line, which
## takes priority over the hexadecimal check on a non-empty digest field.
export pure parse_check_line(line: Str) -> Result[Record] {
  let text = strip_trailing_cr(line)
  var separator = text.find("  ")
  if separator < 0 {
    separator = text.find(" *")
  }
  if separator < 0 {
    return Err(check_line_error("expected `<hex>  <path>` or `<hex> *<path>`"))
  }
  let hex = text.byte_slice(0, separator)
  # The marker is the separator's second byte; 42 is the byte value of `*`.
  let marker = text.byte_at(separator + 1, 32)
  var path = text.byte_slice(separator + 2)
  if path.starts_with("*") {
    path = path.byte_slice(1)
  }
  if hex.byte_len() == 0 or path.byte_len() == 0 {
    return Err(check_line_error("checksum line is incomplete"))
  }
  if !is_hex_text(hex) {
    return Err(check_line_error("checksum is not hexadecimal"))
  }
  return Ok({hex: hex.lower(), path: path, binary: marker == 42})
}
