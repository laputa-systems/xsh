use support.uu as uu

# origin: busybox cat/cat -b
test test_bb_cat_cat_b_ac843bd8 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cat", ["-b"], stdin: b"line 1\n\nline 3\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "     1\tline 1\n\n     2\tline 3\n")
}

# origin: busybox cat/cat -e
test test_bb_cat_cat_e_6276c7a9 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cat", ["-e"], stdin: b"foo\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "foo$\n")
}

# origin: busybox cat/cat -n
test test_bb_cat_cat_n_17793a2a { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cat", ["-n"], stdin: b"line 1\n\nline 3\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "     1\tline 1\n     2\t\n     3\tline 3\n")
}

# origin: busybox cat/cat -v
test test_bb_cat_cat_v_03d8e57e { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cat", ["-v"], stdin: b"foo\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "foo\n")
}

# origin: busybox cat/cat-prints-a-file
test test_bb_cat_cat_prints_a_file_70aae576 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "I WANT\n")?
  let r = uu.invoke(s, "cat", ["foo"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "I WANT\n")
}

# origin: busybox cat/cat-prints-a-file-and-standard-input
test test_bb_cat_cat_prints_a_file_and_standard_input_ee00c67d { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "I WANT\n")?
  let r = uu.invoke(s, "cat", ["foo", "-"], stdin: b"SOMETHING\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "I WANT\nSOMETHING\n")
}

