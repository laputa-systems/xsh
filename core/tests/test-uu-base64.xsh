##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_base64.rs.
##! Each test names its origin; expectations follow the GNU command contract.

use support.uu as uu

# origin: uutils test_base64::test_base64_encode_file
test test_uu_base64_base64_encode_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "base64", "input-simple.txt", "input-simple.txt")?
  let r = uu.invoke(s, "base64", ["input-simple.txt"])?
  uu.succeeds(r)
  uu.stdout_only(r, "SGVsbG8sIFdvcmxkIQo=\n")
}

# origin: uutils test_base64::test_base64_extra_operand
test test_uu_base64_base64_extra_operand { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base64", ["a.txt", "b.txt"])?
  uu.fails(r)
  uu.stderr_only(r, "base64: extra operand 'b.txt'\nTry 'base64 --help' for more information.\n")
}

# origin: uutils test_base64::test_base64_file_not_found
test test_uu_base64_base64_file_not_found { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base64", ["a.txt"])?
  uu.fails(r)
  uu.stderr_only(r, "base64: a.txt: No such file or directory\n")
}

# origin: uutils test_base64::test_base64_file_with_trailing_slash
test test_uu_base64_base64_file_with_trailing_slash { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "b")?
  let r = uu.invoke(s, "base64", ["a/"])?
  uu.fails(r)
  uu.stderr_only(r, "base64: a/: Not a directory\n")
}

# origin: uutils test_base64::test_base64_non_utf8_paths
test test_uu_base64_base64_non_utf8_paths { |ctx|
  let s = uu.scene(ctx)?
  let filename = uu.at_bytes(s, b"\xff\xfe")?
  filename.write(b"hello world")?
  let r = uu.invoke_paths(s, "base64", [b"\xff\xfe" as Path])?
  uu.succeeds(r)
  uu.stdout_is(r, "aGVsbG8gd29ybGQ=\n")
}

# origin: uutils test_base64::test_decode
test test_uu_base64_decode { |ctx|
  let s = uu.scene(ctx)?
  for decode_param in ["-d", "--decode", "--dec"] {
    let r = uu.invoke(s, "base64", [decode_param], b"aGVsbG8sIHdvcmxkIQ==")?
    uu.succeeds(r)
    uu.stdout_only(r, "hello, world!")
  }
}

# origin: uutils test_base64::test_decode_padded_block_followed_by_aligned_tail
test test_uu_base64_decode_padded_block_followed_by_aligned_tail { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base64", ["--decode"], b"MTIzNA==QUJD")?
  uu.succeeds(r)
  uu.stdout_only(r, "1234ABC")
}

# origin: uutils test_base64::test_decode_padded_block_followed_by_unpadded_tail
test test_uu_base64_decode_padded_block_followed_by_unpadded_tail { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base64", ["--decode"], b"MTIzNA==MTIzNA")?
  uu.succeeds(r)
  uu.stdout_only(r, "12341234")
}

# origin: uutils test_base64::test_decode_repeat_flags
test test_uu_base64_decode_repeat_flags { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base64", ["-didiw80", "--wrap=17", "--wrap", "8"], b"aGVsbG8sIHdvcmxkIQ==\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "hello, world!")
}

# origin: uutils test_base64::test_decode_short
test test_uu_base64_decode_short { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base64", ["--decode"], b"aQ")?
  uu.succeeds(r)
  uu.stdout_only(r, "i")
}

# origin: uutils test_base64::test_decode_unpadded_stream_without_equals
test test_uu_base64_decode_unpadded_stream_without_equals { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base64", ["--decode"], b"MTIzNA")?
  uu.succeeds(r)
  uu.stdout_only(r, "1234")
}

# origin: uutils test_base64::test_encode
test test_uu_base64_encode { |ctx|
  let s = uu.scene(ctx)?
  let input = b"hello, world!"
  let r = uu.invoke(s, "base64", [], input)?
  uu.succeeds(r)
  uu.stdout_only(r, "aGVsbG8sIHdvcmxkIQ==\n")
  let dash = uu.invoke(s, "base64", ["-"], input)?
  uu.succeeds(dash)
  uu.stdout_only(dash, "aGVsbG8sIHdvcmxkIQ==\n")
}

# origin: uutils test_base64::test_encode_repeat_flags_later_wrap_10
test test_uu_base64_encode_repeat_flags_later_wrap_10 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base64", ["-ii", "-w15", "-w10"], b"hello, world!")?
  uu.succeeds(r)
  uu.stdout_only(r, "aGVsbG8sIH\ndvcmxkIQ==\n")
}

# origin: uutils test_base64::test_encode_repeat_flags_later_wrap_15
test test_uu_base64_encode_repeat_flags_later_wrap_15 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base64", ["-ii", "-w10", "-w15"], b"hello, world!")?
  uu.succeeds(r)
  uu.stdout_only(r, "aGVsbG8sIHdvcmx\nkIQ==\n")
}

# origin: uutils test_base64::test_garbage
test test_uu_base64_garbage { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base64", ["-d"], b"aGVsbG8sIHdvcmxkIQ==\x00")?
  uu.fails(r)
  uu.stdout_is(r, "hello, world!")
  uu.stderr_is(r, "base64: invalid input\n")
}

# origin: uutils test_base64::test_ignore_garbage
test test_uu_base64_ignore_garbage { |ctx|
  let s = uu.scene(ctx)?
  for ignore_garbage_param in ["-i", "--ignore-garbage", "--ig"] {
    let r = uu.invoke(s, "base64", ["-d", ignore_garbage_param], b"aGVsbG8sIHdvcmxkIQ==\x00")?
    uu.succeeds(r)
    uu.stdout_only(r, "hello, world!")
  }
}

# origin: uutils test_base64::test_multi_lines
test test_uu_base64_multi_lines { |ctx|
  let s = uu.scene(ctx)?
  for input in [b"aQ\n\n\n", b"a\nQ==\n\n\n"] {
    let r = uu.invoke(s, "base64", ["--decode"], input)?
    uu.succeeds(r)
    uu.stdout_only(r, "i")
  }
}

# origin: uutils test_base64::test_no_repeated_trailing_newline
test test_uu_base64_no_repeated_trailing_newline { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base64", ["--wrap", "10", "--", "-"], b"The quick brown fox jumps over the lazy dog.")?
  uu.succeeds(r)
  uu.stdout_only(r, "VGhlIHF1aW\nNrIGJyb3du\nIGZveCBqdW\n1wcyBvdmVy\nIHRoZSBsYX\np5IGRvZy4=\n")
}

# origin: uutils test_base64::test_wrap
test test_uu_base64_wrap { |ctx|
  let s = uu.scene(ctx)?
  for wrap_param in ["-w", "--wrap", "--wr"] {
    let r = uu.invoke(s, "base64", [wrap_param, "20"], b"The quick brown fox jumps over the lazy dog.")?
    uu.succeeds(r)
    uu.stdout_only(r, "VGhlIHF1aWNrIGJyb3du\nIGZveCBqdW1wcyBvdmVy\nIHRoZSBsYXp5IGRvZy4=\n")
  }
  let input = b"hello, world"
  let zero = uu.invoke(s, "base64", ["--wrap", "0"], input)?
  uu.succeeds(zero)
  uu.stdout_only(zero, "aGVsbG8sIHdvcmxk")
  let thirty = uu.invoke(s, "base64", ["--wrap", "30"], input)?
  uu.succeeds(thirty)
  uu.stdout_only(thirty, "aGVsbG8sIHdvcmxk\n")
}

# origin: uutils test_base64::test_wrap_bad_arg
test test_uu_base64_wrap_bad_arg { |ctx|
  let s = uu.scene(ctx)?
  for wrap_param in ["-w", "--wrap"] {
    let r = uu.invoke(s, "base64", [wrap_param, "b"])?
    uu.fails(r)
    uu.stderr_only(r, "base64: invalid wrap size: 'b'\n")
  }
}

# origin: uutils test_base64::test_wrap_default
test test_uu_base64_wrap_default { |ctx|
  let s = uu.scene(ctx)?
  let input = b"The quick brown fox jumps over the lazy dog. The quick brown fox jumps over the lazy dog. The quick brown fox jumps over the lazy dog."
  let r = uu.invoke(s, "base64", ["--", "-"], input)?
  uu.succeeds(r)
  uu.stdout_only(r, "VGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVyIHRoZSBsYXp5IGRvZy4gVGhlIHF1aWNrIGJy\nb3duIGZveCBqdW1wcyBvdmVyIHRoZSBsYXp5IGRvZy4gVGhlIHF1aWNrIGJyb3duIGZveCBqdW1w\ncyBvdmVyIHRoZSBsYXp5IGRvZy4=\n")
}

# origin: uutils test_base64::test_wrap_negative_arg
test test_uu_base64_wrap_negative_arg { |ctx|
  let s = uu.scene(ctx)?
  for arg in ["-5", "-d"] {
    let r = uu.invoke(s, "base64", ["-w", arg])?
    uu.fails(r)
    uu.stderr_only(r, f"base64: invalid wrap size: '{arg}'\n")
  }
}

# origin: uutils test_base64::test_wrap_no_arg
test test_uu_base64_wrap_no_arg { |ctx|
  let s = uu.scene(ctx)?
  for wrap_param in ["-w", "--wrap"] {
    let r = uu.invoke(s, "base64", [wrap_param])?
    uu.fails(r)
    let diagnostic = if wrap_param == "-w" {
      "base64: option requires an argument -- 'w'\nTry 'base64 --help' for more information.\n"
    } else {
      "base64: option '--wrap' requires an argument\nTry 'base64 --help' for more information.\n"
    }
    uu.stderr_only(r, diagnostic)
  }
}
