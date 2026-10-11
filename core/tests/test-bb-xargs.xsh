use support.uu

# origin: busybox xargs/xargs -E ''
test test_bb_xargs_xargs_E_5ff2501b { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "xargs", ["-E", ""], stdin: b"a\n_\nb\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only(r, "a _ b\n")
}

# origin: busybox xargs/xargs -E _ stops on underscore
test test_bb_xargs_xargs_E___stops_on_underscore_13cf3a42 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "xargs", ["-E", "_"], stdin: b"a\n_\nb\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only(r, "a\n")
}

# origin: busybox xargs/xargs -e without param
test test_bb_xargs_xargs_e_without_param_6d28b950 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "xargs", ["-e"], stdin: b"a\n_\nb\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only(r, "a _ b\n")
}

# origin: busybox xargs/xargs -s7 can take one-char input
test test_bb_xargs_xargs_s7_can_take_one_char_input_fd0b48d8 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "xargs", ["-s7", "echo"], stdin: b"a\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only(r, "a\n")
}

# origin: busybox xargs/xargs -sNUM test 1
test test_bb_xargs_xargs_sNUM_test_1_1aa97aa1 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "xargs", ["-ts25", "echo"], stdin: b"1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 00\n", stdout: p"/dev/null", timeout: 5s)?
  uu.succeeds(r)
  uu.stderr_only(r, "echo 1 2 3 4 5 6 7 8 9 0\necho 1 2 3 4 5 6 7 8 9\necho 00\n")
}

# origin: busybox xargs/xargs -sNUM test 2
test test_bb_xargs_xargs_sNUM_test_2_3360dc35 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "xargs", ["-ts25", "echo", "1"], stdin: b"2 3 4 5 6 7 8 9 0 2 3 4 5 6 7 8 9 00\n", stdout: p"/dev/null", timeout: 5s)?
  uu.succeeds(r)
  uu.stderr_only(r, "echo 1 2 3 4 5 6 7 8 9 0\necho 1 2 3 4 5 6 7 8 9\necho 1 00\n")
}

# origin: busybox xargs/xargs does not stop on underscore ('new' GNU behavior)
test test_bb_xargs_xargs_does_not_stop_on_underscore_new_GNU_behavior_0df22fcd { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "xargs", [], stdin: b"a\n_\nb\n", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only(r, "a _ b\n")
}

# origin: busybox xargs/xargs-works
test test_bb_xargs_xargs_works_f2ed784c { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "first-works", "alpha\n")?
  uu.write(s, "second-works", "beta\n")?
  uu.write(s, "two words-works", "gamma\n")?
  let paths = ["first-works", "second-works", "two words-works"]
  let reference = uu.invoke(s, "md5sum", paths, timeout: 5s)?
  uu.succeeds(reference)
  let r = uu.invoke(s, "xargs", ["-0", "md5sum"], stdin: b"first-works\0second-works\0two words-works\0", timeout: 5s)?
  uu.succeeds(r)
  uu.stdout_only_bytes(r, reference.stdout)
}

