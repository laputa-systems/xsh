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

# The failures the file verifier reports.
#
# A declared error variant reports `Family.Variant` unless its payload carries
# a string `kind` field, so `kind` is what keeps the baseline spellings
# `checksum-format` and `checksum-mismatch` visible to callers. The variants
# separate a checksum that is not a checksum from one that is a checksum of
# something else.
error HashVerifyError = Format(kind: Str, message: Str) | Mismatch(kind: Str, message: Str)

# A checksum argument that is not a checksum of the expected shape.
pure format_error(message: Str) -> HashVerifyError {
  return HashVerifyError.Format(kind: "checksum-format", message: message)
}

# A checksum that is well formed but does not match the file.
pure mismatch_error(message: Str) -> HashVerifyError {
  return HashVerifyError.Mismatch(kind: "checksum-mismatch", message: message)
}

# Compare one digest against an expected checksum.
#
# The checksum is validated after the file has already been read, because the
# baseline hashes first and compares second: a caller that passes both an
# unreadable path and a malformed checksum sees the read failure. The length
# rejection is the baseline's `digest_len * 2` against the expected text's
# *byte* length, and the hexadecimal rejection follows it, so a checksum of the
# right byte length with a non-hexadecimal byte reports the hexadecimal
# rejection rather than a length one. The comparison itself is ASCII
# case-insensitive, and the mismatch message reports the expected spelling the
# caller passed, not a normalized one.
pure verify_digest(digest: Digest, algorithm: Str, expected: Str) -> Result[Unit] {
  let actual = digest.hex()
  if expected.byte_len() != actual.byte_len() {
    return Err(
      format_error(
        f"${algorithm} checksum must be ${actual.byte_len()} hex characters",
      ),
    )
  }
  if !is_hex_text(expected) {
    return Err(format_error("checksum must be hexadecimal"))
  }
  if actual.lower() == expected.lower() {
    return Ok()
  }
  return Err(
    mismatch_error(f"${algorithm} digest mismatch: expected ${expected}, got ${actual}"),
  )
}

## Verify a file against an expected checksum.
##
## The algorithm is the name the caller used for the checksum argument, so
## `hash.verify_file(path, sha256: "...")` hashes with SHA-256 and
## `hash.verify_file(path, md5: "...")` with MD5. The file is hashed first and
## the checksum is validated second, so a path that cannot be read reports its
## read failure before a malformed checksum is considered.
##
## A checksum whose byte length is not twice the digest length is rejected with
## `checksum-format` and a message naming the algorithm and the required
## length; a checksum of the right length that is not hexadecimal is rejected
## with `checksum-format` and `checksum must be hexadecimal`; a well-formed
## checksum that differs from the digest is rejected with `checksum-mismatch`
## and a message carrying both spellings. Comparison is ASCII
## case-insensitive, so an uppercase checksum verifies the same file.
##
## This entry is reached only through the specialized `hash.verify_file` call
## form: the algorithm arrives as a third argument rather than as a value, so
## the function is not a positional mirror of the public signature.
export pure verify_file(path: Path, checksum: Str, algorithm: Str) -> Result[Unit] {
  match algorithm {
    "md5" => return verify_digest(hash.md5(path)?, algorithm, checksum)
    "sha1" => return verify_digest(hash.sha1(path)?, algorithm, checksum)
    "sha256" => return verify_digest(hash.sha256(path)?, algorithm, checksum)
    "sha512" => return verify_digest(hash.sha512(path)?, algorithm, checksum)
    _ => return Err(
      format_error(f"unsupported checksum algorithm `${algorithm}`"),
    )
  }
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
