use support.uu as uu

# origin: busybox paste/paste
test test_bb_paste_paste_11f0d133 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "foo1\nfoo2\nfoo3\n")?
  uu.write(s, "bar", "bar1\nbar2\nbar3\n")?
  let r = uu.invoke(s, "paste", ["foo", "bar"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "foo1\tbar1\nfoo2\tbar2\nfoo3\tbar3\n")
}

# origin: busybox paste/paste-back-cuted-lines
test test_bb_paste_paste_back_cuted_lines_d4201998 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo1", "this is the f\nthis is the s\nthis is the t\n")?
  uu.write(s, "foo2", "irst line\necond line\nhird line\n")?
  let r = uu.invoke(s, "paste", ["-d", "\\0", "foo1", "foo2"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "this is the first line\nthis is the second line\nthis is the third line\n")
}

# origin: busybox paste/paste-multi-stdin
test test_bb_paste_paste_multi_stdin_54b109f5 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "paste", ["-", "-", "-"], stdin: b"line1\nline2\nline3\nline4\nline5\nline6\n")?
  uu.succeeds(r)
  uu.stdout_only(r, "line1\tline2\tline3\nline4\tline5\tline6\n")
}

# origin: busybox paste/paste-pairs
test test_bb_paste_paste_pairs_e4894c2c { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "foo1\nbar1\nfoo2\nbar2\nfoo3\n")?
  let r = uu.invoke(s, "paste", ["-s", "-d", "\\t\\n", "foo"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "foo1\tbar1\nfoo2\tbar2\nfoo3\n")
}

# origin: busybox paste/paste-separate
test test_bb_paste_paste_separate_9a744338 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "foo1\nfoo2\nfoo3\n")?
  uu.write(s, "bar", "bar1\nbar2\nbar3\n")?
  let r = uu.invoke(s, "paste", ["-s", "foo", "bar"], stdin: b"")?
  uu.succeeds(r)
  uu.stdout_only(r, "foo1\tfoo2\tfoo3\nbar1\tbar2\tbar3\n")
}

