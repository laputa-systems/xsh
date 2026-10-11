##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_base32.rs.
##! Each test names its origin; GNU diagnostics follow the reference command.
##! The uutils-only -D decode clause is omitted because GNU base32 rejects it.

use support.uu as uu

# origin: uutils test_base32::test_base32_encode_file
test test_uu_base32_base32_encode_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "base32", "input-simple.txt", "input-simple.txt")?
  let r = uu.invoke(s, "base32", ["input-simple.txt"])?
  uu.succeeds(r)
  uu.stdout_only(r, "JBSWY3DPFQQFO33SNRSCCCQ=\n")
}

# origin: uutils test_base32::test_base32_extra_operand
test test_uu_base32_base32_extra_operand { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base32", ["a.txt", "b.txt"])?
  uu.fails(r)
  uu.stderr_only(r, "base32: extra operand 'b.txt'\nTry 'base32 --help' for more information.\n")
}

# origin: uutils test_base32::test_base32_file_not_found
test test_uu_base32_base32_file_not_found { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base32", ["a.txt"])?
  uu.fails(r)
  uu.stderr_only(r, "base32: a.txt: No such file or directory\n")
}

# origin: uutils test_base32::test_base32_file_with_trailing_slash
test test_uu_base32_base32_file_with_trailing_slash { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "b")?
  let r = uu.invoke(s, "base32", ["a/"])?
  uu.fails(r)
  uu.stderr_only(r, "base32: a/: Not a directory\n")
}

# origin: uutils test_base32::test_decode
test test_uu_base32_decode { |ctx|
  let s = uu.scene(ctx)?
  for decode_param in ["-d", "--decode", "--dec"] {
    let r = uu.invoke(s, "base32", [decode_param], stdin: b"JBSWY3DPFQQFO33SNRSCC===\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "Hello, World!")
  }
}

# origin: uutils test_base32::test_decode_repeat_flags
test test_uu_base32_decode_repeat_flags { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base32", ["-didiw80", "--wrap=17", "--wrap", "8"], stdin: b"JBSWY3DPFQQFO33SNRSCC===\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "Hello, World!")
}

# origin: uutils test_base32::test_encode
test test_uu_base32_encode { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base32", [], stdin: b"Hello, World!")?
  uu.succeeds(r)
  uu.stdout_only(r, "JBSWY3DPFQQFO33SNRSCC===\n")

  let dash = uu.invoke(s, "base32", ["-"], stdin: b"Hello, World!")?
  uu.succeeds(dash)
  uu.stdout_only(dash, "JBSWY3DPFQQFO33SNRSCC===\n")
}

# origin: uutils test_base32::test_encode_large_input_is_buffered
test test_uu_base32_encode_large_input_is_buffered { |ctx|
  let s = uu.scene(ctx)?
  let input = ["A"] |> repeat(6000) |> join("")
  let r = uu.invoke(s, "base32", [], stdin: bytes.from_text(input))?
  uu.succeeds(r)
  uu.stdout_contains(r, "BIFAUCQK")
}

# origin: uutils test_base32::test_encode_repeat_flags_later_wrap_10
test test_uu_base32_encode_repeat_flags_later_wrap_10 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base32", ["-ii", "-w17", "-w10"], stdin: b"Hello, World!\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "JBSWY3DPFQ\nQFO33SNRSC\nCCQ=\n")
}

# origin: uutils test_base32::test_encode_repeat_flags_later_wrap_17
test test_uu_base32_encode_repeat_flags_later_wrap_17 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base32", ["-ii", "-w10", "-w17"], stdin: b"Hello, World!\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "JBSWY3DPFQQFO33SN\nRSCCCQ=\n")
}

# origin: uutils test_base32::test_garbage
test test_uu_base32_garbage { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "base32", ["-d"], stdin: b"aGVsbG8sIHdvcmxkIQ==\0")?
  uu.fails(r)
  uu.stderr_only(r, "base32: invalid input\n")
}

# origin: uutils test_base32::test_ignore_garbage
test test_uu_base32_ignore_garbage { |ctx|
  let s = uu.scene(ctx)?
  for ignore_garbage_param in ["-i", "--ignore-garbage", "--ig"] {
    let r = uu.invoke(s, "base32", ["-d", ignore_garbage_param], stdin: b"JBSWY\x013DPFQ\x02QFO33SNRSCC===\n")?
    uu.succeeds(r)
    uu.stdout_only(r, "Hello, World!")
  }
}

# origin: uutils test_base32::test_wrap
test test_uu_base32_wrap { |ctx|
  let s = uu.scene(ctx)?
  for wrap_param in ["-w", "--wrap", "--wr"] {
    let r = uu.invoke(s, "base32", [wrap_param, "20"], stdin: b"The quick brown fox jumps over the lazy dog.")?
    uu.succeeds(r)
    uu.stdout_only(r, "KRUGKIDROVUWG2ZAMJZG\n653OEBTG66BANJ2W24DT\nEBXXMZLSEB2GQZJANRQX\nU6JAMRXWOLQ=\n")
  }
}

# origin: uutils test_base32::test_wrap_bad_arg
test test_uu_base32_wrap_bad_arg { |ctx|
  let s = uu.scene(ctx)?
  for wrap_param in ["-w", "--wrap"] {
    let r = uu.invoke(s, "base32", [wrap_param, "b"])?
    uu.fails(r)
    uu.stderr_only(r, "base32: invalid wrap size: 'b'\n")
  }
}

# origin: uutils test_base32::test_wrap_no_arg
test test_uu_base32_wrap_no_arg { |ctx|
  let s = uu.scene(ctx)?
  for wrap_param in ["-w", "--wrap"] {
    let r = uu.invoke(s, "base32", [wrap_param])?
    uu.fails(r)
    let message = if wrap_param == "-w" { "option requires an argument -- 'w'" } else { "option '--wrap' requires an argument" }
    uu.stderr_only(r, f"base32: {message}\nTry 'base32 --help' for more information.\n")
  }
}
