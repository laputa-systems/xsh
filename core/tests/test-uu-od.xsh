##! Transcribed from the uutils coreutils integration tests for od.

use support.uu as uu

const ALPHA_OUT = "0000000 061141 062143 063145 064147 065151 066153 067155 070157\n0000020 071161 072163 073165 074167 075171 000012\n0000033\n"

# Each upstream scene starts with these files, including operands that look like options.
proc od_scene(ctx: TestContext) [fs, error] -> Result[uu.Scene, Error] {
  let s = uu.scene(ctx)?
  for name in ["-f", "0", "c", "x"] { uu.fixture(s, "od", name, name)? }
  Ok(s)
}

# origin: uutils test_od::diagnostics::test_plain_message_when_stderr_is_a_pipe
test test_uu_od_diagnostics_plain_message_when_stderr_is_a_pipe { |ctx|
  let s = od_scene(ctx)?
  let r1 = uu.invoke(s, "od", ["-N", "3zz", "/dev/null"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_is(r1, "od: invalid suffix in -N argument '3zz'\n")
}

# origin: uutils test_od::test_alignment_Fx
test test_uu_od_alignment_Fx { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([0, 0, 0, 0, 0, 0, 0, 192])?
  let expected_output = "0000000                       -2\n          0000  0000  0000  c000\n0000010\n"
  let r1 = uu.invoke(s, "od", ["--endian=little", "-F", "-x"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_alignment_Xxa
test test_uu_od_alignment_Xxa { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([10, 13, 101, 102, 103, 0, 158, 159])?
  let expected_output = "0000000        66650d0a        9f9e0067\n           0d0a    6665    0067    9f9e\n         nl  cr   e   f   g nul  rs  us\n0000010\n"
  let r1 = uu.invoke(s, "od", ["--endian=little", "-X", "-x", "-a"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_ascii_dump
test test_uu_od_ascii_dump { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([0, 1, 10, 13, 16, 31, 32, 97, 98, 99, 125, 126, 127, 128, 144, 160, 176, 192, 208, 224, 240, 255])?
  let r1 = uu.invoke(s, "od", ["-tx1zacz"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0000000  00  01  0a  0d  10  1f  20  61  62  63  7d  7e  7f  80  90  a0  >...... abc}~....<\n        nul soh  nl  cr dle  us  sp   a   b   c   }   ~ del nul dle  sp\n         \\0 001  \\n  \\r 020 037       a   b   c   }   ~ 177 200 220 240  >...... abc}~....<\n0000020  b0  c0  d0  e0  f0  ff                                          >......<\n          0   @   P   `   p del\n        260 300 320 340 360 377                                          >......<\n0000026\n")
}

# origin: uutils test_od::test_bfloat16_compact
test test_uu_od_bfloat16_compact { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([63, 128, 63, 128])?
  let r1 = uu.invoke(s, "od", ["--endian=big", "-An", "-tfB"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "               1               1\n")
}

# origin: uutils test_od::test_big_endian
test test_uu_od_big_endian { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([192, 0, 0, 0, 0, 0, 0, 0])?
  let expected_output = "0000000                              -2\n                     -2               0\n               c0000000        00000000\n           c000    0000    0000    0000\n0000010\n"
  let r1 = uu.invoke(s, "od", ["--endian=big", "-F", "-f", "-X", "-x"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
  let r2 = uu.invoke(s, "od", ["--endian=b", "-F", "-f", "-X", "-x"], stdin: input)?
  uu.succeeds(r2)
  uu.stdout_only(r2, expected_output)
}

# origin: uutils test_od::test_dec
test test_uu_od_dec { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([0, 0, 1, 0, 2, 0, 3, 0, 255, 127, 0, 128, 1, 128])?
  let expected_output = "0000000      0      1      2      3  32767 -32768 -32767\n0000016\n"
  let r1 = uu.invoke(s, "od", ["--endian=little", "-s"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_dec_offset
test test_uu_od_dec_offset { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([0 for _ in range(19)])?
  let expected_output = "0000000 00000000 00000000 00000000 00000000\n        00000000 00000000 00000000 00000000\n0000016 00000000\n        00000000\n0000019\n"
  let r1 = uu.invoke(s, "od", ["-Ad", "-X", "-X"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_empty_offset
test test_uu_od_empty_offset { |ctx|
  let s = od_scene(ctx)?
  let r1 = uu.invoke(s, "od", ["-A", ""])?
  uu.fails(r1)
  uu.no_stdout(r1)
  uu.stderr_is_bytes(r1, b"od: invalid output address radix '\0'; it must be one character from [doxn]\n")
}

# origin: uutils test_od::test_f16
test test_uu_od_f16 { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([0, 60, 0, 0, 0, 128, 0, 124, 0, 252, 0, 254, 0, 132])?
  let expected_output = "0000000               1               0              -0             inf\n0000010            -inf            -nan  -6.1035156e-05\n0000016\n"
  let r1 = uu.invoke(s, "od", ["--endian=little", "-tf2", "-w8"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_f32
test test_uu_od_f32 { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([82, 6, 158, 191, 78, 97, 60, 75, 15, 155, 148, 254, 0, 0, 0, 128, 255, 255, 255, 127, 194, 22, 1, 0, 0, 0, 127, 128])?
  let expected_output = "0000000      -1.2345679        12345678   -9.876543e+37              -0\n0000020             nan           1e-40  -1.1663108e-38\n0000034\n"
  let r1 = uu.invoke(s, "od", ["--endian=little", "-f"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_f64
test test_uu_od_f64 { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([39, 107, 10, 47, 42, 238, 69, 67, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 16, 128, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 192])?
  let expected_output = "0000000        12345678912345678                        0\n0000020 -2.2250738585072014e-308                   5e-324\n0000040                       -2\n0000050\n"
  let r1 = uu.invoke(s, "od", ["--endian=little", "-F"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_fb
test test_uu_od_fb { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([128, 63, 0, 0, 0, 128, 128, 127, 128, 255, 192, 127, 128, 184])?
  let expected_output = "0000000               1               0              -0             inf\n0000010            -inf             nan  -6.1035156e-05\n0000016\n"
  let r1 = uu.invoke(s, "od", ["--endian=little", "-tfB", "-w8"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_fh
test test_uu_od_fh { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([0, 60, 0, 0, 0, 128, 0, 124, 0, 252, 0, 254, 0, 132])?
  let expected_output = "0000000               1               0              -0             inf\n0000010            -inf            -nan  -6.1035156e-05\n0000016\n"
  let r1 = uu.invoke(s, "od", ["--endian=little", "-tfH", "-w8"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_file
test test_uu_od_file { |ctx|
  let s = od_scene(ctx)?
  uu.write(s, "test", "abcdefghijklmnopqrstuvwxyz\n")?
  for arg in ["--endian=little", "--endian=littl", "--endian=l"] {
    let r = uu.invoke(s, "od", [arg, "test"])?
    uu.succeeds(r)
    uu.stdout_only(r, ALPHA_OUT)
  }
  let r = uu.invoke(s, "od", ["--endian=little", "-t", "o2", "test"])?
  uu.succeeds(r)
  uu.stdout_only(r, ALPHA_OUT)
}

# origin: uutils test_od::test_file_offset
test test_uu_od_file_offset { |ctx|
  let s = od_scene(ctx)?
  let r1 = uu.invoke(s, "od", ["-c", "--", "-f", "10"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0000010   w   e   r   c   a   s   e       f  \\n\n0000022\n")
}

# origin: uutils test_od::test_filename_parsing
test test_uu_od_filename_parsing { |ctx|
  let s = od_scene(ctx)?
  let r1 = uu.invoke(s, "od", ["--format", "a", "-A", "x", "--", "-f"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "000000   m   i   n   u   s  sp   l   o   w   e   r   c   a   s   e  sp\n000010   f  nl\n000012\n")
}

# origin: uutils test_od::test_float16_compact
test test_uu_od_float16_compact { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([60, 0, 60, 0])?
  let r1 = uu.invoke(s, "od", ["--endian=big", "-An", "-tfH"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "               1               1\n")
}

# origin: uutils test_od::test_from_mixed
test test_uu_od_from_mixed { |ctx|
  let s = od_scene(ctx)?
  uu.write(s, "test-1", "abcdefg")?
  uu.write(s, "test-3", "qrstuvwxyz\n")?
  let r = uu.invoke(s, "od", ["--endian=little", "test-1", "-", "test-3"], stdin: b"hijklmnop")?
  uu.succeeds(r)
  uu.stdout_only(r, ALPHA_OUT)
}

# origin: uutils test_od::test_from_stdin
test test_uu_od_from_stdin { |ctx|
  let s = od_scene(ctx)?
  let input = "abcdefghijklmnopqrstuvwxyz\n"
  let r1 = uu.invoke(s, "od", ["--endian=little"], stdin: bytes.from_text(input))?
  uu.succeeds(r1)
  uu.stdout_only(r1, ALPHA_OUT)
}

# origin: uutils test_od::test_hex16
test test_uu_od_hex16 { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([1, 35, 69, 103, 137, 171, 205, 239, 255])?
  let expected_output = "0000000 2301 6745 ab89 efcd 00ff\n0000011\n"
  let r1 = uu.invoke(s, "od", ["--endian=little", "-x"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_hex32
test test_uu_od_hex32 { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([1, 35, 69, 103, 137, 171, 205, 239, 255])?
  let expected_output = "0000000 67452301 efcdab89 000000ff\n0000011\n"
  let r1 = uu.invoke(s, "od", ["--endian=little", "-X"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_hex_lowercase
test test_uu_od_hex_lowercase { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([0 for _ in range(10)])?
  let r1 = uu.invoke(s, "od", ["-Ax"], stdin: input)?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_only(r1, "000000 000000 000000 000000 000000 000000\n00000a\n")
}

# origin: uutils test_od::test_hex_offset
test test_uu_od_hex_offset { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([0 for _ in range(31)])?
  let expected_output = "000000 00000000 00000000 00000000 00000000\n       00000000 00000000 00000000 00000000\n000010 00000000 00000000 00000000 00000000\n       00000000 00000000 00000000 00000000\n00001f\n"
  let r1 = uu.invoke(s, "od", ["-Ax", "-X", "-X"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_hyphen_leading_byte_count_is_reported_as_invalid
test test_uu_od_hyphen_leading_byte_count_is_reported_as_invalid { |ctx|
  let s = od_scene(ctx)?
  for opt in ["-N", "-j"] {
    let r = uu.invoke(s, "od", [opt, "-1"], stdin: b"")?
    uu.fails(r)
    uu.stderr_contains(r, f"invalid {opt} argument '-1'")
  }
}

# origin: uutils test_od::test_invalid_arg
test test_uu_od_invalid_arg { |ctx|
  let s = od_scene(ctx)?
  let r1 = uu.invoke(s, "od", ["--definitely-invalid"])?
  uu.fails_with_code(r1, 1)
}

# origin: uutils test_od::test_invalid_offset
test test_uu_od_invalid_offset { |ctx|
  let s = od_scene(ctx)?
  let r1 = uu.invoke(s, "od", ["-Ab"])?
  uu.fails(r1)
}

# origin: uutils test_od::test_invalid_traditional_offsets_are_filenames
test test_uu_od_invalid_traditional_offsets_are_filenames { |ctx|
  let s = od_scene(ctx)?
  for case in [{input: "++0", display: "++0"}, {input: "+-0", display: "+-0"}, {input: "+ 0", display: "'+ 0'"}] {
    let r = uu.invoke(s, "od", [case.input])?
    uu.fails_with_code(r, 1)
    uu.stderr_only(r, f"od: {case.display}: No such file or directory\n")
  }
  let r = uu.invoke(s, "od", ["--", "-0"])?
  uu.fails_with_code(r, 1)
  uu.stderr_only(r, "od: -0: No such file or directory\n")
}

# origin: uutils test_od::test_invalid_width
test test_uu_od_invalid_width { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([0, 0, 0, 0])?
  let expected_output = "0000000 000000\n0000002 000000\n0000004\n"
  let r1 = uu.invoke(s, "od", ["-w5", "-v"], stdin: input)?
  uu.succeeds(r1)
  uu.stderr_is_bytes(r1, bytes.from_text("od: warning: invalid width 5; using 2 instead\n"))
  uu.stdout_is(r1, expected_output)
}

# origin: uutils test_od::test_is_a_directory
test test_uu_od_is_a_directory { |ctx|
  let s = od_scene(ctx)?
  uu.mkdir(s, "a")?
  let r1 = uu.invoke(s, "od", ["a"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_is(r1, "od: a: Is a directory\n")
}

# origin: uutils test_od::test_large_width_ascii_dump
test test_uu_od_large_width_ascii_dump { |ctx|
  let s = od_scene(ctx)?
  let r = uu.invoke(s, "od", ["-w4000000", "-tcz"], stdin: b"x")?
  uu.succeeds(r)
  assert r.stdout.len() == 4 * 4000000 + 21
  assert r.stdout.utf8()?.ends_with("  >x<\n0000001\n")
}

# origin: uutils test_od::test_max_uint
test test_uu_od_max_uint { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([255 for _ in range(8)])?
  let expected_output = "0000000          1777777777777777777777\n            37777777777     37777777777\n         177777  177777  177777  177777\n        377 377 377 377 377 377 377 377\n                   18446744073709551615\n             4294967295      4294967295\n          65535   65535   65535   65535\n        255 255 255 255 255 255 255 255\n0000010\n"
  let r1 = uu.invoke(s, "od", ["--format=o8", "-Oobtu8", "-Dd", "--format=u1"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_multibyte
test test_uu_od_multibyte { |ctx|
  let s = od_scene(ctx)?
  let input = "’‐ˆ‘˜語🙂✅🐶𝛑Universität Tübingen 𛀀"
  let r1 = uu.invoke(s, "od", ["-t", "c"], stdin: bytes.from_text(input))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0000000 342 200 231 342 200 220 313 206 342 200 230 313 234 350 252 236\n0000020 360 237 231 202 342 234 205 360 237 220 266 360 235 233 221   U\n0000040   n   i   v   e   r   s   i   t 303 244   t       T 303 274   b\n0000060   i   n   g   e   n     360 233 200 200\n0000072\n")
}

# origin: uutils test_od::test_multiple_formats
test test_uu_od_multiple_formats { |ctx|
  let s = od_scene(ctx)?
  let input = "abcdefghijklmnopqrstuvwxyz\n"
  let r1 = uu.invoke(s, "od", ["-c", "-b"], stdin: bytes.from_text(input))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0000000   a   b   c   d   e   f   g   h   i   j   k   l   m   n   o   p\n        141 142 143 144 145 146 147 150 151 152 153 154 155 156 157 160\n0000020   q   r   s   t   u   v   w   x   y   z  \\n\n        161 162 163 164 165 166 167 170 171 172 012\n0000033\n")
}

# origin: uutils test_od::test_negative_width_argument
test test_uu_od_negative_width_argument { |ctx|
  let s = od_scene(ctx)?
  let r1 = uu.invoke(s, "od", ["-w-1", "-An"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "od: invalid -w argument '-1'\n")
}

# origin: uutils test_od::test_no_offset
test test_uu_od_no_offset { |ctx|
  let s = od_scene(ctx)?
  let LINE = " 00000000 00000000 00000000 00000000\n"
  let input = bytes.from_ints([0 for _ in range(31)])?
  let expected_output = [LINE, LINE, LINE, LINE].join("")
  let r1 = uu.invoke(s, "od", ["-An", "-X", "-X"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_non_existing_file
test test_uu_od_non_existing_file { |ctx|
  let s = od_scene(ctx)?
  let r1 = uu.invoke(s, "od", ["non_existing_file"])?
  uu.fails(r1)
}

# origin: uutils test_od::test_non_numeric_width_argument
test test_uu_od_non_numeric_width_argument { |ctx|
  let s = od_scene(ctx)?
  let r1 = uu.invoke(s, "od", ["-ww", "-An"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "od: invalid -w argument 'w'\n")
}

# origin: uutils test_od::test_od_eintr_handling
test test_uu_od_od_eintr_handling { |ctx|
  let s = od_scene(ctx)?
  let file = "test_eintr"
  let test_data = bytes.from_ints([72, 101, 108, 108, 111, 10])?
  uu.write_bytes(s, file, test_data)?
  let r1 = uu.invoke(s, "od", [file, "-t", "c"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_contains(r1, "H")
  let second_s = od_scene(ctx)?
  uu.write_bytes(second_s, file, test_data)?
  let r2 = uu.invoke(second_s, "od", [file, "-j", "1", "-N", "3", "-t", "c"])?
  uu.succeeds(r2)
  uu.no_stderr(r2)
  uu.stdout_contains(r2, "e")
}

# origin: uutils test_od::test_od_invalid_bytes
test test_uu_od_od_invalid_bytes { |ctx|
  let s = od_scene(ctx)?
  for option in ["--read-bytes", "--skip-bytes", "--width"] {
    let invalid = uu.invoke(s, "od", [f"{option}=x", "file"])?
    uu.fails_with_code(invalid, 1)
    uu.stderr_only(invalid, f"od: invalid {option} argument 'x'\n")
    let suffix = uu.invoke(s, "od", [f"{option}=1fb4t", "file"])?
    uu.fails_with_code(suffix, 1)
    uu.stderr_only(suffix, f"od: invalid suffix in {option} argument '1fb4t'\n")
    let big = uu.invoke(s, "od", [f"{option}=1Y", "file"])?
    uu.fails_with_code(big, 1)
    let expected = if option == "--width" { "od: invalid suffix in --width argument '1Y'\n" } else { f"od: {option} argument '1Y' too large\n" }
    uu.stderr_only(big, expected)
  }
}

# origin: uutils test_od::test_od_options_after_filename
test test_uu_od_od_options_after_filename { |ctx|
  let s = od_scene(ctx)?
  let file = "test"
  let input = bytes.from_ints([104, 28, 187, 253])?
  uu.write_bytes(s, file, input)?
  let r1 = uu.invoke(s, "od", [file, "-v", "-An", "-t", "x2", "--endian=little"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, " 1c68 fdbb\n")
}

# origin: uutils test_od::test_od_strings_option
test test_uu_od_od_strings_option { |ctx|
  let s = od_scene(ctx)?
  let expected = "0000000   \n"
  let r1 = uu.invoke(s, "od", ["-S0"], stdin: b"hello\x00world\x00")?
  uu.succeeds(r1)
  uu.stdout_is(r1, "0000000 hello\n0000006 world\n")
  let r2 = uu.invoke(s, "od", ["-S0"], stdin: b"a\x00b\x00cd\x00")?
  uu.succeeds(r2)
  uu.stdout_is(r2, "0000000 a\n0000002 b\n0000004 cd\n")
  let r3 = uu.invoke(s, "od", ["-S3"], stdin: b"\x01hello\x00world\x00ab\x00")?
  uu.succeeds(r3)
  uu.stdout_is(r3, "0000001 hello\n0000007 world\n")
  let r4 = uu.invoke(s, "od", ["-S10"], stdin: b"\x01          \x00          \x00")?
  uu.succeeds(r4)
  uu.stdout_is(r4, "0000001           \n0000014           \n")
  let r5 = uu.invoke(s, "od", ["-S10"], stdin: b"          ")?
  uu.succeeds(r5)
  uu.no_output(r5)
  let r6 = uu.invoke(s, "od", ["-N2", "-S1"], stdin: bytes.from_text("  "))?
  uu.succeeds(r6)
  uu.stdout_is(r6, expected)
  let r7 = uu.invoke(s, "od", ["-N11", "-S11"], stdin: b"          \x00")?
  uu.succeeds(r7)
  uu.no_output(r7)
  let r8 = uu.invoke(s, "od", ["-S3", "-An"], stdin: b"hello\x00world\x00")?
  uu.succeeds(r8)
  uu.stdout_is(r8, "hello\nworld\n")
}

# origin: uutils test_od::test_od_strings_with_n_flag
test test_uu_od_od_strings_with_n_flag { |ctx|
  let s = od_scene(ctx)?
  let input = b"foo\0bar\0"
  let r1 = uu.invoke(s, "od", ["--strings", "-N7"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0000000 foo\n0000004 bar\n")
  let r2 = uu.invoke(s, "od", ["--strings", "-N8"], stdin: input)?
  uu.succeeds(r2)
  uu.stdout_only(r2, "0000000 foo\n0000004 bar\n")
}

# origin: uutils test_od::test_offset_compatibility
test test_uu_od_offset_compatibility { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([0 for _ in range(4)])?
  let expected_output = " 000000 000000\n"
  let r1 = uu.invoke(s, "od", ["-Anone"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_skip_bytes
test test_uu_od_skip_bytes { |ctx|
  let s = od_scene(ctx)?
  let input = "abcdefghijklmnopq"
  let r1 = uu.invoke(s, "od", ["-c", "--skip-bytes=5"], stdin: bytes.from_text(input))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0000005   f   g   h   i   j   k   l   m   n   o   p   q\n0000021\n")
}

# origin: uutils test_od::test_skip_bytes_consumes_single_input
test test_uu_od_skip_bytes_consumes_single_input { |ctx|
  let s = od_scene(ctx)?
  uu.write(s, "g", "a")?
  let r = uu.invoke(s, "od", ["-c", "-j", "1", "-An", "g"])?
  uu.succeeds(r)
  uu.stdout_only(r, "")
}

# origin: uutils test_od::test_skip_bytes_consumes_three_inputs
test test_uu_od_skip_bytes_consumes_three_inputs { |ctx|
  let s = od_scene(ctx)?
  uu.write(s, "g", "a")?
  uu.write(s, "h", "b")?
  uu.write(s, "i", "c")?
  let r = uu.invoke(s, "od", ["-c", "-j", "3", "-An", "g", "h", "i"])?
  uu.succeeds(r)
  uu.stdout_only(r, "")
}

# origin: uutils test_od::test_skip_bytes_consumes_two_inputs
test test_uu_od_skip_bytes_consumes_two_inputs { |ctx|
  let s = od_scene(ctx)?
  uu.write(s, "g", "a")?
  uu.write(s, "h", "b")?
  let r = uu.invoke(s, "od", ["-c", "-j", "2", "-An", "g", "h"])?
  uu.succeeds(r)
  uu.stdout_only(r, "")
}

# origin: uutils test_od::test_skip_bytes_error
test test_uu_od_skip_bytes_error { |ctx|
  let s = od_scene(ctx)?
  let input = "12345"
  let r1 = uu.invoke(s, "od", ["--skip-bytes=10"], stdin: bytes.from_text(input))?
  uu.fails(r1)
}

# origin: uutils test_od::test_skip_bytes_hex
test test_uu_od_skip_bytes_hex { |ctx|
  let s = od_scene(ctx)?
  let input = "abcdefghijklmnopq"
  let r1 = uu.invoke(s, "od", ["-c", "--skip-bytes=0xB"], stdin: bytes.from_text(input))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0000013   l   m   n   o   p   q\n0000021\n")
  let r2 = uu.invoke(s, "od", ["-c", "--skip-bytes=0xE"], stdin: bytes.from_text(input))?
  uu.succeeds(r2)
  uu.stdout_only(r2, "0000016   o   p   q\n0000021\n")
}

# origin: uutils test_od::test_skip_bytes_past_end_message
test test_uu_od_skip_bytes_past_end_message { |ctx|
  let s = od_scene(ctx)?
  uu.write(s, "a", "abc")?
  uu.write(s, "b", "de")?
  let r1 = uu.invoke(s, "od", ["-j6", "a", "b"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_only(r1, "od: cannot skip past end of combined input\n")
}

# origin: uutils test_od::test_skip_bytes_past_end_no_offset
test test_uu_od_skip_bytes_past_end_no_offset { |ctx|
  let s = od_scene(ctx)?
  uu.write(s, "f", "hello")?
  let r1 = uu.invoke(s, "od", ["-j10", "f"])?
  uu.fails(r1)
  uu.no_stdout(r1)
}

# origin: uutils test_od::test_skip_bytes_past_end_of_seekable_device
test test_uu_od_skip_bytes_past_end_of_seekable_device { |ctx|
  let s = od_scene(ctx)?
  let r1 = uu.invoke(s, "od", ["-j1", "/dev/null"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0000001\n")
}

# origin: uutils test_od::test_skip_bytes_prints_after_consuming_multiple_inputs
test test_uu_od_skip_bytes_prints_after_consuming_multiple_inputs { |ctx|
  let s = od_scene(ctx)?
  uu.write(s, "g", "a")?
  uu.write(s, "h", "b")?
  uu.write(s, "i", "c")?
  uu.write(s, "j", "d")?
  let r = uu.invoke(s, "od", ["-c", "-j", "3", "-An", "g", "h", "i", "j"])?
  uu.succeeds(r)
  uu.stdout_only(r, "   d\n")
}

# origin: uutils test_od::test_skip_bytes_proc_file_without_seeking
test test_uu_od_skip_bytes_proc_file_without_seeking { |ctx|
  let s = od_scene(ctx)?
  let contents = p"/proc/version".read_bytes()?
  assert contents.len() > 0
  uu.write(s, "after", "e")?
  let r = uu.invoke(s, "od", ["-An", "-c", "-j", f"{contents.len()}", "/proc/version", "after"])?
  uu.succeeds(r)
  uu.stdout_only(r, "   e\n")
}

# origin: uutils test_od::test_stdin_offset
test test_uu_od_stdin_offset { |ctx|
  let s = od_scene(ctx)?
  let input = "abcdefghijklmnopq"
  let r1 = uu.invoke(s, "od", ["-c", "+5"], stdin: bytes.from_text(input))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0000005   f   g   h   i   j   k   l   m   n   o   p   q\n0000021\n")
}

# origin: uutils test_od::test_suppress_duplicates
test test_uu_od_suppress_duplicates { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])?
  let expected_output = "0000000 00000000000\n         0000  0000\n*\n0000020 00000000001\n         0001  0000\n0000024 00000000000\n         0000  0000\n*\n0000050 00000000000\n         0000\n0000051\n"
  let r1 = uu.invoke(s, "od", ["-w4", "-O", "-x", "--endian=little"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_tf_default_is_double
test test_uu_od_tf_default_is_double { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([0, 0, 128, 63, 0, 0, 0, 64])?
  let r1 = uu.invoke(s, "od", ["--endian=little", "-An", "-tf"], stdin: input)?
  uu.succeeds(r1)
  let r2 = uu.invoke(s, "od", ["--endian=little", "-An", "-tfD"], stdin: input)?
  uu.succeeds(r2)
  let r3 = uu.invoke(s, "od", ["--endian=little", "-An", "-tfF"], stdin: input)?
  uu.succeeds(r3)
  assert r1.stdout == r2.stdout
  assert r1.stdout != r3.stdout
}

# origin: uutils test_od::test_tf_explicit_float_still_uses_4_bytes
test test_uu_od_tf_explicit_float_still_uses_4_bytes { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([0, 0, 128, 63, 0, 0, 0, 64])?
  let r1 = uu.invoke(s, "od", ["--endian=little", "-An", "-tfF"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "               1               2\n")
}

# origin: uutils test_od::test_traditional
test test_uu_od_traditional { |ctx|
  let s = od_scene(ctx)?
  let input = "abcdefghijklmnopq"
  let r1 = uu.invoke(s, "od", ["--traditional", "-a", "-c", "-", "10", "0"], stdin: bytes.from_text(input))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0000010 (0000000)   i   j   k   l   m   n   o   p   q\n          i   j   k   l   m   n   o   p   q\n0000021 (0000011)\n")
}

# origin: uutils test_od::test_traditional_decimal_dot_offset
test test_uu_od_traditional_decimal_dot_offset { |ctx|
  let s = od_scene(ctx)?
  let r1 = uu.invoke(s, "od", ["+1."], stdin: bytes.from_text("a"))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0000001\n")
}

# origin: uutils test_od::test_traditional_dot_block_offset
test test_uu_od_traditional_dot_block_offset { |ctx|
  let s = od_scene(ctx)?
  let r1 = uu.invoke(s, "od", ["+1.b"], stdin: bytes.from_ints([97 for _ in range(512)])?)?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0001000\n")
}

# origin: uutils test_od::test_traditional_error
test test_uu_od_traditional_error { |ctx|
  let s = od_scene(ctx)?
  let r1 = uu.invoke(s, "od", ["--traditional", "0", "0", "0", "0"])?
  uu.fails(r1)
}

# origin: uutils test_od::test_traditional_offset_overflow_diagnosed
test test_uu_od_traditional_offset_overflow_diagnosed { |ctx|
  let s = od_scene(ctx)?
  let long_octal = ["7" for _ in range(255)].join("")
  let long_decimal = ["9" for _ in range(254)].join("") + "."
  let long_hex = "0x" + ["f" for _ in range(253)].join("")
  for input in [long_octal, long_decimal, long_hex] {
    let r = uu.invoke(s, "od", ["-", input], stdin: b"")?
    uu.fails_with_code(r, 1)
    uu.stderr_only(r, f"od: {input}: Numerical result out of range\n")
  }
}

# origin: uutils test_od::test_traditional_only_label
test test_uu_od_traditional_only_label { |ctx|
  let s = od_scene(ctx)?
  let input = "abcdefghijklmnopqrstuvwxyz"
  let r1 = uu.invoke(s, "od", ["-An", "--traditional", "-a", "-c", "-", "10", "0x10"], stdin: bytes.from_text(input))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "(0000010)   i   j   k   l   m   n   o   p   q   r   s   t   u   v   w   x\n          i   j   k   l   m   n   o   p   q   r   s   t   u   v   w   x\n(0000030)   y   z\n          y   z\n(0000032)\n")
}

# origin: uutils test_od::test_traditional_with_skip_bytes_non_override
test test_uu_od_traditional_with_skip_bytes_non_override { |ctx|
  let s = od_scene(ctx)?
  let input = "abcdefghijklmnop"
  let r1 = uu.invoke(s, "od", ["--traditional", "--skip-bytes=10", "-c"], stdin: bytes.from_text(input))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0000012   k   l   m   n   o   p\n0000020\n")
}

# origin: uutils test_od::test_traditional_with_skip_bytes_override
test test_uu_od_traditional_with_skip_bytes_override { |ctx|
  let s = od_scene(ctx)?
  let input = "abcdefghijklmnop"
  let r1 = uu.invoke(s, "od", ["--traditional", "--skip-bytes=10", "-c", "0"], stdin: bytes.from_text(input))?
  uu.succeeds(r1)
  uu.stdout_only(r1, "0000000   a   b   c   d   e   f   g   h   i   j   k   l   m   n   o   p\n0000020\n")
}

# origin: uutils test_od::test_two_files
test test_uu_od_two_files { |ctx|
  let s = od_scene(ctx)?
  uu.write(s, "test1", "abcdefghijklmnop")?
  uu.write(s, "test2", "qrstuvwxyz\n")?
  let r1 = uu.invoke(s, "od", ["--endian=little", "test1", "test2"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, ALPHA_OUT)
}

# origin: uutils test_od::test_very_wide_ascii_output
test test_uu_od_very_wide_ascii_output { |ctx|
  let s = od_scene(ctx)?
  uu.write(s, "data-a", "x")?
  let r1 = uu.invoke(s, "od", ["-a", "-w65537", "-An", "data-a"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "   x\n")
}

# origin: uutils test_od::test_very_wide_char_output
test test_uu_od_very_wide_char_output { |ctx|
  let s = od_scene(ctx)?
  uu.write(s, "data-c", "x")?
  let r1 = uu.invoke(s, "od", ["-c", "-w65537", "-An", "data-c"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "   x\n")
}

# origin: uutils test_od::test_very_wide_hex_byte_ascii_dump
test test_uu_od_very_wide_hex_byte_ascii_dump { |ctx|
  let s = od_scene(ctx)?
  let expected = " 41" + [" " for _ in range(100000 * 3 - 3)].join("") + "  >A<\n"
  let r = uu.invoke(s, "od", ["-An", "-w100000", "-tx1z"], stdin: b"A")?
  uu.succeeds(r)
  uu.stdout_only(r, expected)
}

# origin: uutils test_od::test_very_wide_hex_byte_output
test test_uu_od_very_wide_hex_byte_output { |ctx|
  let s = od_scene(ctx)?
  uu.write_bytes(s, "data-x", bytes.from_ints([66])?)?
  let r1 = uu.invoke(s, "od", ["-tx1", "-w65537", "-An", "data-x"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, " 42\n")
}

# origin: uutils test_od::test_width
test test_uu_od_width { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([0, 0, 0, 0, 0, 0, 0, 0])?
  let expected_output = "0000000 000000 000000\n0000004 000000 000000\n0000010\n"
  let r1 = uu.invoke(s, "od", ["-w4", "-v"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_width_without_value
test test_uu_od_width_without_value { |ctx|
  let s = od_scene(ctx)?
  let input = bytes.from_ints([0 for _ in range(40)])?
  let expected_output = "0000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000 000000\n0000040 000000 000000 000000 000000\n0000050\n"
  let r1 = uu.invoke(s, "od", ["-w"], stdin: input)?
  uu.succeeds(r1)
  uu.stdout_only(r1, expected_output)
}

# origin: uutils test_od::test_write_error_dev_full
test test_uu_od_write_error_dev_full { |ctx|
  let s = od_scene(ctx)?
  let r1 = uu.invoke(s, "od", ["-An"], stdin: bytes.from_text("abcd"), stdout: p"/dev/full")?
  uu.fails(r1)
  uu.fails_with_code(r1, 1)
  uu.stderr_contains(r1, "No space left on device")
}

# origin: uutils test_od::test_zero_width
test test_uu_od_zero_width { |ctx|
  let s = od_scene(ctx)?
  let r1 = uu.invoke(s, "od", ["-w0", "-An"])?
  uu.fails_with_code(r1, 1)
  uu.stderr_only(r1, "od: invalid -w argument '0'\n")
}
