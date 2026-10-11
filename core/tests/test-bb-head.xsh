use support.uu as uu

# origin: busybox head/head (without args)
test test_bb_head_head_without_args_d54e9d11 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "head.input", "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\n")?
  let r = uu.invoke(s, "head", ["head.input"])?
  uu.succeeds(r)
  uu.stdout_only(r, "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\n")
}

# origin: busybox head/head -n <negative number>
test test_bb_head_head_n_negative_number_5db3067b { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "head.input", "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\n")?
  let r = uu.invoke(s, "head", ["-n", "-9", "head.input"])?
  uu.succeeds(r)
  uu.stdout_only(r, "line 1\nline 2\nline 3\n")
}

# origin: busybox head/head -n <positive number>
test test_bb_head_head_n_positive_number_e0736b2e { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "head.input", "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\nline 11\nline 12\n")?
  let r = uu.invoke(s, "head", ["-n", "2", "head.input"])?
  uu.succeeds(r)
  uu.stdout_only(r, "line 1\nline 2\n")
}

