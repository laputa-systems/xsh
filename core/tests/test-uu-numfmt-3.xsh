##! Transcribed numeric padding and NUL record tests from the uutils suite.

use support.uu as uu

# origin: uutils test_numfmt::test_zero_pad_sign_order_issue_11664
test test_uu_numfmt_zero_pad_sign_order_issue_11664 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "numfmt", ["--from=none", "--format=%018.2f", "--", "-9869647"])?
  uu.succeeds(r)
  uu.stdout_is(r, "-00000009869647.00\n")
}

# origin: uutils test_numfmt::test_zero_terminated_command_line_args
test test_uu_numfmt_zero_terminated_command_line_args { |ctx|
  let s = uu.scene(ctx)?
  let long = uu.invoke(s, "numfmt", ["--zero-terminated", "--to=si", "1000"])?
  uu.succeeds(long)
  uu.stdout_is_bytes(long, b"1.0k\x00")

  let short = uu.invoke(s, "numfmt", ["-z", "--to=si", "1000"])?
  uu.succeeds(short)
  uu.stdout_is_bytes(short, b"1.0k\x00")

  let multiple = uu.invoke(s, "numfmt", ["-z", "--to=si", "1000", "2000"])?
  uu.succeeds(multiple)
  uu.stdout_is_bytes(multiple, b"1.0k\x002.0k\x00")
}

# origin: uutils test_numfmt::test_zero_terminated_embedded_newline
test test_uu_numfmt_zero_terminated_embedded_newline { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "numfmt", ["-z", "--from=si", "--field=-"], stdin: b"1K\n2K\x003K\n4K\x00")?
  uu.succeeds(r)
  uu.stdout_is_bytes(r, b"1000 2000\x003000 4000\x00")
}

# origin: uutils test_numfmt::test_zero_terminated_input
test test_uu_numfmt_zero_terminated_input { |ctx|
  let s = uu.scene(ctx)?
  for value in [
    {input: b"1000", expected: b"1.0k"},
    {input: b"1000\x00", expected: b"1.0k\x00"},
    {input: b"1000\x002000\x00", expected: b"1.0k\x002.0k\x00"},
  ] {
    let r = uu.invoke(s, "numfmt", ["-z", "--to=si"], stdin: value.input)?
    uu.succeeds(r)
    uu.stdout_is_bytes(r, value.expected)
  }
}
