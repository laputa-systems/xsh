use support.uu

# origin: busybox echo/echo-does-not-print-newline
test test_bb_echo_echo_does_not_print_newline_8b73cc6c { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-n", "word"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"word")
}

# origin: busybox echo/echo-prints-argument
test test_bb_echo_echo_prints_argument_9c6eda7e { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["fubar"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"fubar\n")
}

# origin: busybox echo/echo-prints-arguments
test test_bb_echo_echo_prints_arguments_7e7b4e96 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["foo", "bar"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"foo bar\n")
}

# origin: busybox echo/echo-prints-dash
test test_bb_echo_echo_prints_dash_f6f44985 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"-\n")
}

# origin: busybox echo/echo-prints-newline
test test_bb_echo_echo_prints_newline_e156a76d { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["word"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"word\n")
}

# origin: busybox echo/echo-prints-non-opts
test test_bb_echo_echo_prints_non_opts_c4970089 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-neEZ"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"-neEZ\n")
}

# origin: busybox echo/echo-prints-slash-zero
test test_bb_echo_echo_prints_slash_zero_80b4d600 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-e", "-n", "msg\\n\\0"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"msg\n\x00")
}

# origin: busybox echo/echo-prints-slash_00041
test test_bb_echo_echo_prints_slash_00041_f96d3345 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-ne", "\\00041z"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"\x041z")
}

# origin: busybox echo/echo-prints-slash_0041
test test_bb_echo_echo_prints_slash_0041_43a63ff1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-ne", "\\0041z"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"!z")
}

# origin: busybox echo/echo-prints-slash_041
test test_bb_echo_echo_prints_slash_041_718be205 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-ne", "\\041z"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"!z")
}

# origin: busybox echo/echo-prints-slash_41
test test_bb_echo_echo_prints_slash_41_25b076e2 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "echo", ["-ne", "\\41z"])?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, b"!z")
}

