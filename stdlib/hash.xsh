##! Embedded file verification policy for the public `hash` module.
# Digest creation, file hashing, and checksum-line parsing live in
# `src/modules/hash.rs`. The named-algorithm file verifier stays here after
# its separate B0 workloads passed on both hosts.

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
