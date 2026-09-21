proc test_hash_digests_checksums_and_digest_methods(ctx: TestContext) [fs, error] {
  let data_path = test.temp_path(ctx, name: "hash-data.txt")
  fs.write(data_path, "abc")?
  let digest = hash.sha256(b"abc")
  test.eq(digest.base64(), "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=")?
  test.eq(hash.sha256(data_path)?.hex(), digest.hex())?
  test.eq(hash.md5(b"abc").hex(), "900150983cd24fb0d6963f7d28e17f72")?
  test.eq(hash.sha1(b"abc").hex(), "a9993e364706816aba3e25717850c26c9cd0d89d")?

  test.eq(
    hash.sha512(b"abc").hex(),
    "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
  )?

  test.eq(hash.crc32(b"123456789"), 3421780262)?
  test.eq(hash.crc32c(b"123456789"), 3808858755)?
  let check = hash.parse_check_line(f"${digest.hex()}  ${data_path.name()}")?
  test.eq(check.hex, digest.hex())?
  test.eq(check.path, data_path.name())?
  hash.verify_file(data_path, sha256: digest.hex())?
  test.error_kind(hash.verify_file(data_path, sha256: "00"), "checksum-format")?

  test.error_kind(
    hash.verify_file(data_path, sha256: "0000000000000000000000000000000000000000000000000000000000000000"),
    "checksum-mismatch",
  )?
}

# The message of a rejected file verification, or the empty string when the
# file verified. `test.error_kind` compares kinds only, so message parity is
# asserted through this.
pure verify_message(result: Result[Unit]) -> Str {
  match result {
    Ok(_) => return ""
    Err(error) => return error.message
  }
}

# Every case the removed native `verify_hex`/`validate_expected_hex` pair
# covered, plus the algorithm selection the specialized call form carries.
proc test_hash_verify_file_policy(ctx: TestContext) [fs, error] {
  let data_path = test.temp_path(ctx, name: "hash-verify.txt")
  fs.write(data_path, "abc")?
  let digest = hash.sha256(data_path)?

  # The digest verifies, and comparison is ASCII case-insensitive.
  test.eq(verify_message(hash.verify_file(data_path, sha256: digest.hex())), "")?
  test.eq(verify_message(hash.verify_file(data_path, sha256: digest.hex().upper())), "")?

  # A checksum of the wrong byte length reports the required length for the
  # algorithm the caller named.
  test.eq(
    verify_message(hash.verify_file(data_path, sha256: "00")),
    "sha256 checksum must be 64 hex characters",
  )?
  test.eq(
    verify_message(hash.verify_file(data_path, md5: "00")),
    "md5 checksum must be 32 hex characters",
  )?

  # A checksum of the right byte length that is not hexadecimal reports the
  # hexadecimal rejection, not a length one.
  test.eq(
    verify_message(
      hash.verify_file(data_path, sha256: "zz00000000000000000000000000000000000000000000000000000000000000"),
    ),
    "checksum must be hexadecimal",
  )?

  # A well-formed checksum of something else reports both spellings, with the
  # expected one as the caller wrote it.
  let zeros64 = "0000000000000000000000000000000000000000000000000000000000000000"
  test.eq(
    verify_message(hash.verify_file(data_path, sha256: zeros64)),
    f"sha256 digest mismatch: expected ${zeros64}, got ${digest.hex()}",
  )?

  # Each named algorithm selects its own digest, and the failure names it.
  test.eq(verify_message(hash.verify_file(data_path, md5: hash.md5(data_path)?.hex())), "")?
  test.eq(verify_message(hash.verify_file(data_path, sha1: hash.sha1(data_path)?.hex())), "")?
  test.eq(verify_message(hash.verify_file(data_path, sha512: hash.sha512(data_path)?.hex())), "")?
  let zeros32 = "00000000000000000000000000000000"
  test.eq(
    verify_message(hash.verify_file(data_path, md5: zeros32)),
    f"md5 digest mismatch: expected ${zeros32}, got ${hash.md5(data_path)?.hex()}",
  )?

  # The file is hashed before the checksum is validated, so a path that cannot
  # be read reports its read failure even when the checksum is malformed.
  let missing = test.temp_path(ctx, name: "hash-verify-missing.txt")
  test.eq(
    verify_message(hash.verify_file(missing, sha256: "00")),
    "No such file or directory (os error 2)",
  )?
  test.error_kind(hash.verify_file(missing, sha256: "00"), "hash-read")?
}

# The message of a rejected checksum line, or the empty string when the line
# parsed. `test.error_kind` compares kinds only, so message parity is asserted
# through this.
pure check_line_message(result: Result[Record]) -> Str {
  match result {
    Ok(_) => return ""
    Err(error) => return error.message
  }
}

proc test_parse_check_line_reads_both_gnu_separators() [error] {
  let digest = "900150983cd24fb0d6963f7d28e17f72"

  let two_space = hash.parse_check_line(f"${digest}  docs/readme.txt")?
  test.eq(two_space.hex, digest)?
  test.eq(two_space.path, "docs/readme.txt")?
  test.eq(two_space.binary, false)?

  let space_star = hash.parse_check_line(f"${digest} *readme.bin")?
  test.eq(space_star.hex, digest)?
  test.eq(space_star.path, "readme.bin")?
  test.eq(space_star.binary, true)?

  # A path may contain spaces: everything after the separator is kept, and
  # only trailing carriage returns are trimmed.
  let spaced = hash.parse_check_line(f"${digest}  my file.txt ")?
  test.eq(spaced.path, "my file.txt ")?
}

proc test_parse_check_line_handles_path_star_and_marker() [error] {
  let digest = "900150983cd24fb0d6963f7d28e17f72"

  # The marker comes from the separator, so a star that begins a double-space
  # path is stripped as part of the path and does not set the marker.
  let starred = hash.parse_check_line(f"${digest}  *readme.bin")?
  test.eq(starred.path, "readme.bin")?
  test.eq(starred.binary, false)?

  # Exactly one leading star is dropped; a second one stays in the path.
  let twice = hash.parse_check_line(f"${digest}  **readme.bin")?
  test.eq(twice.path, "*readme.bin")?
  test.eq(twice.binary, false)?
}

proc test_parse_check_line_normalizes_case_and_carriage_returns() [error] {
  let upper = hash.parse_check_line("900150983CD24FB0D6963F7D28E17F72  readme.txt")?
  test.eq(upper.hex, "900150983cd24fb0d6963f7d28e17f72")?
  test.eq(upper.path, "readme.txt")?

  let crlf = hash.parse_check_line("900150983CD24FB0D6963F7D28E17F72 *readme.bin\r")?
  test.eq(crlf.hex, "900150983cd24fb0d6963f7d28e17f72")?
  test.eq(crlf.path, "readme.bin")?
  test.eq(crlf.binary, true)?

  # Every trailing carriage return is trimmed, not just the last one.
  let doubled = hash.parse_check_line("900150983CD24FB0D6963F7D28E17F72  readme.txt\r\r")?
  test.eq(doubled.hex, "900150983cd24fb0d6963f7d28e17f72")?
  test.eq(doubled.path, "readme.txt")?
}

proc test_parse_check_line_prefers_the_double_space_separator() [error] {
  # Digest length is a `hash.verify_file` policy: the line parser accepts any
  # non-empty hexadecimal field.
  let short = hash.parse_check_line("abc  readme.txt")?
  test.eq(short.hex, "abc")?
  test.eq(short.path, "readme.txt")?

  # The whole line is searched for the double-space separator first, so a
  # space-star pair that appears earlier does not win. The digest field then
  # spans the star and the line is rejected as not hexadecimal.
  let both = hash.parse_check_line("900150983cd24fb0d6963f7d28e17f72 *readme.bin  suffix")
  test.error_kind(both, "checksum-line")?
  test.eq(check_line_message(both), "checksum is not hexadecimal")?
}

proc test_parse_check_line_rejects_malformed_lines() [error] {
  let separator_message = "expected `<hex>  <path>` or `<hex> *<path>`"
  test.error_kind(hash.parse_check_line(""), "checksum-line")?
  test.eq(check_line_message(hash.parse_check_line("")), separator_message)?
  test.error_kind(
    hash.parse_check_line("900150983cd24fb0d6963f7d28e17f72"),
    "checksum-line",
  )?
  test.eq(
    check_line_message(hash.parse_check_line("900150983cd24fb0d6963f7d28e17f72")),
    separator_message,
  )?

  let incomplete_message = "checksum line is incomplete"
  test.error_kind(hash.parse_check_line("  readme.txt"), "checksum-line")?
  test.eq(check_line_message(hash.parse_check_line("  readme.txt")), incomplete_message)?
  test.error_kind(hash.parse_check_line("900150983cd24fb0d6963f7d28e17f72  "), "checksum-line")?
  test.eq(
    check_line_message(hash.parse_check_line("900150983cd24fb0d6963f7d28e17f72  ")),
    incomplete_message,
  )?
  test.error_kind(hash.parse_check_line("900150983cd24fb0d6963f7d28e17f72 *"), "checksum-line")?
  test.eq(check_line_message(hash.parse_check_line("900150983cd24fb0d6963f7d28e17f72 *")), incomplete_message)?

  # An empty field is incomplete even when the other field is not
  # hexadecimal, so emptiness outranks the hexadecimal check.
  test.eq(check_line_message(hash.parse_check_line("  zz")), incomplete_message)?
  test.eq(check_line_message(hash.parse_check_line("zz  ")), incomplete_message)?

  let hex_message = "checksum is not hexadecimal"
  test.error_kind(hash.parse_check_line("zz  readme.txt"), "checksum-line")?
  test.eq(check_line_message(hash.parse_check_line("zz  readme.txt")), hex_message)?
  test.error_kind(
    hash.parse_check_line("900150983cd24fb0d6963f7d28e17e7g  readme.txt"),
    "checksum-line",
  )?
  test.eq(
    check_line_message(hash.parse_check_line("900150983cd24fb0d6963f7d28e17e7g  readme.txt")),
    hex_message,
  )?
}
