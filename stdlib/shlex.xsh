##! Embedded implementation of the public `shlex` module.
# Internal implementation module. The public contract (names, parameters,
# purity, effects, and docs) stays in the standard API registry; only the
# quoting algorithm lives here.

# Whether a single byte may appear unquoted in a shell word.
#
# The safe set is alphanumeric plus `_@%+=:,./-`, matching the baseline
# byte-classification exactly. Every byte outside it, including every
# non-ASCII UTF-8 byte, forces quoting.
pure safe_byte(byte: Int) -> Bool {
  if byte >= 97 and byte <= 122 {
    return true
  }
  if byte >= 65 and byte <= 90 {
    return true
  }
  if byte >= 48 and byte <= 57 {
    return true
  }
  return byte == 95
    or byte == 64
    or byte == 37
    or byte == 43
    or byte == 61
    or byte == 58
    or byte == 44
    or byte == 46
    or byte == 47
    or byte == 45
}

## Quote one string for use as a single shell word.
##
## Empty input becomes `''`. A word whose bytes are all safe is returned
## unchanged. Anything else is wrapped in single quotes with each embedded
## apostrophe escaped as `'\''`; `Str.replace` is byte-preserving, so Unicode
## and embedded newlines survive unchanged.
export pure quote(value: Str) -> Str {
  if value.byte_len() == 0 {
    return "''"
  }
  var needs_quoting = false
  var index = 0
  while index < value.byte_len() {
    if !safe_byte(value.byte_at(index)) {
      needs_quoting = true
      break
    }
    index = index + 1
  }
  if !needs_quoting {
    return value
  }
  return "'" + value.replace("'", "'\\''") + "'"
}

## Quote every argument independently and join them with one space.
##
## Each argument is quoted on its own and the results are joined with a single
## space, so the result re-splits into the original words.
export pure join(argv: List[Str]) -> Str {
  return [quote(word) for word in argv].join(" ")
}
