##! Embedded implementation of the public `env` module.
# Internal implementation module. The public contract (names, parameters,
# purity, effects, and docs) stays in the standard API registry; only the
# fallback and conversion policy behind `env.get_or`, `env.bool`, and `env.int`
# lives here.
#
# Raw acquisition stays native: `env.get`, `env.path`, `env.list`,
# `env.path_list`, `env.path_entries`, and every path-list mutation keep their
# runtime bodies. Each entry below reads through `env.get` and hands its
# failures back unchanged, so key validation (`env-name`) and value decoding
# (`invalid-utf8`) keep their baseline kinds, messages, and spans.
#
# An unset name is the only failure that yields the fallback. A present value
# is never replaced, however unusable it is: an empty value is `""` for
# `env.get_or`, `false` for `env.bool`, and unparsable text for `env.int`.

# The error kind the integer conversion adds.
#
# A declared error variant reports `Family.Variant` unless its payload carries
# a string `kind` field, so `kind` is what keeps the baseline spelling
# `env-int` visible to callers.
error EnvIntError = NotInteger(kind: Str, message: Str)

# The rejection `env.int` reports for a present value that is not an integer.
#
# A present value that is not valid UTF-8 is rejected by `env.get` instead,
# with the `invalid-utf8` kind.
pure int_error() -> EnvIntError {
  return EnvIntError.NotInteger(kind: "env-int", message: "environment value is not an integer")
}

# Whether an `env.get` failure means the name is unset.
#
# The baseline substitutes the fallback for an unset name and propagates every
# other failure, but XSH cannot branch on an error kind: `error .kind` was
# removed from the language, and the runtime kinds a native entry reports are
# not declared variants, so no pattern names `env-missing`. The unset failure
# is therefore recognized by its message, which is part of the public contract
# this module preserves: it is the only `env.get` failure that yields a
# fallback.
pure is_unset_failure(message: Str) -> Bool {
  return message == "environment value is unset"
}

## Look up `name`, returning `fallback` when it is unset.
##
## An unset name yields `fallback` unchanged, including an empty one. A name
## that is empty or contains NUL or `=` fails with `env-name`, and a value that
## is not valid UTF-8 fails with `invalid-utf8`; both are `env.get` failures and
## are propagated as they are. A present value is returned even when it is
## empty, so the fallback is not substituted for it, and the value is returned
## byte for byte with no trimming.
export proc get_or(name: Str, fallback: Str = "") [env] -> Result[Str] {
  match env.get(name) {
    Ok(text) => {
      return Ok(text)
    }
    Err(error) => {
      if is_unset_failure(error.message) {
        return Ok(fallback)
      }
      return Err(error)
    }
  }
}

# Whether a present value spells `true`.
#
# The baseline trims the value and lowercases it before matching the four
# accepted spellings, so surrounding white space and letter case are ignored
# and nothing else is: `0`, `false`, `no`, `off`, `y`, and every other spelling
# are `false` rather than a failure. `Str.trim` trims the same Unicode white
# space as the baseline's `str::trim`. `Str.lower` lowercases the full Unicode
# range where the baseline lowercases ASCII only, and the two differ only for a
# character whose lowercase is an ASCII letter, of which the only one is U+212A
# KELVIN SIGN; no accepted spelling contains a `k`, so the two agree on every
# value this test accepts or rejects.
pure is_truthy(text: Str) -> Bool {
  let lowered = text.trim().lower()
  if lowered == "1" or lowered == "true" or lowered == "yes" or lowered == "on" {
    return true
  }
  return false
}

## Interpret `name` as a boolean, returning `fallback` when it is unset.
##
## The value is trimmed and lowercased and then compared against `1`, `true`,
## `yes`, and `on`. Every other present value is `false`, including the empty
## string and every other spelling of `true` and `false`: an unrecognized
## spelling is not a failure. Only an unset name, a name that is empty or
## contains NUL or `=`, and a value that is not valid UTF-8 fail, each reported
## exactly as `env.get` reports it.
export proc bool(name: Str, fallback: Bool = false) [env] -> Result[Bool] {
  match env.get(name) {
    Ok(text) => {
      return Ok(is_truthy(text))
    }
    Err(error) => {
      if is_unset_failure(error.message) {
        return Ok(fallback)
      }
      return Err(error)
    }
  }
}

# Whether one byte is an ASCII decimal digit.
#
# The test is byte-wise, which matches the baseline: its digits come from
# `str::parse::<i64>`, which accepts ASCII digits only, so a Unicode digit that
# a character-wise parser might accept is rejected here too. No byte of a
# non-ASCII character is below 0x80, so none of them can pass this test.
pure is_decimal_byte(byte: Int) -> Bool {
  return byte >= 48 and byte <= 57
}

# Whether every byte from `start` to the end of `text` is a decimal digit.
#
# An empty run passes here; the caller rejects a sign with no digits after it
# separately.
pure is_decimal_run(text: Str, start: Int) -> Bool {
  var index = start
  while index < text.byte_len() {
    if !is_decimal_byte(text.byte_at(index, 0)) {
      return false
    }
    index = index + 1
  }
  return true
}

# Whether a run of decimal digits is no larger than `limit`.
#
# `limit` is the decimal spelling of the largest magnitude the sign allows:
# `9223372036854775807` for a positive value, and `9223372036854775808` for a
# negative one, whose magnitude is one larger than any positive `Int`. Leading
# zeros are insignificant, so they are dropped before the widths are compared;
# a shorter run is smaller, a longer run is larger, and runs of equal width are
# compared byte by byte from the most significant digit. The bound is checked
# as text so that an out-of-range value is rejected like any other unparsable
# text instead of trapping the checked `Int` arithmetic with `integer-overflow`.
pure magnitude_within_limit(text: Str, start: Int, limit: Str) -> Bool {
  var index = start
  let end = text.byte_len()
  while index < end - 1 and text.byte_at(index, 0) == 48 {
    index = index + 1
  }
  let width = end - index
  if width != limit.byte_len() {
    return width < limit.byte_len()
  }
  var offset = 0
  while offset < width {
    let digit = text.byte_at(index + offset, 0)
    let bound = limit.byte_at(offset, 0)
    if digit != bound {
      return digit < bound
    }
    offset = offset + 1
  }
  return true
}

# Parse the integer text `env.int` accepts.
#
# The baseline trims the text and hands it to `str::parse::<i64>`, so the
# accepted spelling is an optional leading `+` or `-`, then one or more ASCII
# digits, and nothing else. Underscores, inner white space, radix prefixes,
# decimal points, a lone sign, and a repeated sign are all rejected; leading
# zeros are not.
#
# The digits are accumulated in negative space because the negative bound is
# the one whose magnitude is not representable as a positive `Int`. With the
# text already range-checked, neither the running value nor the result leaves
# `Int` range, so the checked `Int` arithmetic cannot trap.
pure parse_env_int(text: Str) -> Result[Int] {
  let trimmed = text.trim()
  let end = trimmed.byte_len()
  var negative = false
  var start = 0
  if end > 0 {
    # 43 is `+` and 45 is `-`.
    let first = trimmed.byte_at(0, 0)
    if first == 45 {
      negative = true
      start = 1
    } else if first == 43 {
      start = 1
    }
  }
  if start == end or !is_decimal_run(trimmed, start) {
    return Err(int_error())
  }
  var limit = "9223372036854775807"
  if negative {
    limit = "9223372036854775808"
  }
  if !magnitude_within_limit(trimmed, start, limit) {
    return Err(int_error())
  }
  var value = 0
  var index = start
  while index < end {
    value = value * 10 - (trimmed.byte_at(index, 0) - 48)
    index = index + 1
  }
  if negative {
    return Ok(value)
  }
  return Ok(-value)
}

## Interpret `name` as a base-10 integer, returning `fallback` when it is
## unset.
##
## The accepted text is the baseline's: optional leading `+` or `-`, then one
## or more ASCII digits, and nothing else, with leading zeros allowed and the
## value required to fit in an `Int`. A present value that is not accepted
## fails with `env-int` and the message `environment value is not an integer`,
## and is not replaced by the fallback; an empty value is not an integer
## either. An unset name, a name that is empty or contains NUL or `=`, and a
## value that is not valid UTF-8 are reported exactly as `env.get` reports
## them.
export proc int(name: Str, fallback: Int = 0) [env] -> Result[Int] {
  match env.get(name) {
    Ok(text) => {
      return parse_env_int(text)
    }
    Err(error) => {
      if is_unset_failure(error.message) {
        return Ok(fallback)
      }
      return Err(error)
    }
  }
}
