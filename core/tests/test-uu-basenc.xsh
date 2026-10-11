##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_basenc.rs.
##! Each test names its origin; expected values are the original assertions.

use support.uu as uu

# origin: uutils test_basenc::test_base16
test test_uu_basenc_base16 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base16"], stdin: b"Hello, World!")?
  uu.succeeds(r)
  uu.stdout_only(r, "48656C6C6F2C20576F726C6421\n")
}

# origin: uutils test_basenc::test_base16_decode
test test_uu_basenc_base16_decode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base16", "-d"], stdin: b"48656C6C6F2C20576F726C6421")?
  uu.succeeds(r)
  uu.stdout_only(r, "Hello, World!")
}

# origin: uutils test_basenc::test_base16_decode_and_ignore_garbage_lowercase
test test_uu_basenc_base16_decode_and_ignore_garbage_lowercase { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base16", "-d", "-i"], stdin: b"48656c6c6f2c20576f726c6421")?
  uu.succeeds(r)
  uu.stdout_only(r, "Hello, World!")
}

# origin: uutils test_basenc::test_base16_decode_lowercase
test test_uu_basenc_base16_decode_lowercase { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base16", "-d"], stdin: b"48656c6c6f2c20576f726c6421")?
  uu.succeeds(r)
  uu.stdout_only(r, "Hello, World!")
}

# origin: uutils test_basenc::test_base16_write_error_is_reported
test test_uu_basenc_base16_write_error_is_reported { |ctx|
  let s = uu.scene(ctx)?
  # Buffered output must report the write error when it is flushed.
  let r = uu.invoke(s, "basenc", ["--base16"], stdin: b"Hello, World!", stdout: /dev/full)?
  uu.fails(r)
  uu.stderr_is(r, "basenc: write error: No space left on device\n")
}

# origin: uutils test_basenc::test_base2lsbf
test test_uu_basenc_base2lsbf { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base2lsbf"], stdin: b"lsbf")?
  uu.succeeds(r)
  uu.stdout_only(r, "00110110110011100100011001100110\n")
}

# origin: uutils test_basenc::test_base2lsbf_decode
test test_uu_basenc_base2lsbf_decode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base2lsbf", "-d"], stdin: b"00110110110011100100011001100110")?
  uu.succeeds(r)
  uu.stdout_only(r, "lsbf")
}

# origin: uutils test_basenc::test_base2msbf
test test_uu_basenc_base2msbf { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base2msbf"], stdin: b"msbf")?
  uu.succeeds(r)
  uu.stdout_only(r, "01101101011100110110001001100110\n")
}

# origin: uutils test_basenc::test_base2msbf_decode
test test_uu_basenc_base2msbf_decode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base2msbf", "-d"], stdin: b"01101101011100110110001001100110")?
  uu.succeeds(r)
  uu.stdout_only(r, "msbf")
}

# origin: uutils test_basenc::test_base32
test test_uu_basenc_base32 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base32"], stdin: b"nice>base?")?
  uu.succeeds(r)
  uu.stdout_only(r, "NZUWGZJ6MJQXGZJ7\n")
}

# origin: uutils test_basenc::test_base32_autopad_multiline_stream
test test_uu_basenc_base32_autopad_multiline_stream { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base32", "--decode"], stdin: b"MFRGGZDF\nMFRGG")?
  uu.succeeds(r)
  uu.stdout_only(r, "abcdeabc")
}

# origin: uutils test_basenc::test_base32_autopad_short_quantum
test test_uu_basenc_base32_autopad_short_quantum { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base32", "--decode"], stdin: b"MFRGG")?
  uu.succeeds(r)
  uu.stdout_only(r, "abc")
}

# origin: uutils test_basenc::test_base32_baddecode_keeps_prefix
test test_uu_basenc_base32_baddecode_keeps_prefix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base32", "--decode"], stdin: b"MFRGGZDF=")?
  uu.fails(r)
  uu.stdout_is(r, "abcde")
  uu.stderr_is(r, "basenc: invalid input\n")
}

# origin: uutils test_basenc::test_base32_decode
test test_uu_basenc_base32_decode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base32", "-d"], stdin: b"NZUWGZJ6MJQXGZJ7")?
  uu.succeeds(r)
  uu.stdout_only(r, "nice>base?")
}

# origin: uutils test_basenc::test_base32_decode_repeated
test test_uu_basenc_base32_decode_repeated { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--ignore", "--wrap=80", "--base32hex", "--z85", "--ignore", "--decode", "--z85", "--base32", "-w", "10"], stdin: b"NZUWGZJ6MJQXGZJ7")?
  uu.succeeds(r)
  uu.stdout_only(r, "nice>base?")
}

# origin: uutils test_basenc::test_base32hex
test test_uu_basenc_base32hex { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base32hex"], stdin: b"nice>base?")?
  uu.succeeds(r)
  uu.stdout_only(r, "DPKM6P9UC9GN6P9V\n")
}

# origin: uutils test_basenc::test_base32hex_autopad_short_quantum
test test_uu_basenc_base32hex_autopad_short_quantum { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base32hex", "--decode"], stdin: b"C5H66")?
  uu.succeeds(r)
  uu.stdout_only(r, "abc")
}

# origin: uutils test_basenc::test_base32hex_decode
test test_uu_basenc_base32hex_decode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base32hex", "-d"], stdin: b"DPKM6P9UC9GN6P9V")?
  uu.succeeds(r)
  uu.stdout_only(r, "nice>base?")
}

# origin: uutils test_basenc::test_base32hex_rejects_trailing_garbage
test test_uu_basenc_base32hex_rejects_trailing_garbage { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base32hex", "-d"], stdin: b"VNC0FKD5W")?
  uu.fails(r)
  uu.stdout_is_bytes(r, b"\xFD\xD8\x07\xD1\xA5")
  uu.stderr_is(r, "basenc: invalid input\n")
}

# origin: uutils test_basenc::test_base32hex_truncated_block_keeps_prefix
test test_uu_basenc_base32hex_truncated_block_keeps_prefix { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base32hex", "-d"], stdin: b"CPNMUO")?
  uu.fails(r)
  uu.stdout_is_bytes(r, b"foo")
  uu.stderr_is(r, "basenc: invalid input\n")
}

# origin: uutils test_basenc::test_base58
test test_uu_basenc_base58 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base58"], stdin: b"Hello, World!")?
  uu.succeeds(r)
  uu.stdout_only(r, "72k1xXWG59fYdzSNoA\n")
}

# origin: uutils test_basenc::test_base58_decode
test test_uu_basenc_base58_decode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base58", "-d"], stdin: b"72k1xXWG59fYdzSNoA")?
  uu.succeeds(r)
  uu.stdout_only(r, "Hello, World!")
}

# origin: uutils test_basenc::test_base58_large_file_no_chunking
test test_uu_basenc_base58_large_file_no_chunking { |ctx|
  let s = uu.scene(ctx)?
  # Base58 treats the whole input as one integer, including files larger than 1024 bytes.
  var input = ""
  repeat 50 times {
    input += "Lorem ipsum dolor sit amet, consectetur adipiscing elit. "
  }
  uu.write(s, "large_file.txt", input)?
  let r = uu.invoke(s, "basenc", ["--base58", "large_file.txt"])?
  uu.succeeds(r)
  let encoded = r.stdout.utf8()?
  let trimmed = rx"\s+$".replace(encoded, with: "")
  assert trimmed.ends_with("ZNRRacEnhrY83ZEYkpwWVZNFK5DFRasr\nw693NsNGtiQ9fYAj")
}

# origin: uutils test_basenc::test_base64
test test_uu_basenc_base64 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base64"], stdin: b"to>be?")?
  uu.succeeds(r)
  uu.stdout_only(r, "dG8+YmU/\n")
}

# origin: uutils test_basenc::test_base64_decode
test test_uu_basenc_base64_decode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base64", "-d"], stdin: b"dG8+YmU/")?
  uu.succeeds(r)
  uu.stdout_only(r, "to>be?")
}

# origin: uutils test_basenc::test_base64url
test test_uu_basenc_base64url { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base64url"], stdin: b"to>be?")?
  uu.succeeds(r)
  uu.stdout_only(r, "dG8-YmU_\n")
}

# origin: uutils test_basenc::test_base64url_decode
test test_uu_basenc_base64url_decode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base64url", "-d"], stdin: b"dG8-YmU_")?
  uu.succeeds(r)
  uu.stdout_only(r, "to>be?")
}

# origin: uutils test_basenc::test_choose_last_encoding_base2lsbf
test test_uu_basenc_choose_last_encoding_base2lsbf { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base64url", "--base16", "--base2msbf", "--base32", "--base64", "--z85", "--base32hex", "--base2lsbf"], stdin: b"lsbf")?
  uu.succeeds(r)
  uu.stdout_only(r, "00110110110011100100011001100110\n")
}

# origin: uutils test_basenc::test_choose_last_encoding_base58
test test_uu_basenc_choose_last_encoding_base58 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base64", "--base32", "--base16", "--z85", "--base58"], stdin: b"Hello!")?
  uu.succeeds(r)
  uu.stdout_only(r, "d3yC1LKr\n")
}

# origin: uutils test_basenc::test_choose_last_encoding_base64
test test_uu_basenc_choose_last_encoding_base64 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base2msbf", "--base2lsbf", "--base64url", "--base32hex", "--base32", "--base16", "--z85", "--base64"], stdin: b"Hello, World!")?
  uu.succeeds(r)
  uu.stdout_only(r, "SGVsbG8sIFdvcmxkIQ==\n")
}

# origin: uutils test_basenc::test_file
test test_uu_basenc_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "file", "foo")?
  let r = uu.invoke(s, "basenc", ["file", "--base64"])?
  uu.succeeds(r)
  uu.stdout_is(r, "Zm9v\n")
}

# origin: uutils test_basenc::test_file_with_non_utf8_name
test test_uu_basenc_file_with_non_utf8_name { |ctx|
  let s = uu.scene(ctx)?
  let file = uu.at_bytes(s, b"\xff\xfe")?
  file.write(b"foo")?
  let r = uu.invoke_paths(s, "basenc", [b"\xff\xfe" as Path, p"--base64"])?
  uu.succeeds(r)
  uu.stdout_is(r, "Zm9v\n")
}

# origin: uutils test_basenc::test_invalid_input
test test_uu_basenc_invalid_input { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--base32", "."])?
  uu.fails(r)
  uu.stderr_only(r, "basenc: read error: Is a directory\n")
}

# origin: uutils test_basenc::test_z85_decode
test test_uu_basenc_z85_decode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--z85", "-d"], stdin: b"nm=QNz.92jz/PV8")?
  uu.succeeds(r)
  uu.stdout_only(r, "Hello, World")
}

# origin: uutils test_basenc::test_z85_length_check
test test_uu_basenc_z85_length_check { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--decode", "--z85"], stdin: b"f!$Kwh8WxM")?
  uu.succeeds(r)
  uu.stdout_only(r, "12345678")
}

# origin: uutils test_basenc::test_z85_not_padded_decode
test test_uu_basenc_z85_not_padded_decode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--z85", "-d"], stdin: b"##########")?
  uu.fails(r)
  uu.stderr_only(r, "basenc: invalid input\n")
}

# origin: uutils test_basenc::test_z85_not_padded_encode
test test_uu_basenc_z85_not_padded_encode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "basenc", ["--z85"], stdin: b"123")?
  uu.fails(r)
  uu.stderr_only(r, "basenc: invalid input (length must be multiple of 4 characters)\n")
}
