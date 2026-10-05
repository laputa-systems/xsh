test test_hash_digests_checksums_and_digest_methods { |ctx|
  let data_path = test.temp_path(ctx, name: "hash-data.txt")
  fs.write(data_path, "abc")
  let digest = hash.sha256(b"abc")
  assert digest.base64() == "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0="
  assert hash.sha256(data_path)?.hex() == digest.hex()
  assert hash.md5(b"abc").hex() == "900150983cd24fb0d6963f7d28e17f72"
  assert hash.sha1(b"abc").hex() == "a9993e364706816aba3e25717850c26c9cd0d89d"

  assert hash.sha512(b"abc").hex() == "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"

  assert hash.crc32(b"123456789") == 3421780262
  assert hash.crc32c(b"123456789") == 3808858755
  let check = hash.parse_check_line(f"{digest.hex()}  {data_path.name()}")?
  assert check.hex == digest.hex()
  assert check.path == data_path.name()
  hash.verify_file(data_path, sha256: digest.hex())
  test.error_kind(hash.verify_file(data_path, sha256: "00"), "checksum-format")

  test.error_kind(
    hash.verify_file(data_path, sha256: "0000000000000000000000000000000000000000000000000000000000000000"),
    "checksum-mismatch",
  )
}

# Named-argument order is not significant: the path binds by position or
# `path:`, and the algorithm-named checksum binds wherever it is written.
test test_hash_verify_file_argument_order { |ctx|
  let data_path = test.temp_path(ctx, name: "hash-order.txt")
  fs.write(data_path, "abc")
  let sha = hash.sha256(data_path)?.hex()
  let md = hash.md5(data_path)?.hex()
  hash.verify_file(data_path, sha256: sha)
  hash.verify_file(path: data_path, sha256: sha)
  hash.verify_file(sha256: sha, path: data_path)
  hash.verify_file(md5: md, path: data_path)
  hash.verify_file(sha256: sha, data_path)
  test.error_kind(hash.verify_file(sha256: "00", path: data_path), "checksum-format")
}

# The message of a rejected file verification, or the empty string when the
# file verified. `test.error_kind` compares kinds only, so message parity is
# asserted through this.
pure verify_message(result: Result[Unit]) -> Str {
  match result {
    Ok(_) => ""
    Err(error) => error.message
  }
}

# Every case the removed native `verify_hex`/`validate_expected_hex` pair
# covered, plus the algorithm selection the specialized call form carries.
test test_hash_verify_file_policy { |ctx|
  let data_path = test.temp_path(ctx, name: "hash-verify.txt")
  fs.write(data_path, "abc")
  let digest = hash.sha256(data_path)?

  # The digest verifies, and comparison is ASCII case-insensitive.
  assert verify_message(hash.verify_file(data_path, sha256: digest.hex())) == ""
  assert verify_message(hash.verify_file(data_path, sha256: digest.hex().upper())) == ""

  # A checksum of the wrong byte length reports the required length for the
  # algorithm the caller named.
  assert verify_message(hash.verify_file(data_path, sha256: "00")) == "sha256 checksum must be 64 hex characters"
  assert verify_message(hash.verify_file(data_path, md5: "00")) == "md5 checksum must be 32 hex characters"

  # A checksum of the right byte length that is not hexadecimal reports the
  # hexadecimal rejection, not a length one.
  assert verify_message(
    hash.verify_file(data_path, sha256: "zz00000000000000000000000000000000000000000000000000000000000000"),
  ) == "checksum must be hexadecimal"

  # A well-formed checksum of something else reports both spellings, with the
  # expected one as the caller wrote it.
  let zeros64 = "0000000000000000000000000000000000000000000000000000000000000000"
  assert verify_message(hash.verify_file(data_path, sha256: zeros64)) == f"sha256 digest mismatch: expected {zeros64}, got {digest.hex()}"

  # Each named algorithm selects its own digest, and the failure names it.
  assert verify_message(hash.verify_file(data_path, md5: hash.md5(data_path)?.hex())) == ""
  assert verify_message(hash.verify_file(data_path, sha1: hash.sha1(data_path)?.hex())) == ""
  assert verify_message(hash.verify_file(data_path, sha512: hash.sha512(data_path)?.hex())) == ""
  let zeros32 = "00000000000000000000000000000000"
  assert verify_message(hash.verify_file(data_path, md5: zeros32)) == f"md5 digest mismatch: expected {zeros32}, got {hash.md5(data_path)?.hex()}"

  # The file is hashed before the checksum is validated, so a path that cannot
  # be read reports its read failure even when the checksum is malformed.
  let missing = test.temp_path(ctx, name: "hash-verify-missing.txt")
  assert verify_message(hash.verify_file(missing, sha256: "00")) == "No such file or directory (os error 2)"
  test.error_kind(hash.verify_file(missing, sha256: "00"), "hash-read")

  # The smallest input: an empty file has the algorithm's canonical digest and
  # verifies like any other.
  let empty = test.temp_path(ctx, name: "hash-verify-empty.txt")
  fs.write(empty, "")
  assert hash.sha256(empty)?.hex() == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
  assert verify_message(hash.verify_file(empty, sha256: hash.sha256(empty)?.hex())) == ""

  # One byte, and a file spanning several digest blocks: the same call reads
  # the whole input, and the digest it compares is the one the reader produced.
  let one_byte = test.temp_path(ctx, name: "hash-verify-byte.bin")
  fs.write(one_byte, "x")
  assert verify_message(hash.verify_file(one_byte, sha256: hash.sha256(one_byte)?.hex())) == ""

  let large = test.temp_path(ctx, name: "hash-verify-large.bin")
  var filler = ""
  var round = 0
  while round < 2048 {
    filler = filler + "0123456789abcdef"
    round = round + 1
  }

  filler = filler + filler + filler + filler + "end"

  fs.write(large, filler)
  let large_bytes = bytes.from_text(filler)
  assert hash.md5(large)?.hex() == hash.md5(large_bytes).hex()
  assert hash.sha1(large)?.hex() == hash.sha1(large_bytes).hex()
  assert hash.sha256(large)?.hex() == hash.sha256(large_bytes).hex()
  assert hash.sha512(large)?.hex() == hash.sha512(large_bytes).hex()
  let large_digest = hash.sha256(large)?.hex()
  assert verify_message(hash.verify_file(large, sha256: large_digest)) == ""
  assert verify_message(hash.verify_file(large, sha256: large_digest.upper())) == ""

  # A batch of small files: the policy is per call, so every file in the batch
  # verifies against its own digest and none of them against the digest of the
  # file the batch wrote before it.
  var previous_digest = ""
  var index = 0
  while index < 32 {
    let batch_path = test.temp_path(ctx, name: f"hash-verify-batch-{index}.txt")
    fs.write(batch_path, f"batch {index}")
    let batch_digest = hash.sha256(batch_path)?.hex()
    assert verify_message(hash.verify_file(batch_path, sha256: batch_digest)) == ""
    if index > 0 {
      test.error_kind(
        hash.verify_file(batch_path, sha256: previous_digest),
        "checksum-mismatch",
      )
    }

    previous_digest = batch_digest
    index = index + 1
  }

  # A destination that cannot be read at all — a directory — is the same read
  # failure, not a checksum one.
  let directory = test.temp_dir(ctx, name: "hash-verify-dir")?
  test.error_kind(hash.verify_file(directory, sha256: zeros64), "hash-read")
}

# The message of a rejected checksum line, or the empty string when the line
# parsed. `test.error_kind` compares kinds only, so message parity is asserted
# through this.
pure check_line_message(result: Result[Record]) -> Str {
  match result {
    Ok(_) => ""
    Err(error) => error.message
  }
}

test test_parse_check_line_reads_both_gnu_separators {
  let digest = "900150983cd24fb0d6963f7d28e17f72"

  let two_space = hash.parse_check_line(f"{digest}  docs/readme.txt")?
  assert two_space.hex == digest
  assert two_space.path == "docs/readme.txt"
  assert two_space.binary == false

  let space_star = hash.parse_check_line(f"{digest} *readme.bin")?
  assert space_star.hex == digest
  assert space_star.path == "readme.bin"
  assert space_star.binary == true

  # A path may contain spaces: everything after the separator is kept, and
  # only trailing carriage returns are trimmed.
  let spaced = hash.parse_check_line(f"{digest}  my file.txt ")?
  assert spaced.path == "my file.txt "
}

test test_parse_check_line_handles_path_star_and_marker {
  let digest = "900150983cd24fb0d6963f7d28e17f72"

  # The marker comes from the separator, so a star that begins a double-space
  # path is stripped as part of the path and does not set the marker.
  let starred = hash.parse_check_line(f"{digest}  *readme.bin")?
  assert starred.path == "readme.bin"
  assert starred.binary == false

  # Exactly one leading star is dropped; a second one stays in the path.
  let twice = hash.parse_check_line(f"{digest}  **readme.bin")?
  assert twice.path == "*readme.bin"
  assert twice.binary == false
}

test test_parse_check_line_normalizes_case_and_carriage_returns {
  let upper = hash.parse_check_line("900150983CD24FB0D6963F7D28E17F72  readme.txt")?
  assert upper.hex == "900150983cd24fb0d6963f7d28e17f72"
  assert upper.path == "readme.txt"

  let crlf = hash.parse_check_line("900150983CD24FB0D6963F7D28E17F72 *readme.bin\r")?
  assert crlf.hex == "900150983cd24fb0d6963f7d28e17f72"
  assert crlf.path == "readme.bin"
  assert crlf.binary == true

  # Every trailing carriage return is trimmed, not just the last one.
  let doubled = hash.parse_check_line("900150983CD24FB0D6963F7D28E17F72  readme.txt\r\r")?
  assert doubled.hex == "900150983cd24fb0d6963f7d28e17f72"
  assert doubled.path == "readme.txt"
}

test test_parse_check_line_prefers_the_double_space_separator {
  # Digest length is a `hash.verify_file` policy: the line parser accepts any
  # non-empty hexadecimal field.
  let short = hash.parse_check_line("abc  readme.txt")?
  assert short.hex == "abc"
  assert short.path == "readme.txt"

  # The whole line is searched for the double-space separator first, so a
  # space-star pair that appears earlier does not win. The digest field then
  # spans the star and the line is rejected as not hexadecimal.
  let both = hash.parse_check_line("900150983cd24fb0d6963f7d28e17f72 *readme.bin  suffix")
  test.error_kind(both, "checksum-line")
  assert check_line_message(both) == "checksum is not hexadecimal"
}

test test_parse_check_line_rejects_malformed_lines {
  let separator_message = "expected `<hex>  <path>` or `<hex> *<path>`"
  test.error_kind(hash.parse_check_line(""), "checksum-line")
  assert check_line_message(hash.parse_check_line("")) == separator_message
  test.error_kind(
    hash.parse_check_line("900150983cd24fb0d6963f7d28e17f72"),
    "checksum-line",
  )
  assert check_line_message(hash.parse_check_line("900150983cd24fb0d6963f7d28e17f72")) == separator_message

  let incomplete_message = "checksum line is incomplete"
  test.error_kind(hash.parse_check_line("  readme.txt"), "checksum-line")
  assert check_line_message(hash.parse_check_line("  readme.txt")) == incomplete_message
  test.error_kind(hash.parse_check_line("900150983cd24fb0d6963f7d28e17f72  "), "checksum-line")
  assert check_line_message(hash.parse_check_line("900150983cd24fb0d6963f7d28e17f72  ")) == incomplete_message
  test.error_kind(hash.parse_check_line("900150983cd24fb0d6963f7d28e17f72 *"), "checksum-line")
  assert check_line_message(hash.parse_check_line("900150983cd24fb0d6963f7d28e17f72 *")) == incomplete_message

  # An empty field is incomplete even when the other field is not
  # hexadecimal, so emptiness outranks the hexadecimal check.
  assert check_line_message(hash.parse_check_line("  zz")) == incomplete_message
  assert check_line_message(hash.parse_check_line("zz  ")) == incomplete_message

  let hex_message = "checksum is not hexadecimal"
  test.error_kind(hash.parse_check_line("zz  readme.txt"), "checksum-line")
  assert check_line_message(hash.parse_check_line("zz  readme.txt")) == hex_message
  test.error_kind(
    hash.parse_check_line("900150983cd24fb0d6963f7d28e17e7g  readme.txt"),
    "checksum-line",
  )
  assert check_line_message(hash.parse_check_line("900150983cd24fb0d6963f7d28e17e7g  readme.txt")) == hex_message
}
